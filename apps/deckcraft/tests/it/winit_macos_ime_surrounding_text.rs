//! vendor/winit patch 01 (`vendor/patches/winit/01-macos-ime-surrounding-text.md`, "D1" on
//! storytold/winit): on macOS the application reports the text around the caret, so the input
//! method can read committed text and reconvert it (再変換), and the replacement range it names
//! is applied by the application rather than by storytold's W26 deletion. A re-vendored copy
//! without the patch would silently lose it. The fork carries its own guards too
//! (`vendor/winit/tests/ime_patches.rs`, the native AppKit driver
//! `vendor/winit/tests/macos_ime_surrounding.rs`).

#[test]
fn winit_answers_the_input_method_from_the_reported_surrounding_text() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let view = std::fs::read_to_string(root.join("vendor/winit/src/platform_impl/macos/view.rs")).unwrap();
    let body = |from: &str, to: &str| -> String {
        let start = view.find(from).unwrap_or_else(|| panic!("{from} is in view.rs"));
        let end = view[start..].find(to).map_or(view.len(), |e| start + e);
        view[start..end].to_string()
    };
    // One mode at a time: while text is reported, W26 stands down (nothing is deleted by byte
    // count, so `take_ime_delete_surrounding` is `None` and egui-winit emits no
    // `DeleteSurroundingBytes`); the application applies the named range itself.
    let replaced = body("fn replaced(&self, range: NSRange)", "fn reported_surrounding(");
    let stand_down = replaced
        .find("if self.reported_surrounding().is_some() {\n            return None;\n        }")
        .expect("W26 stands down while text is reported");
    assert!(stand_down < replaced.find("ime_document.borrow_mut().replace(range)").unwrap_or(0), "before ImeDocument computes a deletion");
    // Answers from the reported text: the marked text's place, the selection, the text itself.
    let marked_range = body("fn marked_range(&self) -> NSRange", "#[method(selectedRange)]");
    assert!(marked_range.contains("return NSRange::new(self.ivars().marked_location.get(), length);"), "markedRange lies in the reported text");
    let selected_range = body("fn selected_range(&self) -> NSRange", "#[method(setMarkedText:selectedRange:replacementRange:)]");
    assert!(selected_range.contains("if let Some(reported) = self.reported_surrounding() {"), "selectedRange answers the reported selection");
    let substring = body("fn attributed_substring_for_proposed_range(", "#[method(characterIndexForPoint:)]");
    assert!(substring.contains("let (text, actual) = reported.substring(range);"), "attributedSubstringForProposedRange answers the reported text");
    // The named range is handed to the application (`take_ime_replacement_range`) by
    // `setMarkedText:` and by every commit, and the reported text follows the edit at once.
    let set_marked = body("fn set_marked_text(", "#[method(unmarkText)]");
    assert!(set_marked.contains("ime_surrounding::record_replacement(range);"), "setMarkedText records the range it names");
    assert!(set_marked.contains("self.ime_update_surrounding("), "setMarkedText updates the surrounding text");
    assert!(!view.contains("TODO: Use _replacement_range"), "the replacement range is no longer ignored");
    let commit = body("fn ime_commit_surrounding(", "fn ime_key_text_commits(");
    assert!(commit.contains("ime_surrounding::record_replacement(range);"), "a commit records the range it names");
    // `unmarkText` commits the marked text as it stands, except while the view itself discards
    // it (W7/W24: reset, IME disallowed), which cancels the composition the app has settled.
    let unmark = body("fn ime_unmark_commit(", "fn ime_settle_surrounding(");
    assert!(unmark.contains("if self.ivars().ime_discarding.get()"), "a discard doesn't commit");
    let surrounding = std::fs::read_to_string(root.join("vendor/winit/src/platform_impl/macos/ime_surrounding.rs")).unwrap();
    assert!(surrounding.contains("pub fn apply(\n        &self,\n        marked: Option<&ImeMarked>,"), "ImeSurroundingText::apply");
    assert!(surrounding.contains("std::env::var_os(\"DECKCRAFT_IME_TRACE\")"), "the opt-in IME trace");
    let fork_guards = std::fs::read_to_string(root.join("vendor/winit/tests/ime_patches.rs")).unwrap();
    assert!(fork_guards.contains("fn macos_reported_surrounding_text_answers_the_input_method()"), "the fork's own guard");
    assert!(root.join("vendor/winit/tests/macos_ime_surrounding.rs").is_file(), "the fork's native AppKit driver");

    let platform = std::fs::read_to_string(root.join("vendor/winit/src/platform/macos.rs")).unwrap();
    assert!(platform.contains("pub fn set_ime_surrounding_text(text: Option<ImeSurroundingText>)"), "public on winit::platform::macos");
    assert!(platform.contains("pub fn take_ime_replacement_range() -> Option<std::ops::Range<usize>>"));
    assert!(platform.contains("pub fn ime_surrounding_text() -> Option<ImeSurroundingText>"));
    assert!(platform.contains("pub use crate::platform_impl::{ImeMarked, ImeSurroundingText};"));

    // The desktop app uses it: the canvas gathers the paragraph, the hooks report it and take the
    // range.
    let main = std::fs::read_to_string(root.join("apps/deckcraft/src/main.rs")).unwrap();
    assert!(main.contains("winit::platform::macos::set_ime_surrounding_text("));
    assert!(main.contains("winit::platform::macos::take_ime_replacement_range()"));
    // The range belongs to the frame whose Preedit or Commit came with it: taken every frame (a
    // commit-only frame carries one too), never left for a later commit elsewhere.
    assert!(main.contains("egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_)"), "a commit-only frame takes the range named with it");
    // The input method talks to the focused window, whose input this is: the range goes to that
    // window's app through the window host.
    assert!(main.contains("w.app.ime.replace = named.filter("), "every frame replaces the last frame's range");
}

/// The text the input method reads right after it changed the marked text, before the next
/// frame reports the slide's text again (an input method that reads the text left of the caret
/// after `setMarkedText("")` would see its own dismissed suggestion as committed text;
/// `NSTextView` answers the changed state at once). Ranges in UTF-16 code units.
#[cfg(target_os = "macos")]
#[test]
fn the_reported_text_follows_the_input_methods_edits_at_once() {
    use winit::platform::macos::{ImeMarked, ImeSurroundingText};
    let doc = |text: &str, sel: std::ops::Range<usize>| ImeSurroundingText { text: text.into(), selected: sel };
    let left = |s: &ImeSurroundingText| {
        let units: Vec<u16> = s.text.encode_utf16().collect();
        String::from_utf16_lossy(&units[..s.selected.start.min(units.len())])
    };

    // 1. A suggestion shown as marked text at the end of 今日は雨だった。 (8 units), then taken
    //    back with `setMarkedText("")` before any frame: the caret is at 8 and the text before it
    //    the committed sentence, nothing of the suggestion.
    let s0 = doc("今日は雨だった。", 8..8);
    let (s1, m1) = s0.apply(None, 8..8, "明日は晴れるでしょう。", Some(0..0));
    assert_eq!((s1.text.as_str(), s1.selected.clone()), ("今日は雨だった。明日は晴れるでしょう。", 8..8));
    assert_eq!(m1, Some(ImeMarked { location: 8, len: 11, replaced: String::new() }));
    let (s2, m2) = s1.apply(m1.as_ref(), 8..19, "", Some(0..0));
    assert_eq!((s2.text.as_str(), s2.selected.clone(), m2), ("今日は雨だった。", 8..8, None));
    assert_eq!(left(&s2), "今日は雨だった。");

    // 2. A notice in brackets taken back the same way: no （ is left before the caret.
    let (s1, m1) = s0.apply(None, 8..8, "（続きはありません）", Some(0..0));
    let (s2, _) = s1.apply(m1.as_ref(), 8..8 + 10, "", Some(0..0));
    assert!(!left(&s2).contains('（'), "{:?}", left(&s2));
    assert_eq!(s2.selected, 8..8);
    // A "…" shown with the selection at its end, then taken back.
    let (s1, m1) = s0.apply(None, 8..8, "…", Some(1..1));
    assert_eq!(s1.selected, 9..9);
    let (s2, _) = s1.apply(m1.as_ref(), 8..9, "", Some(0..0));
    assert_eq!((s2.text.as_str(), s2.selected.clone()), ("今日は雨だった。", 8..8));

    // 3. にほんご marked, then `insertText("日本語")` before any frame: the caret is right after
    //    日本語 and the text before it ends with it.
    let (s1, m1) = s0.apply(None, 8..8, "にほんご", Some(0..4));
    assert_eq!((s1.text.as_str(), s1.selected.clone()), ("今日は雨だった。にほんご", 8..12));
    let (s2, m2) = s1.apply(m1.as_ref(), 8..12, "日本語", None);
    assert_eq!((s2.text.as_str(), s2.selected.clone(), m2), ("今日は雨だった。日本語", 11..11, None));
    assert!(left(&s2).ends_with("日本語"));

    // 4. Reconversion: the selected committed 日本 replaced by marked にほん with a replacement
    //    range; dismissed, 日本 is back and selected (the application puts it back too);
    //    committed, the commit takes its place.
    let s0 = doc("a日本b", 1..3);
    let (s1, m1) = s0.apply(None, 1..3, "にほん", Some(0..3));
    assert_eq!((s1.text.as_str(), s1.selected.clone()), ("aにほんb", 1..4));
    assert_eq!(m1, Some(ImeMarked { location: 1, len: 3, replaced: "日本".into() }));
    let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "", Some(0..0));
    assert_eq!((s2.text.as_str(), s2.selected.clone(), m2), ("a日本b", 1..3, None));
    let (s3, _) = s1.apply(m1.as_ref(), 1..4, "二本", None);
    assert_eq!((s3.text.as_str(), s3.selected.clone()), ("a二本b", 3..3));
    // Live conversion: ご marked after 日本, then 日本ご (1..4, reaching into committed text)
    // marked again as にほんご: it stands for 日本; dismissed, 日本 is back.
    let s0 = doc("a日本b", 3..3);
    let (s1, m1) = s0.apply(None, 3..3, "ご", Some(1..1));
    assert_eq!((s1.text.as_str(), s1.selected.clone()), ("a日本ごb", 4..4));
    let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "にほんご", Some(4..4));
    assert_eq!((s2.text.as_str(), s2.selected.clone()), ("aにほんごb", 5..5));
    assert_eq!(m2, Some(ImeMarked { location: 1, len: 4, replaced: "日本".into() }));
    let (s3, m3) = s2.apply(m2.as_ref(), 1..5, "", Some(0..0));
    assert_eq!((s3.text.as_str(), s3.selected.clone(), m3), ("a日本b", 1..3, None));
    // Committed as 日本語: in place of 日本 and the marked text, the caret after it.
    let (s4, _) = s2.apply(m2.as_ref(), 1..5, "日本語", None);
    assert_eq!((s4.text.as_str(), s4.selected.clone()), ("a日本語b", 4..4));
    // `unmarkText`: the marked text committed as it stands.
    let (s5, m5) = s2.apply(m2.as_ref(), 1..5, "にほんご", None);
    assert_eq!((s5.text.as_str(), s5.selected.clone(), m5), ("aにほんごb", 5..5, None));

    // Hostile ranges and selections are clamped, never panic.
    let s0 = doc("𠮷野家", 2..2);
    let (s1, m1) = s0.apply(None, 9..99, "x", Some(5..9));
    assert_eq!((s1.text.as_str(), s1.selected.clone()), ("𠮷野家x", 5..5));
    let stale = ImeMarked { location: 50, len: 50, replaced: "zz".into() };
    let (s2, _) = s1.apply(Some(&stale), 0..1, "", Some(0..0));
    assert_eq!(s2.text, "𠮷野家xzz");
    let _ = s1.apply(m1.as_ref(), std::ops::Range { start: 4, end: 1 }, "y", None);
    let _ = s1.apply(m1.as_ref(), usize::MAX - 1..usize::MAX, "y", Some(usize::MAX..usize::MAX));

    // The store itself: what the application reports is what the input method reads.
    winit::platform::macos::set_ime_surrounding_text(Some(doc("a日本b", 1..3)));
    assert_eq!(winit::platform::macos::ime_surrounding_text(), Some(doc("a日本b", 1..3)));
    assert_eq!(winit::platform::macos::take_ime_replacement_range(), None, "no replacement named");
    winit::platform::macos::set_ime_surrounding_text(None);
    assert_eq!(winit::platform::macos::ime_surrounding_text(), None);
}
