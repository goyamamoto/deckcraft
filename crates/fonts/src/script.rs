//! Script and font-slot classification.
//!
//! DrawingML gives every run three typefaces: `a:latin`, `a:ea` (East Asian) and `a:cs` (complex
//! script: Arabic, Hebrew, Thai, Indic…). Which one a character uses follows its Unicode script
//! (UAX #24). Characters with no script of their own (Common and Inherited: spaces, digits,
//! punctuation, combining marks) take the script of the text around them, so a space or a
//! parenthesis between two Arabic words is shaped with them. Their slot follows what PowerPoint
//! does with the characters Latin and East Asian fonts share ([`slot_class`]): ASCII and most
//! shared punctuation take the Latin font next to Japanese, full-width forms and some symbols the
//! East Asian one, and a few depend on the run's language or East Asian font.

use crate::cjk::CjkLocale;

/// An ISO 15924 script tag, e.g. `*b"Arab"`.
pub type ScriptTag = [u8; 4];

pub const ARABIC: ScriptTag = *b"Arab";
pub const HEBREW: ScriptTag = *b"Hebr";
pub const LATIN: ScriptTag = *b"Latn";

/// The DrawingML typeface slot a character is drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontSlot {
    /// `a:latin`.
    #[default]
    Latin,
    /// `a:ea`.
    EastAsian,
    /// `a:cs`.
    ComplexScript,
}

/// Inclusive code point ranges and their script (a subset of `Scripts.txt` covering the scripts
/// the slot decision and shaping need; everything else is Latin-slot or Common).
const RANGES: &[(u32, u32, ScriptTag)] = &[
    (0x0041, 0x005A, *b"Latn"),
    (0x0061, 0x007A, *b"Latn"),
    (0x00AA, 0x00AA, *b"Latn"),
    (0x00BA, 0x00BA, *b"Latn"),
    (0x00C0, 0x00D6, *b"Latn"),
    (0x00D8, 0x00F6, *b"Latn"),
    (0x00F8, 0x024F, *b"Latn"),
    (0x0370, 0x03FF, *b"Grek"),
    (0x0400, 0x052F, *b"Cyrl"),
    (0x0530, 0x058F, *b"Armn"),
    (0x0591, 0x05F4, *b"Hebr"),
    // Arabic block, minus the Common/Inherited code points handled in `script_of`.
    (0x0600, 0x06FF, *b"Arab"),
    (0x0700, 0x074F, *b"Syrc"),
    (0x0750, 0x077F, *b"Arab"),
    (0x0780, 0x07BF, *b"Thaa"),
    (0x07C0, 0x07FF, *b"Nkoo"),
    (0x0870, 0x08FF, *b"Arab"),
    (0x0900, 0x097F, *b"Deva"),
    (0x0980, 0x09FF, *b"Beng"),
    (0x0A00, 0x0A7F, *b"Guru"),
    (0x0A80, 0x0AFF, *b"Gujr"),
    (0x0B00, 0x0B7F, *b"Orya"),
    (0x0B80, 0x0BFF, *b"Taml"),
    (0x0C00, 0x0C7F, *b"Telu"),
    (0x0C80, 0x0CFF, *b"Knda"),
    (0x0D00, 0x0D7F, *b"Mlym"),
    (0x0D80, 0x0DFF, *b"Sinh"),
    (0x0E00, 0x0E7F, *b"Thai"),
    (0x0E80, 0x0EFF, *b"Laoo"),
    (0x0F00, 0x0FFF, *b"Tibt"),
    (0x1000, 0x109F, *b"Mymr"),
    (0x10A0, 0x10FF, *b"Geor"),
    (0x1100, 0x11FF, *b"Hang"),
    (0x1200, 0x139F, *b"Ethi"),
    (0x1780, 0x17FF, *b"Khmr"),
    (0x1E00, 0x1EFF, *b"Latn"),
    (0x1F00, 0x1FFF, *b"Grek"),
    (0x2C60, 0x2C7F, *b"Latn"),
    (0x2E80, 0x2FDF, *b"Hani"),
    (0x3040, 0x309F, *b"Hira"),
    (0x30A0, 0x30FF, *b"Kana"),
    (0x3100, 0x312F, *b"Bopo"),
    (0x3130, 0x318F, *b"Hang"),
    (0x31F0, 0x31FF, *b"Kana"),
    (0x3400, 0x4DBF, *b"Hani"),
    (0x4E00, 0x9FFF, *b"Hani"),
    (0xA720, 0xA7FF, *b"Latn"),
    (0xAC00, 0xD7AF, *b"Hang"),
    (0xF900, 0xFAFF, *b"Hani"),
    (0xFB00, 0xFB06, *b"Latn"),
    (0xFB1D, 0xFB4F, *b"Hebr"),
    (0xFB50, 0xFDFF, *b"Arab"),
    (0xFE70, 0xFEFF, *b"Arab"),
    (0xFF21, 0xFF3A, *b"Latn"),
    (0xFF41, 0xFF5A, *b"Latn"),
    (0xFF66, 0xFF9F, *b"Kana"),
    (0x10E60, 0x10E7F, *b"Arab"),
    (0x1EE00, 0x1EEFF, *b"Arab"),
    (0x20000, 0x323AF, *b"Hani"),
];

/// Code points inside the Arabic blocks whose script is Common or Inherited: the Arabic comma,
/// semicolon and question mark, tatweel, the harakat (combining vowel marks) and the BOM-like
/// U+FEFF. They take the script of the text around them.
fn arabic_common(cp: u32) -> bool {
    matches!(cp, 0x060C | 0x061B | 0x061F | 0x0640 | 0x064B..=0x0655 | 0x0670 | 0xFEFF | 0xFD3E | 0xFD3F)
}

/// The script of `c`, or `None` for Common and Inherited characters (spaces, digits,
/// punctuation, combining marks), which take the script of their neighbours.
pub fn script_of(c: char) -> Option<ScriptTag> {
    let cp = c as u32;
    if arabic_common(cp) {
        return None;
    }
    // Ranges are sorted and disjoint: binary search on the start.
    let i = RANGES.partition_point(|(lo, _, _)| *lo <= cp);
    let (lo, hi, tag) = RANGES.get(i.checked_sub(1)?)?;
    (cp >= *lo && cp <= *hi).then_some(*tag)
}

/// The slot a script is drawn with.
pub fn slot_of_script(s: ScriptTag) -> FontSlot {
    match &s {
        b"Hani" | b"Hira" | b"Kana" | b"Hang" | b"Bopo" => FontSlot::EastAsian,
        b"Arab" | b"Hebr" | b"Syrc" | b"Thaa" | b"Nkoo" | b"Deva" | b"Beng" | b"Guru" | b"Gujr" | b"Orya" | b"Taml" | b"Telu" | b"Knda" | b"Mlym"
        | b"Sinh" | b"Thai" | b"Laoo" | b"Tibt" | b"Mymr" | b"Khmr" => FontSlot::ComplexScript,
        _ => FontSlot::Latin,
    }
}

/// How a character's slot is decided ([`slot_class`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotClass {
    /// This slot, whatever the text around it.
    Own(FontSlot),
    /// The Latin slot, unless the text before it (after it, at the start) is complex script, whose
    /// font and shaping it then shares: digits and punctuation between Arabic words stay Arabic.
    Latin,
    /// The slot of the text before it (after it, at the start): combining marks, joiners,
    /// controls and the symbols not classified.
    Inherit,
}

/// How the slot of `c` is decided in a run whose `lang` is Chinese, Japanese or Korean
/// (`lang_is_cjk`) and which has an East Asian font (`has_ea_font`: an `a:ea` typeface, or the
/// theme's font for the run's East Asian language), as PowerPoint for Mac draws the characters
/// Latin and East Asian fonts share:
/// - full-width and half-width forms (Ａ ａ １ （ ｶ), CJK punctuation (、 。 「 and the ideographic
///   space), enclosed and squared CJK, circled numbers (① ②), geometric shapes and miscellaneous
///   symbols (■ □ ▲ ○ ● ◎ ◇ ★ ☆ ♪ ♀) and ™ take the East Asian font;
/// - the single and double curly quotes do so only in a run whose language is East Asian (its
///   `lang`; `altLang` doesn't count), and × ÷ ° ± § ¨ ´ only in a run that has an East Asian font;
/// - ASCII digits, punctuation and spaces, the rest of Latin-1 (© ® ¥ £ ¢ µ ¶ ·), dashes, the
///   ellipses, bullets, daggers, primes, ※ ‰, currency signs, letterlike symbols (℃ № ℡), Roman
///   numerals, arrows, mathematical operators and box drawing take the Latin font, even between
///   kanji: in 第1章 the 1 is drawn with the Latin font, and in 日本(Japan) both parentheses;
/// - letters take their script's slot (Greek and Cyrillic are Latin).
pub fn slot_class(c: char, lang_is_cjk: bool, has_ea_font: bool) -> SlotClass {
    let cp = c as u32;
    let ea_if = |on: bool| if on { SlotClass::Own(FontSlot::EastAsian) } else { SlotClass::Latin };
    // Full-width Latin letters are Latin script but East Asian text.
    if (0xFF01..=0xFFEF).contains(&cp) {
        return SlotClass::Own(FontSlot::EastAsian);
    }
    if let Some(s) = script_of(c) {
        return SlotClass::Own(slot_of_script(s));
    }
    match cp {
        // CJK symbols and punctuation, enclosed CJK letters, CJK compatibility (㎡ ㈱), vertical
        // and compatibility forms; circled numbers, geometric shapes, miscellaneous symbols, ™.
        0x3000..=0x303F | 0x3200..=0x33FF | 0xFE10..=0xFE1F | 0xFE30..=0xFE4F | 0x2460..=0x24FF | 0x25A0..=0x26FF | 0x2122 => {
            SlotClass::Own(FontSlot::EastAsian)
        }
        0x2018 | 0x2019 | 0x201C | 0x201D => ea_if(lang_is_cjk),
        0x00A7 | 0x00A8 | 0x00B0 | 0x00B1 | 0x00B4 | 0x00D7 | 0x00F7 => ea_if(has_ea_font),
        // ASCII and Latin-1 punctuation, symbols and spaces; the spaces, dashes and punctuation of
        // General Punctuation; super- and subscripts; currency; letterlike symbols; number forms;
        // arrows; mathematical operators; box drawing.
        0x0020..=0x007E
        | 0x00A0..=0x00BF
        | 0x2000..=0x200A
        | 0x2010..=0x2027
        | 0x202F..=0x205F
        | 0x2070..=0x20CF
        | 0x2100..=0x22FF
        | 0x2500..=0x257F => SlotClass::Latin,
        _ => SlotClass::Inherit,
    }
}

/// The slot `c` takes next to Latin or East Asian text in such a run ([`slot_class`]); `None`
/// for characters that take the slot of the text before them. Complex-script text around a
/// Latin-class character still draws it in the complex-script font ([`resolve_in`]).
pub fn slot_of_in(c: char, lang_is_cjk: bool, has_ea_font: bool) -> Option<FontSlot> {
    match slot_class(c, lang_is_cjk, has_ea_font) {
        SlotClass::Own(s) => Some(s),
        SlotClass::Latin => Some(FontSlot::Latin),
        SlotClass::Inherit => None,
    }
}

/// The slot of `c` whatever the text around it: `None` for characters whose slot depends on
/// their neighbours. CJK punctuation and full-width forms are East Asian although their script is
/// Common (or, for the full-width letters, Latin).
pub fn slot_of(c: char) -> Option<FontSlot> {
    match slot_class(c, false, false) {
        SlotClass::Own(s) => Some(s),
        SlotClass::Latin | SlotClass::Inherit => None,
    }
}

/// The East Asian tradition the characters of a text tell by themselves: Japanese for kana,
/// Korean for Hangul, whichever comes first; `None` when there is neither (Han characters are
/// shared by all four traditions).
pub fn tradition_of_text(chars: impl IntoIterator<Item = char>) -> Option<CjkLocale> {
    chars.into_iter().find_map(|c| match script_of(c).as_ref() {
        Some(b"Hira" | b"Kana") => Some(CjkLocale::Japanese),
        Some(b"Hang") => Some(CjkLocale::Korean),
        _ => None,
    })
}

/// A default BCP 47 language for a script (shaping picks `locl` forms by language).
pub fn default_language(s: ScriptTag) -> Option<&'static str> {
    Some(match &s {
        b"Arab" => "ar",
        b"Hebr" => "he",
        b"Syrc" => "syr",
        b"Thaa" => "dv",
        b"Thai" => "th",
        b"Deva" => "hi",
        _ => return None,
    })
}

/// Resolve every character's script and slot in text whose runs have no East Asian language or
/// font ([`resolve_in`]).
pub fn resolve(chars: &[char]) -> Vec<(Option<ScriptTag>, FontSlot)> {
    resolve_in(chars, |_| (false, false))
}

/// Resolve every character's script and slot. `run_of(i)` tells, for character `i`, whether its
/// run's language is Chinese, Japanese or Korean and whether the run has an East Asian font
/// ([`slot_class`]). A character with a slot of its own keeps it; the others take the script and
/// slot of the preceding character with one (at the start, of the following one; Latin when there
/// is none), except that Latin-class characters next to East Asian text are Latin text. Symbols
/// with no script that take the East Asian slot (★ ① ™, CJK punctuation) don't tell which text
/// the characters after them follow: the digits of "مرحبا ★ 2024" stay Arabic text.
pub fn resolve_in(chars: &[char], run_of: impl Fn(usize) -> (bool, bool)) -> Vec<(Option<ScriptTag>, FontSlot)> {
    let own: Vec<(Option<ScriptTag>, SlotClass)> = chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (cjk, ea) = run_of(i);
            (script_of(*c), slot_class(*c, cjk, ea))
        })
        .collect();
    let tells = |s: Option<ScriptTag>, sl: FontSlot| s.is_some() || sl != FontSlot::EastAsian;
    let first = own
        .iter()
        .find_map(|(s, cl)| match cl {
            SlotClass::Own(sl) if tells(*s, *sl) => Some((*s, *sl)),
            _ => None,
        })
        .unwrap_or((None, FontSlot::Latin));
    // The last character whose slot tells the text's, and the last one resolved.
    let (mut strong, mut prev) = (first, first);
    own.into_iter()
        .map(|(s, cl)| {
            match cl {
                SlotClass::Own(sl) => {
                    prev = (s, sl);
                    if tells(s, sl) {
                        strong = prev;
                    }
                }
                SlotClass::Latin if strong.1 == FontSlot::EastAsian => prev = (Some(LATIN), FontSlot::Latin),
                SlotClass::Latin => prev = strong,
                SlotClass::Inherit => {}
            }
            prev
        })
        .collect()
}

/// Arabic letters that join to the following letter (joining type D, dual-joining): a kashida
/// (tatweel) may be inserted after them. Right-joining (alef, dal, reh, waw…) and non-joining
/// (hamza) letters are excluded.
pub fn joins_following(c: char) -> bool {
    let cp = c as u32;
    let letter = matches!(cp, 0x0620..=0x064A | 0x066E..=0x06D3 | 0x06FA..=0x06FC | 0x06FF | 0x0750..=0x077F | 0x08A0..=0x08C9);
    letter
        && !matches!(
            cp,
            0x0621..=0x0625 | 0x0627 | 0x0629 | 0x062F..=0x0632 | 0x0648 | 0x0671..=0x0673 | 0x0675..=0x0677 | 0x0688..=0x0699 | 0x06C0
                | 0x06C3..=0x06CB | 0x06CD | 0x06CF | 0x06D2 | 0x06D3 | 0x0759..=0x075B | 0x076B | 0x076C | 0x0771 | 0x0773 | 0x0774
                | 0x0778 | 0x0779 | 0x08AA..=0x08AC | 0x08AE | 0x08B1 | 0x08B2 | 0x08B9
        )
}

/// An Arabic letter that joins to the preceding letter (joining type D or R).
pub fn joins_preceding(c: char) -> bool {
    let cp = c as u32;
    matches!(cp, 0x0622..=0x064A | 0x066E..=0x06D3 | 0x06FA..=0x06FC | 0x06FF | 0x0750..=0x077F | 0x08A0..=0x08C9) && cp != 0x0621
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_slots() {
        assert_eq!(script_of('س'), Some(ARABIC));
        assert_eq!(script_of('ש'), Some(HEBREW));
        assert_eq!(script_of('R'), Some(LATIN));
        assert_eq!(script_of('2'), None);
        assert_eq!(script_of('('), None);
        // Harakat and tatweel are Inherited/Common.
        assert_eq!(script_of('\u{064E}'), None);
        assert_eq!(script_of('\u{0640}'), None);
        assert_eq!(slot_of('س'), Some(FontSlot::ComplexScript));
        assert_eq!(slot_of('日'), Some(FontSlot::EastAsian));
        assert_eq!(slot_of('、'), Some(FontSlot::EastAsian));
        assert_eq!(slot_of('a'), Some(FontSlot::Latin));
        assert_eq!(slot_of(' '), None);
        assert_eq!(script_of('\u{10FFFF}'), None);
        assert_eq!(script_of('\0'), None);
    }

    #[test]
    fn neutrals_take_the_surrounding_slot() {
        let chars: Vec<char> = "سنة 2024 (Retrait) فقط".chars().collect();
        let r = resolve(&chars);
        // "سنة 2024 (" follows Arabic: complex script.
        for (i, c) in chars.iter().enumerate().take(10) {
            assert_eq!(r[i].1, FontSlot::ComplexScript, "{c:?}");
        }
        // "Retrait)" is Latin; the space after ")" stays Latin; "فقط" is complex script.
        assert_eq!(r[10].1, FontSlot::Latin);
        assert_eq!(r[17].1, FontSlot::Latin);
        assert_eq!(r[19], (Some(ARABIC), FontSlot::ComplexScript));
        // Leading neutrals take the first strong character's slot.
        let r = resolve(&['(', 'س']);
        assert_eq!(r[0].1, FontSlot::ComplexScript);
        assert!(resolve(&[]).is_empty());
        assert_eq!(resolve(&['1'])[0], (None, FontSlot::Latin));
    }

    /// The slots of `text` in runs with an East Asian `lang` (`cjk`) and font (`ea`).
    fn slots(text: &str, cjk: bool, ea: bool) -> Vec<FontSlot> {
        let chars: Vec<char> = text.chars().collect();
        resolve_in(&chars, |_| (cjk, ea)).into_iter().map(|r| r.1).collect()
    }

    fn slots_str(text: &str, cjk: bool, ea: bool) -> String {
        slots(text, cjk, ea)
            .into_iter()
            .map(|s| match s {
                FontSlot::Latin => 'L',
                FontSlot::EastAsian => 'E',
                FontSlot::ComplexScript => 'C',
            })
            .collect()
    }

    #[test]
    fn ascii_next_to_japanese_takes_the_latin_slot() {
        // PowerPoint draws the 1 of 第1章 and both parentheses of 日本(Japan) in the Latin font,
        // and the digits of a date as well, whatever the run's language.
        for cjk in [false, true] {
            assert_eq!(slots_str("第1章", cjk, true), "ELE");
            assert_eq!(slots_str("日本(Japan)", cjk, true), "EELLLLLLL");
            assert_eq!(slots_str("2024年10月", cjk, true), "LLLLELLE");
            assert_eq!(slots_str("日本 語", cjk, true), "EELE");
            // CJK punctuation and the ideographic space stay East Asian.
            assert_eq!(slots_str("日本、語。\u{3000}「a」", cjk, true), "EEEEEEELE");
        }
        // Such digits are shaped as Latin text, not with the kanji's script.
        let chars: Vec<char> = "第1章".chars().collect();
        assert_eq!(resolve_in(&chars, |_| (true, true))[1], (Some(LATIN), FontSlot::Latin));
        // A combining mark follows the character it sits on.
        assert_eq!(slots_str("漢1\u{20E3}", true, true), "ELL");
        assert_eq!(slots_str("漢\u{3099}", true, true), "EE");
    }

    #[test]
    fn full_width_forms_take_the_east_asian_slot() {
        for c in "ＰｏｗｅｒＰｏｉｎｔ１２３（）！～￥ｶ".chars() {
            assert_eq!(slot_of(c), Some(FontSlot::EastAsian), "{c:?}");
        }
        // The letters are still Latin script, for shaping.
        assert_eq!(script_of('Ａ'), Some(LATIN));
        assert_eq!(slots_str("ＰｏｗｅｒＰｏｉｎｔ", false, false), "EEEEEEEEEE");
        assert_eq!(slots_str("PowerＰｏｗｅｒ", false, false), "LLLLLEEEEE");
    }

    #[test]
    fn shared_characters_follow_powerpoints_table() {
        // Curly quotes: East Asian in an East Asian language only.
        for c in ['‘', '’', '“', '”'] {
            assert_eq!(slot_of_in(c, true, false), Some(FontSlot::EastAsian), "{c:?}");
            assert_eq!(slot_of_in(c, false, true), Some(FontSlot::Latin), "{c:?}");
        }
        // × ÷ ° ± § ¨ ´: East Asian whenever the run has an East Asian font.
        for c in ['×', '÷', '°', '±', '§', '¨', '´'] {
            assert_eq!(slot_of_in(c, false, true), Some(FontSlot::EastAsian), "{c:?}");
            assert_eq!(slot_of_in(c, true, false), Some(FontSlot::Latin), "{c:?}");
        }
        // Geometric shapes, miscellaneous symbols, circled numbers and ™: always East Asian.
        for c in "■□▲△○●◎◇◆★☆♪♂♀①②⑳™".chars() {
            assert_eq!(slot_of_in(c, false, false), Some(FontSlot::EastAsian), "{c:?}");
            assert_eq!(slot_of(c), Some(FontSlot::EastAsian), "{c:?}");
        }
        // The rest: Latin even in a Japanese run with an East Asian font (Greek, Cyrillic and
        // Latin letters by their script, the others unless complex script surrounds them).
        for c in "′″…‥—―–‐•·¶†‡※℃№℡ⅠⅡ©®¥€£¢‰\u{212B}µ→⇒∀≠─│!?#%&@ \u{A0}".chars() {
            assert_eq!(slot_of_in(c, true, true), Some(FontSlot::Latin), "{c:?}");
            assert_eq!(slot_of(c), None, "{c:?} depends on the text around it");
        }
        for c in ['α', 'Ж', 'Å'] {
            assert_eq!(slot_of_in(c, true, true), Some(FontSlot::Latin), "{c:?}");
            assert_eq!(slot_of(c), Some(FontSlot::Latin), "{c:?}");
        }
        // Not classified: the character before decides.
        assert_eq!(slot_of_in('\u{0301}', true, true), None);
        assert_eq!(slot_of_in('\u{200D}', true, true), None);
        // In text: a quoted English word in a Japanese run keeps both quotes in one font.
        assert_eq!(slots_str("と“Hi”", true, false), "EELLE");
        assert_eq!(slots_str("と“Hi”", false, true), "ELLLL");
        assert_eq!(slots_str("※注意…", true, true), "LEEL");
        assert_eq!(slots_str("Step ①", false, false), "LLLLLE");
        assert_eq!(slots_str("10×20", true, true), "LLELL");
        assert_eq!(slots_str("10×20", true, false), "LLLLL");
    }

    #[test]
    fn complex_script_still_takes_shared_characters() {
        // Latin-class characters between Arabic words are shaped and drawn with them, also in a
        // run that has an East Asian font.
        assert_eq!(slots_str("سنة 2024 …", false, true), "CCCCCCCCCC");
        assert_eq!(slots_str("“س”", false, false), "CCC");
        // An East Asian language makes the quotes East Asian, whatever they enclose.
        assert_eq!(slots_str("“س”", true, false), "ECE");
        // A symbol always drawn in the East Asian font (★ ①, × with an East Asian font) doesn't
        // take the Arabic or Hebrew text's spaces and digits after it out of the complex-script
        // font.
        assert_eq!(slots_str("مرحبا ★ 2024 عالم", false, false), "CCCCCCECCCCCCCCCC");
        assert_eq!(slots_str("שלום ① 2024 עולם", false, false), "CCCCCECCCCCCCCCC");
        assert_eq!(slots_str("سنة 10×20", false, true), "CCCCCCECC");
        // Next to Japanese, the digits after one are still Latin text.
        assert_eq!(slots_str("日本★ 2024", true, true), "EEELLLLL");
        assert_eq!(slots_str("★ 2024 日本", true, true), "ELLLLLLEE");
    }

    #[test]
    fn kana_and_hangul_tell_the_tradition() {
        assert_eq!(tradition_of_text("漢字とかな".chars()), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_text("カタカナ".chars()), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_text("ｶﾀｶﾅ".chars()), Some(CjkLocale::Japanese));
        assert_eq!(tradition_of_text("한국어".chars()), Some(CjkLocale::Korean));
        assert_eq!(tradition_of_text("漢字".chars()), None);
        assert_eq!(tradition_of_text("abc".chars()), None);
        assert_eq!(tradition_of_text("".chars()), None);
    }

    #[test]
    fn joining_classes() {
        assert!(joins_following('س'));
        assert!(joins_following('ن'));
        assert!(!joins_following('ا'));
        assert!(!joins_following('د'));
        assert!(!joins_following('ء'));
        assert!(joins_preceding('ا'));
        assert!(!joins_preceding('ء'));
        assert!(!joins_following('a'));
    }

    #[test]
    fn ranges_are_sorted_and_disjoint() {
        for w in RANGES.windows(2) {
            assert!(w[0].1 < w[1].0, "{:x?} {:x?}", w[0], w[1]);
        }
    }
}
