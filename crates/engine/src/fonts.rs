//! Fonts a presentation uses that this machine doesn't have, so opening it can say so (its text
//! draws in a stand-in; see `FontDb::document_face`).

use deckcraft_fonts::script::{FontSlot, slot_of};
use deckcraft_fonts::{CjkLocale, FontDb};
use deckcraft_model::resolve::{self, Ctx};
use deckcraft_model::text::{Bullet, RunProps, TextBody};
use deckcraft_model::{Presentation, Shape, ShapeKind};

/// Load each embedded face into the font database, so text in that family draws with the file's
/// font on a machine that doesn't have it. A face already loaded (bundled or installed) stays;
/// the part bytes are kept either way. MicroType Express subsets have no TrueType or OpenType
/// font to load and are left for the round trip.
pub fn install_embedded_fonts(p: &Presentation) {
    let db = FontDb::global();
    for font in &p.embedded_fonts {
        for id in font.faces().into_iter().flatten() {
            let Some(item) = p.media(id) else { continue };
            let Some(sfnt) = deckcraft_fonts::embedded_sfnt(&item.data) else { continue };
            db.add_font(sfnt.to_vec());
        }
    }
}

/// The font families the slides' text is drawn in (theme fonts resolved, bullet fonts and the
/// East Asian and complex-script fonts included) that aren't installed or bundled under any of
/// their names (an installed font named メイリオ or ＭＳ Ｐゴシック is there), sorted, without
/// duplicates (case-insensitive).
pub fn missing_fonts(p: &Presentation) -> Vec<String> {
    let db = FontDb::global();
    let mut out: Vec<String> = Vec::new();
    for family in used_fonts(p) {
        if !db.has_family(&family) && !out.iter().any(|f| f.eq_ignore_ascii_case(&family)) {
            out.push(family);
        }
    }
    out.sort_by_key(|f| f.to_lowercase());
    out
}

/// The families the slides' text runs and bullets are drawn in: each run's Latin font, and its
/// East Asian (`a:ea`) and complex-script (`a:cs`) fonts when it has text in those slots, as
/// PowerPoint's Replace Fonts lists ＭＳ Ｐゴシック beside Calibri.
pub fn used_fonts(p: &Presentation) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |f: String| {
        let f = f.trim().to_string();
        if !f.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&f)) {
            out.push(f);
        }
    };
    for slide in &p.slides {
        let Some(ctx) = Ctx::for_slide(p, slide) else { continue };
        deckcraft_model::walk(&slide.shapes, &mut |s, _| {
            if let Some(body) = &s.text {
                body_fonts(&ctx, s, body, &mut add);
            }
            if let ShapeKind::Table(t) = &s.kind {
                for cell in t.rows.iter().flat_map(|r| &r.cells) {
                    body_fonts(&ctx, s, &cell.text, &mut add);
                }
            }
        });
    }
    out
}

fn body_fonts(ctx: &Ctx, s: &Shape, body: &TextBody, add: &mut dyn FnMut(String)) {
    for para in &body.paragraphs {
        let has_text = para.runs.iter().any(|r| !r.text.trim().is_empty());
        if !has_text {
            continue;
        }
        for run in para.runs.iter().filter(|r| !r.text.trim().is_empty()) {
            let r = resolve::run(ctx, s, para, &run.props);
            add(resolve::font_family(ctx, &r));
            let slots: Vec<FontSlot> = run.text.chars().filter_map(slot_of).collect();
            if slots.contains(&FontSlot::EastAsian)
                && let Some(f) = east_asian_family(ctx, &r)
            {
                add(f);
            }
            if slots.contains(&FontSlot::ComplexScript)
                && let Some(f) = slot_family(ctx, &r, r.font_cs.as_deref(), "cs")
            {
                add(f);
            }
        }
        let pp = resolve::para(ctx, s, para);
        if matches!(pp.bullet, Some(Bullet::Char { .. }))
            && let Some(f) = pp.bullet_font.as_deref()
        {
            add(ctx.master.theme.font(f));
        }
    }
}

/// A run's East Asian or complex-script font (`slot` "ea" or "cs"): its own (`explicit`) or the
/// theme's heading or body one (`+mj-ea`, `+mn-cs`…), theme references resolved; `None` when
/// empty.
fn slot_family(ctx: &Ctx, r: &RunProps, explicit: Option<&str>, slot: &str) -> Option<String> {
    let major = r.font.as_deref().is_some_and(|f| f.starts_with("+mj"));
    let theme_ref = format!("{}-{slot}", if major { "+mj" } else { "+mn" });
    let name = ctx.master.theme.font(explicit.unwrap_or(&theme_ref));
    (!name.trim().is_empty()).then_some(name)
}

/// The font a run's East Asian text is drawn in: its `a:ea` font (or the theme's), else, for
/// Japanese text (`lang` or `altLang` ja-JP), the theme's Japanese font (`a:font script="Jpan"`),
/// as PowerPoint fills an empty East Asian font for Japanese runs.
fn east_asian_family(ctx: &Ctx, r: &RunProps) -> Option<String> {
    if let Some(f) = slot_family(ctx, r, r.font_ea.as_deref(), "ea") {
        return Some(f);
    }
    let japanese = [r.lang.as_deref(), r.alt_lang.as_deref()].into_iter().flatten().any(|l| CjkLocale::from_lang(l) == Some(CjkLocale::Japanese));
    if !japanese {
        return None;
    }
    let fonts = &ctx.master.theme.fonts;
    let set = if r.font.as_deref().is_some_and(|f| f.starts_with("+mj")) { &fonts.major } else { &fonts.minor };
    set.script(CjkLocale::Japanese.theme_script()).map(str::to_string)
}

/// Replace the font family `from` with `to` everywhere it is named explicitly: text runs (and
/// their east Asian, complex script and symbol faces), bullet fonts, paragraph end marks, list
/// styles of shapes, layouts and masters, the master text styles, the deck default style, speaker
/// notes, tables and the theme fonts. Names compare case-insensitively; theme references
/// (`+mn-lt`) are left alone. Returns how many names changed.
pub fn replace_font(p: &mut Presentation, from: &str, to: &str) -> usize {
    use std::sync::Arc;
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() || to.is_empty() || from.starts_with('+') || from == to {
        return 0;
    }
    let mut n = 0usize;
    let mut fix = |f: &mut Option<String>| {
        if f.as_deref().is_some_and(|x| x.trim().eq_ignore_ascii_case(from)) {
            *f = Some(to.to_string());
            n = n.saturating_add(1);
        }
    };
    replace_list(&mut p.default_text_style, &mut fix);
    for slide in &mut p.slides {
        let sl = Arc::make_mut(slide);
        replace_shapes(&mut sl.shapes, &mut fix);
        replace_body(&mut sl.notes, &mut fix);
    }
    let mut theme_hits = 0usize;
    let masters = p.masters.iter_mut().chain(p.notes_master.iter_mut()).chain(p.handout_master.iter_mut());
    for m in masters {
        let m = Arc::make_mut(m);
        replace_shapes(&mut m.shapes, &mut fix);
        for l in [&mut m.title_style, &mut m.body_style, &mut m.other_style] {
            replace_list(l, &mut fix);
        }
        for lay in &mut m.layouts {
            replace_shapes(&mut lay.shapes, &mut fix);
        }
        for set in [&mut m.theme.fonts.major, &mut m.theme.fonts.minor] {
            for face in [&mut set.latin, &mut set.ea, &mut set.cs].into_iter().chain(set.scripts.iter_mut().map(|s| &mut s.typeface)) {
                if face.trim().eq_ignore_ascii_case(from) {
                    *face = to.to_string();
                    theme_hits = theme_hits.saturating_add(1);
                }
            }
        }
    }
    n.saturating_add(theme_hits)
}

fn replace_shapes(shapes: &mut [Shape], fix: &mut dyn FnMut(&mut Option<String>)) {
    for s in shapes {
        if let Some(body) = &mut s.text {
            replace_body(body, fix);
        }
        if let ShapeKind::Table(t) = &mut s.kind {
            for cell in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                replace_body(&mut cell.text, fix);
            }
        }
        if let Some(children) = s.children_mut() {
            replace_shapes(children, fix);
        }
    }
}

fn replace_body(body: &mut TextBody, fix: &mut dyn FnMut(&mut Option<String>)) {
    replace_list(&mut body.list_style, fix);
    for para in &mut body.paragraphs {
        fix(&mut para.props.bullet_font);
        replace_run(&mut para.end_props, fix);
        for run in &mut para.runs {
            replace_run(&mut run.props, fix);
        }
    }
}

fn replace_list(l: &mut deckcraft_model::text::ListStyle, fix: &mut dyn FnMut(&mut Option<String>)) {
    for lvl in l.levels.iter_mut().flatten() {
        fix(&mut lvl.para.bullet_font);
        replace_run(&mut lvl.run, fix);
    }
}

fn replace_run(r: &mut deckcraft_model::text::RunProps, fix: &mut dyn FnMut(&mut Option<String>)) {
    for f in [&mut r.font, &mut r.font_ea, &mut r.font_cs, &mut r.font_sym] {
        fix(f);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Arc;

    use deckcraft_model::text::{RunProps, TextBody};

    use super::*;

    /// A deck whose first slide holds one run of `text` set by `props` (its Latin font one this
    /// machine has unless `props` names another).
    fn deck(text: &str, props: RunProps) -> Presentation {
        let mut p = Presentation::default();
        let have = FontDb::global().face(deckcraft_fonts::FALLBACK_FAMILY, "Regular").family.clone();
        let slide = Arc::make_mut(&mut p.slides[0]);
        let mut body = TextBody::from_text(text);
        body.paragraphs[0].runs[0].props = RunProps { font: props.font.clone().or(Some(have)), ..props };
        slide.shapes[0].text = Some(body);
        p
    }

    #[test]
    fn missing_east_asian_fonts_are_listed_for_text_in_them() {
        let ea = RunProps { font_ea: Some("Qqq 存在しない明朝".into()), ..Default::default() };
        let p = deck("日本語のテキスト", ea.clone());
        assert_eq!(missing_fonts(&p), ["Qqq 存在しない明朝"]);
        assert!(used_fonts(&p).contains(&"Qqq 存在しない明朝".to_string()));
        // A run without East Asian text isn't drawn in its East Asian font.
        assert!(missing_fonts(&deck("Latin only", ea)).is_empty());
        // Complex-script text names its `a:cs` font.
        let cs = RunProps { font_cs: Some("Qqq Missing Naskh".into()), ..Default::default() };
        assert_eq!(missing_fonts(&deck("سلام", cs)), ["Qqq Missing Naskh"]);
    }

    /// Japanese text with no East Asian font of its own is drawn in the theme's Japanese font
    /// (`a:font script="Jpan"`) when its `lang` or `altLang` is ja-JP, and that font is listed.
    #[test]
    fn japanese_text_lists_the_themes_japanese_font() {
        let mut p = deck("日本語", RunProps { lang: Some("ja-JP".into()), ..Default::default() });
        let m = Arc::make_mut(&mut p.masters[0]);
        m.theme.fonts.minor.ea.clear();
        m.theme.fonts.minor.scripts.push(deckcraft_model::theme::ScriptFont { script: "Jpan".into(), typeface: "Qqq テーマ ゴシック".into() });
        let master = p.masters[0].clone();
        assert_eq!(missing_fonts(&p), ["Qqq テーマ ゴシック"]);
        let with = |props: RunProps| {
            let mut q = deck("日本語", props);
            q.masters[0] = master.clone();
            missing_fonts(&q)
        };
        assert_eq!(with(RunProps { lang: Some("en-US".into()), alt_lang: Some("ja-JP".into()), ..Default::default() }), ["Qqq テーマ ゴシック"]);
        assert!(with(RunProps { lang: Some("en-US".into()), ..Default::default() }).is_empty(), "not Japanese text");
        assert!(with(RunProps { lang: Some("zh-CN".into()), ..Default::default() }).is_empty());
    }

    /// An installed font named in Japanese, as Japanese PowerPoint writes it, isn't missing.
    /// Skipped where the font isn't installed.
    #[test]
    fn installed_fonts_named_in_japanese_are_not_missing() {
        let db = FontDb::global();
        let mut checked = 0;
        for (japanese, english) in
            [("ヒラギノ角ゴシック", "Hiragino Sans"), ("メイリオ", "Meiryo"), ("ＭＳ Ｐゴシック", "MS PGothic"), ("游ゴシック", "Yu Gothic")]
        {
            if !db.has_family(english) {
                continue;
            }
            checked += 1;
            let p = deck("日本語 text", RunProps { font: Some(japanese.into()), font_ea: Some(japanese.into()), ..Default::default() });
            assert!(missing_fonts(&p).is_empty(), "{japanese}");
        }
        if checked == 0 {
            eprintln!("skipped: no Japanese font installed");
        }
    }
}
