//! Fonts a presentation uses that this machine doesn't have, so opening it can say so (its text
//! draws in a stand-in; see `FontDb::document_face`).

use deckcraft_fonts::FontDb;
use deckcraft_model::resolve::{self, Ctx};
use deckcraft_model::text::{Bullet, TextBody};
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

/// The font families the slides' text is drawn in (theme fonts resolved, bullet fonts included)
/// that aren't installed or bundled, sorted, without duplicates (case-insensitive).
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

/// The families the slides' text runs and bullets are drawn in.
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

/// The fonts of each run with text: its Latin font, and the East Asian font of a run with
/// Japanese, Chinese or Korean text (its `a:ea`, the theme's, or the theme's font for its
/// language) and the complex-script font of a run with Arabic, Hebrew, Thai… text, as
/// PowerPoint's Replace Fonts lists the East Asian fonts a deck uses too.
fn body_fonts(ctx: &Ctx, s: &Shape, body: &TextBody, add: &mut dyn FnMut(String)) {
    use deckcraft_fonts::FontSlot;
    for para in &body.paragraphs {
        let has_text = para.runs.iter().any(|r| !r.text.trim().is_empty());
        if !has_text {
            continue;
        }
        for run in para.runs.iter().filter(|r| !r.text.trim().is_empty()) {
            let r = resolve::run(ctx, s, para, &run.props);
            add(resolve::font_family(ctx, &r));
            let has = |slot: FontSlot| run.text.chars().any(|c| deckcraft_fonts::script::slot_of(c) == Some(slot));
            if has(FontSlot::EastAsian)
                && let Some(f) = crate::cmd::format::east_asian_family(ctx, &r)
            {
                add(f);
            }
            if has(FontSlot::ComplexScript) {
                let major = r.font.as_deref().is_some_and(|f| f.starts_with("+mj"));
                add(ctx.master.theme.font(r.font_cs.as_deref().unwrap_or(if major { "+mj-cs" } else { "+mn-cs" })));
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
    for lvl in l.levels.iter_mut().flatten().chain(l.default.iter_mut()) {
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
    use deckcraft_model::ShapeId;
    use serde_json::json;

    use crate::Session;

    fn text_shape(s: &mut Session, text: &str) -> ShapeId {
        let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [40.0, 40.0, 400.0, 80.0], "text": text})).unwrap();
        ShapeId(r["id"].as_u64().unwrap() as u32)
    }

    fn used(s: &mut Session) -> Vec<String> {
        serde_json::from_value(s.execute("font.used", &json!({})).unwrap()).unwrap()
    }

    #[test]
    fn east_asian_fonts_are_listed_reported_missing_and_replaced() {
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語 and English");
        s.execute("format.font", &json!({"eastAsian": "テスト明朝"})).unwrap();
        let fonts = used(&mut s);
        assert!(fonts.iter().any(|f| f == "テスト明朝"), "{fonts:?}");
        assert!(fonts.iter().any(|f| f == "Inter"), "{fonts:?}");
        assert!(super::missing_fonts(&s.doc().unwrap().doc).iter().any(|f| f == "テスト明朝"), "a missing Japanese font is reported");

        let r = s.execute("font.replace", &json!({"from": "テスト明朝", "to": "BIZ UDPGothic"})).unwrap();
        assert!(r["replaced"].as_u64().is_some_and(|n| n >= 1), "{r}");
        let run = s.doc().unwrap().shape(id).unwrap().text.clone().unwrap().paragraphs[0].runs[0].props.clone();
        assert_eq!(run.font_ea.as_deref(), Some("BIZ UDPGothic"));
        assert!(!used(&mut s).iter().any(|f| f == "テスト明朝"));

        // A complex-script run lists its complex-script font.
        let other = text_shape(&mut s, "مرحبا");
        s.execute("edit.deselect", &json!({})).unwrap();
        s.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).unwrap();
            let sh = deckcraft_model::find_shape_mut(list, other).unwrap();
            sh.text.as_mut().unwrap().paragraphs[0].runs[0].props.font_cs = Some("Test Naskh".into());
            Ok(())
        })
        .unwrap();
        assert!(used(&mut s).iter().any(|f| f == "Test Naskh"));
    }

    #[test]
    fn a_japanese_decks_theme_font_is_listed_for_its_japanese_text() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        text_shape(&mut s, "日本語");
        assert!(used(&mut s).iter().any(|f| f == "BIZ UDPGothic"));
        // The deck's default style (a:defPPr) takes part in Replace Fonts.
        s.edit(|doc, _| {
            if let Some(d) = doc.default_text_style.default.as_mut() {
                d.run.font_ea = Some("Old EA".into());
            }
            Ok(())
        })
        .unwrap();
        s.execute("font.replace", &json!({"from": "Old EA", "to": "BIZ UDMincho"})).unwrap();
        assert_eq!(s.doc().unwrap().doc.default_text_style.default.clone().and_then(|d| d.run.font_ea).as_deref(), Some("BIZ UDMincho"));
    }
}
