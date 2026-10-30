use super::breaking::{
    BreakParams, KinsokuLists, LINE_PITCH, RawLine, Tab, break_lines, break_rules, is_hanging_punct, line_metrics, measure_str, tab_advance,
    tab_segment, tabs_of,
};
use super::run::{Deco, DecoLine, GlyphRun, RunEffects};
use super::shape::{SUB_SUPER, Upright, shape_para};
use super::style::{Cell, MathFaces, MathObject, Opts, ParaShaped, Style, spacing_pts, synthetic_style};
use super::{
    Affine, Align, Anchor, Arc, AutoFit, BodyProps, Bullet, Caps, Ctx, FontDb, FontFace, LineInfo, Rect, Rgba, Shape, Spacing, Strike, TextBody,
    TextDir, TextLayout, TextPath, UPRIGHT_CENTRE, bidi, joins_following, joins_preceding, math, resolve,
};

#[cfg(test)]
thread_local! {
    /// How many times `layout_scaled` ran on this thread (tests count autofit's passes).
    pub(crate) static LAYOUT_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Lay out a shape's text body.
pub fn layout(ctx: &Ctx, shape: &Shape, body: &TextBody, opts: &Opts) -> TextLayout {
    let bp = resolve::body(ctx, shape);
    let autofit = bp.autofit.unwrap_or(AutoFit::None);
    // Shrink on overflow starts from the scale the file was saved with (`fontScale`,
    // `lnSpcReduction`), which is what PowerPoint draws; editing the text resets it.
    let (scale0, reduce0, no_saved_scale) = match (autofit, opts.no_shrink) {
        (AutoFit::Shrink { font_scale, line_reduction }, false) => (
            font_scale.filter(|s| s.is_finite()).map(|s| s.clamp(0.1, 1.0)).unwrap_or(1.0),
            if line_reduction.is_finite() { line_reduction.clamp(0.0, 0.2) } else { 0.0 },
            font_scale.is_none(),
        ),
        _ => (1.0, 0.0, false),
    };
    let mut l = layout_scaled(ctx, shape, body, &bp, opts, scale0, reduce0);
    // A scale PowerPoint saved is drawn as it is: PowerPoint fits the text again only when it
    // changes (and editing resets the saved scale), not when a file opens, even where our
    // metrics would call it overflowing (trailing empty paragraphs, substituted fonts).
    // A body whose file has no `fontScale` at all (`<a:normAutofit/>`) is the schema default
    // 100 % and is drawn as it is too, not refitted on open (#724).
    let saved = no_saved_scale || scale0 < 1.0 || reduce0 > 0.0;
    if let (AutoFit::Shrink { .. }, false) = (autofit, opts.no_shrink)
        && l.overflow
        && !saved
    {
        // Text that overflows even at the smallest scale gets that scale: every larger one
        // overflows too, so searching would only lay it out a dozen more times (seconds for a
        // body of tens of thousands of paragraphs, on every redraw of the slide).
        let smallest = layout_scaled(ctx, shape, body, &bp, opts, 0.1, 0.2);
        if smallest.overflow {
            return smallest;
        }
        // Binary search the largest scale that fits (steps like 90%, 80%… with line reduction).
        let (mut lo, mut hi) = (0.1_f64, scale0);
        let mut best = Some(smallest);
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            let red: f64 = ((1.0 - mid) * 0.8_f64).min(0.2).max(reduce0);
            let t = layout_scaled(ctx, shape, body, &bp, opts, mid, red);
            if t.overflow {
                hi = mid;
            } else {
                lo = mid;
                best = Some(t);
            }
        }
        if let Some(b) = best {
            l = b;
        }
    }
    l
}

/// How a body's text is turned about the shape centre, in degrees (90 for `vert`, `eaVert`,
/// `mongolianVert` and stacked text, 270 for `vert270`, plus `bodyPr/@rot`), and whether its lines
/// then follow each other from left to right: Mongolian vertical and stacked (`wordArtVert`) text
/// is turned like `vert`, but PowerPoint draws its lines from left to right.
fn text_turn(bp: &BodyProps) -> (f64, bool) {
    let vert = bp.vert.unwrap_or(TextDir::Horizontal);
    let lines_ltr = matches!(vert, TextDir::MongolianVertical | TextDir::Stacked);
    let rotation = match vert {
        TextDir::Vertical | TextDir::EaVertical | TextDir::Stacked | TextDir::StackedRtl | TextDir::MongolianVertical => 90.0,
        TextDir::Vertical270 => 270.0,
        TextDir::Horizontal => 0.0,
    } + bp.rot.unwrap_or(0.0);
    (rotation, lines_ltr)
}

/// A turn of 90° or 270°: the text's lines run down the shape.
fn quarter_turned(rotation: f64) -> bool {
    (rotation - 90.0).abs() < 1e-6 || (rotation - 270.0).abs() < 1e-6
}

/// Where the lines of text turned a quarter turn follow each other across the shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalLines {
    /// The first line on the right, the next to its left (`vert`, `eaVert`, `wordArtVertRtl`).
    RightToLeft,
    /// The first line on the left (`vert270`, `mongolianVert`, `wordArtVert`).
    LeftToRight,
}

/// How the lines of body `bp` run when its text is turned a quarter turn, so that each line runs
/// down the shape; `None` when they run across it. A turned body's line length sets the shape's
/// height and its lines' stacking its width, the other way round from horizontal text.
pub fn vertical_lines(bp: &BodyProps) -> Option<VerticalLines> {
    let (rotation, lines_ltr) = text_turn(bp);
    if !quarter_turned(rotation) {
        return None;
    }
    // A 270° turn puts the first line on the left; mirrored lines swap sides.
    let first_left = ((rotation - 270.0).abs() < 1e-6) != lines_ltr;
    Some(if first_left { VerticalLines::LeftToRight } else { VerticalLines::RightToLeft })
}

/// Geometry of the text box shared by every line: rotation, inner rectangle, wrapping and columns.
struct BoxGeom {
    vert: TextDir,
    rotation: f64,
    inner: Rect,
    wrap: bool,
    ncols: usize,
    col_gap: f64,
    col_w: f64,
    rtl_cols: bool,
}

impl BoxGeom {
    fn new(shape: &Shape, bp: &BodyProps, text_rect: Rect) -> Self {
        let vert = bp.vert.unwrap_or(TextDir::Horizontal);
        let (rotation, _) = text_turn(bp);
        // For vertical text, lay out in a box with swapped dimensions centred on the text rect.
        let r = if quarter_turned(rotation) {
            let c = text_rect.center();
            Rect::new(c.x - text_rect.height() / 2.0, c.y - text_rect.width() / 2.0, c.x + text_rect.height() / 2.0, c.y + text_rect.width() / 2.0)
        } else {
            text_rect
        };
        let inner = Rect::new(
            r.x0 + bp.inset_l.unwrap_or(7.2),
            r.y0 + bp.inset_t.unwrap_or(3.6),
            (r.x1 - bp.inset_r.unwrap_or(7.2)).max(r.x0 + bp.inset_l.unwrap_or(7.2)),
            (r.y1 - bp.inset_b.unwrap_or(3.6)).max(r.y0 + bp.inset_t.unwrap_or(3.6)),
        );
        // Text that follows a WordArt arch or circle is set on one line per paragraph.
        let follow = bp.warp.as_deref().is_some_and(deckcraft_geom::warp::is_follow);
        let wrap = bp.wrap.unwrap_or(true) && !follow && !matches!(bp.autofit, Some(AutoFit::Shape) if shape.text_box && !bp.wrap.unwrap_or(true));
        let ncols = bp.columns.unwrap_or(1).clamp(1, 16) as usize;
        let col_gap = bp.col_spacing.unwrap_or(0.0).max(0.0);
        let col_w = ((inner.width() - col_gap * (ncols as f64 - 1.0)) / ncols as f64).max(1.0);
        let rtl_cols = bp.rtl_col.unwrap_or(false) && ncols > 1;
        BoxGeom { vert, rotation, inner, wrap, ncols, col_gap, col_w, rtl_cols }
    }

    /// East Asian vertical text: CJK stands upright in text turned a quarter turn; stacked text
    /// stands every character upright, one under the other.
    fn upright(&self) -> Upright {
        match self.vert {
            TextDir::EaVertical => Upright::Cjk,
            TextDir::Stacked | TextDir::StackedRtl => Upright::All,
            _ => Upright::None,
        }
    }

    /// Mongolian vertical and stacked text: turned like vert, their lines run from left to right
    /// (as PowerPoint draws both; see [`text_turn`]).
    fn lines_ltr(&self) -> bool {
        matches!(self.vert, TextDir::MongolianVertical | TextDir::Stacked)
    }

    /// Where column `c` starts: from the left, or from the right with `rtlCol` (PowerPoint puts the
    /// first column on the right).
    fn col_x(&self, c: usize) -> f64 {
        (if self.rtl_cols { self.ncols.saturating_sub(1 + c) } else { c }) as f64 * (self.col_w + self.col_gap)
    }
}

/// Number shown for a paragraph at list level `lvl`, advancing the per-level counters.
fn next_number(numbering: &mut [u32], lvl: usize, auto_num: bool, empty: bool) -> Option<u32> {
    if auto_num && !empty {
        if let Some(n) = numbering.get_mut(lvl) {
            *n += 1;
        }
        for deeper in numbering.iter_mut().skip(lvl + 1) {
            *deeper = 0;
        }
        numbering.get(lvl).copied()
    } else if auto_num {
        // Preview the next number without consuming it: empty paragraphs do not advance
        // numbering in the saved document or affect later populated paragraphs.
        numbering.get(lvl).map(|n| n.saturating_add(1))
    } else {
        if !empty {
            for n in numbering.iter_mut().skip(lvl) {
                *n = 0;
            }
        }
        None
    }
}

fn shape_paragraphs(ctx: &Ctx, shape: &Shape, body: &TextBody, opts: &Opts, scale: f64, upright: Upright) -> Vec<ParaShaped> {
    let mut numbering: Vec<u32> = vec![0; 9];
    let mut shaped_paras: Vec<ParaShaped> = Vec::with_capacity(body.paragraphs.len());
    for (pi, para) in body.paragraphs.iter().enumerate() {
        let lvl = para.level.min(8) as usize;
        let pp = resolve::para(ctx, shape, para);
        let auto_num = matches!(pp.bullet, Some(Bullet::AutoNum { .. }));
        let number = next_number(&mut numbering, lvl, auto_num, para.is_empty());
        shaped_paras.push(shape_para(ctx, shape, body, pi, scale, opts, number, upright));
    }
    shaped_paras
}

/// A laid-out line with its placement data and, on a paragraph's first line, its bullet.
struct PlacedLine {
    info: LineInfo,
    place: LinePlace,
    bullet: Option<(String, f64, f64)>,
    /// Width of the line's text plus its start offset and right margin.
    content_w: f64,
}

/// Vertical flow state while paragraphs are stacked.
struct Flow {
    y: f64,
    prev_after: f64,
    /// How far the last line's full pitch reaches past the text (exact spacing).
    tail: f64,
    lines: Vec<PlacedLine>,
}

/// Measurements shared by the lines of one paragraph.
struct ParaEnv<'a> {
    p: &'a ParaShaped,
    g: &'a BoxGeom,
    reduce: f64,
    margin: f64,
    indent: f64,
    mr: f64,
    bullet_w: f64,
    first_x: f64,
    rest_x: f64,
    tabs: &'a [Tab],
}

impl ParaEnv<'_> {
    fn start_x(&self, li: usize) -> f64 {
        if li == 0 { self.first_x } else { self.rest_x }
    }
}

pub(crate) fn layout_scaled(ctx: &Ctx, shape: &Shape, body: &TextBody, bp: &BodyProps, opts: &Opts, scale: f64, reduce: f64) -> TextLayout {
    #[cfg(test)]
    LAYOUT_PASSES.with(|n| n.set(n.get() + 1));
    let g = BoxGeom::new(shape, bp, opts.rect);
    let shaped_paras = shape_paragraphs(ctx, shape, body, opts, scale, g.upright());
    let mut out = TextLayout { inner: g.inner, font_scale: scale, line_reduction: reduce, rotation: g.rotation, ..Default::default() };
    let first_last = bp.space_first_last.unwrap_or(false);
    let mut flow = Flow { y: 0.0, prev_after: 0.0, tail: 0.0, lines: vec![] };
    let kinsoku = ctx.pres.kinsoku.as_ref().map(KinsokuLists::of);
    for (pi, p) in shaped_paras.iter().enumerate() {
        place_paragraph(p, pi, &g, (reduce, first_last), kinsoku.as_ref(), &mut flow, &mut out);
    }
    // The text ends at the last line's descent under exact spacing; the last paragraph's space
    // after counts only when the body says so.
    let total = (flow.y - flow.tail).max(0.0) + if first_last { flow.prev_after } else { 0.0 };
    out.content_height = total;
    let mut lines = flow.lines;
    assign_columns(&mut lines, &g);
    let used_h = if g.ncols > 1 { lines.iter().map(|l| l.info.bottom).fold(0.0, f64::max) } else { total };
    out.overflow = used_h > g.inner.height() + 0.5 || (g.ncols > 1 && lines.last().is_some_and(|l| l.info.bottom > g.inner.height() + 0.5));
    let (ox, oy) = text_origin(&lines, &g, bp, used_h);
    // Emit runs, in visual order.
    for mut pl in lines {
        position_line(&mut pl.info, ox, oy, &g);
        if let Some(p) = shaped_paras.get(pl.info.para) {
            let to_empty = body.paragraphs.get(pl.info.para).is_some_and(|p| p.is_empty()) && opts.prompt_color.is_none();
            emit_bullet(&mut out, p, &pl, ox + g.col_x(pl.info.column), to_empty);
            emit_line_runs(&mut out, p, &pl.info, &pl.place);
        }
        out.lines.push(pl.info);
    }
    out.texts = shaped_paras.iter().map(|p| p.cells.iter().map(|c| c.ch).collect()).collect();
    out.boundaries = shaped_paras
        .iter()
        .map(|p| {
            let mut b: Vec<bool> = p.cells.iter().map(|c| c.boundary).collect();
            b.push(true);
            if let Some(f) = b.first_mut() {
                *f = true;
            }
            b
        })
        .collect();
    // Merge adjacent decorations of the same colour on the same line.
    out.decos = merge_decos(std::mem::take(&mut out.decos));
    out
}

/// Where the first line's text starts: past the bullet when there is one.
fn first_line_start(has_bullet: bool, margin: f64, indent: f64, bullet_w: f64) -> f64 {
    if !has_bullet {
        return (margin + indent).max(0.0);
    }
    let bx = (margin + indent).max(0.0);
    // A hanging bullet's text starts at the margin. When the hang reaches left of the box
    // (marL + indent < 0), PowerPoint pins the bullet to the box's edge and keeps the
    // hang: the first line's text starts at -indent (later lines stay at the margin).
    let stop = margin.max(-indent);
    if indent < 0.0 && bx + bullet_w <= stop { stop } else { bx + bullet_w + if indent >= 0.0 { 0.0 } else { 3.0 } }
}

/// Breaks one paragraph into lines and stacks them onto `flow`. `spacing` is
/// (line spacing reduction, whether the first paragraph's space before counts); `kinsoku` the
/// deck's own line-breaking lists (`p:kinsoku`).
fn place_paragraph(
    p: &ParaShaped,
    pi: usize,
    g: &BoxGeom,
    spacing: (f64, bool),
    kinsoku: Option<&KinsokuLists>,
    flow: &mut Flow,
    out: &mut TextLayout,
) {
    let (reduce, first_last) = spacing;
    let props = &p.props;
    let margin = props.margin_left.unwrap_or(0.0).max(0.0);
    let indent = props.indent.unwrap_or(0.0);
    let mr = props.margin_right.unwrap_or(0.0).max(0.0);
    let bullet_w = p.bullet.as_ref().map(|(s, st)| measure_str(&st.face, st.size, s).0).unwrap_or(0.0);
    let first_x = first_line_start(p.bullet.is_some(), margin, indent, bullet_w);
    let rest_x = margin;
    let tabs = tabs_of(props);
    let first_w = (g.col_w - first_x - mr).max(1.0);
    let rest_w = (g.col_w - rest_x - mr).max(1.0);
    let raw = break_lines(
        p,
        &break_rules(p, kinsoku),
        &BreakParams {
            first_w,
            rest_w,
            wrap: g.wrap,
            tab_stops: &tabs,
            default_tab: props.default_tab.unwrap_or(72.0),
            x0_first: first_x,
            x0_rest: rest_x,
        },
    );
    // The first paragraph's space before counts only when the body says so.
    let before = if pi == 0 && !first_last { 0.0 } else { spacing_pts(props.space_before, p.lead_size) };
    flow.y += before.max(0.0) + flow.prev_after;
    let first_line_idx = flow.lines.len();
    let env = ParaEnv { p, g, reduce, margin, indent, mr, bullet_w, first_x, rest_x, tabs: &tabs };
    for (li, rl) in raw.iter().enumerate() {
        let is_last = li + 1 == raw.len();
        let placed = place_line(&env, pi, rl, li, is_last, flow);
        out.content_width = out.content_width.max(placed.content_w);
        flow.lines.push(placed);
    }
    out.para_lines.push((first_line_idx, raw.len()));
    flow.prev_after = spacing_pts(props.space_after, p.lead_size).max(0.0);
}

/// Max ascent / descent / size over the line's styles (or the lead style if empty).
fn line_extent(p: &ParaShaped, rl: &RawLine) -> (f64, f64, f64) {
    let (mut asc, mut desc, mut size) = (0.0f64, 0.0f64, 0.0f64);
    for c in p.cells.get(rl.start..rl.end.max(rl.start)).unwrap_or(&[]) {
        if let (Some(st), Some(face)) = (p.styles.get(c.style), p.faces.get(c.face)) {
            let sz = st.size;
            // An equation taller than its line's text grows the line to hold it.
            if let Some(o) = c.object.and_then(|k| p.objects.get(k)) {
                asc = asc.max(o.layout.ascent + 0.1 * sz);
                desc = desc.max(o.layout.descent + 0.1 * sz);
            }
            // The run's own fonts set the line; a fallback face (emoji, a symbol or a
            // script the run's fonts lack) does not move the baseline in PowerPoint.
            let own = Arc::ptr_eq(face, &st.face)
                || [st.ea.as_deref(), st.cs.as_deref(), st.sym.as_deref()].into_iter().flatten().any(|f| face.family.eq_ignore_ascii_case(f));
            let (a, d) = line_metrics(if own { face } else { &st.face }, sz);
            asc = asc.max(a);
            desc = desc.max(d);
            size = size.max(sz);
        }
    }
    if size == 0.0 {
        // PowerPoint sizes an empty line by the mark that ends it: the `a:rPr` of the
        // `a:br` that closes it, or `a:endParaRPr` for the paragraph's last line -- not
        // by the paragraph's first run (#780). An empty *paragraph* already lands on
        // `end_props` through `lead`, and keeps the same height here.
        let brk = p.cells.get(rl.end).filter(|c| c.ch == '\u{b}').and_then(|c| p.styles.get(c.style));
        let (face, sz) = match brk {
            Some(st) => (&st.face, st.size),
            None if rl.end >= p.cells.len() => (&p.end_face, p.end_size),
            // Anything else empty (a line of tabs alone) keeps the old measure.
            None => (&p.lead_face, p.lead_size),
        };
        let (a, d) = line_metrics(face, sz);
        return (a, d, sz);
    }
    (asc, desc, size)
}

/// Logical advance of each cell on the line (tabs measure from the start edge) and their sum.
fn line_advances(env: &ParaEnv, rl: &RawLine, is_last: bool, x0: f64) -> (Vec<f64>, f64) {
    let p = env.p;
    let line_cells = p.cells.get(rl.start..rl.end).unwrap_or(&[]);
    let n = rl.end.saturating_sub(rl.start);
    let mut adv = Vec::with_capacity(n);
    let mut x = 0.0;
    // A line that breaks at a soft hyphen ends with a visible hyphen.
    let shy_end = !is_last && line_cells.last().is_some_and(|c| c.ch == '\u{AD}');
    for (k, c) in line_cells.iter().enumerate() {
        let a = if shy_end && k + 1 == n {
            p.faces.get(c.face).zip(p.styles.get(c.style)).map(|(f, st)| measure_str(f, st.size, "-").0).unwrap_or(0.0)
        } else if c.ch == '\t' {
            tab_advance(x0 + x, env.tabs, p.props.default_tab.unwrap_or(72.0), tab_segment(&p.cells, rl.start + k))
        } else {
            c.adv
        };
        adv.push(a);
        x += a;
    }
    (adv, x)
}

/// Justification: kashida (justLow, Arabic), spaces, or every character. Widens `adv` and returns
/// the kashida widths and whether the line was filled to the box edges. `body_n` is how many of
/// the line's cells are justified: all but a mark hanging past the end margin.
fn justify_line(p: &ParaShaped, rl: &RawLine, adv: &mut [f64], align: Align, slack: f64, stretch: bool, body_n: usize) -> (Vec<f64>, bool) {
    let n = adv.len();
    let line_cells = p.cells.get(rl.start..rl.end).unwrap_or(&[]);
    let mut kashida = vec![0.0; n];
    let mut filled = false;
    if stretch && align == Align::JustLow {
        let ops = kashida_opportunities(p, rl.start, rl.end);
        if !ops.is_empty() {
            let each = slack / ops.len() as f64;
            for k in ops {
                if let (Some(e), Some(a)) = (kashida.get_mut(k), adv.get_mut(k)) {
                    *e = each;
                    *a += each;
                }
            }
            filled = true;
        }
    }
    if stretch && !filled && matches!(align, Align::Justify | Align::JustLow) {
        let body = line_cells.get(..body_n).unwrap_or(line_cells);
        filled = adv.get_mut(..body_n).is_some_and(|a| justify(a, body, &p.styles, rl.start, slack));
    }
    // Distributed: an equal gap after each character but the last, a character being a
    // whole grapheme (a kana with a combining sound mark gets one gap, not two).
    let starts: Vec<usize> = (1..n).filter(|&k| line_cells.get(k).is_some_and(|c| c.boundary)).collect();
    if align == Align::Distributed && !starts.is_empty() {
        let each = slack / starts.len() as f64;
        for k in starts {
            if let Some(a) = adv.get_mut(k - 1) {
                *a += each;
            }
        }
        filled = true;
    }
    (kashida, filled)
}

/// Left edge of the line's text inside its box `(box_l, box_r)`. `widths` is the line's width and
/// the width it is aligned by (without a mark hanging past the end margin); `one_char`, whether
/// the line holds at most one character (grapheme).
fn line_left(align: Align, filled: bool, rtl: bool, bx: (f64, f64), widths: (f64, f64), slack: f64, one_char: bool) -> f64 {
    let (box_l, box_r) = bx;
    let (line_w, body_w) = widths;
    if filled {
        return box_l;
    }
    match align {
        Align::Left => box_l,
        Align::Right => box_r - body_w,
        Align::Center => box_l + (box_r - box_l - body_w) / 2.0,
        Align::Distributed if one_char => box_l + slack / 2.0,
        // Justified lines that aren't stretched (the last one) sit at the start edge.
        Align::Justify | Align::JustLow | Align::Distributed if rtl => box_r - line_w,
        Align::Justify | Align::JustLow | Align::Distributed => box_l,
    }
}

/// Visual boxes of each cell and the caret x for every boundary, from the visual order and widths.
fn edges_and_carets(order: &[usize], levels: &[u8], adv: &[f64], left: f64, rtl: bool, line_w: f64) -> (Vec<(f64, f64)>, Vec<f64>) {
    let n = adv.len();
    let mut edges = vec![(left, left); n];
    let mut vx = left;
    for &k in order {
        let a = adv.get(k).copied().unwrap_or(0.0);
        if let Some(e) = edges.get_mut(k) {
            *e = (vx, vx + a);
        }
        vx += a;
    }
    let odd = |k: usize| levels.get(k).is_some_and(|l| l % 2 == 1);
    let mut caret_x: Vec<f64> = (0..n).map(|k| edges.get(k).map(|e| if odd(k) { e.1 } else { e.0 }).unwrap_or(left)).collect();
    caret_x.push(match n.checked_sub(1) {
        Some(k) => edges.get(k).map(|e| if odd(k) { e.0 } else { e.1 }).unwrap_or(left),
        None if rtl => left + line_w,
        None => left,
    });
    (edges, caret_x)
}

/// Bullet text, x and baseline for a paragraph's first line.
fn line_bullet(env: &ParaEnv, align: Align, content_shift: f64, baseline: f64) -> Option<(String, f64, f64)> {
    let p = env.p;
    p.bullet.as_ref().map(|(s, _)| {
        // Centred and end-aligned text carries its bullet along.
        let end_aligned = if p.rtl { align == Align::Left } else { align == Align::Right };
        let shifted = align == Align::Center || end_aligned;
        let b = if shifted && p.cells.is_empty() {
            content_shift + env.first_x - env.bullet_w
        } else {
            (env.margin + env.indent).max(0.0) + if shifted { content_shift } else { 0.0 }
        };
        // x_bullet = S - (marL + indent) for RTL: the bullet's start edge is its right.
        let bx = if p.rtl { env.g.col_w - b - env.bullet_w } else { b };
        (s.clone(), bx, baseline)
    })
}

/// Lays out line `li` (`rl`) of a paragraph at the flow's current height and advances the flow.
fn place_line(env: &ParaEnv, pi: usize, rl: &RawLine, li: usize, is_last: bool, flow: &mut Flow) -> PlacedLine {
    let (p, g) = (env.p, env.g);
    let props = &p.props;
    let (asc, desc, size) = line_extent(p, rl);
    let natural = (asc + desc).max(size * 1.0);
    let lh = match props.line_spacing {
        // The reduction comes off the percentage (90 % less 20 % is 70 %), as PowerPoint
        // applies `lnSpcReduction`.
        Some(Spacing::Pct(f)) => natural * (f - env.reduce).max(f * 0.1).max(0.0),
        Some(Spacing::Pts(v)) => v * (1.0 - env.reduce),
        None => natural * (1.0 - env.reduce),
    };
    let ratio = if natural > 0.0 { lh / natural } else { 1.0 };
    let top = flow.y;
    // Exact spacing (`spcPts`), as PowerPoint sets it: the baseline sits at 80 % of the
    // line pitch, and the text ends at the last line's descent rather than a full pitch.
    let exact = matches!(props.line_spacing, Some(Spacing::Pts(_)));
    let baseline = if exact { top + lh * 0.8 } else { top + asc * ratio };
    flow.tail = if exact { (lh * 0.2 - desc).max(-lh) } else { 0.0 };
    flow.y += lh;
    let x0 = env.start_x(li);
    let avail = (g.col_w - x0 - env.mr).max(0.0);
    let n = rl.end.saturating_sub(rl.start);
    let line_cells = p.cells.get(rl.start..rl.end).unwrap_or(&[]);
    let (mut adv, width) = line_advances(env, rl, is_last, x0);
    let align = props.align.unwrap_or(Align::Left);
    // A full stop or comma hanging past the margin (`hangingPunct`) is left out of the
    // justified width: PowerPoint stretches the rest of the line to the margin and lets
    // the mark hang after it ("日本語とEnglish textの…で" + "、": its space +2.92 pt).
    // Right-aligned and centred lines are placed by the width before it too.
    let hanging = g.wrap && width > avail + 0.01 && n > 1 && line_cells.last().is_some_and(|c| is_hanging_punct(c.ch));
    let body_n = if hanging { n - 1 } else { n };
    let slack = (avail - adv.iter().take(body_n).sum::<f64>()).max(0.0);
    let stretch = !is_last && g.wrap && slack > 0.0;
    let (kashida, filled) = justify_line(p, rl, &mut adv, align, slack, stretch, body_n);
    let line_w: f64 = adv.iter().sum();
    // The width the line is aligned by: without a mark hanging past the end margin.
    let body_w = if hanging && !p.rtl { adv.iter().take(body_n).sum() } else { line_w };
    // A line of one character (grapheme) is centred by distributed alignment.
    let one_char = !line_cells.iter().skip(1).any(|c| c.boundary);
    // The line box, in column coordinates: the start edge is the left one for LTR and the
    // right one (S = W - rIns) for RTL paragraphs.
    let (box_l, box_r) = if p.rtl { (env.mr, g.col_w - x0) } else { (x0, g.col_w - env.mr) };
    let left = line_left(align, filled, p.rtl, (box_l, box_r), (line_w, body_w), slack, one_char);
    // Distance from the box's start edge to the text (bullets move with centred or
    // end-aligned text).
    let content_shift = if p.rtl { box_r - (left + line_w) } else { left - box_l };
    // Rules L1 + L2: this line's visual order.
    let chars: Vec<char> = line_cells.iter().map(|c| c.ch).collect();
    let raw_levels: Vec<u8> = line_cells.iter().map(|c| c.level).collect();
    let levels = bidi::line_levels(&chars, &raw_levels, u8::from(p.rtl));
    let order = bidi::visual_order(&levels);
    let (edges, caret_x) = edges_and_carets(&order, &levels, &adv, left, p.rtl, line_w);
    let bullet = if li == 0 { line_bullet(env, align, content_shift, baseline) } else { None };
    let boundary: Vec<bool> = line_cells.iter().map(|c| c.boundary).collect();
    PlacedLine {
        info: LineInfo {
            para: pi,
            start: rl.start,
            end: rl.end,
            top,
            baseline,
            bottom: flow.y,
            caret_x,
            column: 0,
            rtl: p.rtl,
            levels,
            edges,
            boundary,
            extent: (left, left + line_w),
            start_offset: x0,
        },
        place: LinePlace { order, kashida, shift: font_align_shifts(p, line_cells, FontAlign::of(props.font_align.as_deref(), g.vert)) },
        bullet,
        content_w: width + x0 + env.mr,
    }
}

/// Columns: distribute lines by height.
fn assign_columns(lines: &mut [PlacedLine], g: &BoxGeom) {
    let col_h = if g.ncols > 1 { g.inner.height() } else { f64::INFINITY };
    let mut col = 0usize;
    let mut col_y0 = 0.0;
    for pl in lines.iter_mut() {
        let li = &mut pl.info;
        if g.ncols > 1 && li.bottom - col_y0 > col_h + 0.01 && col + 1 < g.ncols {
            col += 1;
            col_y0 = li.top;
        }
        li.column = col;
        if col > 0 {
            let dy = col_y0;
            li.top -= dy;
            li.baseline -= dy;
            li.bottom -= dy;
        }
        let cx = g.col_x(col);
        if cx != 0.0 {
            li.shift_x(cx);
        }
    }
}

/// Origin (x, y) of the text block inside the box, from the anchor and `anchorCtr`.
fn text_origin(lines: &[PlacedLine], g: &BoxGeom, bp: &BodyProps, used_h: f64) -> (f64, f64) {
    let inner = g.inner;
    let dy = match bp.anchor.unwrap_or(Anchor::Top) {
        Anchor::Top => 0.0,
        Anchor::Middle => (inner.height() - used_h) / 2.0,
        // PowerPoint draws justified and distributed anchoring at the bottom.
        Anchor::Bottom | Anchor::Justified | Anchor::Distributed => inner.height() - used_h,
    };
    // anchorCtr: centre the drawn text block on the box. Lines sit where their alignment put
    // them, so the origin is set so the block's centre lands on the box's centre — a left-edge
    // offset alone pushed right-aligned lines out of the box (#736). Empty lines carry no ink
    // and stay out of the block. Overflowing content centres symmetrically past both edges.
    // Not applied to vertical text (there the offset would run along the wrong axis) or
    // multi-column bodies (centring unimplemented for them).
    let centre_block = bp.anchor_ctr.unwrap_or(false) && g.ncols == 1 && matches!(g.vert, TextDir::Horizontal);
    let ox = if centre_block && !lines.is_empty() {
        let ink = lines.iter().map(|l| &l.info).filter(|li| li.end > li.start);
        let (bmin, bmax) = ink.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), li| (a.min(li.extent.0), b.max(li.extent.1)));
        if bmax >= bmin { inner.x0 + inner.width() / 2.0 - (bmin + bmax) / 2.0 } else { inner.x0 }
    } else {
        inner.x0
    };
    (ox, inner.y0 + dy)
}

/// Moves a line from column coordinates into shape-local ones.
fn position_line(line: &mut LineInfo, ox: f64, oy: f64, g: &BoxGeom) {
    line.top += oy;
    line.baseline += oy;
    line.bottom += oy;
    line.shift_x(ox);
    if g.lines_ltr() {
        // The line's place mirrored across the box: the first line ends up on the left.
        let d = g.inner.y0 + g.inner.y1 - line.top - line.bottom;
        line.top += d;
        line.baseline += d;
        line.bottom += d;
    }
}

/// Emits the bullet glyph run of a line's paragraph, if the line carries one. `shift_x` is the
/// text block's x origin plus the line's column offset.
fn emit_bullet(out: &mut TextLayout, p: &ParaShaped, pl: &PlacedLine, shift_x: f64, to_empty: bool) {
    let line = &pl.info;
    let Some((s, bx, _)) = pl.bullet.as_ref() else { return };
    let Some((_, st)) = p.bullet.as_ref() else { return };
    let (_, g) = measure_str(&st.face, st.size, s);
    let run = GlyphRun {
        face: st.face.clone(),
        size: st.size,
        color: st.color,
        glyphs: g.into_iter().map(|(gid, x)| (gid, shift_x + bx + x, line.baseline)).collect(),
        cells: vec![],
        fake_bold: false,
        fake_italic: false,
        outline: None,
        para: line.para,
        chars: (0, 0),
        link: false,
        alpha: st.alpha,
        gradient: None,
        effects: None,
        upright: false,
    };
    if to_empty {
        out.empty_bullets.push(run);
    } else {
        out.runs.push(run);
    }
}

/// Position of one cell on its line: index in the line, in the paragraph, its box, the glyph
/// baseline and size, and how far font alignment moves it down from the line's baseline.
struct CellPos {
    k: usize,
    ci: usize,
    cx: f64,
    cx1: f64,
    base_y: f64,
    size: f64,
    shift: f64,
}

/// Starts a glyph run for `cell` drawn in style `st`.
fn new_run(cell: &Cell, st: &Style, face: &Arc<FontFace>, para: usize, pos: &CellPos) -> GlyphRun {
    let (fake_bold, fake_italic) = synthetic_style(st.bold, st.italic, face);
    GlyphRun {
        face: face.clone(),
        size: pos.size,
        color: st.color,
        glyphs: vec![],
        cells: vec![],
        fake_bold,
        fake_italic,
        outline: st.outline,
        para,
        chars: (pos.ci, pos.ci + 1),
        link: st.link,
        alpha: st.alpha,
        gradient: st.gradient.clone(),
        effects: st.effects,
        upright: cell.upright,
    }
}

/// Emits one line's glyph runs and decorations, in visual order.
fn emit_line_runs(out: &mut TextLayout, p: &ParaShaped, line: &LineInfo, place: &LinePlace) {
    // Group visually consecutive cells by (style, face).
    let mut cur: Option<GlyphRun> = None;
    let mut cur_key = (usize::MAX, usize::MAX, false, false);
    for &k in &place.order {
        let ci = line.start + k;
        let Some(cell) = p.cells.get(ci) else { continue };
        let Some(st) = p.styles.get(cell.style) else { continue };
        let Some(&(cx, cx1)) = line.edges.get(k) else { continue };
        // Font alignment moves the character, its underline and strikethrough together.
        let shift = place.shift.get(k).copied().unwrap_or(0.0);
        let base_y = line.baseline + shift - st.baseline * st.size;
        let size = if st.baseline != 0.0 { st.size * SUB_SUPER } else { st.size };
        let small = st.caps == Caps::Small && cell.ch.is_lowercase();
        let size = if small { size * 0.8 } else { size };
        let pos = CellPos { k, ci, cx, cx1, base_y, size, shift };
        if let Some(o) = cell.object.and_then(|j| p.objects.get(j)) {
            // An equation: drawn whole from its head cell, at the cell's left edge.
            if let Some(r) = cur.take() {
                out.runs.push(r);
            }
            cur_key = (usize::MAX, usize::MAX, false, false);
            if cell.head == ci {
                emit_cell_math(out, p, line, st, o, &pos);
            }
        } else {
            // Small capitals are drawn smaller than the capitals beside them: a run of their own.
            let key = (cell.style, cell.face, small, cell.upright);
            if key != cur_key {
                if let Some(r) = cur.take() {
                    out.runs.push(r);
                }
                let Some(face) = p.faces.get(cell.face) else { continue };
                cur = Some(new_run(cell, st, face, line.para, &pos));
                cur_key = key;
            }
            if let Some(r) = cur.as_mut() {
                push_cell_glyphs(r, p, line, place, cell, &pos);
            }
        }
        if cx1 - cx > 0.0 {
            push_cell_decos(out, cell, st, line, &pos);
        }
    }
    if let Some(r) = cur.take() {
        out.runs.push(r);
    }
}

/// Draws equation `o`, whose head cell is at `pos`, whole at the cell's left edge.
fn emit_cell_math(out: &mut TextLayout, p: &ParaShaped, line: &LineInfo, st: &Style, o: &MathObject, pos: &CellPos) {
    let ci = pos.ci;
    let end = (ci..line.end).take_while(|&j| p.cells.get(j).is_some_and(|c| c.head == ci)).last().map_or(ci + 1, |j| j + 1);
    let paint = MathPaint {
        color: st.color,
        alpha: st.alpha,
        outline: st.outline,
        link: st.link,
        gradient: st.gradient.clone(),
        effects: st.effects,
        chars: (ci, end),
        para: line.para,
        faces: &o.faces,
    };
    emit_math(out, &o.layout, &paint, pos.cx, pos.base_y);
}

/// Appends the glyphs a cell draws to the run `r`.
fn push_cell_glyphs(r: &mut GlyphRun, p: &ParaShaped, line: &LineInfo, place: &LinePlace, cell: &Cell, pos: &CellPos) {
    let CellPos { k, ci, cx, cx1, base_y, .. } = *pos;
    r.chars = (r.chars.0.min(ci), r.chars.1.max(ci + 1));
    if cell.ch == '\u{AD}' && cx1 - cx > 0.01 {
        // The soft hyphen a line breaks at is drawn as a hyphen.
        if let Some(f) = p.faces.get(cell.face) {
            r.glyphs.push((f.glyph_for('-'), cx, base_y));
            r.cells.push((ci, cell.ch));
        }
    } else if cell.head == ci && !cell.glyphs.is_empty() {
        // The cluster's glyphs start at its leftmost cell; kashida (right-to-left
        // only) fills the gap on the cluster's left with tatweel glyphs.
        let cells_of = (k..line.end.saturating_sub(line.start)).take_while(|&j| p.cells.get(line.start + j).is_some_and(|c| c.head == ci));
        let (mut cl, mut kash) = (cx, 0.0);
        for j in cells_of {
            cl = cl.min(line.edges.get(j).map(|e| e.0).unwrap_or(cx));
            kash += place.kashida.get(j).copied().unwrap_or(0.0);
        }
        if kash > 0.0 {
            push_tatweel(r, p.faces.get(cell.face), cell, (cl, kash), pos);
        }
        for (gid, gx, gy) in &cell.glyphs {
            r.glyphs.push((*gid, cl + kash + gx, base_y + gy));
            r.cells.push((ci, cell.ch));
        }
    }
}

/// Fills `kash` points left of a cluster at `cl` with tatweel glyphs.
fn push_tatweel(r: &mut GlyphRun, face: Option<&Arc<FontFace>>, cell: &Cell, fill: (f64, f64), pos: &CellPos) {
    let (cl, kash) = fill;
    let tg = face.map(|f| f.glyph_for('\u{0640}')).unwrap_or(0);
    let tw = face.map(|f| f.advance(tg) * pos.size / f.upem.max(1.0)).unwrap_or(0.0);
    if tg != 0 && tw > 0.0 {
        let count = ((kash / tw).ceil() as usize).clamp(1, 256);
        for i in 0..count {
            r.glyphs.push((tg, cl + (i as f64 * tw).min((kash - tw).max(0.0)), pos.base_y));
            r.cells.push((pos.ci, cell.ch));
        }
    }
}

/// Highlight, underline and strikethrough boxes of a cell.
fn push_cell_decos(out: &mut TextLayout, cell: &Cell, st: &Style, line: &LineInfo, pos: &CellPos) {
    let (cx, cx1) = (pos.cx, pos.cx1);
    if let Some(hl) = st.highlight {
        out.decos.push(Deco { rect: Rect::new(cx, line.top, cx1, line.bottom), color: hl, behind: true, para: line.para, line: DecoLine::Solid });
    }
    if cell.ch.is_whitespace() && st.underline.is_none() {
        return;
    }
    if let Some((uc, u)) = st.underline {
        // At the underline position and thickness of the run's own font.
        let k = st.size / st.face.upem.max(1.0);
        let (p, t) = (st.face.underline.0 * k, (st.face.underline.1 * k).max(0.5));
        for (y0, y1, kind) in u.boxes(p, t) {
            out.decos.push(Deco {
                rect: Rect::new(cx, line.baseline + pos.shift + y0, cx1, line.baseline + pos.shift + y1),
                color: uc,
                behind: false,
                para: line.para,
                line: kind,
            });
        }
    }
    if let Some(sk) = st.strike {
        // At the strikeout position and thickness of the run's own font; through the middle of
        // an upright character's em box in vertical text (a double line as a pair about it).
        let k = st.size / st.face.upem.max(1.0);
        let t = (st.face.strikeout.1 * k).max(0.5);
        let sy = if cell.upright {
            line.baseline + pos.shift - UPRIGHT_CENTRE * st.size + if sk == Strike::Double { t / 2.0 } else { -t / 2.0 }
        } else {
            line.baseline + pos.shift - st.face.strikeout.0 * k
        };
        out.decos.push(Deco { rect: Rect::new(cx, sy, cx1, sy + t), color: st.color, behind: false, para: line.para, line: DecoLine::Solid });
        if sk == Strike::Double {
            out.decos.push(Deco {
                rect: Rect::new(cx, sy - t * 2.0, cx1, sy - t),
                color: st.color,
                behind: false,
                para: line.para,
                line: DecoLine::Solid,
            });
        }
    }
}

fn merge_decos(v: Vec<Deco>) -> Vec<Deco> {
    let mut out: Vec<Deco> = Vec::with_capacity(v.len());
    for d in v {
        // Lines drawn together (a double underline, a strikethrough) interleave: look a few back.
        if let Some(last) = out.iter_mut().rev().take(4).find(|last| {
            last.color == d.color
                && last.behind == d.behind
                && last.line == d.line
                && (last.rect.y0 - d.rect.y0).abs() < 0.01
                && (last.rect.y1 - d.rect.y1).abs() < 0.01
                && (last.rect.x1 - d.rect.x0).abs() < 0.5
        }) {
            last.rect.x1 = d.rect.x1;
            continue;
        }
        out.push(d);
    }
    out
}

/// How a line's cells are placed: visual order (indices from the line start), kashida widths
/// and how far font alignment moves each cell down from the baseline (empty: none) (the boxes,
/// with justification, are in [`LineInfo::edges`]).
struct LinePlace {
    order: Vec<usize>,
    kashida: Vec<f64>,
    shift: Vec<f64>,
}

/// Kashida opportunities on a line (indices from the line start), at most one per word (its last
/// join, where elongation reads most naturally): the last cell of a right-to-left Arabic grapheme
/// that joins the next letter, in a face with a tatweel glyph, when the two aren't one ligature.
fn kashida_opportunities(p: &ParaShaped, start: usize, end: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut word_has = false;
    for i in start..end {
        let Some(c) = p.cells.get(i) else { continue };
        if c.ch.is_whitespace() {
            word_has = false;
            continue;
        }
        if !c.boundary || c.level % 2 == 0 || !joins_following(c.ch) {
            continue;
        }
        // The next grapheme on the line.
        let Some(nx) = (i + 1..end).find(|&j| p.cells.get(j).is_some_and(|d| d.boundary)) else { continue };
        let Some(d) = p.cells.get(nx) else { continue };
        let same_cluster = d.head == c.head || d.head != nx;
        let tatweel = p.faces.get(c.face).is_some_and(|f| f.glyph_for('\u{0640}') != 0);
        if joins_preceding(d.ch) && d.face == c.face && !same_cluster && tatweel {
            // A later join in the same word replaces the earlier one.
            if word_has {
                out.pop();
            }
            out.push(nx - 1 - start);
            word_has = true;
        }
    }
    out
}

/// How the characters of different sizes on one line line up across it (`a:pPr/@fontAlgn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontAlign {
    /// On a common baseline (`base`).
    Baseline,
    /// Their tops on the line's top (`t`).
    Top,
    /// Their centres on the line's centre (`ctr`).
    Center,
    /// Their bottoms on the line's bottom (`b`).
    Bottom,
}

impl FontAlign {
    /// The alignment `fontAlgn` names for text running `vert`. `auto` (no value is taken as
    /// `auto`) is the baseline in horizontal text and in text turned on its side (`vert`,
    /// `vert270`, `mongolianVert`), the centre where characters stand upright (`eaVert`,
    /// `wordArtVert`, `wordArtVertRtl`), as ECMA-376 defines `ST_TextFontAlignType`.
    fn of(v: Option<&str>, vert: TextDir) -> FontAlign {
        match v {
            Some("t") => FontAlign::Top,
            Some("ctr") => FontAlign::Center,
            Some("b") => FontAlign::Bottom,
            Some("base") => FontAlign::Baseline,
            _ if matches!(vert, TextDir::EaVertical | TextDir::Stacked | TextDir::StackedRtl) => FontAlign::Center,
            _ => FontAlign::Baseline,
        }
    }
}

/// A cell's extent across its line, (top, bottom) from the baseline (negative above it): the
/// line height its own run gives ([`line_metrics`], in the run's face or the slot face drawing
/// it), or for an upright character its em box centred [`UPRIGHT_CENTRE`] above the baseline,
/// one line pitch deep.
fn cross_extent(p: &ParaShaped, c: &Cell) -> Option<(f64, f64)> {
    let st = p.styles.get(c.style)?;
    if let Some(o) = c.object.and_then(|k| p.objects.get(k)) {
        return Some((-o.layout.ascent, o.layout.descent));
    }
    if c.upright {
        let mid = -UPRIGHT_CENTRE * st.size;
        let half = st.size * LINE_PITCH / 2.0;
        return Some((mid - half, mid + half));
    }
    let face = p.faces.get(c.face)?;
    // As for the line's height: a fallback face doesn't count, the run's own faces do.
    let own = Arc::ptr_eq(face, &st.face)
        || [st.ea.as_deref(), st.cs.as_deref(), st.sym.as_deref()].into_iter().flatten().any(|f| face.family.eq_ignore_ascii_case(f));
    let (a, d) = line_metrics(if own { face } else { &st.face }, st.size);
    Some((-a, d))
}

/// How far `align` moves each of a line's `cells` down from the baseline (empty when nothing
/// moves): each character's extent ([`cross_extent`]) lines up with the line's, the union of its
/// characters' extents (spaces left out), by its top, centre or bottom. A line of one size and
/// face doesn't move.
fn font_align_shifts(p: &ParaShaped, cells: &[Cell], align: FontAlign) -> Vec<f64> {
    if align == FontAlign::Baseline {
        return Vec::new();
    }
    let ext: Vec<Option<(f64, f64)>> = cells.iter().map(|c| cross_extent(p, c)).collect();
    let (top, bottom) = cells
        .iter()
        .zip(&ext)
        .filter(|(c, _)| !c.ch.is_whitespace())
        .filter_map(|(_, e)| *e)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(t, b), (t1, b1)| (t.min(t1), b.max(b1)));
    if top > bottom {
        return Vec::new();
    }
    ext.iter()
        .map(|e| match (e, align) {
            (Some((t, _)), FontAlign::Top) => top - t,
            (Some((_, b)), FontAlign::Bottom) => bottom - b,
            (Some((t, b)), FontAlign::Center) => (top + bottom - t - b) / 2.0,
            _ => 0.0,
        })
        .collect()
}

/// How far a space may stretch, as a share of its own advance, before the characters of an East
/// Asian justified line move apart (PowerPoint for Mac: Helvetica's 5.56-pt space at 20 pt takes
/// 4.44 pt).
const SPACE_STRETCH: f64 = 0.8;

/// Characters that take no room of their own (soft hyphens, zero-width spaces and joiners, word
/// joiners): justification puts no gap after them.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}') || c.is_control()
}

/// Spread `slack` over a wrapped justified line (`algn="just"`; `adv` the advances of `cells`, the
/// line's cells from paragraph index `start`, its trailing spaces left out), as PowerPoint for Mac
/// does. Returns whether the line now fills its box.
///
/// A character is East Asian for this when its run's `lang` is Chinese, Japanese or Korean,
/// whatever its script: Latin letters in a `ja-JP` run spread too, kanji in an `en-US` run or a
/// run without a language don't.
/// - A line with no such character is justified the Western way: its spaces take all the slack,
///   and a line without spaces stays at the start edge.
/// - Otherwise every U+0020 space (whatever its run's language) first stretches by up to
///   [`SPACE_STRETCH`] × its own advance; when the slack is less than that the spaces share it in
///   proportion and nothing else moves. The rest is divided equally among the gaps after each
///   East Asian character and after each space, except after the line's last character (17 kana
///   on a 432-pt line with 7 pt to spare: 0.4375 pt after each of the first 16). The ideographic
///   space is an ordinary character here: it gets a gap and no stretch.
fn justify(adv: &mut [f64], cells: &[Cell], styles: &[Style], start: usize, slack: f64) -> bool {
    let n = adv.len().min(cells.len());
    let cells = cells.get(..n).unwrap_or(&[]);
    // The line's characters: (first cell, last cell) of each grapheme that starts a shaping
    // cluster; a gap goes after the last cell.
    let mut units: Vec<(usize, usize)> = Vec::with_capacity(n);
    for (k, c) in cells.iter().enumerate() {
        match units.last_mut() {
            Some(u) if !c.breakable(start + k) => u.1 = k,
            _ => units.push((k, k)),
        }
    }
    let is_space = |k: usize| cells.get(k).is_some_and(|c| c.ch == ' ');
    let east_asian = |k: usize| {
        cells.get(k).is_some_and(|c| {
            c.ch != ' '
                && !is_invisible(c.ch)
                && styles.get(c.style).and_then(|st| st.lang.as_deref()).and_then(deckcraft_fonts::CjkLocale::from_lang).is_some()
        })
    };
    let spaces: Vec<usize> = (0..n).filter(|&k| is_space(k)).collect();
    let add = |adv: &mut [f64], k: usize, d: f64| {
        if let Some(a) = adv.get_mut(k) {
            *a += d;
        }
    };
    if !units.iter().any(|&(a, _)| east_asian(a)) {
        if spaces.is_empty() {
            return false;
        }
        let each = slack / spaces.len() as f64;
        for k in spaces {
            add(adv, k, each);
        }
        return true;
    }
    let caps: Vec<(usize, f64)> = spaces.iter().map(|&k| (k, adv.get(k).copied().unwrap_or(0.0).max(0.0) * SPACE_STRETCH)).collect();
    let cap: f64 = caps.iter().map(|c| c.1).sum();
    if cap > 0.0 && slack <= cap {
        for (k, c) in caps {
            add(adv, k, slack * c / cap);
        }
        return true;
    }
    for &(k, c) in &caps {
        add(adv, k, c);
    }
    let gaps: Vec<usize> =
        units.iter().take(units.len().saturating_sub(1)).filter(|&&(a, _)| east_asian(a) || is_space(a)).map(|&(_, last)| last).collect();
    if gaps.is_empty() {
        return false;
    }
    let each = (slack - cap) / gaps.len() as f64;
    for k in gaps {
        add(adv, k, each);
    }
    true
}

/// How an equation's parts are painted: its run's colour and effects.
struct MathPaint<'a> {
    color: Rgba,
    alpha: f64,
    outline: Option<(Rgba, f64)>,
    link: bool,
    gradient: Option<deckcraft_model::style::Gradient>,
    effects: Option<RunEffects>,
    /// The character range the equation's glyph runs report.
    chars: (usize, usize),
    para: usize,
    faces: &'a [Arc<FontFace>],
}

/// Add a laid-out equation to `out` with its baseline origin at `(x, y)`: glyph runs grouped by
/// face and size, fraction bars and overlines as decorations, strokes as paths.
fn emit_math(out: &mut TextLayout, lay: &math::Layout, paint: &MathPaint, x: f64, y: f64) {
    let mut runs: Vec<GlyphRun> = Vec::new();
    for g in &lay.glyphs {
        let Some(face) = paint.faces.get(g.face as usize) else { continue };
        let same = |r: &GlyphRun| Arc::ptr_eq(&r.face, face) && r.size == g.size && r.fake_bold == g.fake_bold && r.fake_italic == g.fake_italic;
        let r = match runs.iter_mut().find(|r| same(r)) {
            Some(r) => r,
            None => {
                runs.push(GlyphRun {
                    face: face.clone(),
                    size: g.size,
                    color: paint.color,
                    glyphs: vec![],
                    cells: vec![],
                    fake_bold: g.fake_bold,
                    fake_italic: g.fake_italic,
                    outline: paint.outline,
                    para: paint.para,
                    chars: paint.chars,
                    link: paint.link,
                    alpha: paint.alpha,
                    gradient: paint.gradient.clone(),
                    effects: paint.effects,
                    upright: false,
                });
                match runs.last_mut() {
                    Some(r) => r,
                    None => continue,
                }
            }
        };
        r.glyphs.push((g.id, x + g.x, y + g.y));
        r.cells.push((paint.chars.0, g.ch));
    }
    out.runs.append(&mut runs);
    for r in &lay.rules {
        out.decos.push(Deco {
            rect: Rect::new(x + r.x0, y + r.y0, x + r.x1, y + r.y1),
            color: paint.color,
            behind: false,
            para: paint.para,
            line: DecoLine::Solid,
        });
    }
    for p in &lay.paths {
        let mut path = p.clone();
        path.apply_affine(Affine::translate((x, y)));
        out.paths.push(TextPath { path, color: paint.color, alpha: paint.alpha, para: paint.para });
    }
}

/// An equation on its own, for a preview: `size` points in `family` (the run font an inserted
/// equation gets), in `color`, in display or text style, its baseline origin at the top left
/// of `inner` plus its ascent. Shape-local coordinates as [`layout`] gives them.
pub fn equation_layout(m: &math::Math, family: &str, size: f64, color: Rgba, display: bool) -> TextLayout {
    let db = FontDb::global();
    let base = db.document_face(family, "Regular");
    let mut faces = MathFaces::new(base);
    let lay = math::layout::layout(m, &mut faces, size, display);
    let mut out = TextLayout {
        inner: Rect::new(0.0, 0.0, lay.width, lay.height()),
        content_width: lay.width,
        content_height: lay.height(),
        font_scale: 1.0,
        ..Default::default()
    };
    let paint =
        MathPaint { color, alpha: 1.0, outline: None, link: false, gradient: None, effects: None, chars: (0, 0), para: 0, faces: &faces.faces };
    emit_math(&mut out, &lay, &paint, 0.0, lay.ascent);
    out.lines.push(LineInfo { top: 0.0, baseline: lay.ascent, bottom: lay.height(), extent: (0.0, lay.width), ..Default::default() });
    out
}
