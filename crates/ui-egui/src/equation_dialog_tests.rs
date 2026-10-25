//! The Equation dialog: TeX typed in, previewed as you type, inserted with ⌘↩, remembered.

use deckcraft_engine::Session;
use deckcraft_model::text::RunKind;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp, UiState};

fn harness_with(s: Session) -> Harness<'static, SlideApp> {
    let app = SlideApp::new(s, Services::default());
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn harness() -> Harness<'static, SlideApp> {
    harness_with(Session::with_new())
}

fn open(h: &mut Harness<'static, SlideApp>) {
    h.state_mut().run("app.dialog", json!({"id": "equation"})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("equation"));
}

fn tex_field(h: &Harness<'_, SlideApp>) -> Option<String> {
    h.state().dialog.as_ref().and_then(|d| d.fields.get("tex").cloned())
}

#[test]
fn typing_tex_previews_it_and_cmd_enter_inserts_and_remembers_it() {
    let mut h = harness();
    open(&mut h);
    // Typing goes straight into the editor.
    h.event(egui::Event::Text("\\frac{a}{b}".into()));
    h.run_steps(3);
    assert_eq!(tex_field(&h).as_deref(), Some("\\frac{a}{b}"));
    let p = h.state().equation_preview.as_ref().expect("a preview picture");
    assert_eq!(p.source, "\\frac{a}{b}");
    assert!(p.size.x > 10.0 && p.size.y > 20.0, "{:?}", p.size);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "Insert closed the dialog");
    let st = h.state().session.active().unwrap();
    let sh = st.selected_shapes().first().cloned().cloned().expect("the equation box is selected");
    assert_eq!(sh.name, "Equation");
    let run = &sh.text.as_ref().unwrap().paragraphs[0].runs[0];
    assert!(matches!(&run.kind, RunKind::Math { omml } if omml.contains("<m:f>")));
    assert_eq!(h.state().ui.equations, vec!["\\frac{a}{b}".to_string()]);
    // A second equation (the first one deselected, else the dialog would edit it) goes first.
    h.state_mut().run("edit.deselect", json!({})).unwrap();
    h.run_steps(2);
    open(&mut h);
    h.event(egui::Event::Text("x^2".into()));
    h.run_steps(2);
    h.get_by_label("Insert").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.equations, vec!["x^2".to_string(), "\\frac{a}{b}".to_string()]);
}

#[test]
fn a_bad_command_names_the_error_and_keeps_the_last_good_preview() {
    let mut h = harness();
    open(&mut h);
    h.event(egui::Event::Text("x^2".into()));
    h.run_steps(3);
    assert_eq!(h.state().equation_preview.as_ref().map(|p| p.source.as_str()), Some("x^2"));
    h.event(egui::Event::Text(" + \\foo".into()));
    h.run_steps(3);
    assert_eq!(tex_field(&h).as_deref(), Some("x^2 + \\foo"));
    assert_eq!(h.state().equation_preview.as_ref().map(|p| p.source.as_str()), Some("x^2"), "the last good preview stays");
    assert!(h.query_by_label_contains("unknown command `\\foo`").is_some(), "the error is shown");
    // Insert is not on offer; ⌘↩ does nothing.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(2);
    assert!(h.state().dialog.is_some());
    assert!(h.state().ui.equations.is_empty());
}

#[test]
fn a_recent_equation_fills_the_editor_and_clear_forgets_them() {
    let mut h = harness();
    h.state_mut().ui.equations = vec!["\\alpha + \\beta".into(), "E = mc^2".into()];
    open(&mut h);
    h.get_by_label("E = mc^2").click();
    h.run_steps(3);
    assert_eq!(tex_field(&h).as_deref(), Some("E = mc^2"));
    assert_eq!(h.state().equation_preview.as_ref().map(|p| p.source.as_str()), Some("E = mc^2"));
    h.get_by_label("Clear").click();
    h.run_steps(2);
    assert!(h.state().ui.equations.is_empty());
}

#[test]
fn the_history_is_kept_in_the_ui_state_short_and_without_repeats() {
    let mut ui = UiState::default();
    for i in 0..40 {
        ui.remember_equation(&format!("x_{{{i}}}"));
    }
    ui.remember_equation("x_{5}");
    assert_eq!(ui.equations.len(), crate::EQUATION_HISTORY_MAX);
    assert_eq!(ui.equations[0], "x_{5}");
    assert_eq!(ui.equations.iter().filter(|e| *e == "x_{5}").count(), 1);
    let json = serde_json::to_string(&ui).unwrap();
    assert!(json.contains("\"equations\""));
    let back: UiState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.equations, ui.equations);
    let damaged = UiState { equations: (0..100).map(|i| if i % 2 == 0 { String::new() } else { "a".into() }).collect(), ..Default::default() };
    let s = damaged.sanitized();
    assert_eq!(s.equations, vec!["a".to_string()]);
}

#[test]
fn opening_the_dialog_beside_an_equation_edits_it() {
    let mut s = Session::with_new();
    s.execute("insert.textBox", &json!({"rect": [40, 40, 400, 40], "text": ""})).unwrap();
    s.execute("text.insert", &json!({"text": "x = "})).unwrap();
    s.execute("insert.equation", &json!({"tex": "y^2"})).unwrap();
    let mut h = harness_with(s);
    open(&mut h);
    assert_eq!(tex_field(&h).as_deref(), Some("y^{2}"));
    let sel = h.state().session.active().unwrap().selection.text.clone().unwrap();
    assert_eq!(sel.ordered(), ((0, 4), (0, 7)), "the equation is selected, so Insert replaces it");
    h.event(egui::Event::Text("+1".into()));
    h.run_steps(2);
    h.get_by_label("Insert").click();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    let t = st.selection.text.clone().unwrap();
    let runs = st.shape(t.shape).unwrap().text.as_ref().unwrap().paragraphs[0].runs.clone();
    assert_eq!(runs.iter().filter(|r| matches!(r.kind, RunKind::Math { .. })).count(), 1, "{runs:?}");
    assert_eq!(runs.last().unwrap().text, "y^2+1");
}

#[test]
fn the_dialog_edits_a_selected_equation_box_in_place() {
    let mut s = Session::with_new();
    s.execute("insert.equation", &json!({"tex": "\\frac{a}{b}"})).unwrap();
    let id = s.active().unwrap().selected_shapes()[0].id;
    let mut h = harness_with(s);
    open(&mut h);
    assert_eq!(tex_field(&h).as_deref(), Some("\\frac{a}{b}"));
    let st = h.state().session.active().unwrap();
    let t = st.selection.text.clone().expect("the box is entered so Insert replaces the equation");
    assert_eq!((t.shape, t.ordered()), (id, ((0, 0), (0, 3))));
    h.event(egui::Event::Text(" + 1".into()));
    h.run_steps(2);
    h.get_by_label("Insert").click();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    assert!(st.selection.text.is_none(), "editing left again");
    assert_eq!(st.selection.shapes, vec![id]);
    let runs = st.shape(id).unwrap().text.as_ref().unwrap().paragraphs[0].runs.clone();
    assert_eq!(runs.iter().filter(|r| matches!(r.kind, RunKind::Math { .. })).count(), 1, "{runs:?}");
    assert_eq!(runs.iter().find(|r| matches!(r.kind, RunKind::Math { .. })).unwrap().text, "a/b+1");
    assert_eq!(h.state().ui.equations, vec!["\\frac{a}{b} + 1".to_string()]);
}

#[test]
fn a_real_double_click_on_an_equation_box_opens_the_dialog_with_the_keyboard_in_the_editor() {
    let mut s = Session::with_new();
    s.execute("insert.equation", &json!({"tex": "a+b"})).unwrap();
    let id = s.active().unwrap().selected_shapes()[0].id;
    s.execute("edit.deselect", &json!({})).unwrap();
    let mut h = harness_with(s);
    let (rect, scale) = {
        let app = h.state();
        (app.slide_rect.expect("the slide is on screen"), app.canvas_scale)
    };
    let x = h.state().session.active().unwrap().shape(id).unwrap().xfrm.unwrap();
    let pos = egui::pos2(rect.min.x + (x.x + x.w / 2.0) as f32 * scale, rect.min.y + (x.y + x.h / 2.0) as f32 * scale);
    for _ in 0..2 {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("equation"), "the double-click opened the dialog");
    assert_eq!(tex_field(&h).as_deref(), Some("a+b"));
    // The keyboard is in the editor: typing lands at the end of the TeX, after the caret.
    h.event(egui::Event::Text("+c".into()));
    h.run_steps(2);
    assert_eq!(tex_field(&h).as_deref(), Some("a+b+c"));
}
