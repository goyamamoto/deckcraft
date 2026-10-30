use super::style::{Cell, ParaShaped, Style};
use super::*;

pub(crate) fn measure_str(face: &FontFace, size: f64, s: &str) -> (f64, Vec<(u32, f64)>) {
    let g = deckcraft_fonts::shape(face, s, &[], |c| c);
    let k = size / face.upem.max(1.0);
    let mut x = 0.0;
    let mut out = vec![];
    for gl in g {
        out.push((gl.gid, x + gl.x_offset as f64 * k));
        x += gl.x_advance as f64 * k;
    }
    (x, out)
}

/// Single line spacing as a multiple of the font size. PowerPoint spaces single-spaced lines 1.2
/// times the size apart in every font (observed on twenty Latin, CJK, Thai and Indic fonts at 18
/// and 40 pt), whatever the font's own ascent and descent.
///
/// Caveat: this was measured on one Windows install of desktop PowerPoint. It may be a Windows or
/// machine setting; PowerPoint on macOS or the web may space lines by the font's own metrics.
/// Keep it a single tunable value, and re-measure on the other platform before changing it.
pub(crate) const LINE_PITCH: f64 = 1.2;

/// A line's height above and below the baseline at `size`: [`LINE_PITCH`] × size, split where
/// the font's Windows ascent and descent put the baseline.
pub(crate) fn line_metrics(face: &FontFace, size: f64) -> (f64, f64) {
    let h = size * LINE_PITCH;
    (h * face.line_split, h * (1.0 - face.line_split))
}

/// Hangul syllables and jamo: PowerPoint wraps Korean at word boundaries (spaces), not between
/// syllables.
fn is_hangul(c: char) -> bool {
    matches!(c, '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{A960}'..='\u{A97F}' | '\u{AC00}'..='\u{D7FF}')
}

/// East Asian commas and full stops, the punctuation PowerPoint lets hang past the end of a line
/// (`hangingPunct`). Closing brackets don't hang: "「。」" overflowing by its "」" moves to the next
/// line whole.
pub(crate) fn is_hanging_punct(c: char) -> bool {
    matches!(c, '、' | '。' | '，' | '．' | '､' | '｡')
}

/// Small kana, the prolonged sound mark and iteration marks: PowerPoint keeps them off the start
/// of a line (strict Japanese line breaking), where UAX #14's default allows a break before them.
fn is_cj_nonstarter(c: char) -> bool {
    matches!(
        c,
        'ぁ' | 'ぃ'
            | 'ぅ'
            | 'ぇ'
            | 'ぉ'
            | 'っ'
            | 'ゃ'
            | 'ゅ'
            | 'ょ'
            | 'ゎ'
            | 'ゕ'
            | 'ゖ'
            | 'ァ'
            | 'ィ'
            | 'ゥ'
            | 'ェ'
            | 'ォ'
            | 'ッ'
            | 'ャ'
            | 'ュ'
            | 'ョ'
            | 'ヮ'
            | 'ヵ'
            | 'ヶ'
            | 'ー'
            | '々'
            | '〻'
            | 'ゝ'
            | 'ゞ'
            | 'ヽ'
            | 'ヾ'
            | '\u{31F0}'..='\u{31FF}'
            | 'ｧ'..='ｰ'
    )
}

/// East Asian text as line breaking sees it: ideographs, kana, Bopomofo, CJK symbols and
/// punctuation (not the ideographic space), enclosed and compatibility CJK, full-width and
/// half-width forms. Not Hangul, which PowerPoint wraps between words, nor the combining sound
/// marks.
fn is_east_asian(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FFF
            | 0x3001..=0x303F
            | 0x3040..=0x3098
            | 0x309B..=0x30FF
            | 0x3100..=0x312F
            | 0x3190..=0x31FF
            | 0x3200..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF01..=0xFFEF
            | 0x20000..=0x3FFFF
    )
}

/// A letter or digit of a Latin, Greek or Cyrillic word, which `latinLnBrk` lets a line break
/// inside.
fn is_latin_word_char(c: char) -> bool {
    c.is_alphanumeric() && !is_east_asian(c) && !is_hangul(c) && matches!(script::script_of(c).as_ref(), None | Some(b"Latn" | b"Grek" | b"Cyrl"))
}

/// A paragraph's line-breaking rules (`a:pPr/@eaLnBrk`, `@latinLnBrk` and the deck's
/// `p:kinsoku`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct BreakRules<'a> {
    /// `latinLnBrk="1"`: a Latin word may break between any two of its letters, to fill the line
    /// (PowerPoint for Mac breaks "a verylongword" as "a v|erylongword"); off, only where Unicode
    /// allows it and never right after `/` or `\`.
    latin: bool,
    /// The East Asian line-breaking rules (kinsoku), on unless `eaLnBrk="0"`: closing
    /// punctuation, small kana, iteration marks and the prolonged sound mark never start a line,
    /// opening brackets never end one. Off, a line may break before any East Asian character
    /// (PowerPoint then starts a line with "。").
    kinsoku: bool,
    /// The deck's own lists (`p:kinsoku`) when they are for the paragraph's East Asian language:
    /// in place of the standard ones wherever East Asian text meets.
    custom: Option<&'a KinsokuLists>,
}

/// The deck's kinsoku lists (`p:kinsoku`), sorted for lookups.
#[derive(Clone, Debug, Default)]
pub(crate) struct KinsokuLists {
    /// The East Asian language they are for: the one `lang` names, Japanese when it names none;
    /// `None` when `lang` is not Chinese, Japanese or Korean (the lists then apply nowhere).
    locale: Option<deckcraft_fonts::CjkLocale>,
    /// `invalStChars`: the characters no line starts with.
    no_start: Vec<char>,
    /// `invalEndChars`: the characters no line ends with.
    no_end: Vec<char>,
}

impl KinsokuLists {
    /// A line-breaking list holds a few dozen characters; a hostile one is cut short.
    const MAX: usize = 4096;

    pub(crate) fn of(k: &deckcraft_model::Kinsoku) -> KinsokuLists {
        let set = |s: &str| {
            let mut v: Vec<char> = s.chars().take(Self::MAX).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let locale = match k.lang.as_deref() {
            Some(l) => deckcraft_fonts::CjkLocale::from_lang(l),
            None => Some(deckcraft_fonts::CjkLocale::Japanese),
        };
        KinsokuLists { locale, no_start: set(&k.start), no_end: set(&k.end) }
    }
}

#[cfg(test)]
impl BreakRules<'_> {
    /// PowerPoint's defaults: kinsoku on with the standard lists, Latin words whole.
    pub(crate) const STANDARD: BreakRules<'static> = BreakRules { latin: false, kinsoku: true, custom: None };
}

/// The line-breaking rules of paragraph `p`: its `eaLnBrk` (absent everywhere means on, as
/// PowerPoint for Mac breaks) and `latinLnBrk` (absent means off), and the deck's `p:kinsoku`
/// lists when their language (Japanese when they name none) is the paragraph's East Asian one
/// (its runs' `lang`, else the system's East Asian language or its kana; a run in a Western
/// language is not East Asian, whatever its characters).
pub(crate) fn break_rules<'a>(p: &ParaShaped, kinsoku: Option<&'a KinsokuLists>) -> BreakRules<'a> {
    let tradition = |st: &Style| match st.lang.as_deref() {
        Some(l) => deckcraft_fonts::CjkLocale::from_lang(l),
        None => st.cjk,
    };
    let custom = kinsoku.filter(|k| k.locale.is_some() && p.styles.iter().find_map(tradition) == k.locale);
    BreakRules { latin: p.props.latin_line_break.unwrap_or(false), kinsoku: p.props.east_asian_line_break.unwrap_or(true), custom }
}

/// May a line break between `prev` and `next` under `rules`, where Unicode line breaking says
/// `uax`?
fn pair_breaks(prev: char, next: char, uax: bool, rules: &BreakRules) -> bool {
    // PowerPoint never breaks after a zero-width space, nor before an em dash (only after it), and
    // wraps Korean between words.
    if (is_hangul(prev) && is_hangul(next)) || prev == '\u{200B}' || next == '\u{2014}' {
        return false;
    }
    if rules.latin && is_latin_word_char(prev) && is_latin_word_char(next) {
        return true;
    }
    if !rules.kinsoku {
        // No kinsoku: before any East Asian character, closing punctuation included.
        if is_east_asian(next) && !prev.is_whitespace() {
            return true;
        }
    } else if let Some(k) = rules.custom {
        // The deck's lists decide wherever East Asian text meets other text; Unicode's rules
        // stay between Latin letters and around spaces.
        if (is_east_asian(prev) || is_east_asian(next)) && !prev.is_whitespace() && !next.is_whitespace() {
            return k.no_start.binary_search(&next).is_err() && k.no_end.binary_search(&prev).is_err();
        }
    } else if is_cj_nonstarter(next) {
        return false;
    }
    if !rules.latin && matches!(prev, '/' | '\\') && !next.is_whitespace() {
        return false;
    }
    uax
}

/// Break opportunities: allowed[i] = a line may start at char i. Unicode line breaking, then
/// PowerPoint's differences ([`pair_breaks`]): kinsoku (standard, the deck's own lists, or off
/// with `eaLnBrk="0"`), `latinLnBrk`, and Korean breaking only between words.
pub(crate) fn break_opportunities(text: &str, rules: &BreakRules) -> (Vec<bool>, Vec<bool>) {
    let n = text.chars().count();
    let mut allowed = vec![false; n + 1];
    let mut mandatory = vec![false; n + 1];
    let mut byte_to_char = std::collections::HashMap::new();
    for (ci, (b, _)) in text.char_indices().enumerate() {
        byte_to_char.insert(b, ci);
    }
    byte_to_char.insert(text.len(), n);
    for (b, op) in unicode_linebreak::linebreaks(text) {
        if let Some(&ci) = byte_to_char.get(&b) {
            if let Some(a) = allowed.get_mut(ci) {
                *a = true;
            }
            if op == unicode_linebreak::BreakOpportunity::Mandatory
                && let Some(m) = mandatory.get_mut(ci)
            {
                *m = true;
            }
        }
    }
    let chars: Vec<char> = text.chars().collect();
    for (i, w) in chars.windows(2).enumerate() {
        let &[prev, next] = w else { continue };
        if mandatory.get(i + 1).copied().unwrap_or(false) {
            continue;
        }
        if let Some(a) = allowed.get_mut(i + 1) {
            *a = pair_breaks(prev, next, *a, rules);
        }
    }
    (allowed, mandatory)
}

/// Spaces that end a wrapped line without counting: they don't push the line to wrap, and its
/// width (for alignment and justification) stops before them. The ideographic space (U+3000) is
/// one too, so a right-aligned or centred Japanese line wrapped after "　" isn't an em short.
fn is_trailing_space(c: char) -> bool {
    c == ' ' || c == '\u{3000}'
}

pub(crate) struct RawLine {
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// End including trailing spaces / the break char consumed.
    next: usize,
}

pub(crate) fn break_lines(
    p: &ParaShaped,
    rules: &BreakRules,
    first_w: f64,
    rest_w: f64,
    wrap: bool,
    tab_stops: &[Tab],
    default_tab: f64,
    x0_first: f64,
    x0_rest: f64,
) -> Vec<RawLine> {
    let text: String = p.cells.iter().map(|c| c.ch).collect();
    let (allowed, _) = break_opportunities(&text, rules);
    // `hangingPunct` is on unless a style turns it off: PowerPoint for Mac hangs "。" and "、"
    // past the margin when no level sets it. Hanging is part of kinsoku: without it
    // (`eaLnBrk="0"`) a "。" that doesn't fit starts the next line.
    let hang = rules.kinsoku && p.props.hanging_punct.unwrap_or(true);
    let n = p.cells.len();
    let mut lines = vec![];
    let mut start = 0;
    if n == 0 {
        return vec![RawLine { start: 0, end: 0, next: 0 }];
    }
    while start < n {
        let avail = if lines.is_empty() { first_w } else { rest_w };
        let x0 = if lines.is_empty() { x0_first } else { x0_rest };
        let mut x = 0.0;
        let mut last_break: Option<usize> = None;
        let mut i = start;
        let mut end = n;
        let mut next = n;
        while i < n {
            let c = p.cells.get(i).map(|c| c.ch).unwrap_or(' ');
            if c == '\u{b}' || c == '\n' {
                end = i;
                next = i + 1;
                break;
            }
            let adv = if c == '\t' {
                tab_advance(x0 + x, tab_stops, default_tab, tab_segment(&p.cells, i))
            } else {
                p.cells.get(i).map(|c| c.adv).unwrap_or(0.0)
            };
            let breakable = |j: usize| p.cells.get(j).is_none_or(|c| c.breakable(j));
            if i > start && allowed.get(i).copied().unwrap_or(false) && breakable(i) {
                last_break = Some(i);
            }
            // Closing East Asian punctuation hangs past the end instead of wrapping
            // (`hangingPunct`); the line breaks after it, and after the spaces and the line break
            // that follow it. It hangs only where the next line may start after it: "。」"
            // doesn't, it moves with the character before it as an overflowing "」" would.
            if wrap && hang && x + adv > avail + 0.01 && x <= avail + 0.01 && is_hanging_punct(c) && i > start {
                let after = (i + 1..n).find(|&j| !p.cells.get(j).is_some_and(|c| is_trailing_space(c.ch))).unwrap_or(n);
                let line_break = p.cells.get(after).is_some_and(|c| matches!(c.ch, '\u{b}' | '\n'));
                if after >= n || line_break || (allowed.get(after).copied().unwrap_or(false) && breakable(after)) {
                    end = i + 1;
                    next = if line_break { after + 1 } else { after };
                    break;
                }
            }
            // A tab that would run past the end moves to the next line (and to its first stop there).
            if wrap && c == '\t' && x + adv > avail + 0.01 && i > start {
                end = i;
                next = i;
                while end > start && p.cells.get(end - 1).is_some_and(|c| is_trailing_space(c.ch)) {
                    end -= 1;
                }
                break;
            }
            if wrap && x + adv > avail + 0.01 && !c.is_whitespace() && i > start {
                // Break at the last opportunity, or force mid-word (between clusters).
                let b = last_break
                    .filter(|b| *b > start)
                    .unwrap_or_else(|| (start + 1..=i).rev().find(|&j| breakable(j)).or_else(|| (i + 1..n).find(|&j| breakable(j))).unwrap_or(n));
                end = b;
                next = b;
                // Trailing spaces (ideographic ones too) belong to the line but don't count.
                while end > start && p.cells.get(end - 1).is_some_and(|c| is_trailing_space(c.ch)) {
                    end -= 1;
                }
                break;
            }
            x += adv;
            i += 1;
        }
        if i >= n {
            end = n;
            next = n;
        }
        lines.push(RawLine { start, end, next });
        if next <= start {
            // No progress: force one char.
            let last = lines.len() - 1;
            let to = (start + 1..n).find(|&j| p.cells.get(j).is_none_or(|c| c.breakable(j))).unwrap_or(n);
            if let Some(l) = lines.get_mut(last) {
                l.end = to;
                l.next = to;
            }
            start = to;
        } else {
            start = next;
        }
        if next == n && p.cells.last().is_some_and(|c| c.ch == '\u{b}') && start >= n {
            // A trailing line break opens an empty last line.
            lines.push(RawLine { start: n, end: n, next: n });
        }
    }
    lines
}

/// How a tab stop aligns the text after it (`a:tab/@algn`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum TabAlign {
    Left,
    Center,
    Right,
    /// The first "." of the text sits at the stop (text without one ends there).
    Decimal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tab {
    pos: f64,
    align: TabAlign,
}

pub(crate) fn tabs_of(props: &ParaProps) -> Vec<Tab> {
    props
        .tabs
        .as_ref()
        .map(|t| {
            t.iter()
                .map(|s| Tab {
                    pos: s.pos,
                    align: match s.align.as_str() {
                        "ctr" => TabAlign::Center,
                        "r" => TabAlign::Right,
                        "dec" => TabAlign::Decimal,
                        _ => TabAlign::Left,
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Width of the text after the tab at cell `i` up to the next tab or line break, and the width of
/// that text before its first ".".
pub(crate) fn tab_segment(cells: &[Cell], i: usize) -> (f64, f64) {
    let mut w = 0.0;
    let mut dec = None;
    for c in cells.iter().skip(i + 1) {
        if matches!(c.ch, '\t'..='\u{b}') {
            break;
        }
        if c.ch == '.' && dec.is_none() {
            dec = Some(w);
        }
        w += c.adv;
    }
    (w, dec.unwrap_or(w))
}

/// Advance of a tab at `x` (from the start edge) to the next stop, placing the text after it
/// (`seg`: its width, and its width before the decimal point) as the stop aligns it but never
/// moving back, as PowerPoint does; past the last stop, to the next default stop.
pub(crate) fn tab_advance(x: f64, stops: &[Tab], default_tab: f64, seg: (f64, f64)) -> f64 {
    for s in stops {
        if s.pos > x + 0.5 {
            let to = s.pos - x;
            return match s.align {
                TabAlign::Left => to,
                TabAlign::Right => (to - seg.0).max(0.0),
                TabAlign::Center => (to - seg.0 / 2.0).max(0.0),
                TabAlign::Decimal => (to - seg.1).max(0.0),
            };
        }
    }
    let d = if default_tab > 1.0 { default_tab } else { 72.0 };
    let next = ((x / d).floor() + 1.0) * d;
    (next - x).max(1.0)
}
