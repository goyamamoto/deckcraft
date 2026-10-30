use super::run::{RunEffects, Underline};
use super::shape::run_effects;
use super::*;

/// Values for fields (`slidenum`, `datetime…`, `footer`). `None` keeps the field's saved text.
pub trait Fields {
    fn field(&self, kind: &str) -> Option<String>;
    /// The value of field `kind` in a run of language `lang` (BCP 47, e.g. `en-US`): dates are
    /// written the language's way.
    fn field_in(&self, kind: &str, lang: Option<&str>) -> Option<String> {
        let _ = lang;
        self.field(kind)
    }
}

pub struct NoFields;

impl Fields for NoFields {
    fn field(&self, kind: &str) -> Option<String> {
        if kind == "slidenum" { Some("1".into()) } else { None }
    }
}

pub struct Opts<'a> {
    /// The text box: the preset's text rectangle in shape-local space.
    pub rect: Rect,
    pub fields: &'a dyn Fields,
    /// Draw this text instead of the body (placeholder prompts), in this colour.
    pub prompt_color: Option<Rgba>,
    /// Ignore autofit shrinking (used to measure natural height).
    pub no_shrink: bool,
}

pub(crate) struct Style {
    pub(crate) face: Arc<FontFace>,
    pub(crate) size: f64,
    pub(crate) color: Rgba,
    pub(crate) alpha: f64,
    /// Bold and italic as the run asks for them; each glyph run is emboldened or slanted when its
    /// own face lacks the style ([`synthetic_style`]).
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) baseline: f64,
    pub(crate) spacing: f64,
    pub(crate) caps: Caps,
    pub(crate) underline: Option<(Rgba, Underline)>,
    pub(crate) strike: Option<Strike>,
    pub(crate) highlight: Option<Rgba>,
    pub(crate) outline: Option<(Rgba, f64)>,
    pub(crate) link: bool,
    pub(crate) gradient: Option<deckcraft_model::style::Gradient>,
    pub(crate) effects: Option<RunEffects>,
    /// "Regular", "Bold"… for the slot and fallback faces.
    pub(crate) style_name: &'static str,
    /// BCP 47 language (`a:rPr/@lang`), for shaping.
    pub(crate) lang: Option<String>,
    /// The East Asian tradition Han characters fall back in ([`run_traditions`]).
    pub(crate) cjk: Option<CjkLocale>,
    /// The `a:ea` and `a:cs` families (theme references resolved), if the document names one. An
    /// empty `a:ea` takes the theme's font for the run's East Asian language ([`run_traditions`]),
    /// as PowerPoint draws Japanese text in the theme's `Jpan` font.
    pub(crate) ea: Option<String>,
    pub(crate) cs: Option<String>,
    /// The `a:sym` family: private-use characters (U+F000–U+F0FF, as PowerPoint stores symbol-font
    /// characters) are drawn in it.
    pub(crate) sym: Option<String>,
    /// Heading font (`+mj-…`): the theme's major font set applies.
    major: bool,
    /// Pair kerning on: `a:rPr/@kern` is set and the size reaches it.
    pub(crate) kern: bool,
}

/// One character of a paragraph after shaping.
#[derive(Clone)]
pub(crate) struct Cell {
    pub(crate) ch: char,
    pub(crate) style: usize,
    pub(crate) face: usize,
    /// Resolved bidi embedding level (before the line rules).
    pub(crate) level: u8,
    /// Glyphs of the shaping cluster this cell heads, from the cluster's visual left edge (gid,
    /// x, y). Empty for the other cells of a cluster.
    pub(crate) glyphs: Vec<(u32, f64, f64)>,
    /// This character's advance: its share of the cluster (ligature carets); 0 inside a grapheme.
    pub(crate) adv: f64,
    /// Index of the cell heading this character's shaping cluster.
    pub(crate) head: usize,
    /// A grapheme starts here: carets stand and lines break only before such cells.
    pub(crate) boundary: bool,
    /// Drawn upright in East Asian vertical text.
    pub(crate) upright: bool,
    /// The equation (`ParaShaped::objects`) this character is part of: the cluster's head cell
    /// draws it, the others have no width.
    pub(crate) object: Option<usize>,
}

impl Cell {
    /// A line may start before this cell: a grapheme that starts a shaping cluster.
    pub(crate) fn breakable(&self, i: usize) -> bool {
        self.boundary && self.head == i
    }
}

pub(crate) struct ParaShaped {
    pub(crate) cells: Vec<Cell>,
    pub(crate) styles: Vec<Style>,
    pub(crate) faces: Vec<Arc<FontFace>>,
    pub(crate) props: ParaProps,
    /// Size of the paragraph's first run (spacing percentages, empty lines).
    pub(crate) lead_size: f64,
    pub(crate) lead_face: Arc<FontFace>,
    /// Size of the paragraph mark (`a:endParaRPr`): what an empty last line is sized by.
    pub(crate) end_size: f64,
    pub(crate) end_face: Arc<FontFace>,
    pub(crate) bullet: Option<(String, Style)>,
    #[allow(dead_code)]
    pub(crate) level: u8,
    /// Right-to-left paragraph (`a:pPr/@rtl`).
    pub(crate) rtl: bool,
    /// Equations laid out as one inline object each (`RunKind::Math`).
    pub(crate) objects: Vec<MathObject>,
}

/// An equation laid out at its run's size and colour.
pub(crate) struct MathObject {
    pub(crate) layout: math::Layout,
    /// The faces the layout's glyphs index.
    pub(crate) faces: Vec<Arc<FontFace>>,
}

/// [`math::MathFont`] over the font database: the run's family in each style, other faces for
/// the characters it lacks.
pub(crate) struct MathFaces {
    db: &'static FontDb,
    family: String,
    base: Arc<FontFace>,
    pub(crate) faces: Vec<Arc<FontFace>>,
    cache: HashMap<(char, bool, bool), Option<math::Glyph>>,
}

impl MathFaces {
    pub(crate) fn new(base: Arc<FontFace>) -> Self {
        let db = FontDb::global();
        // PowerPoint's math font names "the math font": equations are drawn in Computer Modern
        // (TeX's, CMU Serif) when craft-fonts bundles it, even where Cambria Math is installed.
        let base = if base.answers_to("Cambria Math") { db.available_face("CMU Serif", "Regular").unwrap_or(base) } else { base };
        MathFaces { db, family: base.family.clone(), base, faces: Vec::new(), cache: HashMap::new() }
    }

    fn index(&mut self, face: &Arc<FontFace>) -> u32 {
        if let Some(i) = self.faces.iter().position(|f| Arc::ptr_eq(f, face)) {
            return i as u32;
        }
        self.faces.push(face.clone());
        (self.faces.len() - 1) as u32
    }
}

impl math::MathFont for MathFaces {
    fn glyph(&mut self, c: char, bold: bool, italic: bool) -> Option<math::Glyph> {
        if let Some(g) = self.cache.get(&(c, bold, italic)) {
            return *g;
        }
        let style = style_name(bold, italic);
        let own = if !bold && !italic { self.base.clone() } else { self.db.document_face(&self.family, style) };
        let face = if own.covers(c) {
            Some(own)
        } else if self.base.covers(c) {
            Some(self.base.clone())
        } else {
            // A symbol the run's family lacks: from a math font (the bundled STIX Two Math, or
            // one installed), so operators and relations match each other, before any face.
            MATH_SYMBOL_FONTS
                .iter()
                .find_map(|f| self.db.available_face(f, "Regular").filter(|f| f.covers(c)))
                .or_else(|| self.db.fallback_styled(c, own.id(), style, None).filter(|f| f.covers(c)))
        };
        let g = face.and_then(|face| {
            let gid = face.glyph_for(c);
            if gid == 0 {
                return None;
            }
            let upem = face.upem.max(1.0);
            let outline = self.db.outline(&face, gid);
            let ink = if outline.elements().is_empty() { Rect::ZERO } else { outline.bounding_box() };
            let ink = Rect::new(ink.x0 / upem, ink.y0 / upem, ink.x1 / upem, ink.y1 / upem);
            let (fake_bold, fake_italic) = synthetic_style(bold, italic, &face);
            let index = self.index(&face);
            Some(math::Glyph { face: index, id: gid, advance: face.advance(gid) / upem, ink, fake_bold, fake_italic })
        });
        self.cache.insert((c, bold, italic), g);
        g
    }

    fn outline(&mut self, g: &math::Glyph) -> Option<BezPath> {
        let face = self.faces.get(g.face as usize)?;
        let upem = face.upem.max(1.0);
        let mut p = (*self.db.outline(face, g.id)).clone();
        if p.elements().is_empty() {
            return None;
        }
        p.apply_affine(Affine::scale(1.0 / upem));
        Some(p)
    }

    fn x_height(&self) -> f64 {
        let v = self.base.x_height / self.base.upem.max(1.0);
        if v.is_finite() && v > 0.0 { v.clamp(0.3, 0.7) } else { 0.45 }
    }

    fn rule_thickness(&self) -> f64 {
        // TeX's fraction bar is 0.04 em; a face's underline is often heavier (CMU Serif 0.05).
        let v = self.base.underline.1 / self.base.upem.max(1.0);
        if v.is_finite() && v > 0.0 { v.clamp(0.035, 0.045) } else { 0.04 }
    }
}

/// Math fonts tried, in order, for a symbol the equation's family lacks.
const MATH_SYMBOL_FONTS: &[&str] =
    &["STIX Two Math", "Latin Modern Math", "Cambria Math", "XITS Math", "Libertinus Math", "Noto Sans Math", "STIX Math"];

fn style_name(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    }
}

/// Whether a glyph run of a style asking for `bold`/`italic` drawn in `face` is emboldened or
/// slanted: PowerPoint does so for a face without the style itself, decided for each face that
/// draws the text, so bold kanji drawn in a Japanese face with no bold come out heavier beside the
/// real bold of the Latin face, and a Japanese face with a real bold is not emboldened again.
pub(crate) fn synthetic_style(bold: bool, italic: bool, face: &FontFace) -> (bool, bool) {
    (bold && face.weight < 550.0, italic && !face.italic)
}

/// The East Asian traditions of a run with `lang` and `alt_lang`, where the system's East Asian
/// language is `default` and kana or Hangul in the paragraph tell `text`
/// ([`script::tradition_of_text`]): (the tradition its Han characters fall back in, the
/// tradition whose theme font (`a:font script="Jpan"`, `Hans`, `Hant`, `Hang`) fills an empty
/// `a:ea`).
///
/// PowerPoint for Mac draws a run in the theme's `Jpan` font when its `lang` or its `altLang` is
/// `ja-JP`, and Chinese and Korean runs follow their own tradition the same way. For Chinese that
/// is a deliberate difference: on a Japanese system PowerPoint draws a `zh-CN` run's Han in its
/// Japanese fallback face (MS Gothic) and leaves the theme's `Hans` font unused, so the deck's
/// Chinese looks different on each system; DeckCraft draws it in Chinese forms and the font the
/// theme names for Chinese wherever it is opened (and × ° then take that font too). Korean runs
/// were not measured. An `en-US` run's kanji don't take the theme font; they fall back to a
/// Japanese face on a Japanese system. A run with no language at all takes the system's East
/// Asian language, else the one its paragraph's kana or Hangul tell.
pub(crate) fn run_traditions(
    lang: Option<&str>,
    alt_lang: Option<&str>,
    default: Option<CjkLocale>,
    text: Option<CjkLocale>,
) -> (Option<CjkLocale>, Option<CjkLocale>) {
    let lang = lang.filter(|l| !l.trim().is_empty());
    let own = [lang, alt_lang].into_iter().flatten().find_map(CjkLocale::from_lang);
    let theme = own.or_else(|| if lang.is_some() { None } else { default.or(text) });
    (own.or(default).or(text), theme)
}

/// The theme's font for East Asian tradition `t`, from the heading font set for a `+mj-ea` slot
/// (or a heading run with no `a:ea`) and the body set otherwise.
fn theme_script_font(theme: &Theme, ea_ref: &str, major: bool, t: CjkLocale) -> Option<String> {
    let major = ea_ref.starts_with("+mj") || (major && !ea_ref.starts_with("+mn"));
    let set = if major { &theme.fonts.major } else { &theme.fonts.minor };
    set.script(t.theme_script()).filter(|f| !f.trim().is_empty()).map(String::from)
}

pub(crate) fn make_style(ctx: &Ctx, r: &RunProps, scale: f64, prompt: Option<Rgba>, text_cjk: Option<CjkLocale>) -> Style {
    let family = resolve::font_family(ctx, r);
    let bold = r.bold.unwrap_or(false);
    let italic = r.italic.unwrap_or(false);
    let face = FontDb::global().document_face(&family, style_name(bold, italic));
    let size = (r.size.unwrap_or(18.0) * scale).clamp(0.5, 4000.0);
    let (mut color, alpha, gradient) = match &r.fill {
        // The resolved colour carries the fill's transparency (`a:alpha`): it is applied once.
        Some(Fill::Solid { color }) => (ctx.color(color, None), 1.0, None),
        Some(Fill::Gradient(g)) => {
            // Colours resolved here (theme, colour map), so the renderer needs no colour scheme.
            let mut g = g.clone();
            for s in &mut g.stops {
                s.color = deckcraft_model::ColorRef::rgb(ctx.color(&s.color, None));
            }
            (g.stops.first().map(|s| ctx.color(&s.color, None)).unwrap_or(Rgba::BLACK), 1.0, Some(g))
        }
        Some(Fill::None) => (Rgba::TRANSPARENT, 0.0, None),
        _ => (resolve::text_color(ctx, r), 1.0, None),
    };
    if let Some(p) = prompt {
        color = p;
    }
    let major = r.font.as_deref().is_some_and(|f| f.starts_with("+mj"));
    let theme = &ctx.master.theme;
    let slot_family = |explicit: &Option<String>, theme_ref: &str| {
        let name = theme.font(explicit.as_deref().unwrap_or(theme_ref));
        (!name.trim().is_empty()).then_some(name)
    };
    let (cjk, ea_tradition) = run_traditions(r.lang.as_deref(), r.alt_lang.as_deref(), deckcraft_fonts::default_cjk_locale(), text_cjk);
    let ea_ref = r.font_ea.as_deref().unwrap_or(if major { "+mj-ea" } else { "+mn-ea" });
    let ea = slot_family(&r.font_ea, ea_ref).or_else(|| ea_tradition.and_then(|t| theme_script_font(theme, ea_ref, major, t)));
    let cs = slot_family(&r.font_cs, if major { "+mj-cs" } else { "+mn-cs" });
    let underline =
        r.underline.as_deref().and_then(Underline::from_xml).map(|u| (r.underline_color.as_ref().map(|c| ctx.color(c, None)).unwrap_or(color), u));
    let outline = r.outline.as_ref().and_then(|l| match &l.fill {
        Some(Fill::Solid { color: c }) => Some((ctx.color(c, None), l.width.unwrap_or(0.75))),
        _ => None,
    });
    Style {
        face,
        size,
        color,
        alpha,
        bold,
        italic,
        baseline: r.baseline.unwrap_or(0.0),
        spacing: r.spacing.unwrap_or(0.0) * scale,
        caps: r.caps.unwrap_or(Caps::None),
        underline,
        strike: r.strike.filter(|s| *s != Strike::None),
        highlight: r.highlight.as_ref().map(|c| ctx.color(c, None)),
        outline,
        link: r.link.is_some(),
        gradient,
        effects: run_effects(ctx, r, scale),
        style_name: style_name(bold, italic),
        lang: r.lang.clone().filter(|l| !l.is_empty()),
        cjk,
        ea,
        cs,
        sym: r.font_sym.as_deref().map(|f| ctx.master.theme.font(f)).filter(|f| !f.trim().is_empty()),
        major,
        kern: r.kern.is_some_and(|k| k > 0.0 && size + 1e-6 >= k * scale),
    }
}

/// The faces to try for a cluster of `slot`/`script` in style `st`, in order: the slot's
/// typeface (for the East Asian slot, the theme's font for the run's tradition when `a:ea` is
/// empty: see [`make_style`]), the theme's font for the character's own script
/// (`a:font[@script]`: Arabic, Hebrew, Thai… and Hangul, whose ISO 15924 code is the theme's),
/// then the run's Latin face.
pub(crate) fn face_chain(ctx: &Ctx, st: &Style, slot: FontSlot, script: Option<ScriptTag>) -> Vec<Arc<FontFace>> {
    let db = FontDb::global();
    let mut chain: Vec<Arc<FontFace>> = Vec::with_capacity(3);
    let mut push = |family: &str| {
        // Only families that exist (or a missing CJK font's stand-in of its kind, a Mincho for
        // ＭＳ 明朝): `FontDb::face` would substitute the default face.
        if let Some(f) = db.available_face(family, st.style_name)
            && !chain.iter().any(|c| Arc::ptr_eq(c, &f))
        {
            chain.push(f);
        }
    };
    match slot {
        FontSlot::EastAsian => st.ea.as_deref().into_iter().for_each(&mut push),
        FontSlot::ComplexScript => st.cs.as_deref().into_iter().for_each(&mut push),
        FontSlot::Latin => {}
    }
    if let Some(sc) = script.and_then(|t| std::str::from_utf8(&t).ok().map(String::from)) {
        let set = if st.major { &ctx.master.theme.fonts.major } else { &ctx.master.theme.fonts.minor };
        if let Some(f) = set.script(&sc) {
            push(f);
        }
    }
    if !chain.iter().any(|c| Arc::ptr_eq(c, &st.face)) {
        chain.push(st.face.clone());
    }
    chain
}

/// Space before or after a paragraph, points. Shrink-on-overflow's line spacing reduction
/// doesn't apply to it (PowerPoint keeps it).
pub(crate) fn spacing_pts(s: Option<Spacing>, size: f64) -> f64 {
    match s {
        Some(Spacing::Pct(p)) => p * size * 1.2,
        Some(Spacing::Pts(v)) => v,
        None => 0.0,
    }
}
