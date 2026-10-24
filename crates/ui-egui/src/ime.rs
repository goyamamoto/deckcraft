//! What the input method may read of the slide's text: the caret's paragraph and the selection in
//! it, reported to the platform each frame ([`ImeState::surrounding`]), and the range of it the
//! input method asks to replace with its composition ([`ImeState::replace`]).
//!
//! Office for Mac lets the input method read committed text (observed in Word for Mac): a
//! selection can be reconverted (再変換 — the Japanese input method marks the selected text's
//! reading and offers candidates for it) and live conversion reads the words before the caret.
//! Both go through
//! `NSTextInputClient`'s `selectedRange` and `attributedSubstringForProposedRange:actualRange:`,
//! which winit answers with nothing; the vendored winit (`vendor/patches/winit/01-macos-ime-
//! surrounding-text.md`) answers from what the desktop app reports here, and hands back the
//! replacement range of `setMarkedText:selectedRange:replacementRange:`, which [`select_range`]
//! turns into the selection the composition then takes the place of (`text.compose` replaces the
//! selection, puts it back when the composition is dismissed, and the commit replaces it for good
//! as one undo step).
//!
//! The text is that of the shape or table cell being edited on the slide. The notes pane and the
//! interface's text fields are egui text edits, which report nothing (as before the patch).
//! Ranges are in UTF-16 code units, as the platform counts them.

use std::ops::Range;

use deckcraft_engine::Session;

/// The text around the caret and the selection within it (UTF-16 code units).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeSurrounding {
    /// The caret's paragraph (a line break as U+000B, a field as its shown text, as the
    /// presentation holds them).
    pub text: String,
    /// The selection, or the caret when collapsed.
    pub selected: Range<usize>,
}

/// The input method's state the canvas and the desktop app share, frame to frame.
#[derive(Clone, Debug, Default)]
pub struct ImeState {
    /// What the input method may read this frame (the caret's paragraph and the selection), set
    /// while the slide's text has the keyboard; the desktop app reports it to the platform.
    pub surrounding: Option<ImeSurrounding>,
    /// The range of `surrounding` (UTF-16 code units) the input method asked to replace with the
    /// composition arriving in this frame's `Preedit`, or the text of its `Commit` (reconversion
    /// of committed text); the desktop app sets it from the platform. It lasts the frame: one
    /// no commit took is dropped once the frame's keyboard input is handled.
    pub replace: Option<Range<usize>>,
    /// The IME position last reported. While the slide's text has the keyboard the position is
    /// reported every frame — this one when the caret can't be placed in a frame — since a frame
    /// without one turns the input method off (egui), which could drop the rest of a conversion
    /// whose first clause had just been committed.
    pub rect: Option<egui::Rect>,
}

/// The paragraph the caret is in, of the text being edited on the slide, and its index. `None`
/// when no slide text is being edited (the notes pane is an egui text field).
fn caret_paragraph(session: &Session) -> Option<(usize, String)> {
    let st = session.active()?;
    let t = st.selection.text.as_ref()?;
    if t.notes {
        return None;
    }
    // A shape whose text body is still empty has an empty paragraph to read.
    let body = deckcraft_engine::cmd::text::body_of(st, t);
    let text = body.and_then(|b| b.paragraphs.get(t.caret.0)).map(|p| p.text()).unwrap_or_default();
    Some((t.caret.0, text))
}

/// The caret's paragraph and the selection within it. A selection reaching outside the paragraph
/// is reported as the caret (the input method could not replace it in one piece anyway).
pub fn surrounding(session: &Session) -> Option<ImeSurrounding> {
    let (para, text) = caret_paragraph(session)?;
    let t = session.active()?.selection.text.as_ref()?;
    let (a, b) = t.ordered();
    let (start, end) = if a.0 == para && b.0 == para { (a.1, b.1) } else { (t.caret.1, t.caret.1) };
    let start = char_to_utf16(&text, start);
    let end = char_to_utf16(&text, end).max(start);
    Some(ImeSurrounding { text, selected: start..end })
}

/// Make `range` (UTF-16 code units of the caret's paragraph) the selection, so the composition
/// takes its place; false (and nothing changes) when it doesn't fit the paragraph.
pub fn select_range(session: &mut Session, range: Range<usize>) -> bool {
    let Some((para, text)) = caret_paragraph(session) else { return false };
    let units = text.encode_utf16().count();
    if range.start > range.end || range.end > units {
        return false;
    }
    let (a, b) = (utf16_to_char(&text, range.start), utf16_to_char(&text, range.end));
    session
        .select(|_, sel| {
            if let Some(t) = sel.text.as_mut() {
                t.anchor = (para, a);
                t.caret = (para, b);
            }
        })
        .is_ok()
}

/// The committed text a `range` the input method named stands for, with the composition taken
/// out of the slide. The input method counts in the text it was shown, composition included;
/// while one is shown it may name a range reaching into committed text — live conversion marks
/// the words before its marked text again to convert them together, and a commit may name the
/// text it replaces — so the range is mapped through the composition: what lies before it keeps
/// its place, the composition itself stands for the text it replaced (a reconverted selection,
/// put back), what follows moves by the difference. Without a composition the range is as given.
pub fn committed_range(session: &mut Session, range: Range<usize>) -> Range<usize> {
    let Some(st) = session.active() else { return range };
    let Some(c) = st.composition.clone() else { return range };
    let Some((para, text)) = caret_paragraph(session) else { return range };
    if para != c.start.0 {
        return range;
    }
    let m = char_to_utf16(&text, c.start.1);
    let l = char_to_utf16(&text, c.start.1.saturating_add(c.len)).saturating_sub(m);
    // Dismissed: the slide and the selection are back to what the composition replaced.
    if session.compose_text("", None).is_err() {
        return range;
    }
    // The selection the composition replaced (within one paragraph), or the caret.
    let r = match (caret_paragraph(session), session.active().and_then(|d| d.selection.text.as_ref())) {
        (Some((p, text)), Some(t)) => {
            let (a, b) = t.ordered();
            if a.0 == p && b.0 == p { char_to_utf16(&text, b.1).saturating_sub(char_to_utf16(&text, a.1)) } else { 0 }
        }
        _ => 0,
    };
    let map = |x: usize| {
        if x <= m {
            x
        } else if x < m.saturating_add(l) {
            m.saturating_add(r)
        } else {
            x.saturating_sub(l).saturating_add(r)
        }
    };
    let start = map(range.start);
    start..map(range.end).max(start)
}

/// UTF-16 code units before character `ch` of `text` (clamped to the text).
pub fn char_to_utf16(text: &str, ch: usize) -> usize {
    text.chars().take(ch).map(char::len_utf16).sum()
}

/// The character offset of `text` after `units` UTF-16 code units (clamped to the text; a unit
/// inside a surrogate pair lands after the pair).
pub fn utf16_to_char(text: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (i, c) in text.chars().enumerate() {
        if seen >= units {
            return i;
        }
        seen = seen.saturating_add(c.len_utf16());
    }
    text.chars().count()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use serde_json::json;

    use super::*;

    fn editing(text: &str) -> Session {
        let mut s = Session::with_new();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("text.edit", &json!({"id": title})).unwrap();
        s.execute("text.insert", &json!({"text": text})).unwrap();
        s
    }

    fn title_text(s: &Session) -> String {
        s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().map(|t| t.text()).unwrap_or_default()
    }

    fn selected_text(s: &Session) -> String {
        let st = s.doc().unwrap();
        let t = st.selection.text.as_ref().unwrap();
        let (a, b) = t.ordered();
        let body = deckcraft_engine::cmd::text::body_of(st, t).unwrap();
        assert_eq!(a.0, b.0, "within one paragraph");
        body.paragraphs[a.0].text().chars().skip(a.1).take(b.1 - a.1).collect()
    }

    #[test]
    fn utf16_offsets_round_trip() {
        let t = "a𠮷b";
        assert_eq!(char_to_utf16(t, 0), 0);
        assert_eq!(char_to_utf16(t, 1), 1);
        assert_eq!(char_to_utf16(t, 2), 3, "a surrogate pair is two units");
        assert_eq!(char_to_utf16(t, 3), 4);
        assert_eq!(char_to_utf16(t, 99), 4);
        assert_eq!(utf16_to_char(t, 0), 0);
        assert_eq!(utf16_to_char(t, 1), 1);
        assert_eq!(utf16_to_char(t, 2), 2, "half a surrogate pair: after it");
        assert_eq!(utf16_to_char(t, 3), 2);
        assert_eq!(utf16_to_char(t, 4), 3);
        assert_eq!(utf16_to_char(t, usize::MAX), 3);
        assert_eq!(char_to_utf16("", 3), 0);
        assert_eq!(utf16_to_char("", 3), 0);
    }

    #[test]
    fn the_input_method_reads_the_paragraph_and_the_selection_in_it() {
        let mut s = editing("a日本語b\ncd");
        s.execute("text.select", &json!({"anchor": [0, 1], "caret": [0, 4]})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "a日本語b".into(), selected: 1..4 }));
        s.execute("text.select", &json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "a日本語b".into(), selected: 5..5 }));
        // Across paragraphs: the caret only, in its paragraph.
        s.execute("text.select", &json!({"anchor": [0, 2], "caret": [1, 1]})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "cd".into(), selected: 1..1 }));
        // Reconversion: the range the input method names becomes the selection.
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 0]})).unwrap();
        let undo0 = s.doc().unwrap().history.undo.len();
        assert!(select_range(&mut s, 1..4));
        assert_eq!(selected_text(&s), "日本語");
        assert!(!select_range(&mut s, 3..9), "outside the paragraph");
        assert!(!select_range(&mut s, std::ops::Range { start: 4, end: 1 }), "reversed");
        assert!(!select_range(&mut s, usize::MAX - 1..usize::MAX), "hostile");
        assert_eq!(selected_text(&s), "日本語", "unchanged");
        assert!(select_range(&mut s, 5..5));
        let t = s.doc().unwrap().selection.text.clone().unwrap();
        assert_eq!((t.anchor, t.caret), ((0, 5), (0, 5)));
        assert_eq!(s.doc().unwrap().history.undo.len(), undo0, "selecting is no undo step");
    }

    #[test]
    fn surrogate_pairs_breaks_and_fields_count_as_the_platform_counts_them() {
        // A surrogate pair is two units, a line break (Shift+Enter) one, a field its shown text.
        let mut s = editing("𠮷野家\u{b}x");
        s.execute("text.select", &json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "𠮷野家\u{b}x".into(), selected: 6..6 }));
        assert!(select_range(&mut s, 0..2), "the pair");
        assert_eq!(selected_text(&s), "𠮷");
        assert!(select_range(&mut s, 1..3), "half a pair stands for the pair's end");
        assert_eq!(selected_text(&s), "野");
        s.execute("text.select", &json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
        s.execute("insert.slideNumber", &json!({})).unwrap();
        let r = surrounding(&s).unwrap();
        assert!(r.text.starts_with("𠮷野家\u{b}x"), "{:?}", r.text);
        assert_eq!(r.selected.start, r.text.encode_utf16().count(), "the caret after the field");
    }

    #[test]
    fn the_input_method_reads_the_text_being_edited_wherever_it_is() {
        // A long paragraph is reported whole.
        let long: String = "日本語の長い文章です".repeat(300);
        let s = editing(&long);
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: long.clone(), selected: 3000..3000 }));
        // A table cell.
        let mut s = Session::with_new();
        let id = s.execute("insert.table", &json!({"rows": 2, "cols": 2, "rect": [80.0, 90.0, 400.0, 120.0]})).unwrap()["id"].as_u64().unwrap();
        s.execute("text.edit", &json!({"id": id, "cell": [1, 0]})).unwrap();
        s.execute("text.insert", &json!({"text": "セルの文"})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "セルの文".into(), selected: 4..4 }));
        s.execute("text.select", &json!({"anchor": [0, 2], "caret": [0, 4]})).unwrap();
        assert_eq!(surrounding(&s).unwrap().selected, 2..4);
        assert!(select_range(&mut s, 0..2));
        assert_eq!(selected_text(&s), "セル");
        // An empty shape being edited: an empty paragraph.
        let mut s = Session::with_new();
        let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100]})).unwrap();
        s.execute("text.edit", &json!({"id": r["id"]})).unwrap();
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: String::new(), selected: 0..0 }));
        // A selected shape not being edited, or the speaker notes: nothing.
        s.execute("text.exit", &json!({})).unwrap();
        assert_eq!(surrounding(&s), None);
        assert!(!select_range(&mut s, 0..0));
        s.execute("slide.notes", &json!({"text": "ノート"})).unwrap();
        s.execute("text.edit", &json!({"notes": true})).unwrap();
        assert!(s.doc().unwrap().selection.text.as_ref().unwrap().notes);
        assert_eq!(surrounding(&s), None);
        assert!(!select_range(&mut s, 0..1));
    }

    #[test]
    fn a_range_named_while_composing_maps_through_the_composition() {
        // "a日本b", the caret before b: the composition ご shows as a日本ごb (the input method's
        // text); a range it names there stands for committed text once the composition is out.
        let mut s = editing("a日本b");
        s.execute("text.select", &json!({"anchor": [0, 3], "caret": [0, 3]})).unwrap();
        let undo0 = s.doc().unwrap().history.undo.len();
        assert_eq!(committed_range(&mut s, 1..3), 1..3, "no composition: as given");
        s.compose_text("ご", None).unwrap();
        assert_eq!(title_text(&s), "a日本ごb");
        assert_eq!(surrounding(&s), Some(ImeSurrounding { text: "a日本ごb".into(), selected: 4..4 }), "the composition is in the text read");
        // Live conversion marks 日本ご (1..4) again: 日本 (1..3) is the committed text it stands for.
        assert_eq!(committed_range(&mut s, 1..4), 1..3);
        assert!(s.doc().unwrap().composition.is_none());
        assert_eq!(title_text(&s), "a日本b");
        // After the composition: moved back by its length. Inside it: the (empty) text it replaced.
        s.compose_text("ご", None).unwrap();
        assert_eq!(committed_range(&mut s, 4..5), 3..4);
        s.compose_text("ごはん", None).unwrap();
        assert_eq!(committed_range(&mut s, 3..5), 3..3);
        // A composition that replaced a selection (再変換): it stands for that text, put back.
        s.execute("text.select", &json!({"anchor": [0, 1], "caret": [0, 3]})).unwrap();
        s.compose_text("にほん", Some((0, 3))).unwrap();
        assert_eq!(title_text(&s), "aにほんb");
        assert_eq!(committed_range(&mut s, 0..4), 0..3, "a + the composition = a日本");
        assert_eq!(title_text(&s), "a日本b");
        s.compose_text("にほん", Some((0, 3))).unwrap();
        assert_eq!(committed_range(&mut s, 4..5), 3..4, "b after it");
        assert_eq!(selected_text(&s), "日本", "the selection it replaced is back");
        assert_eq!(s.doc().unwrap().history.undo.len(), undo0, "nothing of it is an undo step");
        // Hostile ranges are clamped by the mapping, never panic.
        s.compose_text("に", None).unwrap();
        let r = committed_range(&mut s, usize::MAX - 1..usize::MAX);
        assert!(r.start <= r.end);
        assert!(!select_range(&mut s, r));
    }
}
