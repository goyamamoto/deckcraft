use kurbo::{BezPath, Rect, Shape as _};

use super::*;
use crate::layout::{Glyph, Layout, MathFont};
use crate::symbols::Class;

/// A font whose every character has fixed, plausible metrics (em units, y down).
struct Mock;

fn ink_of(c: char) -> Rect {
    match c {
        'b' | 'd' | 'f' | 'h' | 'k' | 'l' | 't' => Rect::new(0.02, -0.7, 0.48, 0.0),
        'g' | 'j' | 'p' | 'q' | 'y' => Rect::new(0.02, -0.45, 0.48, 0.2),
        'a'..='z' => Rect::new(0.02, -0.45, 0.48, 0.0),
        'A'..='Z' | '0'..='9' => Rect::new(0.02, -0.7, 0.48, 0.0),
        '(' | ')' | '[' | ']' | '{' | '}' | '|' => Rect::new(0.05, -0.75, 0.35, 0.25),
        '+' | '−' | '=' | '<' | '>' | '×' | '±' | '≤' | '→' => Rect::new(0.05, -0.55, 0.55, -0.05),
        '∑' | '∫' | '∏' => Rect::new(0.05, -0.9, 0.95, 0.3),
        'ˆ' | '˜' | '¯' | '˙' | '¨' | 'ˇ' => Rect::new(0.1, -0.7, 0.4, -0.55),
        '′' => Rect::new(0.05, -0.7, 0.2, -0.4),
        _ => Rect::new(0.02, -0.6, 0.48, 0.0),
    }
}

impl MathFont for Mock {
    fn glyph(&mut self, c: char, bold: bool, italic: bool) -> Option<Glyph> {
        if c == '\u{E000}' {
            return None;
        }
        let ink = ink_of(c);
        let advance = match c {
            '(' | ')' | '[' | ']' | '{' | '}' | '|' => 0.4,
            '+' | '−' | '=' | '<' | '>' | '×' | '±' | '≤' | '→' => 0.6,
            '∑' | '∫' | '∏' => 1.0,
            '′' => 0.25,
            _ => 0.5,
        };
        Some(Glyph { face: u32::from(bold) + 2 * u32::from(italic), id: c as u32, advance, ink, fake_bold: false, fake_italic: false })
    }
    fn outline(&mut self, g: &Glyph) -> Option<BezPath> {
        Some(g.ink.to_path(0.1))
    }
}

fn lay(tex: &str, display: bool) -> Layout {
    let m = tex::parse(tex).unwrap_or_else(|e| panic!("{tex}: {e}"));
    layout::layout(&m, &mut Mock, 10.0, display)
}

fn glyph_of(l: &Layout, c: char) -> &layout::PlacedGlyph {
    l.glyphs.iter().find(|g| g.ch == c).unwrap_or_else(|| panic!("no glyph {c:?} in {:?}", l.glyphs.iter().map(|g| g.ch).collect::<String>()))
}

// --- TeX parsing --------------------------------------------------------------------------

#[test]
fn fractions_scripts_and_roots_parse_to_their_nodes() {
    let m = tex::parse("\\frac{a}{b}").unwrap();
    assert_eq!(m, vec![Node::Frac { num: vec![Node::run("a")], den: vec![Node::run("b")], bar: true }]);
    let m = tex::parse("x^2_i").unwrap();
    assert_eq!(m, vec![Node::Script { base: vec![Node::run("x")], sub: Some(vec![Node::run("i")]), sup: Some(vec![Node::run("2")]) }]);
    let m = tex::parse("\\sqrt[3]{x+1}").unwrap();
    assert_eq!(m, vec![Node::Rad { index: Some(vec![Node::run("3")]), body: vec![Node::run("x"), Node::run("+"), Node::run("1")] }]);
}

#[test]
fn scripts_attach_to_the_last_letter_of_a_run() {
    let m = tex::parse("ab^2").unwrap();
    assert_eq!(m, vec![Node::run("a"), Node::Script { base: vec![Node::run("b")], sub: None, sup: Some(vec![Node::run("2")]) }]);
    let m = tex::parse("{ab}^2").unwrap();
    assert_eq!(m, vec![Node::Script { base: vec![Node::run("ab")], sub: None, sup: Some(vec![Node::run("2")]) }]);
}

#[test]
fn primes_become_superscripts() {
    let m = tex::parse("f'(x) + g''").unwrap();
    assert!(matches!(&m[0], Node::Script { sup: Some(s), .. } if s == &vec![Node::styled("′", Style::Upright)]));
    assert!(matches!(m.last(), Some(Node::Script { sup: Some(s), .. }) if s == &vec![Node::styled("″", Style::Upright)]));
}

#[test]
fn sums_take_their_limits_and_the_expression_up_to_a_relation() {
    let m = tex::parse("\\sum_{i=1}^n i^2 = \\frac{n(n+1)(2n+1)}{6}").unwrap();
    let Node::Nary { op, sub, sup, body, limits } = &m[0] else { panic!("{m:?}") };
    assert_eq!((*op, *limits), ('∑', true));
    assert_eq!(sub.as_ref().unwrap().len(), 3);
    assert_eq!(sup, &Some(vec![Node::run("n")]));
    assert!(matches!(body.as_slice(), [Node::Script { .. }]), "{body:?}");
    assert_eq!(m[1], Node::run("="));
    assert!(matches!(m[2], Node::Frac { .. }));
    let m = tex::parse("\\int_0^1 f(x)\\,dx").unwrap();
    let Node::Nary { op, limits, body, .. } = &m[0] else { panic!("{m:?}") };
    assert_eq!((*op, *limits), ('∫', false));
    assert_eq!(body.len(), 6, "{body:?}");
    let m = tex::parse("\\int\\limits_a^b").unwrap();
    assert!(matches!(&m[0], Node::Nary { limits: true, .. }));
}

#[test]
fn left_right_delimiters_and_middles() {
    let m = tex::parse("\\left( \\frac{a}{b} \\right]").unwrap();
    assert!(matches!(&m[0], Node::Delim { open: Some('('), close: Some(']'), sep: None, items } if items.len() == 1));
    let m = tex::parse("\\left\\{ x \\middle| x>0 \\right\\}").unwrap();
    assert!(matches!(&m[0], Node::Delim { open: Some('{'), close: Some('}'), sep: Some('|'), items } if items.len() == 2));
    let m = tex::parse("\\left. \\frac{df}{dx} \\right|_{x=0}").unwrap();
    assert!(matches!(&m[0], Node::Script { base, .. } if matches!(base.as_slice(), [Node::Delim { open: None, close: Some('|'), .. }])));
}

#[test]
fn environments_become_matrices_cases_and_aligned_lines() {
    let m = tex::parse("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}").unwrap();
    let Node::Delim { open: Some('('), close: Some(')'), items, .. } = &m[0] else { panic!("{m:?}") };
    let Node::Matrix { rows, left: false } = &items[0][0] else { panic!("{items:?}") };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][1], vec![Node::run("d")]);
    let m = tex::parse("f(x) = \\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}").unwrap();
    assert!(
        matches!(m.last(), Some(Node::Delim { open: Some('{'), close: None, items, .. }) if matches!(items[0][0], Node::Matrix { left: true, .. }))
    );
    let m = tex::parse("\\begin{aligned} a &= b \\\\ c &= d + e \\end{aligned}").unwrap();
    let Node::EqArr { rows } = &m[0] else { panic!("{m:?}") };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], vec![Node::run("a"), Node::AlignMark, Node::run("="), Node::run("b")]);
}

#[test]
fn top_level_line_breaks_and_alignment_make_an_equation_array() {
    let m = tex::parse("a &= b \\\\ c &= d").unwrap();
    assert!(matches!(&m[0], Node::EqArr { rows } if rows.len() == 2 && rows[1].contains(&Node::AlignMark)));
    let m = tex::parse("a = b \\\\").unwrap();
    assert!(matches!(&m[0], Node::EqArr { rows } if rows.len() == 1));
}

#[test]
fn functions_limits_accents_text_and_alphabets() {
    let m = tex::parse("\\sin^2 x + \\cos^2 x = 1").unwrap();
    let Node::Func { name, arg } = &m[0] else { panic!("{m:?}") };
    assert!(matches!(name.as_slice(), [Node::Script { base, sup: Some(_), .. }] if base == &vec![Node::styled("sin", Style::Upright)]));
    assert_eq!(arg, &vec![Node::run("x")]);
    let m = tex::parse("\\lim_{x \\to 0} \\frac{\\sin x}{x}").unwrap();
    let Node::Func { name, arg } = &m[0] else { panic!("{m:?}") };
    assert!(matches!(name.as_slice(), [Node::Lim { above: false, lim, .. }] if lim.len() == 3));
    assert!(matches!(arg.as_slice(), [Node::Frac { .. }]));
    let m = tex::parse("\\hat{x} \\vec{v} \\overline{AB} \\underbrace{a+b}_{n}").unwrap();
    assert_eq!(m[0], Node::Acc { chr: '\u{302}', body: vec![Node::run("x")] });
    assert_eq!(m[1], Node::Acc { chr: '\u{20D7}', body: vec![Node::run("v")] });
    assert!(matches!(&m[2], Node::Bar { top: true, .. }));
    assert!(matches!(&m[3], Node::Script { base, sub: Some(_), sup: None } if matches!(base.as_slice(), [Node::GroupChr { top: false, .. }])));
    let m = tex::parse("\\text{for all } x \\in \\mathbb{R}").unwrap();
    assert_eq!(m[0], Node::styled("for all ", Style::Text));
    assert_eq!(m[3], Node::styled("ℝ", Style::Upright));
    let m = tex::parse("\\mathbf{v} \\cdot \\mathrm{d}x").unwrap();
    assert_eq!(m[0], Node::styled("v", Style::Bold));
    assert_eq!(m[2], Node::styled("d", Style::Upright));
    let m = tex::parse("\\alpha + \\beta \\leq \\infty, \\quad a \\not= b").unwrap();
    assert_eq!(m[0], Node::run("α"));
    assert_eq!(m[4], Node::run("∞"));
    assert_eq!(m[6], Node::Space(1.0));
    assert_eq!(m[8], Node::run("≠"));
}

#[test]
fn binom_choose_and_atop() {
    let m = tex::parse("\\binom{n}{k}").unwrap();
    assert!(matches!(&m[0], Node::Delim { open: Some('('), close: Some(')'), items, .. } if matches!(items[0][0], Node::Frac { bar: false, .. })));
    let m = tex::parse("{n \\choose k}").unwrap();
    assert!(matches!(&m[0], Node::Delim { .. }));
    let m = tex::parse("{a \\over b}").unwrap();
    assert!(matches!(&m[0], Node::Frac { bar: true, .. }));
}

#[test]
fn errors_name_the_problem_and_where_it_is() {
    let e = tex::parse("a + \\foo{b}").unwrap_err();
    assert_eq!(e.pos, 4);
    assert!(e.msg.contains("\\foo"), "{e}");
    let e = tex::parse("\\frac{a}").unwrap_err();
    assert!(e.msg.contains("argument"), "{e}");
    let e = tex::parse("x^2^3").unwrap_err();
    assert!(e.msg.contains("double superscript"), "{e}");
    let e = tex::parse("{a + b").unwrap_err();
    assert!(e.msg.contains("missing `}`"), "{e}");
    let e = tex::parse("a } b").unwrap_err();
    assert!(e.msg.contains("unexpected `}`"), "{e}");
    let e = tex::parse("\\left( a").unwrap_err();
    assert!(e.msg.contains("\\right"), "{e}");
    let e = tex::parse("\\begin{pmatrix} a").unwrap_err();
    assert!(e.msg.contains("\\end"), "{e}");
    let e = tex::parse("\\begin{foo} a \\end{foo}").unwrap_err();
    assert!(e.msg.contains("environment"), "{e}");
    assert_eq!(format!("{}", TexError { pos: 4, msg: "x".into() }), "x (at character 5)");
}

#[test]
fn parsing_never_panics_on_odd_input() {
    let deep = "{".repeat(200) + "x" + &"}".repeat(200);
    assert!(tex::parse(&deep).is_err());
    let long = "x".repeat(tex::MAX_LEN + 1);
    assert!(tex::parse(&long).is_err());
    let odd = [
        "\\",
        "^",
        "_",
        "&",
        "\\\\",
        "}",
        "{",
        "\\left",
        "\\right)",
        "\\begin",
        "\\begin{",
        "\\sqrt[",
        "\\text{",
        "%",
        "'",
        "\\not",
        "x^",
        "\\frac\\frac",
    ];
    for s in odd {
        let _ = tex::parse(s);
    }
    // Pseudo-random strings over the TeX alphabet.
    let alphabet: Vec<char> = "ab12+-=^_{}\\ &'()[]|.,;:!<>αβ∑ fracsqrtleftrightbeginendsumintlimtext".chars().collect();
    let mut seed = 0x1234_5678u64;
    for _ in 0..4000 {
        let mut s = String::new();
        for _ in 0..24 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            s.push(alphabet[(seed >> 33) as usize % alphabet.len()]);
        }
        if let Ok(m) = tex::parse(&s) {
            let _ = omml::write(&m);
            let _ = tex::to_tex(&m);
            let _ = linear::linear(&m);
            let _ = layout::layout(&m, &mut Mock, 10.0, true);
        }
    }
}

// --- OMML ----------------------------------------------------------------------------------

const CORPUS: &[&str] = &[
    "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
    "e^{i\\pi} + 1 = 0",
    "\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}",
    "\\int_0^\\infty e^{-x^2} \\, dx = \\frac{\\sqrt{\\pi}}{2}",
    "\\lim_{x \\to 0} \\frac{\\sin x}{x} = 1",
    "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix} \\begin{bmatrix} 1 \\\\ 0 \\end{bmatrix}",
    "f(x) = \\begin{cases} 1 & x \\geq 0 \\\\ 0 & \\text{otherwise} \\end{cases}",
    "\\hat{x} + \\vec{v} + \\overline{z} + \\underline{w} + \\dot{y}",
    "\\left( \\frac{a}{b} \\right) \\left[ x \\middle| y \\right]",
    "\\binom{n}{k} = \\frac{n!}{k!(n-k)!}",
    "\\nabla \\cdot \\mathbf{E} = \\frac{\\rho}{\\varepsilon_0}",
    "a &= b + c \\\\ d &= e",
    "\\sqrt[3]{x^3} = x, \\quad x \\in \\mathbb{R}",
    "\\overbrace{a+b}^{n} \\underbrace{c+d}_{m} \\boxed{E=mc^2}",
    "\\prod_{k=1}^n k = n! \\qquad \\oint_C f \\, dz",
    "\\sin^2 \\theta + \\cos^2 \\theta = 1",
    "\\max_{x} f(x) \\leq \\sup f",
    "\\alpha^{\\prime\\prime} + f'(x) + g''(x)",
];

#[test]
fn omml_round_trips_the_tree() {
    for tex in CORPUS {
        let m = tex::parse(tex).unwrap_or_else(|e| panic!("{tex}: {e}"));
        let xml = omml::write(&m);
        assert!(xml.starts_with("<m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>"), "{xml}");
        let back = omml::read(&xml).unwrap_or_else(|| panic!("{tex}: unreadable {xml}"));
        assert_eq!(back, m, "{tex}\n{xml}");
    }
}

#[test]
fn tex_round_trips_the_tree() {
    for tex in CORPUS {
        let m = tex::parse(tex).unwrap_or_else(|e| panic!("{tex}: {e}"));
        let again = tex::to_tex(&m);
        let back = tex::parse(&again).unwrap_or_else(|e| panic!("{tex} → {again}: {e}"));
        assert_eq!(back, m, "{tex}\n→ {again}");
    }
    assert_eq!(tex::to_tex(&tex::parse("\\frac{a}{b}").unwrap()), "\\frac{a}{b}");
    assert_eq!(tex::to_tex(&tex::parse("x^2_i").unwrap()), "x_{i}^{2}");
    assert_eq!(tex::to_tex(&tex::parse("\\alpha\\beta").unwrap()), "\\alpha \\beta");
}

#[test]
fn omml_written_by_office_reads_with_its_run_properties() {
    // What PowerPoint writes: `a:rPr` inside `m:r`, properties elements, a wrapper paragraph.
    let xml = r#"<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><m:oMathParaPr><m:jc m:val="centerGroup"/></m:oMathParaPr><m:oMath><m:sSup><m:sSupPr><m:ctrlPr><a:rPr lang="en-US" i="1"><a:latin typeface="Cambria Math"/></a:rPr></m:ctrlPr></m:sSupPr><m:e><m:r><a:rPr lang="en-US" i="1"><a:latin typeface="Cambria Math"/></a:rPr><m:t>x</m:t></m:r></m:e><m:sup><m:r><a:rPr lang="en-US" i="1"/><m:t>2</m:t></m:r></m:sup></m:sSup><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t xml:space="preserve"> + </m:t></m:r><m:f><m:fPr><m:type m:val="noBar"/></m:fPr><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f><m:nary><m:naryPr><m:limLoc m:val="undOvr"/><m:supHide m:val="1"/></m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup/><m:e><m:r><m:t>y</m:t></m:r></m:e></m:nary><m:d><m:e><m:r><m:t>z</m:t></m:r></m:e></m:d><m:eqArr><m:e><m:r><m:t>a&amp;=b</m:t></m:r></m:e></m:eqArr></m:oMath></m:oMathPara>"#;
    let m = omml::read(xml).unwrap();
    assert_eq!(m[0], Node::Script { base: vec![Node::run("x")], sub: None, sup: Some(vec![Node::run("2")]) });
    assert_eq!(m[1], Node::Space(1.0 / 3.0));
    assert_eq!(m[2], Node::styled("+", Style::Upright));
    assert_eq!(m[3], Node::Space(1.0 / 3.0));
    assert!(matches!(&m[4], Node::Frac { bar: false, .. }));
    assert!(matches!(&m[5], Node::Nary { op: '∫', limits: true, sub: Some(_), sup: None, .. }));
    assert!(matches!(&m[6], Node::Delim { open: Some('('), close: Some(')'), .. }));
    assert_eq!(m[7], Node::EqArr { rows: vec![vec![Node::run("a"), Node::AlignMark, Node::run("=b")]] });
    assert_eq!(tex::to_tex(&m[..1].to_vec()), "x^{2}");
}

#[test]
fn run_properties_go_into_every_run_and_read_back_unchanged() {
    let m = tex::parse("x^2 + \\text{a b}").unwrap();
    let props = "<a:rPr><a:latin typeface=\"Cambria Math\"/></a:rPr>";
    let xml = omml::write_runs(&m, props);
    assert_eq!(xml.matches(props).count(), 4, "{xml}");
    assert!(xml.contains("<m:r><m:rPr><m:nor/></m:rPr><a:rPr>"), "m:rPr first, then a:rPr: {xml}");
    assert_eq!(omml::read(&xml).unwrap(), m);
    assert_eq!(omml::write(&m), omml::write_runs(&m, ""));
}

#[test]
fn bad_omml_is_refused_or_read_leniently() {
    assert_eq!(omml::read("<m:oMath><m:r><m:t>x"), None);
    assert_eq!(omml::read("not xml at all <"), None);
    assert_eq!(omml::read(""), Some(vec![]));
    assert_eq!(omml::read("<m:oMath><m:unknown><m:r><m:t>q</m:t></m:r></m:unknown></m:oMath>"), Some(vec![Node::run("q")]));
    let bomb = "<a>".repeat(300) + &"</a>".repeat(300);
    assert_eq!(omml::read(&bomb), None);
}

#[test]
fn linear_form_reads_as_one_line() {
    let lin = |s: &str| linear::linear(&tex::parse(s).unwrap());
    assert_eq!(lin("\\frac{a}{b}"), "a/b");
    assert_eq!(lin("\\frac{a+1}{2b}"), "(a+1)/(2b)");
    assert_eq!(lin("x^2 + y_{ij}"), "x^2+y_ij");
    assert_eq!(lin("\\sqrt{x+1}"), "√(x+1)");
    assert_eq!(lin("\\sum_{i=1}^n i"), "∑_(i=1)^n▒i");
    assert_eq!(lin("\\left( a, b \\right)"), "(a,b)");
    assert_eq!(lin("\\alpha \\leq \\beta"), "α≤β");
    let (xml, text) = tex_to_omml("E = mc^2").unwrap();
    assert!(xml.contains("<m:sSup>"));
    assert_eq!(text, "E=mc^2");
    assert_eq!(omml_to_tex(&xml).as_deref(), Some("E = mc^{2}"));
}

// --- Layout --------------------------------------------------------------------------------

#[test]
fn fraction_puts_the_bar_between_numerator_and_denominator() {
    let l = lay("\\frac{a}{b}", true);
    let a = glyph_of(&l, 'a');
    let b = glyph_of(&l, 'b');
    let bar = l.rules.first().expect("bar");
    assert!(a.y < bar.y0, "numerator baseline {} above bar {}", a.y, bar.y0);
    assert!(a.y + 0.0 <= bar.y0, "numerator sits on or above the bar");
    assert!(b.y - 0.7 * b.size > bar.y1, "denominator top {} below bar {}", b.y - 0.7 * b.size, bar.y1);
    assert!((bar.y0 + bar.y1) / 2.0 < 0.0 && (bar.y0 + bar.y1) / 2.0 > -5.0, "bar on the axis: {bar:?}");
    assert!(l.ascent > 8.0 && l.descent > 4.0, "{l:?}");
    // Text style is tighter than display style.
    let t = lay("\\frac{a}{b}", false);
    assert!(t.height() < l.height());
}

#[test]
fn scripts_are_smaller_and_shifted() {
    let l = lay("x^2_i", true);
    let x = glyph_of(&l, 'x');
    let two = glyph_of(&l, '2');
    let i = glyph_of(&l, 'i');
    assert_eq!(x.size, 10.0);
    assert_eq!(two.size, 7.0);
    assert!(two.y < x.y - 3.0, "superscript raised: {}", two.y);
    assert!(i.y > x.y + 1.0, "subscript lowered: {}", i.y);
    assert!(two.x > x.x + 4.0 && (two.x - i.x).abs() < 0.01);
    let ss = lay("x^{y^z}", true);
    assert_eq!(glyph_of(&ss, 'z').size, 5.0);
}

#[test]
fn radical_covers_its_body_with_a_bar() {
    let l = lay("\\sqrt{x}", true);
    let x = glyph_of(&l, 'x');
    let bar = l.rules.iter().find(|r| r.width() > 3.0).expect("overbar");
    assert!(bar.y1 < x.y - 4.5, "bar {bar:?} above x at {}", x.y);
    assert!(bar.x0 < x.x && bar.x1 > x.x + 4.0);
    assert_eq!(l.paths.len(), 3, "the sign's three strokes");
    assert!(x.x > 3.0, "body after the sign");
    let idx = lay("\\sqrt[3]{x}", true);
    let three = glyph_of(&idx, '3');
    assert_eq!(three.size, 5.0);
    assert!(three.x < glyph_of(&idx, 'x').x);
}

#[test]
fn sum_limits_go_above_and_below_in_display_and_beside_in_text() {
    let d = lay("\\sum_{i=1}^{n} i", true);
    let op = glyph_of(&d, '∑');
    let n = glyph_of(&d, 'n');
    let one = glyph_of(&d, '1');
    assert_eq!(op.size, 14.0);
    assert!(n.y < op.y - 10.0, "upper limit above the operator: {} vs {}", n.y, op.y);
    assert!(one.y > op.y + 3.0, "lower limit below");
    let body = glyph_of(&d, 'i');
    let last_i = d.glyphs.iter().filter(|g| g.ch == 'i').map(|g| g.x).fold(0.0, f64::max);
    assert!(last_i > op.x + 10.0, "body after the operator {body:?}");
    let t = lay("\\sum_{i=1}^{n} i", false);
    let op = glyph_of(&t, '∑');
    let n = glyph_of(&t, 'n');
    assert_eq!(op.size, 10.0);
    assert!(n.x > op.x + 9.0, "limits beside in text style");
    let int = lay("\\int_0^1 x", true);
    assert!(glyph_of(&int, '1').x > glyph_of(&int, '∫').x + 10.0, "integral limits beside");
}

#[test]
fn delimiters_stretch_to_tall_content() {
    let small = lay("\\left( x \\right)", true);
    assert!(small.paths.is_empty(), "a glyph does for one line");
    assert!(small.glyphs.iter().any(|g| g.ch == '('));
    let tall = lay("\\left( \\frac{a}{b} \\right)", true);
    assert_eq!(tall.paths.len(), 2, "both parentheses stretched");
    let bb = tall.paths[0].bounding_box();
    assert!(bb.height() > 12.0, "{bb:?}");
    assert!((bb.center().y + 2.5).abs() < 1.0, "centred on the axis: {bb:?}");
}

#[test]
fn spacing_follows_the_atom_classes() {
    let gap = |tex: &str, a: char, b: char| {
        let l = lay(tex, true);
        glyph_of(&l, b).x - (glyph_of(&l, a).x + 5.0)
    };
    assert!((gap("ab", 'a', 'b')).abs() < 0.01, "no space between ordinary letters");
    assert!((gap("a+b", 'a', '+') - 4.0 / 18.0 * 10.0).abs() < 0.01, "medium space before a binary operator");
    assert!((gap("a=b", 'a', '=') - 5.0 / 18.0 * 10.0).abs() < 0.01, "thick space before a relation");
    assert!((gap("a,b", 'a', ',')).abs() < 0.01 && (gap("a,b", ',', 'b') - 3.0 / 18.0 * 10.0).abs() < 0.01, "thin space after punctuation");
    let neg = lay("-x", true);
    assert!((glyph_of(&neg, 'x').x - 6.0).abs() < 0.01, "a leading minus is ordinary: {}", glyph_of(&neg, 'x').x);
    let script = lay("x^{a+b}", true);
    let a = glyph_of(&script, 'a');
    let plus = glyph_of(&script, '+');
    assert!((plus.x - (a.x + 3.5)).abs() < 0.01, "no medium space in scripts: {}", plus.x - a.x);
}

#[test]
fn matrices_align_columns_and_centre_on_the_axis() {
    let l = lay("\\begin{matrix} a & bb \\\\ ccc & d \\end{matrix}", true);
    let a = glyph_of(&l, 'a');
    let d = glyph_of(&l, 'd');
    let c = l.glyphs.iter().filter(|g| g.ch == 'c').map(|g| g.x).fold(f64::INFINITY, f64::min);
    assert!(c < a.x, "wider cell starts left of the centred narrow one");
    assert!(d.y > a.y + 5.0, "second row below the first");
    assert!(l.ascent > 5.0 && l.descent > 2.0, "{l:?}");
    assert!((l.ascent - l.descent - 5.0).abs() < 0.01, "centred on the axis (2.5 pt): {} {}", l.ascent, l.descent);
}

#[test]
fn equation_arrays_line_up_at_their_marks() {
    let l = lay("a &= b \\\\ cc &= d", true);
    let eq: Vec<&layout::PlacedGlyph> = l.glyphs.iter().filter(|g| g.ch == '=').collect();
    assert_eq!(eq.len(), 2);
    assert!((eq[0].x - eq[1].x).abs() < 0.01, "equals signs aligned: {} {}", eq[0].x, eq[1].x);
    assert!(eq[1].y > eq[0].y + 5.0);
}

#[test]
fn accents_bars_limits_braces_and_boxes_grow_the_box() {
    let plain = lay("x", true);
    assert!(lay("\\hat{x}", true).ascent > plain.ascent + 1.0);
    assert!(lay("\\overline{x}", true).ascent > plain.ascent + 0.5);
    assert!(lay("\\underline{x}", true).descent > plain.descent + 0.5);
    let lim = lay("\\lim_{x \\to 0} f", true);
    assert!(lim.descent > 3.0, "limit below: {lim:?}");
    assert!(glyph_of(&lim, '0').y > glyph_of(&lim, 'l').y + 3.0);
    let brace = lay("\\overbrace{a+b}^{n}", true);
    assert_eq!(brace.paths.len(), 1);
    assert!(glyph_of(&brace, 'n').y < glyph_of(&brace, 'a').y - 8.0);
    let boxed = lay("\\boxed{x}", true);
    assert_eq!(boxed.rules.len(), 4);
    assert!(boxed.width > 7.0);
    let text = lay("\\text{if } x", true);
    assert!((glyph_of(&text, 'x').x - 13.333).abs() < 0.01, "text spaces are kept: {}", glyph_of(&text, 'x').x);
    let func = lay("\\sin x", true);
    assert!((glyph_of(&func, 'x').x - (15.0 + 3.0 / 18.0 * 10.0)).abs() < 0.01, "thin space after a function name");
}

#[test]
fn missing_glyphs_draw_a_box_and_empty_input_is_empty() {
    let m = vec![Node::run("\u{E000}")];
    let l = layout::layout(&m, &mut Mock, 10.0, true);
    assert_eq!(l.rules.len(), 4);
    assert!(l.glyphs.is_empty());
    let e = layout::layout(&vec![], &mut Mock, 10.0, true);
    assert_eq!((e.width, e.ascent, e.descent), (0.0, 0.0, 0.0));
    let nan = layout::layout(&tex::parse("x").unwrap(), &mut Mock, f64::NAN, true);
    assert!(nan.width.is_finite() && nan.width > 0.0);
}

#[test]
fn italic_letters_and_upright_digits_pick_their_faces() {
    let l = lay("x1\\mathrm{d}\\mathbf{v}", true);
    assert_eq!(glyph_of(&l, 'x').face, 2, "italic");
    assert_eq!(glyph_of(&l, '1').face, 0, "upright");
    assert_eq!(glyph_of(&l, 'd').face, 0, "upright by \\mathrm");
    assert_eq!(glyph_of(&l, 'v').face, 1, "bold");
    assert_eq!(symbols::class_of('+'), Class::Bin);
}
