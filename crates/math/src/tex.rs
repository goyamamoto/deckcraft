//! TeX ⇄ equation tree.
//!
//! [`parse`] reads the TeX most people type for mathematics: `\frac`, `^` and `_`, `\sqrt[n]`,
//! `\sum_{i=1}^n`, `\left( … \right)`, `\begin{pmatrix}`, `\sin`, `\lim_{x\to 0}`, accents,
//! `\text{}`, `\mathrm{}`, `\mathbf{}`, `\mathbb{}`, Greek and the usual symbols, spaces, `&` and
//! `\\` for aligned lines. Anything else is an error that names the command and where it is, so
//! the editor can point at it. [`to_tex`] writes a tree back (an imported equation opens as TeX).

use std::fmt;

use crate::ast::{Math, Node, Style};
use crate::symbols::{self, Alphabet, Class};

/// A parse error: `pos` is the character index the error is at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexError {
    pub pos: usize,
    pub msg: String,
}

impl fmt::Display for TexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at character {})", self.msg, self.pos + 1)
    }
}

impl std::error::Error for TexError {}

/// Longest TeX string parsed; longer input is an error rather than a stall.
pub const MAX_LEN: usize = 20_000;
/// Deepest nesting of groups and arguments.
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Char(char),
    /// A control word: `\frac` → `frac`.
    Word(String),
    /// A control symbol: `\,` → `,`.
    Sym(char),
    Open,
    Close,
    Sup,
    Sub,
    Amp,
    /// `\\`
    NewRow,
    Prime,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    pos: usize,
}

fn tokenize(chars: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let pos = i;
        i += 1;
        match c {
            c if c.is_whitespace() => {}
            '%' => {
                while chars.get(i).is_some_and(|c| *c != '\n') {
                    i += 1;
                }
            }
            '\\' => match chars.get(i) {
                Some(&n) if n.is_ascii_alphabetic() => {
                    let mut w = String::new();
                    while let Some(&l) = chars.get(i).filter(|l| l.is_ascii_alphabetic()) {
                        w.push(l);
                        i += 1;
                    }
                    // `\begin{align*}` keeps its star inside the braces; `\sum*` is not TeX.
                    out.push(Token { tok: Tok::Word(w), pos });
                }
                Some('\\') => {
                    i += 1;
                    out.push(Token { tok: Tok::NewRow, pos });
                }
                Some(&n) => {
                    i += 1;
                    out.push(Token { tok: Tok::Sym(n), pos });
                }
                None => out.push(Token { tok: Tok::Word(String::new()), pos }),
            },
            '{' => out.push(Token { tok: Tok::Open, pos }),
            '}' => out.push(Token { tok: Tok::Close, pos }),
            '^' => out.push(Token { tok: Tok::Sup, pos }),
            '_' => out.push(Token { tok: Tok::Sub, pos }),
            '&' => out.push(Token { tok: Tok::Amp, pos }),
            '\'' => out.push(Token { tok: Tok::Prime, pos }),
            c => out.push(Token { tok: Tok::Char(c), pos }),
        }
    }
    out
}

struct Parser<'a> {
    toks: &'a [Token],
    chars: &'a [char],
    i: usize,
    depth: usize,
    /// Inside `\sqrt[…]`: a `]` ends the row.
    in_index: bool,
}

type R<T> = Result<T, TexError>;

/// Parse TeX into an equation. Lines separated by `\\` become an equation array; `&` marks the
/// alignment point of a line.
pub fn parse(s: &str) -> R<Math> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > MAX_LEN {
        return Err(TexError { pos: MAX_LEN, msg: format!("the equation is longer than {MAX_LEN} characters") });
    }
    let toks = tokenize(&chars);
    let mut p = Parser { toks: &toks, chars: &chars, i: 0, depth: 0, in_index: false };
    let mut rows: Vec<Math> = Vec::new();
    let mut row: Math = Vec::new();
    loop {
        let mut part = p.row()?;
        row.append(&mut part);
        match p.peek() {
            None => break,
            Some(Tok::Amp) => {
                p.i += 1;
                row.push(Node::AlignMark);
            }
            Some(Tok::NewRow) => {
                p.i += 1;
                rows.push(std::mem::take(&mut row));
            }
            Some(Tok::Close) => return Err(p.err("unexpected `}` with no `{` open")),
            Some(Tok::Word(w)) if w == "right" => return Err(p.err("`\\right` without `\\left`")),
            Some(Tok::Word(w)) if w == "end" => return Err(p.err("`\\end` without `\\begin`")),
            Some(Tok::Word(w)) if w == "middle" => return Err(p.err("`\\middle` outside `\\left … \\right`")),
            Some(_) => return Err(p.err("unexpected input")),
        }
    }
    if rows.is_empty() {
        if row.iter().any(|n| matches!(n, Node::AlignMark)) {
            return Ok(vec![Node::EqArr { rows: vec![row] }]);
        }
        return Ok(row);
    }
    // A trailing `\\` leaves an empty last line; keep it only when there is content.
    if !row.is_empty() {
        rows.push(row);
    }
    Ok(vec![Node::EqArr { rows }])
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.tok)
    }

    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.chars.len(), |t| t.pos)
    }

    fn err(&self, msg: impl Into<String>) -> TexError {
        TexError { pos: self.pos(), msg: msg.into() }
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.i).cloned();
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("the equation is nested too deeply"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Nodes up to the next `}`, `&`, `\\`, `\right`, `\end` or the end of the input, which is
    /// left for the caller. `\over`, `\atop` and `\choose` split the row into a fraction.
    fn row(&mut self) -> R<Math> {
        let mut row: Math = Vec::new();
        loop {
            match self.peek() {
                None | Some(Tok::Close) | Some(Tok::Amp) | Some(Tok::NewRow) => break,
                Some(Tok::Char(']')) if self.in_index => break,
                Some(Tok::Word(w)) if w == "right" || w == "end" || w == "middle" => break,
                Some(Tok::Word(w)) if w == "over" || w == "atop" || w == "choose" => {
                    let kind = w.clone();
                    self.i += 1;
                    let den = self.row()?;
                    let num = std::mem::take(&mut row);
                    let frac = Node::Frac { num, den, bar: kind == "over" };
                    row.push(if kind == "choose" {
                        Node::Delim { open: Some('('), close: Some(')'), sep: None, items: vec![vec![frac]] }
                    } else {
                        frac
                    });
                    break;
                }
                Some(_) => {
                    if !self.item(&mut row)? {
                        break;
                    }
                }
            }
        }
        Ok(row)
    }

    /// One nucleus with the scripts and primes after it, appended to `row`. False when the next
    /// token starts no nucleus.
    fn item(&mut self, row: &mut Math) -> R<bool> {
        let Some((mut nucleus, grouped)) = self.nucleus()? else { return Ok(false) };
        let (mut sub, mut sup) = (None, None);
        let mut primes = 0usize;
        loop {
            match self.peek() {
                Some(Tok::Sup) => {
                    self.i += 1;
                    if sup.is_some() {
                        return Err(self.err("double superscript: use `^{…}` for both parts"));
                    }
                    sup = Some(self.arg("^")?);
                }
                Some(Tok::Sub) => {
                    self.i += 1;
                    if sub.is_some() {
                        return Err(self.err("double subscript: use `_{…}` for both parts"));
                    }
                    sub = Some(self.arg("_")?);
                }
                Some(Tok::Prime) => {
                    self.i += 1;
                    primes += 1;
                }
                _ => break,
            }
        }
        if primes > 0 {
            let marks: String = match primes {
                1 => "′".into(),
                2 => "″".into(),
                3 => "‴".into(),
                _ => "′".repeat(primes.min(8)),
            };
            let mut s = vec![Node::styled(marks, Style::Upright)];
            s.extend(sup.take().unwrap_or_default());
            sup = Some(s);
        }
        if sub.is_some() || sup.is_some() {
            // Scripts belong to the last character: `ab^2` is a b², `{ab}^2` is not.
            let base = match nucleus.pop() {
                Some(Node::Run { text, style }) if !grouped && text.chars().count() > 1 => {
                    let mut it = text.chars();
                    let last = it.next_back().map(String::from).unwrap_or_default();
                    row.append(&mut nucleus);
                    row.push(Node::Run { text: it.collect(), style });
                    vec![Node::Run { text: last, style }]
                }
                Some(last) if nucleus.is_empty() => vec![last],
                Some(last) => {
                    nucleus.push(last);
                    std::mem::take(&mut nucleus)
                }
                None => Vec::new(),
            };
            row.extend(nucleus);
            row.push(Node::Script { base, sub, sup });
        } else {
            row.extend(nucleus);
        }
        Ok(true)
    }

    /// An argument: a `{…}` group, or a single character or command.
    fn arg(&mut self, what: &str) -> R<Math> {
        match self.peek() {
            Some(Tok::Open) => {
                self.i += 1;
                self.group_rest()
            }
            Some(Tok::Char(c)) => {
                // One character, as TeX takes it: `x^10` is x¹0.
                let c = *c;
                self.i += 1;
                Ok(vec![Node::run(
                    match c {
                        '-' => '−',
                        '*' => '∗',
                        c => c,
                    }
                    .to_string(),
                )])
            }
            Some(Tok::Word(_)) | Some(Tok::Sym(_)) => {
                // A command that is itself a structure (`\frac\alpha\beta`) counts as one argument.
                match self.nucleus()? {
                    Some((n, _)) => Ok(n),
                    None => Err(self.err(format!("`{what}` needs an argument"))),
                }
            }
            _ => Err(self.err(format!("`{what}` needs an argument"))),
        }
    }

    /// The rest of a `{…}` group whose `{` was consumed; a single node comes back unwrapped.
    fn group_rest(&mut self) -> R<Math> {
        self.enter()?;
        let row = self.row()?;
        self.leave();
        match self.peek() {
            Some(Tok::Close) => {
                self.i += 1;
                Ok(row)
            }
            Some(Tok::Amp) | Some(Tok::NewRow) => Err(self.err("`&` and `\\\\` belong inside `\\begin{…}` rows")),
            Some(Tok::Word(w)) if w == "right" => Err(self.err("`\\right` without `\\left` in this group")),
            Some(Tok::Word(w)) if w == "end" => Err(self.err("`\\end` without `\\begin` in this group")),
            _ => Err(self.err("missing `}`")),
        }
    }

    /// The raw text of a `{…}` group (for `\text`), spaces kept.
    fn raw_group(&mut self, what: &str) -> R<String> {
        let Some(Token { tok: Tok::Open, pos }) = self.toks.get(self.i).cloned() else {
            return Err(self.err(format!("`\\{what}` needs a braced argument")));
        };
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            match t.tok {
                Tok::Open => depth += 1,
                Tok::Close => {
                    depth -= 1;
                    if depth == 0 {
                        let text: String = self.chars.get(pos + 1..t.pos).unwrap_or_default().iter().collect();
                        self.i = j + 1;
                        return Ok(text.replace("\\{", "{").replace("\\}", "}").replace("\\\\", "\\"));
                    }
                }
                _ => {}
            }
            j += 1;
        }
        Err(TexError { pos, msg: format!("`\\{what}`: missing `}}`") })
    }

    /// The next nucleus and whether it was a braced group.
    fn nucleus(&mut self) -> R<Option<(Math, bool)>> {
        loop {
            let Some(t) = self.next() else { return Ok(None) };
            return Ok(Some((
                match t.tok {
                    Tok::Close | Tok::Amp | Tok::NewRow => {
                        self.i -= 1;
                        return Ok(None);
                    }
                    Tok::Char(c) => self.chars_run(c),
                    Tok::Open => {
                        let row = self.group_rest()?;
                        return Ok(Some((if row.is_empty() { vec![Node::run("")] } else { row }, true)));
                    }
                    // `^2` with nothing before it: an empty base.
                    Tok::Sup | Tok::Sub => {
                        self.i -= 1;
                        vec![Node::run("")]
                    }
                    Tok::Prime => vec![Node::styled("′", Style::Upright)],
                    Tok::Sym(c) => match c {
                        ',' | ':' | ';' | '!' | ' ' => vec![Node::Space(symbols::space(&c.to_string()).unwrap_or(0.0))],
                        '{' | '}' | '%' | '$' | '&' | '#' | '_' => vec![Node::run(c.to_string())],
                        '|' => vec![Node::run("‖")],
                        '\'' | '`' | '^' | '"' | '~' | '.' => continue,
                        other => return Err(TexError { pos: t.pos, msg: format!("unknown command `\\{other}`") }),
                    },
                    Tok::Word(name) => match self.command(&name, t.pos)? {
                        Some(nodes) => nodes,
                        None => continue,
                    },
                },
                false,
            )));
        }
    }

    /// Consecutive letters or digits (with a decimal point) as one run; `-` is the minus sign.
    fn chars_run(&mut self, c: char) -> Math {
        let mut text = String::new();
        match c {
            '-' => text.push('−'),
            '*' => text.push('∗'),
            c if c.is_ascii_digit() => {
                text.push(c);
                while let Some(Tok::Char(n)) = self.peek() {
                    let n = *n;
                    let dot = n == '.' && matches!(self.toks.get(self.i + 1).map(|t| &t.tok), Some(Tok::Char(d)) if d.is_ascii_digit());
                    if n.is_ascii_digit() || dot {
                        text.push(n);
                        self.i += 1;
                    } else {
                        break;
                    }
                }
            }
            c if c.is_alphabetic() && c.is_ascii() => {
                text.push(c);
                while let Some(Tok::Char(n)) = self.peek() {
                    if n.is_ascii_alphabetic() {
                        text.push(*n);
                        self.i += 1;
                    } else {
                        break;
                    }
                }
            }
            c => text.push(c),
        }
        vec![Node::run(text)]
    }

    /// A control word's nodes; `None` for words that produce nothing (`\displaystyle`).
    fn command(&mut self, name: &str, pos: usize) -> R<Option<Math>> {
        let one = |n: Node| Ok(Some(vec![n]));
        match name {
            "" => Err(TexError { pos, msg: "a `\\` with no command after it".into() }),
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.arg(&format!("\\{name}"))?;
                let den = self.arg(&format!("\\{name}"))?;
                one(Node::Frac { num, den, bar: true })
            }
            "binom" | "dbinom" | "tbinom" => {
                let num = self.arg(&format!("\\{name}"))?;
                let den = self.arg(&format!("\\{name}"))?;
                one(Node::Delim { open: Some('('), close: Some(')'), sep: None, items: vec![vec![Node::Frac { num, den, bar: false }]] })
            }
            "sqrt" => {
                let index = if matches!(self.peek(), Some(Tok::Char('['))) {
                    self.i += 1;
                    self.enter()?;
                    let was = self.in_index;
                    self.in_index = true;
                    let idx = self.row();
                    self.in_index = was;
                    let idx = idx?;
                    self.leave();
                    if !matches!(self.peek(), Some(Tok::Char(']'))) {
                        return Err(self.err("`\\sqrt[`: missing `]`"));
                    }
                    self.i += 1;
                    Some(idx)
                } else {
                    None
                };
                let body = self.arg("\\sqrt")?;
                one(Node::Rad { index, body })
            }
            "left" => self.left().map(Some),
            "right" => Err(TexError { pos, msg: "`\\right` without `\\left`".into() }),
            "middle" => Err(TexError { pos, msg: "`\\middle` outside `\\left … \\right`".into() }),
            "begin" => self.environment(pos).map(Some),
            "end" => Err(TexError { pos, msg: "`\\end` without `\\begin`".into() }),
            "text" | "textrm" | "textnormal" | "mbox" | "textit" | "textbf" | "textsf" | "texttt" | "hbox" => {
                let s = self.raw_group(name)?;
                let style = match name {
                    "textbf" => Style::Bold,
                    "textit" => Style::Italic,
                    _ => Style::Text,
                };
                one(Node::Run { text: s, style })
            }
            "mathrm" | "operatorname" | "mathop" | "textstyle" | "mathnormal" | "mathit" | "mathbf" | "boldsymbol" | "bm" | "pmb" => {
                if name == "textstyle" {
                    return Ok(None);
                }
                let mut body = self.arg(&format!("\\{name}"))?;
                let style = match name {
                    "mathrm" | "operatorname" | "mathop" => Style::Upright,
                    "mathit" => Style::Italic,
                    "mathbf" => Style::Bold,
                    "boldsymbol" | "bm" | "pmb" => Style::BoldItalic,
                    _ => Style::Math,
                };
                restyle(&mut body, style);
                Ok(Some(body))
            }
            "mathbb" | "mathcal" | "mathscr" | "mathfrak" | "mathsf" | "mathtt" => {
                let mut body = self.arg(&format!("\\{name}"))?;
                if let Some(a) = Alphabet::from_command(name) {
                    alphabetize(&mut body, a);
                }
                Ok(Some(body))
            }
            "overline" | "underline" => {
                let body = self.arg(&format!("\\{name}"))?;
                one(Node::Bar { body, top: name == "overline" })
            }
            "overbrace" | "underbrace" => {
                let body = self.arg(&format!("\\{name}"))?;
                let top = name == "overbrace";
                one(Node::GroupChr { chr: if top { '⏞' } else { '⏟' }, body, top })
            }
            "overset" | "stackrel" | "underset" => {
                let lim = self.arg(&format!("\\{name}"))?;
                let base = self.arg(&format!("\\{name}"))?;
                one(Node::Lim { base, lim, above: name != "underset" })
            }
            "boxed" => {
                let body = self.arg("\\boxed")?;
                one(Node::Boxed { body })
            }
            "not" => {
                let mut body = self.arg("\\not")?;
                match body.first_mut() {
                    Some(Node::Run { text, .. }) if text.chars().count() == 1 => {
                        let c = text.chars().next().unwrap_or('=');
                        *text = symbols::negate(c).map(String::from).unwrap_or_else(|| format!("{c}\u{338}"));
                    }
                    _ => body.insert(0, Node::run("\u{338}")),
                }
                Ok(Some(body))
            }
            "quad" | "qquad" | "thinspace" | "medspace" | "thickspace" | "negthinspace" | "enspace" | "space" | "nobreakspace" => {
                one(Node::Space(symbols::space(name).unwrap_or(0.0)))
            }
            "hspace" | "hspace*" => {
                let s = self.raw_group("hspace")?;
                one(Node::Space(parse_length(&s)))
            }
            "displaystyle" | "scriptstyle" | "scriptscriptstyle" | "nonumber" | "notag" | "mathstrut" | "strut" | "noindent" | "relax"
            | "allowbreak" | "limits" | "nolimits" | "displaylimits" | "nobreak" | "ignorespaces" | "smash" | "vphantom" | "left." => {
                if matches!(name, "smash" | "vphantom") {
                    // `\smash{x}` draws x with no height: here, as x.
                    return self.arg(&format!("\\{name}")).map(Some);
                }
                Ok(None)
            }
            "label" | "tag" | "color" | "vspace" | "phantom" | "hphantom" | "ref" | "eqref" | "cite" => {
                let _ = self.raw_group(name)?;
                Ok(None)
            }
            "textcolor" => {
                let _ = self.raw_group(name)?;
                self.arg("\\textcolor").map(Some)
            }
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl" | "biggr" | "Biggl" | "Biggr" | "bigm" | "Bigm"
            | "biggm" | "Biggm" => {
                let d = self.delimiter(name)?;
                Ok(Some(d.map(|c| vec![Node::run(c.to_string())]).unwrap_or_default()))
            }
            "substack" => {
                let s = self.raw_group("substack")?;
                let rows: Vec<Math> = s.split("\\\\").map(|r| parse(r).unwrap_or_default()).collect();
                one(Node::Matrix { rows: rows.into_iter().map(|r| vec![r]).collect(), left: false })
            }
            "mathbin" | "mathrel" | "mathord" | "mathpunct" | "mathinner" | "mathopen" | "mathclose" => self.arg(&format!("\\{name}")).map(Some),
            _ => {
                if let Some((op, limits)) = symbols::nary(name) {
                    return self.nary(op, limits).map(Some);
                }
                if let Some(chr) = symbols::accent(name) {
                    let body = self.arg(&format!("\\{name}"))?;
                    return one(Node::Acc { chr, body });
                }
                if symbols::LIMIT_FUNCTIONS.contains(&name) {
                    return self.limit_function(name).map(Some);
                }
                if symbols::FUNCTIONS.contains(&name) {
                    return self.function(vec![Node::styled(name, Style::Upright)]).map(Some);
                }
                if let Some(c) = symbols::symbol(name) {
                    return one(Node::run(c.to_string()));
                }
                Err(TexError { pos, msg: format!("unknown command `\\{name}`") })
            }
        }
    }

    /// A delimiter after `\left`, `\right`, `\middle` or `\big`: `(`, `\{`, `\langle`, `.` for
    /// none.
    fn delimiter(&mut self, after: &str) -> R<Option<char>> {
        let Some(t) = self.next() else { return Err(self.err(format!("`\\{after}` needs a delimiter"))) };
        let c = match t.tok {
            Tok::Char('.') => return Ok(None),
            Tok::Char(c) if matches!(c, '(' | ')' | '[' | ']' | '|' | '/' | '<' | '>') => match c {
                '<' => '⟨',
                '>' => '⟩',
                c => c,
            },
            Tok::Sym('{') => '{',
            Tok::Sym('}') => '}',
            Tok::Sym('|') => '‖',
            Tok::Word(ref w) => match symbols::symbol(w) {
                Some(c) if symbols::stretches(c) || matches!(c, '↑' | '↓' | '↕' | '⇑' | '⇓' | '⇕') => c,
                _ => return Err(TexError { pos: t.pos, msg: format!("`\\{w}` is not a delimiter") }),
            },
            _ => return Err(TexError { pos: t.pos, msg: format!("`\\{after}` needs a delimiter such as `(` or `\\{{`") }),
        };
        Ok(Some(c))
    }

    fn left(&mut self) -> R<Math> {
        let open = self.delimiter("left")?;
        self.enter()?;
        let mut items = Vec::new();
        let mut sep = None;
        loop {
            items.push(self.row()?);
            match self.peek() {
                Some(Tok::Word(w)) if w == "right" => {
                    self.i += 1;
                    break;
                }
                Some(Tok::Word(w)) if w == "middle" => {
                    self.i += 1;
                    let d = self.delimiter("middle")?;
                    if sep.is_none() {
                        sep = d;
                    }
                }
                _ => return Err(self.err("`\\left` without `\\right`")),
            }
        }
        self.leave();
        let close = self.delimiter("right")?;
        Ok(vec![Node::Delim { open, close, sep, items }])
    }

    fn environment(&mut self, pos: usize) -> R<Math> {
        let name = self.raw_group("begin")?;
        let name = name.trim().to_string();
        if name == "array" || name == "tabular" {
            // The column specification (`{cc}`) is not kept.
            let _ = self.raw_group("array")?;
        }
        self.enter()?;
        let mut rows: Vec<Vec<Math>> = Vec::new();
        let mut cur: Vec<Math> = Vec::new();
        loop {
            cur.push(self.row()?);
            match self.peek() {
                Some(Tok::Amp) => self.i += 1,
                Some(Tok::NewRow) => {
                    self.i += 1;
                    // `\\[2pt]`: optional space, dropped.
                    if matches!(self.peek(), Some(Tok::Char('['))) {
                        while let Some(t) = self.next() {
                            if t.tok == Tok::Char(']') {
                                break;
                            }
                        }
                    }
                    rows.push(std::mem::take(&mut cur));
                }
                Some(Tok::Word(w)) if w == "end" => {
                    self.i += 1;
                    let end = self.raw_group("end")?;
                    if end.trim() != name {
                        return Err(self.err(format!("`\\begin{{{name}}}` closed by `\\end{{{}}}`", end.trim())));
                    }
                    break;
                }
                _ => return Err(TexError { pos, msg: format!("`\\begin{{{name}}}` without `\\end{{{name}}}`") }),
            }
        }
        self.leave();
        if !(cur.len() == 1 && cur.first().is_some_and(Vec::is_empty)) {
            rows.push(cur);
        }
        let matrix = |rows: Vec<Vec<Math>>, left: bool| Node::Matrix { rows, left };
        let delim = |open: char, close: char, inner: Node| Node::Delim { open: Some(open), close: Some(close), sep: None, items: vec![vec![inner]] };
        Ok(vec![match name.as_str() {
            "matrix" | "smallmatrix" | "array" | "tabular" => matrix(rows, false),
            "pmatrix" => delim('(', ')', matrix(rows, false)),
            "bmatrix" => delim('[', ']', matrix(rows, false)),
            "Bmatrix" => delim('{', '}', matrix(rows, false)),
            "vmatrix" => delim('|', '|', matrix(rows, false)),
            "Vmatrix" => delim('‖', '‖', matrix(rows, false)),
            "cases" | "dcases" | "rcases" => Node::Delim { open: Some('{'), close: None, sep: None, items: vec![vec![matrix(rows, true)]] },
            "aligned" | "align" | "align*" | "alignat" | "alignat*" | "flalign" | "eqnarray" | "eqnarray*" | "split" | "gathered" | "gather"
            | "gather*" | "multline" | "multline*" | "equation" | "equation*" | "displaymath" | "math" => {
                let lines = rows
                    .into_iter()
                    .map(|cells| {
                        let mut line = Vec::new();
                        for (k, mut cell) in cells.into_iter().enumerate() {
                            if k > 0 {
                                line.push(Node::AlignMark);
                            }
                            line.append(&mut cell);
                        }
                        line
                    })
                    .collect();
                Node::EqArr { rows: lines }
            }
            other => return Err(TexError { pos, msg: format!("unknown environment `{other}`") }),
        }])
    }

    /// `\sum_{i=1}^{n} body`: the scripts in either order, then the expression up to the next
    /// relation, binary operator or punctuation at this level.
    fn nary(&mut self, op: char, default_limits: bool) -> R<Math> {
        let (mut sub, mut sup) = (None, None);
        let mut limits = default_limits;
        loop {
            match self.peek() {
                Some(Tok::Sub) if sub.is_none() => {
                    self.i += 1;
                    sub = Some(self.arg("_")?);
                }
                Some(Tok::Sup) if sup.is_none() => {
                    self.i += 1;
                    sup = Some(self.arg("^")?);
                }
                Some(Tok::Word(w)) if w == "limits" => {
                    self.i += 1;
                    limits = true;
                }
                Some(Tok::Word(w)) if w == "nolimits" => {
                    self.i += 1;
                    limits = false;
                }
                _ => break,
            }
        }
        self.enter()?;
        let mut body = Vec::new();
        while !self.at_operator() {
            if !self.item(&mut body)? {
                break;
            }
        }
        self.leave();
        Ok(vec![Node::Nary { op, sub, sup, body, limits }])
    }

    /// Does the next token start a relation, binary operator or punctuation (where an n-ary
    /// operator's body ends)?
    fn at_operator(&self) -> bool {
        let class = match self.peek() {
            Some(Tok::Char(c)) => symbols::class_of(if *c == '-' { '−' } else { *c }),
            Some(Tok::Word(w)) => match symbols::symbol(w) {
                Some(c) => symbols::class_of(c),
                None => return false,
            },
            _ => return false,
        };
        matches!(class, Class::Rel | Class::Bin | Class::Punct)
    }

    /// `\sin^2 x`: the name takes the scripts, then one item is the argument.
    fn function(&mut self, mut name: Math) -> R<Math> {
        let (mut sub, mut sup) = (None, None);
        loop {
            match self.peek() {
                Some(Tok::Sub) if sub.is_none() => {
                    self.i += 1;
                    sub = Some(self.arg("_")?);
                }
                Some(Tok::Sup) if sup.is_none() => {
                    self.i += 1;
                    sup = Some(self.arg("^")?);
                }
                _ => break,
            }
        }
        if sub.is_some() || sup.is_some() {
            name = vec![Node::Script { base: name, sub, sup }];
        }
        let mut arg = Vec::new();
        if !self.at_operator() {
            self.enter()?;
            self.item(&mut arg)?;
            self.leave();
        }
        Ok(vec![Node::Func { name, arg }])
    }

    /// `\lim_{x \to 0} f(x)`: scripts under and over the name.
    fn limit_function(&mut self, name: &str) -> R<Math> {
        let mut base = vec![Node::styled(name, Style::Upright)];
        loop {
            match self.peek() {
                Some(Tok::Sub) => {
                    self.i += 1;
                    let lim = self.arg("_")?;
                    base = vec![Node::Lim { base, lim, above: false }];
                }
                Some(Tok::Sup) => {
                    self.i += 1;
                    let lim = self.arg("^")?;
                    base = vec![Node::Lim { base, lim, above: true }];
                }
                _ => break,
            }
        }
        let mut arg = Vec::new();
        if !self.at_operator() {
            self.enter()?;
            self.item(&mut arg)?;
            self.leave();
        }
        Ok(vec![Node::Func { name: base, arg }])
    }
}

/// A TeX length (`2em`, `10pt`, `1cm`) in em, roughly.
fn parse_length(s: &str) -> f64 {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, unit) = (s.get(..split).unwrap_or(""), s.get(split..).unwrap_or(""));
    let v: f64 = num.trim().parse().unwrap_or(0.0);
    let em = match unit.trim() {
        "em" | "" => v,
        "ex" => v * 0.45,
        "pt" => v / 10.0,
        "mm" => v * 2.835 / 10.0,
        "cm" => v * 28.35 / 10.0,
        "in" => v * 72.0 / 10.0,
        _ => v,
    };
    em.clamp(-5.0, 20.0)
}

fn restyle(m: &mut Math, style: Style) {
    for n in m.iter_mut() {
        match n {
            Node::Run { style: s, .. } => *s = style,
            other => {
                for c in children_mut(other) {
                    restyle(c, style);
                }
            }
        }
    }
}

fn alphabetize(m: &mut Math, a: Alphabet) {
    for n in m.iter_mut() {
        match n {
            Node::Run { text, style } => {
                *text = text.chars().map(|c| symbols::alphabet(a, c)).collect();
                *style = Style::Upright;
            }
            other => {
                for c in children_mut(other) {
                    alphabetize(c, a);
                }
            }
        }
    }
}

pub(crate) fn children_mut(n: &mut Node) -> Vec<&mut Math> {
    match n {
        Node::Run { .. } | Node::AlignMark | Node::Space(_) => vec![],
        Node::Frac { num, den, .. } => vec![num, den],
        Node::Script { base, sub, sup } => std::iter::once(base).chain(sub.iter_mut()).chain(sup.iter_mut()).collect(),
        Node::Rad { index, body } => index.iter_mut().chain(std::iter::once(body)).collect(),
        Node::Nary { sub, sup, body, .. } => sub.iter_mut().chain(sup.iter_mut()).chain(std::iter::once(body)).collect(),
        Node::Delim { items, .. } => items.iter_mut().collect(),
        Node::Acc { body, .. } | Node::Bar { body, .. } | Node::GroupChr { body, .. } | Node::Boxed { body } => vec![body],
        Node::Func { name, arg } => vec![name, arg],
        Node::Lim { base, lim, .. } => vec![base, lim],
        Node::Matrix { rows, .. } => rows.iter_mut().flatten().collect(),
        Node::EqArr { rows } => rows.iter_mut().collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// Writing

/// The equation as TeX, as [`parse`] reads it back.
pub fn to_tex(m: &Math) -> String {
    let mut out = String::new();
    write_row(m, &mut out, 0);
    tidy(&out)
}

fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            if prev_space && matches!(c, '}' | ')' | ']' | ',' | '^' | '_') && out.ends_with(' ') {
                out.pop();
            }
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn write_row(m: &Math, out: &mut String, depth: usize) {
    if depth > 100 {
        return;
    }
    for n in m {
        write_node(n, out, depth + 1);
    }
}

/// A row as an argument: braced unless it is one short run.
fn write_arg(m: &Math, out: &mut String, depth: usize) {
    out.push('{');
    write_row(m, out, depth);
    out.push('}');
}

/// A row as a script base: braced when it is more than one plain character.
fn write_base(m: &Math, out: &mut String, depth: usize) {
    match m.as_slice() {
        [n @ Node::Run { text, style }] if text.chars().count() == 1 && *style == Style::Math => write_node(n, out, depth),
        [n @ (Node::Frac { .. } | Node::Rad { .. } | Node::Delim { .. } | Node::Acc { .. } | Node::Bar { .. } | Node::Boxed { .. })] => {
            write_node(n, out, depth)
        }
        // `\sin^2`: the parser gives a function name its scripts.
        [n @ Node::Run { text, style: Style::Upright }] if symbols::FUNCTIONS.contains(&text.as_str()) => write_node(n, out, depth),
        _ => write_arg(m, out, depth),
    }
}

fn escape_text(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '{' | '}' | '\\' | '%' | '&' | '#' | '$' | '_' | '^' | '~' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

fn write_char(c: char, out: &mut String) {
    match c {
        '−' => out.push('-'),
        '∗' => out.push('*'),
        '{' | '}' | '%' | '&' | '#' | '$' | '_' => {
            out.push('\\');
            out.push(c);
        }
        '\\' => out.push_str("\\backslash "),
        '‖' => out.push_str("\\| "),
        c if c.is_ascii() => {
            // Relations read better with air around them.
            if symbols::class_of(c) == Class::Rel && c != ':' {
                out.push(' ');
                out.push(c);
                out.push(' ');
            } else {
                out.push(c);
            }
        }
        c => {
            if let Some(n) = symbols::nary_command(c) {
                out.push('\\');
                out.push_str(n);
                out.push(' ');
            } else if let Some(n) = symbols::command_for(c) {
                out.push('\\');
                out.push_str(n);
                out.push(' ');
            } else if let Some((a, l)) = symbols::plain_letter(c) {
                out.push('\\');
                out.push_str(a.command());
                out.push('{');
                out.push(l);
                out.push('}');
            } else if let Some(em) = symbols::space_width(c) {
                write_space(em, out);
            } else {
                out.push(c);
            }
        }
    }
}

fn write_space(em: f64, out: &mut String) {
    let s = if em < 0.0 {
        "\\!"
    } else if em < 0.2 {
        "\\,"
    } else if em < 0.26 {
        "\\:"
    } else if em < 0.3 {
        "\\;"
    } else if em < 0.75 {
        "\\ "
    } else if em < 1.5 {
        "\\quad"
    } else {
        "\\qquad"
    };
    out.push_str(s);
    out.push(' ');
}

fn is_primes(m: &Math) -> Option<usize> {
    match m.as_slice() {
        [Node::Run { text, .. }] if !text.is_empty() && text.chars().all(|c| matches!(c, '′' | '″' | '‴')) => Some(
            text.chars()
                .map(|c| match c {
                    '′' => 1,
                    '″' => 2,
                    _ => 3,
                })
                .sum(),
        ),
        _ => None,
    }
}

fn write_node(n: &Node, out: &mut String, depth: usize) {
    if depth > 100 {
        return;
    }
    match n {
        Node::Run { text, style } => write_run(text, *style, out),
        Node::Frac { num, den, bar } => write_frac(num, den, *bar, out, depth),
        Node::Script { base, sub, sup } => write_script(base, sub.as_ref(), sup.as_ref(), out, depth),
        Node::Rad { index, body } => write_rad(index.as_ref(), body, out, depth),
        Node::Nary { op, sub, sup, body, limits } => {
            write_nary_op(*op, *limits, out);
            write_limits(sub.as_ref(), sup.as_ref(), out, depth);
            out.push(' ');
            write_row(body, out, depth);
            out.push(' ');
        }
        Node::Delim { open, close, sep, items } => write_delim_node(*open, *close, *sep, items, out, depth),
        Node::Acc { chr, body } => {
            out.push('\\');
            out.push_str(symbols::accent_command(*chr).unwrap_or("hat"));
            write_arg(body, out, depth);
        }
        Node::Bar { body, top } => {
            out.push_str(if *top { "\\overline" } else { "\\underline" });
            write_arg(body, out, depth);
        }
        Node::Func { name, arg } => write_func(name, arg, out, depth),
        Node::Lim { base, lim, above } => write_lim(base, lim, *above, out, depth),
        Node::Matrix { rows, .. } => write_matrix("matrix", rows, out, depth),
        Node::EqArr { rows } => write_eqarr(rows, out, depth),
        Node::AlignMark => out.push_str(" & "),
        Node::GroupChr { body, top, .. } => {
            out.push_str(if *top { "\\overbrace" } else { "\\underbrace" });
            write_arg(body, out, depth);
        }
        Node::Boxed { body } => {
            out.push_str("\\boxed");
            write_arg(body, out, depth);
        }
        Node::Space(em) => write_space(*em, out),
    }
}

/// A run in its style: `\sin`, digits as they are, `\mathrm{…}`, `\mathbf{…}`, `\text{…}`.
fn write_run(text: &str, style: Style, out: &mut String) {
    match style {
        Style::Math => text.chars().for_each(|c| write_char(c, out)),
        Style::Upright => {
            if symbols::FUNCTIONS.contains(&text) || symbols::LIMIT_FUNCTIONS.contains(&text) {
                out.push('\\');
                out.push_str(text);
                out.push(' ');
            } else if text.chars().all(|c| c.is_ascii_digit() || matches!(c, '′' | '″' | '‴')) {
                text.chars().for_each(|c| write_char(c, out));
            } else {
                out.push_str("\\mathrm{");
                text.chars().for_each(|c| write_char(c, out));
                out.push('}');
            }
        }
        Style::Bold | Style::Italic | Style::BoldItalic => {
            out.push_str(match style {
                Style::Bold => "\\mathbf{",
                Style::Italic => "\\mathit{",
                _ => "\\boldsymbol{",
            });
            text.chars().for_each(|c| write_char(c, out));
            out.push('}');
        }
        Style::Text => {
            out.push_str("\\text{");
            out.push_str(&escape_text(text));
            out.push('}');
        }
    }
}

/// `\frac{num}{den}`, or `{num \atop den}` without the bar.
fn write_frac(num: &Math, den: &Math, bar: bool, out: &mut String, depth: usize) {
    if bar {
        out.push_str("\\frac");
        write_arg(num, out, depth);
        write_arg(den, out, depth);
    } else {
        out.push('{');
        write_row(num, out, depth);
        out.push_str(" \\atop ");
        write_row(den, out, depth);
        out.push('}');
    }
}

/// A base with its scripts; a superscript of primes is written as `'`s.
fn write_script(base: &Math, sub: Option<&Math>, sup: Option<&Math>, out: &mut String, depth: usize) {
    write_base(base, out, depth);
    if let Some(s) = sub {
        out.push('_');
        write_arg(s, out, depth);
    }
    if let Some(s) = sup {
        match is_primes(s) {
            Some(k) => out.push_str(&"'".repeat(k.min(8))),
            None => {
                out.push('^');
                write_arg(s, out, depth);
            }
        }
    }
}

/// `\sqrt[index]{body}`.
fn write_rad(index: Option<&Math>, body: &Math, out: &mut String, depth: usize) {
    out.push_str("\\sqrt");
    if let Some(i) = index {
        out.push('[');
        write_row(i, out, depth);
        out.push(']');
    }
    write_arg(body, out, depth);
}

/// An n-ary operator (`\sum`, or the character itself), with `\limits` or `\nolimits` when its
/// limits are not where the operator puts them by default.
fn write_nary_op(op: char, limits: bool, out: &mut String) {
    match symbols::nary_command(op) {
        Some(n) => {
            out.push('\\');
            out.push_str(n);
        }
        None => out.push(op),
    }
    if limits != symbols::nary_limits(op) {
        out.push_str(if limits { "\\limits" } else { "\\nolimits" });
    }
}

/// An n-ary operator's limits: `_{sub}^{sup}`.
fn write_limits(sub: Option<&Math>, sup: Option<&Math>, out: &mut String, depth: usize) {
    if let Some(s) = sub {
        out.push('_');
        write_arg(s, out, depth);
    }
    if let Some(s) = sup {
        out.push('^');
        write_arg(s, out, depth);
    }
}

/// Items between stretching delimiters: `\binom` for a bar-less fraction in parentheses, the
/// matrix environment (`pmatrix`, `cases` …) for a lone matrix in its delimiters, else
/// `\left … \middle … \right`.
fn write_delim_node(open: Option<char>, close: Option<char>, sep: Option<char>, items: &[Math], out: &mut String, depth: usize) {
    let env = match (open, close, items) {
        (Some('('), Some(')'), [m]) => match m.as_slice() {
            [Node::Frac { num, den, bar: false }] => {
                out.push_str("\\binom");
                write_arg(num, out, depth);
                write_arg(den, out, depth);
                return;
            }
            [Node::Matrix { left: false, .. }] => Some("pmatrix"),
            _ => None,
        },
        (Some('['), Some(']'), [m]) if matches!(m.as_slice(), [Node::Matrix { left: false, .. }]) => Some("bmatrix"),
        (Some('{'), Some('}'), [m]) if matches!(m.as_slice(), [Node::Matrix { left: false, .. }]) => Some("Bmatrix"),
        (Some('|'), Some('|'), [m]) if matches!(m.as_slice(), [Node::Matrix { left: false, .. }]) => Some("vmatrix"),
        (Some('‖'), Some('‖'), [m]) if matches!(m.as_slice(), [Node::Matrix { left: false, .. }]) => Some("Vmatrix"),
        (Some('{'), None, [m]) if matches!(m.as_slice(), [Node::Matrix { left: true, .. }]) => Some("cases"),
        _ => None,
    };
    if let (Some(env), Some(Node::Matrix { rows, .. })) = (env, items.first().and_then(|m| m.first())) {
        write_matrix(env, rows, out, depth);
        return;
    }
    out.push_str("\\left");
    write_delim(open, out);
    for (k, item) in items.iter().enumerate() {
        if k > 0 {
            out.push_str(" \\middle");
            write_delim(sep, out);
        }
        write_row(item, out, depth);
    }
    out.push_str(" \\right");
    write_delim(close, out);
    out.push(' ');
}

/// A function name and its argument, braced unless it is one letter, a delimited group, a
/// fraction or a script.
fn write_func(name: &Math, arg: &Math, out: &mut String, depth: usize) {
    write_row(name, out, depth);
    out.push(' ');
    match arg.as_slice() {
        [] => {}
        [Node::Run { text, style: Style::Math }] if text.chars().count() == 1 => write_row(arg, out, depth),
        [Node::Delim { .. } | Node::Frac { .. } | Node::Script { .. }] => write_row(arg, out, depth),
        _ => write_arg(arg, out, depth),
    }
    out.push(' ');
}

/// A limit over or under its base: `\lim_{…}`, `\overbrace{…}^{…}`, `\underset{…}{…}`.
fn write_lim(base: &Math, lim: &Math, above: bool, out: &mut String, depth: usize) {
    match base.as_slice() {
        [Node::Run { text, style: Style::Upright }] if symbols::LIMIT_FUNCTIONS.contains(&text.as_str()) => {
            out.push('\\');
            out.push_str(text);
            out.push(if above { '^' } else { '_' });
            write_arg(lim, out, depth);
        }
        [Node::Lim { base: inner, lim: l2, above: a2 }] if matches!(inner.as_slice(), [Node::Run { style: Style::Upright, .. }]) => {
            write_node(&Node::Lim { base: inner.clone(), lim: l2.clone(), above: *a2 }, out, depth);
            out.push(if above { '^' } else { '_' });
            write_arg(lim, out, depth);
        }
        [Node::GroupChr { body, top, .. }] => {
            out.push_str(if *top { "\\overbrace" } else { "\\underbrace" });
            write_arg(body, out, depth);
            out.push(if *top { '^' } else { '_' });
            write_arg(lim, out, depth);
        }
        _ => {
            out.push_str(if above { "\\overset" } else { "\\underset" });
            write_arg(lim, out, depth);
            write_arg(base, out, depth);
        }
    }
}

/// Lines of an equation array as an `aligned` environment.
fn write_eqarr(rows: &[Math], out: &mut String, depth: usize) {
    out.push_str("\\begin{aligned} ");
    for (k, r) in rows.iter().enumerate() {
        if k > 0 {
            out.push_str(" \\\\ ");
        }
        write_row(r, out, depth);
    }
    out.push_str(" \\end{aligned}");
}

fn write_delim(c: Option<char>, out: &mut String) {
    match c {
        None => out.push('.'),
        Some('{') => out.push_str("\\{"),
        Some('}') => out.push_str("\\}"),
        Some('‖') => out.push_str("\\|"),
        Some(c) if c.is_ascii() => out.push(c),
        Some(c) => match symbols::command_for(c) {
            Some(n) => {
                out.push('\\');
                out.push_str(n);
                out.push(' ');
            }
            None => out.push(c),
        },
    }
}

fn write_matrix(env: &str, rows: &[Vec<Math>], out: &mut String, depth: usize) {
    out.push_str("\\begin{");
    out.push_str(env);
    out.push_str("} ");
    for (ri, r) in rows.iter().enumerate() {
        if ri > 0 {
            out.push_str(" \\\\ ");
        }
        for (ci, c) in r.iter().enumerate() {
            if ci > 0 {
                out.push_str(" & ");
            }
            write_row(c, out, depth);
        }
    }
    out.push_str(" \\end{");
    out.push_str(env);
    out.push('}');
}
