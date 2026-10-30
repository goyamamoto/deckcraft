//! Home › Editing: Find and Replace across slide text, table cells and speaker notes.

use std::sync::Arc;

use deckcraft_model::text::RunKind;
use deckcraft_model::{Paragraph, Shape, ShapeKind, Slide, TextBody};
use serde_json::{Value, json};

use super::{CommandSpec, bad, bool_or, cmd, has_doc, str_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "text.find",
            "Find",
            ["Home", "Editing"],
            None,
            "{text, matchCase?: bool, wholeWords?: bool, matchWidth?: bool (true keeps half- and full-width forms apart)} → [{slide, shape (null = notes), cell?: [row, col], para, start, end}] (char offsets in the paragraph)",
            has_doc,
            find
        ),
        cmd!(
            "text.replace",
            "Replace",
            ["Home", "Editing"],
            None,
            "{text, with, matchCase?, wholeWords?, matchWidth?, at?: a text.find match (replace just that one; default: replace all)} → {replaced}",
            has_doc,
            replace
        ),
    ]
}

/// Where a text body lives on a slide.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Loc {
    shape: Option<u64>,
    cell: Option<(usize, usize)>,
}

/// Visit every text body on a slide: shapes depth-first (groups, table cells), then the notes.
fn bodies(slide: &mut Slide, f: &mut dyn FnMut(Loc, &mut TextBody)) {
    fn rec(shapes: &mut [Shape], depth: usize, f: &mut dyn FnMut(Loc, &mut TextBody)) {
        if depth > 64 {
            return;
        }
        for sh in shapes {
            let id = Some(u64::from(sh.id.0));
            if let Some(t) = &mut sh.text {
                f(Loc { shape: id, cell: None }, t);
            }
            match &mut sh.kind {
                ShapeKind::Table(t) => {
                    for (r, row) in t.rows.iter_mut().enumerate() {
                        for (c, cell) in row.cells.iter_mut().enumerate() {
                            f(Loc { shape: id, cell: Some((r, c)) }, &mut cell.text);
                        }
                    }
                }
                ShapeKind::Group { children, .. } => rec(children, depth + 1, f),
                _ => {}
            }
        }
    }
    rec(&mut slide.shapes, 0, f);
    f(Loc { shape: None, cell: None }, &mut slide.notes);
}

struct Opts {
    pat: Vec<char>,
    case: bool,
    whole: bool,
    /// PowerPoint's Match half/full width forms (shown with East Asian editing): the half- and
    /// full-width forms of a character stay apart. Off (the default), they match each other (see
    /// [`width_key`]).
    width: bool,
}

impl Opts {
    fn from(p: &Value, cmd: &str) -> Result<Self> {
        let text = str_param(p, "text").unwrap_or("");
        if text.is_empty() {
            return Err(bad(cmd, "missing `text`"));
        }
        Ok(Opts::new(text, bool_or(p, "matchCase", false), bool_or(p, "wholeWords", false), bool_or(p, "matchWidth", false)))
    }
    fn new(text: &str, case: bool, whole: bool, width: bool) -> Self {
        // Folding widths, a half-width kana with its sound mark (ｶﾞ) stands for one character.
        let pat = if width { text.chars().collect() } else { compose_half_width(text) };
        Opts { pat, case, whole, width }
    }
    fn eq(&self, a: char, b: char) -> bool {
        let (a, b) = if self.width { (a, b) } else { (width_key(a), width_key(b)) };
        a == b || (!self.case && a.to_lowercase().eq(b.to_lowercase()))
    }
    /// Where a match of the pattern that starts at `i` ends, if one does. Folding widths, a
    /// half-width kana and the sound mark after it match the voiced kana (ｶﾞ = ガ), so a match
    /// can be longer than the pattern.
    fn match_at(&self, text: &[char], i: usize) -> Option<usize> {
        let mut k = i;
        for &p in &self.pat {
            let c = *text.get(k)?;
            let voiced = if self.width { None } else { text.get(k + 1).and_then(|m| voiced_half_width(c, *m)) };
            k += match voiced {
                Some(v) if self.eq(v, p) => 2,
                _ if self.eq(c, p) => 1,
                _ => return None,
            };
        }
        Some(k)
    }
    /// Non-overlapping matches as char ranges.
    fn matches(&self, text: &[char]) -> Vec<(usize, usize)> {
        let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
        let mut out = vec![];
        let mut i = 0;
        while i < text.len() {
            let whole = |e: usize| !self.whole || (!word(i.checked_sub(1).and_then(|j| text.get(j))) && !word(text.get(e)));
            match self.match_at(text, i).filter(|&e| e > i && whole(e)) {
                Some(e) => {
                    out.push((i, e));
                    i = e;
                }
                None => i += 1,
            }
        }
        out
    }
}

/// Half-width katakana and punctuation (U+FF61…U+FF9F) as their full-width forms, in code
/// point order (Unicode's `<narrow>` decompositions).
const FULL_OF_HALF: &str =
    "。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜";

/// Katakana and their voiced forms (written with ﾞ in half-width text).
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
    ('ワ', 'ヷ'),
    ('ヲ', 'ヺ'),
];
/// Katakana and their semi-voiced forms (written with ﾟ in half-width text).
const SEMI_VOICED: &[(char, char)] = &[('ハ', 'パ'), ('ヒ', 'ピ'), ('フ', 'プ'), ('ヘ', 'ペ'), ('ホ', 'ポ')];

/// The full-width form of a half-width katakana or punctuation mark.
fn full_of_half(c: char) -> Option<char> {
    let i = (c as u32).checked_sub(0xFF61)?;
    FULL_OF_HALF.chars().nth(usize::try_from(i).ok()?)
}

/// The character the half- and full-width forms of `c` share, which Find compares when half-
/// and full-width forms match each other (Match half/full width forms off, the default):
/// full-width ASCII is ASCII (ＡＢＣ = ABC, １ = 1, （ = `(`), half-width katakana is
/// katakana (ｶﾀｶﾅ = カタカナ, ｰ = ー, ･ = ・), ￥ is `\` and the ideographic space is a space.
fn width_key(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{FFE5}' => '\\',
        '\u{3000}' => ' ',
        '\u{FF61}'..='\u{FF9F}' => full_of_half(c).unwrap_or(c),
        _ => c,
    }
}

/// The voiced kana a half-width katakana `c` and the half-width sound mark `mark` (ﾞ or ﾟ)
/// after it stand for: ｶﾞ is ガ, ﾊﾟ is パ.
fn voiced_half_width(c: char, mark: char) -> Option<char> {
    let semi = match mark {
        '\u{FF9E}' => false,
        '\u{FF9F}' => true,
        _ => return None,
    };
    let base = full_of_half(c).filter(|_| ('\u{FF66}'..='\u{FF9D}').contains(&c))?;
    let table = if semi { SEMI_VOICED } else { VOICED };
    table.iter().find(|(b, _)| *b == base).map(|(_, v)| *v)
}

/// `text` with each half-width kana and the sound mark after it that stand for one voiced kana
/// composed (ｶﾞｲﾄﾞ → ガｲド), so every pattern character is one character of the text, or a
/// half-width pair.
fn compose_half_width(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match chars.get(i + 1).and_then(|m| voiced_half_width(c, *m)) {
            Some(v) => {
                out.push(v);
                i += 2;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn para_chars(p: &Paragraph) -> Vec<char> {
    p.runs.iter().flat_map(|r| r.content().chars()).collect()
}

/// Matches that lie wholly in plain text runs (never inside a field, equation or break).
fn para_matches(p: &Paragraph, o: &Opts) -> Vec<(usize, usize)> {
    let mut spans = vec![];
    let mut off = 0;
    for r in &p.runs {
        let n = r.char_len();
        spans.push((off, off + n, matches!(r.kind, RunKind::Text)));
        off += n;
    }
    o.matches(&para_chars(p)).into_iter().filter(|&(a, b)| spans.iter().all(|&(s, e, text)| text || e <= a || s >= b || s == e)).collect()
}

/// Replace chars `a..b` of the paragraph; the replacement takes the first run's formatting.
fn splice(p: &mut Paragraph, a: usize, b: usize, with: &str) {
    let mut off = 0;
    let mut placed = false;
    for r in &mut p.runs {
        let chars: Vec<char> = r.content().chars().collect();
        let (s, e) = (off, off + chars.len());
        off = e;
        if e <= a || s >= b {
            continue;
        }
        let keep_head: String = chars[..a.saturating_sub(s)].iter().collect();
        let keep_tail: String = chars[b.saturating_sub(s).min(chars.len())..].iter().collect();
        r.text = if placed { keep_tail } else { format!("{keep_head}{with}{keep_tail}") };
        placed = true;
    }
    if p.runs.len() > 1 {
        p.runs.retain(|r| !(r.text.is_empty() && matches!(r.kind, RunKind::Text)));
    }
}

fn find(s: &mut Session, p: &Value) -> Result<Value> {
    let o = Opts::from(p, "text.find")?;
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        let mut sl = (**sl).clone();
        bodies(&mut sl, &mut |loc, t| {
            for (pi, para) in t.paragraphs.iter().enumerate() {
                for (a, b) in para_matches(para, &o) {
                    out.push(json!({"slide": i, "shape": loc.shape, "cell": loc.cell.map(|(r, c)| [r, c]), "para": pi, "start": a, "end": b}));
                }
            }
        });
    }
    Ok(Value::Array(out))
}

fn replace(s: &mut Session, p: &Value) -> Result<Value> {
    let o = Opts::from(p, "text.replace")?;
    let with = str_param(p, "with").unwrap_or("").to_string();
    let at = p.get("at").filter(|v| v.is_object()).map(|v| -> Result<(usize, Loc, usize, usize)> {
        let n = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as usize).ok_or_else(|| bad("text.replace", format!("`at` needs `{k}`")));
        let cell = v.get("cell").and_then(Value::as_array).and_then(|c| Some((c.first()?.as_u64()? as usize, c.get(1)?.as_u64()? as usize)));
        Ok((n("slide")?, Loc { shape: v.get("shape").and_then(Value::as_u64), cell }, n("para")?, n("start")?))
    });
    let at = at.transpose()?;
    let count = s.edit(|doc, _| {
        let mut count = 0;
        for (i, slp) in doc.slides.iter_mut().enumerate() {
            if at.is_some_and(|a| a.0 != i) {
                continue;
            }
            let mut sl = (**slp).clone();
            let mut changed = 0;
            bodies(&mut sl, &mut |loc, t| {
                if at.is_some_and(|a| a.1 != loc) {
                    return;
                }
                for (pi, para) in t.paragraphs.iter_mut().enumerate() {
                    if at.is_some_and(|a| a.2 != pi) {
                        continue;
                    }
                    for (a, b) in para_matches(para, &o).into_iter().rev() {
                        if at.is_some_and(|x| x.3 != a) {
                            continue;
                        }
                        splice(para, a, b, &with);
                        changed += 1;
                    }
                }
            });
            if changed > 0 {
                *slp = Arc::new(sl);
                count += changed;
            }
        }
        if count == 0 {
            return Err(bad("text.replace", "no matches"));
        }
        Ok(count)
    })?;
    Ok(json!({"replaced": count}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckcraft_model::{Run, RunProps};

    fn opts(t: &str, case: bool, whole: bool) -> Opts {
        Opts::new(t, case, whole, false)
    }

    fn found(pat: &str, text: &str, case: bool, width: bool) -> Vec<(usize, usize)> {
        Opts::new(pat, case, false, width).matches(&text.chars().collect::<Vec<char>>())
    }

    /// Half- and full-width forms match each other unless Match half/full width forms is on.
    #[test]
    fn half_and_full_width_forms_match_unless_kept_apart() {
        assert_eq!(found("abc", "ＡＢＣ abc", false, false), vec![(0, 3), (4, 7)]);
        assert_eq!(found("ABC", "ＡＢＣ abc", true, false), vec![(0, 3)], "match case still holds");
        assert_eq!(found("ＡＢＣ", "ABC", true, false), vec![(0, 3)]);
        assert_eq!(found("abc", "ＡＢＣ abc", false, true), vec![(4, 7)]);
        assert_eq!(found("カタカナ", "ｶﾀｶﾅとカタカナ", false, false), vec![(0, 4), (5, 9)]);
        assert_eq!(found("ｶﾀｶﾅ", "カタカナ", false, false), vec![(0, 4)]);
        assert!(found("カタカナ", "ｶﾀｶﾅ", false, true).is_empty());
        // A half-width kana and its sound mark are one voiced kana: the match covers both.
        assert_eq!(found("ガイド", "ｶﾞｲﾄﾞ", false, false), vec![(0, 5)]);
        assert_eq!(found("ｶﾞｲﾄﾞ", "ガイドとｶﾞｲﾄﾞ", false, false), vec![(0, 3), (4, 9)]);
        assert_eq!(found("パ", "ﾊﾟﾊﾞﾊ", false, false), vec![(0, 2)]);
        assert_eq!(found("ハ", "ﾊﾟﾊﾞﾊ", false, false), vec![(0, 1), (2, 3), (4, 5)], "ﾊ alone is ハ");
        assert!(found("ガ", "カ", false, false).is_empty());
        assert_eq!(found("\\", "￥100", false, false), vec![(0, 1)]);
        assert!(found("\\", "¥100", false, false).is_empty(), "¥ is not a width form of \\");
        assert_eq!(found("a b", "a\u{3000}b", false, false), vec![(0, 3)]);
        assert_eq!(found("ー・", "ｰ･", false, false), vec![(0, 2)]);
        assert!(found("。", ".", false, false).is_empty(), "。 is not a full stop");
        assert_eq!((full_of_half('\u{FF61}'), full_of_half('\u{FF9D}'), full_of_half('\u{FF9F}')), (Some('。'), Some('ン'), Some('゜')));
        assert_eq!((full_of_half('\u{FFA0}'), full_of_half('a')), (None, None));
        // Whole words count the text the match covers.
        assert_eq!(Opts::new("ｶﾞ", false, true, false).matches(&"ガ ｶﾞx ｶﾞ".chars().collect::<Vec<_>>()), vec![(0, 1), (6, 8)]);
    }

    #[test]
    fn matching_honours_case_and_whole_words() {
        let t: Vec<char> = "Cat cat category".chars().collect();
        assert_eq!(opts("cat", false, false).matches(&t).len(), 3);
        assert_eq!(opts("cat", true, false).matches(&t).len(), 2);
        assert_eq!(opts("cat", false, true).matches(&t), vec![(0, 3), (4, 7)]);
    }

    fn text_run(t: &str) -> Run {
        Run { text: t.into(), props: RunProps::default(), kind: RunKind::Text }
    }

    fn br() -> Run {
        Run { text: String::new(), props: RunProps::default(), kind: RunKind::Break }
    }

    #[test]
    fn a_match_never_spans_a_line_break() {
        let p = Paragraph { runs: vec![text_run("a"), br(), text_run("b")], ..Default::default() };
        assert!(para_matches(&p, &opts("ab", true, false)).is_empty());
    }

    #[test]
    fn offsets_count_a_line_break_as_one_char() {
        let p = Paragraph { runs: vec![text_run("ab"), br(), text_run("cd")], ..Default::default() };
        assert_eq!(para_matches(&p, &opts("cd", true, false)), vec![(3, 5)]);
    }

    #[test]
    fn splice_after_a_line_break_keeps_the_break() {
        let mut p = Paragraph { runs: vec![text_run("ab"), br(), text_run("cd")], ..Default::default() };
        splice(&mut p, 3, 5, "X");
        assert_eq!(p.text(), "ab\u{b}X");
        assert_eq!(p.runs[1].kind, RunKind::Break);
    }

    #[test]
    fn splice_across_runs_keeps_the_first_runs_format() {
        let bold = RunProps { bold: Some(true), ..Default::default() };
        let mut p = Paragraph {
            runs: vec![
                Run { text: "Hel".into(), props: bold.clone(), kind: RunKind::Text },
                Run { text: "lo world".into(), props: RunProps::default(), kind: RunKind::Text },
            ],
            ..Default::default()
        };
        splice(&mut p, 1, 5, "ola");
        assert_eq!(para_chars(&p).iter().collect::<String>(), "Hola world");
        assert_eq!(p.runs[0].text, "Hola");
        assert_eq!(p.runs[0].props, bold);
    }
}
