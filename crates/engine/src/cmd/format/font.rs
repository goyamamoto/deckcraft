use super::{
    Arc, Caps, ColorRef, ColorTransform, Fill, Result, RunProps, SchemeSlot, Session, Shape, ShapeId, ShapeKind, Strike, Value, bad, bool_or,
    bool_param, color_param, current_run, ed, edit_shapes, f64_param, format_run, json, ok, on_param, resolve, str_param,
};

pub(super) fn bold(s: &mut Session, p: &Value) -> Result<Value> {
    let v = on_param(p, current_run(s).bold);
    format_run(s, &move |r| r.bold = Some(v))
}

pub(super) fn italic(s: &mut Session, p: &Value) -> Result<Value> {
    let v = on_param(p, current_run(s).italic);
    format_run(s, &move |r| r.italic = Some(v))
}

pub(super) fn underline(s: &mut Session, p: &Value) -> Result<Value> {
    let cur = current_run(s).underline.is_some_and(|u| u != "none");
    let v = on_param(p, Some(cur));
    let style = str_param(p, "style").unwrap_or("sng").to_string();
    format_run(s, &move |r| r.underline = Some(if v { style.clone() } else { "none".into() }))
}

pub(super) fn strike(s: &mut Session, p: &Value) -> Result<Value> {
    let cur = current_run(s).strike.is_some_and(|x| x != Strike::None);
    let v = on_param(p, Some(cur));
    let d = bool_or(p, "double", false);
    format_run(s, &move |r| {
        r.strike = Some(if !v {
            Strike::None
        } else if d {
            Strike::Double
        } else {
            Strike::Single
        })
    })
}

pub(super) fn superscript(s: &mut Session, p: &Value) -> Result<Value> {
    let v = on_param(p, Some(current_run(s).baseline.unwrap_or(0.0) > 0.0));
    format_run(s, &move |r| r.baseline = Some(if v { 0.3 } else { 0.0 }))
}

pub(super) fn subscript(s: &mut Session, p: &Value) -> Result<Value> {
    let v = on_param(p, Some(current_run(s).baseline.unwrap_or(0.0) < 0.0));
    format_run(s, &move |r| r.baseline = Some(if v { -0.25 } else { 0.0 }))
}

/// A font name parameter: absent, or a non-blank string of at most 200 characters (trimmed).
fn font_name(p: &Value, cmd: &str, key: &str) -> Result<Option<String>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() || t.chars().count() > 200 {
                return Err(bad(cmd, format!("`{key}` must be 1–200 characters")));
            }
            Ok(Some(t.to_string()))
        }
        Some(_) => Err(bad(cmd, format!("`{key}` must be a string"))),
    }
}

/// The East Asian font parameter (`eastAsian`, or its alias `ea`).
fn east_asian_param(p: &Value, cmd: &str) -> Result<Option<String>> {
    match font_name(p, cmd, "eastAsian")? {
        Some(f) => Ok(Some(f)),
        None => font_name(p, cmd, "ea"),
    }
}

/// Is `family` an East Asian (Japanese, Chinese or Korean) font, which PowerPoint applies to East
/// Asian text too when it is picked in the font box? Its name is written in those scripts
/// (メイリオ, 游ゴシック, 맑은 고딕), or its installed face maps kana, common ideographs or
/// Hangul. Theme references (`+mn-lt`) and fonts that aren't installed under a Latin name aren't.
pub(crate) fn is_east_asian_family(family: &str) -> bool {
    use deckcraft_fonts::{FontDb, FontSlot};
    let name = family.trim();
    if name.is_empty() || name.starts_with('+') {
        return false;
    }
    if name.chars().any(|c| deckcraft_fonts::script::slot_of(c) == Some(FontSlot::EastAsian)) {
        return true;
    }
    let db = FontDb::global();
    if !db.has_family(name) {
        return false;
    }
    let face = db.face(name, "Regular");
    // `face` falls back to another family when this one has no usable face: only its own counts.
    let own = face.family.eq_ignore_ascii_case(name) || face.legacy.as_ref().is_some_and(|(f, _)| f.eq_ignore_ascii_case(name));
    let maps = |cs: &[char]| cs.iter().all(|c| face.covers(*c));
    own && (maps(&['あ', 'ア']) || maps(&['中', '国', '日']) || maps(&['가', '한']))
}

/// Home ▸ Font: as in PowerPoint, picking an East Asian font sets both the Latin and the East
/// Asian font (`a:latin`, `a:ea`), so Japanese text takes it too, while picking a Latin font
/// leaves the East Asian text's font alone. `eastAsian` sets only the East Asian font, as the
/// Font dialog's Asian text font box does.
pub(super) fn font(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "format.font";
    let family = font_name(p, CMD, "family")?;
    let east_asian = east_asian_param(p, CMD)?;
    if family.is_none() && east_asian.is_none() {
        return Err(bad(CMD, "missing `family` (or `eastAsian`)"));
    }
    let ea = east_asian.or_else(|| family.clone().filter(|f| is_east_asian_family(f)));
    format_run(s, &move |r| {
        if let Some(f) = &family {
            r.font = Some(f.clone());
        }
        if let Some(f) = &ea {
            r.font_ea = Some(f.clone());
        }
    })
}

pub(super) fn size(s: &mut Session, p: &Value) -> Result<Value> {
    let v = f64_param(p, "size").ok_or_else(|| bad("format.size", "missing `size`"))?.clamp(1.0, 4000.0);
    format_run(s, &move |r| r.size = Some(v))
}

/// PowerPoint's font size list (grow/shrink steps through it).
pub const SIZES: [f64; 20] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0];

fn step(cur: f64, up: bool) -> f64 {
    if up {
        SIZES.iter().copied().find(|v| *v > cur + 0.01).unwrap_or((cur + 8.0).min(4000.0))
    } else {
        SIZES.iter().rev().copied().find(|v| *v < cur - 0.01).unwrap_or((cur - 1.0).max(1.0))
    }
}

fn grow_shrink(s: &mut Session, up: bool) -> Result<Value> {
    // Each run steps from its own size, like PowerPoint.
    let st = s.doc()?;
    let ctx_sizes = current_run(s).size.unwrap_or(18.0);
    let _ = st;
    let base = ctx_sizes;
    format_run(s, &move |r| {
        let cur = r.size.unwrap_or(base);
        r.size = Some(step(cur, up));
    })
}

pub(super) fn grow(s: &mut Session, _p: &Value) -> Result<Value> {
    grow_shrink(s, true)
}

pub(super) fn shrink(s: &mut Session, _p: &Value) -> Result<Value> {
    grow_shrink(s, false)
}

pub(super) fn color(s: &mut Session, p: &Value) -> Result<Value> {
    let c = color_param(p, "color").ok_or_else(|| bad("format.color", "missing or invalid `color`"))?;
    // A selected chart has no runs: Font Color colours its text (title, axes, legend, labels),
    // which used to answer and change nothing.
    let st = s.doc()?;
    if st.selection.text.is_none() {
        let charts: Vec<ShapeId> =
            st.selection.shapes.iter().copied().filter(|id| matches!(st.shape(*id).map(|sh| &sh.kind), Some(ShapeKind::Chart(_)))).collect();
        let only_charts = charts.len() == st.selection.shapes.len();
        if !charts.is_empty() {
            let tc = c.clone();
            edit_shapes(s, &json!({"ids": charts}), "format.color", |sh| {
                if let ShapeKind::Chart(ch) = &mut sh.kind {
                    ch.text_color = Some(tc.clone());
                    ch.raw = None;
                }
                Ok(())
            })?;
            if only_charts {
                return ok();
            }
        }
    }
    format_run(s, &move |r| r.fill = Some(Fill::solid(c.clone())))
}

/// `a:alpha` for `transparency` (0–1) on `c`, replacing any alpha it had; fully opaque drops it.
fn with_transparency(c: &mut ColorRef, transparency: f64) {
    c.mods.retain(|m| !matches!(m, ColorTransform::Alpha(_)));
    if transparency > 0.0 {
        c.mods.push(ColorTransform::Alpha(((1.0 - transparency) * 100_000.0).round() as i32));
    }
}

/// Text Fill › Transparency: the text keeps its colour and gets an alpha. A run without its own
/// fill takes the colour it shows now (from the placeholder, master or theme) so only its
/// opacity changes.
pub(super) fn text_transparency(s: &mut Session, p: &Value) -> Result<Value> {
    let v = f64_param(p, "value").ok_or_else(|| bad("format.textTransparency", "missing `value` (0–1)"))?;
    if !v.is_finite() {
        return Err(bad("format.textTransparency", "`value` must be a number from 0 to 1"));
    }
    let v = v.clamp(0.0, 1.0);
    let shown = match current_run(s).fill {
        Some(f @ (Fill::Solid { .. } | Fill::Gradient(_))) => f,
        _ => Fill::solid(ColorRef::scheme(SchemeSlot::Tx1)),
    };
    format_run(s, &move |r| {
        let mut f = match &r.fill {
            Some(f @ (Fill::Solid { .. } | Fill::Gradient(_))) => f.clone(),
            Some(Fill::None) => return,
            _ => shown.clone(),
        };
        match &mut f {
            Fill::Solid { color } => with_transparency(color, v),
            Fill::Gradient(g) => g.stops.iter_mut().for_each(|st| with_transparency(&mut st.color, v)),
            _ => {}
        }
        r.fill = Some(f);
    })
}

/// The `lang` and `noProof` params of the language commands, checked: a run-props setter.
fn language_edit(id: &str, p: &Value) -> Result<impl Fn(&mut RunProps) + Send + Sync + 'static> {
    let lang = str_param(p, "lang").map(str::to_string);
    let no_proof = bool_param(p, "noProof");
    if lang.is_none() && no_proof.is_none() {
        return Err(bad(id, "give `lang` or `noProof`"));
    }
    if lang.as_deref().is_some_and(|l| l.is_empty() || l.len() > 35 || !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        return Err(bad(id, "`lang` is a language tag such as en-US"));
    }
    Ok(move |r: &mut RunProps| {
        if let Some(l) = &lang {
            r.lang = Some(l.clone());
        }
        if let Some(n) = no_proof {
            r.no_proof = n.then_some(true);
        }
    })
}

/// Review › Language on the selected text (or the selected shapes' text); with nothing selected,
/// the language of new text ([`default_language`]).
pub(super) fn language(s: &mut Session, p: &Value) -> Result<Value> {
    let f = language_edit("format.language", p)?;
    if s.doc().is_ok_and(|st| st.selection.text.is_none() && st.selection.shapes.is_empty()) {
        return default_language(s, str_param(p, "lang"), &f);
    }
    format_run(s, &f)
}

/// `format.language` with nothing selected: the language of new text, the one the status bar
/// shows then. The deck's default text style (`a:defPPr`) takes it. Text that had the old default
/// only from there keeps that language, now written on its runs, as typed text in PowerPoint
/// carries its own: a Japanese deck switched to English keeps its Japanese text Japanese (its
/// fonts, line breaking and dates), and what is typed next is English. The text styles that
/// repeat the old default take the new one too ([`restyle_default_lang`]): after Whole
/// presentation (`format.languageAll`) every level names it, and new text would keep it. `lang` is
/// the new language and `f` its checked setter ([`language_edit`]).
fn default_language(s: &mut Session, lang: Option<&str>, f: &dyn Fn(&mut RunProps)) -> Result<Value> {
    s.edit(|doc, _| {
        let old = doc.default_text_style.default.as_ref().and_then(|d| d.run.lang.clone()).filter(|l| !l.trim().is_empty());
        if let (Some(old), Some(new)) = (&old, lang)
            && !old.eq_ignore_ascii_case(new)
        {
            keep_default_lang(doc, old);
        }
        f(&mut doc.default_text_style.default.get_or_insert_with(Default::default).run);
        if let Some(old) = &old {
            restyle_default_lang(doc, old, f);
        }
        Ok(())
    })?;
    Ok(json!({"default": true}))
}

/// The text style levels that name language `old` (the deck's default before
/// [`default_language`] changes it) take the new one (`f`): the deck's default text style, the
/// masters' title, body and other styles, and the list styles of the text on slides, notes,
/// layouts and masters. Whole presentation (`format.languageAll`) writes its language on each,
/// and there they would hide the deck's default from new text. A level in another language (a box
/// kept in German) is left alone. The text on slides, layouts and masters that had `old` from
/// these levels already has it written on its runs ([`keep_default_lang`]).
fn restyle_default_lang(doc: &mut deckcraft_model::Presentation, old: &str, f: &dyn Fn(&mut RunProps)) {
    use deckcraft_model::text::ListStyle;
    let style = |l: &mut ListStyle| {
        let mut changed = false;
        for lv in l.levels.iter_mut().flatten().chain(l.default.as_mut()).filter(|lv| lv.run.lang.as_deref() == Some(old)) {
            f(&mut lv.run);
            changed = true;
        }
        changed
    };
    fn shapes(list: &mut [Shape], style: &dyn Fn(&mut ListStyle) -> bool, depth: usize) -> bool {
        let mut changed = false;
        for sh in list.iter_mut().filter(|_| depth <= 64) {
            if let Some(t) = sh.text.as_mut() {
                changed |= style(&mut t.list_style);
            }
            if let deckcraft_model::ShapeKind::Table(t) = &mut sh.kind {
                for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    changed |= style(&mut c.text.list_style);
                }
            }
            if let Some(children) = sh.children_mut() {
                changed |= shapes(children, style, depth + 1);
            }
        }
        changed
    }
    // Copies first, so slides and masters with nothing to change stay shared with the undo step.
    for sl in &mut doc.slides {
        let mut list = sl.shapes.clone();
        let mut notes = sl.notes.list_style.clone();
        let changed = shapes(&mut list, &style, 0);
        if style(&mut notes) || changed {
            let sl = Arc::make_mut(sl);
            sl.shapes = list;
            sl.notes.list_style = notes;
        }
    }
    let extra = [doc.notes_master.as_mut(), doc.handout_master.as_mut()];
    for m in doc.masters.iter_mut().chain(extra.into_iter().flatten()) {
        let mut copy = (**m).clone();
        let mut changed = shapes(&mut copy.shapes, &style, 0);
        for l in &mut copy.layouts {
            changed |= shapes(&mut l.shapes, &style, 0);
        }
        for l in [&mut copy.title_style, &mut copy.body_style, &mut copy.other_style] {
            changed |= style(l);
        }
        if changed {
            *m = Arc::new(copy);
        }
    }
    style(&mut doc.default_text_style);
}

/// Write language `old` on every run that has it only from the text styles (none of its own): the
/// deck's default text style, or the levels [`restyle_default_lang`] moves to the new language. On
/// slides, layouts and masters, tables included.
fn keep_default_lang(doc: &mut deckcraft_model::Presentation, old: &str) {
    use deckcraft_model::text::TextBody;
    fn body(ctx: &resolve::Ctx, sh: &Shape, b: &mut TextBody, old: &str) -> bool {
        let mut changed = false;
        for para in &mut b.paragraphs {
            let at = para.clone();
            for r in &mut para.runs {
                if r.props.lang.is_none() && resolve::run(ctx, sh, &at, &r.props).lang.as_deref() == Some(old) {
                    r.props.lang = Some(old.to_string());
                    changed = true;
                }
            }
        }
        changed
    }
    fn shapes(ctx: &resolve::Ctx, list: &mut [Shape], old: &str, depth: usize) -> bool {
        let mut changed = false;
        for sh in list.iter_mut().filter(|_| depth <= 64) {
            let at = sh.clone();
            if let Some(b) = sh.text.as_mut() {
                changed |= body(ctx, &at, b, old);
            }
            if let deckcraft_model::ShapeKind::Table(t) = &mut sh.kind {
                for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    changed |= body(ctx, &at, &mut c.text, old);
                }
            }
            if let Some(children) = sh.children_mut() {
                changed |= shapes(ctx, children, old, depth + 1);
            }
        }
        changed
    }
    let before = doc.clone();
    for (i, slide) in before.slides.iter().enumerate() {
        let Some(ctx) = resolve::Ctx::for_slide(&before, slide) else { continue };
        let mut list = slide.shapes.clone();
        if shapes(&ctx, &mut list, old, 0)
            && let Some(s) = doc.slides.get_mut(i)
        {
            Arc::make_mut(s).shapes = list;
        }
    }
    for (i, m) in before.masters.iter().enumerate() {
        let mut own = m.shapes.clone();
        let own_changed = shapes(&resolve::Ctx::for_master(&before, m), &mut own, old, 0);
        let mut layouts: Vec<(usize, Vec<Shape>)> = vec![];
        for (j, l) in m.layouts.iter().enumerate() {
            let mut list = l.shapes.clone();
            if shapes(&resolve::Ctx::for_layout(&before, m, l), &mut list, old, 0) {
                layouts.push((j, list));
            }
        }
        if (own_changed || !layouts.is_empty())
            && let Some(dm) = doc.masters.get_mut(i)
        {
            let dm = Arc::make_mut(dm);
            if own_changed {
                dm.shapes = own;
            }
            for (j, list) in layouts {
                if let Some(l) = dm.layouts.get_mut(j) {
                    l.shapes = list;
                }
            }
        }
    }
}

/// Every run in the deck, and the text styles new text inherits from, in one language.
pub(super) fn language_all(s: &mut Session, p: &Value) -> Result<Value> {
    let f = language_edit("format.languageAll", p)?;
    /// Runs, end marks and the body's own list style levels and `a:defPPr` (never creating one).
    fn body(t: &mut deckcraft_model::text::TextBody, f: &dyn Fn(&mut RunProps)) {
        ed::format_all(t, f);
        t.list_style.levels.iter_mut().flatten().chain(t.list_style.default.as_mut()).for_each(|lv| f(&mut lv.run));
    }
    /// Every text body in `list`: shapes, table cells and groups (nested at most 64 deep).
    fn shapes(list: &mut [Shape], f: &dyn Fn(&mut RunProps), depth: usize) {
        if depth > 64 {
            return;
        }
        for sh in list {
            if let Some(t) = &mut sh.text {
                body(t, f);
            }
            match &mut sh.kind {
                deckcraft_model::ShapeKind::Table(t) => t.rows.iter_mut().flat_map(|r| &mut r.cells).for_each(|c| body(&mut c.text, f)),
                deckcraft_model::ShapeKind::Group { children, .. } => shapes(children, f, depth + 1),
                _ => {}
            }
        }
    }
    /// A master or presentation text style's levels and `a:defPPr`: what new text inherits.
    fn styles(l: &mut deckcraft_model::text::ListStyle, f: &dyn Fn(&mut RunProps)) {
        // Level 1 always, so a deck without a default text style gets one.
        if l.level(0).is_none() {
            l.set(0, Default::default());
        }
        l.levels.iter_mut().flatten().chain(l.default.as_mut()).for_each(|lv| f(&mut lv.run));
    }
    s.edit(|doc, _| {
        for sl in &mut doc.slides {
            let sl = Arc::make_mut(sl);
            shapes(&mut sl.shapes, &f, 0);
            body(&mut sl.notes, &f);
        }
        let extra = [doc.notes_master.as_mut(), doc.handout_master.as_mut()];
        for m in doc.masters.iter_mut().chain(extra.into_iter().flatten()) {
            let m = Arc::make_mut(m);
            shapes(&mut m.shapes, &f, 0);
            m.layouts.iter_mut().for_each(|l| shapes(&mut l.shapes, &f, 0));
            [&mut m.title_style, &mut m.body_style, &mut m.other_style].into_iter().for_each(|l| styles(l, &f));
        }
        // The deck's `a:defPPr` too, made if missing: the language of new text, which format.language
        // sets with nothing selected and the status bar shows then.
        if doc.default_text_style.default.is_none() {
            doc.default_text_style.default = Some(Default::default());
        }
        styles(&mut doc.default_text_style, &f);
        Ok(())
    })?;
    ok()
}

pub(super) fn highlight(s: &mut Session, p: &Value) -> Result<Value> {
    let c = color_param(p, "color");
    format_run(s, &move |r| r.highlight = c.clone())
}

pub(super) fn caps(s: &mut Session, p: &Value) -> Result<Value> {
    let c = match str_param(p, "caps").unwrap_or("none") {
        "small" => Caps::Small,
        "all" => Caps::All,
        _ => Caps::None,
    };
    format_run(s, &move |r| r.caps = Some(c))
}

pub(super) fn spacing(s: &mut Session, p: &Value) -> Result<Value> {
    let v = f64_param(p, "pt").unwrap_or(0.0).clamp(-100.0, 400.0);
    format_run(s, &move |r| r.spacing = Some(v))
}

/// Character formatting applied together, so the Font dialog is one undo step.
struct FontEdits {
    family: Option<String>,
    /// The Asian text font (`a:ea`); `family` is the Latin text font only.
    east_asian: Option<String>,
    size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<String>,
    color: Option<deckcraft_model::ColorRef>,
    strike: Option<Strike>,
    baseline: Option<f64>,
    caps: Option<Caps>,
    spacing: Option<f64>,
}

impl FontEdits {
    fn is_empty(&self) -> bool {
        self.family.is_none()
            && self.east_asian.is_none()
            && self.size.is_none()
            && self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.color.is_none()
            && self.strike.is_none()
            && self.baseline.is_none()
            && self.caps.is_none()
            && self.spacing.is_none()
    }

    fn write(&self, r: &mut RunProps) {
        if let Some(f) = &self.family {
            r.font = Some(f.clone());
        }
        if let Some(f) = &self.east_asian {
            r.font_ea = Some(f.clone());
        }
        if let Some(v) = self.size {
            r.size = Some(v);
        }
        if let Some(v) = self.bold {
            r.bold = Some(v);
        }
        if let Some(v) = self.italic {
            r.italic = Some(v);
        }
        if let Some(u) = &self.underline {
            r.underline = Some(u.clone());
        }
        if let Some(c) = &self.color {
            r.fill = Some(Fill::solid(c.clone()));
        }
        if let Some(v) = self.strike {
            r.strike = Some(v);
        }
        if let Some(v) = self.baseline {
            r.baseline = Some(v);
        }
        if let Some(v) = self.caps {
            r.caps = Some(v);
        }
        if let Some(v) = self.spacing {
            r.spacing = Some(v);
        }
    }
}

fn font_bool(p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(bad("format.fontDialog", format!("`{key}` must be a bool"))),
    }
}

fn font_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(bad("format.fontDialog", format!("`{key}` must be a string"))),
    }
}

fn font_edits(p: &Value) -> Result<FontEdits> {
    // The dialog's two boxes set one font each, as PowerPoint's Latin text font and Asian text
    // font boxes do (only the ribbon's font box applies an East Asian family to both).
    let family = font_name(p, "format.fontDialog", "family")?;
    let east_asian = east_asian_param(p, "format.fontDialog")?;
    let size = match p.get("size") {
        None => None,
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad("format.fontDialog", "`size` must be a finite number"))?;
            Some(n.clamp(1.0, 4000.0))
        }
    };
    let underline = match font_str(p, "underline")? {
        None => None,
        Some(s) => {
            if !matches!(s, "none" | "sng" | "dbl" | "heavy" | "dotted" | "dash" | "wavy") {
                return Err(bad("format.fontDialog", "`underline` must be none, sng, dbl, heavy, dotted, dash or wavy"));
            }
            Some(s.to_string())
        }
    };
    let color =
        if p.get("color").is_some() { Some(color_param(p, "color").ok_or_else(|| bad("format.fontDialog", "invalid `color`"))?) } else { None };
    let strike = match font_str(p, "strike")? {
        None => None,
        Some("none") => Some(Strike::None),
        Some("sng" | "single") => Some(Strike::Single),
        Some("dbl" | "double") => Some(Strike::Double),
        Some(_) => return Err(bad("format.fontDialog", "`strike` must be none, sng or dbl")),
    };
    let baseline = match p.get("baseline") {
        None => None,
        Some(Value::String(s)) => match s.as_str() {
            "none" => Some(0.0),
            "super" | "superscript" => Some(0.3),
            "sub" | "subscript" => Some(-0.25),
            _ => return Err(bad("format.fontDialog", "`baseline` must be none, super, sub or a fraction")),
        },
        Some(Value::Number(n)) => {
            let f = n.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad("format.fontDialog", "`baseline` must be finite"))?;
            Some(f.clamp(-1.0, 1.0))
        }
        Some(_) => return Err(bad("format.fontDialog", "`baseline` must be none, super, sub or a fraction")),
    };
    let caps = match font_str(p, "caps")? {
        None => None,
        Some("none") => Some(Caps::None),
        Some("small") => Some(Caps::Small),
        Some("all") => Some(Caps::All),
        Some(_) => return Err(bad("format.fontDialog", "`caps` must be none, small or all")),
    };
    let spacing = match p.get("spacing") {
        None => None,
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad("format.fontDialog", "`spacing` must be a finite number"))?;
            Some(n.clamp(-100.0, 400.0))
        }
    };
    Ok(FontEdits {
        family,
        east_asian,
        size,
        bold: font_bool(p, "bold")?,
        italic: font_bool(p, "italic")?,
        underline,
        color,
        strike,
        baseline,
        caps,
        spacing,
    })
}

/// A colour as a `format.fontDialog` / `format.color` parameter, including transforms.
fn color_command_value(c: &deckcraft_model::ColorRef) -> Value {
    use deckcraft_model::ColorBase;
    let mut o = serde_json::Map::new();
    for m in &c.mods {
        if let Some(v) = m.value() {
            o.insert(m.xml_name().to_string(), json!(v));
        }
    }
    match &c.base {
        ColorBase::Rgb { rgb } if o.is_empty() => json!(format!("#{}", rgb.hex())),
        ColorBase::Rgb { rgb } => {
            o.insert("rgb".into(), json!(format!("#{}", rgb.hex())));
            Value::Object(o)
        }
        ColorBase::Scheme { slot } => {
            o.insert("scheme".into(), json!(slot.xml_name()));
            Value::Object(o)
        }
        ColorBase::Preset { name } => json!(name),
        ColorBase::System { last, .. } => json!(format!("#{}", last.hex())),
    }
}

/// The East Asian font text with resolved props `r` is drawn in, as the text layout picks it: the
/// run's own `a:ea` (theme references resolved), else the theme's East Asian heading or body
/// font, else the theme's font for the run's East Asian language (`Jpan` for Japanese: its
/// `lang`, then `altLang`, then the system's East Asian language). `None` when none applies and
/// East Asian text falls back past the Latin font.
pub(crate) fn east_asian_family(ctx: &resolve::Ctx, r: &RunProps) -> Option<String> {
    let theme = &ctx.master.theme;
    let major = r.font.as_deref().is_some_and(|f| f.starts_with("+mj"));
    let named = theme.font(r.font_ea.as_deref().unwrap_or(if major { "+mj-ea" } else { "+mn-ea" }));
    if !named.trim().is_empty() {
        return Some(named);
    }
    let cjk = [r.lang.as_deref(), r.alt_lang.as_deref()]
        .into_iter()
        .flatten()
        .find_map(deckcraft_fonts::CjkLocale::from_lang)
        .or_else(deckcraft_fonts::default_cjk_locale)?;
    let set = if major { &theme.fonts.major } else { &theme.fonts.minor };
    set.script(cjk.theme_script()).map(str::to_string)
}

/// The Latin and East Asian fonts of the run props at the selection.
pub(super) fn current_fonts(s: &Session, r: &RunProps) -> (String, Option<String>) {
    let Ok(st) = s.doc() else { return (String::new(), None) };
    match crate::cmd::text::ctx_for(&st.doc, &st.selection) {
        Some(c) => (resolve::font_family(&c, r), east_asian_family(&c, r)),
        None => (r.font.clone().unwrap_or_default(), r.font_ea.clone()),
    }
}

/// The character the font box describes: the first selected one, else the one before the caret
/// (what typing there continues), else the one after it; with shapes selected, their text's
/// first character.
pub(super) fn described_char(s: &Session) -> Option<char> {
    let st = s.doc().ok()?;
    let Some(t) = &st.selection.text else {
        let sh = st.selected_shapes().into_iter().next()?;
        return sh.text.as_ref()?.paragraphs.iter().find_map(|p| p.text().chars().next());
    };
    let body = crate::cmd::text::body_of(st, t)?;
    let (a, b) = t.ordered();
    let line: Vec<char> = body.paragraphs.get(a.0)?.text().chars().collect();
    let after = line.get(a.1).copied();
    if a != b {
        return after;
    }
    a.1.checked_sub(1).and_then(|i| line.get(i).copied()).or(after)
}

fn font_dialog_params(s: &Session) -> Value {
    let r = current_run(s);
    let (family, east_asian) = current_fonts(s, &r);
    let baseline = r.baseline.unwrap_or(0.0);
    let mut v = json!({
        "family": family,
        "eastAsian": east_asian.unwrap_or_default(),
        "size": r.size.unwrap_or(18.0),
        "bold": r.bold.unwrap_or(false),
        "italic": r.italic.unwrap_or(false),
        "underline": r.underline.as_deref().filter(|u| *u != "none").unwrap_or("none"),
        "strike": match r.strike {
            Some(Strike::Double) => "dbl",
            Some(Strike::Single) => "sng",
            _ => "none",
        },
        "baseline": if baseline > 0.0 { "super" } else if baseline < 0.0 { "sub" } else { "none" },
        "caps": match r.caps {
            Some(Caps::Small) => "small",
            Some(Caps::All) => "all",
            _ => "none",
        },
        "spacing": r.spacing.unwrap_or(0.0),
    });
    if let Some(Fill::Solid { color }) = &r.fill {
        v["color"] = color_command_value(color);
    }
    v
}

pub(super) fn font_dialog(s: &mut Session, p: &Value) -> Result<Value> {
    let edits = font_edits(p)?;
    if edits.is_empty() {
        s.ui_requests.push(crate::UiRequest::Dialog { id: "font".into(), params: font_dialog_params(s) });
        return ok();
    }
    format_run(s, &|r| edits.write(r))
}

pub(super) fn clear(s: &mut Session, _p: &Value) -> Result<Value> {
    format_run(s, &|r| *r = RunProps { lang: r.lang.clone(), link: r.link.clone(), ..Default::default() })
}
