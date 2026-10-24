//! The text tools PowerPoint adds with East Asian editing, driven through the interface: Text
//! Direction with Vertical, Insert ▸ Text Box ▸ Vertical Text Box, Align Text and the alignment
//! buttons in vertical text, Distributed, Half-width and Full-width in Change Case, Match
//! half-width and full-width forms in Find, the East Asian numbering schemes, the Paragraph
//! dialog's Asian Typography page, the Format Shape pane's Text direction, Header & Footer's
//! Japanese dates, the notes as Notes Pages print them, and full-width digits in number fields.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use deckcraft_engine::{CjkLocale, Session};
use deckcraft_model::ShapeId;
use deckcraft_model::text::{Align, Anchor, BodyProps, Bullet, Paragraph, RunKind, Spacing, TextDir};
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use crate::{Services, SlideApp};

/// A blank slide with one text box holding `text` (East Asian vertical when `vertical`), selected.
fn deck_with_box(text: &str, vertical: bool) -> (Session, ShapeId) {
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let cmd = if vertical { "insert.verticalTextBox" } else { "insert.textBox" };
    let id = s.execute(cmd, &json!({"rect": [100.0, 100.0, 300.0, 200.0], "text": text})).unwrap()["id"].as_u64().unwrap() as u32;
    let _ = s.execute("text.exit", &json!({}));
    s.execute("selection.select", &json!({"ids": [id]})).unwrap();
    (s, ShapeId(id))
}

/// The app on ribbon tab `tab`, editing in Japanese when `japanese`.
fn harness(s: Session, tab: &str, japanese: bool) -> Harness<'static, SlideApp> {
    let mut app = SlideApp::new(s, Services::default());
    app.session.east_asian_editing = japanese.then_some(CjkLocale::Japanese);
    app.ui.tab = tab.into();
    let mut h = Harness::builder().with_size(egui::vec2(1800.0, 1000.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn body(h: &Harness<'_, SlideApp>, id: ShapeId) -> BodyProps {
    h.state().session.active().unwrap().shape(id).and_then(|s| s.text.as_ref()).map(|t| t.body.clone()).unwrap_or_default()
}

fn para(h: &Harness<'_, SlideApp>, id: ShapeId) -> Paragraph {
    h.state().session.active().unwrap().shape(id).and_then(|s| s.text.as_ref()).and_then(|t| t.paragraphs.first().cloned()).unwrap()
}

fn text(h: &Harness<'_, SlideApp>, id: ShapeId) -> String {
    h.state().session.active().unwrap().shape(id).and_then(|s| s.text.as_ref()).map(|t| t.text()).unwrap_or_default()
}

fn ticked(h: &Harness<'_, SlideApp>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().toggled() == Some(Toggled::True)
}

/// Click the control named `label` with the pointer (ribbon buttons and their menus).
fn click(h: &mut Harness<'_, SlideApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

/// Click the control named `label` in a dialog, through accessibility: a dialog window that moves
/// as it grows can't make the click land elsewhere.
fn tap(h: &mut Harness<'_, SlideApp>, label: &str) {
    h.get_by_label(label).click_accesskit();
    h.run_steps(2);
}

fn undo_len(h: &Harness<'_, SlideApp>) -> usize {
    h.state().session.active().unwrap().history.undo.len()
}

#[test]
fn text_direction_offers_vertical_with_east_asian_editing_and_ticks_the_current_one() {
    let (s, id) = deck_with_box("縦書きの見出し", false);
    let mut h = harness(s, "Home", true);
    click(&mut h, "Text Direction");
    for label in ["Horizontal", "Vertical", "Rotate all text 90°", "Rotate all text 270°", "Stacked", "Vertical (all characters upright)"] {
        assert!(h.query_by_label(label).is_some(), "{label} is offered");
    }
    assert!(ticked(&h, "Horizontal") && !ticked(&h, "Vertical"));
    click(&mut h, "Vertical");
    assert_eq!(body(&h, id).vert, Some(TextDir::EaVertical));
    assert!(h.query_by_label("Stacked").is_none(), "choosing closes the menu");
    click(&mut h, "Text Direction");
    assert!(ticked(&h, "Vertical") && !ticked(&h, "Horizontal"), "the menu ticks the box's direction");
    click(&mut h, "Vertical (all characters upright)");
    assert_eq!(body(&h, id).vert, Some(TextDir::StackedRtl));
}

#[test]
fn text_direction_keeps_the_latin_choices_without_east_asian_editing() {
    let (s, id) = deck_with_box("Heading", false);
    let mut h = harness(s, "Home", false);
    click(&mut h, "Text Direction");
    assert!(h.query_by_label("Rotate all text 90°").is_some());
    assert!(h.query_by_label("Vertical").is_none() && h.query_by_label("Vertical (all characters upright)").is_none());
    click(&mut h, "Stacked");
    assert_eq!(body(&h, id).vert, Some(TextDir::Stacked));
    // A box that is vertical already (from a file) shows it, ticked.
    let (s, _) = deck_with_box("縦", true);
    let mut h = harness(s, "Home", false);
    click(&mut h, "Text Direction");
    assert!(ticked(&h, "Vertical"));
    assert!(h.query_by_label("Vertical (all characters upright)").is_none());
}

#[test]
fn text_box_is_a_split_button_with_a_vertical_text_box() {
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let mut h = harness(s, "Insert", false);
    let shapes = |h: &Harness<'_, SlideApp>| h.state().session.active().unwrap().current_slide().unwrap().shapes.clone();
    click(&mut h, "Text Box options");
    assert!(h.query_by_label("Draw Horizontal Text Box").is_some());
    click(&mut h, "Vertical Text Box");
    let sh = shapes(&h).last().cloned().unwrap();
    assert!(sh.text_box);
    assert_eq!(sh.text.as_ref().unwrap().body.vert, Some(TextDir::EaVertical), "a vertical text box writes vert=eaVert");
    assert_eq!(h.state().session.active().unwrap().history.undo.last().unwrap().label, "Vertical Text Box");
    let _ = h.state_mut().run("text.exit", json!({}));
    h.run_steps(1);
    click(&mut h, "Text Box options");
    click(&mut h, "Draw Horizontal Text Box");
    let sh = shapes(&h).last().cloned().unwrap();
    assert!(sh.text_box && sh.text.as_ref().unwrap().body.vert.is_none());
    assert_eq!(shapes(&h).len(), 2);
    // The menu bar's Insert menu has it too.
    let insert = crate::menus::menu_tree().into_iter().find(|(m, _)| *m == "Insert").unwrap().1;
    assert!(insert.contains(&("Vertical Text Box", "insert.verticalTextBox")));
    assert!(crate::ribbon::known("insert.verticalTextBox"));
}

#[test]
fn align_text_and_the_alignment_buttons_turn_with_vertical_text() {
    let (s, id) = deck_with_box("縦書き", true);
    let mut h = harness(s, "Home", false);
    // A line of vertical text runs down its column: the buttons align it top, centre, bottom.
    assert!(h.query_by_label_contains("Align Top").is_some() && h.query_by_label_contains("Align Bottom").is_some());
    assert!(h.query_by_label_contains("Align Left").is_none());
    click(&mut h, "Align Text");
    assert!(h.query_by_label("Top").is_none() && h.query_by_label("Middle").is_none());
    // Columns from right to left: the first column's side is Right.
    click(&mut h, "Right");
    assert_eq!(body(&h, id).anchor, Some(Anchor::Top));
    click(&mut h, "Align Text");
    click(&mut h, "Left");
    assert_eq!(body(&h, id).anchor, Some(Anchor::Bottom));
    click(&mut h, "Align Text");
    click(&mut h, "Center");
    assert_eq!(body(&h, id).anchor, Some(Anchor::Middle));

    // Horizontal text keeps Top / Middle / Bottom.
    let (s, id) = deck_with_box("Across", false);
    let mut h = harness(s, "Home", false);
    assert!(h.query_by_label_contains("Align Left").is_some());
    click(&mut h, "Align Text");
    click(&mut h, "Bottom");
    assert_eq!(body(&h, id).anchor, Some(Anchor::Bottom));
}

#[test]
fn turned_text_maps_align_text_and_the_buttons_to_its_sides() {
    assert_eq!(crate::ribbon::align_text_choices("rotate270").map(|c| c.0), ["Left", "Center", "Right"]);
    assert_eq!(crate::ribbon::align_text_choices("stackedRtl").map(|c| c.0), ["Right", "Center", "Left"]);
    assert_eq!(crate::ribbon::align_text_choices("mongolianVert")[0], ("Left", "top"));
    assert_eq!(crate::ribbon::paragraph_align_labels("rotate270"), ["Align Bottom", "Center", "Align Top"]);
    assert_eq!(crate::ribbon::paragraph_align_labels("rotate90"), ["Align Top", "Center", "Align Bottom"]);
    assert_eq!(crate::ribbon::paragraph_align_labels("bogus"), ["Align Left", "Center", "Align Right"]);
}

#[test]
fn distributed_alignment_is_a_button_with_east_asian_editing() {
    let (s, id) = deck_with_box("均等割り付け", false);
    let mut h = harness(s, "Home", true);
    click(&mut h, "Distributed");
    assert_eq!(para(&h, id).props.align, Some(Align::Distributed));
    let (s, _) = deck_with_box("Distributed", false);
    let h = harness(s, "Home", false);
    assert!(h.query_by_label("Distributed").is_none(), "Latin editing keeps the four alignment buttons");
}

#[test]
fn change_case_converts_half_and_full_width_with_east_asian_editing() {
    let (s, id) = deck_with_box("ｶﾀｶﾅ abc", false);
    let mut h = harness(s, "Home", true);
    click(&mut h, "Change Case");
    assert!(h.query_by_label("Half-width").is_some());
    click(&mut h, "Full-width");
    assert_eq!(text(&h, id), "カタカナ　ａｂｃ");
    click(&mut h, "Change Case");
    click(&mut h, "Half-width");
    assert_eq!(text(&h, id), "ｶﾀｶﾅ abc");
    let (s, _) = deck_with_box("abc", false);
    let mut h = harness(s, "Home", false);
    click(&mut h, "Change Case");
    assert!(h.query_by_label("UPPERCASE").is_some());
    assert!(h.query_by_label("Half-width").is_none() && h.query_by_label("Full-width").is_none());
}

#[test]
fn find_matches_half_and_full_width_forms_unless_told_apart() {
    let (mut s, _) = deck_with_box("ＡＢＣ", false);
    s.doc_mut().unwrap().selection.shapes.clear();
    let mut h = harness(s, "Home", true);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(3);
    h.event(egui::Event::Text("ABC".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let status = |h: &Harness<'_, SlideApp>| h.state().dialog.as_ref().and_then(|d| d.fields.get("status").cloned()).unwrap_or_default();
    assert!(status(&h).starts_with("Match 1 of 1"), "ABC finds ＡＢＣ: {}", status(&h));
    tap(&mut h, "Match half-width and full-width forms");
    tap(&mut h, "Find Next");
    assert_eq!(status(&h), "DeckCraft can't find “ABC”.");
    // Without East Asian editing there is no such box (and the forms still match).
    let (mut s, _) = deck_with_box("ＡＢＣ", false);
    s.doc_mut().unwrap().selection.shapes.clear();
    let mut h = harness(s, "Home", false);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(3);
    assert!(h.query_by_label("Match half-width and full-width forms").is_none());
    assert!(h.query_by_label("Match case").is_some());
}

/// The sample `format.numberingSchemes` shows for `scheme`.
fn sample(scheme: &str) -> String {
    let mut s = Session::with_new();
    let v = s.execute("format.numberingSchemes", &json!({})).unwrap();
    v["schemes"].as_array().unwrap().iter().find(|e| e["scheme"] == scheme).unwrap()["sample"].as_str().unwrap().to_string()
}

fn scheme_of(h: &Harness<'_, SlideApp>, id: ShapeId) -> Option<String> {
    match para(h, id).props.bullet {
        Some(Bullet::AutoNum { scheme, .. }) => Some(scheme),
        _ => None,
    }
}

#[test]
fn the_numbering_gallery_adds_east_asian_schemes_with_east_asian_editing() {
    let circled = sample("circleNumDbPlain");
    assert_eq!(circled, "① ② ③");
    let (s, id) = deck_with_box("一つ目", false);
    let mut h = harness(s, "Home", true);
    click(&mut h, "Numbering styles");
    assert!(h.query_by_label(&sample("arabicDbPeriod")).is_some() && h.query_by_label(&sample("ea1JpnChsDbPeriod")).is_some());
    click(&mut h, &circled);
    assert_eq!(scheme_of(&h, id).as_deref(), Some("circleNumDbPlain"));
    let (s, _) = deck_with_box("First", false);
    let mut h = harness(s, "Home", false);
    click(&mut h, "Numbering styles");
    assert!(h.query_by_label(&sample("arabicPeriod")).is_some());
    assert!(h.query_by_label(&circled).is_none(), "Latin editing keeps the Latin schemes");
}

#[test]
fn bullets_and_numbering_lists_the_east_asian_schemes() {
    let (s, id) = deck_with_box("一つ目", false);
    let mut h = harness(s, "Home", true);
    let mut dlg = crate::dialogs::Dialog::new("bullets");
    dlg.fields.insert("tab".into(), "numbered".into());
    h.state_mut().dialog = Some(dlg);
    h.run_steps(2);
    tap(&mut h, &sample("ea1JpnChsDbPeriod"));
    h.state_mut().dialog.as_mut().unwrap().fields.insert("start".into(), "３".into());
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(scheme_of(&h, id).as_deref(), Some("ea1JpnChsDbPeriod"));
    assert!(matches!(para(&h, id).props.bullet, Some(Bullet::AutoNum { start_at: 3, .. })), "full-width ３ starts at 3");
}

fn open_paragraph_dialog(h: &mut Harness<'_, SlideApp>) {
    h.get_by_label("Paragraph…").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("paragraph"));
}

#[test]
fn paragraph_dialog_has_an_asian_typography_page_applied_in_one_undo() {
    let (s, id) = deck_with_box("句読点。", false);
    let mut h = harness(s, "Home", true);
    open_paragraph_dialog(&mut h);
    tap(&mut h, "Asian Typography");
    // The paragraph's values, PowerPoint's defaults when it has none.
    assert!(ticked(&h, "Apply Asian line-breaking rules (kinsoku)"));
    assert!(!ticked(&h, "Let Latin words break at any letter"));
    assert!(ticked(&h, "Hang punctuation past the line end"));
    tap(&mut h, "Hang punctuation past the line end");
    tap(&mut h, "Let Latin words break at any letter");
    h.state_mut().dialog.as_mut().unwrap().fields.insert("fontAlgn".into(), "ctr".into());
    // Tabs… and back keeps the page and the changes.
    h.get_by_label("Tabs…").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("tabs"));
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("paragraph"));
    assert!(!ticked(&h, "Hang punctuation past the line end"), "the change survived Tabs…");
    let undo = undo_len(&h);
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let p = para(&h, id).props;
    assert_eq!((p.hanging_punct, p.latin_line_break, p.font_align.as_deref()), (Some(false), Some(true), Some("ctr")));
    assert_eq!(undo_len(&h), undo + 1);
    assert_eq!(h.state().session.active().unwrap().history.undo.last().unwrap().label, "Paragraph…");

    // Latin editing has no such page.
    let (s, _) = deck_with_box("Text", false);
    let mut h = harness(s, "Home", false);
    open_paragraph_dialog(&mut h);
    assert!(h.query_by_label("Asian Typography").is_none());
    assert!(h.query_by_label("Spacing before").is_some());
}

#[test]
fn an_unchanged_asian_typography_page_writes_nothing() {
    let (s, id) = deck_with_box("段落", false);
    let mut h = harness(s, "Home", true);
    let before = para(&h, id).props;
    open_paragraph_dialog(&mut h);
    tap(&mut h, "Asian Typography");
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    let after = para(&h, id).props;
    assert_eq!(
        (after.east_asian_line_break, after.latin_line_break, after.hanging_punct, after.font_align),
        (before.east_asian_line_break, before.latin_line_break, before.hanging_punct, before.font_align)
    );
}

#[test]
fn paragraph_dialog_reads_full_width_digits() {
    let (s, id) = deck_with_box("Spaced", false);
    let mut h = harness(s, "Home", false);
    open_paragraph_dialog(&mut h);
    h.state_mut().dialog.as_mut().unwrap().fields.insert("spaceBefore".into(), "１２".into());
    h.state_mut().dialog.as_mut().unwrap().fields.insert("lineValue".into(), "１．５".into());
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let p = para(&h, id).props;
    assert_eq!(p.space_before, Some(Spacing::Pts(12.0)));
    assert_eq!(p.line_spacing, Some(Spacing::Pct(1.5)));
}

#[test]
fn format_shape_pane_has_a_text_direction_list() {
    let (s, id) = deck_with_box("縦にする", false);
    let mut app_h = harness(s, "Home", true);
    app_h.state_mut().ui.pane = Some("format".into());
    app_h.state_mut().ui.format_tab = "text".into();
    app_h.run_steps(3);
    click(&mut app_h, "Text direction");
    click(&mut app_h, "Vertical");
    assert_eq!(body(&app_h, id).vert, Some(TextDir::EaVertical));
    // Vertical alignment reads across the columns now.
    assert!(app_h.query_by_role_and_label(Role::Button, "Top").is_none());
    app_h.get_by_role_and_label(Role::Button, "Right").click();
    app_h.run_steps(2);
    assert_eq!(body(&app_h, id).anchor, Some(Anchor::Top));
}

/// The date run of the current slide's date placeholder: (field type, lang).
fn footer_date(h: &Harness<'_, SlideApp>) -> Option<(String, Option<String>)> {
    let slide = h.state().session.active()?.current_slide()?.clone();
    let ph = slide.shapes.iter().find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date))?;
    let run = ph.text.as_ref()?.paragraphs.first()?.runs.first()?.clone();
    match run.kind {
        RunKind::Field { field } => Some((field, run.props.lang)),
        _ => Some(("text".into(), run.props.lang)),
    }
}

#[test]
fn header_and_footer_offers_japanese_dates_when_editing_in_japanese() {
    let mut h = harness(Session::with_new(), "Insert", true);
    click(&mut h, "Header & Footer");
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("headerFooter"));
    assert_eq!(h.state().dialog.as_ref().unwrap().fields.get("dateLang").map(String::as_str), Some("ja-JP"), "the editing language");
    tap(&mut h, "Date and time");
    assert!(h.query_by_label("Calendar type:").is_some(), "Japanese dates can count years by era");
    let now = deckcraft_text::datetime::DateTime::now_local().unwrap();
    let japanese = deckcraft_text::datetime::field_text("datetime2", Some("ja-JP"), now).unwrap();
    tap(&mut h, "Format:");
    tap(&mut h, &japanese);
    tap(&mut h, "Apply to All");
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    assert_eq!(footer_date(&h), Some(("datetime2".into(), Some("ja-JP".into()))));

    // Reopened, the dialog starts from that date; Fixed text replaces it.
    click(&mut h, "Header & Footer");
    assert_eq!(h.state().dialog.as_ref().unwrap().fields.get("dateFormat").map(String::as_str), Some("datetime2"));
    tap(&mut h, "Fixed");
    h.state_mut().dialog.as_mut().unwrap().fields.insert("dateText".into(), "第1回".into());
    tap(&mut h, "Apply");
    assert_eq!(footer_date(&h).map(|d| d.0).as_deref(), Some("text"));
}

#[test]
fn header_and_footer_without_japanese_editing_keeps_us_dates() {
    let mut h = harness(Session::with_new(), "Insert", false);
    click(&mut h, "Header & Footer");
    assert_eq!(h.state().dialog.as_ref().unwrap().fields.get("dateLang").map(String::as_str), Some("en-US"));
    tap(&mut h, "Date and time");
    assert!(h.query_by_label("Calendar type:").is_none());
    tap(&mut h, "Apply");
    // Nothing chosen: a plain datetime1 field without a language of its own.
    assert_eq!(footer_date(&h), Some(("datetime1".into(), None)));
}

#[test]
fn notes_previews_are_drawn_by_the_slide_text_engine_and_cached() {
    let mut s = Session::with_new();
    s.execute("slide.notes", &json!({"text": "話す内容のメモ。PowerPoint と同じく折り返す。"})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    let ctx = egui::Context::default();
    let frame = deckcraft_geom::Rect::new(72.0, 420.0, 540.0, 738.0);
    let first = crate::sorter::notes_picture(&ctx, &doc, 0, frame, 1.0, 1.0).expect("a picture of the notes");
    assert_eq!(first.1, egui::vec2(468.0, 318.0), "one pixel per page point");
    let again = crate::sorter::notes_picture(&ctx, &doc, 0, frame, 1.0, 1.0).unwrap();
    assert_eq!(first.0.id(), again.0.id(), "cached");
    assert_eq!(crate::sorter::notes_pictures_held(&ctx), 1);
    // Edited notes are drawn again.
    s.execute("slide.notes", &json!({"text": "Changed"})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    let changed = crate::sorter::notes_picture(&ctx, &doc, 0, frame, 1.0, 1.0).unwrap();
    assert_ne!(changed.0.id(), first.0.id());
    // A slide without notes has nothing to draw.
    s.execute("slide.new", &json!({})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    assert!(crate::sorter::notes_picture(&ctx, &doc, 1, frame, 1.0, 1.0).is_none());
    assert!(crate::sorter::notes_picture(&ctx, &doc, 9, frame, 1.0, 1.0).is_none());
}

#[test]
fn print_preview_and_notes_page_show_the_printed_notes() {
    let mut s = Session::with_new();
    s.execute("slide.notes", &json!({"text": "Notes for the speaker"})).unwrap();
    let mut h = harness(s, "Home", false);
    h.state_mut().open_backstage(crate::backstage::Page::Print);
    h.state_mut().backstage.as_mut().unwrap().print_layout = "notes";
    h.run_steps(3);
    assert_eq!(crate::sorter::notes_pictures_held(&h.ctx), 1, "the Print preview draws the notes as printed");
    h.state_mut().backstage = None;
    let _ = h.state_mut().run("view.notesPage", json!({}));
    h.run_steps(3);
    assert_eq!(crate::sorter::notes_pictures_held(&h.ctx), 2, "so does the Notes Page view, at its own size");
}
