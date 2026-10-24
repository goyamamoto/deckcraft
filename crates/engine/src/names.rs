//! File names as people read them. macOS (and zip archives or Git checkouts made there) often
//! store Japanese names decomposed: `プ` as `フ` followed by the combining handakuten U+309A, `ゼ`
//! as `セ` and the combining dakuten U+3099. PowerPoint for Mac shows such names composed in the
//! title bar and the Recent list. The interface draws each code point on its own, so a decomposed
//! name would show its voiced mark as a separate `゛`; [`display_name`] composes the marks for
//! display, and the real path is kept for reading and writing.

use std::borrow::Cow;

/// Combining katakana-hiragana voiced sound mark (dakuten).
const DAKUTEN: char = '\u{3099}';
/// Combining katakana-hiragana semi-voiced sound mark (handakuten).
const HANDAKUTEN: char = '\u{309A}';

/// `name` with every kana followed by a combining dakuten or handakuten replaced by its composed
/// form, as Unicode canonical composition (NFC) does (`か` + U+3099 → `が`, `ハ` + U+309A → `パ`,
/// `ウ` + U+3099 → `ヴ`). A mark after anything else is left as it is.
pub fn display_name(name: &str) -> String {
    compose_kana(name).into_owned()
}

/// [`display_name`] without a copy when nothing needs composing.
pub fn compose_kana(name: &str) -> Cow<'_, str> {
    if !name.contains([DAKUTEN, HANDAKUTEN]) {
        return Cow::Borrowed(name);
    }
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        let composed = match chars.peek() {
            Some(&DAKUTEN) => voiced(c),
            Some(&HANDAKUTEN) => semi_voiced(c),
            _ => None,
        };
        match composed {
            Some(v) => {
                out.push(v);
                chars.next();
            }
            None => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// The kana `c` with a dakuten (NFC's compositions with U+3099).
fn voiced(c: char) -> Option<char> {
    let u = c as u32;
    let v = match u {
        // Hiragana か–ぢ, つ–ど and は–ほ (every other or every third code point), う and ゝ.
        0x304B..=0x3061 if (u - 0x304B).is_multiple_of(2) => u + 1,
        0x3064..=0x3068 if (u - 0x3064).is_multiple_of(2) => u + 1,
        0x306F..=0x307B if (u - 0x306F).is_multiple_of(3) => u + 1,
        0x3046 => 0x3094,
        0x309D => 0x309E,
        // Katakana カ–ヂ, ツ–ド and ハ–ホ, ウ, ワ–ヲ and ヽ.
        0x30AB..=0x30C1 if (u - 0x30AB).is_multiple_of(2) => u + 1,
        0x30C4..=0x30C8 if (u - 0x30C4).is_multiple_of(2) => u + 1,
        0x30CF..=0x30DB if (u - 0x30CF).is_multiple_of(3) => u + 1,
        0x30A6 => 0x30F4,
        0x30EF..=0x30F2 => u + 8,
        0x30FD => 0x30FE,
        _ => return None,
    };
    char::from_u32(v)
}

/// The kana `c` with a handakuten (NFC's compositions with U+309A): は–ほ and ハ–ホ.
fn semi_voiced(c: char) -> Option<char> {
    let u = c as u32;
    let v = match u {
        0x306F..=0x307B if (u - 0x306F).is_multiple_of(3) => u + 2,
        0x30CF..=0x30DB if (u - 0x30CF).is_multiple_of(3) => u + 2,
        _ => return None,
    };
    char::from_u32(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every voiced and semi-voiced kana written decomposed composes back to itself.
    #[test]
    fn decomposed_kana_compose() {
        let composed = "がぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽゔゞガギグゲゴザジズゼゾダヂヅデドバビブベボパピプペポヴヷヸヹヺヾ";
        let decomposed = "か\u{3099}き\u{3099}く\u{3099}け\u{3099}こ\u{3099}さ\u{3099}し\u{3099}す\u{3099}せ\u{3099}そ\u{3099}\
            た\u{3099}ち\u{3099}つ\u{3099}て\u{3099}と\u{3099}は\u{3099}ひ\u{3099}ふ\u{3099}へ\u{3099}ほ\u{3099}\
            は\u{309A}ひ\u{309A}ふ\u{309A}へ\u{309A}ほ\u{309A}う\u{3099}ゝ\u{3099}\
            カ\u{3099}キ\u{3099}ク\u{3099}ケ\u{3099}コ\u{3099}サ\u{3099}シ\u{3099}ス\u{3099}セ\u{3099}ソ\u{3099}\
            タ\u{3099}チ\u{3099}ツ\u{3099}テ\u{3099}ト\u{3099}ハ\u{3099}ヒ\u{3099}フ\u{3099}ヘ\u{3099}ホ\u{3099}\
            ハ\u{309A}ヒ\u{309A}フ\u{309A}ヘ\u{309A}ホ\u{309A}ウ\u{3099}ワ\u{3099}ヰ\u{3099}ヱ\u{3099}ヲ\u{3099}ヽ\u{3099}";
        assert_eq!(display_name(decomposed), composed);
        assert_eq!(display_name("フ\u{3099}レセ\u{3099}ン資料"), "ブレゼン資料");
        assert_eq!(display_name("フ\u{309A}レセ\u{3099}ン"), "プレゼン");
    }

    /// Names without a decomposed mark come back as they are; a mark after a letter that has no
    /// voiced form (or at the start, or doubled) stays.
    #[test]
    fn other_text_is_left_alone() {
        assert!(matches!(compose_kana("Quarterly review"), Cow::Borrowed(_)));
        assert!(matches!(compose_kana("プレゼン"), Cow::Borrowed(_)));
        for s in ["\u{3099}", "あ\u{3099}", "っ\u{3099}", "ん\u{309A}", "か\u{309A}", "a\u{3099}", "が\u{3099}"] {
            assert_eq!(display_name(s), s, "{s:?}");
        }
        assert_eq!(display_name("か\u{3099}\u{3099}"), "が\u{3099}");
        assert_eq!(display_name(""), "");
    }

    /// The window and its tab name a presentation saved under a decomposed macOS name composed;
    /// the path it is read from and saved to stays as it is.
    #[test]
    fn titles_compose_and_paths_stay() {
        let mut s = crate::Session::with_new();
        let path = "/tmp/フ\u{309A}レセ\u{3099}ン.pptx";
        s.active_mut().unwrap().path = Some(path.into());
        let d = s.active().unwrap();
        assert_eq!(d.title(), "プレゼン");
        assert_eq!(d.path.as_deref(), Some(path));
    }
}
