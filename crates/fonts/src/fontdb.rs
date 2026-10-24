//! Font database: bundled OFL fonts, user fonts, the installed system fonts (cataloged the first
//! time a lookup needs them, native only), outline cache.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use kurbo::BezPath;
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{GlyphId, MetadataProvider};

/// The family used when a requested family is unknown and has no substitute.
pub const FALLBACK_FAMILY: &str = "Inter";

/// The always-available last-resort face (Ubuntu Light, compiled in through
/// `epaint_default_fonts`; licence in ATTRIBUTION.md).
pub const LAST_RESORT_FAMILY: &str = "Ubuntu";

/// Compiled-in font files: the craft-fonts faces (Latin presentation fonts, then Japanese;
/// empty without `CRAFT_FONTS_DIR`), then the last-resort face.
pub fn bundled() -> Vec<&'static [u8]> {
    let mut v: Vec<&'static [u8]> = crate::CRAFT_FONTS.iter().map(|f| f.bytes).collect();
    v.push(epaint_default_fonts::UBUNTU_LIGHT);
    v
}

/// Open substitutes for families documents often ask for but we can't ship (metric-compatible
/// where one exists), tried in order after the family itself.
pub fn substitutes(family: &str) -> &'static [&'static str] {
    match family.to_ascii_lowercase().as_str() {
        "calibri" | "calibri light" => &["Carlito", "Liberation Sans", "Arial", "Inter"],
        "cambria" | "cambria math" => &["Caladea", "Liberation Serif", "Times New Roman", "Merriweather"],
        "arial" | "helvetica" | "helvetica neue" | "arial nova" => &["Liberation Sans", "Arial", "Helvetica Neue", "Helvetica", "Inter"],
        "times new roman" | "times" => &["Liberation Serif", "Times New Roman", "Times", "Merriweather"],
        "courier new" | "courier" | "consolas" | "menlo" => &["Liberation Mono", "Courier New", "Menlo", "Courier"],
        "aptos" | "aptos display" | "aptos narrow" | "segoe ui" | "segoe ui light" | "segoe ui semibold" | "tahoma" | "verdana" => {
            &["Inter", "Helvetica Neue", "Arial", "Liberation Sans"]
        }
        "georgia" | "garamond" | "book antiqua" | "palatino" | "palatino linotype" | "constantia" => &["Merriweather", "Liberation Serif", "Georgia"],
        "century gothic" | "avenir" | "avenir next" | "futura" | "gill sans" | "gill sans mt" | "trebuchet ms" | "corbel" | "candara" => {
            &["Montserrat", "Poppins", "Avenir Next", "Inter"]
        }
        "franklin gothic" | "franklin gothic book" | "franklin gothic medium" | "univers" => &["Open Sans", "Inter"],
        "open sans" | "lato" | "roboto" | "montserrat" | "poppins" | "nunito sans" => &["Inter"],
        "playfair display" | "merriweather" | "source serif 4" | "source serif pro" => &["Merriweather", "Liberation Serif", "Georgia"],
        "source sans 3" | "source sans pro" => &["Open Sans", "Inter"],
        "impact" | "arial black" => &["Montserrat", "Inter"],
        _ => &[],
    }
}

/// Families whose open substitutes ([`substitutes`]) keep their metrics, so text drawn with
/// them breaks into the same lines.
fn has_metric_twin(family: &str) -> bool {
    matches!(
        family.to_ascii_lowercase().as_str(),
        "calibri" | "calibri light" | "cambria" | "cambria math" | "arial" | "helvetica" | "times new roman" | "times" | "courier new" | "courier"
    )
}

/// The installed family PowerPoint draws a missing `family` with instead of Calibri: Liberation
/// Sans and Serif as Arial and Times New Roman (observed in PowerPoint; Liberation Mono, Carlito,
/// Caladea, Arimo and Tinos are not mapped and draw in Calibri), and Windows' standard font
/// substitutes (Arial CE, Times New Roman CYR…).
fn office_twin(family: &str) -> Option<&'static str> {
    let f = family.to_ascii_lowercase();
    Some(match f.as_str() {
        "liberation sans" | "helvetica" | "helv" => "Arial",
        "liberation serif" | "times" | "tms rmn" => "Times New Roman",
        _ => {
            let base = f
                .strip_suffix(" baltic")
                .or(f.strip_suffix(" ce"))
                .or(f.strip_suffix(" cyr"))
                .or(f.strip_suffix(" greek"))
                .or(f.strip_suffix(" tur"))?;
            match base {
                "arial" => "Arial",
                "courier new" => "Courier New",
                "times new roman" => "Times New Roman",
                _ => return None,
            }
        }
    })
}

/// Office's downloadable (cloud) families: PowerPoint fetches them when a document uses them, so
/// they draw as themselves there, not as Calibri. Without them DeckCraft keeps their closest open
/// substitute ([`substitutes`]; Aptos as Inter).
fn is_office_cloud_font(family: &str) -> bool {
    let f = family.to_ascii_lowercase();
    f.starts_with("aptos") || ["bierstadt", "grandview", "seaford", "skeena", "tenorite"].iter().any(|c| f.starts_with(c))
}

/// The family PowerPoint draws a document's Latin text in when the font it names isn't installed,
/// whatever the theme, `panose` or `pitchFamily` say.
pub const MISSING_FONT_FAMILY: &str = "Calibri";

/// A family name in another language, from a font's `name` table: ヒラギノ角ゴ ProN W3 for
/// Hiragino Kaku Gothic ProN W3, メイリオ for Meiryo. Japanese PowerPoint writes fonts by these
/// names, and finds the installed font by either name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalName {
    /// The record's BCP 47 language, as the font gives it (`ja-JP`, `ja`, `zh-Hans`, `ko-KR`).
    pub lang: String,
    /// The name in that language.
    pub name: String,
    /// The English family it stands for: the typographic family for a localized typographic name
    /// (name ID 16), the Windows family for a localized Windows name (ID 1).
    pub english: String,
}

/// A font name folded for matching, as PowerPoint matches a typeface to a font: full-width ASCII
/// as ASCII (ＭＳ Ｐゴシック is MS Pゴシック), the minus sign as a hyphen (Osaka−等幅), case
/// ignored, and any run of spaces (the ideographic space too) as one space, none at the ends.
/// Long names are cut at 256 characters. Lookups that find nothing by this key try it without
/// spaces too ([`fold_compact`]).
pub fn fold_font_name(name: &str) -> String {
    const MAX_CHARS: usize = 256;
    let mut out = String::with_capacity(name.len().min(MAX_CHARS * 4));
    for c in name.chars().take(MAX_CHARS) {
        let c = match u32::from(c) {
            cp @ 0xFF01..=0xFF5E => char::from_u32(cp - 0xFF01 + 0x21).unwrap_or(c),
            0x2212 => '-',
            _ => c,
        };
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.extend(c.to_lowercase());
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// [`fold_font_name`] without spaces: "MSPGothic" and "MS PGothic" alike. Only a last resort for
/// lookups, after the spaced key: Office's Yu Gothic and macOS's YuGothic are different fonts.
pub fn fold_compact(name: &str) -> String {
    let mut k = fold_font_name(name);
    k.retain(|c| c != ' ');
    k
}

/// Does a `name` table language tag (`ja-JP`, `zh-Hans`, `zh-TW`) suit the interface language
/// `ui` (`ja`, `zh-Hans`, `zh-Hant`, `ko`)? Chinese matches by script: Simplified for mainland
/// China and Singapore (and plain `zh`), Traditional for Taiwan, Hong Kong and Macao.
pub fn name_language_matches(ui: &str, tag: &str) -> bool {
    let parts = |s: &str| s.split(['-', '_']).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    let (u, t) = (parts(ui), parts(tag));
    let (Some(up), Some(tp)) = (u.first(), t.first()) else { return false };
    if up.is_empty() || up != tp {
        return false;
    }
    if up != "zh" {
        return true;
    }
    let traditional = |p: &[String]| p.iter().any(|x| matches!(x.as_str(), "hant" | "tw" | "hk" | "mo"));
    let simplified = |p: &[String]| p.iter().any(|x| matches!(x.as_str(), "hans" | "cn" | "sg"));
    if traditional(&u) { traditional(&t) } else { simplified(&t) || !traditional(&t) }
}

#[derive(Clone)]
enum FontBytes {
    Static(&'static [u8]),
    Owned(Arc<Vec<u8>>),
}

/// One loaded font face.
pub struct FontFace {
    id: u32,
    /// Typographic family name (e.g. "Source Sans 3").
    pub family: String,
    /// Typographic style name (e.g. "Semibold", "Italic").
    pub style: String,
    /// The Windows (GDI) family and style names (`name` IDs 1 and 2) when the family differs from
    /// the typographic one: "Segoe UI Semibold" / "Regular" for Segoe UI Semibold. Documents name
    /// fonts this way.
    pub legacy: Option<(String, String)>,
    /// The family's names in other languages (ヒラギノ角ゴシック for Hiragino Sans), from the
    /// `name` table's Windows and Mac records. Lookups by family name accept them.
    pub local_names: Vec<LocalName>,
    /// Every name above folded ([`fold_font_name`]), for lookups, with the face's full names
    /// (name ID 4, every language) and PostScript name (ID 6), each of which selects this face
    /// (Osaka−等幅 is Osaka's Regular-Mono).
    keys: NameKeys,
    /// The same without spaces ([`fold_compact`]), for lookups nothing else answers.
    loose_keys: NameKeys,
    /// OS/2 `fsSelection` marks the face bold: what the system pairs with a regular weight as its
    /// bold (Hiragino's W6 for W3).
    bold_linked: bool,
    /// A serif design (OS/2 family class or PANOSE, or a 明朝/Mincho name): East Asian text the
    /// face lacks falls back to a Mincho face.
    serif: bool,
    /// The East Asian tradition the face is a font for, by its family name.
    tradition: Option<crate::CjkLocale>,
    /// usWeightClass-style weight (400 = regular).
    pub weight: f32,
    pub italic: bool,
    bytes: FontBytes,
    index: u32,
    pub upem: f64,
    /// Ascender in font units (positive = up).
    pub ascent: f64,
    /// Descender in font units (positive = down).
    pub descent: f64,
    /// Cap height and x height in font units (estimated from the ascent when the font has no OS/2
    /// values).
    pub cap_height: f64,
    pub x_height: f64,
    /// Where the baseline sits in a line, as a share of the line's height from its top: the
    /// Windows ascent over the Windows ascent plus descent (OS/2 `usWinAscent`/`usWinDescent`),
    /// the split PowerPoint uses (computed by `line_split`).
    pub line_split: f64,
    /// Underline position (top edge, below the baseline) and thickness in font units (`post`
    /// table; 0.1 and 0.05 em when the font has none). PowerPoint draws single underlines there.
    pub underline: (f64, f64),
    /// Strikethrough position (top edge, above the baseline) and thickness in font units (OS/2;
    /// 0.26 and 0.05 em when the font has none). PowerPoint draws single strikethroughs there.
    pub strikeout: (f64, f64),
    pub shaper: harfrust::ShaperData,
    /// Variable fonts: the named instance's axis settings (user units; empty = default instance),
    /// the normalised location and the shaper's view of it.
    pub coords: Vec<([u8; 4], f32)>,
    location: Location,
    pub instance: Option<harfrust::ShaperInstance>,
    /// Basic Multilingual Plane coverage bitset, built on first use.
    bmp: std::sync::OnceLock<Box<[u64]>>,
    /// Colour glyph tables present (`is_color`), looked up on first use.
    color: std::sync::OnceLock<bool>,
}

/// A cheap `Copy` handle to a face. Faces are never unloaded, so the handle lives for the rest of
/// the process; unlike cloning an `Arc`, copying it touches no shared reference count (which made
/// parallel composition scale negatively).
#[derive(Clone, Copy)]
pub struct FaceRef(&'static FontFace);

impl FaceRef {
    /// The handle for a loaded face (memoised per face).
    pub fn of(face: &Arc<FontFace>) -> FaceRef {
        static LEAKED: RwLock<Vec<Option<&FontFace>>> = RwLock::new(Vec::new());
        let id = face.id as usize;
        if let Some(Some(f)) = LEAKED.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(id) {
            return FaceRef(f);
        }
        let mut w = LEAKED.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if w.len() <= id {
            w.resize(id + 1, None);
        }
        let f: &'static FontFace = w[id].unwrap_or_else(|| {
            let keep: &'static Arc<FontFace> = Box::leak(Box::new(face.clone()));
            keep
        });
        w[id] = Some(f);
        FaceRef(f)
    }
    pub fn get(self) -> &'static FontFace {
        self.0
    }
}

impl std::ops::Deref for FaceRef {
    type Target = FontFace;
    fn deref(&self) -> &FontFace {
        self.0
    }
}

impl std::fmt::Debug for FaceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for FaceRef {
    fn eq(&self, o: &FaceRef) -> bool {
        self.0.id == o.0.id
    }
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FontFace({} {})", self.family, self.style)
    }
}

impl FontFace {
    pub fn data(&self) -> &[u8] {
        match &self.bytes {
            FontBytes::Static(b) => b,
            FontBytes::Owned(v) => v.as_slice(),
        }
    }
    pub fn skrifa(&self) -> Option<skrifa::FontRef<'_>> {
        skrifa::FontRef::from_index(self.data(), self.index).ok()
    }
    /// Does the face draw colour glyphs (a `COLR`, `CBDT`, `sbix` or `SVG ` table, as colour emoji
    /// fonts have)?
    pub fn is_color(&self) -> bool {
        *self.color.get_or_init(|| has_color_tables(self.data(), self.index))
    }
    pub fn hb(&self) -> Option<harfrust::FontRef<'_>> {
        harfrust::FontRef::from_index(self.data(), self.index).ok()
    }
    /// Where in the design space glyphs, metrics and outlines come from.
    pub fn location(&self) -> LocationRef<'_> {
        (&self.location).into()
    }
    /// Is this a named instance of a variable font?
    pub fn is_variable(&self) -> bool {
        !self.coords.is_empty()
    }
    /// Face index within the font file (collections); 0 for plain fonts.
    pub fn index(&self) -> u32 {
        self.index
    }
    /// Unique id of this face within the process.
    pub fn id(&self) -> u32 {
        self.id
    }
    /// Does the face map `c` to a glyph?
    pub fn covers(&self, c: char) -> bool {
        let cp = c as u32;
        if cp < 0x1_0000 {
            let bits = self.bmp.get_or_init(|| {
                let mut b = vec![0u64; 1024].into_boxed_slice();
                if let Some(f) = self.skrifa() {
                    for (cp, _) in f.charmap().mappings() {
                        if cp < 0x1_0000 {
                            b[(cp / 64) as usize] |= 1 << (cp % 64);
                        }
                    }
                }
                b
            });
            return bits[(cp / 64) as usize] & (1 << (cp % 64)) != 0;
        }
        self.skrifa().is_some_and(|f| f.charmap().map(c).is_some())
    }
    /// Units per em.
    pub fn units_per_em(&self) -> f64 {
        self.upem
    }
    /// (ascent, descent) in font units, both positive.
    pub fn vertical_metrics(&self) -> (f64, f64) {
        (self.ascent, self.descent)
    }
    /// Every mapped character and its glyph id, sorted by code point (the Glyphs panel).
    pub fn chars(&self) -> Vec<(char, u32)> {
        let Some(f) = self.skrifa() else { return vec![] };
        let mut v: Vec<(char, u32)> = f.charmap().mappings().filter_map(|(cp, g)| char::from_u32(cp).map(|c| (c, g.to_u32()))).collect();
        v.sort_unstable_by_key(|x| x.0);
        v.dedup_by_key(|x| x.0);
        v
    }
    /// Advance width of glyph `gid` in font units.
    pub fn advance(&self, gid: u32) -> f64 {
        self.skrifa()
            .and_then(|f| f.glyph_metrics(Size::unscaled(), self.location()).advance_width(GlyphId::new(gid)))
            .map(|a| a as f64)
            .unwrap_or(self.upem * 0.5)
    }
    /// Glyph id for `c` (0 = .notdef).
    pub fn glyph_for(&self, c: char) -> u32 {
        self.skrifa().and_then(|f| f.charmap().map(c)).map(|g| g.to_u32()).unwrap_or(0)
    }
    /// Is `family` one of this face's names — typographic, Windows, localized (メイリオ), full or
    /// PostScript — compared as PowerPoint compares typefaces ([`fold_font_name`])?
    pub fn answers_to(&self, family: &str) -> bool {
        self.name_hit(&fold_font_name(family)).is_some() || self.loose_hit(&fold_compact(family)).is_some()
    }
    /// A serif design: a Mincho stands in for the East Asian characters it lacks.
    pub fn is_serif(&self) -> bool {
        self.serif
    }
    /// What the folded name `key` ([`fold_font_name`]) names in this face, if anything.
    fn name_hit(&self, key: &str) -> Option<NameHit> {
        self.hit_in(&self.keys, key)
    }
    /// [`FontFace::name_hit`] for a key without spaces ([`fold_compact`]).
    fn loose_hit(&self, key: &str) -> Option<NameHit> {
        self.hit_in(&self.loose_keys, key)
    }
    fn hit_in(&self, keys: &NameKeys, key: &str) -> Option<NameHit> {
        if key.is_empty() {
            return None;
        }
        if keys.family == key {
            return Some(NameHit { family: self.family.clone(), rank: 0, exact: false });
        }
        if let (Some(k), Some((lf, _))) = (&keys.legacy, &self.legacy)
            && k == key
        {
            return Some(NameHit { family: lf.clone(), rank: 0, exact: false });
        }
        if let Some(i) = keys.local.iter().position(|k| k == key) {
            return self.local_names.get(i).map(|n| NameHit { family: n.english.clone(), rank: 1, exact: false });
        }
        if keys.exact.iter().any(|k| k == key) {
            // A face with a Windows family of its own is looked up by it (so bold still finds
            // its partner); the others are the face itself.
            return Some(match &self.legacy {
                Some((lf, _)) => NameHit { family: lf.clone(), rank: 2, exact: false },
                None => NameHit { family: self.family.clone(), rank: 2, exact: true },
            });
        }
        None
    }
}

/// What a font name refers to.
#[derive(Clone, Debug, PartialEq)]
struct NameHit {
    /// The English family to look up: a typographic or a Windows family name.
    family: String,
    /// 0: an English family name; 1: a family name in another language; 2: a full or PostScript
    /// name.
    rank: u8,
    /// A full or PostScript name of a face without a Windows family of its own: that one face.
    exact: bool,
}

/// Folded names of a face ([`fold_font_name`]).
#[derive(Clone, Debug, Default)]
struct NameKeys {
    family: String,
    legacy: Option<String>,
    /// One per [`FontFace::local_names`] entry.
    local: Vec<String>,
    exact: Vec<String>,
}

impl NameKeys {
    fn new(family: &str, legacy: Option<&str>, local: &[LocalName], exact: &[String]) -> Self {
        NameKeys {
            family: fold_font_name(family),
            legacy: legacy.map(fold_font_name),
            local: local.iter().map(|n| fold_font_name(&n.name)).collect(),
            exact: exact.iter().map(|n| fold_font_name(n)).collect(),
        }
    }

    /// The keys without spaces ([`fold_compact`]).
    fn compact(&self) -> Self {
        let c = |k: &String| k.chars().filter(|c| *c != ' ').collect::<String>();
        NameKeys {
            family: c(&self.family),
            legacy: self.legacy.as_ref().map(c),
            local: self.local.iter().map(c).collect(),
            exact: self.exact.iter().map(c).collect(),
        }
    }
}

/// What a face is called besides its English family and style.
#[derive(Clone, Debug, Default)]
struct OtherNames {
    local: Vec<LocalName>,
    exact: Vec<String>,
}

/// One string of a `name` table that names a font (IDs 1, 4, 6 and 16).
struct NameRecordText {
    id: u16,
    lang: Option<String>,
    /// A Macintosh-platform record.
    mac: bool,
    text: String,
}

/// The family, full and PostScript names of `f` in every language. Mac OS's Japanese, Chinese
/// and Korean encodings are decoded too (skrifa reads only Unicode and Mac Roman records): fonts
/// made for the Mac, such as Osaka, carry their Japanese names only there. Bounded: a damaged
/// font can list any number of records.
fn name_records(f: &skrifa::FontRef<'_>) -> Vec<NameRecordText> {
    use skrifa::raw::TableProvider;
    const MAX_RECORDS: usize = 2048;
    const MAX_CHARS: usize = 128;
    let Ok(table) = f.name() else { return Vec::new() };
    let mut out = Vec::new();
    for rec in table.name_record().iter().take(MAX_RECORDS) {
        let id = rec.name_id().to_u16();
        if !matches!(id, 1 | 4 | 6 | 16) {
            continue;
        }
        let s = skrifa::string::LocalizedString::new(&table, rec);
        let text: String = match (rec.platform_id(), rec.encoding_id()) {
            (1, enc) if enc != 0 => mac_cjk_text(&table, rec, enc).unwrap_or_default(),
            _ => s.chars().take(MAX_CHARS).collect(),
        };
        let text = text.trim();
        if text.is_empty() || text.contains('\u{FFFD}') || text.chars().any(char::is_control) {
            continue;
        }
        out.push(NameRecordText { id, lang: s.language().map(str::to_string), mac: rec.platform_id() == 1, text: text.to_string() });
    }
    out
}

/// A Macintosh `name` record in Japanese (Shift JIS), Traditional Chinese (Big5), Korean (EUC-KR)
/// or Simplified Chinese (GB) encoding, decoded; `None` for other encodings and damaged bytes.
#[cfg(not(target_arch = "wasm32"))]
fn mac_cjk_text(table: &skrifa::raw::tables::name::Name<'_>, rec: &skrifa::raw::tables::name::NameRecord, encoding: u16) -> Option<String> {
    let enc = match encoding {
        1 => encoding_rs::SHIFT_JIS,
        2 => encoding_rs::BIG5,
        3 => encoding_rs::EUC_KR,
        25 => encoding_rs::GBK,
        _ => return None,
    };
    let start = usize::try_from(rec.string_offset().to_u32()).ok()?;
    let end = start.checked_add(usize::from(rec.length()))?;
    let bytes = table.string_data().as_bytes().get(start..end)?;
    let (text, errors) = enc.decode_without_bom_handling(bytes);
    (!errors).then(|| text.chars().take(128).collect())
}

/// The web build reads only Unicode and Mac Roman records (it has no installed Mac fonts).
#[cfg(target_arch = "wasm32")]
fn mac_cjk_text(_: &skrifa::raw::tables::name::Name<'_>, _: &skrifa::raw::tables::name::NameRecord, _: u16) -> Option<String> {
    None
}

/// The family's names in other languages and the face's full and PostScript names, from its
/// `name` records. A localized typographic name (ID 16) stands for `family`; a localized Windows
/// name (ID 1) for `legacy`, the Windows family (or for `family` when the font has no typographic
/// names, the Windows family being the family then, and for Mac records, whose family is the
/// typographic one). `whole_face` is false for a variable font's named instance, which keeps the
/// family-wide names only. Bounded.
fn other_names(records: &[NameRecordText], family: &str, legacy: Option<&str>, whole_face: bool) -> OtherNames {
    const MAX_LOCAL: usize = 64;
    const MAX_EXACT: usize = 32;
    let english_lang = |l: &Option<String>| l.as_deref().is_none_or(|l| l == "en" || l.starts_with("en-"));
    let typographic = records.iter().any(|r| r.id == 16);
    let fam_key = fold_font_name(family);
    let legacy_key = legacy.map(fold_font_name);
    let english_key = |k: &str| k == fam_key || legacy_key.as_deref() == Some(k);
    let mut out = OtherNames::default();
    for r in records {
        let key = fold_font_name(&r.text);
        match r.id {
            1 | 16 => {
                if english_lang(&r.lang) || out.local.len() >= MAX_LOCAL {
                    continue;
                }
                let english = if r.id == 16 || r.mac || !typographic { family } else { legacy.unwrap_or(family) };
                if (!whole_face && english != family) || english_key(&key) {
                    continue;
                }
                let lang = r.lang.clone().unwrap_or_default();
                if out.local.iter().any(|n| n.lang == lang && n.name == r.text) {
                    continue;
                }
                out.local.push(LocalName { lang, name: r.text.clone(), english: english.to_string() });
            }
            _ => {
                if !whole_face || english_key(&key) || out.exact.len() >= MAX_EXACT || out.exact.iter().any(|e| fold_font_name(e) == key) {
                    continue;
                }
                out.exact.push(r.text.clone());
            }
        }
    }
    out
}

/// Does the face's design look serif? OS/2 `sFamilyClass` (classes 1–7 are serifs, 8 sans
/// serif), else PANOSE's serif style (Latin text faces), else its family name (Mincho, 明朝,
/// Serif, Song, Ming, Batang).
fn serif_design(f: &skrifa::FontRef<'_>, family: &str) -> bool {
    use skrifa::raw::TableProvider;
    if crate::substitute::is_serif_name(family) {
        return true;
    }
    let Ok(os2) = f.os2() else { return false };
    match os2.s_family_class() >> 8 {
        1..=7 => return true,
        8 => return false,
        _ => {}
    }
    let p = os2.panose_10();
    matches!((p.first(), p.get(1)), (Some(2), Some(2..=10)))
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct CatalogEntry {
    /// The typographic family, or (a second entry for the same face) its Windows family.
    family: String,
    style: String,
    /// `family`, folded.
    key: String,
    /// On the typographic entry only: the family's names in other languages, and the face's full
    /// and PostScript names with what they select.
    local: Vec<LocalName>,
    exact: Vec<String>,
    exact_hit: Option<NameHit>,
    path: std::path::PathBuf,
}

/// The installed fonts found by the system font scan, and every name they answer to.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Catalog {
    entries: Vec<CatalogEntry>,
    /// Folded names ([`fold_font_name`]) and what each names: the best rank, then the first face
    /// found.
    names: HashMap<String, NameHit>,
    /// The same keyed without spaces ([`fold_compact`]), for lookups nothing else answers.
    loose: HashMap<String, NameHit>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Catalog {
    fn new(entries: Vec<CatalogEntry>) -> Self {
        let mut names: HashMap<String, NameHit> = HashMap::new();
        let mut loose: HashMap<String, NameHit> = HashMap::new();
        let mut put = |key: String, hit: NameHit| {
            if key.is_empty() {
                return;
            }
            let compact: String = key.chars().filter(|c| *c != ' ').collect();
            for (map, key) in [(&mut names, key), (&mut loose, compact)] {
                match map.get(&key) {
                    Some(old) if old.rank <= hit.rank => {}
                    _ => {
                        map.insert(key, hit.clone());
                    }
                }
            }
        };
        for e in &entries {
            put(e.key.clone(), NameHit { family: e.family.clone(), rank: 0, exact: false });
            for n in &e.local {
                put(fold_font_name(&n.name), NameHit { family: n.english.clone(), rank: 1, exact: false });
            }
            if let Some(hit) = &e.exact_hit {
                for x in &e.exact {
                    put(fold_font_name(x), hit.clone());
                }
            }
        }
        Catalog { entries, names, loose }
    }
}

/// Process-wide font database.
pub struct FontDb {
    faces: RwLock<Vec<Arc<FontFace>>>,
    outlines: Mutex<HashMap<(u32, u32), Arc<BezPath>>>,
    /// East Asian fallback faces looked up so far ([`FontDb::cascade_for`]), per (tradition,
    /// Mincho, style): one entry per family of the tradition's fallback list, in order, `None`
    /// where the family isn't there.
    pub(crate) ea_faces: RwLock<HashMap<(crate::CjkLocale, bool, &'static str), Vec<Option<Arc<FontFace>>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    catalog: RwLock<Catalog>,
    /// The folders the system font scan reads (the platform's font folders for [`FontDb::global`]).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    font_dirs: Vec<std::path::PathBuf>,
    /// Set once the font folders have been scanned. Lookups by family name wait for the first
    /// scan, so what they find doesn't depend on what ran before them.
    #[cfg(not(target_arch = "wasm32"))]
    cataloged: std::sync::OnceLock<()>,
    /// Families (lower case) whose cataloged files have all been loaded: a family's files load
    /// together, so a lookup never settles for the one face another name loaded ("Segoe UI
    /// Semibold" loads a face of the family "Segoe UI").
    #[cfg(not(target_arch = "wasm32"))]
    loaded_families: Mutex<std::collections::HashSet<String>>,
    /// System fallback state: enabled, and characters no system font covers.
    #[cfg(not(target_arch = "wasm32"))]
    sys: Mutex<SysFallback>,
    /// Held while installed fonts are loaded, so threads that need the same family wait for one
    /// load instead of racing it: what a lookup finds doesn't depend on which thread got there
    /// first.
    #[cfg(not(target_arch = "wasm32"))]
    loading: Mutex<()>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct SysFallback {
    enabled: bool,
    misses: std::collections::HashSet<char>,
}

/// Does face `index` of `data` carry colour glyph tables?
fn has_color_tables(data: &[u8], index: u32) -> bool {
    use skrifa::raw::types::Tag;
    let Ok(f) = skrifa::FontRef::from_index(data, index) else { return false };
    [b"COLR", b"CBDT", b"sbix", b"SVG "].iter().any(|t| f.table_data(Tag::new(t)).is_some())
}

/// Families tried (when installed) for characters the loaded fonts lack: CJK, symbols, emoji.
#[cfg(not(target_arch = "wasm32"))]
const SYSTEM_FALLBACKS: &[&str] = &[
    "Helvetica Neue",
    "Arial",
    "Segoe UI",
    "Noto Sans",
    "DejaVu Sans",
    "PingFang SC",
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Apple SD Gothic Neo",
    "Heiti SC",
    "STHeiti",
    "Microsoft YaHei",
    "Yu Gothic",
    "Malgun Gothic",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Arial Unicode MS",
    "Apple Symbols",
    "Segoe UI Symbol",
    "Noto Sans Symbols",
    "Noto Sans Symbols2",
    "Noto Emoji",
    "Segoe UI Emoji",
    "Apple Color Emoji",
    "Noto Color Emoji",
];

/// The East Asian families of [`SYSTEM_FALLBACKS`] and their tradition.
#[cfg(not(target_arch = "wasm32"))]
fn system_fallback_tradition(family: &str) -> Option<crate::CjkLocale> {
    use crate::CjkLocale::{Japanese, Korean, SimplifiedChinese};
    Some(match family {
        "PingFang SC" | "Heiti SC" | "STHeiti" | "Microsoft YaHei" | "Noto Sans CJK SC" => SimplifiedChinese,
        "Hiragino Sans" | "Hiragino Kaku Gothic ProN" | "Yu Gothic" | "Noto Sans CJK JP" => Japanese,
        "Apple SD Gothic Neo" | "Malgun Gothic" => Korean,
        _ => return None,
    })
}

/// [`SYSTEM_FALLBACKS`] for text of tradition `cjk`: its East Asian families moved ahead of the
/// other traditions' (Japanese text tries Hiragino and Yu Gothic before PingFang SC and Microsoft
/// YaHei). Without a tradition the list keeps its order.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn system_fallbacks(cjk: Option<crate::CjkLocale>) -> Vec<&'static str> {
    let first = SYSTEM_FALLBACKS.iter().position(|f| system_fallback_tradition(f).is_some());
    let (Some(l), Some((head, tail))) = (cjk, first.and_then(|i| SYSTEM_FALLBACKS.split_at_checked(i))) else { return SYSTEM_FALLBACKS.to_vec() };
    let mut v: Vec<&'static str> = head.to_vec();
    v.extend(tail.iter().filter(|f| system_fallback_tradition(f) == Some(l)));
    v.extend(tail.iter().filter(|f| system_fallback_tradition(f) != Some(l)));
    v
}

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
const OUTLINE_CACHE_MAX: usize = 50_000;

fn name(font: &skrifa::FontRef<'_>, ids: &[StringId]) -> Option<String> {
    ids.iter().find_map(|id| font.localized_strings(*id).english_or_first().map(|s| s.to_string()).filter(|s| !s.is_empty()))
}

/// A face's family and style names (the default instance's style for a variable font).
fn face_names(f: &skrifa::FontRef<'_>) -> Option<(String, String)> {
    let family = name(f, &[StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME])?;
    let style = name(f, &[StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME]).unwrap_or_else(|| "Regular".into());
    Some((family, style))
}

/// A face's Windows (GDI) family and style (`name` IDs 1 and 2) when that family differs from the
/// typographic `family`.
fn legacy_names(f: &skrifa::FontRef<'_>, family: &str) -> Option<(String, String)> {
    let legacy = name(f, &[StringId::FAMILY_NAME])?;
    if legacy.eq_ignore_ascii_case(family) {
        return None;
    }
    Some((legacy, name(f, &[StringId::SUBFAMILY_NAME]).unwrap_or_else(|| "Regular".into())))
}

/// The tables skrifa reads glyph outlines from: `glyf` (TrueType), `CFF `, `CFF2` and `VARC`. For a
/// face with none of them (bitmaps only, or outlines in Apple's `hvgl` table), [`FontDb::outline`]
/// returns an empty path for every glyph.
#[cfg(not(target_arch = "wasm32"))]
const OUTLINE_TABLES: [&[u8; 4]; 4] = [b"glyf", b"CFF ", b"CFF2", b"VARC"];

/// The catalog entries of every face in the font file at `path` (its typographic family and
/// style, and its Windows family when that differs, with the names in other languages, full and
/// PostScript names), reading only its table directories and `name` tables: a scan opens
/// hundreds of font files, many of them megabytes long. A variable font is cataloged under its
/// default style; its named instances appear once it loads. Faces with none of the
/// [`OUTLINE_TABLES`] are left out.
#[cfg(not(target_arch = "wasm32"))]
fn file_face_names(path: &std::path::Path) -> Vec<CatalogEntry> {
    use std::io::{Read, Seek, SeekFrom};
    /// Caps on what a (possibly damaged) file can make the scan read.
    const MAX_FACES: u32 = 256;
    const MAX_NAME_TABLE: u32 = 1 << 20;
    let Ok(mut file) = std::fs::File::open(path) else { return vec![] };
    let mut read_at = |offset: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let be32 = |b: &[u8], at: usize| b.get(at..at + 4).and_then(|s| s.try_into().ok()).map(u32::from_be_bytes);
    let Some(head) = read_at(0, 12) else { return vec![] };
    // A collection lists where each face's table directory starts.
    let starts: Vec<u32> = if head.starts_with(b"ttcf") {
        let n = be32(&head, 8).unwrap_or(0).min(MAX_FACES) as usize;
        read_at(12, n * 4).map(|b| b.as_chunks::<4>().0.iter().map(|c| u32::from_be_bytes(*c)).collect()).unwrap_or_default()
    } else {
        vec![0]
    };
    starts
        .into_iter()
        .filter_map(|start| {
            let dir = read_at(start.into(), 12)?;
            let tables = u16::from_be_bytes(dir.get(4..6)?.try_into().ok()?) as usize;
            let records = read_at(u64::from(start) + 12, tables * 16)?;
            if !records.as_chunks::<16>().0.iter().any(|r| OUTLINE_TABLES.iter().any(|t| r.starts_with(*t))) {
                return None;
            }
            let rec = records.as_chunks::<16>().0.iter().find(|r| r.starts_with(b"name"))?;
            let (offset, len) = (be32(rec, 8)?, be32(rec, 12)?);
            if len > MAX_NAME_TABLE {
                return None;
            }
            let table = read_at(offset.into(), len as usize)?;
            // A one-table font holding just the `name` table, to read it as the font itself would.
            let mut font = Vec::with_capacity(28 + table.len());
            font.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            font.extend_from_slice(&[0, 1, 0, 16, 0, 0, 0, 0]);
            font.extend_from_slice(b"name");
            for v in [0, 28, len] {
                font.extend_from_slice(&u32::to_be_bytes(v));
            }
            font.extend_from_slice(&table);
            let f = skrifa::FontRef::new(&font).ok()?;
            let (family, style) = face_names(&f)?;
            // Cataloged under its Windows family too, so "Segoe UI Semibold" finds the file.
            let legacy = legacy_names(&f, &family);
            let names = other_names(&name_records(&f), &family, legacy.as_ref().map(|l| l.0.as_str()), true);
            let exact_hit = match &legacy {
                Some((lf, _)) => NameHit { family: lf.clone(), rank: 2, exact: false },
                None => NameHit { family: family.clone(), rank: 2, exact: true },
            };
            let entry = |family: String, style: String, names: OtherNames, exact_hit: Option<NameHit>| CatalogEntry {
                key: fold_font_name(&family),
                family,
                style,
                local: names.local,
                exact: names.exact,
                exact_hit,
                path: path.to_path_buf(),
            };
            let main = entry(family, style, names, Some(exact_hit));
            Some(std::iter::once(main).chain(legacy.map(|(lf, ls)| entry(lf, ls, OtherNames::default(), None))))
        })
        .flatten()
        .collect()
}

/// Folder depth below a font folder that the system scan descends.
#[cfg(not(target_arch = "wasm32"))]
const MAX_SCAN_DEPTH: usize = 12;
/// Directory entries the system scan looks at in all.
#[cfg(not(target_arch = "wasm32"))]
const MAX_SCAN_ENTRIES: usize = 200_000;
/// The largest font file read (the biggest CJK collections are a few hundred MB).
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_FONT_FILE: u64 = 512 << 20;

/// A regular file of a size a font can have.
#[cfg(not(target_arch = "wasm32"))]
fn font_file_ok(meta: &std::fs::Metadata) -> bool {
    meta.is_file() && meta.len() > 12 && meta.len() <= MAX_FONT_FILE
}

/// The platform's font folders (the system's and the user's), scanned by [`FontDb::global`].
/// Empty on wasm.
pub fn system_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(target_arch = "wasm32") {
        return dirs;
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(Into::into));
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
            // Fonts Microsoft Office downloads on demand (Aptos…), used where Office put them.
            dirs.push(h.join("Library/Group Containers/UBF8T346G9.Office/FontCache/4/CloudFonts"));
        }
        // Fonts Office installs inside its apps.
        for app in ["Microsoft PowerPoint", "Microsoft Word", "Microsoft Excel"] {
            dirs.push(std::path::PathBuf::from(format!("/Applications/{app}.app/Contents/Resources/DFonts")));
        }
        // Fonts macOS ships as assets (PingFang, Yu Mincho, …): com_apple_MobileAsset_Font<N>.
        for parent in ["/System/Library/AssetsV2", "/System/Library/AssetsV2/PreinstalledAssetsV2/InstallWithOs"] {
            let Ok(entries) = std::fs::read_dir(parent) else { continue };
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with("com_apple_MobileAsset_Font") {
                    dirs.push(e.path());
                }
            }
        }
    } else if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
        dirs.push(root.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            let l = std::path::PathBuf::from(l);
            dirs.push(l.join("Microsoft\\Windows\\Fonts"));
            // Fonts Microsoft Office downloads on demand (Aptos…), used where Office put them;
            // DeckCraft never ships or needs them.
            dirs.push(l.join("Microsoft\\FontCache\\4\\CloudFonts"));
        }
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(Into::into));
        if let Some(h) = &home {
            dirs.push(h.join(".fonts"));
            dirs.push(h.join(".local/share/fonts"));
        }
        let env = |k: &str| std::env::var_os(k).map(|v| v.to_string_lossy().into_owned());
        dirs.extend(xdg_font_dirs(env("XDG_DATA_HOME").as_deref(), env("XDG_DATA_DIRS").as_deref(), home.as_deref()));
        // Inside Flatpak the host's fonts are mounted here.
        dirs.extend(["/run/host/fonts", "/run/host/local-fonts", "/run/host/user-fonts"].map(Into::into));
        // Folders fontconfig is configured with (NixOS, Guix, custom setups).
        for conf in fontconfig_files() {
            if let Ok(text) = std::fs::read_to_string(&conf) {
                dirs.extend(fontconfig_dirs(&text, env("XDG_DATA_HOME").as_deref(), home.as_deref()));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// `<data dir>/fonts` for `XDG_DATA_HOME` (default `~/.local/share`) and each `XDG_DATA_DIRS`
/// entry (default `/usr/local/share:/usr/share`).
fn xdg_font_dirs(data_home: Option<&str>, data_dirs: Option<&str>, home: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    match data_home.filter(|d| !d.is_empty()) {
        Some(d) => v.push(std::path::Path::new(d).join("fonts")),
        None => {
            if let Some(h) = home {
                v.push(h.join(".local/share/fonts"));
            }
        }
    }
    let dirs = data_dirs.filter(|d| !d.is_empty()).unwrap_or("/usr/local/share:/usr/share");
    v.extend(dirs.split(':').filter(|d| d.starts_with('/')).take(64).map(|d| std::path::Path::new(d).join("fonts")));
    v
}

/// The fontconfig configuration files: `/etc/fonts/fonts.conf` and `/etc/fonts/conf.d/*.conf`.
fn fontconfig_files() -> Vec<std::path::PathBuf> {
    let mut v = vec![std::path::PathBuf::from("/etc/fonts/fonts.conf"), std::path::PathBuf::from("/etc/fonts/local.conf")];
    if let Ok(entries) = std::fs::read_dir("/etc/fonts/conf.d") {
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "conf")).collect();
        files.sort();
        files.truncate(512);
        v.extend(files);
    }
    v
}

/// The `<dir>` folders of a fontconfig file (`prefix="xdg"` is under `XDG_DATA_HOME`, a leading
/// `~` under home; relative and `cachedir` entries are skipped).
fn fontconfig_dirs(text: &str, data_home: Option<&str>, home: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    let mut rest = text;
    while let Some(start) = rest.find("<dir") {
        let after = &rest[start + 4..];
        // `<dir>` or `<dir …>`, not `<dirname>`.
        if !(after.starts_with('>') || after.starts_with(' ')) {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        let attrs = &after[..gt];
        let body = &after[gt + 1..];
        let Some(end) = body.find("</dir>") else { break };
        let path = body[..end].trim();
        rest = &body[end..];
        if attrs.trim_end().ends_with('/') || path.is_empty() || v.len() >= 64 {
            continue;
        }
        let p = if attrs.contains("prefix=\"xdg\"") {
            let base = data_home.filter(|d| !d.is_empty()).map(std::path::PathBuf::from).or_else(|| home.map(|h| h.join(".local/share")));
            base.map(|b| b.join(path))
        } else if let Some(r) = path.strip_prefix("~/") {
            home.map(|h| h.join(r))
        } else if path.starts_with('/') {
            Some(std::path::PathBuf::from(path))
        } else {
            None
        };
        v.extend(p);
    }
    v
}

/// One face found in a font file: index, family, style and (variable fonts) the named instance's
/// axis settings.
type Found = (u32, String, String, Vec<([u8; 4], f32)>);

/// Parse every face in `data` (a font file or collection); a variable font yields one face per
/// named instance (InDesign lists them as styles).
fn enumerate_faces(data: &[u8]) -> Vec<Found> {
    let count = match FileRef::new(data) {
        Ok(FileRef::Font(_)) => 1,
        Ok(FileRef::Collection(c)) => c.len(),
        Err(_) => 0,
    };
    let mut out = Vec::new();
    for i in 0..count {
        let Ok(f) = skrifa::FontRef::from_index(data, i) else { continue };
        let Some((family, style)) = face_names(&f) else { continue };
        let axes = f.axes();
        let mut named = Vec::new();
        for ni in f.named_instances().iter() {
            let Some(style) = name(&f, &[ni.subfamily_name_id()]) else { continue };
            if named.iter().any(|(_, s, _): &(u32, String, _)| s.eq_ignore_ascii_case(&style)) {
                continue;
            }
            let coords: Vec<([u8; 4], f32)> = axes.iter().zip(ni.user_coords()).map(|(a, v)| (a.tag().to_be_bytes(), v)).collect();
            named.push((i, style, coords));
        }
        if named.is_empty() {
            out.push((i, family, style, Vec::new()));
        } else {
            out.extend(named.into_iter().map(|(i, s, c)| (i, family.clone(), s, c)));
        }
    }
    out
}

/// The compiled-in last-resort face, for a database that somehow has no usable faces at all.
pub(crate) fn last_resort_face() -> Arc<FontFace> {
    static FACE: std::sync::OnceLock<Arc<FontFace>> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        // The font is compiled in (`include_bytes!`), so parsing it can't depend on input; the
        // `last_resort_face_parses` test proves it on every run.
        #[allow(clippy::expect_used)]
        let face = make_face(FontBytes::Static(epaint_default_fonts::UBUNTU_LIGHT), 0, LAST_RESORT_FAMILY.into(), "Regular".into(), Vec::new())
            .expect("the compiled-in last-resort face parses");
        Arc::new(face)
    })
    .clone()
}

/// The share of a line above the baseline (`FontFace::line_split`): OS/2 `usWinAscent` over
/// `usWinAscent + usWinDescent`, else the ascent over ascent plus descent. Fonts whose Windows
/// extent is huge (Gabriola, Cambria Math: over 1.5 em) get 0.81, where PowerPoint puts their
/// baseline (observed).
fn line_split(f: &skrifa::FontRef, ascent: f64, descent: f64, upem: f64) -> f64 {
    use skrifa::raw::TableProvider;
    let (a, d) = match f.os2() {
        Ok(os2) if os2.us_win_ascent() > 0 => (os2.us_win_ascent() as f64, os2.us_win_descent() as f64),
        _ => (ascent, descent),
    };
    let total = a + d;
    if !total.is_finite() || total <= 0.0 || a < 0.0 || d < 0.0 {
        return 0.8;
    }
    if total > upem * 1.5 { 0.81 } else { (a / total).clamp(0.5, 1.0) }
}

fn make_face(bytes: FontBytes, index: u32, family: String, style: String, coords: Vec<([u8; 4], f32)>) -> Option<FontFace> {
    let data: &[u8] = match &bytes {
        FontBytes::Static(b) => b,
        FontBytes::Owned(v) => v.as_slice(),
    };
    let f = skrifa::FontRef::from_index(data, index).ok()?;
    let settings: Vec<(skrifa::Tag, f32)> = coords.iter().map(|(t, v)| (skrifa::Tag::new(t), *v)).collect();
    let location = if coords.is_empty() { Location::default() } else { f.axes().location(settings.iter().copied()) };
    let m = f.metrics(Size::unscaled(), &location);
    let a = f.attributes();
    let hb = harfrust::FontRef::from_index(data, index).ok()?;
    let shaper = harfrust::ShaperData::new(&hb);
    let instance = (!coords.is_empty())
        .then(|| harfrust::ShaperInstance::from_variations(&hb, settings.iter().map(|(t, v)| harfrust::Variation { tag: *t, value: *v })));
    let axis = |tag: &[u8; 4]| coords.iter().find(|(t, _)| t == tag).map(|(_, v)| *v);
    let weight = axis(b"wght").unwrap_or(if coords.is_empty() { a.weight.value() } else { style_weight(&style) });
    let italic = axis(b"ital")
        .map(|v| v >= 0.5)
        .or(axis(b"slnt").map(|v| v.abs() > 0.1))
        .unwrap_or(!matches!(a.style, skrifa::attribute::Style::Normal) || style_italic(&style));
    let legacy = if coords.is_empty() { legacy_names(&f, &family) } else { None };
    let names = other_names(&name_records(&f), &family, legacy.as_ref().map(|l| l.0.as_str()), coords.is_empty());
    let keys = NameKeys::new(&family, legacy.as_ref().map(|l| l.0.as_str()), &names.local, &names.exact);
    let loose_keys = keys.compact();
    let bold_linked = {
        use skrifa::raw::TableProvider;
        f.os2().is_ok_and(|os2| os2.fs_selection().contains(skrifa::raw::tables::os2::SelectionFlags::BOLD))
    };
    Some(FontFace {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        serif: serif_design(&f, &family),
        tradition: crate::substitute::tradition_of_family(&family),
        family,
        style,
        legacy,
        local_names: names.local,
        keys,
        loose_keys,
        bold_linked,
        weight,
        italic,
        upem: m.units_per_em.max(1) as f64,
        ascent: m.ascent as f64,
        descent: -(m.descent as f64),
        cap_height: m.cap_height.map(|v| v as f64).filter(|v| *v > 0.0).unwrap_or(m.ascent as f64 * 0.72),
        x_height: m.x_height.map(|v| v as f64).filter(|v| *v > 0.0).unwrap_or(m.ascent as f64 * 0.5),
        line_split: line_split(&f, m.ascent as f64, -(m.descent as f64), m.units_per_em.max(1) as f64),
        underline: underline_metrics(m.underline, m.units_per_em.max(1) as f64),
        strikeout: strikeout_metrics(m.strikeout, m.units_per_em.max(1) as f64),
        shaper,
        coords,
        location,
        instance,
        bytes,
        index,
        bmp: std::sync::OnceLock::new(),
        color: std::sync::OnceLock::new(),
    })
}

/// The underline's top edge below the baseline and its thickness, font units, from the `post`
/// table (its position is the top edge: Arial's -217/2048 is where PowerPoint's underline starts,
/// 10 px below the baseline at 96 pt); fonts without a usable one get 0.1 em and 0.05 em.
fn underline_metrics(d: Option<skrifa::metrics::Decoration>, upem: f64) -> (f64, f64) {
    match d {
        Some(d) if d.thickness > 0.0 && (d.thickness as f64) < upem * 0.3 && (d.offset as f64).abs() < upem * 0.5 => {
            (-(d.offset as f64), d.thickness as f64)
        }
        _ => (upem * 0.1, upem * 0.05),
    }
}

/// The strikethrough's top edge above the baseline and its thickness, font units, from OS/2 (its
/// position is the top edge: Arial's 530/2048 is where PowerPoint's strikethrough starts, 25 px
/// above the baseline at 96 pt); fonts without a usable one get 0.26 em and 0.05 em.
fn strikeout_metrics(d: Option<skrifa::metrics::Decoration>, upem: f64) -> (f64, f64) {
    match d {
        Some(d) if d.thickness > 0.0 && (d.thickness as f64) < upem * 0.3 && d.offset > 0.0 && (d.offset as f64) < upem => {
            (d.offset as f64, d.thickness as f64)
        }
        _ => (upem * 0.26, upem * 0.05),
    }
}

/// The named style part of a style with axis settings (`Bold {wght:650}` → `Bold`).
pub fn base_style(style: &str) -> &str {
    style.split('{').next().unwrap_or(style).trim()
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Weight implied by a style name. Hiragino's and other Japanese fonts' W0–W9 are their
/// usWeightClass: W3 300, W6 600 (W2 250).
pub(crate) fn style_weight(style: &str) -> f32 {
    let s = norm(style);
    if let [b'w', d] = s.as_bytes()
        && d.is_ascii_digit()
    {
        return match d - b'0' {
            0 => 100.0,
            2 => 250.0,
            n => f32::from(n) * 100.0,
        };
    }
    const TABLE: &[(&str, f32)] = &[
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("hairline", 100.0),
        ("thin", 100.0),
        ("light", 300.0),
        ("medium", 500.0),
        ("bold", 700.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    TABLE.iter().find(|(k, _)| s.contains(k)).map(|(_, w)| *w).unwrap_or(400.0)
}

pub(crate) fn style_italic(style: &str) -> bool {
    let s = norm(style);
    s.contains("italic") || s.contains("oblique") || s == "it"
}

/// One of the four styles text asks for, for `style`: Regular, Bold, Italic or Bold Italic.
pub(crate) fn plain_style(style: &str) -> &'static str {
    match (style_weight(style) >= 600.0, style_italic(style)) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    }
}

/// The weight from which a face is drawn as bold without being made bolder (the text layout
/// emboldens lighter faces for bold text): a weight-named family's W6 to W9 are their own bold.
const ALREADY_BOLD: f32 = 550.0;

/// The bold face a weight-named family's `base` face pairs with, as macOS pairs them: in the
/// same typographic family, marked bold in OS/2 but not the Bold of another Windows family (Yu
/// Gothic Bold is Yu Gothic's, not Yu Gothic Light's), the lightest at least 250 heavier
/// (Hiragino W3 → W6, Hiragino Mincho ProN W3 → W6).
fn bold_partner(faces: &[Arc<FontFace>], base: &FontFace) -> Option<Arc<FontFace>> {
    faces
        .iter()
        .filter(|f| {
            f.keys.family == base.keys.family && f.id != base.id && f.bold_linked && f.italic == base.italic && f.weight >= base.weight + 250.0
        })
        .filter(|f| !norm(f.legacy.as_ref().map_or(f.style.as_str(), |l| l.1.as_str())).contains("bold"))
        .min_by(|a, b| a.weight.total_cmp(&b.weight))
        .cloned()
}

impl FontDb {
    /// A database holding the bundled fonts (the craft-fonts faces in manifest order, Latin then
    /// Japanese, empty without `CRAFT_FONTS_DIR`, then the last-resort face), whose system font
    /// scan reads `font_dirs`.
    pub fn with_font_dirs(font_dirs: Vec<std::path::PathBuf>) -> Self {
        Self::with_files(bundled(), font_dirs)
    }

    /// A database with the last-resort face only (no craft-fonts faces), for tests whose
    /// expectations must not depend on how the crate was built. No system fallback.
    #[cfg(test)]
    pub(crate) fn without_bundled(font_dirs: Vec<std::path::PathBuf>) -> Self {
        let db = Self::with_files(vec![epaint_default_fonts::UBUNTU_LIGHT], font_dirs);
        db.set_system_fallback(false);
        db
    }

    fn with_files(files: Vec<&'static [u8]>, font_dirs: Vec<std::path::PathBuf>) -> Self {
        let mut faces = Vec::new();
        for data in files {
            for (i, family, style, coords) in enumerate_faces(data) {
                if let Some(f) = make_face(FontBytes::Static(data), i, family, style, coords) {
                    faces.push(Arc::new(f));
                }
            }
        }
        Self {
            faces: RwLock::new(faces),
            outlines: Mutex::new(HashMap::new()),
            ea_faces: RwLock::new(HashMap::new()),
            #[cfg(not(target_arch = "wasm32"))]
            catalog: RwLock::new(Catalog::default()),
            font_dirs,
            #[cfg(not(target_arch = "wasm32"))]
            cataloged: std::sync::OnceLock::new(),
            #[cfg(not(target_arch = "wasm32"))]
            loaded_families: Mutex::new(std::collections::HashSet::new()),
            #[cfg(not(target_arch = "wasm32"))]
            sys: Mutex::new(SysFallback { enabled: true, ..Default::default() }),
            #[cfg(not(target_arch = "wasm32"))]
            loading: Mutex::new(()),
        }
    }

    /// Enable or disable the lazy system-font fallback for characters the loaded fonts lack
    /// (native only; on by default).
    pub fn set_system_fallback(&self, on: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner).enabled = on;
        }
        #[cfg(target_arch = "wasm32")]
        let _ = on;
    }

    /// Process-wide database: the bundled fonts, then the installed system fonts, cataloged the
    /// first time a lookup by family name needs them (or ahead of time by
    /// [`FontDb::scan_in_background`]).
    pub fn global() -> &'static FontDb {
        static DB: std::sync::OnceLock<FontDb> = std::sync::OnceLock::new();
        DB.get_or_init(|| FontDb::with_font_dirs(system_font_dirs()))
    }

    pub(crate) fn read_faces(&self) -> std::sync::RwLockReadGuard<'_, Vec<Arc<FontFace>>> {
        self.faces.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The system font catalog, scanned first if it hasn't been yet.
    #[cfg(not(target_arch = "wasm32"))]
    fn read_catalog(&self) -> std::sync::RwLockReadGuard<'_, Catalog> {
        self.ensure_catalog();
        self.catalog.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Family names available (loaded plus cataloged system fonts), sorted and deduplicated.
    pub fn families(&self) -> Vec<String> {
        let mut v: Vec<String> = self.read_faces().iter().map(|f| f.family.clone()).collect();
        #[cfg(not(target_arch = "wasm32"))]
        v.extend(self.read_catalog().entries.iter().map(|c| c.family.clone()));
        v.sort_by_key(|a| a.to_lowercase());
        v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        v
    }

    /// The English family `family` names — itself, or the family a name in another language
    /// stands for (ヒラギノ角ゴシック → Hiragino Sans) — and that family's names in other
    /// languages as (language, name) pairs, each name once: what a font list shows for a family
    /// in a Japanese, Chinese or Korean interface. `None` for a name no loaded or installed font
    /// has.
    pub fn family_names(&self, family: &str) -> Option<(String, Vec<(String, String)>)> {
        let hit = self.resolve_name(family)?;
        let english = if hit.exact { self.find_exact(family).map(|f| f.family.clone()).unwrap_or(hit.family) } else { hit.family };
        let key = fold_font_name(&english);
        let mut names: Vec<(String, String)> = Vec::new();
        let mut add = |list: &[LocalName]| {
            for n in list.iter().filter(|n| fold_font_name(&n.english) == key) {
                if !names.iter().any(|(_, m)| *m == n.name) {
                    names.push((n.lang.clone(), n.name.clone()));
                }
            }
        };
        for f in self.read_faces().iter() {
            add(&f.local_names);
        }
        #[cfg(not(target_arch = "wasm32"))]
        for c in self.read_catalog().entries.iter() {
            add(&c.local);
        }
        Some((english, names))
    }

    /// Every family's name in the interface language `lang` (`ja`, `zh-Hans`, `zh-Hant`, `ko`),
    /// as (English name, localized name) pairs sorted by English name: Meiryo is メイリオ and
    /// Hiragino Sans ヒラギノ角ゴシック in Japanese. Families without a name in that language are
    /// left out.
    pub fn localized_names(&self, lang: &str) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = Vec::new();
        let mut add = |list: &[LocalName]| {
            for n in list.iter().filter(|n| name_language_matches(lang, &n.lang)) {
                if !v.iter().any(|(e, _)| e.eq_ignore_ascii_case(&n.english)) {
                    v.push((n.english.clone(), n.name.clone()));
                }
            }
        };
        for f in self.read_faces().iter() {
            add(&f.local_names);
        }
        #[cfg(not(target_arch = "wasm32"))]
        for c in self.read_catalog().entries.iter() {
            add(&c.local);
        }
        v.sort_by_key(|(e, _)| e.to_lowercase());
        v
    }

    /// Style names available for `family` (Regular first, then by weight); for a name in another
    /// language, those of the family it stands for.
    pub fn styles(&self, family: &str) -> Vec<String> {
        let family = match self.resolve_name(family) {
            Some(hit) if hit.rank > 0 && !hit.exact => hit.family,
            _ => family.to_string(),
        };
        let key = fold_font_name(&family);
        let mut v: Vec<(bool, f32, String)> = self
            .read_faces()
            .iter()
            .filter(|f| f.keys.family == key && !f.style.contains('{'))
            .map(|f| (f.italic, f.weight, f.style.clone()))
            .collect();
        #[cfg(not(target_arch = "wasm32"))]
        for c in self.read_catalog().entries.iter() {
            if c.key == key && !v.iter().any(|(_, _, s)| s.eq_ignore_ascii_case(&c.style)) {
                v.push((style_italic(&c.style), style_weight(&c.style), c.style.clone()));
            }
        }
        v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        v.dedup_by(|a, b| a.2 == b.2);
        v.into_iter().map(|t| t.2).collect()
    }

    /// Add a user font (TTF/OTF/TTC bytes). Returns the number of faces added (0 if unparseable or
    /// every face was already present).
    pub fn add_font(&self, bytes: Vec<u8>) -> usize {
        self.add_fonts(vec![bytes])
    }

    /// Add font files together: lookups see all of their faces or none, never part of a family.
    /// Returns the number of faces added.
    fn add_fonts(&self, files: Vec<Vec<u8>>) -> usize {
        let present = |faces: &[Arc<FontFace>], family: &str, style: &str| {
            faces.iter().any(|f| f.family.eq_ignore_ascii_case(family) && f.style.eq_ignore_ascii_case(style))
        };
        let mut new = Vec::new();
        for bytes in files {
            let data = Arc::new(bytes);
            for (i, family, style, coords) in enumerate_faces(&data) {
                if present(&self.read_faces(), &family, &style) {
                    continue;
                }
                if let Some(f) = make_face(FontBytes::Owned(data.clone()), i, family, style, coords) {
                    new.push(Arc::new(f));
                }
            }
        }
        let mut faces = self.faces.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut added = 0;
        for f in new {
            // Checked again under the lock: another thread may have added the same face.
            if !present(&faces, &f.family, &f.style) {
                faces.push(f);
                added += 1;
            }
        }
        added
    }

    /// Catalog the installed fonts unless that has been done: the first caller scans the font
    /// folders, any other waits for that scan to finish.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_catalog(&self) {
        self.cataloged.get_or_init(|| {
            self.scan_font_dirs();
        });
    }

    /// Catalog the installed fonts on a background thread, so the first lookup by family name
    /// (opening a file, the font menus) doesn't wait for the scan. A no-op once they are
    /// cataloged, and on wasm.
    pub fn scan_in_background(&'static self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.cataloged.get().is_none() {
            // A failed spawn leaves the scan to the first lookup that needs it.
            let _ = std::thread::Builder::new().name("font-scan".into()).spawn(move || self.ensure_catalog());
        }
    }

    /// Scan the font folders again (fonts installed or removed since), cataloging the faces
    /// found (native only). Font data is loaded when a cataloged family is first resolved.
    /// Returns the number of faces cataloged.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_system_fonts(&self) -> usize {
        // Families load again from the new catalog when next looked up, and East Asian fallbacks
        // are looked up again among them.
        self.loaded_families.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        self.ea_faces.write().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        // The first scan, or a rescan once it is done (never both at once).
        let mut first = None;
        self.cataloged.get_or_init(|| first = Some(self.scan_font_dirs()));
        first.unwrap_or_else(|| self.scan_font_dirs())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn scan_font_dirs(&self) -> usize {
        let mut found = Vec::new();
        let mut stack: Vec<(std::path::PathBuf, usize)> = self.font_dirs.iter().map(|d| (d.clone(), 0)).collect();
        // Each folder once, however links lead back to it.
        let mut visited = std::collections::HashSet::new();
        let mut entries = 0usize;
        while let Some((d, depth)) = stack.pop() {
            if !visited.insert(std::fs::canonicalize(&d).unwrap_or_else(|_| d.clone())) {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                // A link to `/` or a vast tree mustn't turn start-up into a disk crawl.
                entries += 1;
                if entries > MAX_SCAN_ENTRIES {
                    log::warn!("font scan stopped after {MAX_SCAN_ENTRIES} entries");
                    stack.clear();
                    break;
                }
                let p = e.path();
                // Follows links: what matters is what the entry leads to.
                let Ok(meta) = std::fs::metadata(&p) else { continue };
                if meta.is_dir() {
                    if depth < MAX_SCAN_DEPTH {
                        stack.push((p, depth + 1));
                    }
                    continue;
                }
                let ext = p.extension().and_then(std::ffi::OsStr::to_str).map(str::to_ascii_lowercase);
                if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
                    continue;
                }
                // Only regular files: opening a FIFO named like a font blocks until something
                // writes to it (start-up hung), and a device never ends.
                if !font_file_ok(&meta) {
                    continue;
                }
                found.extend(file_face_names(&p));
            }
        }
        let n = found.len();
        log::debug!("cataloged {n} system font faces");
        *self.catalog.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Catalog::new(found);
        n
    }

    /// Load the files of the installed `family` (faces already loaded are kept). Returns whether
    /// it is loaded.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_cataloged(&self, family: &str) -> bool {
        let loading = self.loading.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.load_family(family, &loading)
    }

    /// [`FontDb::load_cataloged`] for a caller already holding `loading`.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_family(&self, family: &str, _loading: &std::sync::MutexGuard<'_, ()>) -> bool {
        // Not skipped when some face of the family is loaded: a lookup by a Windows family name
        // loads only that style's file, and the family's other files must still load later.
        let paths: Vec<std::path::PathBuf> = {
            let cat = self.read_catalog();
            let key = fold_font_name(family);
            let mut p: Vec<_> = cat.entries.iter().filter(|c| c.key == key).map(|c| c.path.clone()).collect();
            p.sort();
            p.dedup();
            p
        };
        self.add_fonts(paths.iter().filter(|p| std::fs::metadata(p).is_ok_and(|m| font_file_ok(&m))).filter_map(|p| std::fs::read(p).ok()).collect());
        self.is_loaded(family)
    }

    /// Resolve a family + style to a face, falling back to the closest style of the family, then to
    /// an open substitute ([`substitutes`]) or, for a Chinese, Japanese or Korean font, an installed
    /// face of its kind (a gothic for ＭＳ Ｐゴシック, a Mincho for 游明朝), then to Inter. The
    /// family is found by any of its names (English, localized, full or PostScript, compared
    /// with [`fold_font_name`]). Installed system fonts are found by name whatever ran before.
    pub fn face(&self, family: &str, style: &str) -> Arc<FontFace> {
        if let Some(f) = self.try_family(family, style) {
            return f;
        }
        for sub in substitutes(family) {
            if let Some(f) = self.try_family(sub, style) {
                return f;
            }
        }
        if let Some(f) = self.cjk_substitute(family, style) {
            return f;
        }
        for sub in [FALLBACK_FAMILY, "Helvetica Neue", "Arial", "Liberation Sans", "DejaVu Sans", "Segoe UI"] {
            if let Some(f) = self.try_family(sub, style) {
                return f;
            }
        }
        self.find(LAST_RESORT_FAMILY, "Regular")
            .or_else(|| self.find(FALLBACK_FAMILY, "Regular"))
            .or_else(|| self.read_faces().first().cloned())
            .unwrap_or_else(last_resort_face)
    }

    /// The face a document's text in `family` is drawn with, as PowerPoint draws it: the family
    /// when it is installed or bundled (by any of its names: 游ゴシック finds Yu Gothic), or a
    /// metric-compatible stand-in (Carlito for Calibri, Liberation Sans for Arial…), or the family
    /// PowerPoint maps it to (Liberation Sans as Arial, see `office_twin`). A missing Chinese,
    /// Japanese or Korean font is drawn — Latin letters included — in an installed face of its
    /// kind, as Office draws such text in an East Asian face (see [`FontDb::face`]); any other
    /// missing family, and a CJK one with no stand-in installed, in Calibri (or Carlito).
    /// DeckCraft's own default family keeps [`FontDb::face`]'s fallbacks.
    pub fn document_face(&self, family: &str, style: &str) -> Arc<FontFace> {
        if family.eq_ignore_ascii_case(FALLBACK_FAMILY) || has_metric_twin(family) || is_office_cloud_font(family) || self.has_family(family) {
            return self.face(family, style);
        }
        if let Some(twin) = office_twin(family) {
            return self.face(twin, style);
        }
        if let Some(f) = self.cjk_substitute(family, style) {
            return f;
        }
        self.face(MISSING_FONT_FAMILY, style)
    }

    /// The face for `family` when it is installed or bundled (by any of its names), or, for a
    /// Chinese, Japanese or Korean font that isn't, an installed face of its kind standing in for
    /// it (a Mincho for a missing ＭＳ 明朝); `None` otherwise. What a run's East Asian font
    /// (`a:ea`) or a theme's script font is drawn with.
    pub fn available_face(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        self.try_family(family, style).or_else(|| self.cjk_substitute(family, style))
    }

    /// The first installed (or craft-fonts) stand-in for a missing CJK `family`.
    fn cjk_substitute(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        crate::substitute::cjk_substitutes(family)?.iter().find_map(|s| self.try_family(s, style))
    }

    /// The face of `family` (by any of its names) in `style`, loading its installed files first;
    /// `None` when no loaded or installed face has the name. Never another family's face.
    pub(crate) fn try_family(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        if let Some(f) = self.try_named(family, style) {
            return Some(f);
        }
        // A family name in another language, a full or PostScript name, or a name spaced
        // differently: the family (or the face) it names.
        let hit = self.resolve_name(family).filter(|h| h.exact || fold_font_name(&h.family) != fold_font_name(family))?;
        #[cfg(not(target_arch = "wasm32"))]
        self.ensure_loaded(&hit.family);
        if hit.exact {
            return self.find_exact(family);
        }
        self.try_named(&hit.family, style)
    }

    /// [`FontDb::try_family`] for an English (typographic or Windows) family name.
    fn try_named(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        #[cfg(not(target_arch = "wasm32"))]
        self.ensure_loaded(family);
        let found = self.find(family, style)?;
        // Bold of a weight-named Windows family (Hiragino Kaku Gothic ProN W3): the bold face it
        // pairs with (W6) is in the rest of the typographic family, which loads now. Decided by
        // the plain face's weight, not by what `find` settled for among the faces loaded so far.
        #[cfg(not(target_arch = "wasm32"))]
        if style_weight(style) >= 600.0
            && found.keys.family != fold_font_name(family)
            && self.find(family, if style_italic(style) { "Italic" } else { "Regular" }).is_some_and(|plain| plain.weight < ALREADY_BOLD)
            && self.ensure_loaded(&found.family)
        {
            return self.find(family, style).or(Some(found));
        }
        Some(found)
    }

    /// Load every installed file of `family` unless that was done; true if it was done now. The
    /// lock is held while the files load, so another thread looking up the family waits for them
    /// rather than finding none and falling back to another family.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_loaded(&self, family: &str) -> bool {
        let mut loaded = self.loaded_families.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = fold_font_name(family);
        if loaded.contains(&key) {
            return false;
        }
        self.load_cataloged(family);
        loaded.insert(key);
        true
    }

    /// What `family` names among the loaded and installed faces: an English family name first,
    /// then a family name in another language (ヒラギノ角ゴ ProN W3 → Hiragino Kaku Gothic ProN
    /// W3), then a full or PostScript name (Osaka−等幅), compared with [`fold_font_name`]; failing
    /// all of them, the same names compared without spaces ([`fold_compact`]: MSPGothic).
    fn resolve_name(&self, family: &str) -> Option<NameHit> {
        let key = fold_font_name(family);
        if key.is_empty() {
            return None;
        }
        let best = |hits: &mut dyn Iterator<Item = NameHit>| hits.min_by_key(|h| h.rank);
        let found = {
            let faces = self.read_faces();
            best(&mut faces.iter().filter_map(|f| f.name_hit(&key)))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let found = {
            let cat = self.read_catalog();
            best(&mut found.into_iter().chain(cat.names.get(&key).cloned()))
        };
        if found.is_some() {
            return found;
        }
        let compact = fold_compact(family);
        let found = {
            let faces = self.read_faces();
            best(&mut faces.iter().filter_map(|f| f.loose_hit(&compact)))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let found = {
            let cat = self.read_catalog();
            best(&mut found.into_iter().chain(cat.loose.get(&compact).cloned()))
        };
        found
    }

    /// The loaded face whose full or PostScript name is `name` (folded, else without spaces).
    fn find_exact(&self, name: &str) -> Option<Arc<FontFace>> {
        let (key, compact) = (fold_font_name(name), fold_compact(name));
        let faces = self.read_faces();
        faces.iter().find(|f| f.keys.exact.contains(&key)).or_else(|| faces.iter().find(|f| f.loose_keys.exact.contains(&compact))).cloned()
    }

    /// Is `family` available (loaded, or installed on the system), by any of its names: English
    /// (typographic or Windows), in another language (メイリオ, ＭＳ Ｐゴシック, ヒラギノ角ゴ ProN
    /// W3), full or PostScript, compared with [`fold_font_name`]?
    pub fn has_family(&self, family: &str) -> bool {
        self.resolve_name(family).is_some()
    }

    /// Is a face named `family` (by any of its names) loaded?
    pub(crate) fn is_loaded(&self, family: &str) -> bool {
        let key = fold_font_name(family);
        !key.is_empty() && self.read_faces().iter().any(|f| f.name_hit(&key).is_some())
    }

    /// A variable font's axes: (tag, name, min, default, max), empty for static fonts.
    pub fn axes(&self, family: &str, style: &str) -> Vec<(String, String, f32, f32, f32)> {
        let face = self.face(family, base_style(style));
        let Some(f) = face.skrifa() else { return vec![] };
        f.axes()
            .iter()
            .map(|a| {
                let tag = String::from_utf8_lossy(&a.tag().to_be_bytes()).to_string();
                let name = f.localized_strings(a.name_id()).english_or_first().map(|s| s.to_string()).unwrap_or_else(|| tag.clone());
                (tag, name, a.min_value(), a.default_value(), a.max_value())
            })
            .collect()
    }

    /// `Style {wght:650,wdth:90}`: the named style's font at those axis values, made on first use.
    fn instance(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        let open = style.find('{')?;
        let base = self.find(family, style[..open].trim())?;
        if base.skrifa().is_none_or(|f| f.axes().is_empty()) {
            return Some(base);
        }
        let mut coords = base.coords.clone();
        for kv in style[open + 1..].trim_end_matches('}').split(',') {
            let (k, v) = kv.split_once(':')?;
            let k = k.trim().as_bytes();
            let v: f32 = v.trim().parse().ok()?;
            if k.len() != 4 {
                return None;
            }
            let tag = [k[0], k[1], k[2], k[3]];
            match coords.iter_mut().find(|(t, _)| *t == tag) {
                Some(c) => c.1 = v,
                None => coords.push((tag, v)),
            }
        }
        let f = make_face(base.bytes.clone(), base.index, base.family.clone(), style.to_string(), coords)?;
        let f = Arc::new(f);
        self.faces.write().unwrap_or_else(std::sync::PoisonError::into_inner).push(f.clone());
        Some(f)
    }

    /// The face of the English (typographic or Windows) `family` closest to `style`, among the
    /// loaded faces. Names compare folded ([`fold_font_name`]).
    fn find(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        let key = fold_font_name(family);
        if style.contains('{') {
            {
                let faces = self.read_faces();
                if let Some(f) = faces.iter().find(|f| f.keys.family == key && f.style == style) {
                    return Some(f.clone());
                }
            }
            return self.instance(family, style);
        }
        let faces = self.read_faces();
        let mut cands: Vec<&Arc<FontFace>> = faces.iter().filter(|f| f.keys.family == key).collect();
        let ns = norm(style);
        if cands.is_empty() {
            // A Windows family name ("Segoe UI Semibold", "Arial Black"): its faces, matched by
            // their Windows style ("Regular", "Bold"…).
            cands = faces.iter().filter(|f| f.keys.legacy.as_deref() == Some(key.as_str())).collect();
            if let Some(f) = cands.iter().find(|f| f.legacy.as_ref().is_some_and(|(_, ls)| norm(ls) == ns)) {
                return Some((*f).clone());
            }
            // Bold of a family that is itself a weight (Semibold, Light, W3): the bold face the
            // font pairs with it (Hiragino's W6 for W3, as macOS draws it), else the face itself,
            // made bolder by the caller, as Windows does, rather than a face of another weight. A
            // face that is already bold (W6 and heavier) is its own bold, whatever heavier
            // weights are loaded.
            let plain = |italic: bool| {
                cands.iter().find(|f| f.legacy.as_ref().is_some_and(|(_, ls)| norm(ls) == (if italic { "italic" } else { "regular" })))
            };
            if let Some(f) = plain(style_italic(style)).or_else(|| plain(false)) {
                if style_weight(style) >= 600.0
                    && f.weight < ALREADY_BOLD
                    && let Some(partner) = bold_partner(&faces, f)
                {
                    return Some(partner);
                }
                return Some((*f).clone());
            }
        }
        if cands.is_empty() {
            return None;
        }
        if let Some(f) = cands.iter().find(|f| norm(&f.style) == ns) {
            return Some((*f).clone());
        }
        let (tw, ti) = (style_weight(style), style_italic(style));
        cands
            .iter()
            .min_by(|a, b| {
                let sa = (a.weight - tw).abs() + if a.italic != ti { 1000.0 } else { 0.0 };
                let sb = (b.weight - tw).abs() + if b.italic != ti { 1000.0 } else { 0.0 };
                sa.total_cmp(&sb)
            })
            .map(|f| (*f).clone())
    }

    /// First face (fallback family first, then load order) that covers `c`; on native, system
    /// fonts are cataloged and loaded lazily the first time no loaded face covers a character.
    /// East Asian characters prefer faces of the system's East Asian tradition
    /// ([`crate::default_cjk_locale`]) when it is set.
    pub fn fallback_for(&self, c: char, exclude: u32) -> Option<Arc<FontFace>> {
        self.fallback_in(c, exclude, crate::default_cjk_locale(), &|_| true, true)
    }

    /// A covering fallback accepted by the caller (for example, an interface that cannot use
    /// colour or very large faces). Rejected faces do not stop the search for another face.
    /// A policy-specific miss is not stored as a general document-font miss.
    pub fn fallback_for_filtered(&self, c: char, exclude: u32, accept: impl Fn(&FontFace) -> bool) -> Option<Arc<FontFace>> {
        self.fallback_in(c, exclude, crate::default_cjk_locale(), &accept, false)
    }

    /// [`FontDb::fallback_for`] in `style`: the chosen family's face in that style when it has
    /// one that covers `c` (bold Japanese text drawn by a fallback family stays bold: Hiragino
    /// W6, Yu Gothic Bold), else the face found. East Asian characters prefer faces of
    /// tradition `cjk`.
    pub fn fallback_styled(&self, c: char, exclude: u32, style: &str, cjk: Option<crate::CjkLocale>) -> Option<Arc<FontFace>> {
        let base = self.fallback_in(c, exclude, cjk, &|_| true, true)?;
        let style = plain_style(style);
        if style == "Regular" {
            return Some(base);
        }
        match self.try_family(&base.family, style) {
            Some(f) if f.keys.family == base.keys.family && f.id != exclude && f.covers(c) => Some(f),
            _ => Some(base),
        }
    }

    /// The fallback search: a loaded face, else an installed one loaded for `c`.
    pub(crate) fn fallback_in(
        &self,
        c: char,
        exclude: u32,
        cjk: Option<crate::CjkLocale>,
        accept: &dyn Fn(&FontFace) -> bool,
        remember_miss: bool,
    ) -> Option<Arc<FontFace>> {
        if let Some(f) = self.loaded_fallback_filtered(c, exclude, accept, cjk) {
            return Some(f);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.system_fallback_filtered(c, &|f| f.id != exclude && accept(f), remember_miss, cjk) {
            return self.loaded_fallback_filtered(c, exclude, accept, cjk);
        }
        let _ = remember_miss;
        None
    }

    /// The first loaded face covering `c` that `accept`s: Inter first, upright, nearest regular
    /// weight. For an East Asian character, faces of tradition `cjk` come before faces of no
    /// known tradition, and those before the other traditions' (Japanese text never settles for
    /// a loaded PingFang SC while a Japanese face is loaded).
    fn loaded_fallback_filtered(
        &self,
        c: char,
        exclude: u32,
        accept: &dyn Fn(&FontFace) -> bool,
        cjk: Option<crate::CjkLocale>,
    ) -> Option<Arc<FontFace>> {
        let faces = self.read_faces();
        let east_asian = cjk.filter(|_| crate::fallback::is_east_asian_char(c));
        let tradition = |f: &FontFace| match (east_asian, f.tradition) {
            (None, _) => 0,
            (Some(l), Some(t)) if l == t => 0,
            (Some(_), None) => 1,
            _ => 2,
        };
        let mut order: Vec<&Arc<FontFace>> = faces.iter().filter(|f| f.id != exclude).collect();
        order.sort_by_key(|f| (tradition(f), !f.family.eq_ignore_ascii_case(FALLBACK_FAMILY), f.italic, (f.weight - 400.0).abs() as i32));
        order.into_iter().find(|f| f.covers(c) && accept(f)).cloned()
    }

    /// Load a system font covering `c` (preferred fallback families first, then any cataloged
    /// file under 40 MB). Returns true if one was loaded. Misses are remembered.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn system_fallback(&self, c: char) -> bool {
        self.system_fallback_filtered(c, &|_| true, true, crate::default_cjk_locale())
    }

    /// [`FontDb::system_fallback`] for faces `accept`ed, trying the preferred families in the
    /// order of tradition `cjk` ([`system_fallbacks`]).
    #[cfg(not(target_arch = "wasm32"))]
    fn system_fallback_filtered(&self, c: char, accept: &dyn Fn(&FontFace) -> bool, remember_miss: bool, cjk: Option<crate::CjkLocale>) -> bool {
        if c.is_control() || c.is_whitespace() {
            return false;
        }
        let seen = self.read_faces().len();
        {
            let sys = self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !sys.enabled || sys.misses.contains(&c) {
                return false;
            }
        }
        let covered = |db: &FontDb| db.read_faces().iter().any(|f| f.covers(c) && accept(f));
        let cataloged: Vec<String> = self.read_catalog().entries.iter().map(|e| e.family.clone()).collect();
        let loading = self.loading.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Another thread may have loaded a face for `c` (faces are only ever added), or found
        // there is none, while this one waited.
        if self.read_faces().len() != seen && covered(self) {
            return true;
        }
        if self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner).misses.contains(&c) {
            return false;
        }
        for fam in system_fallbacks(cjk) {
            if !self.is_loaded(fam) && cataloged.iter().any(|f| f.eq_ignore_ascii_case(fam)) {
                self.load_family(fam, &loading);
                if covered(self) {
                    return true;
                }
            }
        }
        let mut paths: Vec<std::path::PathBuf> =
            self.catalog.read().unwrap_or_else(std::sync::PoisonError::into_inner).entries.iter().map(|e| e.path.clone()).collect();
        paths.sort();
        paths.dedup();
        for p in paths {
            if std::fs::metadata(&p).map(|m| m.len() > 40 << 20).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&p) else { continue };
            let hit =
                enumerate_faces(&data).iter().any(|(i, _, _, _)| skrifa::FontRef::from_index(&data, *i).is_ok_and(|f| f.charmap().map(c).is_some()));
            if hit && self.add_font(data) > 0 && covered(self) {
                return true;
            }
        }
        if remember_miss {
            self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner).misses.insert(c);
        }
        false
    }

    /// A loaded colour face drawing all of `cluster`, loading an installed colour font that covers
    /// `key` if none is loaded yet. Misses are remembered.
    pub(crate) fn color_face_for(&self, key: char, cluster: &[char]) -> Option<Arc<FontFace>> {
        let pick = |db: &FontDb| {
            let faces = db.read_faces();
            let mut v: Vec<&Arc<FontFace>> = faces.iter().filter(|f| crate::covers_cluster(f, cluster) && f.is_color()).collect();
            v.sort_by_key(|f| (f.italic, (f.weight - 400.0).abs() as i32));
            v.first().map(|f| (*f).clone())
        };
        if let Some(f) = pick(self) {
            return Some(f);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.system_color_fallback(key) {
            return pick(self);
        }
        let _ = key;
        None
    }

    /// A loaded plain (not colour) face drawing all of `cluster`, loading an installed plain font
    /// that covers `key` if none is loaded yet. Misses are remembered.
    pub(crate) fn plain_face_for(&self, key: char, cluster: &[char]) -> Option<Arc<FontFace>> {
        // Symbol fonts first (PowerPoint draws ⚠ ♥ ⬆ from the system's symbol font), then the
        // usual fallback families in order, then any other.
        let rank = |f: &FontFace| {
            if f.family.contains("Symbol") {
                return 0;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(i) = SYSTEM_FALLBACKS.iter().position(|s| s.eq_ignore_ascii_case(&f.family)) {
                return i + 1;
            }
            1000
        };
        let pick = |db: &FontDb| {
            let faces = db.read_faces();
            let mut v: Vec<&Arc<FontFace>> = faces.iter().filter(|f| !f.is_color() && crate::covers_cluster(f, cluster)).collect();
            v.sort_by_key(|f| (rank(f), f.italic, (f.weight - 400.0).abs() as i32));
            v.first().map(|f| (*f).clone())
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let cataloged: Vec<String> = self.read_catalog().entries.iter().map(|e| e.family.clone()).collect();
            for fam in SYSTEM_FALLBACKS.iter().filter(|f| f.contains("Symbol")) {
                if !self.is_loaded(fam) && cataloged.iter().any(|f| f.eq_ignore_ascii_case(fam)) {
                    self.load_cataloged(fam);
                }
            }
        }
        if let Some(f) = pick(self) {
            return Some(f);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.system_plain_fallback(key) {
            return pick(self);
        }
        let _ = key;
        None
    }

    /// Load an installed font without colour tables covering `c`. True if one was loaded.
    #[cfg(not(target_arch = "wasm32"))]
    fn system_plain_fallback(&self, c: char) -> bool {
        static MISSES: Mutex<Option<std::collections::HashSet<char>>> = Mutex::new(None);
        if c.is_control() || c.is_whitespace() {
            return false;
        }
        if !self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner).enabled {
            return false;
        }
        if MISSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(Default::default).contains(&c) {
            return false;
        }
        let mut paths: Vec<std::path::PathBuf> = self.read_catalog().entries.iter().map(|e| e.path.clone()).collect();
        paths.sort();
        paths.dedup();
        for p in paths {
            if std::fs::metadata(&p).map(|m| m.len() > 40 << 20).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&p) else { continue };
            let hit = enumerate_faces(&data).iter().any(|(i, _, _, _)| {
                !has_color_tables(&data, *i) && skrifa::FontRef::from_index(&data, *i).is_ok_and(|f| f.charmap().map(c).is_some())
            });
            if hit && self.add_font(data) > 0 {
                return true;
            }
        }
        MISSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(Default::default).insert(c);
        false
    }

    /// Load an installed colour font covering `c`. True if one was loaded.
    #[cfg(not(target_arch = "wasm32"))]
    fn system_color_fallback(&self, c: char) -> bool {
        static MISSES: Mutex<Option<std::collections::HashSet<char>>> = Mutex::new(None);
        if c.is_control() || c.is_whitespace() {
            return false;
        }
        {
            let sys = self.sys.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !sys.enabled {
                return false;
            }
        }
        if MISSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(Default::default).contains(&c) {
            return false;
        }
        let mut paths: Vec<std::path::PathBuf> = self.read_catalog().entries.iter().map(|e| e.path.clone()).collect();
        paths.sort();
        paths.dedup();
        for p in paths {
            if std::fs::metadata(&p).map(|m| m.len() > 64 << 20).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&p) else { continue };
            let hit = enumerate_faces(&data).iter().any(|(i, _, _, _)| {
                has_color_tables(&data, *i) && skrifa::FontRef::from_index(&data, *i).is_ok_and(|f| f.charmap().map(c).is_some())
            });
            if hit && self.add_font(data) > 0 {
                return true;
            }
        }
        MISSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(Default::default).insert(c);
        false
    }

    /// Glyph outline in font units, y-down (flipped), cached per (face, glyph).
    pub fn outline(&self, face: &FontFace, gid: u32) -> Arc<BezPath> {
        let key = (face.id, gid);
        if let Some(p) = self.outlines.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
            return p.clone();
        }
        let mut pen = FlipPen(BezPath::new());
        if let Some(f) = face.skrifa()
            && let Some(g) = f.outline_glyphs().get(GlyphId::new(gid))
        {
            let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), face.location()), &mut pen);
        }
        let p = Arc::new(pen.0);
        let mut cache = self.outlines.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.len() >= OUTLINE_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, p.clone());
        p
    }
}

struct FlipPen(BezPath);

impl OutlinePen for FlipPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, -y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, -y as f64));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to((cx0 as f64, -cy0 as f64), (x as f64, -y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to((cx0 as f64, -cy0 as f64), (cx1 as f64, -cy1 as f64), (x as f64, -y as f64));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(test)]
mod font_dir_tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn xdg_data_dirs_contribute_their_fonts_folders() {
        let home = Path::new("/home/ann");
        assert_eq!(
            xdg_font_dirs(None, None, Some(home)),
            [home.join(".local/share/fonts"), "/usr/local/share/fonts".into(), "/usr/share/fonts".into()]
        );
        let v = xdg_font_dirs(Some("/data/ann"), Some("/nix/profile/share:relative:/var/lib/flatpak/exports/share"), Some(home));
        assert_eq!(v, [PathBuf::from("/data/ann/fonts"), "/nix/profile/share/fonts".into(), "/var/lib/flatpak/exports/share/fonts".into()]);
    }

    #[test]
    fn fontconfig_dir_entries_are_read() {
        let conf = r#"<?xml version="1.0"?>
<fontconfig>
  <dir>/usr/share/fonts</dir>
  <dir prefix="xdg">fonts</dir>
  <dir>~/.fonts</dir>
  <dir>relative/fonts</dir>
  <cachedir>/var/cache/fontconfig</cachedir>
  <dirname>ignored</dirname>
  <dir>/nix/store/abc-fonts/share/fonts</dir>
</fontconfig>"#;
        let home = Path::new("/home/ann");
        let v = fontconfig_dirs(conf, None, Some(home));
        assert_eq!(
            v,
            [PathBuf::from("/usr/share/fonts"), home.join(".local/share/fonts"), home.join(".fonts"), "/nix/store/abc-fonts/share/fonts".into()]
        );
        assert!(fontconfig_dirs("<dir>/never closed", None, Some(home)).is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_dirs_include_flatpak_host_fonts_without_duplicates() {
        let d = system_font_dirs();
        assert!(d.contains(&PathBuf::from("/run/host/user-fonts")));
        let mut u = d.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), d.len());
    }
}
