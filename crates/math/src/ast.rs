//! The equation tree: what a TeX string parses to, what OMML reads to, and what the layout draws.
//!
//! The shapes follow Office Math (ECMA-376 §22.1): a fraction, scripts, a radical, an n-ary
//! operator with limits, a delimited group, an accent, a bar, a function, limits above or below,
//! a matrix, an equation array, a group character (brace) and a bordered box. TeX constructs map
//! onto these one to one.

/// A row of equation content: an equation, an argument, a cell.
pub type Math = Vec<Node>;

/// How the characters of a run are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// TeX math: letters italic, digits and operators upright (OMML's default style).
    #[default]
    Math,
    /// Upright (`\mathrm`, function names; OMML `m:sty` `p`).
    Upright,
    /// Bold upright (`\mathbf`; `m:sty` `b`).
    Bold,
    /// Italic, digits included (`\mathit`; `m:sty` `i`).
    Italic,
    /// Bold italic (`\boldsymbol`; `m:sty` `bi`).
    BoldItalic,
    /// Ordinary text with its spaces (`\text`; OMML `m:nor`).
    Text,
}

impl Style {
    /// Whether a letter of this style is drawn italic.
    pub fn italic_letters(self) -> bool {
        matches!(self, Style::Math | Style::Italic | Style::BoldItalic)
    }
    /// Whether a character other than a letter is drawn italic.
    pub fn italic_others(self) -> bool {
        matches!(self, Style::Italic | Style::BoldItalic)
    }
    pub fn bold(self) -> bool {
        matches!(self, Style::Bold | Style::BoldItalic)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// Characters drawn as they are: letters, digits, operators, words.
    Run { text: String, style: Style },
    /// Numerator over denominator; `bar` false for a binomial-style stack.
    Frac { num: Math, den: Math, bar: bool },
    /// A base with a subscript, a superscript or both.
    Script { base: Math, sub: Option<Math>, sup: Option<Math> },
    /// A radical with an optional index (degree).
    Rad { index: Option<Math>, body: Math },
    /// An n-ary operator (∑ ∏ ∫ …) with its limits and the expression it applies to.
    /// `limits` puts the limits above and below the operator rather than beside it.
    Nary { op: char, sub: Option<Math>, sup: Option<Math>, body: Math, limits: bool },
    /// Items between stretching delimiters, separated by `sep`; `None` for no character.
    Delim { open: Option<char>, close: Option<char>, sep: Option<char>, items: Vec<Math> },
    /// A combining accent (U+0302 ̂ …) over the body.
    Acc { chr: char, body: Math },
    /// A line over (`top`) or under the body.
    Bar { body: Math, top: bool },
    /// A function name applied to an argument (sin x).
    Func { name: Math, arg: Math },
    /// A limit written under (`above` false) or over the base (lim, max, overbrace labels).
    Lim { base: Math, lim: Math, above: bool },
    /// Rows of cells, columns centred or (`left`) left-aligned.
    Matrix { rows: Vec<Vec<Math>>, left: bool },
    /// Lines of an equation stacked; a [`Node::AlignMark`] in a line is its alignment point.
    EqArr { rows: Vec<Math> },
    /// Alignment point inside an equation array row (TeX `&`).
    AlignMark,
    /// A brace (⏞ over, ⏟ under) stretched across the body.
    GroupChr { chr: char, body: Math, top: bool },
    /// A rectangle around the body (`\boxed`).
    Boxed { body: Math },
    /// Horizontal space, in em of the current size (negative for a back kern).
    Space(f64),
}

impl Node {
    pub fn run(text: impl Into<String>) -> Node {
        Node::Run { text: text.into(), style: Style::Math }
    }
    pub fn styled(text: impl Into<String>, style: Style) -> Node {
        Node::Run { text: text.into(), style }
    }
}

/// The plain text of a row (every run's characters), for searching and for readers without
/// equation support.
pub fn text_of(m: &Math) -> String {
    let mut s = String::new();
    for n in m {
        if let Node::Run { text, .. } = n {
            s.push_str(text);
        } else {
            s.push_str(&crate::linear::linear_node(n));
        }
    }
    s
}

/// Nodes nested in `m`, depth first (the row itself first).
pub fn depth(m: &Math) -> usize {
    fn d(m: &Math, level: usize, max: &mut usize) {
        *max = (*max).max(level);
        if level > 200 {
            return;
        }
        for n in m {
            for c in children(n) {
                d(c, level + 1, max);
            }
        }
    }
    let mut max = 0;
    d(m, 0, &mut max);
    max
}

/// The rows directly inside a node.
pub fn children(n: &Node) -> Vec<&Math> {
    match n {
        Node::Run { .. } | Node::AlignMark | Node::Space(_) => vec![],
        Node::Frac { num, den, .. } => vec![num, den],
        Node::Script { base, sub, sup } => std::iter::once(base).chain(sub.iter()).chain(sup.iter()).collect(),
        Node::Rad { index, body } => index.iter().chain(std::iter::once(body)).collect(),
        Node::Nary { sub, sup, body, .. } => sub.iter().chain(sup.iter()).chain(std::iter::once(body)).collect(),
        Node::Delim { items, .. } => items.iter().collect(),
        Node::Acc { body, .. } | Node::Bar { body, .. } | Node::GroupChr { body, .. } | Node::Boxed { body } => vec![body],
        Node::Func { name, arg } => vec![name, arg],
        Node::Lim { base, lim, .. } => vec![base, lim],
        Node::Matrix { rows, .. } => rows.iter().flatten().collect(),
        Node::EqArr { rows } => rows.iter().collect(),
    }
}
