//! DeckCraft patch (`01-macos-ime-surrounding-text.md`): the text around the caret that the app
//! reports, so the input method can read committed text and ask for a range of it to be replaced
//! (再変換 — reconversion of a selection — and context for live conversion).
//!
//! While the app reports text (`set_ime_surrounding_text(Some(…))`), `WinitView` answers
//! `selectedRange`, `markedRange` and `attributedSubstringForProposedRange:actualRange:` from it
//! instead of from `ImeDocument`, and hands the replacement ranges the input method names to the
//! app (`take_ime_replacement_range`) instead of deleting the replaced text by byte count (W26).
//! With nothing reported (`None`, the default), the view behaves as without this patch.
//!
//! One window takes keyboard input at a time, so one store for the process suffices and the app
//! needs no handle to the window (eframe keeps it).
//!
//! Offsets are UTF-16 code units, as in `NSRange`.

use std::ops::Range;
use std::sync::Mutex;

/// The text around the caret of the window taking keyboard input, as the app reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeSurroundingText {
    /// The text the input method may read (the caret's paragraph, say), UTF-8.
    pub text: String,
    /// The selection within `text` in UTF-16 code units; collapsed, it is the caret.
    pub selected: Range<usize>,
}

/// The marked text's place in the surrounding text: where it lies and its length, in UTF-16 code
/// units, and the committed text it stands for (a reconverted selection, or nothing).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeMarked {
    pub location: usize,
    pub len: usize,
    pub replaced: String,
}

impl ImeSurroundingText {
    /// This text as the input method has just changed it, until the app reports it again: `text`
    /// put in place of `range` — UTF-16 code units of this text, the marked text `marked`
    /// included, as the input method counts — as marked text with the input method's selection
    /// `selected` within it, or committed (`None`).
    ///
    /// A range reaching into the marked text counts it as the committed text it stands for.
    /// Empty marked text takes the marked text out and selects what it stood for, as the app
    /// does. Returns the new text with its selection, and the marked text's place in it, if any.
    /// Ranges and selections out of bounds are clamped.
    pub fn apply(
        &self,
        marked: Option<&ImeMarked>,
        range: Range<usize>,
        text: &str,
        selected: Option<Range<usize>>,
    ) -> (ImeSurroundingText, Option<ImeMarked>) {
        // The committed text: the marked text replaced by what it stands for, the range mapped
        // onto it (what lies before keeps its place, the marked text is what it stood for, what
        // follows moves by the difference).
        let mut units: Vec<u16> = self.text.encode_utf16().collect();
        let (m, l, r) = match marked {
            Some(k) => {
                let m = k.location.min(units.len());
                let l = k.len.min(units.len() - m);
                let put_back: Vec<u16> = k.replaced.encode_utf16().collect();
                let r = put_back.len();
                units.splice(m..m + l, put_back);
                (m, l, r)
            },
            None => (0, 0, 0),
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
        let start = map(range.start).min(units.len());
        let end = map(range.end).clamp(start, units.len());
        let replaced = String::from_utf16_lossy(&units[start..end]);
        let new: Vec<u16> = text.encode_utf16().collect();
        let n = new.len();
        match selected {
            // Dismissed: the committed text is back, what the marked text stood for selected.
            Some(_) if n == 0 => (
                ImeSurroundingText { text: String::from_utf16_lossy(&units), selected: start..end },
                None,
            ),
            Some(sel) => {
                units.splice(start..end, new);
                let a = sel.start.min(n);
                let b = sel.end.clamp(a, n);
                let text = String::from_utf16_lossy(&units);
                (
                    ImeSurroundingText { text, selected: start + a..start + b },
                    Some(ImeMarked { location: start, len: n, replaced }),
                )
            },
            None => {
                units.splice(start..end, new);
                let caret = start + n;
                (
                    ImeSurroundingText {
                        text: String::from_utf16_lossy(&units),
                        selected: caret..caret,
                    },
                    None,
                )
            },
        }
    }

    /// The selection, clamped to the text and ordered.
    pub(crate) fn selection(&self) -> Range<usize> {
        let len = utf16_len(&self.text);
        let start = self.selected.start.min(self.selected.end).min(len);
        start..self.selected.end.max(self.selected.start).clamp(start, len)
    }

    /// `range` of the text, clamped to it, with the range it covers.
    pub(crate) fn substring(&self, range: Range<usize>) -> (String, Range<usize>) {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let start = range.start.min(units.len());
        let end = range.end.clamp(start, units.len());
        (String::from_utf16_lossy(&units[start..end]), start..end)
    }
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

static SURROUNDING: Mutex<Option<ImeSurroundingText>> = Mutex::new(None);
static REPLACEMENT: Mutex<Option<Range<usize>>> = Mutex::new(None);

/// Tell the input method what text lies around the caret (`None`: nothing, as without the patch).
pub fn set_ime_surrounding_text(text: Option<ImeSurroundingText>) {
    if let Ok(mut s) = SURROUNDING.lock() {
        *s = text;
    }
}

/// What the input method is told lies around the caret right now.
pub fn ime_surrounding_text() -> Option<ImeSurroundingText> {
    SURROUNDING.lock().ok().and_then(|s| s.clone())
}

/// The range of the surrounding text (UTF-16 code units) the input method asked to replace with
/// its marked text or its commit, if one was named since the last call; the `Ime::Preedit` or
/// `Ime::Commit` event carrying that text follows it.
pub fn take_ime_replacement_range() -> Option<Range<usize>> {
    REPLACEMENT.lock().ok().and_then(|mut r| r.take())
}

/// Hand the app a replacement range (`take_ime_replacement_range`); the latest one wins.
pub(crate) fn record_replacement(range: Range<usize>) {
    if let Ok(mut r) = REPLACEMENT.lock() {
        *r = Some(range);
    }
}

/// A record of what the input method asks of the view, written when `DECKCRAFT_IME_TRACE` names a
/// file (each call, its text and ranges, the IME state): an input method's real calls can't be
/// reproduced without typing.
pub(crate) fn trace(line: impl FnOnce() -> String) {
    use std::io::Write;
    static FILE: std::sync::OnceLock<Option<Mutex<std::fs::File>>> = std::sync::OnceLock::new();
    let file = FILE.get_or_init(|| {
        let path = std::env::var_os("DECKCRAFT_IME_TRACE")?;
        std::fs::OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    });
    if let Some(f) = file {
        if let Ok(mut f) = f.lock() {
            let _ = writeln!(f, "{}", line());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str, selected: Range<usize>) -> ImeSurroundingText {
        ImeSurroundingText { text: text.into(), selected }
    }

    fn state(s: &ImeSurroundingText) -> (&str, Range<usize>) {
        (s.text.as_str(), s.selected.clone())
    }

    /// The text before the caret.
    fn left(s: &ImeSurroundingText) -> String {
        s.substring(0..s.selection().start).0
    }

    #[test]
    fn marked_text_replaces_the_selection_without_a_named_range() {
        // にほん typed with 日本 selected: the marked text takes the selection's place and stands
        // for it.
        let s0 = doc("a日本b", 1..3);
        let (s1, m1) = s0.apply(None, 1..3, "にほん", Some(3..3));
        assert_eq!(state(&s1), ("aにほんb", 4..4));
        assert_eq!(m1, Some(ImeMarked { location: 1, len: 3, replaced: "日本".into() }));
        // A collapsed selection: the marked text goes at the caret and stands for nothing.
        let (s1, m1) = doc("ab", 1..1).apply(None, 1..1, "か", Some(1..1));
        assert_eq!(state(&s1), ("aかb", 2..2));
        assert_eq!(m1, Some(ImeMarked { location: 1, len: 1, replaced: String::new() }));
    }

    #[test]
    fn marked_text_replaces_the_marked_text() {
        // か, then かん: the old marked text is replaced, the selection is the input method's.
        let s0 = doc("ab", 1..1);
        let (s1, m1) = s0.apply(None, 1..1, "か", Some(1..1));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..2, "かん", Some(0..2));
        assert_eq!(state(&s2), ("aかんb", 1..3));
        assert_eq!(m2, Some(ImeMarked { location: 1, len: 2, replaced: String::new() }));
        // A reconversion keeps standing for the selection it replaced.
        let (s1, m1) = doc("a日本b", 1..3).apply(None, 1..3, "にほん", Some(0..3));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "二本", Some(0..2));
        assert_eq!(state(&s2), ("a二本b", 1..3));
        assert_eq!(m2.unwrap().replaced, "日本");
    }

    #[test]
    fn marked_text_replaces_a_named_range() {
        // Reconversion with a named range (the selection's): 日本 replaced by にほん.
        let s0 = doc("a日本b", 3..3);
        let (s1, m1) = s0.apply(None, 1..3, "にほん", Some(0..3));
        assert_eq!(state(&s1), ("aにほんb", 1..4));
        assert_eq!(m1, Some(ImeMarked { location: 1, len: 3, replaced: "日本".into() }));
        // Live conversion: ご marked after 日本, then 日本ご (1..4, reaching into committed text)
        // marked again as にほんご: it stands for 日本.
        let (s1, m1) = s0.apply(None, 3..3, "ご", Some(1..1));
        assert_eq!(state(&s1), ("a日本ごb", 4..4));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "にほんご", Some(4..4));
        assert_eq!(state(&s2), ("aにほんごb", 5..5));
        assert_eq!(m2, Some(ImeMarked { location: 1, len: 4, replaced: "日本".into() }));
        // A named range after the marked text moves by the difference: b (4..5 with ご shown)
        // is b (3..4) of the committed text, and the marked text ご it doesn't reach is dropped.
        let (s2, m2) = s1.apply(m1.as_ref(), 4..5, "び", Some(1..1));
        assert_eq!(state(&s2), ("a日本び", 4..4));
        assert_eq!(m2.map(|k| (k.location, k.replaced)), Some((3, "b".to_owned())));
    }

    #[test]
    fn empty_marked_text_puts_back_what_it_replaced() {
        // A suggestion shown at the end of 今日は雨だった。 (8 units), then taken back: the text
        // before the caret is the committed sentence, nothing of the suggestion.
        let s0 = doc("今日は雨だった。", 8..8);
        let (s1, m1) = s0.apply(None, 8..8, "明日は晴れるでしょう。", Some(0..0));
        assert_eq!(state(&s1), ("今日は雨だった。明日は晴れるでしょう。", 8..8));
        let (s2, m2) = s1.apply(m1.as_ref(), 8..19, "", Some(0..0));
        assert_eq!((state(&s2), m2), (("今日は雨だった。", 8..8), None));
        assert_eq!(left(&s2), "今日は雨だった。");
        // A notice in brackets, taken back the same way: no （ is left before the caret.
        let (s1, m1) = s0.apply(None, 8..8, "（続きはありません）", Some(0..0));
        let (s2, _) = s1.apply(m1.as_ref(), 8..18, "", Some(0..0));
        assert!(!left(&s2).contains('（'), "{:?}", left(&s2));
        // A reconversion dismissed: 日本 is back and selected (the app puts it back too).
        let (s1, m1) = doc("a日本b", 1..3).apply(None, 1..3, "にほん", Some(0..3));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "", Some(0..0));
        assert_eq!((state(&s2), m2), (("a日本b", 1..3), None));
        // Live conversion dismissed after re-marking 日本ご: 日本 is back.
        let (s1, m1) = doc("a日本b", 3..3).apply(None, 3..3, "ご", Some(1..1));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "にほんご", Some(4..4));
        let (s3, m3) = s2.apply(m2.as_ref(), 1..5, "", Some(0..0));
        assert_eq!((state(&s3), m3), (("a日本b", 1..3), None));
    }

    #[test]
    fn insert_text_while_marked_commits_in_place_of_the_marked_text() {
        // にほんご marked, then committed as 日本語: the caret is right after it.
        let s0 = doc("今日は雨だった。", 8..8);
        let (s1, m1) = s0.apply(None, 8..8, "にほんご", Some(0..4));
        assert_eq!(state(&s1), ("今日は雨だった。にほんご", 8..12));
        let (s2, m2) = s1.apply(m1.as_ref(), 8..12, "日本語", None);
        assert_eq!((state(&s2), m2), (("今日は雨だった。日本語", 11..11), None));
        assert!(left(&s2).ends_with("日本語"));
        // A reconversion committed: the commit takes 日本's place.
        let (s1, m1) = doc("a日本b", 1..3).apply(None, 1..3, "にほん", Some(0..3));
        let (s2, _) = s1.apply(m1.as_ref(), 1..4, "二本", None);
        assert_eq!(state(&s2), ("a二本b", 3..3));
        // Live conversion re-marked 日本ご, committed as 日本語 over 日本 and the marked text.
        let (s1, m1) = doc("a日本b", 3..3).apply(None, 3..3, "ご", Some(1..1));
        let (s2, m2) = s1.apply(m1.as_ref(), 1..4, "にほんご", Some(4..4));
        let (s3, _) = s2.apply(m2.as_ref(), 1..5, "日本語", None);
        assert_eq!(state(&s3), ("a日本語b", 4..4));
        // `unmarkText`: the marked text committed as it stands.
        let (s4, m4) = s2.apply(m2.as_ref(), 1..5, "にほんご", None);
        assert_eq!((state(&s4), m4), (("aにほんごb", 5..5), None));
        // Press-and-hold without marked text: é over the e just typed (a named range).
        let (s1, m1) = doc("cafe", 4..4).apply(None, 3..4, "é", None);
        assert_eq!((state(&s1), m1), (("café", 4..4), None));
    }

    #[test]
    fn hostile_ranges_and_selections_are_clamped() {
        let s0 = doc("𠮷野家", 2..2);
        let (s1, m1) = s0.apply(None, 9..99, "x", Some(5..9));
        assert_eq!(state(&s1), ("𠮷野家x", 5..5));
        // Stale marked text beyond the end: what it stood for goes at the end.
        let stale = ImeMarked { location: 50, len: 50, replaced: "zz".into() };
        let (s2, _) = s1.apply(Some(&stale), 0..1, "", Some(0..0));
        assert_eq!(s2.text, "𠮷野家xzz");
        #[allow(clippy::reversed_empty_ranges)]
        let (reversed, backwards) = (4..1, 5..1);
        let _ = s1.apply(m1.as_ref(), reversed, "y", None);
        let _ =
            s1.apply(m1.as_ref(), usize::MAX - 1..usize::MAX, "y", Some(usize::MAX..usize::MAX));
        let _ = s1.apply(None, usize::MAX..usize::MAX, "", Some(0..0));
        // Half a surrogate pair is replaced by U+FFFD, never a panic.
        let (s3, _) = s0.apply(None, 1..1, "a", None);
        assert_eq!(s3.text.chars().count(), 5);
        // What the view answers: a reversed or out-of-bounds report is clamped and ordered.
        assert_eq!(doc("abc", backwards).selection(), 1..3);
        assert_eq!(doc("abc", 9..9).selection(), 3..3);
        assert_eq!(doc("a日本b", 0..0).substring(1..99), ("日本b".to_owned(), 1..4));
        assert_eq!(doc("abc", 0..0).substring(7..9), (String::new(), 3..3));
    }

    #[test]
    fn the_store_hands_over_what_the_app_reports() {
        set_ime_surrounding_text(Some(doc("a日本b", 1..3)));
        assert_eq!(ime_surrounding_text(), Some(doc("a日本b", 1..3)));
        assert_eq!(take_ime_replacement_range(), None, "no replacement named");
        record_replacement(1..3);
        record_replacement(0..2);
        assert_eq!(take_ime_replacement_range(), Some(0..2), "the latest range");
        assert_eq!(take_ime_replacement_range(), None, "taken once");
        set_ime_surrounding_text(None);
        assert_eq!(ime_surrounding_text(), None);
    }
}
