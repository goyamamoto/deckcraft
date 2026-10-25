//! TeX-fashion layout of an equation tree over a font's metrics.
//!
//! The rules are The TeXbook's appendix G in outline: atoms spaced by class, scripts shifted by
//! the base's height and depth, fractions about the math axis with a minimum clearance from the
//! bar, radicals with a clearance under their overbar, large operators centred on the axis with
//! limits above and below in display style, delimiters stretched (the glyph scaled vertically)
//! to cover what they enclose, matrices with centred columns. Sizes step 100 % → 70 % → 50 %.
//!
//! The font is abstract ([`MathFont`]): the caller maps characters to glyphs in whatever faces
//! it has, and gets back glyph placements, rules (fraction bars, overlines) and filled paths
//! (stretched delimiters, radical signs, braces) in points, y down, origin at the left end of
//! the baseline.

use kurbo::{Affine, BezPath, Rect};

use crate::ast::{Math, Node, Style};
use crate::symbols::{self, Class};

/// What the layout asks a font for. Lengths are in em (fractions of the font size).
pub trait MathFont {
    /// The glyph drawing `c` bold and/or italic, or `None` when no face has it.
    fn glyph(&mut self, c: char, bold: bool, italic: bool) -> Option<Glyph>;
    /// The outline of `g` in em units, y down, origin at the pen position on the baseline.
    fn outline(&mut self, g: &Glyph) -> Option<BezPath>;
    /// Height of a lowercase x, in em.
    fn x_height(&self) -> f64 {
        0.45
    }
    /// Thickness of fraction bars and overlines, in em.
    fn rule_thickness(&self) -> f64 {
        0.05
    }
}

/// A glyph of some face, with its metrics in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// An opaque face handle the font gave out (an index into its faces).
    pub face: u32,
    pub id: u32,
    pub advance: f64,
    /// Ink bounds in em, y down (`y0` negative above the baseline); empty for a space.
    pub ink: Rect,
    /// The face lacks the weight or slant asked for: the drawing fakes it.
    pub fake_bold: bool,
    pub fake_italic: bool,
}

/// A glyph placed in points: pen position `(x, y)` on its baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub face: u32,
    pub id: u32,
    pub size: f64,
    pub x: f64,
    pub y: f64,
    pub fake_bold: bool,
    pub fake_italic: bool,
    /// The character drawn (text extraction, selection).
    pub ch: char,
}

/// A laid-out equation (or part of one): `width` along the baseline, `ascent` above it and
/// `descent` below (both ≥ 0), and what to draw, in points, y down, origin at the baseline's
/// left end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    /// Rightmost ink, for scripts after a slanted letter (≥ `width` only for overhangs).
    pub ink_right: f64,
    pub glyphs: Vec<PlacedGlyph>,
    /// Filled rectangles: fraction bars, overlines, box borders.
    pub rules: Vec<Rect>,
    /// Filled paths: stretched delimiters, radical signs, braces.
    pub paths: Vec<BezPath>,
}

impl Layout {
    pub fn height(&self) -> f64 {
        self.ascent + self.descent
    }

    /// The box, y down about the baseline.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, -self.ascent, self.width, self.descent)
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        for g in &mut self.glyphs {
            g.x += dx;
            g.y += dy;
        }
        for r in &mut self.rules {
            *r = Rect::new(r.x0 + dx, r.y0 + dy, r.x1 + dx, r.y1 + dy);
        }
        for p in &mut self.paths {
            p.apply_affine(Affine::translate((dx, dy)));
        }
    }

    /// Add `other` with its origin at `(dx, dy)`, growing this box to hold it.
    fn add(&mut self, mut other: Layout, dx: f64, dy: f64) {
        other.translate(dx, dy);
        self.glyphs.append(&mut other.glyphs);
        self.rules.append(&mut other.rules);
        self.paths.append(&mut other.paths);
        self.width = self.width.max(dx + other.width);
        self.ink_right = self.ink_right.max(dx + other.ink_right);
        self.ascent = self.ascent.max(other.ascent - dy);
        self.descent = self.descent.max(other.descent + dy);
    }

    fn with_rule(mut self, r: Rect) -> Layout {
        self.ascent = self.ascent.max(-r.y0);
        self.descent = self.descent.max(r.y1);
        self.width = self.width.max(r.x1);
        self.rules.push(r);
        self
    }
}

/// TeX's four styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Display,
    Text,
    Script,
    ScriptScript,
}

impl Level {
    fn factor(self) -> f64 {
        match self {
            Level::Display | Level::Text => 1.0,
            Level::Script => 0.7,
            Level::ScriptScript => 0.5,
        }
    }
    fn script(self) -> Level {
        match self {
            Level::Display | Level::Text => Level::Script,
            _ => Level::ScriptScript,
        }
    }
    fn frac(self) -> Level {
        match self {
            Level::Display => Level::Text,
            Level::Text => Level::Script,
            _ => Level::ScriptScript,
        }
    }
    /// Display or text: the styles with medium and thick spaces.
    fn big(self) -> bool {
        matches!(self, Level::Display | Level::Text)
    }
}

/// Math axis height in em: the centre of fraction bars, minus signs and delimiters.
const AXIS: f64 = 0.25;
const MAX_DEPTH: usize = 120;

/// An item of a row before spacing: an atom with its class, or an explicit kern.
enum Item {
    Atom { class: Class, lay: Layout },
    Kern(f64),
}

struct Cx<'a> {
    font: &'a mut dyn MathFont,
    /// Font size in points at display and text level.
    base: f64,
    depth: usize,
}

/// Lay out `m` at `size` points; `display` for an equation on its own line (limits above and
/// below large operators, bigger fraction clearances), text style for one in a line of text.
pub fn layout(m: &Math, font: &mut dyn MathFont, size: f64, display: bool) -> Layout {
    let size = if size.is_finite() && size > 0.0 { size.min(10_000.0) } else { 12.0 };
    if crate::ast::depth(m) > 100 {
        return Layout { width: size, ..Default::default() };
    }
    let mut cx = Cx { font, base: size, depth: 0 };
    cx.row(m, if display { Level::Display } else { Level::Text })
}

/// TeX's inter-atom spacing in eighteenths of an em; the second value says the space is only
/// taken in display and text styles.
fn spacing(a: Class, b: Class) -> (f64, bool) {
    use Class::*;
    match (a, b) {
        (Ord, Op) | (Op, Ord) | (Op, Op) | (Close, Op) | (Inner, Op) => (3.0, false),
        (Ord, Bin) | (Bin, Ord) | (Bin, Op) | (Bin, Open) | (Bin, Inner) | (Close, Bin) | (Inner, Bin) => (4.0, true),
        (Ord, Rel) | (Op, Rel) | (Rel, Ord) | (Rel, Op) | (Rel, Open) | (Rel, Inner) | (Close, Rel) | (Inner, Rel) => (5.0, true),
        (Ord, Inner) | (Op, Inner) | (Close, Inner) | (Inner, Inner) | (Inner, Ord) | (Inner, Open) | (Inner, Punct) => (3.0, true),
        (Punct, Bin) => (0.0, false),
        (Punct, _) => (3.0, true),
        _ => (0.0, false),
    }
}

impl Cx<'_> {
    fn em(&self, lvl: Level) -> f64 {
        self.base * lvl.factor()
    }

    fn theta(&self, lvl: Level) -> f64 {
        self.font.rule_thickness().clamp(0.01, 0.2) * self.em(lvl)
    }

    fn xh(&self, lvl: Level) -> f64 {
        self.font.x_height().clamp(0.2, 0.8) * self.em(lvl)
    }

    fn row(&mut self, m: &Math, lvl: Level) -> Layout {
        let items = self.items(m, lvl);
        self.place(items, lvl)
    }

    /// The items of a row, before spacing.
    fn items(&mut self, m: &Math, lvl: Level) -> Vec<Item> {
        let mut items = Vec::new();
        if self.depth > MAX_DEPTH {
            return items;
        }
        self.depth += 1;
        for n in m {
            self.node(n, lvl, &mut items);
        }
        self.depth -= 1;
        items
    }

    /// Items placed along the baseline with TeX's spaces between them.
    fn place(&mut self, mut items: Vec<Item>, lvl: Level) -> Layout {
        // A binary operator at the start or end of a row, or beside another operator, a relation
        // or an opening delimiter, is an ordinary symbol (the minus in −x).
        let classes: Vec<Option<Class>> = items.iter().map(|i| if let Item::Atom { class, .. } = i { Some(*class) } else { None }).collect();
        let atom_at = |k: usize, dir: isize| -> Option<Class> {
            let mut j = k as isize + dir;
            while j >= 0 && (j as usize) < classes.len() {
                if let Some(c) = classes.get(j as usize).copied().flatten() {
                    return Some(c);
                }
                j += dir;
            }
            None
        };
        for k in 0..items.len() {
            let Some(Item::Atom { class, .. }) = items.get_mut(k) else { continue };
            if *class != Class::Bin {
                continue;
            }
            let prev = atom_at(k, -1);
            let next = atom_at(k, 1);
            let ord_before = prev.is_none_or(|c| matches!(c, Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct));
            let ord_after = next.is_none_or(|c| matches!(c, Class::Rel | Class::Close | Class::Punct));
            if ord_before || ord_after {
                *class = Class::Ord;
            }
        }
        let em = self.em(lvl);
        let mut out = Layout::default();
        let mut x = 0.0f64;
        let mut prev: Option<Class> = None;
        for item in items {
            match item {
                Item::Kern(k) => {
                    x += k;
                    out.width = out.width.max(x);
                }
                Item::Atom { class, lay } => {
                    if let Some(p) = prev {
                        let (n, big_only) = spacing(p, class);
                        if !big_only || lvl.big() {
                            x += n / 18.0 * em;
                        }
                    }
                    let w = lay.width;
                    out.add(lay, x, 0.0);
                    x += w;
                    prev = Some(class);
                }
            }
        }
        out.width = out.width.max(x.max(0.0));
        out
    }

    fn glyph_layout(&mut self, c: char, bold: bool, italic: bool, size: f64) -> Layout {
        if let Some(w) = symbols::space_width(c) {
            return Layout { width: w * size, ..Default::default() };
        }
        let g = self
            .font
            .glyph(c, bold, italic)
            .or_else(|| self.font.glyph(c, false, false))
            .or_else(|| if c == '−' { self.font.glyph('-', bold, italic) } else { None });
        match g {
            Some(g) => Layout {
                width: g.advance.max(0.0) * size,
                ascent: (-g.ink.y0).max(0.0) * size,
                descent: g.ink.y1.max(0.0) * size,
                ink_right: g.ink.x1.max(g.advance) * size,
                glyphs: vec![PlacedGlyph { face: g.face, id: g.id, size, x: 0.0, y: 0.0, fake_bold: g.fake_bold, fake_italic: g.fake_italic, ch: c }],
                ..Default::default()
            },
            None => {
                // No face has the character: a small open box, as a missing glyph is shown.
                let (w, h, t) = (0.5 * size, 0.6 * size, 0.04 * size);
                Layout { width: w, ascent: h, descent: 0.0, ink_right: w, ..Default::default() }
                    .with_rule(Rect::new(0.0, -h, w, -h + t))
                    .with_rule(Rect::new(0.0, -t, w, 0.0))
                    .with_rule(Rect::new(0.0, -h, t, 0.0))
                    .with_rule(Rect::new(w - t, -h, w, 0.0))
            }
        }
    }

    fn node(&mut self, n: &Node, lvl: Level, items: &mut Vec<Item>) {
        let em = self.em(lvl);
        match n {
            Node::Run { text, style } => {
                if *style == Style::Text {
                    let mut lay = Layout::default();
                    let mut x = 0.0;
                    for c in text.chars() {
                        let g = self.glyph_layout(c, style.bold(), false, em);
                        let w = g.width;
                        lay.add(g, x, 0.0);
                        x += w;
                    }
                    lay.width = lay.width.max(x);
                    items.push(Item::Atom { class: Class::Ord, lay });
                    return;
                }
                for c in text.chars() {
                    if let Some(w) = symbols::space_width(c) {
                        items.push(Item::Kern(w * em));
                        continue;
                    }
                    let italic = if c.is_alphabetic() && !is_symbolic_letter(c) { style.italic_letters() } else { style.italic_others() };
                    let lay = self.glyph_layout(c, style.bold(), italic, em);
                    items.push(Item::Atom { class: symbols::class_of(c), lay });
                }
            }
            Node::Frac { num, den, bar } => {
                let lay = self.frac(num, den, *bar, lvl);
                items.push(Item::Atom { class: Class::Inner, lay });
            }
            Node::Script { base, sub, sup } => {
                let base_items = self.items(base, lvl);
                let class = match base_items.as_slice() {
                    [Item::Atom { class, .. }] => *class,
                    _ => Class::Ord,
                };
                let base_lay = self.place(base_items, lvl);
                let lay = self.scripts(base_lay, sub.as_ref(), sup.as_ref(), lvl);
                items.push(Item::Atom { class, lay });
            }
            Node::Rad { index, body } => {
                let lay = self.radical(index.as_ref(), body, lvl);
                items.push(Item::Atom { class: Class::Ord, lay });
            }
            Node::Nary { op, sub, sup, body, limits } => {
                let lay = self.nary(*op, sub.as_ref(), sup.as_ref(), *limits, lvl);
                items.push(Item::Atom { class: Class::Op, lay });
                let mut body_items = self.items(body, lvl);
                items.append(&mut body_items);
            }
            Node::Delim { open, close, sep, items: parts } => {
                let lay = self.delim(*open, *close, *sep, parts, lvl);
                items.push(Item::Atom { class: Class::Inner, lay });
            }
            Node::Acc { chr, body } => {
                let lay = self.accent(*chr, body, lvl);
                items.push(Item::Atom { class: Class::Ord, lay });
            }
            Node::Bar { body, top } => {
                let body_lay = self.row(body, lvl);
                let t = self.theta(lvl);
                let r = if *top {
                    Rect::new(0.0, -(body_lay.ascent + 2.0 * t), body_lay.width, -(body_lay.ascent + t))
                } else {
                    Rect::new(0.0, body_lay.descent + t, body_lay.width, body_lay.descent + 2.0 * t)
                };
                items.push(Item::Atom { class: Class::Ord, lay: body_lay.with_rule(r) });
            }
            Node::Func { name, arg } => {
                let lay = self.row(name, lvl);
                items.push(Item::Atom { class: Class::Op, lay });
                let mut arg_items = self.items(arg, lvl);
                items.append(&mut arg_items);
            }
            Node::Lim { base, lim, above } => {
                let base_lay = self.row(base, lvl);
                let lay = self.limit(base_lay, lim, *above, lvl);
                items.push(Item::Atom { class: Class::Op, lay });
            }
            Node::Matrix { rows, left } => {
                let lay = self.matrix(rows, *left, lvl);
                items.push(Item::Atom { class: Class::Inner, lay });
            }
            Node::EqArr { rows } => {
                let lay = self.eq_array(rows, lvl);
                items.push(Item::Atom { class: Class::Inner, lay });
            }
            Node::AlignMark => {}
            Node::GroupChr { body, top, .. } => {
                let lay = self.group_chr(body, *top, lvl);
                items.push(Item::Atom { class: Class::Ord, lay });
            }
            Node::Boxed { body } => {
                let inner = self.row(body, lvl);
                let (pad, t) = (0.2 * em, self.theta(lvl));
                let (w, top, bottom) = (inner.width + 2.0 * pad, -(inner.ascent + pad), inner.descent + pad);
                let mut lay = Layout::default();
                lay.add(inner, pad, 0.0);
                let lay = lay
                    .with_rule(Rect::new(0.0, top, w, top + t))
                    .with_rule(Rect::new(0.0, bottom - t, w, bottom))
                    .with_rule(Rect::new(0.0, top, t, bottom))
                    .with_rule(Rect::new(w - t, top, w, bottom));
                items.push(Item::Atom { class: Class::Ord, lay });
            }
            Node::Space(e) => items.push(Item::Kern(e.clamp(-5.0, 20.0) * em)),
        }
    }

    /// Superscript and subscript beside `base` (The TeXbook, rule 18).
    fn scripts(&mut self, base: Layout, sub: Option<&Math>, sup: Option<&Math>, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let s_lvl = lvl.script();
        let s_em = self.em(s_lvl);
        let xh = self.xh(lvl);
        let theta = self.theta(lvl);
        let x = base.ink_right.max(base.width) + 0.02 * em;
        let sup_lay = sup.map(|m| self.row(m, s_lvl));
        let sub_lay = sub.map(|m| self.row(m, s_lvl));
        let sup_shift = match lvl {
            Level::Display => 0.413,
            Level::Text => 0.363,
            _ => 0.289,
        } * em;
        let mut u = 0.0;
        if let Some(s) = &sup_lay {
            u = (base.ascent - 0.386 * s_em).max(sup_shift).max(s.descent + xh / 4.0);
        }
        let mut v = 0.0;
        if let Some(s) = &sub_lay {
            let sub_shift = if sup_lay.is_some() { 0.247 } else { 0.15 } * em;
            v = (base.descent + 0.1 * s_em).max(sub_shift).max(s.ascent - 0.8 * xh);
            if let Some(sp) = &sup_lay {
                let gap = (v - s.ascent) - (sp.descent - u);
                if gap < 4.0 * theta {
                    v += 4.0 * theta - gap;
                }
            }
        }
        let mut out = base;
        let mut w = 0.0f64;
        if let Some(s) = sup_lay {
            w = w.max(s.width);
            out.add(s, x, -u);
        }
        if let Some(s) = sub_lay {
            w = w.max(s.width);
            out.add(s, x, v);
        }
        out.width = (x + w + 0.05 * em).max(out.width);
        out.ink_right = out.ink_right.max(out.width);
        out
    }

    /// Numerator over denominator about the axis (rule 15).
    fn frac(&mut self, num: &Math, den: &Math, bar: bool, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let theta = if bar { self.theta(lvl) } else { 0.0 };
        let axis = AXIS * em;
        let f_lvl = lvl.frac();
        let num_lay = self.row(num, f_lvl);
        let den_lay = self.row(den, f_lvl);
        let (u0, v0, clearance) = match lvl {
            Level::Display => (0.677, 0.686, 3.0 * theta.max(0.03 * em)),
            Level::Text => (0.394, 0.345, theta.max(0.03 * em)),
            _ => (0.444, 0.345, theta.max(0.03 * em)),
        };
        let u = (u0 * em).max(axis + theta / 2.0 + clearance + num_lay.descent);
        let v = (v0 * em).max(-axis + theta / 2.0 + clearance + den_lay.ascent);
        let w = num_lay.width.max(den_lay.width);
        let pad = 0.12 * em;
        let mut out = Layout::default();
        let nx = pad + (w - num_lay.width) / 2.0;
        let dx = pad + (w - den_lay.width) / 2.0;
        out.add(num_lay, nx, -u);
        out.add(den_lay, dx, v);
        out.width = w + 2.0 * pad;
        out.ink_right = out.width;
        if bar {
            out = out.with_rule(Rect::new(pad * 0.5, -axis - theta / 2.0, w + pad * 1.5, -axis + theta / 2.0));
        }
        out
    }

    /// A radical sign stroked around the body, with its overbar (rule 11).
    fn radical(&mut self, index: Option<&Math>, body: &Math, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let theta = self.theta(lvl);
        let body_lay = self.row(body, lvl);
        let clearance = if lvl == Level::Display { theta + self.xh(lvl) / 4.0 } else { theta };
        let bar_top = -(body_lay.ascent + clearance + theta);
        let bottom = body_lay.descent + 0.06 * em;
        let h = bottom - bar_top;
        let ws = (0.42 * em + 0.1 * h).min(0.95 * em);
        // The index sits over the sign's tick, raised above its start.
        let idx = index.map(|i| self.row(i, Level::ScriptScript));
        let tick_y = bar_top + 0.6 * h;
        let mut x0 = 0.0f64;
        if let Some(i) = &idx {
            let ix = 0.58 * ws - i.width;
            if ix < 0.0 {
                x0 = -ix;
            }
        }
        let p0 = kurbo::Point::new(x0, tick_y);
        let p1 = kurbo::Point::new(x0 + 0.26 * ws, tick_y - 0.08 * h);
        let p2 = kurbo::Point::new(x0 + 0.54 * ws, bottom);
        let p3 = kurbo::Point::new(x0 + ws, bar_top + theta / 2.0);
        let mut out = Layout::default();
        out.paths.push(stroke(&[p0, p1], theta * 0.9));
        out.paths.push(stroke(&[p1, p2], theta * 1.7));
        out.paths.push(stroke(&[p2, p3], theta * 0.95));
        let bx = x0 + ws;
        out.add(body_lay, bx + 0.04 * em, 0.0);
        let bar = Rect::new(bx - theta * 0.4, bar_top, out.width + 0.08 * em, bar_top + theta);
        out = out.with_rule(bar);
        out.ascent = out.ascent.max(-bar_top);
        out.descent = out.descent.max(bottom);
        if let Some(i) = idx {
            let y = tick_y - 0.12 * em - i.descent;
            let ix = (x0 + 0.58 * ws - i.width).max(0.0);
            out.add(i, ix, y);
        }
        out.ink_right = out.width;
        out
    }

    /// A large operator on the axis with its limits (rule 13).
    fn nary(&mut self, op: char, sub: Option<&Math>, sup: Option<&Math>, limits: bool, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let axis = AXIS * em;
        let k = if lvl == Level::Display { 1.4 } else { 1.0 };
        let mut op_lay = self.glyph_layout(op, false, false, em * k);
        // Centre the operator's ink on the axis.
        let mid = (-op_lay.ascent + op_lay.descent) / 2.0;
        op_lay.translate(0.0, -axis - mid);
        let (asc, desc) = (op_lay.ascent + axis + mid, op_lay.descent - axis - mid);
        op_lay.ascent = asc.max(0.0);
        op_lay.descent = desc.max(0.0);
        if limits && lvl == Level::Display {
            let s_lvl = lvl.script();
            let sup_lay = sup.map(|m| self.row(m, s_lvl));
            let sub_lay = sub.map(|m| self.row(m, s_lvl));
            let gap = 0.15 * em;
            let w = op_lay.width.max(sup_lay.as_ref().map_or(0.0, |l| l.width)).max(sub_lay.as_ref().map_or(0.0, |l| l.width));
            let mut out = Layout::default();
            let (op_asc, op_desc, op_w) = (op_lay.ascent, op_lay.descent, op_lay.width);
            out.add(op_lay, (w - op_w) / 2.0, 0.0);
            if let Some(s) = sup_lay {
                let y = -op_asc - gap - s.descent;
                let sw = s.width;
                out.add(s, (w - sw) / 2.0, y);
            }
            if let Some(s) = sub_lay {
                let y = op_desc + gap + s.ascent;
                let sw = s.width;
                out.add(s, (w - sw) / 2.0, y);
            }
            out.width = w;
            out.ink_right = w;
            out
        } else {
            self.scripts(op_lay, sub, sup, lvl)
        }
    }

    /// Delimiters stretched to the content between them (rule 19).
    fn delim(&mut self, open: Option<char>, close: Option<char>, sep: Option<char>, parts: &[Math], lvl: Level) -> Layout {
        let em = self.em(lvl);
        let axis = AXIS * em;
        let mut items = Vec::new();
        for (k, part) in parts.iter().enumerate() {
            if k > 0 {
                let c = sep.unwrap_or('|');
                let lay = self.glyph_layout(c, false, false, em);
                items.push(Item::Atom { class: Class::Punct, lay });
            }
            let mut part_items = self.items(part, lvl);
            items.append(&mut part_items);
        }
        let content = self.place(items, lvl);
        let delta = (content.ascent - axis).max(content.descent + axis).max(0.0);
        let target = (2.0 * delta * 0.901).max(2.0 * delta - 0.5 * em);
        // Separators stretch with the delimiters.
        let mut seps: Vec<Layout> = Vec::new();
        let sep_count = parts.len().saturating_sub(1);
        for _ in 0..sep_count {
            seps.push(self.delimiter(sep.unwrap_or('|'), target, lvl));
        }
        // A little air inside the delimiters: slanted letters overhang their advance.
        let air = 0.06 * em;
        let mut out = Layout::default();
        let mut x = 0.0;
        if let Some(c) = open {
            let d = self.delimiter(c, target, lvl);
            x = d.width + air;
            out.add(d, 0.0, 0.0);
        }
        if sep_count > 0 {
            // Re-place the parts with stretched separators between them.
            let mut inner = Layout::default();
            let mut ix = 0.0;
            for (k, part) in parts.iter().enumerate() {
                if k > 0
                    && let Some(s) = seps.get(k - 1)
                {
                    let w = s.width;
                    inner.add(s.clone(), ix + 0.1 * em, 0.0);
                    ix += w + 0.2 * em;
                }
                let p = self.row(part, lvl);
                let w = p.width;
                inner.add(p, ix, 0.0);
                ix += w;
            }
            inner.width = inner.width.max(ix);
            let w = inner.width;
            out.add(inner, x, 0.0);
            x += w;
        } else {
            let w = content.width;
            out.add(content, x, 0.0);
            x += w;
        }
        if let Some(c) = close {
            let d = self.delimiter(c, target, lvl);
            let w = d.width;
            x += air;
            out.add(d, x, 0.0);
            x += w;
        }
        out.width = out.width.max(x);
        out.ink_right = out.width;
        out
    }

    /// One delimiter covering `target` points, centred on the axis: the glyph when it is tall
    /// enough, else its outline stretched vertically.
    fn delimiter(&mut self, c: char, target: f64, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let axis = AXIS * em;
        let Some(g) = self.font.glyph(c, false, false) else { return self.glyph_layout(c, false, false, em) };
        let ink_h = (g.ink.y1 - g.ink.y0).max(0.01) * em;
        let mid = (g.ink.y0 + g.ink.y1) / 2.0 * em;
        if target <= ink_h * 1.08 || !symbols::stretches(c) {
            let mut lay = self.glyph_layout(c, false, false, em);
            let dy = -axis - mid;
            lay.translate(0.0, dy);
            lay.ascent = ((g.ink.y1 - g.ink.y0) / 2.0 * em + axis).max(0.0);
            lay.descent = ((g.ink.y1 - g.ink.y0) / 2.0 * em - axis).max(0.0);
            return lay;
        }
        let sy = (target / ink_h).clamp(1.0, 8.0);
        let Some(mut path) = self.font.outline(&g) else { return self.glyph_layout(c, false, false, em) };
        // Scale to points, stretch about the ink's centre, centre on the axis.
        path.apply_affine(Affine::translate((0.0, -axis)) * Affine::scale_non_uniform(1.0, sy) * Affine::translate((0.0, -mid)) * Affine::scale(em));
        let half = ink_h * sy / 2.0;
        Layout {
            width: g.advance.max(0.0) * em,
            ascent: half + axis,
            descent: (half - axis).max(0.0),
            ink_right: g.ink.x1.max(g.advance) * em,
            paths: vec![path],
            ..Default::default()
        }
    }

    /// An accent centred over the body (rule 12).
    fn accent(&mut self, chr: char, body: &Math, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let body_lay = self.row(body, lvl);
        let (c, k) = symbols::accent_glyph(chr);
        let size = em * k;
        let Some(g) = self.font.glyph(c, false, false) else { return body_lay };
        let acc = self.glyph_layout(c, false, false, size);
        // Its ink just clears the body (a lowercase body counts as x-height tall).
        let body_top = body_lay.ascent.max(self.xh(lvl));
        let y = -body_top - 0.04 * em - g.ink.y1 * size;
        let ink_mid = (g.ink.x0 + g.ink.x1) / 2.0 * size;
        let x = body_lay.width / 2.0 - ink_mid;
        let mut out = body_lay;
        let w = out.width;
        out.add(acc, x, y);
        out.width = w;
        out
    }

    /// A limit under or over the base, centred (rule 13a).
    fn limit(&mut self, base: Layout, lim: &Math, above: bool, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let lim_lay = self.row(lim, lvl.script());
        let gap = 0.12 * em;
        let w = base.width.max(lim_lay.width);
        let mut out = Layout::default();
        let (bw, basc, bdesc) = (base.width, base.ascent, base.descent);
        out.add(base, (w - bw) / 2.0, 0.0);
        let y = if above { -basc - gap - lim_lay.descent } else { bdesc + gap + lim_lay.ascent };
        let lw = lim_lay.width;
        out.add(lim_lay, (w - lw) / 2.0, y);
        out.width = w;
        out.ink_right = w;
        out
    }

    /// Rows of cells, centred on the axis.
    fn matrix(&mut self, rows: &[Vec<Math>], left: bool, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let axis = AXIS * em;
        let c_lvl = if lvl == Level::Display { Level::Text } else { lvl };
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let cells: Vec<Vec<Layout>> = rows.iter().take(256).map(|r| r.iter().take(64).map(|c| self.row(c, c_lvl)).collect()).collect();
        let mut col_w = vec![0.0f64; cols];
        for r in &cells {
            for (k, c) in r.iter().enumerate() {
                if let Some(w) = col_w.get_mut(k) {
                    *w = w.max(c.width);
                }
            }
        }
        let col_gap = 0.8 * em;
        let row_gap = 0.3 * em;
        let mut out = Layout::default();
        let mut y = 0.0;
        for (ri, r) in cells.iter().enumerate() {
            let asc = r.iter().map(|c| c.ascent).fold(0.0, f64::max).max(0.55 * em);
            let desc = r.iter().map(|c| c.descent).fold(0.0, f64::max).max(0.15 * em);
            if ri > 0 {
                y += row_gap;
            }
            y += asc;
            let mut x = 0.0;
            for (ci, c) in r.iter().enumerate() {
                let cw = col_w.get(ci).copied().unwrap_or(c.width);
                let dx = if left { 0.0 } else { (cw - c.width) / 2.0 };
                out.add(c.clone(), x + dx, y);
                x += cw + col_gap;
            }
            y += desc;
        }
        let total = y;
        let width = col_w.iter().sum::<f64>() + col_gap * cols.saturating_sub(1) as f64;
        out.translate(0.0, -total / 2.0 - axis);
        out.ascent = total / 2.0 + axis;
        out.descent = (total / 2.0 - axis).max(0.0);
        out.width = width.max(0.0);
        out.ink_right = out.width;
        out
    }

    /// Stacked lines, aligned at their marks when they have them, centred otherwise.
    fn eq_array(&mut self, rows: &[Math], lvl: Level) -> Layout {
        let em = self.em(lvl);
        let axis = AXIS * em;
        let mut lines: Vec<(Layout, Option<Layout>)> = Vec::new();
        for r in rows.iter().take(256) {
            match r.iter().position(|n| matches!(n, Node::AlignMark)) {
                Some(p) => {
                    let left: Math = r.get(..p).unwrap_or_default().to_vec();
                    let right: Math = r.get(p + 1..).unwrap_or_default().iter().filter(|n| !matches!(n, Node::AlignMark)).cloned().collect();
                    lines.push((self.row(&left, lvl), Some(self.row(&right, lvl))));
                }
                None => lines.push((self.row(r, lvl), None)),
            }
        }
        let aligned = lines.iter().any(|(_, r)| r.is_some());
        let left_w = lines.iter().filter(|(_, r)| r.is_some()).map(|(l, _)| l.width).fold(0.0, f64::max);
        let total_w = if aligned {
            lines
                .iter()
                .map(|(l, r)| match r {
                    Some(r) => left_w + r.width,
                    None => l.width,
                })
                .fold(0.0, f64::max)
        } else {
            lines.iter().map(|(l, _)| l.width).fold(0.0, f64::max)
        };
        let row_gap = 0.45 * em;
        let mut out = Layout::default();
        let mut y = 0.0;
        for (ri, (l, r)) in lines.into_iter().enumerate() {
            let asc = l.ascent.max(r.as_ref().map_or(0.0, |r| r.ascent)).max(0.55 * em);
            let desc = l.descent.max(r.as_ref().map_or(0.0, |r| r.descent)).max(0.15 * em);
            if ri > 0 {
                y += row_gap;
            }
            y += asc;
            match r {
                Some(r) => {
                    let lw = l.width;
                    out.add(l, left_w - lw, y);
                    out.add(r, left_w, y);
                }
                None => {
                    let dx = if aligned { 0.0 } else { (total_w - l.width) / 2.0 };
                    out.add(l, dx, y);
                }
            }
            y += desc;
        }
        let total = y;
        out.translate(0.0, -total / 2.0 - axis);
        out.ascent = total / 2.0 + axis;
        out.descent = (total / 2.0 - axis).max(0.0);
        out.width = total_w.max(0.0);
        out.ink_right = out.width;
        out
    }

    /// A horizontal brace over or under the body.
    fn group_chr(&mut self, body: &Math, top: bool, lvl: Level) -> Layout {
        let em = self.em(lvl);
        let theta = self.theta(lvl);
        let body_lay = self.row(body, lvl);
        let w = body_lay.width.max(0.4 * em);
        let hb = 0.3 * em;
        let (near, tip) = if top {
            let near = -(body_lay.ascent + 0.08 * em);
            (near, near - hb)
        } else {
            let near = body_lay.descent + 0.08 * em;
            (near, near + hb)
        };
        let ym = (near + tip) / 2.0;
        let curl = (0.12 * w).min(0.3 * em);
        let mut p = BezPath::new();
        p.move_to((0.0, near));
        p.quad_to((0.0, ym), (curl, ym));
        p.line_to((w / 2.0 - curl, ym));
        p.quad_to((w / 2.0, ym), (w / 2.0, tip));
        p.quad_to((w / 2.0, ym), (w / 2.0 + curl, ym));
        p.line_to((w - curl, ym));
        p.quad_to((w, ym), (w, near));
        let path = kurbo::stroke(p, &kurbo::Stroke::new(theta * 1.2), &kurbo::StrokeOpts::default(), 0.05);
        let mut out = body_lay;
        out.paths.push(path);
        out.ascent = out.ascent.max(-tip.min(near));
        out.descent = out.descent.max(tip.max(near));
        out.width = out.width.max(w);
        out
    }
}

/// Letters that TeX never slants: Greek capitals and the like stay upright only by command, so
/// just the characters with no italic form.
fn is_symbolic_letter(c: char) -> bool {
    matches!(c, 'ℝ' | 'ℂ' | 'ℕ' | 'ℤ' | 'ℚ' | 'ℍ' | 'ℙ' | 'ℏ' | 'ℓ' | 'ℜ' | 'ℑ' | 'ℵ' | '∂' | '∞' | 'ı' | 'ȷ')
        || (0x1D400..=0x1D7FF).contains(&(c as u32))
}

/// A polyline stroked `width` wide, as a filled path.
fn stroke(points: &[kurbo::Point], width: f64) -> BezPath {
    let mut p = BezPath::new();
    for (i, pt) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*pt);
        } else {
            p.line_to(*pt);
        }
    }
    let style = kurbo::Stroke::new(width.max(0.05)).with_caps(kurbo::Cap::Butt).with_join(kurbo::Join::Miter);
    kurbo::stroke(p, &style, &kurbo::StrokeOpts::default(), 0.05)
}
