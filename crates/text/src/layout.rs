use super::breaking::{
    KinsokuLists, LINE_PITCH, break_lines, break_rules, is_hanging_punct, line_metrics, measure_str, tab_advance, tab_segment, tabs_of,
};
use super::run::{Deco, DecoLine, GlyphRun, RunEffects};
use super::shape::{SUB_SUPER, Upright, shape_para};
use super::style::{Cell, MathFaces, Opts, ParaShaped, Style, spacing_pts, synthetic_style};
use super::*;

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

#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_scaled(ctx: &Ctx, shape: &Shape, body: &TextBody, bp: &BodyProps, opts: &Opts, scale: f64, reduce: f64) -> TextLayout {
    #[cfg(test)]
    LAYOUT_PASSES.with(|n| n.set(n.get() + 1));
    let r = opts.rect;
    let vert = bp.vert.unwrap_or(TextDir::Horizontal);
    // East Asian vertical text: CJK stands upright in text turned a quarter turn; stacked text
    // stands every character upright, one under the other.
    let upright_text = match vert {
        TextDir::EaVertical => Upright::Cjk,
        TextDir::Stacked | TextDir::StackedRtl => Upright::All,
        _ => Upright::None,
    };
    let (rotation, lines_ltr) = text_turn(bp);
    // For vertical text, lay out in a box with swapped dimensions centred on the text rect.
    let r = if quarter_turned(rotation) {
        let c = r.center();
        Rect::new(c.x - r.height() / 2.0, c.y - r.width() / 2.0, c.x + r.height() / 2.0, c.y + r.width() / 2.0)
    } else {
        r
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

    let mut out = TextLayout { inner, font_scale: scale, line_reduction: reduce, rotation, ..Default::default() };
    let mut y = 0.0;
    let mut numbering: Vec<u32> = vec![0; 9];
    let mut all_lines: Vec<(LineInfo, LinePlace, f64, Option<(String, f64, f64)>)> = vec![];
    let mut prev_after = 0.0;
    // How far the last line's full pitch reaches past the text (exact spacing; see below).
    let mut tail = 0.0;
    let first_last = bp.space_first_last.unwrap_or(false);
    let mut shaped_paras: Vec<ParaShaped> = Vec::with_capacity(body.paragraphs.len());
    let kinsoku = ctx.pres.kinsoku.as_ref().map(KinsokuLists::of);
    for (pi, para) in body.paragraphs.iter().enumerate() {
        let lvl = para.level.min(8) as usize;
        let pp = resolve::para(ctx, shape, para);
        let number = if matches!(pp.bullet, Some(Bullet::AutoNum { .. })) && !para.is_empty() {
            if let Some(n) = numbering.get_mut(lvl) {
                *n += 1;
            }
            for deeper in numbering.iter_mut().skip(lvl + 1) {
                *deeper = 0;
            }
            numbering.get(lvl).copied()
        } else if matches!(pp.bullet, Some(Bullet::AutoNum { .. })) {
            // Preview the next number without consuming it: empty paragraphs do not advance
            // numbering in the saved document or affect later populated paragraphs.
            numbering.get(lvl).map(|n| n.saturating_add(1))
        } else {
            if !para.is_empty() {
                for n in numbering.iter_mut().skip(lvl) {
                    *n = 0;
                }
            }
            None
        };
        shaped_paras.push(shape_para(ctx, shape, body, pi, scale, opts, number, upright_text));
    }
    for (pi, p) in shaped_paras.iter().enumerate() {
        let props = &p.props;
        let margin = props.margin_left.unwrap_or(0.0).max(0.0);
        let indent = props.indent.unwrap_or(0.0);
        let mr = props.margin_right.unwrap_or(0.0).max(0.0);
        let bullet_w = p.bullet.as_ref().map(|(s, st)| measure_str(&st.face, st.size, s).0).unwrap_or(0.0);
        let first_x = if p.bullet.is_some() {
            let bx = (margin + indent).max(0.0);
            // A hanging bullet's text starts at the margin. When the hang reaches left of the box
            // (marL + indent < 0), PowerPoint pins the bullet to the box's edge and keeps the
            // hang: the first line's text starts at -indent (later lines stay at the margin).
            let stop = margin.max(-indent);
            if indent < 0.0 && bx + bullet_w <= stop { stop } else { bx + bullet_w + if indent >= 0.0 { 0.0 } else { 3.0 } }
        } else {
            (margin + indent).max(0.0)
        };
        let rest_x = margin;
        let tabs = tabs_of(props);
        let first_w = (col_w - first_x - mr).max(1.0);
        let rest_w = (col_w - rest_x - mr).max(1.0);
        let raw = break_lines(p, &break_rules(p, kinsoku.as_ref()), first_w, rest_w, wrap, &tabs, props.default_tab.unwrap_or(72.0), first_x, rest_x);
        // The first paragraph's space before counts only when the body says so.
        let before = if pi == 0 && !first_last { 0.0 } else { spacing_pts(props.space_before, p.lead_size) };
        y += before.max(0.0) + prev_after;
        let first_line_idx = all_lines.len();
        for (li, rl) in raw.iter().enumerate() {
            // Line metrics: max ascent/descent over the line's styles (or the lead style if empty).
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
                asc = a;
                desc = d;
                size = sz;
            }
            let natural = (asc + desc).max(size * 1.0);
            let lh = match props.line_spacing {
                // The reduction comes off the percentage (90 % less 20 % is 70 %), as PowerPoint
                // applies `lnSpcReduction`.
                Some(Spacing::Pct(f)) => natural * (f - reduce).max(f * 0.1).max(0.0),
                Some(Spacing::Pts(v)) => v * (1.0 - reduce),
                None => natural * (1.0 - reduce),
            };
            let ratio = if natural > 0.0 { lh / natural } else { 1.0 };
            let top = y;
            // Exact spacing (`spcPts`), as PowerPoint sets it: the baseline sits at 80 % of the
            // line pitch, and the text ends at the last line's descent rather than a full pitch.
            let exact = matches!(props.line_spacing, Some(Spacing::Pts(_)));
            let baseline = if exact { top + lh * 0.8 } else { top + asc * ratio };
            tail = if exact { (lh * 0.2 - desc).max(-lh) } else { 0.0 };
            y += lh;
            let x0 = if li == 0 { first_x } else { rest_x };
            let avail = (col_w - x0 - mr).max(0.0);
            let n = rl.end.saturating_sub(rl.start);
            let line_cells = p.cells.get(rl.start..rl.end).unwrap_or(&[]);
            // Logical advances; tabs measure from the start edge.
            let mut adv = Vec::with_capacity(n);
            let mut x = 0.0;
            // A line that breaks at a soft hyphen ends with a visible hyphen.
            let shy_end = li + 1 < raw.len() && line_cells.last().is_some_and(|c| c.ch == '\u{AD}');
            for (k, c) in line_cells.iter().enumerate() {
                let a = if shy_end && k + 1 == n {
                    p.faces.get(c.face).zip(p.styles.get(c.style)).map(|(f, st)| measure_str(f, st.size, "-").0).unwrap_or(0.0)
                } else if c.ch == '\t' {
                    tab_advance(x0 + x, &tabs, props.default_tab.unwrap_or(72.0), tab_segment(&p.cells, rl.start + k))
                } else {
                    c.adv
                };
                adv.push(a);
                x += a;
            }
            let width = x;
            let align = props.align.unwrap_or(Align::Left);
            let last_line = li + 1 == raw.len();
            // A full stop or comma hanging past the margin (`hangingPunct`) is left out of the
            // justified width: PowerPoint stretches the rest of the line to the margin and lets
            // the mark hang after it ("日本語とEnglish textの…で" + "、": its space +2.92 pt).
            // Right-aligned and centred lines are placed by the width before it too.
            let hanging = wrap && width > avail + 0.01 && n > 1 && line_cells.last().is_some_and(|c| is_hanging_punct(c.ch));
            let body_n = if hanging { n - 1 } else { n };
            let slack = (avail - adv.iter().take(body_n).sum::<f64>()).max(0.0);
            // Justification: kashida (justLow, Arabic), spaces, or every character.
            let stretch = !last_line && wrap && slack > 0.0;
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
            let one_char = starts.is_empty();
            if align == Align::Distributed && !one_char {
                let each = slack / starts.len() as f64;
                for k in starts {
                    if let Some(a) = adv.get_mut(k - 1) {
                        *a += each;
                    }
                }
                filled = true;
            }
            let line_w: f64 = adv.iter().sum();
            // The width the line is aligned by: without a mark hanging past the end margin.
            let body_w = if hanging && !p.rtl { adv.iter().take(body_n).sum() } else { line_w };
            // The line box, in column coordinates: the start edge is the left one for LTR and the
            // right one (S = W - rIns) for RTL paragraphs.
            let (box_l, box_r) = if p.rtl { (mr, col_w - x0) } else { (x0, col_w - mr) };
            let left = if filled {
                box_l
            } else {
                match align {
                    Align::Left => box_l,
                    Align::Right => box_r - body_w,
                    Align::Center => box_l + (box_r - box_l - body_w) / 2.0,
                    Align::Distributed if one_char => box_l + slack / 2.0,
                    // Justified lines that aren't stretched (the last one) sit at the start edge.
                    Align::Justify | Align::JustLow | Align::Distributed if p.rtl => box_r - line_w,
                    Align::Justify | Align::JustLow | Align::Distributed => box_l,
                }
            };
            // Distance from the box's start edge to the text (bullets move with centred or
            // end-aligned text).
            let content_shift = if p.rtl { box_r - (left + line_w) } else { left - box_l };
            // Rules L1 + L2: this line's visual order.
            let chars: Vec<char> = line_cells.iter().map(|c| c.ch).collect();
            let raw_levels: Vec<u8> = line_cells.iter().map(|c| c.level).collect();
            let levels = bidi::line_levels(&chars, &raw_levels, u8::from(p.rtl));
            let order = bidi::visual_order(&levels);
            let mut edges = vec![(left, left); n];
            let mut vx = left;
            for &k in &order {
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
                None if p.rtl => left + line_w,
                None => left,
            });
            let bullet = if li == 0 {
                p.bullet.as_ref().map(|(s, _)| {
                    // Centred and end-aligned text carries its bullet along.
                    let end_aligned = if p.rtl { align == Align::Left } else { align == Align::Right };
                    let shifted = align == Align::Center || end_aligned;
                    let b = if shifted && p.cells.is_empty() {
                        content_shift + x0 - bullet_w
                    } else {
                        (margin + indent).max(0.0) + if shifted { content_shift } else { 0.0 }
                    };
                    // x_bullet = S - (marL + indent) for RTL: the bullet's start edge is its right.
                    let bx = if p.rtl { col_w - b - bullet_w } else { b };
                    (s.clone(), bx, baseline)
                })
            } else {
                None
            };
            out.content_width = out.content_width.max(width + x0 + mr);
            let boundary: Vec<bool> = line_cells.iter().map(|c| c.boundary).collect();
            all_lines.push((
                LineInfo {
                    para: pi,
                    start: rl.start,
                    end: rl.end,
                    top,
                    baseline,
                    bottom: y,
                    caret_x,
                    column: 0,
                    rtl: p.rtl,
                    levels,
                    edges,
                    boundary,
                    extent: (left, left + line_w),
                    start_offset: x0,
                },
                LinePlace { order, kashida, shift: font_align_shifts(p, line_cells, FontAlign::of(props.font_align.as_deref(), vert)) },
                lh,
                bullet,
            ));
        }
        out.para_lines.push((first_line_idx, raw.len()));
        prev_after = spacing_pts(props.space_after, p.lead_size).max(0.0);
    }
    // The text ends at the last line's descent under exact spacing; the last paragraph's space
    // after counts only when the body says so.
    let total = (y - tail).max(0.0) + if first_last { prev_after } else { 0.0 };
    out.content_height = total;
    // Where column `c` starts: from the left, or from the right with `rtlCol` (PowerPoint puts the
    // first column on the right).
    let rtl_cols = bp.rtl_col.unwrap_or(false) && ncols > 1;
    let col_x = |c: usize| (if rtl_cols { ncols.saturating_sub(1 + c) } else { c }) as f64 * (col_w + col_gap);
    // Columns: distribute lines by height.
    let col_h = if ncols > 1 { inner.height() } else { f64::INFINITY };
    let mut col = 0usize;
    let mut col_y0 = 0.0;
    for (li, _, _, _) in all_lines.iter_mut() {
        if ncols > 1 && li.bottom - col_y0 > col_h + 0.01 && col + 1 < ncols {
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
        let cx = col_x(col);
        if cx != 0.0 {
            li.shift_x(cx);
        }
    }
    let used_h = if ncols > 1 { all_lines.iter().map(|(l, ..)| l.bottom).fold(0.0, f64::max) } else { total };
    out.overflow = used_h > inner.height() + 0.5 || (ncols > 1 && all_lines.last().is_some_and(|(l, ..)| l.bottom > inner.height() + 0.5));
    let anchor = bp.anchor.unwrap_or(Anchor::Top);
    let dy = match anchor {
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
    let centre_block = bp.anchor_ctr.unwrap_or(false) && ncols == 1 && matches!(vert, TextDir::Horizontal);
    let ox = if centre_block && !all_lines.is_empty() {
        let ink = all_lines.iter().filter(|(li, ..)| li.end > li.start);
        let (bmin, bmax) = ink.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), (li, ..)| (a.min(li.extent.0), b.max(li.extent.1)));
        if bmax >= bmin { inner.x0 + inner.width() / 2.0 - (bmin + bmax) / 2.0 } else { inner.x0 }
    } else {
        inner.x0
    };
    let oy = inner.y0 + dy;
    // Emit runs, in visual order.
    for (line, place, _, bullet) in all_lines.iter_mut() {
        line.top += oy;
        line.baseline += oy;
        line.bottom += oy;
        line.shift_x(ox);
        if lines_ltr {
            // The line's place mirrored across the box: the first line ends up on the left.
            let d = inner.y0 + inner.y1 - line.top - line.bottom;
            line.top += d;
            line.baseline += d;
            line.bottom += d;
        }
        let Some(p) = shaped_paras.get(line.para) else { continue };
        if let Some((s, bx, _)) = bullet.take()
            && let Some((_, st)) = p.bullet.as_ref()
        {
            let (_, g) = measure_str(&st.face, st.size, &s);
            let run = GlyphRun {
                face: st.face.clone(),
                size: st.size,
                color: st.color,
                glyphs: g.into_iter().map(|(gid, x)| (gid, ox + bx + x + col_x(line.column), line.baseline)).collect(),
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
            if body.paragraphs.get(line.para).is_some_and(|p| p.is_empty()) && opts.prompt_color.is_none() {
                out.empty_bullets.push(run);
            } else {
                out.runs.push(run);
            }
        }
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
            let size = if st.caps == Caps::Small && cell.ch.is_lowercase() { size * 0.8 } else { size };
            if let Some(o) = cell.object.and_then(|k| p.objects.get(k)) {
                // An equation: drawn whole from its head cell, at the cell's left edge.
                if let Some(r) = cur.take() {
                    out.runs.push(r);
                }
                cur_key = (usize::MAX, usize::MAX, false, false);
                if cell.head == ci {
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
                    emit_math(&mut out, &o.layout, &paint, cx, base_y);
                }
            } else {
                // Small capitals are drawn smaller than the capitals beside them: a run of their own.
                let key = (cell.style, cell.face, st.caps == Caps::Small && cell.ch.is_lowercase(), cell.upright);
                if key != cur_key {
                    if let Some(r) = cur.take() {
                        out.runs.push(r);
                    }
                    let Some(face) = p.faces.get(cell.face) else { continue };
                    let (fake_bold, fake_italic) = synthetic_style(st.bold, st.italic, face);
                    cur = Some(GlyphRun {
                        face: face.clone(),
                        size,
                        color: st.color,
                        glyphs: vec![],
                        cells: vec![],
                        fake_bold,
                        fake_italic,
                        outline: st.outline,
                        para: line.para,
                        chars: (ci, ci + 1),
                        link: st.link,
                        alpha: st.alpha,
                        gradient: st.gradient.clone(),
                        effects: st.effects,
                        upright: cell.upright,
                    });
                    cur_key = key;
                }
                if let Some(r) = cur.as_mut() {
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
                        let cells_of =
                            (k..line.end.saturating_sub(line.start)).take_while(|&j| p.cells.get(line.start + j).is_some_and(|c| c.head == ci));
                        let (mut cl, mut kash) = (cx, 0.0);
                        for j in cells_of {
                            cl = cl.min(line.edges.get(j).map(|e| e.0).unwrap_or(cx));
                            kash += place.kashida.get(j).copied().unwrap_or(0.0);
                        }
                        if kash > 0.0 {
                            let face = p.faces.get(cell.face);
                            let tg = face.map(|f| f.glyph_for('\u{0640}')).unwrap_or(0);
                            let tw = face.map(|f| f.advance(tg) * size / f.upem.max(1.0)).unwrap_or(0.0);
                            if tg != 0 && tw > 0.0 {
                                let count = ((kash / tw).ceil() as usize).clamp(1, 256);
                                for i in 0..count {
                                    r.glyphs.push((tg, cl + (i as f64 * tw).min((kash - tw).max(0.0)), base_y));
                                    r.cells.push((ci, cell.ch));
                                }
                            }
                        }
                        for (gid, gx, gy) in &cell.glyphs {
                            r.glyphs.push((*gid, cl + kash + gx, base_y + gy));
                            r.cells.push((ci, cell.ch));
                        }
                    }
                }
            }
            let adv = cx1 - cx;
            if adv <= 0.0 {
                continue;
            }
            if let Some(hl) = st.highlight {
                out.decos.push(Deco {
                    rect: Rect::new(cx, line.top, cx1, line.bottom),
                    color: hl,
                    behind: true,
                    para: line.para,
                    line: DecoLine::Solid,
                });
            }
            if !cell.ch.is_whitespace() || st.underline.is_some() {
                if let Some((uc, u)) = st.underline {
                    // At the underline position and thickness of the run's own font.
                    let k = st.size / st.face.upem.max(1.0);
                    let (p, t) = (st.face.underline.0 * k, (st.face.underline.1 * k).max(0.5));
                    for (y0, y1, kind) in u.boxes(p, t) {
                        out.decos.push(Deco {
                            rect: Rect::new(cx, line.baseline + shift + y0, cx1, line.baseline + shift + y1),
                            color: uc,
                            behind: false,
                            para: line.para,
                            line: kind,
                        });
                    }
                }
                if let Some(sk) = st.strike {
                    // At the strikeout position and thickness of the run's own font; through the
                    // middle of an upright character's em box in vertical text (a double line
                    // as a pair about it).
                    let k = st.size / st.face.upem.max(1.0);
                    let t = (st.face.strikeout.1 * k).max(0.5);
                    let sy = if cell.upright {
                        line.baseline + shift - UPRIGHT_CENTRE * st.size + if sk == Strike::Double { t / 2.0 } else { -t / 2.0 }
                    } else {
                        line.baseline + shift - st.face.strikeout.0 * k
                    };
                    out.decos.push(Deco {
                        rect: Rect::new(cx, sy, cx1, sy + t),
                        color: st.color,
                        behind: false,
                        para: line.para,
                        line: DecoLine::Solid,
                    });
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
        }
        if let Some(r) = cur.take() {
            out.runs.push(r);
        }
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
    out.lines = all_lines.into_iter().map(|(l, ..)| l).collect();
    // Merge adjacent decorations of the same colour on the same line.
    out.decos = merge_decos(std::mem::take(&mut out.decos));
    out
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
