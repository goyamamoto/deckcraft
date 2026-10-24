//! #101: Korean, Japanese and Chinese in the interface: input method composition on the slide and
//! in the notes pane (egui `Ime` events, as egui-winit sends them), the input method's caret and
//! clause-by-clause commits, what it reads of the slide's text and the committed text it
//! reconverts (再変換, `crate::ime`), and interface text drawn with real glyphs rather than boxes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use egui::{Event, FontId, ImeEvent};
use serde_json::json;

use crate::{Services, SlideApp};
use deckcraft_engine::Session;

fn harness(session: Session) -> egui_kittest::Harness<'static, SlideApp> {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        SlideApp::new(session, Services::default()),
    );
    // The app keeps requesting repaints, so step frames instead of run().
    h.run_steps(3);
    h
}

fn preedit(text: &str) -> Event {
    Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: None })
}

fn title_text(app: &SlideApp) -> String {
    let sh = &app.session.active().unwrap().doc.slides[0].shapes[0];
    sh.text.as_ref().map(|t| t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

fn undo_len(app: &SlideApp) -> usize {
    app.session.active().unwrap().history.undo.len()
}

/// Typing 한글 with the Korean input method in the title: each preedit replaces the last in the
/// text box (not ㅎ ㅏ ㄴ …), is underlined, and the commit is one undo step. The input method
/// is on while the slide's text is edited, with its window at the insertion point.
#[test]
fn korean_composes_in_a_slide_text_box() {
    let mut h = harness(Session::with_new());
    assert!(h.output().platform_output.ime.is_none(), "nothing to type into: the input method stays off");
    let title = h.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    h.run_steps(2);
    let ime = h.output().platform_output.ime.expect("the input method is on while editing");
    let slide = h.state().slide_rect.unwrap();
    assert!(slide.contains(ime.rect.center()), "candidate window at the insertion point {:?}, on the slide {slide:?}", ime.rect);
    assert!(ime.rect.width() < 8.0 && ime.rect.height() > 4.0, "a caret, not the box: {:?}", ime.rect);
    let caret0 = ime.rect;

    let undo0 = undo_len(h.state());
    for pre in ["ㅎ", "하", "한", "한ㄱ", "한그", "한글"] {
        h.event(preedit(pre));
        h.run_steps(1);
        assert_eq!(title_text(h.state()), pre, "the preedit is shown in place, replacing the last");
        assert_eq!(undo_len(h.state()), undo0, "no undo step while composing");
        let st = h.state().session.active().unwrap();
        assert!(!deckcraft_engine::cmd::text::composition_underlines(st).is_empty(), "underlined while composing");
    }
    h.run_steps(1);
    let ime = h.output().platform_output.ime.unwrap();
    assert!(ime.rect.center().x > caret0.center().x, "the candidate window follows the insertion point");
    h.event(Event::Ime(ImeEvent::Commit("한글".into())));
    h.run_steps(2);
    assert_eq!(title_text(h.state()), "한글");
    assert!(h.state().session.active().unwrap().composition.is_none());
    assert!(deckcraft_engine::cmd::text::composition_underlines(h.state().session.active().unwrap()).is_empty());
    assert_eq!(undo_len(h.state()), undo0 + 1, "the composition is one undo step");

    // Escape cancels a composition: an empty preedit, and the text is as it was.
    h.event(preedit("ㅁ"));
    h.event(preedit(""));
    h.run_steps(2);
    assert_eq!(title_text(h.state()), "한글");
    assert_eq!(undo_len(h.state()), undo0 + 1);

    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "", "one undo removes the whole word");
}

/// Japanese conversion: the part being converted is drawn with a thicker line; a key settles a
/// composition the input method left open.
#[test]
fn japanese_conversion_shows_the_active_part() {
    let mut h = harness(Session::with_new());
    let title = h.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    h.event(Event::Ime(ImeEvent::Preedit { text: "日本ご".into(), active_range_chars: Some(2..3) }));
    h.run_steps(2);
    let u = deckcraft_engine::cmd::text::composition_underlines(h.state().session.active().unwrap());
    assert_eq!(u.iter().filter(|x| x.2).count(), 1, "{u:?}");
    let undo0 = undo_len(h.state());
    h.key_press(egui::Key::End);
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "日本ご");
    assert!(h.state().session.active().unwrap().composition.is_none());
    assert_eq!(undo_len(h.state()), undo0 + 1);
}

/// Typing on a selected (not edited) shape with an input method replaces its text, as typing does;
/// the input method is on for it.
#[test]
fn composing_on_a_selected_shape_starts_editing_it() {
    let mut h = harness(Session::with_new());
    let r = h.state_mut().run("shape.insert", json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "Old"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    h.state_mut().run("text.exit", json!({})).unwrap();
    h.run_steps(2);
    assert!(h.state().session.active().unwrap().selection.text.is_none());
    assert!(h.output().platform_output.ime.is_some(), "a selected shape takes typing");
    h.event(preedit("新"));
    h.event(Event::Ime(ImeEvent::Commit("新しい".into())));
    h.run_steps(2);
    let v = h.state_mut().session.execute("text.get", &json!({"id": id})).unwrap();
    assert_eq!(v["text"], "新しい");
}

/// The notes pane is an egui text field: composition shows in it and replaces each preedit, the
/// candidate window is at its caret, and the notes are one undo step when it loses focus.
#[test]
fn korean_composes_in_the_notes_pane() {
    let mut h = harness(Session::with_new());
    let id = egui::Id::new("notes_edit");
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run_steps(2);
    assert!(h.ctx.memory(|m| m.has_focus(id)));
    let undo0 = undo_len(h.state());
    for pre in ["ㄴ", "노", "놑", "노트"] {
        h.event(preedit(pre));
        h.run_steps(1);
        assert_eq!(h.state().notes_buf.2.text, pre, "the preedit replaces the last one");
    }
    let ime = h.output().platform_output.ime.expect("the notes field turns the input method on");
    let slide = h.state().slide_rect.unwrap();
    assert!(ime.cursor_rect.min.y > slide.max.y, "candidate window at the notes caret, under the slide");
    h.event(Event::Ime(ImeEvent::Commit("노트".into())));
    h.run_steps(1);
    assert_eq!(h.state().notes_buf.2.text, "노트");
    // Focus leaves the field (Escape): the notes are committed as one step.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let notes = h.state().session.active().unwrap().doc.slides[0].notes_text();
    assert_eq!(notes, "노트");
    assert_eq!(undo_len(h.state()), undo0 + 1);
    assert!(h.ctx.fonts_mut(|f| f.has_glyphs(&FontId::proportional(13.0), "노트")) || !system_has("노트"));

    // Composing, then going to another slide: the notes stay with the slide they were typed for.
    h.state_mut().run("slide.new", json!({})).unwrap();
    h.state_mut().run("slide.go", json!({"index": 0})).unwrap();
    h.run_steps(2);
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run_steps(2);
    h.event(Event::Key { key: egui::Key::End, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
    h.event(preedit("ㅇ"));
    h.event(Event::Ime(ImeEvent::Commit("요".into())));
    h.run_steps(1);
    h.state_mut().run("slide.go", json!({"index": 1})).unwrap();
    h.run_steps(2);
    let d = h.state().session.active().unwrap();
    assert_eq!(d.doc.slides[0].notes_text(), "노트요");
    assert_eq!(d.doc.slides[1].notes_text(), "");
}

fn preedit_at(text: &str, active: std::ops::Range<usize>) -> Event {
    Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: Some(active) })
}

fn commit(text: &str) -> Event {
    Event::Ime(ImeEvent::Commit(text.into()))
}

/// A harness editing the title, which holds `text`.
fn editing_title(text: &str) -> egui_kittest::Harness<'static, SlideApp> {
    let mut s = Session::with_new();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    if !text.is_empty() {
        s.execute("text.set", &json!({"id": title, "text": text})).unwrap();
    }
    let mut h = harness(s);
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    h.run_steps(2);
    h
}

fn caret(app: &SlideApp) -> (usize, usize) {
    app.session.active().unwrap().selection.text.as_ref().unwrap().caret
}

fn ime_allowed(h: &egui_kittest::Harness<'_, SlideApp>) -> bool {
    h.output().platform_output.ime.is_some()
}

/// Deliver `events` in one frame, as egui-winit does with the events the platform queues
/// together (a dismissal and its commit); `Harness::event` runs a frame for each event.
fn in_one_frame<const N: usize>(h: &mut egui_kittest::Harness<'_, SlideApp>, events: [Event; N]) {
    h.input_mut().events.extend(events);
    h.run_steps(1);
}

/// Committing the first clause of a multi-clause conversion: the input method commits it and
/// marks the rest again (on macOS `insertText:` then `setMarkedText:`; the events in one frame or
/// the next), and later commits the rest. Every clause reaches the slide, and the input method
/// stays allowed (the IME position is reported every frame), so its remaining marked text is not
/// dropped. With stock winit the rest was lost on macOS (`vendor/patches/winit/01`).
#[test]
fn committing_the_first_clause_keeps_the_rest_of_the_composition() {
    let mut h = editing_title("");
    assert!(ime_allowed(&h));
    h.event(preedit_at("きょうはいいてんき", 0..9));
    h.run_steps(1);
    h.event(preedit_at("今日はいい天気", 0..3));
    h.run_steps(1);
    assert!(ime_allowed(&h));
    let undo0 = undo_len(h.state());
    // The first clause committed and the rest marked again, in the same frame.
    h.event(preedit(""));
    h.event(commit("今日は"));
    h.event(preedit_at("いい天気", 0..2));
    for _ in 0..3 {
        h.run_steps(1);
        assert!(ime_allowed(&h), "the input method stays allowed while the rest is marked");
    }
    assert_eq!(title_text(h.state()), "今日はいい天気", "the first clause committed, the rest composing in place");
    let c = h.state().session.active().unwrap().composition.clone().expect("the rest is still a composition");
    assert_eq!((c.start, c.len), ((0, 3), 4));
    assert_eq!(undo_len(h.state()), undo0 + 1, "the committed clause is an undo step");
    h.event(preedit(""));
    h.event(commit("いい天気"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "今日はいい天気");
    assert!(h.state().session.active().unwrap().composition.is_none());
    // The same with the rest marked a frame later.
    h.event(preedit_at("あしたははれ", 0..3));
    h.run_steps(1);
    h.event(preedit(""));
    h.event(commit("明日は"));
    h.run_steps(1);
    assert!(ime_allowed(&h));
    h.event(preedit_at("晴れ", 0..2));
    h.run_steps(1);
    assert!(ime_allowed(&h));
    h.event(preedit(""));
    h.event(commit("晴れ"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "今日はいい天気明日は晴れ");
    // A full-width space the input method types is kept full width.
    h.event(commit("\u{3000}"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "今日はいい天気明日は晴れ\u{3000}");
}

/// The input method's caret moved with the arrow keys inside text not yet converted (an empty
/// selection in its marked text): the caret and the candidate window's position go there, and
/// what is typed there arrives as the whole composition again. A clause being converted puts
/// them at its end.
#[test]
fn the_caret_follows_the_input_methods_caret_inside_the_composition() {
    let mut h = editing_title("");
    let ime_x = |h: &egui_kittest::Harness<'_, SlideApp>| h.output().platform_output.ime.map(|ime| ime.rect.center().x);
    h.event(preedit_at("にほご", 3..3));
    h.run_steps(2);
    let at_end = ime_x(&h).expect("IME position");
    assert_eq!(caret(h.state()), (0, 3));
    // Left arrow twice.
    h.event(preedit_at("にほご", 1..1));
    h.run_steps(2);
    assert_eq!(caret(h.state()), (0, 1), "the caret stands after に");
    let inside = ime_x(&h).expect("IME position");
    assert!(inside < at_end - 10.0, "the candidate window follows the caret: {inside} vs {at_end}");
    let u = deckcraft_engine::cmd::text::composition_underlines(h.state().session.active().unwrap());
    assert!(!u.is_empty() && u.iter().all(|x| !x.2), "underlined, nothing thick: {u:?}");
    // ん typed there.
    h.event(preedit_at("にほんご", 3..3));
    h.run_steps(2);
    assert_eq!(caret(h.state()), (0, 3));
    // Converting: the first clause thick, the caret and the candidate window at its end.
    h.event(preedit_at("日本語", 0..2));
    h.run_steps(2);
    assert_eq!(caret(h.state()), (0, 2));
    let clause = ime_x(&h).expect("IME position");
    assert!(clause < at_end, "{clause} vs {at_end}");
    // Committed: the caret ends the committed text.
    h.event(preedit(""));
    h.event(commit("日本語"));
    h.run_steps(2);
    assert_eq!(title_text(h.state()), "日本語");
    assert_eq!(caret(h.state()), (0, 3), "after the commit the caret ends the committed text");
}

/// 再変換: the input method reads the caret's paragraph and the selection (gathered each frame
/// for the platform), names the committed text its composition replaces, and that text gives way
/// to the composition — put back if the composition is dismissed, replaced for good by the
/// commit, as one undo step.
#[test]
fn ime_reconverts_committed_text_it_asked_to_replace() {
    let mut h = editing_title("a日本語b");
    h.state_mut().run("text.select", json!({"anchor": [0, 1], "caret": [0, 4]})).unwrap();
    h.run_steps(2);
    assert_eq!(
        h.state().ime.surrounding,
        Some(crate::ime::ImeSurrounding { text: "a日本語b".into(), selected: 1..4 }),
        "the paragraph and the selection are reported"
    );
    let undo0 = undo_len(h.state());
    // The input method asks for 1..4 to be replaced by the reading it marks.
    h.state_mut().ime.replace = Some(1..4);
    h.event(preedit_at("にほんご", 0..4));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "aにほんごb", "the committed text gives way to the composition");
    assert!(h.state().ime.replace.is_none(), "taken");
    assert_eq!(undo_len(h.state()), undo0);
    // Dismissed: the committed text is back, still selected.
    h.event(preedit(""));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本語b");
    let t = h.state().session.active().unwrap().selection.text.clone().unwrap();
    assert_eq!(t.ordered(), ((0, 1), (0, 4)));
    // From the caret: reconverted and committed, one undo step.
    h.state_mut().run("text.select", json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
    h.run_steps(1);
    assert_eq!(h.state().ime.surrounding.as_ref().map(|s| s.selected.clone()), Some(5..5), "the caret");
    h.state_mut().ime.replace = Some(1..4);
    h.event(preedit_at("にほんご", 0..4));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "aにほんごb");
    h.event(preedit(""));
    h.event(commit("二本後"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a二本後b");
    assert_eq!(undo_len(h.state()), undo0 + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(title_text(h.state()), "a日本語b");
    // A range that doesn't fit the paragraph is ignored: the composition goes at the caret.
    let title = h.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    h.state_mut().run("text.select", json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
    h.state_mut().ime.replace = Some(2..40);
    h.event(preedit("あ"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本語bあ");
    h.event(preedit(""));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本語b");
}

/// The input method may name committed text while its composition shows (macOS live conversion
/// marks the words before its marked text again to convert them together), and a commit may name
/// the text it replaces: both replace that text, never add to it.
#[test]
fn ime_replaces_committed_text_named_while_composing_and_on_commit() {
    let mut h = editing_title("a日本b");
    let before_b = |h: &mut egui_kittest::Harness<'static, SlideApp>| {
        h.state_mut().run("text.select", json!({"anchor": [0, 3], "caret": [0, 3]})).unwrap();
        h.run_steps(1);
    };
    before_b(&mut h);
    // Composing ご before b: the input method reads a日本ごb.
    h.event(preedit("ご"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本ごb");
    assert_eq!(h.state().ime.surrounding.as_ref().map(|s| s.text.as_str()), Some("a日本ごb"));
    // It marks 日本ご (1..4 of that text) again as 日本語: the committed 日本 gives way too.
    h.state_mut().ime.replace = Some(1..4);
    h.event(preedit_at("日本語", 0..3));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本語b", "no duplicated 日本");
    let c = h.state().session.active().unwrap().composition.clone().unwrap();
    assert_eq!((c.start, c.len), ((0, 1), 3));
    // Dismissed: the committed text is back; committed: replaced for good, one undo step.
    h.event(preedit(""));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本b");
    let undo0 = undo_len(h.state());
    before_b(&mut h);
    h.event(preedit("ご"));
    h.run_steps(1);
    h.state_mut().ime.replace = Some(1..4);
    h.event(preedit_at("日本語", 0..3));
    h.run_steps(1);
    h.event(preedit(""));
    h.event(commit("日本語"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本語b");
    assert_eq!(undo_len(h.state()), undo0 + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(title_text(h.state()), "a日本b");
    // A commit naming the text it replaces without marking it first (the dismissal carries the
    // range, as the vendored winit reports `insertText:replacementRange:`): 日本ご → 日本語.
    let title = h.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    before_b(&mut h);
    h.event(preedit("ご"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本ごb");
    h.state_mut().ime.replace = Some(1..4);
    in_one_frame(&mut h, [preedit(""), commit("日本語")]);
    assert_eq!(title_text(h.state()), "a日本語b");
    assert!(h.state().ime.replace.is_none());
    assert_eq!(caret(h.state()), (0, 4), "the caret after the committed text");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(title_text(h.state()), "a日本b");
}

/// A commit naming the committed text it replaces with no composition (an input method that
/// reconverts without marking first; the vendored winit hands over the range of
/// `insertText:replacementRange:` with the lone `Commit`): it replaces that text, never adds to it.
#[test]
fn a_commit_without_a_composition_replaces_the_text_it_names() {
    let mut h = editing_title("cafe");
    assert_eq!(caret(h.state()), (0, 4));
    let undo0 = undo_len(h.state());
    h.state_mut().ime.replace = Some(3..4);
    h.event(commit("é"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "café", "not cafeé");
    assert_eq!(caret(h.state()), (0, 4));
    assert_eq!(undo_len(h.state()), undo0 + 1);
}

/// A range the input method named belongs to that frame's composition or commit: a dismissal
/// naming one that no commit follows, or one named while the notes pane had the keyboard, is
/// dropped with the frame, so a later commit (a full-width space typed with nothing marked)
/// goes in at the caret instead of replacing text the user never selected.
#[test]
fn a_range_no_commit_took_never_reaches_a_later_commit() {
    let mut h = editing_title("a日本b");
    h.state_mut().run("text.select", json!({"anchor": [0, 1], "caret": [0, 3]})).unwrap();
    h.run_steps(1);
    // Reconverted, then dismissed naming a range, and nothing committed.
    h.state_mut().ime.replace = Some(1..3);
    h.event(preedit_at("にほん", 0..3));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "aにほんb");
    h.state_mut().ime.replace = Some(1..4);
    h.event(preedit(""));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本b", "dismissed: the committed text is back");
    assert!(h.state().ime.replace.is_none(), "dropped with the frame no commit followed in");
    // The caret moves to the end, and a full-width space arrives alone.
    h.state_mut().run("text.select", json!({"anchor": [0, 4], "caret": [0, 4]})).unwrap();
    h.run_steps(1);
    h.event(commit("\u{3000}"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本b\u{3000}", "at the caret, 日本 intact");

    // Named while the notes pane has the keyboard: the slide never sees that frame's events, and
    // the range doesn't wait for the slide's next commit either.
    let id = egui::Id::new("notes_edit");
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run_steps(2);
    assert!(h.ctx.memory(|m| m.has_focus(id)));
    h.state_mut().ime.replace = Some(1..3);
    h.event(preedit("の"));
    h.run_steps(1);
    assert!(h.state().ime.replace.is_none(), "not for the slide's text");
    h.event(preedit(""));
    h.event(commit("の"));
    h.run_steps(1);
    h.ctx.memory_mut(|m| m.surrender_focus(id));
    let title = h.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
    h.state_mut().run("text.edit", json!({"id": title})).unwrap();
    h.state_mut().run("text.select", json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
    h.run_steps(2);
    h.event(commit("\u{3000}"));
    h.run_steps(1);
    assert_eq!(title_text(h.state()), "a日本b\u{3000}\u{3000}", "at the caret, 日本 intact");
}

/// What the input method may read is gathered every frame the slide's text has the keyboard: the
/// title's paragraph, a table cell's; nothing once no text is edited, or while the notes pane (an
/// egui text field, which reports nothing) has the keyboard.
#[test]
fn the_input_method_reads_the_text_being_edited_each_frame() {
    let mut h = editing_title("第1段落\n日本語の文章です");
    h.state_mut().run("text.select", json!({"anchor": [1, 8], "caret": [1, 8]})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ime.surrounding, Some(crate::ime::ImeSurrounding { text: "日本語の文章です".into(), selected: 8..8 }));
    assert!(h.state().ime.rect.is_some(), "the IME position is kept for frames that can't place the caret");
    // A table cell.
    let id = h.state_mut().run("insert.table", json!({"rows": 2, "cols": 2, "rect": [80.0, 300.0, 400.0, 120.0]})).unwrap()["id"].as_u64().unwrap();
    h.state_mut().run("text.edit", json!({"id": id, "cell": [0, 1]})).unwrap();
    h.state_mut().run("text.insert", json!({"text": "セルの文"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ime.surrounding, Some(crate::ime::ImeSurrounding { text: "セルの文".into(), selected: 4..4 }));
    // The table selected, not edited: the input method is on (typing starts editing) but reads nothing.
    h.state_mut().run("text.exit", json!({})).unwrap();
    h.run_steps(2);
    assert!(ime_allowed(&h));
    assert_eq!(h.state().ime.surrounding, None);
    // The notes pane has the keyboard: nothing from the slide.
    h.state_mut().run("text.edit", json!({"id": id, "cell": [0, 1]})).unwrap();
    h.run_steps(2);
    assert!(h.state().ime.surrounding.is_some());
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("notes_edit")));
    h.run_steps(2);
    assert_eq!(h.state().ime.surrounding, None);
}

/// Does some font on this machine (embedded or installed) draw every letter of `s`? Machines
/// without a Korean or Chinese font (a bare Linux runner) can't draw it in the interface either.
fn system_has(s: &str) -> bool {
    s.chars().filter(|c| !c.is_whitespace()).all(|c| deckcraft_fonts::FontDb::global().fallback_for(c, u32::MAX).is_some_and(|f| !f.is_color()))
}

/// #101: a Korean file name, title, notes and comments, a Japanese slide and a Chinese section are
/// drawn with glyphs in the interface (title bar, outline, notes pane, comments), not boxes: faces
/// that draw them are borrowed from the font database. A Latin presentation borrows nothing.
#[test]
fn cjk_text_in_the_interface_is_drawn_with_real_glyphs() {
    let h = harness(Session::with_new());
    assert!(h.state().ui_fonts.faces.is_empty(), "nothing to borrow for {:?}", h.state().session.active().unwrap().title());

    let mut s = Session::with_new();
    let korean = "한국어 발표 자료";
    let japanese = "日本語のスライド";
    let chinese = "简体中文演示文稿";
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.set", &json!({"id": title, "text": japanese})).unwrap();
    s.execute("slide.notes", &json!({"text": "발표자 노트입니다"})).unwrap();
    s.execute("section.add", &json!({"name": chinese})).unwrap();
    s.doc_mut().unwrap().path = Some(format!("{korean}.pptx"));
    let mut h = harness(s);
    h.state_mut().run("view.outline", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.state().session.active().unwrap().title().contains(korean));
    let mut checked = 0;
    for text in [korean, japanese, chinese, "발표자 노트입니다"] {
        if !system_has(text) {
            eprintln!("skipped {text:?}: no font on this machine draws it");
            continue;
        }
        for family in [FontId::proportional(13.0), crate::theme::bold(13.0)] {
            let missing: String = text.chars().filter(|c| !c.is_whitespace() && !h.ctx.fonts_mut(|f| f.has_glyph(&family, *c))).collect();
            assert!(missing.is_empty(), "{family:?} draws {missing:?} of {text:?} as boxes");
        }
        checked += 1;
    }
    if cfg!(windows) || cfg!(target_os = "macos") {
        assert_eq!(checked, 4, "Windows and macOS have Korean and Chinese fonts");
    }
    // Text typed later (a dialog field, a new comment) is looked at too.
    let typed = "ไทย";
    h.event(Event::Text(typed.into()));
    h.run_steps(2);
    if system_has(typed) {
        assert!(h.ctx.fonts_mut(|f| f.has_glyphs(&FontId::proportional(13.0), typed)));
    }
}
