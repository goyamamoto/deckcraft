use super::font::{current_fonts, described_char};
use super::paragraph::dir_name;
use super::{
    Bullet, Effects, Fill, Line, ParaProps, Result, RunProps, Session, Shape, Strike, TextDir, Value, bad, base64_decode, bool_or, current_para,
    current_run, ed, edit_shapes, format_run, ids_param, json, ok, resolve, str_param,
};

/// Formatting picked up by the Format Painter.
#[derive(Clone, Debug, Default)]
pub struct Painted {
    pub run: Option<RunProps>,
    pub para: Option<ParaProps>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    pub effects: Option<Effects>,
    pub style: Option<deckcraft_model::ShapeStyle>,
}

pub(super) fn painter_pick(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mut painted = Painted::default();
    if let Some(t) = &st.selection.text {
        if let Some(b) = crate::cmd::text::body_of(st, t) {
            let a = t.ordered().0;
            painted.run = Some(ed::props_at(b, (a.0, a.1 + 1)));
            painted.para = b.paragraphs.get(a.0).map(|x| x.props.clone());
        }
    } else if let Some(sh) = st.selected_shapes().first() {
        painted.fill = sh.fill.clone();
        painted.line = sh.line.clone();
        painted.effects = sh.effects.clone();
        painted.style = sh.style.clone();
        painted.run = sh.text.as_ref().and_then(|t| t.paragraphs.first()).and_then(|p| p.runs.first()).map(|r| r.props.clone());
    }
    s.painter = Some((painted, bool_or(p, "sticky", false)));
    Ok(json!({"armed": true}))
}

pub(crate) fn apply_painted(sh: &mut Shape, p: &Painted) {
    if p.fill.is_some() || p.style.is_some() {
        sh.fill.clone_from(&p.fill);
        sh.line.clone_from(&p.line);
        sh.effects.clone_from(&p.effects);
        sh.style.clone_from(&p.style);
    }
    if let (Some(r), Some(t)) = (&p.run, sh.text.as_mut()) {
        ed::format_all(t, &|rp| *rp = r.clone());
    }
}

/// The paragraphs the text selection covers whole: it starts at or before their start and
/// reaches their end. A paragraph the selection only touches the start of (it ends at offset
/// 0, as after a triple-click) is not covered.
fn covered_paragraphs(s: &Session) -> Vec<usize> {
    let Ok(st) = s.doc() else { return vec![] };
    let Some(t) = &st.selection.text else { return vec![] };
    let Some(body) = crate::cmd::text::body_of(st, t) else { return vec![] };
    let (a, b) = t.ordered();
    // A hostile selection can point past the body: walk only the paragraphs it has.
    (a.0..=b.0.min(body.paragraphs.len().saturating_sub(1)))
        .filter(|&pi| {
            let Some(para) = body.paragraphs.get(pi) else { return false };
            let starts = pi > a.0 || a.1 == 0;
            let ends = if pi < b.0 {
                true
            } else if b.0 > a.0 && b.1 == 0 {
                false
            } else {
                b.1 >= para.char_len()
            };
            starts && ends
        })
        .collect()
}

pub(super) fn painter_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let Some((painted, sticky)) = s.painter.clone() else { return Err(bad("format.painterApply", "pick formatting with the Format Painter first")) };
    if !sticky {
        s.painter = None;
    }
    if s.doc()?.selection.text.as_ref().is_some_and(|t| t.is_range()) && ids_param(p, "ids").is_none() {
        if let Some(r) = painted.run.clone() {
            format_run(s, &move |rp| *rp = r.clone())?;
        }
        // Paragraph formatting (bullets, alignment, spacing) goes only onto paragraphs the
        // selection covers whole; painting part of a bulleted line changes just its text.
        let covered = covered_paragraphs(s);
        if let (Some(pp), false) = (painted.para.clone(), covered.is_empty()) {
            crate::cmd::text::edit_text(s, |body, _| {
                for &pi in &covered {
                    if let Some(para) = body.paragraphs.get_mut(pi) {
                        para.props = pp.clone();
                    }
                }
            })?;
        }
        return ok();
    }
    edit_shapes(s, p, "format.painterApply", |sh| {
        apply_painted(sh, &painted);
        Ok(())
    })
}

pub(super) fn state(s: &mut Session, _p: &Value) -> Result<Value> {
    let r = current_run(s);
    let pp = current_para(s);
    let (family, east_asian) = current_fonts(s, &r);
    // On East Asian text the font box names the East Asian font, as PowerPoint's does.
    let on_east_asian = described_char(s).is_some_and(|c| deckcraft_fonts::script::slot_of(c) == Some(deckcraft_fonts::FontSlot::EastAsian));
    let (shown, font_theme) = match (&east_asian, on_east_asian) {
        (Some(ea), true) => (ea.clone(), theme_role_of(r.font_ea.as_deref(), r.font.as_deref(), true)),
        _ => (family.clone(), theme_role_of(r.font.as_deref(), None, false)),
    };
    let (text_lang, no_proof) = selection_language(s);
    let dir = s.doc().ok().and_then(|st| {
        let id = st.selection.text.as_ref().filter(|t| !t.notes).map(|t| t.shape).or_else(|| st.selection.shapes.first().copied())?;
        let sh = st.shape(id)?;
        let ctx = crate::cmd::text::ctx_for(&st.doc, &st.selection)?;
        resolve::body(&ctx, sh).vert
    });
    Ok(json!({
        "font": shown,
        "fontTheme": font_theme,
        "fontLatin": family,
        "fontEa": east_asian,
        "fontRef": r.font,
        "fontEaRef": r.font_ea,
        "dir": dir_name(dir.unwrap_or(TextDir::Horizontal)),
        "size": r.size,
        "bold": r.bold.unwrap_or(false),
        "italic": r.italic.unwrap_or(false),
        "underline": r.underline.as_deref().is_some_and(|u| u != "none"),
        "strike": r.strike.is_some_and(|x| x != Strike::None),
        "superscript": r.baseline.unwrap_or(0.0) > 0.0,
        "subscript": r.baseline.unwrap_or(0.0) < 0.0,
        "align": pp.align.map(|a| a.xml()),
        "bullets": matches!(pp.bullet, Some(Bullet::Char { .. })),
        "numbering": matches!(pp.bullet, Some(Bullet::AutoNum { .. })),
        "lang": r.lang,
        "textLang": text_lang,
        "noProof": no_proof,
        "lineSpacing": serde_json::to_value(pp.line_spacing).unwrap_or_default(),
        "painter": s.painter.is_some(),
    }))
}

/// Is the font a run is drawn in one of the theme's, and which (`headings` or `body`)? The font
/// box then says so after the name, as PowerPoint's does. `font` is the run's (resolved) font
/// reference for the slot shown; for the East Asian slot, `latin` is its Latin one, which picks
/// the heading or body face when the run names no East Asian font. A run without a Latin font
/// takes the theme's body font.
fn theme_role_of(font: Option<&str>, latin: Option<&str>, east_asian: bool) -> Option<&'static str> {
    let role = |f: &str| {
        if f.starts_with("+mj") {
            Some("headings")
        } else if f.starts_with("+mn") {
            Some("body")
        } else {
            None
        }
    };
    match font.map(str::trim) {
        Some(f) => role(f),
        None if east_asian && latin.is_some_and(|l| l.trim().starts_with("+mj")) => Some("headings"),
        None => Some("body"),
    }
}

/// The language of the text at the selection, as the status bar names it, and whether that text
/// is marked Do Not Check Spelling: the run's language, inherited from its placeholder, layout,
/// master and the deck's default (`a:defPPr`, `ja-JP` in a Japanese deck) when it has none of its
/// own; its `altLang` instead when the character described (the first selected, else the one
/// before the caret) is East Asian and only the `altLang` is an East Asian language; with nothing
/// selected, the deck's default language. `None` when nothing names one.
pub fn selection_language(s: &Session) -> (Option<String>, bool) {
    let Ok(st) = s.doc() else { return (None, false) };
    if st.selection.text.is_none() && st.selection.shapes.is_empty() {
        let lang = st.doc.default_text_style.default.as_ref().and_then(|d| d.run.lang.clone());
        return (lang.filter(|l| !l.trim().is_empty()), false);
    }
    let r = current_run(s);
    let cjk = |l: &Option<String>| l.as_deref().and_then(deckcraft_fonts::CjkLocale::from_lang).is_some();
    let on_east_asian = described_char(s).is_some_and(|c| deckcraft_fonts::script::slot_of(c) == Some(deckcraft_fonts::FontSlot::EastAsian));
    let lang = if on_east_asian && !cjk(&r.lang) && cjk(&r.alt_lang) { r.alt_lang.clone() } else { r.lang.clone() };
    (lang.filter(|l| !l.trim().is_empty()), r.no_proof == Some(true))
}

/// The theme fonts of the master the selection is on (the first master's with no slide).
fn selection_theme_fonts(s: &Session) -> Option<deckcraft_model::theme::FontScheme> {
    let st = s.doc().ok()?;
    match crate::cmd::text::ctx_for(&st.doc, &st.selection) {
        Some(c) => Some(c.master.theme.fonts.clone()),
        None => st.doc.masters.first().map(|m| m.theme.fonts.clone()),
    }
}

/// Home ▸ Font's font list: the theme's fonts, then every installed family, each by its name in
/// the interface language `lang`.
pub(super) fn font_list(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "font.list";
    let lang = match p.get("lang") {
        None | Some(Value::Null) => "en".to_string(),
        Some(Value::String(l)) if !l.is_empty() && l.len() <= 35 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') => l.clone(),
        Some(_) => return Err(bad(CMD, "`lang` is a language tag such as ja or zh-Hans")),
    };
    let flag = |key: &str| match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(bad(CMD, format!("`{key}` must be true or false"))),
    };
    let editing = s.east_asian_editing.or_else(deckcraft_fonts::default_cjk_locale);
    let east_asian = flag("eastAsian")?.unwrap_or(editing.is_some());
    let with_fonts = flag("fonts")?.unwrap_or(true);
    let with_theme = flag("theme")?.unwrap_or(true);
    let locale = east_asian.then(|| editing.or_else(|| deckcraft_fonts::CjkLocale::from_lang(&lang)).unwrap_or(deckcraft_fonts::CjkLocale::Japanese));
    let theme = match selection_theme_fonts(s) {
        Some(f) if with_theme => crate::fonts::theme_font_choices(&f, locale),
        _ => Vec::new(),
    };
    let fonts = if with_fonts { crate::fonts::installed_font_choices(&lang) } else { Vec::new() };
    Ok(json!({
        "eastAsian": east_asian,
        "theme": serde_json::to_value(theme).unwrap_or_default(),
        "fonts": serde_json::to_value(fonts).unwrap_or_default(),
    }))
}

/// Add a font from its bytes for this session (it then shows in the font menu and draws).
pub(super) fn font_add(_s: &mut Session, p: &Value) -> Result<Value> {
    let data = str_param(p, "data").and_then(base64_decode).ok_or_else(|| bad("font.add", "missing or invalid base64 `data`"))?;
    if data.len() > 64 << 20 {
        return Err(bad("font.add", "font file larger than 64 MB"));
    }
    let faces = deckcraft_fonts::FontDb::global().add_font(data);
    if faces == 0 {
        return Err(bad("font.add", "not a font DeckCraft can read (or already added)"));
    }
    Ok(json!({"faces": faces}))
}

/// Catalog the installed fonts again, so fonts installed while DeckCraft runs appear.
pub(super) fn font_rescan(_s: &mut Session, _p: &Value) -> Result<Value> {
    let db = deckcraft_fonts::FontDb::global();
    #[cfg(not(target_arch = "wasm32"))]
    let faces = db.load_system_fonts();
    #[cfg(target_arch = "wasm32")]
    let faces = 0usize;
    Ok(json!({"faces": faces, "families": db.families().len()}))
}
