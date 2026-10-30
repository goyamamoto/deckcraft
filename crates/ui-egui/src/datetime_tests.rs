//! Insert › Date and Time, driven like a person: the ribbon opens the dialog, a format is
//! chosen, OK inserts it in one undo, Cancel leaves the slide alone.

use deckcraft_engine::Session;
use deckcraft_model::text::RunKind;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

fn harness() -> Harness<'static, SlideApp> {
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.ui.tab = "Insert".into();
    let mut h = Harness::builder().with_size(egui::vec2(1800.0, 1100.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn date_run(h: &Harness<'_, SlideApp>) -> Option<deckcraft_model::Run> {
    h.state()
        .session
        .active()
        .unwrap()
        .current_slide()
        .unwrap()
        .shapes
        .iter()
        .find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date))
        .and_then(|x| x.text.as_ref())
        .and_then(|t| t.paragraphs.first())
        .and_then(|p| p.runs.first())
        .cloned()
}

/// Insert opens date/time settings, and OK inserts exactly one field.
#[test]
fn insert_opens_the_dialog_and_ok_inserts_one_field() {
    let mut h = harness();
    h.get_by_label("Date & Time").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("dateTime"));
    let _ = h.get_by_label("Update automatically");
    assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Language").value().as_deref(), Some("English (United States)"));
    let now = deckcraft_text::datetime::DateTime::now_local().unwrap();
    let sample = deckcraft_text::datetime::field_text("datetime2", Some("en-US"), now).unwrap();
    h.get_by_label(&sample).click();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    let run = date_run(&h).unwrap();
    assert!(matches!(run.kind, RunKind::Field { ref field } if field == "datetime2"), "{run:?}");
    assert_eq!(run.text, sample);
    let undo = &h.state().session.active().unwrap().history.undo;
    assert_eq!(undo.len(), 1);
    assert_eq!(undo[0].label, "Date & Time");
    h.state_mut().session.execute("edit.undo", &json!({})).unwrap();
    h.run_steps(1);
    assert!(date_run(&h).is_none());
}

#[test]
fn japanese_lists_the_thirteen_formats_and_inserts_a_ja_jp_era_date() {
    let mut h = harness();
    h.get_by_label("Date & Time").click();
    h.run_steps(2);
    // An English row first, so switching languages keeps the type.
    let now = deckcraft_text::datetime::DateTime::now_local().unwrap();
    let english = deckcraft_text::datetime::field_text("datetime4", Some("en-US"), now).unwrap();
    h.get_by_label(&english).click();
    h.run_steps(1);
    assert!(h.query_by_label("Calendar type:").is_none(), "English has no calendar choice");
    // The combo boxes are named by their field (egui 0.37), their value shown in it.
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Language").click();
    h.run_steps(2);
    h.get_by_label("Japanese").click();
    h.run_steps(2);
    let d = h.state().dialog.as_ref().unwrap();
    assert_eq!((d.fields.get("language").map(String::as_str), d.fields.get("format").map(String::as_str)), (Some("ja-JP"), Some("datetime4")));
    // Every type has its Japanese text, listed in type order.
    let rows = crate::dialogs::date_rows("ja-JP");
    assert_eq!(rows.iter().map(|(k, _)| *k).collect::<Vec<_>>(), crate::dialogs::DATE_KINDS);
    for kind in ["datetime2", "datetime3", "datetime4", "datetime6", "datetime7"] {
        let sample = deckcraft_text::datetime::field_text(kind, Some("ja-JP"), now).unwrap();
        assert!(h.query_by_label(&sample).is_some(), "{kind}: {sample}");
    }
    // 2026年10月24日 is a Western-calendar date; picking the Japanese era picks its first format.
    let _ = h.get_by_label("Calendar type:");
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Calendar type").click_accesskit();
    h.run_steps(2);
    h.get_by_label("Japanese era").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().unwrap().fields.get("format").map(String::as_str), Some("datetime3"));
    let era = deckcraft_text::datetime::field_text("datetime3", Some("ja-JP"), now).unwrap();
    assert!(era.starts_with("令和"), "{era}");
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    let run = date_run(&h).unwrap();
    assert!(matches!(run.kind, RunKind::Field { ref field } if field == "datetime3"), "{run:?}");
    assert_eq!(run.text, era);
    assert_eq!((run.props.lang.as_deref(), run.props.alt_lang.as_deref()), (Some("ja-JP"), Some("en-US")));
}

#[test]
fn the_dialog_opens_in_japanese_when_the_system_edits_in_japanese() {
    use deckcraft_fonts::CjkLocale;
    assert_eq!(crate::dialogs::date_lang_for(Some(CjkLocale::Japanese)), "ja-JP");
    assert_eq!(crate::dialogs::date_lang_for(Some(CjkLocale::Korean)), "en-US");
    assert_eq!(crate::dialogs::date_lang_for(None), "en-US");
}

#[test]
fn cancel_closes_the_dialog_without_a_date() {
    let mut h = harness();
    h.get_by_label("Date & Time").click();
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert!(date_run(&h).is_none());
    assert!(h.state().session.active().unwrap().history.undo.is_empty());
}
