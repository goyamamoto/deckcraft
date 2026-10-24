//! Change Case's two East Asian kinds (Home ▸ Font ▸ Change Case with East Asian editing):
//! Half-width and Full-width. PowerPoint offers only these two besides the Latin cases (no
//! Hiragana/Katakana kinds). The conversion follows Word for Mac's, as measured for WordCraft
//! (2026-10-18), sound marks included:
//!
//! - **Half-width**: full-width ASCII → ASCII (￥ and ＼ both → `\`), the ideographic space → a
//!   space, katakana and 。、「」・ー゛゜ → their half-width forms, a voiced kana → its base and
//!   ﾞ/ﾟ (ガ → ｶﾞ, ヴ → ｳﾞ). Kana without a half-width form (ヮヰヱヵヶヷヸヹヺヽヾ), hiragana, ￣
//!   and the full-width cent/pound/not/bar/won signs stay.
//! - **Full-width**: ASCII → full width (`\` → ￥), a space → the ideographic space, half-width
//!   kana → full width; a half-width kana followed by ﾞ/ﾟ composes when a voiced kana exists
//!   (ｶﾞ → ガ, ﾊﾟ → パ, ｳﾞ → ヴ), otherwise the mark becomes ゛/゜ (ﾜﾞ → ワ゛, ｲﾟ → イ゜). A
//!   full-width mark never composes (ｶ゛ → カ゛). `¥`, Latin-1 and half-width Hangul stay.
//!
//! Latin letters change width only; their case is left as it is.

/// Half-width or Full-width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Half,
    Full,
}

/// The full-width form of each half-width form U+FF61..=U+FF9F, in code point order.
const FULL_OF_HALF: &str =
    "。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜";

/// Katakana whose voiced form (`+ ﾞ`) exists, and ウ → ヴ.
const VOICED: &[(char, char)] = &[
    ('カ', 'ガ'),
    ('キ', 'ギ'),
    ('ク', 'グ'),
    ('ケ', 'ゲ'),
    ('コ', 'ゴ'),
    ('サ', 'ザ'),
    ('シ', 'ジ'),
    ('ス', 'ズ'),
    ('セ', 'ゼ'),
    ('ソ', 'ゾ'),
    ('タ', 'ダ'),
    ('チ', 'ヂ'),
    ('ツ', 'ヅ'),
    ('テ', 'デ'),
    ('ト', 'ド'),
    ('ハ', 'バ'),
    ('ヒ', 'ビ'),
    ('フ', 'ブ'),
    ('ヘ', 'ベ'),
    ('ホ', 'ボ'),
    ('ウ', 'ヴ'),
];

/// Katakana with a semi-voiced form (`+ ﾟ`).
const SEMI_VOICED: &[(char, char)] = &[('ハ', 'パ'), ('ヒ', 'ピ'), ('フ', 'プ'), ('ヘ', 'ペ'), ('ホ', 'ポ')];

const HALF_VOICED_MARK: char = 'ﾞ';
const HALF_SEMI_MARK: char = 'ﾟ';
const FULL_SEMI_MARK: char = '゜';

/// Offset between an ASCII character and its full-width form (U+FF01..=U+FF5E).
const FULL_ASCII_OFFSET: u32 = 0xFEE0;

fn is_halfwidth_form(c: char) -> bool {
    ('\u{FF61}'..='\u{FF9F}').contains(&c)
}

fn is_half_mark(c: char) -> bool {
    c == HALF_VOICED_MARK || c == HALF_SEMI_MARK
}

/// The full-width katakana (or punctuation) of a half-width form.
fn full_of_half(c: char) -> Option<char> {
    let i = u32::from(c).checked_sub(0xFF61)?;
    FULL_OF_HALF.chars().nth(usize::try_from(i).ok()?)
}

/// The half-width form of a full-width katakana or punctuation mark.
fn half_of_full(c: char) -> Option<char> {
    let i = FULL_OF_HALF.chars().position(|f| f == c)?;
    char::from_u32(0xFF61u32.checked_add(u32::try_from(i).ok()?)?)
}

/// The voiced (ﾞ) or semi-voiced (ﾟ) katakana of a plain full-width katakana.
fn voiced(base: char, semi: bool) -> Option<char> {
    let table = if semi { SEMI_VOICED } else { VOICED };
    table.iter().find(|(b, _)| *b == base).map(|(_, v)| *v)
}

/// A voiced katakana split into its plain katakana and whether its mark is the semi-voiced one.
fn unvoiced(c: char) -> Option<(char, bool)> {
    if let Some((b, _)) = VOICED.iter().find(|(_, v)| *v == c) {
        return Some((*b, false));
    }
    SEMI_VOICED.iter().find(|(_, v)| *v == c).map(|(b, _)| (*b, true))
}

/// Half-width of one character (never composes).
fn half(c: char) -> String {
    let cp = u32::from(c);
    match cp {
        0x3000 => return " ".into(),
        0xFFE5 => return "\\".into(),
        0xFF01..=0xFF5E => return cp.checked_sub(FULL_ASCII_OFFSET).and_then(char::from_u32).unwrap_or(c).into(),
        _ => {}
    }
    if let Some(h) = half_of_full(c) {
        return h.into();
    }
    if let Some((base, semi)) = unvoiced(c)
        && let Some(h) = half_of_full(base)
    {
        let mut s = String::from(h);
        s.push(if semi { HALF_SEMI_MARK } else { HALF_VOICED_MARK });
        return s;
    }
    c.into()
}

/// Full-width of one character (composition is handled by the caller).
fn full(c: char) -> char {
    let cp = u32::from(c);
    match cp {
        0x20 => '\u{3000}',
        0x5C => '￥',
        0x21..=0x7E => cp.checked_add(FULL_ASCII_OFFSET).and_then(char::from_u32).unwrap_or(c),
        _ => full_of_half(c).unwrap_or(c),
    }
}

/// The voiced full-width katakana of a half-width kana and a sound mark, if the kana takes it.
fn compose(base: char, mark: char) -> Option<char> {
    if !is_halfwidth_form(base) {
        return None;
    }
    voiced(full_of_half(base)?, mark == HALF_SEMI_MARK || mark == FULL_SEMI_MARK)
}

/// The converted form of each character of `src`, in order: one entry per char, so the result
/// can be split back into the runs the text came from. An empty entry is a sound mark folded
/// into the kana before it (ｶﾞ → ガ).
pub fn convert(src: &str, width: Width) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<String> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let next = chars.get(i + 1).copied();
        i += 1;
        match width {
            Width::Half => out.push(half(c)),
            Width::Full => match next.filter(|n| is_half_mark(*n)).and_then(|n| compose(c, n)) {
                Some(v) => {
                    out.push(v.into());
                    out.push(String::new());
                    i += 1;
                }
                None => out.push(full(c).into()),
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, width: Width) -> String {
        convert(src, width).concat()
    }

    #[test]
    fn one_entry_per_source_char() {
        for width in [Width::Half, Width::Full] {
            let src = "ｶﾞ ガ う゛ ﾜﾞ abc";
            assert_eq!(convert(src, width).len(), src.chars().count(), "{width:?}");
        }
    }

    #[test]
    fn half_width() {
        assert_eq!(run("Hello ＨＥＬＬＯ ｗｏｒｌｄ １２３", Width::Half), "Hello HELLO world 123");
        assert_eq!(run("アイウエオ ガギグ ヴ ッ ー ・ 。、「」！？", Width::Half), "ｱｲｳｴｵ ｶﾞｷﾞｸﾞ ｳﾞ ｯ ｰ ･ ｡､｢｣!?");
        assert_eq!(run("あいうえお ぱぴぷ 漢字 ヮ ヰヱ ヵヶ ヷヺ ヽヾ", Width::Half), "あいうえお ぱぴぷ 漢字 ヮ ヰヱ ヵヶ ヷヺ ヽヾ");
        assert_eq!(run("！？，．：；＇＂（）［］｛｝＜＞＠＃＄％＾＆＊－＿＝＋／＼｜｀～", Width::Half), "!?,.:;'\"()[]{}<>@#$%^&*-_=+/\\|`~");
        assert_eq!(run("￥ ¥ ￣ ‾ ｟｠ ￠￡￢￤￦", Width::Half), "\\ ¥ ￣ ‾ ｟｠ ￠￡￢￤￦");
        assert_eq!(run("a\u{3000}b ゛゜ か゛ ガ゛", Width::Half), "a b ﾞﾟ かﾞ ｶﾞﾞ");
        assert_eq!(run("ァィゥェォャュョ ｧ ‘’“” ① ㍻ café", Width::Half), "ｧｨｩｪｫｬｭｮ ｧ ‘’“” ① ㍻ café");
    }

    #[test]
    fn full_width() {
        assert_eq!(run("Hello World 123", Width::Full), "Ｈｅｌｌｏ\u{3000}Ｗｏｒｌｄ\u{3000}１２３");
        assert_eq!(run("!?,.:;'\"()[]{}<>@#$%^&*-_=+/\\|`~", Width::Full), "！？，．：；＇＂（）［］｛｝＜＞＠＃＄％＾＆＊－＿＝＋／￥｜｀～");
        assert_eq!(
            run("ｱｲｳｴｵ ｶﾞｷﾞｸﾞ ﾊﾟ ｳﾞ ｯ ｰ ･ ｡､｢｣", Width::Full),
            "アイウエオ\u{3000}ガギグ\u{3000}パ\u{3000}ヴ\u{3000}ッ\u{3000}ー\u{3000}・\u{3000}。、「」"
        );
        assert_eq!(
            run("ﾜﾞ ｲﾟ ｰﾞ ﾞﾟ ｶﾞﾞ ｶ゛ カﾞ ぁﾞ ｶ\u{3099}", Width::Full),
            "ワ゛\u{3000}イ゜\u{3000}ー゛\u{3000}゛゜\u{3000}ガ゛\u{3000}カ゛\u{3000}カ゛\u{3000}ぁ゛\u{3000}カ\u{3099}"
        );
        assert_eq!(run("¥ ¢£¬¦₩ ﾡﾢ 가 é ‾ あ 漢", Width::Full), "¥\u{3000}¢£¬¦₩\u{3000}ﾡﾢ\u{3000}가\u{3000}é\u{3000}‾\u{3000}あ\u{3000}漢");
    }

    #[test]
    fn half_then_full_restores_katakana_and_ascii() {
        let src = "ＡＢＣ１２３　ガパヴ";
        assert_eq!(run(&run(src, Width::Half), Width::Full), src);
    }

    #[test]
    fn hostile_input_never_panics() {
        for width in [Width::Half, Width::Full] {
            let src: String =
                ['\u{0}', '\u{FF60}', '\u{FF61}', '\u{FF9F}', '\u{FFA0}', '\u{FFFF}', '\u{10FFFF}', 'ﾞ', 'ﾟ', '\u{D7FF}'].iter().collect();
            assert_eq!(convert(&src, width).len(), src.chars().count());
            assert!(convert("", width).is_empty());
        }
    }
}
