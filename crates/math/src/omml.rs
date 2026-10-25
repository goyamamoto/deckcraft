//! Office Math (OMML, ECMA-376 Part 1 §22.1) ⇄ equation tree.
//!
//! [`write`] produces the `m:oMathPara` a PresentationML `a14:m` element holds; [`read`] takes
//! any OMML (an `m:oMathPara`, an `m:oMath`, or either inside wrappers) and never fails on bad
//! input: it gives back what it understood, or `None` for XML it cannot parse at all.

use quick_xml::events::Event;

use crate::ast::{Math, Node, Style};
use crate::symbols;

pub const NS_M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

/// The equation as a display equation: `<m:oMathPara xmlns:m="…"><m:oMath>…</m:oMath></m:oMathPara>`.
pub fn write(m: &Math) -> String {
    write_runs(m, "", true)
}

/// [`write`] with `run_props` (XML, e.g. a DrawingML `a:rPr` naming the math font, as PowerPoint
/// writes in every `m:r`) inside each run, after its `m:rPr` and before its text. `display`
/// wraps the equation in `m:oMathPara` (a display equation: large operators with their limits
/// above and below, display-style fractions); without it the equation is a bare `m:oMath`, an
/// inline one set in text style, as Word tells the two apart.
pub fn write_runs(m: &Math, run_props: &str, display: bool) -> String {
    let mut out = String::with_capacity(256);
    if display {
        out.push_str(&format!("<m:oMathPara xmlns:m=\"{NS_M}\"><m:oMath>"));
    } else {
        out.push_str(&format!("<m:oMath xmlns:m=\"{NS_M}\">"));
    }
    Writer { out: &mut out, run_props }.row(m, 0);
    out.push_str(if display { "</m:oMath></m:oMathPara>" } else { "</m:oMath>" });
    out
}

/// Whether `omml` is a display equation (an `m:oMathPara`) rather than an inline `m:oMath`.
pub fn is_display(omml: &str) -> bool {
    omml.contains("oMathPara")
}

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

struct Writer<'a> {
    out: &'a mut String,
    run_props: &'a str,
}

impl Writer<'_> {
    fn push(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// `<m:tag m:val="v"/>`
    fn val(&mut self, tag: &str, v: &str) {
        self.push("<m:");
        self.push(tag);
        self.push(" m:val=\"");
        esc(v, self.out);
        self.push("\"/>");
    }

    /// `<m:tag>…row…</m:tag>`
    fn tagged(&mut self, tag: &str, m: &Math, depth: usize) {
        self.push("<m:");
        self.push(tag);
        self.push(">");
        self.row(m, depth);
        self.push("</m:");
        self.push(tag);
        self.push(">");
    }

    fn row(&mut self, m: &Math, depth: usize) {
        if depth > 100 {
            return;
        }
        for n in m {
            self.node(n, depth + 1);
        }
    }

    fn text_run(&mut self, text: &str, props: &str) {
        self.push("<m:r>");
        self.push(props);
        let rp = self.run_props;
        self.push(rp);
        if text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") {
            self.push("<m:t xml:space=\"preserve\">");
        } else {
            self.push("<m:t>");
        }
        esc(text, self.out);
        self.push("</m:t></m:r>");
    }

    fn node(&mut self, n: &Node, depth: usize) {
        match n {
            Node::Run { text, style } => {
                let props = match style {
                    Style::Math => "",
                    Style::Upright => "<m:rPr><m:sty m:val=\"p\"/></m:rPr>",
                    Style::Bold => "<m:rPr><m:sty m:val=\"b\"/></m:rPr>",
                    Style::Italic => "<m:rPr><m:sty m:val=\"i\"/></m:rPr>",
                    Style::BoldItalic => "<m:rPr><m:sty m:val=\"bi\"/></m:rPr>",
                    Style::Text => "<m:rPr><m:nor/></m:rPr>",
                };
                self.text_run(text, props);
            }
            Node::Frac { num, den, bar } => {
                self.push("<m:f>");
                if !bar {
                    self.push("<m:fPr><m:type m:val=\"noBar\"/></m:fPr>");
                }
                self.tagged("num", num, depth);
                self.tagged("den", den, depth);
                self.push("</m:f>");
            }
            Node::Script { base, sub, sup } => match (sub, sup) {
                (Some(sb), Some(sp)) => {
                    self.push("<m:sSubSup>");
                    self.tagged("e", base, depth);
                    self.tagged("sub", sb, depth);
                    self.tagged("sup", sp, depth);
                    self.push("</m:sSubSup>");
                }
                (Some(sb), None) => {
                    self.push("<m:sSub>");
                    self.tagged("e", base, depth);
                    self.tagged("sub", sb, depth);
                    self.push("</m:sSub>");
                }
                (None, Some(sp)) => {
                    self.push("<m:sSup>");
                    self.tagged("e", base, depth);
                    self.tagged("sup", sp, depth);
                    self.push("</m:sSup>");
                }
                (None, None) => self.row(base, depth),
            },
            Node::Rad { index, body } => {
                self.push("<m:rad>");
                match index {
                    Some(i) => self.tagged("deg", i, depth),
                    None => self.push("<m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/>"),
                }
                self.tagged("e", body, depth);
                self.push("</m:rad>");
            }
            Node::Nary { op, sub, sup, body, limits } => {
                self.push("<m:nary><m:naryPr>");
                self.val("chr", &op.to_string());
                self.val("limLoc", if *limits { "undOvr" } else { "subSup" });
                if sub.is_none() {
                    self.val("subHide", "1");
                }
                if sup.is_none() {
                    self.val("supHide", "1");
                }
                self.push("</m:naryPr>");
                let empty = Vec::new();
                self.tagged("sub", sub.as_ref().unwrap_or(&empty), depth);
                self.tagged("sup", sup.as_ref().unwrap_or(&empty), depth);
                self.tagged("e", body, depth);
                self.push("</m:nary>");
            }
            Node::Delim { open, close, sep, items } => {
                self.push("<m:d><m:dPr>");
                self.val("begChr", &open.map(String::from).unwrap_or_default());
                if items.len() > 1 || sep.is_some() {
                    self.val("sepChr", &sep.map(String::from).unwrap_or_default());
                }
                self.val("endChr", &close.map(String::from).unwrap_or_default());
                self.push("</m:dPr>");
                if items.is_empty() {
                    self.push("<m:e/>");
                }
                for item in items {
                    self.tagged("e", item, depth);
                }
                self.push("</m:d>");
            }
            Node::Acc { chr, body } => {
                self.push("<m:acc><m:accPr>");
                self.val("chr", &chr.to_string());
                self.push("</m:accPr>");
                self.tagged("e", body, depth);
                self.push("</m:acc>");
            }
            Node::Bar { body, top } => {
                self.push("<m:bar><m:barPr>");
                self.val("pos", if *top { "top" } else { "bot" });
                self.push("</m:barPr>");
                self.tagged("e", body, depth);
                self.push("</m:bar>");
            }
            Node::Func { name, arg } => {
                self.push("<m:func>");
                self.tagged("fName", name, depth);
                self.tagged("e", arg, depth);
                self.push("</m:func>");
            }
            Node::Lim { base, lim, above } => {
                let tag = if *above { "m:limUpp" } else { "m:limLow" };
                self.push("<");
                self.push(tag);
                self.push(">");
                self.tagged("e", base, depth);
                self.tagged("lim", lim, depth);
                self.push("</");
                self.push(tag);
                self.push(">");
            }
            Node::Matrix { rows, left } => {
                let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                self.push("<m:m><m:mPr><m:mcs><m:mc><m:mcPr>");
                self.val("count", &cols.to_string());
                self.val("mcJc", if *left { "left" } else { "center" });
                self.push("</m:mcPr></m:mc></m:mcs></m:mPr>");
                for r in rows {
                    self.push("<m:mr>");
                    for c in 0..cols {
                        match r.get(c) {
                            Some(cell) => self.tagged("e", cell, depth),
                            None => self.push("<m:e/>"),
                        }
                    }
                    self.push("</m:mr>");
                }
                self.push("</m:m>");
            }
            Node::EqArr { rows } => {
                self.push("<m:eqArr>");
                if rows.is_empty() {
                    self.push("<m:e/>");
                }
                for r in rows {
                    self.tagged("e", r, depth);
                }
                self.push("</m:eqArr>");
            }
            // Word's alignment point inside an equation array row.
            Node::AlignMark => self.text_run("&", ""),
            Node::GroupChr { chr, body, top } => {
                self.push("<m:groupChr><m:groupChrPr>");
                self.val("chr", &chr.to_string());
                if *top {
                    self.val("pos", "top");
                    self.val("vertJc", "bot");
                }
                self.push("</m:groupChrPr>");
                self.tagged("e", body, depth);
                self.push("</m:groupChr>");
            }
            Node::Boxed { body } => {
                self.push("<m:borderBox>");
                self.tagged("e", body, depth);
                self.push("</m:borderBox>");
            }
            Node::Space(em) => {
                if let Some(sp) = symbols::space_char(*em) {
                    self.text_run(sp, "");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reading

/// A parsed element: local name (prefix dropped), its `m:val`, children and text.
#[derive(Debug, Default)]
struct El {
    name: String,
    val: Option<String>,
    kids: Vec<El>,
    text: String,
}

const MAX_NODES: usize = 50_000;
const MAX_DEPTH: usize = 128;

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

/// Parse `xml` into a tree; `None` when it is not well formed.
fn parse_xml(xml: &str) -> Option<El> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = true;
    let mut stack: Vec<El> = vec![El { name: "#root".into(), ..Default::default() }];
    let mut count = 0usize;
    loop {
        let ev = reader.read_event().ok()?;
        match ev {
            Event::Start(e) => {
                count += 1;
                if count > MAX_NODES || stack.len() > MAX_DEPTH {
                    return None;
                }
                let mut el = El { name: local(e.name().as_ref()), ..Default::default() };
                for a in e.attributes().flatten() {
                    if local(a.key.as_ref()) == "val" {
                        el.val = Some(a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map(|v| v.to_string()).unwrap_or_default());
                    }
                }
                stack.push(el);
            }
            Event::End(_) => {
                let el = stack.pop()?;
                stack.last_mut()?.kids.push(el);
            }
            Event::Text(t) => {
                let text = t.xml_content(quick_xml::XmlVersion::Implicit1_0).map(|c| c.to_string()).unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&text);
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&c));
                }
            }
            // `&amp;`, `&#x2211;`: entity and character references come as events of their own.
            Event::GeneralRef(r) => {
                let name = String::from_utf8_lossy(&r).to_string();
                let c = match name.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => r.resolve_char_ref().ok().flatten(),
                };
                if let (Some(c), Some(top)) = (c, stack.last_mut()) {
                    top.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    (stack.len() == 1).then(|| stack.pop()).flatten()
}

impl El {
    fn child(&self, name: &str) -> Option<&El> {
        self.kids.iter().find(|k| k.name == name)
    }
    fn children(&self, name: &str) -> impl Iterator<Item = &El> {
        self.kids.iter().filter(move |k| k.name == name)
    }
    /// `<m:x>` → `<m:xPr>` → `<m:name m:val="…"/>`.
    fn prop(&self, pr: &str, name: &str) -> Option<&str> {
        self.child(pr)?.child(name)?.val.as_deref()
    }
    fn prop_on(&self, pr: &str, name: &str) -> bool {
        match self.child(pr).and_then(|p| p.child(name)) {
            Some(e) => e.val.as_deref().is_none_or(|v| !matches!(v, "0" | "false" | "off")),
            None => false,
        }
    }
    fn all_text(&self, out: &mut String, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        out.push_str(&self.text);
        for k in &self.kids {
            k.all_text(out, depth + 1);
        }
    }
}

/// Read OMML. `None` when it is not XML; a tree missing what was not understood otherwise.
pub fn read(xml: &str) -> Option<Math> {
    let root = parse_xml(xml)?;
    let mut m = Vec::new();
    read_children(&root, &mut m, 0, false);
    Some(m)
}

fn read_children(el: &El, out: &mut Math, depth: usize, in_eqarr: bool) {
    if depth > MAX_DEPTH {
        return;
    }
    for k in &el.kids {
        read_el(k, out, depth + 1, in_eqarr);
    }
}

fn read_row(el: Option<&El>, depth: usize) -> Math {
    let mut m = Vec::new();
    if let Some(e) = el {
        read_children(e, &mut m, depth, false);
    }
    m
}

fn opt_row(el: Option<&El>, depth: usize) -> Option<Math> {
    let m = read_row(el, depth);
    (!m.is_empty()).then_some(m)
}

fn first_char(s: Option<&str>) -> Option<char> {
    s.and_then(|v| v.chars().next())
}

fn read_el(el: &El, out: &mut Math, depth: usize, in_eqarr: bool) {
    if depth > MAX_DEPTH {
        return;
    }
    match el.name.as_str() {
        "r" => {
            let mut text = String::new();
            for t in el.children("t") {
                t.all_text(&mut text, depth);
            }
            if text.is_empty() {
                return;
            }
            let style = if el.child("rPr").is_some_and(|p| p.child("nor").is_some()) {
                Style::Text
            } else {
                match el.prop("rPr", "sty") {
                    Some("p") => Style::Upright,
                    Some("b") => Style::Bold,
                    Some("i") => Style::Italic,
                    Some("bi") => Style::BoldItalic,
                    _ => Style::Math,
                }
            };
            if in_eqarr && style != Style::Text && text.contains('&') {
                for (k, part) in text.split('&').enumerate() {
                    if k > 0 {
                        out.push(Node::AlignMark);
                    }
                    if !part.is_empty() {
                        push_run(part, style, out);
                    }
                }
            } else {
                push_run(&text, style, out);
            }
        }
        "f" => {
            let num = read_row(el.child("num"), depth);
            let den = read_row(el.child("den"), depth);
            match el.prop("fPr", "type") {
                Some("lin") | Some("skw") => {
                    out.extend(num);
                    out.push(Node::run("/"));
                    out.extend(den);
                }
                t => out.push(Node::Frac { num, den, bar: t != Some("noBar") }),
            }
        }
        "sSup" => out.push(Node::Script { base: read_row(el.child("e"), depth), sub: None, sup: Some(read_row(el.child("sup"), depth)) }),
        "sSub" => out.push(Node::Script { base: read_row(el.child("e"), depth), sub: Some(read_row(el.child("sub"), depth)), sup: None }),
        "sSubSup" => out.push(Node::Script {
            base: read_row(el.child("e"), depth),
            sub: Some(read_row(el.child("sub"), depth)),
            sup: Some(read_row(el.child("sup"), depth)),
        }),
        "sPre" => {
            // Prescripts: drawn as scripts on an empty base before the base.
            out.push(Node::Script { base: vec![], sub: opt_row(el.child("sub"), depth), sup: opt_row(el.child("sup"), depth) });
            out.extend(read_row(el.child("e"), depth));
        }
        "rad" => {
            let index = if el.prop_on("radPr", "degHide") { None } else { opt_row(el.child("deg"), depth) };
            out.push(Node::Rad { index, body: read_row(el.child("e"), depth) });
        }
        "nary" => {
            let op = first_char(el.prop("naryPr", "chr")).unwrap_or('∫');
            let limits = match el.prop("naryPr", "limLoc") {
                Some("undOvr") => true,
                Some("subSup") => false,
                _ => symbols::nary_limits(op),
            };
            let sub = if el.prop_on("naryPr", "subHide") { None } else { opt_row(el.child("sub"), depth) };
            let sup = if el.prop_on("naryPr", "supHide") { None } else { opt_row(el.child("sup"), depth) };
            out.push(Node::Nary { op, sub, sup, body: read_row(el.child("e"), depth), limits });
        }
        "d" => {
            let open = match el.prop("dPr", "begChr") {
                Some(v) => v.chars().next(),
                None => Some('('),
            };
            let close = match el.prop("dPr", "endChr") {
                Some(v) => v.chars().next(),
                None => Some(')'),
            };
            let items: Vec<Math> = el.children("e").map(|e| read_row(Some(e), depth)).collect();
            let sep = match el.prop("dPr", "sepChr") {
                Some(v) => v.chars().next(),
                None if items.len() > 1 => Some('|'),
                None => None,
            };
            out.push(Node::Delim { open, close, sep, items });
        }
        "acc" => {
            let chr = first_char(el.prop("accPr", "chr")).unwrap_or('\u{302}');
            out.push(Node::Acc { chr, body: read_row(el.child("e"), depth) });
        }
        "bar" => {
            let top = el.prop("barPr", "pos") == Some("top");
            out.push(Node::Bar { body: read_row(el.child("e"), depth), top });
        }
        "func" => out.push(Node::Func { name: read_row(el.child("fName"), depth), arg: read_row(el.child("e"), depth) }),
        "limLow" => out.push(Node::Lim { base: read_row(el.child("e"), depth), lim: read_row(el.child("lim"), depth), above: false }),
        "limUpp" => out.push(Node::Lim { base: read_row(el.child("e"), depth), lim: read_row(el.child("lim"), depth), above: true }),
        "m" => {
            let left = el
                .child("mPr")
                .and_then(|p| p.child("mcs"))
                .map(|mcs| mcs.children("mc").any(|mc| mc.prop("mcPr", "mcJc") == Some("left")))
                .unwrap_or(false);
            let rows: Vec<Vec<Math>> = el.children("mr").map(|r| r.children("e").map(|e| read_row(Some(e), depth)).collect()).collect();
            out.push(Node::Matrix { rows, left });
        }
        "eqArr" => {
            let rows: Vec<Math> = el
                .children("e")
                .map(|e| {
                    let mut m = Vec::new();
                    read_children(e, &mut m, depth, true);
                    m
                })
                .collect();
            out.push(Node::EqArr { rows });
        }
        "groupChr" => {
            let top = el.prop("groupChrPr", "pos") == Some("top");
            let chr = first_char(el.prop("groupChrPr", "chr")).unwrap_or(if top { '⏞' } else { '⏟' });
            out.push(Node::GroupChr { chr, body: read_row(el.child("e"), depth), top });
        }
        "borderBox" => out.push(Node::Boxed { body: read_row(el.child("e"), depth) }),
        "box" | "phant" => out.extend(read_row(el.child("e"), depth)),
        // Properties and anything unknown: look inside for content (`m:oMathPara`, `m:oMath`,
        // `mc:AlternateContent` wrappers, a `w:p`…).
        name if name.ends_with("Pr") => {}
        _ => read_children(el, out, depth, in_eqarr),
    }
}

/// A run's text, with spaces that mean kerns taken out as [`Node::Space`].
fn push_run(text: &str, style: Style, out: &mut Math) {
    if style == Style::Text {
        out.push(Node::Run { text: text.to_string(), style });
        return;
    }
    let mut cur = String::new();
    let mut space = 0.0;
    for c in text.chars() {
        match symbols::space_width(c) {
            Some(w) => {
                if !cur.is_empty() {
                    out.push(Node::Run { text: std::mem::take(&mut cur), style });
                }
                space += w;
            }
            None => {
                if space > 0.0 {
                    out.push(Node::Space(space));
                    space = 0.0;
                }
                cur.push(c);
            }
        }
    }
    if !cur.is_empty() {
        out.push(Node::Run { text: cur, style });
    }
    if space > 0.0 {
        out.push(Node::Space(space));
    }
}
