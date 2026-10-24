//! Japanese line layout as PowerPoint for Mac draws it: justification, kerning, line breaking
//! (kinsoku, `eaLnBrk`, `latinLnBrk`, `hangingPunct`, the deck's `p:kinsoku` lists), the
//! ideographic space at a wrap, font alignment, vertical strikethroughs and circled numbers.

use super::*;
use deckcraft_model::Kinsoku;
use deckcraft_model::text::TextDir;

/// Japanese faces a test machine may have, installed or bundled.
const JAPANESE_FAMILIES: &[&str] = &[
    "BIZ UDPGothic",
    "BIZ UDGothic",
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Yu Gothic",
    "YuGothic",
    "Meiryo",
    "MS Gothic",
    "IPAexGothic",
    "IPAGothic",
    "TakaoGothic",
    "VL Gothic",
];

/// A face for Japanese and Latin text, for tests that measure Japanese lines (none: they are
/// skipped).
fn japanese_family() -> Option<String> {
    first_family(JAPANESE_FAMILIES, |f| ['あ', '漢', 'A', ' ', '。', '\u{3000}'].iter().all(|c| f.covers(*c)))
}

/// A run of `text` in language `lang`, set in `family` (Latin and East Asian slots) at `size`.
fn run_in(text: &str, lang: Option<&str>, family: &str, size: f64) -> Run {
    let mut r = Run::new(text);
    r.props.lang = lang.map(String::from);
    r.props.font = Some(family.into());
    r.props.font_ea = Some(family.into());
    r.props.size = Some(size);
    r
}

/// One paragraph of `runs` aligned `align`, `w` wide with no insets, `set` applied to it.
fn lay_runs(runs: Vec<Run>, w: f64, align: Align, set: impl Fn(&mut ParaProps)) -> TextLayout {
    let (p, mut sh) = bidi_setup(runs, w, false, align);
    let t = sh.text.as_mut().unwrap();
    t.body.inset_l = Some(0.0);
    t.body.inset_r = Some(0.0);
    set(&mut t.paragraphs[0].props);
    lay(&p, &sh, w, 2000.0)
}

/// The widths of line `i`'s characters.
fn widths(l: &TextLayout, i: usize) -> Vec<f64> {
    l.lines[i].edges.iter().map(|e| e.1 - e.0).collect()
}

/// The characters' natural widths: `runs` on one unjustified line.
fn natural(runs: Vec<Run>) -> Vec<f64> {
    let l = lay_runs(runs, 20_000.0, Align::Left, |_| {});
    assert_eq!(l.lines.len(), 1);
    widths(&l, 0)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn justified_kana_share_the_slack_equally_after_each_character_but_the_last() {
    // PowerPoint for Mac: 17 kana at 25 pt on a 432-pt line, 7 pt to spare, get 0.4375 pt after
    // each of the first 16 (whatever the face's kana measure, the slack is the box's width less
    // the 17 characters'). The ideographic space is one of the characters: a gap, no stretch.
    let Some(fam) = japanese_family() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    for text in ["あいうえおかきくけこさしすせそたちつてとなにぬねの", "あいう\u{3000}えおかきくけこさしすせそたちつてとなにぬねの"]
    {
        let runs = || vec![run_in(text, Some("ja-JP"), &fam, 25.0)];
        let nat = natural(runs());
        assert!(nat[17] > 7.0);
        let w = nat[..17].iter().sum::<f64>() + 7.0;
        let l = lay_runs(runs(), w, Align::Justify, |_| {});
        assert_eq!((l.lines[0].start, l.lines[0].end), (0, 17), "{text}");
        let got = widths(&l, 0);
        for k in 0..16 {
            assert!(close(got[k], nat[k] + 0.4375), "{text} #{k}: {} vs {}", got[k], nat[k]);
        }
        assert!(close(got[16], nat[16]), "nothing after the line's last character");
        assert!(close(l.lines[0].extent.0, 0.0) && close(l.lines[0].extent.1, w), "{:?} in {w}", l.lines[0].extent);
        // The paragraph's last line isn't stretched.
        let last = l.lines.len() - 1;
        let tail = &nat[l.lines[last].start..l.lines[last].end];
        assert!(widths(&l, last).iter().zip(tail).all(|(a, b)| close(*a, *b)));
    }
}

#[test]
fn a_mixed_line_stretches_its_space_first_then_spreads_the_rest_after_japanese_characters() {
    // PowerPoint for Mac: あいうえおかき (ja-JP) + くけこ Latin (en-US) with 41.08 pt to spare:
    // 4.58 after each of the seven Japanese characters, the space 4.45 (0.8 of its 5.56) + 4.58,
    // the en-US characters, kana included, nothing.
    let Some(fam) = japanese_family() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let runs = || vec![run_in("あいうえおかき", Some("ja-JP"), &fam, 20.0), run_in("くけこ Latin continuation", Some("en-US"), &fam, 20.0)];
    let nat = natural(runs());
    let slack = 41.08;
    let w = nat[..16].iter().sum::<f64>() + slack;
    let l = lay_runs(runs(), w, Align::Justify, |_| {});
    assert_eq!((l.lines[0].start, l.lines[0].end), (0, 16));
    let cap = 0.8 * nat[10];
    let each = (slack - cap) / 8.0;
    let got = widths(&l, 0);
    for k in 0..16 {
        let want = match k {
            0..=6 => nat[k] + each,
            10 => nat[k] + cap + each,
            _ => nat[k],
        };
        assert!(close(got[k], want), "#{k}: {} want {want}", got[k]);
    }
    assert!(close(l.lines[0].extent.1, w));
}

#[test]
fn a_justified_line_stretches_to_the_margin_before_its_hanging_comma() {
    // PowerPoint for Mac: 日本語とEnglish textの混在した段落で on a 432-pt line at 25 pt, its 、
    // hanging past the margin: the space takes the 2.9 pt to spare and the 、 starts at the margin.
    let Some(fam) = japanese_family() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let text = "日本語とEnglish textの混在した段落で、続きの文";
    let runs = || vec![run_in(text, Some("ja-JP"), &fam, 25.0)];
    let nat = natural(runs());
    let comma = text.chars().position(|c| c == '、').unwrap();
    let body: f64 = nat[..comma].iter().sum();
    let w = body + 2.9;
    let l = lay_runs(runs(), w, Align::Justify, |_| {});
    assert_eq!(l.lines[0].end, comma + 1, "the comma hangs on the first line");
    let got = widths(&l, 0);
    let space = text.chars().position(|c| c == ' ').unwrap();
    assert!(close(got[space], nat[space] + 2.9), "{} vs {}", got[space], nat[space]);
    assert!(close(l.lines[0].edges[comma].0, w), "the 、 starts at the margin: {:?}", l.lines[0].edges[comma]);
}

#[test]
fn latin_letters_in_a_japanese_run_spread_too_after_the_spaces_stretch() {
    let Some(fam) = japanese_family() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let runs = |lang: &str| vec![run_in("ab cd ef Wide lmnop", Some(lang), &fam, 20.0)];
    let nat = natural(runs("ja-JP"));
    let line: f64 = nat[..8].iter().sum();
    let cap = 0.8 * (nat[2] + nat[5]);
    // Slack beyond the spaces' stretch: they take 0.8 of their advance, then every character but
    // the last gets an equal share of the rest.
    let slack = cap + 7.0;
    let l = lay_runs(runs("ja-JP"), line + slack, Align::Justify, |_| {});
    assert_eq!(l.lines[0].end, 8);
    let got = widths(&l, 0);
    for k in 0..8 {
        let want = match k {
            2 | 5 => nat[k] * 1.8 + 1.0,
            7 => nat[k],
            _ => nat[k] + 1.0,
        };
        assert!(close(got[k], want), "#{k}: {} want {want}", got[k]);
    }
    // Less slack than that: the spaces share it by their advances and nothing else moves.
    let slack = cap / 2.0;
    let l = lay_runs(runs("ja-JP"), line + slack, Align::Justify, |_| {});
    let got = widths(&l, 0);
    for k in 0..8 {
        let want = if k == 2 || k == 5 { nat[k] * 1.4 } else { nat[k] };
        assert!(close(got[k], want), "#{k}: {} want {want}", got[k]);
    }
    // The same line in English is justified the Western way: the spaces take all of it.
    let nat = natural(runs("en-US"));
    let line: f64 = nat[..8].iter().sum();
    let l = lay_runs(runs("en-US"), line + 12.0, Align::Justify, |_| {});
    let got = widths(&l, 0);
    for k in 0..8 {
        let want = if k == 2 || k == 5 { nat[k] + 6.0 } else { nat[k] };
        assert!(close(got[k], want), "#{k}: {} want {want}", got[k]);
    }
}

#[test]
fn kanji_without_an_east_asian_language_are_not_spread() {
    // Kanji in an en-US run, or in a run with no language, justify the Western way: with no
    // space to take the slack the line stays at the start edge.
    let Some(fam) = japanese_family() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let text = "日本語の文章は空白なしで続きます";
    for lang in [Some("en-US"), None] {
        let nat = natural(vec![run_in(text, lang, &fam, 20.0)]);
        let w = nat[..10].iter().sum::<f64>() + 5.0;
        let l = lay_runs(vec![run_in(text, lang, &fam, 20.0)], w, Align::Justify, |_| {});
        assert_eq!(l.lines[0].end, 10);
        assert!(close(l.lines[0].extent.0, 0.0));
        assert!(widths(&l, 0).iter().zip(&nat).all(|(a, b)| close(*a, *b)), "{lang:?}");
    }
    // In Chinese or Korean they spread.
    let nat = natural(vec![run_in(text, Some("zh-CN"), &fam, 20.0)]);
    let w = nat[..10].iter().sum::<f64>() + 4.5;
    let l = lay_runs(vec![run_in(text, Some("zh-CN"), &fam, 20.0)], w, Align::Justify, |_| {});
    assert!(close(widths(&l, 0)[0], nat[0] + 0.5));
}

#[test]
fn kana_and_kanji_are_never_kerned() {
    // PowerPoint for Mac kerns Latin pairs at kern="1200" but not kana, even in a face with kana
    // pairs (Hiragino Sans: し→す -1.0 pt and て→と -0.75 pt at 25 pt, which DeckCraft applied).
    let advance = |f: &FontFace, s: &str, kern: bool| {
        let off: Vec<deckcraft_fonts::Feature> = deckcraft_fonts::feature("-kern").into_iter().collect();
        deckcraft_fonts::shape(f, s, if kern { &[] } else { &off }, |c| c).iter().map(|g| g.x_advance).sum::<i32>()
    };
    let kana_pairs = |f: &FontFace| f.covers('し') && advance(f, "しすてと", true) != advance(f, "しすてと", false);
    let Some(fam) = first_family(JAPANESE_FAMILIES, kana_pairs) else {
        eprintln!("skipped: no Japanese face with kana kerning pairs");
        return;
    };
    let width = |text: &str, kern: f64| {
        let mut r = run_in(text, Some("ja-JP"), &fam, 25.0);
        r.props.kern = Some(kern);
        let l = lay_runs(vec![r], 20_000.0, Align::Left, |_| {});
        l.lines[0].extent.1 - l.lines[0].extent.0
    };
    for text in ["しすてと", "漢字「しす」、てと。"] {
        assert!(close(width(text, 12.0), width(text, 0.0)), "{text}: kerned");
    }
    // Latin pairs in the same face still kern.
    let face = FontDb::global().face(&fam, "Regular");
    if advance(&face, "AVAV", true) != advance(&face, "AVAV", false) {
        assert!(width("AVAV", 12.0) < width("AVAV", 0.0) - 0.1);
    }
}

#[test]
fn without_east_asian_line_breaking_a_full_stop_and_small_kana_start_lines() {
    let text = "白なしで続きます。句読点も含めて。";
    let w = one_line("白なしで続きます") + 0.5;
    // eaLnBrk and hangingPunct absent everywhere: kinsoku and hanging punctuation are on, as
    // PowerPoint for Mac lays it out, and "。" hangs past the margin.
    assert_eq!(wrapped(text, w, |_| {})[0], "白なしで続きます。");
    assert_eq!(wrapped(text, w, |p| p.east_asian_line_break = Some(true))[0], "白なしで続きます。");
    // eaLnBrk="0": no kinsoku, no hanging: "。" starts the next line.
    let lines = wrapped(text, w, |p| p.east_asian_line_break = Some(false));
    assert_eq!(lines[0], "白なしで続きます", "{lines:?}");
    assert!(lines[1].starts_with('。'), "{lines:?}");
    // Small kana and the prolonged sound mark may start a line too, and an opening bracket end one.
    let w = one_line("あいうえお") + 0.5;
    for text in ["あいうえおっかき", "あいうえおーかき"] {
        assert_eq!(wrapped(text, w, |_| {})[0], "あいうえ");
        assert_eq!(wrapped(text, w, |p| p.east_asian_line_break = Some(false))[0], "あいうえお", "{text}");
    }
    let w = one_line("あいうえ「") + 0.5;
    assert_eq!(wrapped("あいうえ「かき」くけ", w, |_| {})[0], "あいうえ");
    assert_eq!(wrapped("あいうえ「かき」くけ", w, |p| p.east_asian_line_break = Some(false))[0], "あいうえ「");
}

#[test]
fn hanging_punct_off_moves_the_character_before_the_full_stop_down() {
    let text = "白なしで続きます。句読点も含めて。";
    let w = one_line("白なしで続きます") + 0.5;
    assert_eq!(wrapped(text, w, |p| p.hanging_punct = Some(false))[0], "白なしで続きま");
}

/// The lines of Japanese (`ja-JP`) `text` `w` wide in a deck with `kinsoku` lists.
fn wrapped_in_deck(text: &str, w: f64, lang: &str, kinsoku: Option<Kinsoku>, set: impl Fn(&mut ParaProps)) -> Vec<String> {
    let mut run = Run::new(text);
    run.props.lang = Some(lang.into());
    let (mut p, mut sh) = bidi_setup(vec![run], w, false, Align::Left);
    p.kinsoku = kinsoku;
    let t = sh.text.as_mut().unwrap();
    t.body.inset_l = Some(0.0);
    t.body.inset_r = Some(0.0);
    set(&mut t.paragraphs[0].props);
    let l = lay(&p, &sh, w, 2000.0);
    let chars: Vec<char> = text.chars().collect();
    l.lines.iter().map(|li| chars[li.start..li.end].iter().collect()).collect()
}

/// Width of Japanese (`ja-JP`) `text` on one line.
fn one_line_ja(text: &str) -> f64 {
    let mut run = Run::new(text);
    run.props.lang = Some("ja-JP".into());
    let (p, sh) = bidi_setup(vec![run], 5000.0, false, Align::Left);
    let l = lay(&p, &sh, 5000.0, 300.0);
    l.lines[0].caret_x.last().unwrap() - l.lines[0].caret_x[0]
}

#[test]
fn the_decks_kinsoku_lists_replace_the_standard_ones() {
    let lists = |lang: Option<&str>, start: &str, end: &str| Some(Kinsoku { lang: lang.map(String::from), start: start.into(), end: end.into() });
    let no_hang = |p: &mut ParaProps| p.hanging_punct = Some(false);
    // invalStChars="あ": "あ" no longer starts a line, and "。", not in the list, does.
    let text = "白なしで続きます。句読点";
    let w = one_line_ja("白なしで続きます") + 0.5;
    assert_eq!(wrapped_in_deck(text, w, "ja-JP", None, no_hang)[0], "白なしで続きま");
    let lines = wrapped_in_deck(text, w, "ja-JP", lists(None, "あ", ""), no_hang);
    assert_eq!(lines[0], "白なしで続きます", "{lines:?}");
    assert!(lines[1].starts_with('。'));
    let w = one_line_ja("いいいい") + 0.5;
    assert_eq!(wrapped_in_deck("いいいいあいう", w, "ja-JP", None, |_| {})[0], "いいいい");
    assert_eq!(wrapped_in_deck("いいいいあいう", w, "ja-JP", lists(None, "あ", ""), |_| {})[0], "いいい");
    assert_eq!(wrapped_in_deck("いいいいあいう", w, "ja-JP", lists(Some("ja-JP"), "あ", ""), |_| {})[0], "いいい");
    // invalEndChars="か": "か" no longer ends a line.
    let w = one_line_ja("あいうか") + 0.5;
    assert_eq!(wrapped_in_deck("あいうかきく", w, "ja-JP", None, |_| {})[0], "あいうか");
    assert_eq!(wrapped_in_deck("あいうかきく", w, "ja-JP", lists(None, "", "か"), |_| {})[0], "あいう");
    // Lists for another language, or a paragraph in another language, keep the standard rules.
    let w = one_line_ja("いいいい") + 0.5;
    assert_eq!(wrapped_in_deck("いいいいあいう", w, "ja-JP", lists(Some("zh-CN"), "あ", ""), |_| {})[0], "いいいい");
    assert_eq!(wrapped_in_deck("いいいいあいう", w, "en-US", lists(None, "あ", ""), |_| {})[0], "いいいい");
    // Latin text keeps Unicode's breaks: only at spaces.
    let w = one_line_ja("alpha be") + 0.5;
    assert_eq!(wrapped_in_deck("alpha beta gamma", w, "ja-JP", lists(None, "", ""), |_| {})[0].trim_end(), "alpha");
}

#[test]
fn an_ideographic_space_at_a_wrap_does_not_count_toward_the_line() {
    // Like spaces at a wrap, a trailing "　" neither pushes the line to wrap nor counts toward its
    // width: right-aligned, the line ends at the box's edge, not an em short of it.
    let text = "あいうえ\u{3000}かきくけ";
    let w = one_line("あいうえ") + 0.5;
    let (p, mut sh) = bidi_setup(vec![Run::new(text)], w, false, Align::Right);
    let t = sh.text.as_mut().unwrap();
    t.body.inset_l = Some(0.0);
    t.body.inset_r = Some(0.0);
    let l = lay(&p, &sh, w, 600.0);
    assert_eq!((l.lines[0].start, l.lines[0].end), (0, 4));
    assert_eq!(l.lines[1].start, 5, "the space ends the first line");
    assert!((l.lines[0].extent.1 - w).abs() < 0.01, "{:?} in {w}", l.lines[0].extent);
    // Mid-line it is an ordinary character.
    let l = lay(&p, &sh, 5000.0, 600.0);
    assert_eq!((l.lines[0].start, l.lines[0].end), (0, 9));
}

#[test]
fn circled_numbers_go_on_to_fifty() {
    for (n, want) in [(1, "①"), (20, "⑳"), (21, "㉑"), (35, "㉟"), (36, "㊱"), (50, "㊿"), (51, "51"), (120, "120")] {
        assert_eq!(autonum("circleNumDbPlain", n), want, "{n}");
    }
}

/// The glyph y of the first run of `size` points.
fn run_y(l: &TextLayout, size: f64) -> f64 {
    l.runs.iter().find(|r| (r.size - size).abs() < 1e-9 && !r.glyphs.is_empty()).map(|r| r.glyphs[0].2).unwrap()
}

#[test]
fn font_alignment_lines_runs_up_by_their_tops_centres_or_bottoms() {
    let runs = || vec![sized_run("Big ", 40.0), sized_run("small", 12.0)];
    let l = lay_runs(runs(), 2000.0, Align::Left, |_| {});
    let base = l.lines[0].baseline;
    let face = |size: f64| l.runs.iter().find(|r| (r.size - size).abs() < 1e-9 && !r.glyphs.is_empty()).map(|r| r.face.clone()).unwrap();
    let (ab, db) = line_metrics(&face(40.0), 40.0);
    let (a, d) = line_metrics(&face(12.0), 12.0);
    for (algn, want) in
        [(None, 0.0), (Some("auto"), 0.0), (Some("base"), 0.0), (Some("t"), a - ab), (Some("b"), db - d), (Some("ctr"), ((db - ab) - (d - a)) / 2.0)]
    {
        let l = lay_runs(runs(), 2000.0, Align::Left, |p| p.font_align = algn.map(String::from));
        assert!(close(l.lines[0].baseline, base), "the line doesn't move");
        assert!(close(run_y(&l, 40.0), base), "{algn:?}: the tallest run stays on the baseline");
        assert!(close(run_y(&l, 12.0), base + want), "{algn:?}: {} want {}", run_y(&l, 12.0), base + want);
    }
    // The small run's underline moves with it.
    let mut small = sized_run("small", 12.0);
    small.props.underline = Some("sng".into());
    let l = lay_runs(vec![sized_run("Big ", 40.0), small.clone()], 2000.0, Align::Left, |_| {});
    let t = lay_runs(vec![sized_run("Big ", 40.0), small], 2000.0, Align::Left, |p| p.font_align = Some("t".into()));
    assert!(close(t.decos[0].rect.y0 - l.decos[0].rect.y0, a - ab));
}

/// An East Asian vertical (`eaVert`) body of `runs`, fontAlgn `algn`, laid out.
fn ea_vertical(runs: Vec<Run>, algn: Option<&str>) -> TextLayout {
    let (p, mut sh) = setup("x", 300.0, 600.0);
    let t = sh.text.as_mut().unwrap();
    t.body.vert = Some(TextDir::EaVertical);
    t.paragraphs = vec![Paragraph { runs, ..Default::default() }];
    t.paragraphs[0].props.bullet = Some(Bullet::None);
    t.paragraphs[0].props.font_align = algn.map(String::from);
    lay(&p, &sh, 300.0, 600.0)
}

#[test]
fn mixed_sizes_share_the_column_centre_in_east_asian_vertical_text() {
    // fontAlgn auto is the centre in eaVert (ECMA-376): upright characters of two sizes stand on
    // one centre line down the column; on the baseline the smaller one sits off it.
    let centre = |l: &TextLayout, size: f64| {
        let r = l.runs.iter().find(|r| r.upright && (r.size - size).abs() < 1e-9).unwrap();
        let g = r.glyphs[0];
        r.upright_centre(g.0, g.1, g.2).unwrap().1
    };
    let runs = || vec![sized_run("日本", 24.0), sized_run("語", 12.0)];
    for algn in [None, Some("auto"), Some("ctr")] {
        let l = ea_vertical(runs(), algn);
        assert!(close(centre(&l, 24.0), centre(&l, 12.0)), "{algn:?}");
        assert!(close(centre(&l, 24.0), l.lines[0].baseline - 24.0 * UPRIGHT_CENTRE), "the tallest stays");
    }
    let l = ea_vertical(runs(), Some("base"));
    assert!(close(centre(&l, 12.0) - centre(&l, 24.0), 12.0 * UPRIGHT_CENTRE));
    // One size: nothing moves.
    let l = ea_vertical(vec![sized_run("日本語", 20.0)], None);
    assert!(close(centre(&l, 20.0), l.lines[0].baseline - 20.0 * UPRIGHT_CENTRE));
}

#[test]
fn a_strikethrough_crosses_upright_characters_through_their_middle() {
    for (strike, lines) in [(Strike::Single, 1), (Strike::Double, 2)] {
        let mut r = sized_run("日本", 20.0);
        r.props.strike = Some(strike);
        let l = ea_vertical(vec![r], None);
        assert_eq!(l.decos.len(), lines);
        let (top, bottom) = l.decos.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(t, b), d| (t.min(d.rect.y0), b.max(d.rect.y1)));
        assert!(close((top + bottom) / 2.0, l.lines[0].baseline - 20.0 * UPRIGHT_CENTRE), "{strike:?}");
    }
    // Latin lying on its side in the column keeps its font's strikeout position.
    let mut r = sized_run("ABC", 20.0);
    r.props.strike = Some(Strike::Single);
    let l = ea_vertical(vec![r], None);
    let face = &l.runs.iter().find(|r| !r.glyphs.is_empty()).unwrap().face;
    let k = 20.0 / face.upem.max(1.0);
    assert!(l.decos.iter().all(|d| close(d.rect.y0, l.lines[0].baseline - face.strikeout.0 * k)), "{:?}", l.decos);
}

#[test]
fn vertical_lines_say_where_the_first_line_stands() {
    let body = |vert: TextDir| BodyProps { vert: Some(vert), ..Default::default() };
    assert_eq!(vertical_lines(&BodyProps::default()), None);
    assert_eq!(vertical_lines(&body(TextDir::Horizontal)), None);
    for v in [TextDir::Vertical, TextDir::EaVertical, TextDir::StackedRtl] {
        assert_eq!(vertical_lines(&body(v)), Some(VerticalLines::RightToLeft), "{v:?}");
    }
    for v in [TextDir::Vertical270, TextDir::MongolianVertical, TextDir::Stacked] {
        assert_eq!(vertical_lines(&body(v)), Some(VerticalLines::LeftToRight), "{v:?}");
    }
    // A body turned by `rot` too.
    assert_eq!(vertical_lines(&BodyProps { rot: Some(90.0), ..Default::default() }), Some(VerticalLines::RightToLeft));
}

#[test]
fn hostile_japanese_settings_never_panic() {
    let (mut p, mut sh) = bidi_setup(vec![Run::new("あ。」「\u{3000}\u{3000}ーっ\u{200B}abc\u{AD}def\u{3099}")], 3.0, false, Align::Justify);
    p.kinsoku = Some(Kinsoku { lang: Some("not a language".into()), start: "\u{0}".repeat(5000), end: "。".into() });
    for (ea, latin, hang, algn) in [(false, true, true, "t"), (true, false, false, "b"), (true, true, true, "ctr"), (false, false, false, "??")] {
        let pp = &mut sh.text.as_mut().unwrap().paragraphs[0].props;
        pp.east_asian_line_break = Some(ea);
        pp.latin_line_break = Some(latin);
        pp.hanging_punct = Some(hang);
        pp.font_align = Some(algn.into());
        for w in [0.0, 1.0, 10.0, 60.0, 5000.0] {
            let l = lay(&p, &sh, w, 300.0);
            assert!(l.lines.iter().all(|li| li.start <= li.end));
        }
        p.kinsoku = Some(Kinsoku { lang: None, start: "あいうえお".into(), end: "あいうえお".into() });
    }
}
