//! Lengths in the region's unit (PowerPoint in Japan measures in centimetres), the paper notes
//! pages print on, file names with decomposed kana, and `=`/`+` shortcuts on a JIS keyboard.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use deckcraft_engine::Session;
use deckcraft_geom::Region;
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

fn harness_with(s: Session) -> Harness<'static, SlideApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        SlideApp::new(s, Services::default()),
    );
    h.run_steps(3);
    h
}

/// A session in `region` with one rectangle (72 × 100 pt at 36, 18) selected.
fn session_in(region: &str) -> (Session, u64) {
    let mut s = Session::with_new();
    s.set_region(Region::from_tag(region));
    let id =
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [36.0, 18.0, 72.0, 100.0], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    let _ = s.execute("text.exit", &json!({}));
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    (s, id)
}

fn field(h: &Harness<'_, SlideApp>, key: &str) -> String {
    h.state().dialog.as_ref().and_then(|d| d.fields.get(key).cloned()).unwrap_or_default()
}

fn set_field(h: &mut Harness<'_, SlideApp>, key: &str, value: &str) {
    h.state_mut().dialog.as_mut().unwrap().fields.insert(key.into(), value.into());
    h.run_steps(1);
}

fn xfrm(h: &Harness<'_, SlideApp>, id: u64) -> deckcraft_geom::Xfrm {
    let st = h.state().session.active().unwrap();
    st.shape(deckcraft_model::ShapeId(id as u32)).and_then(|s| s.xfrm).unwrap()
}

fn ok(h: &mut Harness<'_, SlideApp>) {
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
}

/// Size and Position on a Japanese system shows centimetres, takes a number in centimetres or
/// with its own unit, and leaves fields that weren't edited exactly as they were.
#[test]
fn size_and_position_measures_in_centimetres_in_japan() {
    let (s, id) = session_in("ja-JP");
    let mut h = harness_with(s);
    h.state_mut().run("shape.sizePosition", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(
        (field(&h, "w"), field(&h, "h"), field(&h, "x"), field(&h, "y")),
        ("2.54 cm".into(), "3.53 cm".into(), "1.27 cm".into(), "0.64 cm".into())
    );
    assert!(h.query_by_label("Width:").is_some() && h.query_by_label("Horizontal:").is_some(), "labels without an inch mark");
    set_field(&mut h, "w", "5");
    set_field(&mut h, "x", "1in");
    ok(&mut h);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let x = xfrm(&h, id);
    assert!((x.w - 5.0 / 2.54 * 72.0).abs() < 1e-6, "{x:?}");
    assert_eq!((x.x, x.y, x.h), (72.0, 18.0, 100.0), "1 inch, and the untouched fields exactly as they were");
    // Not a length: the dialog stays open and says so.
    h.state_mut().run("shape.sizePosition", json!({})).unwrap();
    h.run_steps(2);
    set_field(&mut h, "h", "tall");
    ok(&mut h);
    assert!(h.state().dialog.is_some());
    assert_eq!(h.state().status.as_ref().map(|s| s.0.as_str()), Some("Height must be a length"));
}

/// The United States keeps inches, and Preferences can pick millimetres anywhere.
#[test]
fn size_and_position_follows_the_unit_setting() {
    let (s, id) = session_in("en-US");
    let mut h = harness_with(s);
    h.state_mut().run("shape.sizePosition", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(field(&h, "w"), "1\"");
    h.get_by_label("Cancel").click_accesskit();
    h.run_steps(1);
    h.state_mut().run("tools.units", json!({"value": "millimeters"})).unwrap();
    h.state_mut().run("shape.sizePosition", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(field(&h, "w"), "25.4 mm");
    set_field(&mut h, "h", "50.8");
    ok(&mut h);
    assert!((xfrm(&h, id).h - 144.0).abs() < 1e-6);
}

/// Slide Size shows a widescreen slide as 33.867 × 19.05 cm in Japan; OK without an edit keeps
/// the exact size, and a typed size is read in centimetres.
#[test]
fn slide_size_in_centimetres() {
    let (s, _) = session_in("ja-JP");
    let mut h = harness_with(s);
    h.state_mut().dialog = Some(crate::dialogs::Dialog::new("slideSize"));
    h.run_steps(2);
    assert_eq!((field(&h, "w"), field(&h, "h")), ("33.867 cm".into(), "19.05 cm".into()));
    ok(&mut h);
    let size = h.state().session.active().unwrap().doc.slide_size;
    assert_eq!((size.width, size.height), (960.0, 540.0));
    h.state_mut().dialog = Some(crate::dialogs::Dialog::new("slideSize"));
    h.run_steps(2);
    set_field(&mut h, "w", "25.4");
    set_field(&mut h, "h", "190.5 mm");
    ok(&mut h);
    let size = h.state().session.active().unwrap().doc.slide_size;
    assert!((size.width - 720.0).abs() < 1e-6 && (size.height - 540.0).abs() < 1e-6, "{size:?}");
}

/// Paragraph indents in centimetres; the untouched indent keeps its exact value.
#[test]
fn paragraph_indents_in_centimetres() {
    let (mut s, _) = session_in("ja-JP");
    s.execute("format.paragraphDialog", &json!({"indentLeft": 36.0, "indentRight": 20.0})).unwrap();
    let mut h = harness_with(s);
    h.get_by_label("Paragraph…").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("paragraph"));
    assert_eq!((field(&h, "before"), field(&h, "after")), ("1.27 cm".into(), "0.71 cm".into()));
    set_field(&mut h, "before", "２ｃｍ");
    ok(&mut h);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let st = h.state().session.active().unwrap();
    let p = st.selected_shapes()[0].text.as_ref().unwrap().paragraphs[0].props.clone();
    assert!((p.margin_left.unwrap() - 2.0 / 2.54 * 72.0).abs() < 1e-6, "{p:?}");
    assert_eq!(p.margin_right, Some(20.0), "0.71 cm shown, 20 pt kept");
}

/// Tabs: the default spacing and the stops in centimetres.
#[test]
fn tab_stops_in_centimetres() {
    let mut s = Session::with_new();
    s.set_region(Region::from_tag("ja-JP"));
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let id = s.execute("insert.textBox", &json!({"rect": [100, 150, 500, 40]})).unwrap()["id"].as_u64().unwrap();
    s.execute("text.edit", &json!({"id": id})).unwrap();
    s.execute("text.insert", &json!({"text": "Name\tRole"})).unwrap();
    s.execute("format.tabs", &json!({"add": {"pos": 72.0 / 2.54}})).unwrap();
    let mut h = harness_with(s);
    h.state_mut().dialog = Some(crate::dialogs::Dialog::new("tabs"));
    h.run_steps(2);
    assert_eq!(field(&h, "default"), "2.54 cm");
    assert!(h.query_by_label("1 cm").is_some(), "the stop is listed in centimetres");
    set_field(&mut h, "newpos", "5");
    h.get_by_label("Set").click();
    h.run_steps(2);
    ok(&mut h);
    let r = h.state_mut().session.execute("format.ruler", &json!({})).unwrap();
    let pos: Vec<f64> = r["tabs"].as_array().unwrap().iter().map(|t| t["pos"].as_f64().unwrap()).collect();
    assert_eq!(pos.len(), 2, "{r}");
    assert!((pos[1] - 5.0 / 2.54 * 72.0).abs() < 1e-6, "{pos:?}");
    assert!((r["defaultTab"].as_f64().unwrap() - 72.0).abs() < 1e-9, "the default kept exactly");
}

/// Preferences ▸ Measurement units: Automatic names the region's unit; picking one applies at
/// once, and Cancel puts the setting back.
#[test]
fn preferences_pick_a_measurement_unit() {
    let (s, _) = session_in("ja-JP");
    let mut h = harness_with(s);
    h.state_mut().run("app.preferences", json!({})).unwrap();
    h.run_steps(2);
    assert!(h.query_by_label("Measurement units:").is_some());
    h.query_all_by_role_and_label(egui::accesskit::Role::ComboBox, "Automatic (Centimeters)").next().expect("the unit dropdown").click_accesskit();
    h.run_steps(3);
    h.get_by_label("Points").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().session.prefs.units, Some(deckcraft_geom::Unit::Points));
    assert_eq!(h.state().session.units(), deckcraft_geom::Unit::Points);
    h.get_by_label("Cancel").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().session.prefs.units, None, "Cancel puts Automatic back");
}

/// The texts one frame of the window draws (the rulers' numbers among them).
fn drawn_texts(app: &mut SlideApp) -> Vec<String> {
    fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
    let mut out = Vec::new();
    for i in 0..4 {
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(f64::from(i) * 0.1), ..Default::default() };
        let mut full = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        full.textures_delta.clear();
        out.clear();
        for s in &full.shapes {
            texts(&s.shape, &mut out);
        }
    }
    out
}

/// The rulers count centimetres in Japan (a widescreen slide is 33.87 cm wide) and inches in
/// the United States (13.33 in).
#[test]
fn rulers_count_the_regions_unit() {
    for (region, last, beyond) in [("ja-JP", "33", "34"), ("en-US", "13", "14")] {
        let mut s = Session::with_new();
        s.set_region(Region::from_tag(region));
        let mut app = SlideApp::new(s, Services::default());
        app.ui.ruler = true;
        app.ui.zoom = Some(1.0);
        let texts = drawn_texts(&mut app);
        assert!(texts.iter().any(|t| t == last), "{region}: no {last} on the ruler in {texts:?}");
        assert!(!texts.iter().any(|t| t == beyond), "{region}: {beyond} is past the slide");
    }
}

/// Printing notes pages and handouts starts on A4 in Japan, on Letter in the United States, and
/// the paper list has the Japanese B4 and B5 sizes.
#[test]
fn print_paper_follows_the_region() {
    for (region, paper) in [("ja-JP", "a4"), ("en-JP", "a4"), ("en-US", "letter"), ("fr-CA", "letter")] {
        let (s, _) = session_in(region);
        let mut h = harness_with(s);
        h.state_mut().run("app.backstage", json!({"page": "print"})).unwrap();
        h.run_steps(2);
        assert_eq!(h.state().backstage.as_ref().map(|b| b.print_paper), Some(paper), "{region}");
    }
    let ids: Vec<&str> = deckcraft_engine::cmd::file::PAPERS.iter().map(|p| p.0).collect();
    assert!(ids.contains(&"b4") && ids.contains(&"b5"), "{ids:?}");
}

/// A file name stored decomposed (macOS) shows composed in the Recent list and the title, while
/// the real path is kept for opening.
#[test]
fn decomposed_file_names_show_composed() {
    let decomposed = "/deckcraft-tests/u3/フ\u{309A}レセ\u{3099}ン資料.pptx";
    let (mut s, _) = session_in("en-US");
    s.active_mut().unwrap().path = Some(decomposed.to_string());
    assert_eq!(s.active().unwrap().title(), "プレゼン資料");
    let mut h = harness_with(s);
    h.state_mut().ui.recent = vec![decomposed.to_string()];
    h.state_mut().run("app.backstage", json!({"page": "open"})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("プレゼン資料.pptx").is_some(), "the Recent entry is composed");
    assert_eq!(h.state().ui.recent, vec![decomposed.to_string()], "the real path is kept");
    assert_eq!(h.state().session.active().unwrap().path.as_deref(), Some(decomposed));
}

fn key(h: &mut Harness<'_, SlideApp>, key: Key, physical: Key, modifiers: Modifiers) {
    for pressed in [true, false] {
        h.event(Event::Key { key, physical_key: Some(physical), pressed, repeat: false, modifiers });
    }
    h.run_steps(2);
}

/// On a JIS keyboard `+` is Shift+; and `=` is Shift+-: Cmd+Shift+; (`+`) applies Superscript as
/// Cmd+Shift+= does on a US keyboard, and Cmd+Shift+- (`=`) zooms in as Cmd+= does.
#[test]
fn jis_equals_and_plus_shortcuts() {
    let (s, _) = session_in("ja-JP");
    let mut h = harness_with(s);
    let cmd_shift = Modifiers { shift: true, ..Modifiers::COMMAND };
    let superscript = |h: &mut Harness<'_, SlideApp>| h.state_mut().session.execute("format.state", &json!({})).unwrap()["superscript"] == true;
    assert!(!superscript(&mut h));
    key(&mut h, Key::Plus, Key::Semicolon, cmd_shift);
    assert!(superscript(&mut h), "JIS Cmd+Shift+; is Superscript; status {:?}", h.state().status);
    assert_eq!(h.state().ui.zoom, None, "and doesn't zoom");
    key(&mut h, Key::Plus, Key::Equals, cmd_shift);
    assert!(!superscript(&mut h), "US Cmd+Shift+= toggles Superscript off again");
    key(&mut h, Key::Equals, Key::Minus, cmd_shift);
    assert!(h.state().ui.zoom.is_some(), "JIS Cmd+Shift+- (=) zooms in like Cmd+=");
    assert!(!superscript(&mut h), "and is not Superscript");
}
