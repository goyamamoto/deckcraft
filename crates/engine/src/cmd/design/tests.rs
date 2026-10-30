use std::sync::Arc;

use deckcraft_color::SchemeSlot;
use deckcraft_model::style::PictureMode;
use deckcraft_model::theme::{FontScheme, ScriptFont, builtin_color_schemes, builtin_font_schemes};
use deckcraft_model::{Background, Fill};
use serde_json::{Value, json};

use crate::{Session, UiRequest};

fn scheme(s: &Session) -> deckcraft_color::ColorScheme {
    s.doc().unwrap().doc.masters[0].theme.colors.clone()
}

fn font_scheme(s: &Session) -> FontScheme {
    s.doc().unwrap().doc.masters[0].theme.fonts.clone()
}

fn undo_len(s: &Session) -> usize {
    s.doc().unwrap().history.undo.len()
}

fn png() -> String {
    let mut img = image::RgbaImage::new(2, 2);
    img.put_pixel(0, 0, image::Rgba([10, 20, 30, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    crate::cmd::base64_encode(&bytes)
}

fn picture(s: &Session) -> deckcraft_model::style::PictureFill {
    match &s.doc().unwrap().doc.slides[0].background {
        Some(Background::Fill { fill: Fill::Picture(pf) }) => pf.clone(),
        other => panic!("expected a picture background, got {other:?}"),
    }
}

#[test]
fn layout_commands_reject_invalid_index() {
    let mut s = Session::with_new();
    s.execute("master.insertLayout", &json!({})).unwrap();
    let count = |s: &Session| s.doc().unwrap().doc.masters.iter().map(|m| m.layouts.len()).sum::<usize>();
    let names = |s: &Session| s.doc().unwrap().doc.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.name.clone())).collect::<Vec<_>>();
    let n = count(&s);
    let before = names(&s);
    for bad in [json!(-1), json!(1.5)] {
        assert!(s.execute("master.deleteLayout", &json!({"layout": bad})).is_err());
        assert!(s.execute("master.deleteLayout", &json!({"master": bad})).is_err());
        assert!(s.execute("master.renameLayout", &json!({"layout": bad, "name": "x"})).is_err());
        assert_eq!(count(&s), n);
        assert_eq!(names(&s), before);
    }
    s.execute("master.deleteLayout", &json!({})).unwrap();
    assert_eq!(count(&s), n - 1);
}

#[test]
fn civil_dates() {
    assert_eq!(super::civil(0), (1970, 1, 1));
    assert_eq!(super::civil(20_734), (2026, 10, 8));
    // Past `NaiveDate`'s range the 400-year cycles carry the year, exactly, up to the extremes.
    assert_eq!(super::civil(95_026_602), (262_144, 1, 1));
    assert_eq!(super::civil(-96_465_964), (-262_145, 3, 1));
    assert_eq!(super::civil(i64::MAX), (25_252_734_927_768_524, 7, 27));
    assert_eq!(super::civil(i64::MIN), (-25_252_734_927_764_585, 6, 7));
}

#[test]
fn colors_dialog_apply_and_reject() {
    let mut s = Session::with_new();
    let original = scheme(&s);
    let n = undo_len(&s);
    s.execute("design.colors", &json!({})).unwrap();
    s.execute("design.colors", &Value::Null).unwrap();
    assert_eq!(scheme(&s), original);
    assert_eq!(undo_len(&s), n, "opening the dialog does not edit");
    assert_eq!(s.ui_requests.len(), 2);
    match &s.ui_requests[0] {
        UiRequest::Dialog { id, params } => {
            assert_eq!(id, "themeColors");
            assert_eq!(params["name"], "DeckCraft");
            let colors = params["colors"].as_object().unwrap();
            assert_eq!(colors.len(), 12);
            assert_eq!(colors["accent1"].as_str().unwrap(), format!("#{}", original.get(SchemeSlot::Accent1).hex()));
            assert!(colors.contains_key("folHlink"));
        }
        other => panic!("expected a dialog, got {other:?}"),
    }

    s.execute("design.colors", &json!({"name": "Harbor"})).unwrap();
    let harbor = builtin_color_schemes().into_iter().find(|c| c.name == "Harbor").unwrap();
    assert_eq!(scheme(&s).name, "Harbor");
    assert_eq!(scheme(&s).get(SchemeSlot::Accent1), harbor.get(SchemeSlot::Accent1));
    assert_eq!(undo_len(&s), n + 1);
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Colors");
    s.execute("design.colors", &json!({"name": "Harbor"})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "the same scheme is not an undo step");
    colors_dialog_rejects(&mut s, n, &harbor);
}

fn colors_dialog_rejects(s: &mut Session, n: usize, harbor: &deckcraft_color::ColorScheme) {
    let err = s.execute("design.colors", &json!({"name": "Nope"})).unwrap_err().to_string();
    assert!(err.contains("no colour scheme `Nope`"), "{err}");
    assert!(s.execute("design.colors", &json!({"name": "  "})).is_err());
    assert!(s.execute("design.colors", &json!({"name": 1})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {}})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": "red"})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {"bg1": "#112233"}})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {"tx1": "#112233"}})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {"phClr": "#112233"}})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {"nope": "#112233"}})).is_err());
    assert!(s.execute("design.colors", &json!({"colors": {"accent1": "zz"}})).is_err());
    assert!(s.execute("design.colors", &json!({"name": "Harbor", "colors": {}})).is_err());
    assert_eq!(scheme(s).name, "Harbor");
    assert_eq!(scheme(s).get(SchemeSlot::Accent1), harbor.get(SchemeSlot::Accent1));
    assert_eq!(undo_len(s), n + 1);
    colors_dialog_custom_overrides(s, n, harbor);
}

fn colors_dialog_custom_overrides(s: &mut Session, n: usize, harbor: &deckcraft_color::ColorScheme) {
    s.execute("design.colors", &json!({"colors": {"accent1": "112233"}})).unwrap();
    assert_eq!(scheme(s).name, "Custom");
    assert_eq!(scheme(s).get(SchemeSlot::Accent1).hex(), "112233");
    assert_eq!(scheme(s).get(SchemeSlot::Accent2), harbor.get(SchemeSlot::Accent2));
    assert_eq!(undo_len(s), n + 2);

    let meadow = builtin_color_schemes().into_iter().find(|c| c.name == "Meadow").unwrap();
    s.execute("design.colors", &json!({"name": "Meadow", "colors": {"lt1": "#010203"}})).unwrap();
    assert_eq!(scheme(s).name, "Meadow");
    assert_eq!(scheme(s).get(SchemeSlot::Lt1).hex(), "010203");
    assert_eq!(scheme(s).get(SchemeSlot::Accent1), meadow.get(SchemeSlot::Accent1));
    assert_eq!(undo_len(s), n + 3);

    s.execute("design.colors", &json!({"name": "Talk", "colors": {"dk1": "#ABCDEF"}})).unwrap();
    assert_eq!(scheme(s).name, "Talk");
    assert_eq!(scheme(s).get(SchemeSlot::Dk1).hex(), "ABCDEF");
    assert_eq!(scheme(s).get(SchemeSlot::Lt1).hex(), "010203");
    assert_eq!(scheme(s).get(SchemeSlot::Accent1), meadow.get(SchemeSlot::Accent1));

    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(scheme(s).name, "Meadow");
    assert_eq!(scheme(s).get(SchemeSlot::Dk1), meadow.get(SchemeSlot::Dk1));
}

#[test]
fn fonts_dialog_apply_and_reject() {
    let mut s = Session::with_new();
    let original = font_scheme(&s);
    let n = undo_len(&s);
    s.execute("design.fonts", &json!({})).unwrap();
    s.execute("design.fonts", &Value::Null).unwrap();
    s.execute("design.fonts", &json!([1, 2])).unwrap();
    assert_eq!(font_scheme(&s), original);
    assert_eq!(undo_len(&s), n, "opening the dialog does not edit");
    assert_eq!(s.ui_requests.len(), 3);
    match &s.ui_requests[0] {
        UiRequest::Dialog { id, params } => {
            assert_eq!(id, "themeFonts");
            assert_eq!(params["name"], original.name);
            assert_eq!(params["major"], original.major.latin);
            assert_eq!(params["minor"], original.minor.latin);
        }
        other => panic!("expected a dialog, got {other:?}"),
    }

    s.execute("design.fonts", &json!({"name": "classic"})).unwrap();
    let classic = builtin_font_schemes().into_iter().find(|f| f.name == "Classic").unwrap();
    assert_eq!(font_scheme(&s), classic, "a builtin name keeps that scheme, including its own name");
    assert_eq!(undo_len(&s), n + 1);
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Fonts");
    s.execute("design.fonts", &json!({"name": "Classic"})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "the same scheme is not an undo step");

    let err = s.execute("design.fonts", &json!({"name": "Nope"})).unwrap_err().to_string();
    assert!(err.contains("no font scheme `Nope`"), "{err}");
    assert!(s.execute("design.fonts", &json!({"name": "  "})).is_err());
    assert!(s.execute("design.fonts", &json!({"name": 1})).is_err());
    assert!(s.execute("design.fonts", &json!({"major": ""})).is_err());
    assert!(s.execute("design.fonts", &json!({"major": "  "})).is_err());
    assert!(s.execute("design.fonts", &json!({"minor": 1})).is_err());
    assert!(s.execute("design.fonts", &json!({"name": "Classic", "major": ""})).is_err());
    assert_eq!(font_scheme(&s), classic);
    assert_eq!(undo_len(&s), n + 1);
    fonts_dialog_custom_overrides(&mut s);
}

fn fonts_dialog_custom_overrides(s: &mut Session) {
    s.edit(|doc, _| {
        let fonts = &mut std::sync::Arc::make_mut(&mut doc.masters[0]).theme.fonts;
        fonts.major.ea = "EA Major".into();
        fonts.minor.cs = "CS Minor".into();
        fonts.major.scripts.push(ScriptFont { script: "Arab".into(), typeface: "Noto Naskh Arabic".into() });
        Ok(())
    })
    .unwrap();
    let marked = font_scheme(s);
    let n = undo_len(s);

    s.execute("design.fonts", &json!({"major": " Lato "})).unwrap();
    let custom = font_scheme(s);
    assert_eq!(custom.name, "Custom");
    assert_eq!(custom.major.latin, "Lato");
    assert_eq!(custom.minor.latin, "Lato", "an omitted minor copies major");
    assert_eq!(custom.major.ea, "EA Major");
    assert_eq!(custom.minor.cs, "CS Minor");
    assert_eq!(custom.major.scripts, marked.major.scripts);
    assert_eq!(undo_len(s), n + 1);

    s.execute("design.fonts", &json!({"minor": "Roboto"})).unwrap();
    assert_eq!(font_scheme(s).name, "Custom");
    assert_eq!(font_scheme(s).major.latin, "Lato");
    assert_eq!(font_scheme(s).minor.latin, "Roboto");
    assert_eq!(font_scheme(s).major.ea, "EA Major");
    assert_eq!(undo_len(s), n + 2);

    s.execute("design.fonts", &json!({"name": "Classic", "major": "Inter"})).unwrap();
    assert_eq!(font_scheme(s).name, "Classic");
    assert_eq!(font_scheme(s).major.latin, "Inter");
    assert_eq!(font_scheme(s).minor.latin, "Open Sans");
    assert_eq!(font_scheme(s).major.ea, "EA Major", "a Latin-only builtin keeps the deck's East Asian font");
    assert_eq!(font_scheme(s).minor.cs, "CS Minor");
    assert_eq!(font_scheme(s).major.scripts, marked.major.scripts, "and its script fonts");
    assert_eq!(undo_len(s), n + 3);

    s.edit(|doc, _| {
        std::sync::Arc::make_mut(&mut doc.masters[0]).theme.fonts.major.ea = "Kept".into();
        Ok(())
    })
    .unwrap();
    s.execute("design.fonts", &json!({"name": "Talk", "minor": "Nunito Sans"})).unwrap();
    assert_eq!(font_scheme(s).name, "Talk");
    assert_eq!(font_scheme(s).major.latin, "Inter");
    assert_eq!(font_scheme(s).minor.latin, "Nunito Sans");
    assert_eq!(font_scheme(s).major.ea, "Kept");
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Fonts");

    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(font_scheme(s).name, "Classic");
    assert_eq!(font_scheme(s).major.ea, "Kept");
    assert_eq!(font_scheme(s).minor.latin, "Open Sans");
}

#[test]
fn a_japanese_deck_keeps_its_theme_fonts_across_theme_and_font_changes() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
    assert_eq!(font_scheme(&s).minor.script("Jpan"), Some("BIZ UDPGothic"));
    s.edit(|doc, _| {
        std::sync::Arc::make_mut(&mut doc.masters[0]).theme.fonts.minor.ea = "Body EA".into();
        Ok(())
    })
    .unwrap();
    s.execute("design.theme", &json!({"name": "Harbor"})).unwrap();
    let f = font_scheme(&s);
    assert_eq!((f.major.latin.as_str(), f.minor.latin.as_str()), ("Merriweather", "Open Sans"), "the theme's Latin fonts apply");
    assert_eq!(f.major.script("Jpan"), Some("BIZ UDPGothic"));
    assert_eq!(f.minor.script("Jpan"), Some("BIZ UDPGothic"));
    assert_eq!(f.minor.ea, "Body EA");
    assert_eq!(s.doc().unwrap().doc.masters[0].theme.name, "Harbor");
    s.execute("design.fonts", &json!({"name": "Neutral"})).unwrap();
    let f = font_scheme(&s);
    assert_eq!(f.name, "Neutral");
    assert_eq!(f.minor.latin, "Roboto");
    assert_eq!(f.minor.script("Jpan"), Some("BIZ UDPGothic"));
    assert_eq!(f.minor.ea, "Body EA");
    // The deck goes to PowerPoint with its Jpan font.
    let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
    let back = deckcraft_pptx::import(&bytes).unwrap();
    assert_eq!(back.masters[0].theme.fonts.minor.script("Jpan"), Some("BIZ UDPGothic"));
}

fn part(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}

#[test]
fn a_new_deck_follows_the_sessions_east_asian_editing_language() {
    // The desktop app sets the editing language from the system; File › New then starts in it.
    // The field, not set_east_asian_editing: that also sets the process-wide default font
    // tradition, which tests running in parallel would see.
    let mut s = Session::new();
    s.east_asian_editing = Some(deckcraft_fonts::CjkLocale::Japanese);
    s.execute("file.new", &json!({})).unwrap();
    let lang = s.doc().unwrap().doc.default_text_style.default.clone().and_then(|d| d.run.lang);
    assert_eq!(lang.as_deref(), Some("ja-JP"));
    // An explicit "" still makes a deck without one.
    s.execute("file.new", &json!({"eastAsian": ""})).unwrap();
    assert!(s.doc().unwrap().doc.default_text_style.default.is_none());
}

#[test]
fn a_new_deck_carries_powerpoints_asian_typography_and_japanese_editing_makes_it_japanese() {
    // Every new deck: eaLnBrk/latinLnBrk/hangingPunct on its default and master text styles.
    let s = Session::with_new();
    let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
    for name in ["ppt/presentation.xml", "ppt/slideMasters/slideMaster1.xml"] {
        let xml = part(&bytes, name);
        for attr in [r#"eaLnBrk="1""#, r#"latinLnBrk="0""#, r#"hangingPunct="1""#] {
            assert!(xml.contains(attr), "{name} lacks {attr}");
        }
    }
    assert!(!part(&bytes, "ppt/presentation.xml").contains("ja-JP"), "no East Asian language unless asked");
    assert!(s.doc().unwrap().doc.default_text_style.default.is_none());

    // With Japanese editing: the deck is in Japanese and its theme names a Jpan font.
    let mut s = Session::new();
    let r = s.execute("file.new", &json!({"eastAsian": "ja-JP", "theme": "Ember"})).unwrap();
    assert_eq!(r["slides"], 1);
    let doc = s.doc().unwrap().doc.clone();
    let def = doc.default_text_style.default.clone().unwrap();
    assert_eq!((def.run.lang.as_deref(), def.run.kumimoji), (Some("ja-JP"), Some(true)));
    let fonts = &doc.masters[0].theme.fonts;
    assert_eq!((fonts.major.script("Jpan"), fonts.minor.script("Jpan")), (Some("BIZ UDPGothic"), Some("BIZ UDPGothic")));
    assert_eq!(fonts.major.latin, "Montserrat", "the chosen theme's Latin fonts");
    let bytes = deckcraft_pptx::export(&doc).unwrap();
    let pres = part(&bytes, "ppt/presentation.xml");
    assert!(pres.contains("<a:defPPr>") && pres.contains(r#"lang="ja-JP""#), "{pres}");
    assert!(part(&bytes, "ppt/theme/theme1.xml").contains(r#"script="Jpan" typeface="BIZ UDPGothic""#));
    let back = deckcraft_pptx::import(&bytes).unwrap();
    assert_eq!(back.default_text_style.default.and_then(|d| d.run.lang).as_deref(), Some("ja-JP"));

    // A template starts in Japanese too; Korean gets the language only.
    let starter = crate::templates::starters().first().map(|t| t.name).unwrap();
    s.execute("file.new", &json!({"eastAsian": "ja", "template": starter})).unwrap();
    assert_eq!(s.doc().unwrap().doc.default_text_style.default.clone().and_then(|d| d.run.lang).as_deref(), Some("ja-JP"));
    s.execute("file.new", &json!({"eastAsian": "ko-KR"})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    assert_eq!(doc.default_text_style.default.clone().and_then(|d| d.run.lang).as_deref(), Some("ko-KR"));
    assert!(doc.masters[0].theme.fonts.minor.scripts.is_empty());
    for none in [json!({"eastAsian": ""}), json!({"eastAsian": "none"}), json!({"eastAsian": null}), json!({})] {
        s.execute("file.new", &none).unwrap();
        assert!(s.doc().unwrap().doc.default_text_style.default.is_none(), "{none}");
    }
    let docs = s.documents().len();
    assert!(s.execute("file.new", &json!({"eastAsian": "fr-FR"})).is_err());
    assert!(s.execute("file.new", &json!({"eastAsian": 1})).is_err());
    assert_eq!(s.documents().len(), docs, "a bad language opens no deck");
}

#[test]
fn theme_fonts_take_asian_text_heading_and_body_fonts() {
    let mut s = Session::with_new();
    s.ui_requests.clear();
    s.execute("design.fonts", &json!({})).unwrap();
    match s.ui_requests.last() {
        Some(UiRequest::Dialog { id, params }) => {
            assert_eq!(id, "themeFonts");
            assert_eq!(params["majorEa"], "");
            assert_eq!(params["minorEa"], "");
        }
        other => panic!("expected a dialog, got {other:?}"),
    }
    let n = undo_len(&s);
    s.execute("design.fonts", &json!({"majorEa": "BIZ UDPGothic", "minorEa": "BIZ UDMincho"})).unwrap();
    let f = font_scheme(&s);
    assert_eq!((f.major.ea.as_str(), f.minor.ea.as_str()), ("BIZ UDPGothic", "BIZ UDMincho"));
    assert_eq!(f.major.latin, "Inter", "the Latin fonts stay");
    assert_eq!(f.name, "Custom");
    assert_eq!(undo_len(&s), n + 1);
    s.execute("design.fonts", &json!({"major": "Lato", "minor": "Roboto", "majorEa": "Head JP"})).unwrap();
    let f = font_scheme(&s);
    assert_eq!(
        (f.major.latin.as_str(), f.minor.latin.as_str(), f.major.ea.as_str(), f.minor.ea.as_str()),
        ("Lato", "Roboto", "Head JP", "BIZ UDMincho")
    );
    assert_eq!(undo_len(&s), n + 2, "one undo step for the whole scheme");
    assert!(s.execute("design.fonts", &json!({"minorEa": ""})).is_err());
    assert!(s.execute("design.fonts", &json!({"majorEa": 3})).is_err());
    assert_eq!(undo_len(&s), n + 2);
    let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
    let back = deckcraft_pptx::import(&bytes).unwrap();
    assert_eq!(back.masters[0].theme.fonts.major.ea, "Head JP");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(font_scheme(&s).major.ea, "BIZ UDPGothic");
}

#[test]
fn background_picture_options_tile_transparency_and_copy() {
    let mut s = Session::with_new();
    let n = undo_len(&s);
    let err = s.execute("design.background", &json!({"pictureProps": {"tile": true}})).unwrap_err().to_string();
    assert!(err.contains("picture background"), "{err}");
    assert!(s.execute("design.background", &json!({"transparency": 0.2})).is_err());
    assert_eq!(undo_len(&s), n);

    s.execute("design.background", &json!({"color": "accent1", "transparency": 0.25})).unwrap();
    match &s.doc().unwrap().doc.slides[0].background {
        Some(Background::Fill { fill: Fill::Solid { color } }) => assert!((color.alpha() - 0.75).abs() < 1e-4, "{}", color.alpha()),
        other => panic!("expected a solid background, got {other:?}"),
    }
    let n = undo_len(&s);
    s.execute("design.background", &json!({"transparency": 0.25})).unwrap();
    assert_eq!(undo_len(&s), n, "the same transparency is not an undo step");

    s.execute("design.background", &json!({"picture": png(), "name": "dot.png", "transparency": 0.4, "pictureProps": {"tile": true}})).unwrap();
    let pf = picture(&s);
    assert!((pf.alpha.unwrap_or(1.0) - 0.6).abs() < 1e-6);
    assert!(matches!(pf.mode, PictureMode::Tile { .. }));
    let n = undo_len(&s);

    s.execute(
        "design.background",
        &json!({"pictureProps": {"offsetX": 12.0, "offsetY": -3.0, "scaleX": 0.5, "scaleY": 50.0, "align": "ctr", "mirror": "xy", "rotateWithShape": false}}),
    )
    .unwrap();
    match picture(&s).mode {
        PictureMode::Tile { tx, ty, sx, sy, flip, align } => {
            assert_eq!((tx, ty, sx, sy), (12.0, -3.0, 0.5, 10.0));
            assert_eq!(flip, "xy");
            assert_eq!(align, "ctr");
        }
        other => panic!("expected a tile, got {other:?}"),
    }
    assert!(picture(&s).upright);
    s.execute("design.background", &json!({"pictureProps": {"align": "bogus", "offsetX": "nope"}})).unwrap();
    match picture(&s).mode {
        PictureMode::Tile { tx, align, .. } => {
            assert_eq!(tx, 12.0);
            assert_eq!(align, "ctr");
        }
        other => panic!("expected the tile to stay, got {other:?}"),
    }
    assert_eq!(undo_len(&s), n + 1, "ignored options are not an undo step");

    s.execute("design.background", &json!({"pictureProps": {"tile": false, "stretch": [0.1, 0.2, 0.0, 0.0]}})).unwrap();
    match picture(&s).mode {
        PictureMode::Stretch { fill_rect } => assert_eq!(fill_rect, [0.1, 0.2, 0.0, 0.0]),
        other => panic!("expected a stretch, got {other:?}"),
    }

    s.execute("slide.new", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.slides[1].background.is_none());
    s.execute("slide.go", &json!({"index": 0})).unwrap();
    let n = undo_len(&s);
    s.execute("design.background", &json!({"copy": true})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides[1].background, s.doc().unwrap().doc.slides[0].background);
    assert_eq!(undo_len(&s), n + 1);
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Format Background");
    s.execute("design.background", &json!({"copy": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "copying an already shared background is not an undo step");
}

#[test]
fn background_all_applies_to_master_and_new_slides_inherit() {
    let mut s = Session::with_new();
    s.execute("file.new", &json!({"theme": "Nocturne"})).unwrap();
    let s0 = s.execute("slide.inspect", &json!({"index": 0})).unwrap();
    assert_eq!(s0["background"], "#121826");

    let n = undo_len(&s);
    s.execute("design.background", &json!({"color": "#0D0D0D", "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1);

    // Idempotency: re-applying the same background to all is a no-op (no extra undo step)
    s.execute("design.background", &json!({"color": "#0D0D0D", "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "re-applying already applied background is not an undo step");

    // Slide 0 has new background, master has new background, layouts cleared
    let doc = &s.doc().unwrap().doc;
    assert!(matches!(doc.slides[0].background, Some(Background::Fill { .. })));
    assert_eq!(doc.masters[0].background, doc.slides[0].background);
    for l in &doc.masters[0].layouts {
        assert!(l.background.is_none());
    }

    // Newly added slides inherit the master background (fixes #891)
    s.execute("slide.new", &json!({"layout": "titleAndContent", "title": "new"})).unwrap();
    let s0_after = s.execute("slide.inspect", &json!({"index": 0})).unwrap();
    let s1_after = s.execute("slide.inspect", &json!({"index": 1})).unwrap();
    assert_eq!(s0_after["background"], "#0D0D0D");
    assert_eq!(s1_after["background"], "#0D0D0D");

    // Undo restores previous slide and master backgrounds
    s.execute("edit.undo", &json!({})).unwrap(); // undos slide.new
    assert_eq!(s.doc().unwrap().doc.slides.len(), 1);
    s.execute("edit.undo", &json!({})).unwrap(); // undos design.background
    let s0_reverted = s.execute("slide.inspect", &json!({"index": 0})).unwrap();
    assert_eq!(s0_reverted["background"], "#121826");
}

#[test]
fn background_copy_from_slide_following_master_preserves_master_and_layout_backgrounds() {
    let mut s = Session::with_new();
    s.execute("file.new", &json!({"theme": "Nocturne"})).unwrap();
    s.execute("slide.new", &json!({"layout": "titleAndContent"})).unwrap();

    let custom_layout_bg = Background::Fill { fill: Fill::solid(deckcraft_model::ColorRef::rgb(deckcraft_color::Rgba::rgb(0x22, 0x22, 0x22))) };
    s.edit(|doc, _| {
        let m = Arc::make_mut(&mut doc.masters[0]);
        m.layouts[0].background = Some(custom_layout_bg.clone());
        Ok(())
    })
    .unwrap();

    // Slide 0 follows master (background: None)
    assert!(s.doc().unwrap().doc.slides[0].background.is_none());
    // Give Slide 1 an explicit override
    s.execute("design.background", &json!({"color": "#FF0000", "index": 1})).unwrap();
    assert!(s.doc().unwrap().doc.slides[1].background.is_some());

    let orig_master_bg = s.doc().unwrap().doc.masters[0].background.clone();
    assert!(orig_master_bg.is_some());

    // Select slide 0 (which follows master) and apply copy to all
    s.execute("slide.go", &json!({"index": 0})).unwrap();
    let n = undo_len(&s);
    s.execute("design.background", &json!({"copy": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1);

    // Slide 1 has been reset to None (follows master), slide 0 remains None
    let doc = &s.doc().unwrap().doc;
    assert!(doc.slides[0].background.is_none());
    assert!(doc.slides[1].background.is_none());

    // Master and layout backgrounds survived!
    assert_eq!(doc.masters[0].background, orig_master_bg);
    assert_eq!(doc.masters[0].layouts[0].background, Some(custom_layout_bg));

    // Idempotency: re-applying copy is a no-op
    s.execute("design.background", &json!({"copy": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "re-copying when already matching is a no-op");
}

#[test]
fn background_reset_all_preserves_master_and_layout_backgrounds() {
    let mut s = Session::with_new();
    s.execute("file.new", &json!({"theme": "Nocturne"})).unwrap();
    s.execute("slide.new", &json!({"layout": "titleAndContent"})).unwrap();

    let custom_layout_bg = Background::Fill { fill: Fill::solid(deckcraft_model::ColorRef::rgb(deckcraft_color::Rgba::rgb(0x33, 0x33, 0x33))) };
    s.edit(|doc, _| {
        let m = Arc::make_mut(&mut doc.masters[0]);
        m.layouts[0].background = Some(custom_layout_bg.clone());
        Ok(())
    })
    .unwrap();

    let orig_master_bg = s.doc().unwrap().doc.masters[0].background.clone();
    assert!(orig_master_bg.is_some());

    // Set explicit backgrounds on both slides individually (master remains untouched)
    s.execute("design.background", &json!({"color": "#0D0D0D", "index": 0})).unwrap();
    s.execute("design.background", &json!({"color": "#0D0D0D", "index": 1})).unwrap();
    assert!(s.doc().unwrap().doc.slides[0].background.is_some());
    assert!(s.doc().unwrap().doc.slides[1].background.is_some());

    // Now perform reset with all
    let n = undo_len(&s);
    s.execute("design.background", &json!({"reset": true, "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1);

    // Slides reset to None to follow master
    let doc = &s.doc().unwrap().doc;
    assert!(doc.slides[0].background.is_none());
    assert!(doc.slides[1].background.is_none());

    // Master and layout backgrounds survived!
    assert_eq!(doc.masters[0].background, orig_master_bg);
    assert_eq!(doc.masters[0].layouts[0].background, Some(custom_layout_bg));

    // Idempotency: re-running reset all is a no-op
    s.execute("design.background", &json!({"reset": true, "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "re-resetting when already reset is a no-op");
}

#[test]
fn background_all_only_touches_used_masters() {
    let mut s = Session::with_new();
    s.execute("file.new", &json!({"theme": "Nocturne"})).unwrap();

    // Add a second master to presentation
    let second_master_bg = Background::Fill { fill: Fill::solid(deckcraft_model::ColorRef::rgb(deckcraft_color::Rgba::rgb(0x55, 0x55, 0x55))) };
    s.edit(|doc, _| {
        let mut m2 = (*doc.masters[0]).clone();
        m2.id = deckcraft_model::MasterId(999);
        m2.background = Some(second_master_bg.clone());
        doc.masters.push(Arc::new(m2));
        Ok(())
    })
    .unwrap();

    assert_eq!(s.doc().unwrap().doc.masters.len(), 2);
    // Slides only use master 0 (default layout)
    s.execute("design.background", &json!({"color": "#AA0000", "all": true})).unwrap();

    let doc = &s.doc().unwrap().doc;
    // Master 0 updated
    assert_eq!(doc.masters[0].background, doc.slides[0].background);
    // Master 1 preserved!
    assert_eq!(doc.masters[1].background, Some(second_master_bg));
}

#[test]
fn background_all_clears_layout_overrides_even_if_slides_and_master_match() {
    let mut s = Session::with_new();
    s.execute("file.new", &json!({"theme": "Nocturne"})).unwrap();

    // Set all slides and master to #AA0000
    s.execute("design.background", &json!({"color": "#AA0000", "all": true})).unwrap();
    assert_eq!(s.doc().unwrap().doc.masters[0].background, s.doc().unwrap().doc.slides[0].background);

    // Add a layout override on layout 0
    let custom_bg = Background::Fill { fill: Fill::solid(deckcraft_model::ColorRef::rgb(deckcraft_color::Rgba::rgb(0x12, 0x34, 0x56))) };
    s.edit(|doc, _| {
        let m = Arc::make_mut(&mut doc.masters[0]);
        m.layouts[0].background = Some(custom_bg);
        Ok(())
    })
    .unwrap();

    // Re-run Apply to All with #AA0000. Must not be skipped as no-op; must clear layout override.
    let n = undo_len(&s);
    s.execute("design.background", &json!({"color": "#AA0000", "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "should execute and clear layout override, not treat as no-op");

    let doc = &s.doc().unwrap().doc;
    assert!(doc.masters[0].layouts[0].background.is_none());

    // Now that layout override is cleared, repeating is an actual no-op
    s.execute("design.background", &json!({"color": "#AA0000", "all": true})).unwrap();
    assert_eq!(undo_len(&s), n + 1, "now it is truly a no-op");
}
