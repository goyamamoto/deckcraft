//! Symbol tables: TeX control words and the characters they stand for, each character's spacing
//! class (TeX's atom types), delimiters, big operators, function names and accents.

/// TeX's atom classes, which decide the space between neighbours (The TeXbook, chapter 18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Ord,
    /// A large operator (∑, ∫) or a function name.
    Op,
    /// A binary operator (+, ×).
    Bin,
    /// A relation (=, <, →).
    Rel,
    Open,
    Close,
    Punct,
    /// A fraction, a delimited group, a matrix.
    Inner,
}

/// TeX control word (without the backslash) → character, for symbols that are a single
/// character. Greek, operators, relations, arrows, miscellany and delimiters.
pub const SYMBOLS: &[(&str, char)] = &[
    // Greek
    ("alpha", 'α'),
    ("beta", 'β'),
    ("gamma", 'γ'),
    ("delta", 'δ'),
    ("epsilon", 'ϵ'),
    ("varepsilon", 'ε'),
    ("zeta", 'ζ'),
    ("eta", 'η'),
    ("theta", 'θ'),
    ("vartheta", 'ϑ'),
    ("iota", 'ι'),
    ("kappa", 'κ'),
    ("lambda", 'λ'),
    ("mu", 'μ'),
    ("nu", 'ν'),
    ("xi", 'ξ'),
    ("omicron", 'ο'),
    ("pi", 'π'),
    ("varpi", 'ϖ'),
    ("rho", 'ρ'),
    ("varrho", 'ϱ'),
    ("sigma", 'σ'),
    ("varsigma", 'ς'),
    ("tau", 'τ'),
    ("upsilon", 'υ'),
    ("phi", 'ϕ'),
    ("varphi", 'φ'),
    ("chi", 'χ'),
    ("psi", 'ψ'),
    ("omega", 'ω'),
    ("Gamma", 'Γ'),
    ("Delta", 'Δ'),
    ("Theta", 'Θ'),
    ("Lambda", 'Λ'),
    ("Xi", 'Ξ'),
    ("Pi", 'Π'),
    ("Sigma", 'Σ'),
    ("Upsilon", 'Υ'),
    ("Phi", 'Φ'),
    ("Psi", 'Ψ'),
    ("Omega", 'Ω'),
    // Binary operators
    ("pm", '±'),
    ("mp", '∓'),
    ("times", '×'),
    ("div", '÷'),
    ("cdot", '⋅'),
    ("ast", '∗'),
    ("star", '⋆'),
    ("circ", '∘'),
    ("bullet", '∙'),
    ("oplus", '⊕'),
    ("ominus", '⊖'),
    ("otimes", '⊗'),
    ("oslash", '⊘'),
    ("odot", '⊙'),
    ("cap", '∩'),
    ("cup", '∪'),
    ("sqcap", '⊓'),
    ("sqcup", '⊔'),
    ("setminus", '∖'),
    ("wedge", '∧'),
    ("land", '∧'),
    ("vee", '∨'),
    ("lor", '∨'),
    ("uplus", '⊎'),
    ("amalg", '⨿'),
    ("wr", '≀'),
    ("dagger", '†'),
    ("ddagger", '‡'),
    ("diamond", '⋄'),
    ("triangleleft", '◃'),
    ("triangleright", '▹'),
    ("bigtriangleup", '△'),
    ("bigtriangledown", '▽'),
    // Relations
    ("leq", '≤'),
    ("le", '≤'),
    ("geq", '≥'),
    ("ge", '≥'),
    ("neq", '≠'),
    ("ne", '≠'),
    ("equiv", '≡'),
    ("approx", '≈'),
    ("sim", '∼'),
    ("simeq", '≃'),
    ("cong", '≅'),
    ("propto", '∝'),
    ("ll", '≪'),
    ("gg", '≫'),
    ("subset", '⊂'),
    ("supset", '⊃'),
    ("subseteq", '⊆'),
    ("supseteq", '⊇'),
    ("subsetneq", '⊊'),
    ("supsetneq", '⊋'),
    ("sqsubseteq", '⊑'),
    ("sqsupseteq", '⊒'),
    ("in", '∈'),
    ("notin", '∉'),
    ("ni", '∋'),
    ("owns", '∋'),
    ("parallel", '∥'),
    ("perp", '⊥'),
    ("mid", '∣'),
    ("nmid", '∤'),
    ("models", '⊨'),
    ("vdash", '⊢'),
    ("dashv", '⊣'),
    ("prec", '≺'),
    ("succ", '≻'),
    ("preceq", '≼'),
    ("succeq", '≽'),
    ("doteq", '≐'),
    ("asymp", '≍'),
    ("bowtie", '⋈'),
    ("smile", '⌣'),
    ("frown", '⌢'),
    ("coloneqq", '≔'),
    ("triangleq", '≜'),
    // Arrows
    ("to", '→'),
    ("rightarrow", '→'),
    ("leftarrow", '←'),
    ("gets", '←'),
    ("leftrightarrow", '↔'),
    ("Rightarrow", '⇒'),
    ("Leftarrow", '⇐'),
    ("Leftrightarrow", '⇔'),
    ("implies", '⟹'),
    ("impliedby", '⟸'),
    ("iff", '⟺'),
    ("mapsto", '↦'),
    ("longmapsto", '⟼'),
    ("uparrow", '↑'),
    ("downarrow", '↓'),
    ("updownarrow", '↕'),
    ("Uparrow", '⇑'),
    ("Downarrow", '⇓'),
    ("Updownarrow", '⇕'),
    ("longrightarrow", '⟶'),
    ("longleftarrow", '⟵'),
    ("longleftrightarrow", '⟷'),
    ("Longrightarrow", '⟹'),
    ("Longleftarrow", '⟸'),
    ("Longleftrightarrow", '⟺'),
    ("hookrightarrow", '↪'),
    ("hookleftarrow", '↩'),
    ("nearrow", '↗'),
    ("searrow", '↘'),
    ("swarrow", '↙'),
    ("nwarrow", '↖'),
    ("rightharpoonup", '⇀'),
    ("leftharpoonup", '↼'),
    ("rightleftharpoons", '⇌'),
    ("leadsto", '⇝'),
    // Miscellany
    ("infty", '∞'),
    ("partial", '∂'),
    ("nabla", '∇'),
    ("forall", '∀'),
    ("exists", '∃'),
    ("nexists", '∄'),
    ("emptyset", '∅'),
    ("varnothing", '∅'),
    ("hbar", 'ℏ'),
    ("ell", 'ℓ'),
    ("Re", 'ℜ'),
    ("Im", 'ℑ'),
    ("aleph", 'ℵ'),
    ("beth", 'ℶ'),
    ("wp", '℘'),
    ("prime", '′'),
    ("angle", '∠'),
    ("measuredangle", '∡'),
    ("triangle", '△'),
    ("square", '□'),
    ("Box", '□'),
    ("blacksquare", '■'),
    ("Diamond", '◇'),
    ("lozenge", '◊'),
    ("clubsuit", '♣'),
    ("diamondsuit", '♢'),
    ("heartsuit", '♡'),
    ("spadesuit", '♠'),
    ("flat", '♭'),
    ("natural", '♮'),
    ("sharp", '♯'),
    ("dots", '…'),
    ("ldots", '…'),
    ("cdots", '⋯'),
    ("vdots", '⋮'),
    ("ddots", '⋱'),
    ("therefore", '∴'),
    ("because", '∵'),
    ("degree", '°'),
    ("top", '⊤'),
    ("bot", '⊥'),
    ("neg", '¬'),
    ("lnot", '¬'),
    ("surd", '√'),
    ("checkmark", '✓'),
    ("S", '§'),
    ("P", '¶'),
    ("copyright", '©'),
    ("pounds", '£'),
    ("yen", '¥'),
    ("euro", '€'),
    ("imath", 'ı'),
    ("jmath", 'ȷ'),
    ("eth", 'ð'),
    ("ss", 'ß'),
    ("AA", 'Å'),
    ("backslash", '\\'),
    ("vert", '|'),
    ("Vert", '‖'),
    ("lvert", '|'),
    ("rvert", '|'),
    ("lVert", '‖'),
    ("rVert", '‖'),
    ("langle", '⟨'),
    ("rangle", '⟩'),
    ("lfloor", '⌊'),
    ("rfloor", '⌋'),
    ("lceil", '⌈'),
    ("rceil", '⌉'),
    ("lbrace", '{'),
    ("rbrace", '}'),
    ("lbrack", '['),
    ("rbrack", ']'),
    ("ulcorner", '⌜'),
    ("urcorner", '⌝'),
    ("llcorner", '⌞'),
    ("lrcorner", '⌟'),
    ("colon", ':'),
    ("%", '%'),
    ("$", '$'),
    ("&", '&'),
    ("#", '#'),
    ("_", '_'),
    ("{", '{'),
    ("}", '}'),
    ("|", '‖'),
];

/// Control word → character.
pub fn symbol(name: &str) -> Option<char> {
    SYMBOLS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Character → the TeX control word that produces it, for writing an equation back as TeX.
pub fn command_for(c: char) -> Option<&'static str> {
    if c.is_ascii() && !matches!(c, '%' | '$' | '&' | '#' | '_' | '{' | '}' | '\\') {
        return None;
    }
    SYMBOLS.iter().find(|(_, ch)| *ch == c).map(|(n, _)| *n)
}

/// Big operators: `\sum` and friends. The second value says whether TeX puts the limits above
/// and below the operator in display style (integrals keep them beside).
pub const NARY: &[(&str, char, bool)] = &[
    ("sum", '∑', true),
    ("prod", '∏', true),
    ("coprod", '∐', true),
    ("int", '∫', false),
    ("iint", '∬', false),
    ("iiint", '∭', false),
    ("oint", '∮', false),
    ("oiint", '∯', false),
    ("bigcup", '⋃', true),
    ("bigcap", '⋂', true),
    ("bigvee", '⋁', true),
    ("bigwedge", '⋀', true),
    ("bigoplus", '⨁', true),
    ("bigotimes", '⨂', true),
    ("bigodot", '⨀', true),
    ("biguplus", '⨄', true),
    ("bigsqcup", '⨆', true),
];

pub fn nary(name: &str) -> Option<(char, bool)> {
    NARY.iter().find(|(n, ..)| *n == name).map(|(_, c, l)| (*c, *l))
}

pub fn nary_command(c: char) -> Option<&'static str> {
    NARY.iter().find(|(_, ch, _)| *ch == c).map(|(n, ..)| *n)
}

/// Whether an n-ary operator character takes its limits above and below by default.
pub fn nary_limits(c: char) -> bool {
    NARY.iter().find(|(_, ch, _)| *ch == c).is_none_or(|(.., l)| *l)
}

/// Function names drawn upright: `\sin x`.
pub const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "arccot", "sinh", "cosh", "tanh", "coth", "log", "ln", "lg", "exp",
    "det", "dim", "ker", "deg", "hom", "arg", "gcd", "lcm", "Pr", "sgn", "tr", "rank", "mod", "Re", "Im",
];

/// Operators whose scripts are written under and over them: `\lim_{x\to 0}`.
pub const LIMIT_FUNCTIONS: &[&str] = &["lim", "limsup", "liminf", "max", "min", "sup", "inf", "argmax", "argmin", "varlimsup", "varliminf"];

/// Accents: control word → combining character (what OMML's `m:acc` stores).
pub const ACCENTS: &[(&str, char)] = &[
    ("hat", '\u{302}'),
    ("widehat", '\u{302}'),
    ("tilde", '\u{303}'),
    ("widetilde", '\u{303}'),
    ("bar", '\u{304}'),
    ("vec", '\u{20D7}'),
    ("overrightarrow", '\u{20D7}'),
    ("overleftarrow", '\u{20D6}'),
    ("dot", '\u{307}'),
    ("ddot", '\u{308}'),
    ("dddot", '\u{20DB}'),
    ("check", '\u{30C}'),
    ("breve", '\u{306}'),
    ("acute", '\u{301}'),
    ("grave", '\u{300}'),
    ("mathring", '\u{30A}'),
];

pub fn accent(name: &str) -> Option<char> {
    ACCENTS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

pub fn accent_command(c: char) -> Option<&'static str> {
    ACCENTS.iter().find(|(_, ch)| *ch == c).map(|(n, _)| *n)
}

/// The spacing (non-combining) character an accent is drawn with, and its relative size.
pub fn accent_glyph(c: char) -> (char, f64) {
    match c {
        '\u{302}' => ('ˆ', 1.0),
        '\u{303}' => ('˜', 1.0),
        '\u{304}' | '\u{305}' => ('¯', 1.0),
        '\u{20D7}' => ('→', 0.6),
        '\u{20D6}' => ('←', 0.6),
        '\u{307}' => ('˙', 1.0),
        '\u{308}' => ('¨', 1.0),
        '\u{20DB}' => ('⋯', 0.6),
        '\u{30C}' => ('ˇ', 1.0),
        '\u{306}' => ('˘', 1.0),
        '\u{301}' => ('´', 1.0),
        '\u{300}' => ('`', 1.0),
        '\u{30A}' => ('˚', 1.0),
        other => (other, 1.0),
    }
}

/// Space commands: control word (or symbol) → em of the current size.
pub fn space(name: &str) -> Option<f64> {
    Some(match name {
        "," | "thinspace" => 3.0 / 18.0,
        ":" | "medspace" => 4.0 / 18.0,
        ";" | "thickspace" => 5.0 / 18.0,
        "!" | "negthinspace" => -3.0 / 18.0,
        "quad" => 1.0,
        "qquad" => 2.0,
        " " | "~" | "nobreakspace" | "space" => 1.0 / 3.0,
        "enspace" => 0.5,
        _ => return None,
    })
}

/// The character a space of `em` is written as in OMML text (there is no space element).
pub fn space_char(em: f64) -> Option<&'static str> {
    if em <= 0.0 {
        None
    } else if em < 0.2 {
        Some("\u{2009}")
    } else if em < 0.26 {
        Some("\u{2005}")
    } else if em < 0.4 {
        Some(" ")
    } else if em < 0.75 {
        Some("\u{2002}")
    } else if em < 1.5 {
        Some("\u{2003}")
    } else {
        Some("\u{2003}\u{2003}")
    }
}

/// The width in em of a space character in equation text.
pub fn space_width(c: char) -> Option<f64> {
    Some(match c {
        ' ' | '\u{a0}' => 1.0 / 3.0,
        '\u{2009}' | '\u{200a}' => 3.0 / 18.0,
        '\u{2005}' | '\u{2006}' => 4.0 / 18.0,
        '\u{2004}' => 5.0 / 18.0,
        '\u{2002}' => 0.5,
        '\u{2003}' => 1.0,
        '\u{200b}' | '\u{2062}' | '\u{2063}' | '\u{2064}' => 0.0,
        _ => return None,
    })
}

/// A character's spacing class.
pub fn class_of(c: char) -> Class {
    match c {
        '+' | '−' | '-' | '±' | '∓' | '×' | '÷' | '⋅' | '·' | '∗' | '⋆' | '∘' | '∙' | '⊕' | '⊖' | '⊗' | '⊘' | '⊙' | '∩' | '∪' | '⊓' | '⊔' | '∖'
        | '∧' | '∨' | '⊎' | '⨿' | '≀' | '†' | '‡' | '⋄' | '◃' | '▹' | '*' => Class::Bin,
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≡' | '≢' | '≈' | '∼' | '≃' | '≅' | '∝' | '≪' | '≫' | '⊂' | '⊃' | '⊆' | '⊇' | '⊊' | '⊋' | '⊑' | '⊒'
        | '∈' | '∉' | '∋' | '∥' | '⊥' | '∣' | '∤' | '⊨' | '⊢' | '⊣' | '≺' | '≻' | '≼' | '≽' | '≐' | '≍' | '⋈' | '≔' | '≜' | '→' | '←' | '↔' | '⇒'
        | '⇐' | '⇔' | '⟹' | '⟸' | '⟺' | '↦' | '⟼' | '↑' | '↓' | '↕' | '⇑' | '⇓' | '⇕' | '⟶' | '⟵' | '⟷' | '↪' | '↩' | '↗' | '↘' | '↙' | '↖' | '⇀'
        | '↼' | '⇌' | '⇝' | ':' | '≮' | '≯' | '≰' | '≱' | '⊄' | '≁' => Class::Rel,
        '(' | '[' | '{' | '⟨' | '⌊' | '⌈' | '⌜' | '⌞' => Class::Open,
        ')' | ']' | '}' | '⟩' | '⌋' | '⌉' | '⌝' | '⌟' | '!' => Class::Close,
        ',' | ';' => Class::Punct,
        '∑' | '∏' | '∐' | '∫' | '∬' | '∭' | '∮' | '∯' | '⋃' | '⋂' | '⋁' | '⋀' | '⨁' | '⨂' | '⨀' | '⨄' | '⨆' => {
            Class::Op
        }
        _ => Class::Ord,
    }
}

/// Delimiters that stretch to the height of what they enclose.
pub fn stretches(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '|' | '‖' | '⟨' | '⟩' | '⌊' | '⌋' | '⌈' | '⌉' | '/' | '\\' | '⌜' | '⌝' | '⌞' | '⌟')
}

/// The negation of a relation (`\not=`), as one character when Unicode has it.
pub fn negate(c: char) -> Option<char> {
    Some(match c {
        '=' => '≠',
        '∈' => '∉',
        '≡' => '≢',
        '<' => '≮',
        '>' => '≯',
        '≤' => '≰',
        '≥' => '≱',
        '⊂' => '⊄',
        '⊃' => '⊅',
        '⊆' => '⊈',
        '⊇' => '⊉',
        '∼' => '≁',
        '≈' => '≉',
        '≅' => '≇',
        '∣' => '∤',
        '∥' => '∦',
        '∃' => '∄',
        _ => return None,
    })
}

/// A Latin letter or digit in a mathematical alphabet (`\mathbb`, `\mathcal`, `\mathfrak`,
/// `\mathsf`, `\mathtt`): the Mathematical Alphanumeric Symbols character, or the letter itself
/// when the alphabet has none.
pub fn alphabet(kind: Alphabet, c: char) -> char {
    let (upper, lower, digits): (u32, u32, Option<u32>) = match kind {
        Alphabet::DoubleStruck => match c {
            'C' => return 'ℂ',
            'H' => return 'ℍ',
            'N' => return 'ℕ',
            'P' => return 'ℙ',
            'Q' => return 'ℚ',
            'R' => return 'ℝ',
            'Z' => return 'ℤ',
            _ => (0x1D538, 0x1D552, Some(0x1D7D8)),
        },
        Alphabet::Script => match c {
            'B' => return 'ℬ',
            'E' => return 'ℰ',
            'F' => return 'ℱ',
            'H' => return 'ℋ',
            'I' => return 'ℐ',
            'L' => return 'ℒ',
            'M' => return 'ℳ',
            'R' => return 'ℛ',
            'e' => return 'ℯ',
            'g' => return 'ℊ',
            'o' => return 'ℴ',
            _ => (0x1D49C, 0x1D4B6, None),
        },
        Alphabet::Fraktur => match c {
            'C' => return 'ℭ',
            'H' => return 'ℌ',
            'I' => return 'ℑ',
            'R' => return 'ℜ',
            'Z' => return 'ℨ',
            _ => (0x1D504, 0x1D51E, None),
        },
        Alphabet::SansSerif => (0x1D5A0, 0x1D5BA, Some(0x1D7E2)),
        Alphabet::Monospace => (0x1D670, 0x1D68A, Some(0x1D7F6)),
    };
    let code = if c.is_ascii_uppercase() {
        upper + (c as u32 - 'A' as u32)
    } else if c.is_ascii_lowercase() {
        lower + (c as u32 - 'a' as u32)
    } else if let (true, Some(d)) = (c.is_ascii_digit(), digits) {
        d + (c as u32 - '0' as u32)
    } else {
        return c;
    };
    char::from_u32(code).unwrap_or(c)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alphabet {
    DoubleStruck,
    Script,
    Fraktur,
    SansSerif,
    Monospace,
}

/// The ordinary letter a mathematical-alphabet character stands for, and its alphabet.
pub fn plain_letter(c: char) -> Option<(Alphabet, char)> {
    let specials: &[(char, Alphabet, char)] = &[
        ('ℂ', Alphabet::DoubleStruck, 'C'),
        ('ℍ', Alphabet::DoubleStruck, 'H'),
        ('ℕ', Alphabet::DoubleStruck, 'N'),
        ('ℙ', Alphabet::DoubleStruck, 'P'),
        ('ℚ', Alphabet::DoubleStruck, 'Q'),
        ('ℝ', Alphabet::DoubleStruck, 'R'),
        ('ℤ', Alphabet::DoubleStruck, 'Z'),
        ('ℬ', Alphabet::Script, 'B'),
        ('ℰ', Alphabet::Script, 'E'),
        ('ℱ', Alphabet::Script, 'F'),
        ('ℋ', Alphabet::Script, 'H'),
        ('ℐ', Alphabet::Script, 'I'),
        ('ℒ', Alphabet::Script, 'L'),
        ('ℳ', Alphabet::Script, 'M'),
        ('ℛ', Alphabet::Script, 'R'),
        ('ℯ', Alphabet::Script, 'e'),
        ('ℊ', Alphabet::Script, 'g'),
        ('ℴ', Alphabet::Script, 'o'),
        ('ℭ', Alphabet::Fraktur, 'C'),
        ('ℌ', Alphabet::Fraktur, 'H'),
        ('ℨ', Alphabet::Fraktur, 'Z'),
    ];
    if let Some((_, a, l)) = specials.iter().find(|(ch, ..)| *ch == c) {
        return Some((*a, *l));
    }
    let code = c as u32;
    let ranges: &[(u32, u32, Alphabet, char)] = &[
        (0x1D538, 26, Alphabet::DoubleStruck, 'A'),
        (0x1D552, 26, Alphabet::DoubleStruck, 'a'),
        (0x1D7D8, 10, Alphabet::DoubleStruck, '0'),
        (0x1D49C, 26, Alphabet::Script, 'A'),
        (0x1D4B6, 26, Alphabet::Script, 'a'),
        (0x1D504, 26, Alphabet::Fraktur, 'A'),
        (0x1D51E, 26, Alphabet::Fraktur, 'a'),
        (0x1D5A0, 26, Alphabet::SansSerif, 'A'),
        (0x1D5BA, 26, Alphabet::SansSerif, 'a'),
        (0x1D7E2, 10, Alphabet::SansSerif, '0'),
        (0x1D670, 26, Alphabet::Monospace, 'A'),
        (0x1D68A, 26, Alphabet::Monospace, 'a'),
        (0x1D7F6, 10, Alphabet::Monospace, '0'),
    ];
    for (start, n, a, first) in ranges {
        if code >= *start && code < start + n {
            return char::from_u32(*first as u32 + (code - start)).map(|l| (*a, l));
        }
    }
    None
}

impl Alphabet {
    pub fn command(self) -> &'static str {
        match self {
            Alphabet::DoubleStruck => "mathbb",
            Alphabet::Script => "mathcal",
            Alphabet::Fraktur => "mathfrak",
            Alphabet::SansSerif => "mathsf",
            Alphabet::Monospace => "mathtt",
        }
    }
    pub fn from_command(name: &str) -> Option<Alphabet> {
        Some(match name {
            "mathbb" => Alphabet::DoubleStruck,
            "mathcal" | "mathscr" => Alphabet::Script,
            "mathfrak" => Alphabet::Fraktur,
            "mathsf" => Alphabet::SansSerif,
            "mathtt" => Alphabet::Monospace,
            _ => return None,
        })
    }
}
