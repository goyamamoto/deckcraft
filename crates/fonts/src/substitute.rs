//! Stand-ins for Chinese, Japanese and Korean fonts, and the faces East Asian text falls back to.
//!
//! Decks made in Japanese PowerPoint name their fonts in Japanese (游ゴシック, ＭＳ Ｐ明朝,
//! メイリオ, ヒラギノ角ゴ ProN W3) or in English (Yu Gothic, MS PMincho). When the font isn't
//! installed, Office still draws the text — kana, kanji and Latin letters alike — in an installed
//! East Asian face, never in Calibri (measured in Word for Mac, which shares Office's font
//! handling; PowerPoint's own pick is not verified). DeckCraft keeps the font's kind: a gothic
//! (sans) stands in for a gothic, a 明朝 (Mincho, serif) for a Mincho, a rounded gothic for a
//! rounded one, and a fixed-pitch font (ＭＳ ゴシック, ＭＳ 明朝, Osaka−等幅, whose Latin letters
//! are half an em) for a fixed-pitch one when one is there. The stand-ins are the platform's own
//! fonts first (Hiragino on macOS, Yu Gothic and Meiryo on Windows, IPAex, VL and Takao on
//! Linux), then the craft-fonts faces when built with them (BIZ UDPGothic, BIZ UDMincho, Shippori
//! Mincho), then the other platforms' fonts. Nothing here ships a font: installed fonts are used
//! where they are. Adobe-designed families are never named (AGENTS.md §1.1).
//!
//! Names are compared folded ([`fold_compact`]), so ＭＳ Ｐゴシック, MS Pゴシック and
//! "ms pgothic" are one name.

use std::sync::OnceLock;

use crate::CjkLocale;
use crate::fontdb::fold_compact;

/// The kind of an East Asian font, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CjkKind {
    /// Japanese gothic (sans): 游ゴシック, ＭＳ Ｐゴシック, メイリオ, ヒラギノ角ゴ.
    Gothic,
    /// Japanese gothic with half-width Latin: ＭＳ ゴシック, Osaka−等幅.
    GothicMono,
    /// Japanese Mincho (serif): 游明朝, ＭＳ Ｐ明朝, ヒラギノ明朝.
    Mincho,
    /// Japanese Mincho with half-width Latin: ＭＳ 明朝.
    MinchoMono,
    /// Japanese rounded gothic: ヒラギノ丸ゴ, HG丸ｺﾞｼｯｸM-PRO.
    Rounded,
    SansSc,
    SerifSc,
    SansTc,
    SerifTc,
    SansKr,
    SerifKr,
}

impl CjkKind {
    /// The tradition the kind's text is written in.
    pub(crate) fn locale(self) -> CjkLocale {
        match self {
            CjkKind::Gothic | CjkKind::GothicMono | CjkKind::Mincho | CjkKind::MinchoMono | CjkKind::Rounded => CjkLocale::Japanese,
            CjkKind::SansSc | CjkKind::SerifSc => CjkLocale::SimplifiedChinese,
            CjkKind::SansTc | CjkKind::SerifTc => CjkLocale::TraditionalChinese,
            CjkKind::SansKr | CjkKind::SerifKr => CjkLocale::Korean,
        }
    }

    /// A serif (明朝, 宋体, 明體, 바탕) kind?
    pub(crate) fn serif(self) -> bool {
        matches!(self, CjkKind::Mincho | CjkKind::MinchoMono | CjkKind::SerifSc | CjkKind::SerifTc | CjkKind::SerifKr)
    }
}

/// Does `c` belong to a Chinese, Japanese or Korean name: Han, kana (full or half width),
/// Hangul, bopomofo or a full-width form?
fn is_cjk_name_char(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF
        | 0x2E80..=0x2FDF
        | 0x3000..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF)
}

fn has_hangul(s: &str) -> bool {
    s.chars().any(|c| matches!(u32::from(c), 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF))
}

/// The kind of the Chinese, Japanese or Korean font `family` names (English or native name,
/// full- or half-width letters), `None` for other fonts. Any other name written in CJK
/// characters is taken by the system's East Asian language ([`crate::default_cjk_locale`]): on
/// a Japanese system, or one with none, a Japanese gothic, as Word for Mac, which shares
/// Office's font handling, draws every missing CJK-named font in 游ゴシック; on a Chinese or
/// Korean system, a sans of that tradition. A name with kana is a Japanese gothic everywhere.
pub(crate) fn cjk_kind(family: &str) -> Option<CjkKind> {
    cjk_kind_in(family, crate::default_cjk_locale())
}

/// [`cjk_kind`] on a system whose East Asian language is `system`.
fn cjk_kind_in(family: &str, system: Option<CjkLocale>) -> Option<CjkKind> {
    let k = fold_compact(family);
    if k.is_empty() {
        return None;
    }
    let has = |parts: &[&str]| parts.iter().any(|p| k.contains(p));
    let starts = |parts: &[&str]| parts.iter().any(|p| k.starts_with(p));
    // Fixed-pitch Japanese fonts (half-width Latin), before the proportional ones they prefix.
    if matches!(k.as_str(), "msゴシック" | "msgothic" | "ipagothic" | "takaogothic" | "vlgothic" | "bizudgothic" | "osaka-mono" | "osaka-等幅")
    {
        return Some(CjkKind::GothicMono);
    }
    if matches!(k.as_str(), "ms明朝" | "msmincho" | "ipamincho" | "takaomincho" | "bizudmincho") {
        return Some(CjkKind::MinchoMono);
    }
    // Chinese and Korean fonts by name.
    if starts(&[
        "微软雅黑",
        "microsoftyahei",
        "等线",
        "dengxian",
        "黑体",
        "simhei",
        "pingfangsc",
        "苹方",
        "hiraginosansgb",
        "heitisc",
        "stheiti",
        "华文黑体",
    ]) || matches!(k.as_str(), "wenquanyimicrohei" | "wenquanyizenhei")
    {
        return Some(CjkKind::SansSc);
    }
    if starts(&[
        "宋体",
        "新宋体",
        "simsun",
        "nsimsun",
        "仿宋",
        "fangsong",
        "楷体",
        "kaiti",
        "songtisc",
        "stsong",
        "stkaiti",
        "stfangsong",
        "华文宋体",
        "华文楷体",
    ]) {
        return Some(CjkKind::SerifSc);
    }
    if starts(&["微軟正黑體", "microsoftjhenghei", "pingfangtc", "pingfanghk", "heititc"]) {
        return Some(CjkKind::SansTc);
    }
    if starts(&["新細明體", "細明體", "pmingliu", "mingliu", "標楷體", "dfkai-sb", "songtitc", "lisong"]) {
        return Some(CjkKind::SerifTc);
    }
    if starts(&["바탕", "batang", "궁서", "gungsuh", "applemyungjo", "nanummyeongjo", "나눔명조", "unbatang"])
        || (has_hangul(&k) && has(&["명조", "바탕"]))
    {
        return Some(CjkKind::SerifKr);
    }
    if starts(&[
        "맑은고딕",
        "malgungothic",
        "굴림",
        "gulim",
        "돋움",
        "dotum",
        "applesdgothicneo",
        "applegothic",
        "nanumgothic",
        "나눔고딕",
        "undotum",
    ]) || has_hangul(&k)
    {
        return Some(CjkKind::SansKr);
    }
    // Japanese fonts: rounded, then Mincho, then gothic.
    if has(&["丸ゴ", "丸ｺﾞ", "marugothic", "marugo"]) {
        return Some(CjkKind::Rounded);
    }
    if has(&["明朝", "mincho"]) {
        return Some(CjkKind::Mincho);
    }
    let japanese_gothic = has(&["ゴシック", "ｺﾞｼｯｸ", "角ゴ", "メイリオ"])
        || starts(&[
            "yugothic",
            "mspgothic",
            "msuigothic",
            "meiryo",
            "hiraginosans",
            "hiraginokakugothic",
            "kozukagothic",
            "bizudpgothic",
            "ipaexgothic",
            "ipapgothic",
            "takaopgothic",
            "vlpgothic",
            "osaka",
            "udデジタル",
            "uddigikyokasho",
        ])
        || (k.starts_with("hg") && k.contains("gothic"));
    if japanese_gothic || family.chars().any(crate::fallback::is_kana) {
        return Some(CjkKind::Gothic);
    }
    if !family.chars().any(is_cjk_name_char) {
        return None;
    }
    Some(match system {
        None | Some(CjkLocale::Japanese) => CjkKind::Gothic,
        Some(CjkLocale::SimplifiedChinese) => CjkKind::SansSc,
        Some(CjkLocale::TraditionalChinese) => CjkKind::SansTc,
        Some(CjkLocale::Korean) => CjkKind::SansKr,
    })
}

/// Is `family` a Chinese, Japanese or Korean font by its name (an Office or platform CJK font,
/// in English or in its own script, or any name written in CJK characters)? A missing one is
/// drawn in an installed face of its kind ([`crate::FontDb::document_face`]), not in Calibri.
pub fn is_cjk_family(family: &str) -> bool {
    cjk_kind(family).is_some()
}

/// Is `family` a fixed-pitch East Asian font whose Latin letters, digits and punctuation are
/// half an em wide and its kana and kanji a whole em: ＭＳ ゴシック, ＭＳ 明朝, Osaka−等幅, the
/// IPA, Takao and VL fixed-pitch fonts, BIZ UDGothic and BIZ UDMincho, and the Korean 체 fonts
/// (GulimChe, BatangChe…)? Their proportional siblings (ＭＳ Ｐゴシック, ＭＳ Ｐ明朝, Gulim) are
/// not. A stand-in for one of these whose Latin is proportional changes where lines break.
pub fn is_fixed_pitch_cjk_family(family: &str) -> bool {
    let k = fold_compact(family);
    matches!(cjk_kind(family), Some(CjkKind::GothicMono | CjkKind::MinchoMono))
        || matches!(k.as_str(), "gulimche" | "dotumche" | "batangche" | "gungsuhche" | "굴림체" | "돋움체" | "바탕체" | "궁서체")
}

/// Is `c` set half an em wide by a fixed-pitch East Asian font ([`is_fixed_pitch_cjk_family`])?
/// That is how those fonts are drawn: ASCII (the space too), the Latin-1 letters and symbols
/// (é ñ © ¥ £), the en and em dashes and the half-width katakana take half an em; × and ÷, Greek,
/// Cyrillic, the curly quotes and the CJK characters a whole em.
pub fn is_half_width_char(c: char) -> bool {
    matches!(c, '\u{20}'..='\u{7E}' | '\u{A0}'..='\u{FF}' | '\u{2013}' | '\u{2014}' | '\u{FF61}'..='\u{FF9F}') && !matches!(c, '×' | '÷')
}

/// Families of one kind on each platform, and the craft-fonts faces of that kind.
struct Segments {
    mac: &'static [&'static str],
    windows: &'static [&'static str],
    linux: &'static [&'static str],
    bundled: &'static [&'static str],
}

const JA_GOTHIC: Segments = Segments {
    // Hiragino Kaku Gothic ProN before Hiragino Sans: the same design, in two files instead of ten.
    mac: &["Hiragino Kaku Gothic ProN", "Hiragino Sans", "YuGothic"],
    windows: &["Yu Gothic", "Meiryo", "MS PGothic", "BIZ UDPGothic"],
    linux: &["IPAexGothic", "IPAPGothic", "VL PGothic", "TakaoPGothic"],
    bundled: &["BIZ UDPGothic"],
};
const JA_GOTHIC_MONO: Segments =
    Segments { mac: &["Osaka-Mono"], windows: &["MS Gothic", "BIZ UDGothic"], linux: &["IPAGothic", "TakaoGothic", "VL Gothic"], bundled: &[] };
const JA_MINCHO: Segments = Segments {
    mac: &["Hiragino Mincho ProN", "YuMincho"],
    windows: &["Yu Mincho", "MS PMincho", "BIZ UDPMincho"],
    linux: &["IPAexMincho", "IPAPMincho", "TakaoPMincho"],
    bundled: &["Shippori Mincho", "BIZ UDMincho"],
};
const JA_MINCHO_MONO: Segments =
    Segments { mac: &[], windows: &["MS Mincho", "BIZ UDMincho"], linux: &["IPAMincho", "TakaoMincho"], bundled: &["BIZ UDMincho"] };
const JA_ROUNDED: Segments = Segments { mac: &["Hiragino Maru Gothic ProN"], windows: &["HGMaruGothicMPRO"], linux: &[], bundled: &[] };
const SANS_SC: Segments = Segments {
    mac: &["PingFang SC", "Hiragino Sans GB", "Heiti SC", "STHeiti"],
    windows: &["Microsoft YaHei", "DengXian", "SimHei"],
    linux: &["WenQuanYi Micro Hei", "WenQuanYi Zen Hei"],
    bundled: &[],
};
const SERIF_SC: Segments = Segments { mac: &["Songti SC", "STSong"], windows: &["SimSun", "NSimSun"], linux: &["AR PL UMing CN"], bundled: &[] };
const SANS_TC: Segments =
    Segments { mac: &["PingFang TC", "Heiti TC", "PingFang HK"], windows: &["Microsoft JhengHei"], linux: &["WenQuanYi Zen Hei"], bundled: &[] };
const SERIF_TC: Segments =
    Segments { mac: &["Songti TC", "LiSong Pro"], windows: &["PMingLiU", "MingLiU"], linux: &["AR PL UMing TW"], bundled: &[] };
const SANS_KR: Segments = Segments {
    mac: &["Apple SD Gothic Neo", "AppleGothic"],
    windows: &["Malgun Gothic", "Gulim", "Dotum"],
    linux: &["NanumGothic", "UnDotum"],
    bundled: &[],
};
const SERIF_KR: Segments = Segments { mac: &["AppleMyungjo"], windows: &["Batang", "Gungsuh"], linux: &["NanumMyeongjo", "UnBatang"], bundled: &[] };
/// The interface's own faces, for text the interface fonts can't draw: Yu Gothic UI and Meiryo UI
/// as Japanese Windows shows its menus, Hiragino Sans as macOS does.
const UI_JA: Segments = Segments {
    mac: &["Hiragino Sans", "Hiragino Kaku Gothic ProN"],
    windows: &["Yu Gothic UI", "Meiryo UI", "Meiryo", "Yu Gothic", "MS UI Gothic"],
    linux: &["IPAexGothic", "IPAPGothic", "VL PGothic", "TakaoPGothic"],
    bundled: &["BIZ UDPGothic"],
};
const UI_SC: Segments = Segments {
    mac: &["PingFang SC", "Hiragino Sans GB"],
    windows: &["Microsoft YaHei UI", "Microsoft YaHei", "DengXian"],
    linux: &["WenQuanYi Micro Hei", "WenQuanYi Zen Hei"],
    bundled: &[],
};
const UI_TC: Segments = Segments {
    mac: &["PingFang TC", "PingFang HK"],
    windows: &["Microsoft JhengHei UI", "Microsoft JhengHei"],
    linux: &["WenQuanYi Zen Hei"],
    bundled: &[],
};
const UI_KR: Segments = Segments { mac: &["Apple SD Gothic Neo"], windows: &["Malgun Gothic"], linux: &["NanumGothic"], bundled: &[] };

/// The running platform's segment first, then the craft-fonts faces, then the other platforms'.
fn platform_order(s: &Segments, out: &mut Vec<&'static str>) {
    let (own, others): (&[&str], [&[&str]; 2]) = if cfg!(target_os = "macos") {
        (s.mac, [s.windows, s.linux])
    } else if cfg!(windows) {
        (s.windows, [s.mac, s.linux])
    } else {
        (s.linux, [s.mac, s.windows])
    };
    let mut push = |names: &[&'static str]| {
        for n in names {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                out.push(n);
            }
        }
    };
    push(own);
    push(s.bundled);
    for o in others {
        push(o);
    }
}

/// The segments a kind's stand-ins are taken from, best first.
fn kind_segments(kind: CjkKind) -> &'static [&'static Segments] {
    match kind {
        CjkKind::Gothic => &[&JA_GOTHIC],
        CjkKind::GothicMono => &[&JA_GOTHIC_MONO, &JA_GOTHIC],
        CjkKind::Mincho => &[&JA_MINCHO],
        CjkKind::MinchoMono => &[&JA_MINCHO_MONO, &JA_MINCHO],
        CjkKind::Rounded => &[&JA_ROUNDED, &JA_GOTHIC],
        CjkKind::SansSc => &[&SANS_SC],
        CjkKind::SerifSc => &[&SERIF_SC, &SANS_SC],
        CjkKind::SansTc => &[&SANS_TC],
        CjkKind::SerifTc => &[&SERIF_TC, &SANS_TC],
        CjkKind::SansKr => &[&SANS_KR],
        CjkKind::SerifKr => &[&SERIF_KR, &SANS_KR],
    }
}

/// The craft-fonts families for a tradition (`Jpan`, `Hans`, `Hant`, `Hang`/`Kore`) of the
/// wanted genre (Mincho/serif or gothic/sans), in manifest order.
fn bundled_of(locale: CjkLocale, serif: bool) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for f in crate::CRAFT_FONTS.iter().filter(|f| f.scripts.iter().any(|s| tradition_of_script(s) == Some(locale))) {
        if is_serif_name(f.family) == serif && !v.contains(&f.family) {
            v.push(f.family);
        }
    }
    v
}

/// The East Asian tradition an ISO 15924 script tag of a craft-fonts catalog row stands for
/// (`Hani`, shared by the Chinese traditions and Japanese, stands for none).
fn tradition_of_script(tag: &str) -> Option<CjkLocale> {
    match tag {
        "Jpan" => Some(CjkLocale::Japanese),
        "Hans" => Some(CjkLocale::SimplifiedChinese),
        "Hant" => Some(CjkLocale::TraditionalChinese),
        "Hang" | "Kore" => Some(CjkLocale::Korean),
        _ => None,
    }
}

/// The one tradition a craft-fonts family's catalog rows name (LXGW WenKai Lite: `Hani,Hans`);
/// `None` for a face of no East Asian tradition or of several.
fn bundled_tradition(family: &str) -> Option<CjkLocale> {
    let mut traditions =
        crate::CRAFT_FONTS.iter().filter(|f| f.family == family).flat_map(|f| f.scripts.iter().filter_map(|s| tradition_of_script(s)));
    let first = traditions.next()?;
    traditions.all(|t| t == first).then_some(first)
}

/// Does a family name say serif: Mincho, 明朝, Serif (not Sans Serif), Song, Ming, Myeongjo,
/// Batang?
pub(crate) fn is_serif_name(family: &str) -> bool {
    let k = fold_compact(family);
    (k.contains("serif") && !k.contains("sansserif"))
        || ["mincho", "明朝", "songti", "stsong", "simsun", "宋", "mingliu", "明體", "myeongjo", "myungjo", "batang", "바탕", "명조"]
            .iter()
            .any(|p| k.contains(p))
}

/// The installed (or craft-fonts) families standing in for a missing CJK `family`, best first;
/// `None` for a name that isn't a CJK font's.
pub(crate) fn cjk_substitutes(family: &str) -> Option<&'static [&'static str]> {
    cjk_kind(family).map(cjk_substitutes_of)
}

/// The installed (or craft-fonts) families standing in for a missing font of `kind`, best first.
fn cjk_substitutes_of(kind: CjkKind) -> &'static [&'static str] {
    static CHAINS: OnceLock<Vec<(CjkKind, Vec<&'static str>)>> = OnceLock::new();
    let chains = CHAINS.get_or_init(|| {
        let kinds = [
            CjkKind::Gothic,
            CjkKind::GothicMono,
            CjkKind::Mincho,
            CjkKind::MinchoMono,
            CjkKind::Rounded,
            CjkKind::SansSc,
            CjkKind::SerifSc,
            CjkKind::SansTc,
            CjkKind::SerifTc,
            CjkKind::SansKr,
            CjkKind::SerifKr,
        ];
        kinds
            .iter()
            .map(|&k| {
                let mut v = Vec::new();
                for s in kind_segments(k) {
                    platform_order(s, &mut v);
                }
                for b in bundled_of(k.locale(), k.serif()) {
                    if !v.contains(&b) {
                        v.push(b);
                    }
                }
                (k, v)
            })
            .collect()
    });
    chains.iter().find(|(k, _)| *k == kind).map(|(_, v)| v.as_slice()).unwrap_or(&[])
}

/// The families East Asian text of tradition `locale` falls back to when the run's fonts lack a
/// character, in order: the craft-fonts faces of the requested genre (Mincho when the requested
/// face is a serif one, else gothic), then the installed families of that genre, then the other
/// genre's. The same on every machine where craft-fonts is built in.
pub(crate) fn fallback_families(locale: CjkLocale, serif: bool) -> &'static [&'static str] {
    static CHAINS: OnceLock<Vec<((CjkLocale, bool), Vec<&'static str>)>> = OnceLock::new();
    let chains = CHAINS.get_or_init(|| {
        let mut out = Vec::new();
        for l in [CjkLocale::Japanese, CjkLocale::SimplifiedChinese, CjkLocale::TraditionalChinese, CjkLocale::Korean] {
            for s in [false, true] {
                let (sans, serif_segs): (&Segments, &Segments) = match l {
                    CjkLocale::Japanese => (&JA_GOTHIC, &JA_MINCHO),
                    CjkLocale::SimplifiedChinese => (&SANS_SC, &SERIF_SC),
                    CjkLocale::TraditionalChinese => (&SANS_TC, &SERIF_TC),
                    CjkLocale::Korean => (&SANS_KR, &SERIF_KR),
                };
                let (first, second) = if s { (serif_segs, sans) } else { (sans, serif_segs) };
                let mut v: Vec<&'static str> = Vec::new();
                let push = |names: Vec<&'static str>, v: &mut Vec<&'static str>| {
                    for n in names {
                        if !v.contains(&n) {
                            v.push(n);
                        }
                    }
                };
                let mut bundled = first.bundled.to_vec();
                bundled.extend(bundled_of(l, s));
                push(bundled, &mut v);
                let mut installed = Vec::new();
                platform_order(&Segments { bundled: &[], ..*first }, &mut installed);
                push(installed, &mut v);
                let mut bundled = second.bundled.to_vec();
                bundled.extend(bundled_of(l, !s));
                push(bundled, &mut v);
                let mut installed = Vec::new();
                platform_order(&Segments { bundled: &[], ..*second }, &mut installed);
                push(installed, &mut v);
                out.push(((l, s), v));
            }
        }
        out
    });
    chains.iter().find(|(k, _)| *k == (locale, serif)).map(|(_, v)| v.as_slice()).unwrap_or(&[])
}

/// The interface's families for East Asian text of tradition `locale`, best first.
pub(crate) fn interface_families(locale: CjkLocale) -> &'static [&'static str] {
    static CHAINS: OnceLock<Vec<(CjkLocale, Vec<&'static str>)>> = OnceLock::new();
    let chains = CHAINS.get_or_init(|| {
        [(CjkLocale::Japanese, &UI_JA), (CjkLocale::SimplifiedChinese, &UI_SC), (CjkLocale::TraditionalChinese, &UI_TC), (CjkLocale::Korean, &UI_KR)]
            .into_iter()
            .map(|(l, s)| {
                let mut v = Vec::new();
                platform_order(s, &mut v);
                (l, v)
            })
            .collect()
    });
    chains.iter().find(|(l, _)| *l == locale).map(|(_, v)| v.as_slice()).unwrap_or(&[])
}

/// The platform's own interface families for East Asian text of tradition `locale`, best first:
/// Hiragino on macOS and Yu Gothic UI on Windows for Japanese. None elsewhere: on Linux and the web
/// the bundled face draws the interface.
pub(crate) fn platform_interface_families(locale: CjkLocale) -> &'static [&'static str] {
    let s = match locale {
        CjkLocale::Japanese => &UI_JA,
        CjkLocale::SimplifiedChinese => &UI_SC,
        CjkLocale::TraditionalChinese => &UI_TC,
        CjkLocale::Korean => &UI_KR,
    };
    if cfg!(target_os = "macos") {
        s.mac
    } else if cfg!(windows) {
        s.windows
    } else {
        &[]
    }
}

/// The tradition a family is a font for, by its name: one of the stand-in or fallback families
/// above or of the system fallback list, a CJK-named font, or a name ending in a region (`… JP`,
/// `… SC`, `… GB`, `… TC`, `… HK`, `… KR`). `None` for other fonts (and pan-CJK ones).
pub(crate) fn tradition_of_family(family: &str) -> Option<CjkLocale> {
    static KNOWN: OnceLock<std::collections::HashMap<String, CjkLocale>> = OnceLock::new();
    let known = KNOWN.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        let all: [(&Segments, CjkLocale); 15] = [
            (&JA_GOTHIC, CjkLocale::Japanese),
            (&JA_GOTHIC_MONO, CjkLocale::Japanese),
            (&JA_MINCHO, CjkLocale::Japanese),
            (&JA_MINCHO_MONO, CjkLocale::Japanese),
            (&JA_ROUNDED, CjkLocale::Japanese),
            (&UI_JA, CjkLocale::Japanese),
            (&SANS_SC, CjkLocale::SimplifiedChinese),
            (&SERIF_SC, CjkLocale::SimplifiedChinese),
            (&UI_SC, CjkLocale::SimplifiedChinese),
            (&SANS_TC, CjkLocale::TraditionalChinese),
            (&SERIF_TC, CjkLocale::TraditionalChinese),
            (&UI_TC, CjkLocale::TraditionalChinese),
            (&SANS_KR, CjkLocale::Korean),
            (&SERIF_KR, CjkLocale::Korean),
            (&UI_KR, CjkLocale::Korean),
        ];
        for (s, l) in all {
            for n in s.mac.iter().chain(s.windows).chain(s.linux).chain(s.bundled) {
                // WenQuanYi Zen Hei covers both Chinese traditions; the first listing wins.
                m.entry(fold_compact(n)).or_insert(l);
            }
        }
        // The craft-fonts faces by the tradition their catalog names, as [`bundled_of`] offers
        // them as stand-ins (a Chinese face is never taken for a Japanese one).
        for f in crate::CRAFT_FONTS.iter() {
            if let Some(l) = bundled_tradition(f.family) {
                m.entry(fold_compact(f.family)).or_insert(l);
            }
        }
        m
    });
    let k = fold_compact(family);
    if let Some(l) = known.get(&k) {
        return Some(*l);
    }
    let lower = family.trim().to_ascii_lowercase();
    if lower.ends_with(" jp") || lower.ends_with(" japanese") {
        return Some(CjkLocale::Japanese);
    }
    if lower.ends_with(" sc") || lower.ends_with(" gb") || lower.ends_with(" cn") {
        return Some(CjkLocale::SimplifiedChinese);
    }
    if lower.ends_with(" tc") || lower.ends_with(" hk") || lower.ends_with(" tw") {
        return Some(CjkLocale::TraditionalChinese);
    }
    if lower.ends_with(" kr") {
        return Some(CjkLocale::Korean);
    }
    cjk_kind(family).filter(|_| family.chars().any(is_cjk_name_char)).map(CjkKind::locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_font_names_are_classified_by_kind() {
        for (name, kind) in [
            ("游ゴシック", CjkKind::Gothic),
            ("游ゴシック Light", CjkKind::Gothic),
            ("Yu Gothic UI", CjkKind::Gothic),
            ("ＭＳ Ｐゴシック", CjkKind::Gothic),
            ("MS PGothic", CjkKind::Gothic),
            ("MS UI Gothic", CjkKind::Gothic),
            ("メイリオ", CjkKind::Gothic),
            ("Meiryo UI", CjkKind::Gothic),
            ("BIZ UDPGothic", CjkKind::Gothic),
            ("HGPｺﾞｼｯｸE", CjkKind::Gothic),
            ("HGSGothicM", CjkKind::Gothic),
            ("ヒラギノ角ゴ ProN W3", CjkKind::Gothic),
            ("ヒラギノ角ゴシック W6", CjkKind::Gothic),
            ("小塚ゴシック Pr6N R", CjkKind::Gothic),
            ("HG創英角ﾎﾟｯﾌﾟ体", CjkKind::Gothic),
            ("UD デジタル 教科書体 N-R", CjkKind::Gothic),
            ("ＭＳ ゴシック", CjkKind::GothicMono),
            ("MS Gothic", CjkKind::GothicMono),
            ("Osaka−等幅", CjkKind::GothicMono),
            ("ＭＳ 明朝", CjkKind::MinchoMono),
            ("MS Mincho", CjkKind::MinchoMono),
            ("ＭＳ Ｐ明朝", CjkKind::Mincho),
            ("游明朝 Demibold", CjkKind::Mincho),
            ("Yu Mincho", CjkKind::Mincho),
            ("HG明朝E", CjkKind::Mincho),
            ("ヒラギノ明朝 ProN W3", CjkKind::Mincho),
            ("小塚明朝 Pr6N R", CjkKind::Mincho),
            ("ヒラギノ丸ゴ ProN W4", CjkKind::Rounded),
            ("HG丸ｺﾞｼｯｸM-PRO", CjkKind::Rounded),
            ("微软雅黑", CjkKind::SansSc),
            ("等线 Light", CjkKind::SansSc),
            ("宋体", CjkKind::SerifSc),
            ("新細明體", CjkKind::SerifTc),
            ("微軟正黑體", CjkKind::SansTc),
            ("맑은 고딕", CjkKind::SansKr),
            ("바탕", CjkKind::SerifKr),
        ] {
            assert_eq!(cjk_kind(name), Some(kind), "{name}");
        }
        // Latin fonts named Gothic are not Japanese.
        for latin in ["Century Gothic", "Franklin Gothic Medium", "Copperplate Gothic Bold", "News Gothic MT", "Calibri", "Arial", "Café Sans", ""] {
            assert_eq!(cjk_kind(latin), None, "{latin}");
        }
        assert!(is_cjk_family("ＭＳ Ｐゴシック") && is_cjk_family("Malgun Gothic") && !is_cjk_family("Gill Sans"));
    }

    /// A CJK-named font no list knows is a Japanese gothic on a Japanese system (or one with no
    /// East Asian language), as Word for Mac draws it, and a sans of the system's tradition on a
    /// Chinese or Korean one; a name with kana is Japanese everywhere.
    #[test]
    fn other_cjk_named_fonts_follow_the_systems_east_asian_language() {
        use CjkLocale::*;
        for (system, kind) in [
            (None, CjkKind::Gothic),
            (Some(Japanese), CjkKind::Gothic),
            (Some(SimplifiedChinese), CjkKind::SansSc),
            (Some(TraditionalChinese), CjkKind::SansTc),
            (Some(Korean), CjkKind::SansKr),
        ] {
            for name in ["幼圆", "方正兰亭黑简体", "華康儷黑", "ＤＣ　ＡＢＣ"] {
                assert_eq!(cjk_kind_in(name, system), Some(kind), "{name} on {system:?}");
            }
            for japanese in ["HG創英角ﾎﾟｯﾌﾟ体", "デッキ大阪", "ヒラギノ角ゴ ProN W3", "游明朝"] {
                assert_eq!(cjk_kind_in(japanese, system).map(CjkKind::locale), Some(Japanese), "{japanese} on {system:?}");
            }
            // Fonts the lists know keep their kind, and Latin names stay Latin.
            assert_eq!(cjk_kind_in("宋体", system), Some(CjkKind::SerifSc));
            assert_eq!(cjk_kind_in("맑은 고딕", system), Some(CjkKind::SansKr));
            assert_eq!(cjk_kind_in("Century Gothic", system), None);
        }
        let stand_ins = cjk_substitutes_of(CjkKind::SansSc);
        assert!(stand_ins.iter().all(|f| tradition_of_family(f) == Some(SimplifiedChinese)), "{stand_ins:?}");
    }

    #[test]
    fn fixed_pitch_fonts_are_told_from_their_proportional_siblings() {
        for fixed in ["ＭＳ ゴシック", "MS Gothic", "ＭＳ 明朝", "MS Mincho", "Osaka−等幅", "BIZ UDGothic", "IPAGothic", "GulimChe", "바탕체"]
        {
            assert!(is_fixed_pitch_cjk_family(fixed), "{fixed}");
        }
        for proportional in ["ＭＳ Ｐゴシック", "MS PMincho", "MS UI Gothic", "IPAPGothic", "IPAexGothic", "Gulim", "Osaka", "BIZ UDPGothic"] {
            assert!(!is_fixed_pitch_cjk_family(proportional), "{proportional}");
        }
        for half in ['A', ' ', '~', 'é', '©', '¥', '–', '—', 'ｱ', 'ﾟ'] {
            assert!(is_half_width_char(half), "{half:?}");
        }
        for whole in ['×', '÷', 'α', 'Ж', '“', '’', 'あ', '漢', 'Ａ', '　'] {
            assert!(!is_half_width_char(whole), "{whole:?}");
        }
    }

    #[test]
    fn stand_ins_keep_the_kind_and_put_the_platform_first() {
        let gothic = cjk_substitutes("游ゴシック").unwrap();
        let mincho = cjk_substitutes("ＭＳ Ｐ明朝").unwrap();
        let mono = cjk_substitutes("ＭＳ ゴシック").unwrap();
        let rounded = cjk_substitutes("HG丸ｺﾞｼｯｸM-PRO").unwrap();
        let own = if cfg!(target_os = "macos") {
            ("Hiragino Kaku Gothic ProN", "Hiragino Mincho ProN", "Osaka-Mono")
        } else if cfg!(windows) {
            ("Yu Gothic", "Yu Mincho", "MS Gothic")
        } else {
            ("IPAexGothic", "IPAexMincho", "IPAGothic")
        };
        assert_eq!((gothic[0], mincho[0], mono[0]), own);
        // Every chain holds the craft-fonts face of its kind and the other platforms' fonts.
        assert!(gothic.contains(&"BIZ UDPGothic") && gothic.contains(&"IPAexGothic") && gothic.contains(&"Hiragino Sans"));
        assert!(mincho.contains(&"Shippori Mincho") && mincho.contains(&"BIZ UDMincho") && !mincho.contains(&"BIZ UDPGothic"));
        // A fixed-pitch font's stand-ins: fixed-pitch faces, then proportional gothics.
        let first_proportional = mono.iter().position(|f| *f == "IPAexGothic").unwrap();
        assert!(mono.iter().position(|f| *f == "IPAGothic").unwrap() < first_proportional);
        assert_eq!(rounded.first(), Some(&if cfg!(windows) { "HGMaruGothicMPRO" } else { "Hiragino Maru Gothic ProN" }));
        assert!(rounded.contains(&"IPAexGothic"), "then the gothic chain");
        assert!(cjk_substitutes("Calibri").is_none());
        // No Adobe-designed family is ever named (AGENTS.md §1.1).
        for chain in [gothic, mincho, mono, rounded, cjk_substitutes("宋体").unwrap(), fallback_families(CjkLocale::Korean, false)] {
            assert!(chain.iter().all(|f| !f.contains("Source Han") && !f.contains("Noto Sans CJK") && !f.contains("Noto Serif CJK")), "{chain:?}");
        }
    }

    #[test]
    fn fallback_chains_follow_the_tradition_and_genre() {
        let ja = fallback_families(CjkLocale::Japanese, false);
        assert_eq!(ja.first(), Some(&"BIZ UDPGothic"), "the craft-fonts gothic first");
        let ja_serif = fallback_families(CjkLocale::Japanese, true);
        assert_eq!(&ja_serif[..2], ["Shippori Mincho", "BIZ UDMincho"]);
        let pos = |chain: &[&str], f: &str| chain.iter().position(|x| *x == f).unwrap();
        assert!(pos(ja, "IPAexGothic") < pos(ja, "IPAexMincho"));
        assert!(pos(ja_serif, "IPAexMincho") < pos(ja_serif, "IPAexGothic"));
        assert!(!ja.contains(&"PingFang SC") && fallback_families(CjkLocale::SimplifiedChinese, false).contains(&"PingFang SC"));
        assert!(fallback_families(CjkLocale::TraditionalChinese, false).contains(&"PingFang TC"));
        assert!(fallback_families(CjkLocale::Korean, false).contains(&"Apple SD Gothic Neo"));
        assert_eq!(tradition_of_family("Hiragino Sans"), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_family("PingFang SC"), Some(CjkLocale::SimplifiedChinese));
        assert_eq!(tradition_of_family("Noto Sans CJK JP"), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_family("Apple SD Gothic Neo"), Some(CjkLocale::Korean));
        assert_eq!(tradition_of_family("ＭＳ Ｐゴシック"), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_family("Arial Unicode MS"), None);
        assert_eq!(tradition_of_family("Inter"), None);
        // A craft-fonts face is of the tradition its catalog row names.
        for f in crate::CRAFT_FONTS.iter() {
            if let Some(l) = bundled_tradition(f.family) {
                assert_eq!(tradition_of_family(f.family), Some(l), "{}", f.family);
            }
        }
        assert_eq!(tradition_of_script("Hans"), Some(CjkLocale::SimplifiedChinese));
        assert_eq!(tradition_of_script("Hani"), None);
        assert_eq!(tradition_of_script("Latn"), None);
        let ui = interface_families(CjkLocale::Japanese);
        assert_eq!(
            ui.first(),
            Some(&if cfg!(target_os = "macos") {
                "Hiragino Sans"
            } else if cfg!(windows) {
                "Yu Gothic UI"
            } else {
                "IPAexGothic"
            })
        );
        // The interface's own face: the platform's, never the bundled one or another platform's.
        let own = platform_interface_families(CjkLocale::Japanese);
        assert!(!own.contains(&"BIZ UDPGothic"));
        if cfg!(target_os = "macos") {
            assert_eq!(own, ["Hiragino Sans", "Hiragino Kaku Gothic ProN"]);
        } else if cfg!(windows) {
            assert_eq!(own.first(), Some(&"Yu Gothic UI"));
        } else {
            assert!(own.is_empty(), "Linux and the web draw the interface in the bundled face");
        }
        assert!(is_serif_name("Hiragino Mincho ProN") && is_serif_name("游明朝") && is_serif_name("PMingLiU") && is_serif_name("Noto Serif"));
        assert!(!is_serif_name("BIZ UDPGothic") && !is_serif_name("Microsoft Sans Serif") && !is_serif_name("Coming Soon"));
    }
}
