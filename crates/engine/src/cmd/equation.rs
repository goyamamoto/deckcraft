//! Equations: Insert ▸ Equation from TeX (`insert.equation`), and what the Equation dialog asks
//! (`equation.at`, `equation.check`). The equation is stored as Office Math (OMML) with its
//! linear text, as PowerPoint stores it; `deckcraft-math` does the TeX, OMML and layout.
//! Agents find equations in `slide.inspect` (`equations`, with their TeX) and rewrite one with
//! `insert.equation {id, at}`.

use deckcraft_geom::Xfrm;
use deckcraft_math as math;
use deckcraft_model::edit::Pos;
use deckcraft_model::text::{Align, Paragraph, Run, RunKind, RunProps, TextBody};
use deckcraft_model::{Shape, ShapeId};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_param, cmd, f64_param, has_slide, id_param, rect_param, str_param};
use crate::{Result, Session};

const INSERT: &str = "insert.equation";

/// The font equations are set in: PowerPoint's math font, which the font database stands in for
/// (Caladea) where it is not installed.
pub const MATH_FONT: &str = "Cambria Math";

/// The DrawingML run properties every OMML run carries, as PowerPoint writes them: the math font
/// by name (`xmlns:a` is the slide's).
const MATH_RUN_PROPS: &str = "<a:rPr><a:latin typeface=\"Cambria Math\"/></a:rPr>";

/// The size an equation run has when it names none.
const DEFAULT_SIZE: f64 = 18.0;

/// The inset of an equation box on each side: room for the caret beside the equation, inside
/// the frame. Sizing goes by the equation inside, so the inset never scales.
pub const BOX_INSET: f64 = 2.0;

/// The horizontal and vertical insets of `sh`'s text box, as laid out.
pub fn insets(sh: &Shape) -> (f64, f64) {
    let b = sh.text.as_ref().map(|t| &t.body);
    let l = b.and_then(|b| b.inset_l).unwrap_or(7.2) + b.and_then(|b| b.inset_r).unwrap_or(7.2);
    let t = b.and_then(|b| b.inset_t).unwrap_or(3.6) + b.and_then(|b| b.inset_b).unwrap_or(3.6);
    (l.max(0.0), t.max(0.0))
}

/// The ratio a box of `old` sized to `new` scales its equation by: the equation's own extent
/// (the box less its insets) is what grows, by width when `w` is asked for, else by height.
pub fn scale_ratio(sh: &Shape, old: (f64, f64), new: (Option<f64>, Option<f64>)) -> Option<f64> {
    let (il, it) = insets(sh);
    let ratio = match new {
        (Some(w), _) if old.0 - il > 1e-6 => (w - il) / (old.0 - il),
        (_, Some(h)) if old.1 - it > 1e-6 => (h - it) / (old.1 - it),
        _ => return None,
    };
    (ratio.is_finite() && ratio > 0.0).then(|| ratio.clamp(0.01, 100.0))
}

/// A shape holding one equation and nothing else (Insert ▸ Equation's text box): one paragraph
/// whose only content is a math run. Such a shape is sized as a picture is — its handles scale
/// the equation — and double-clicking it edits the equation.
pub fn equation_box(sh: &Shape) -> bool {
    equation_run(sh).is_some()
}

/// The math run of an [`equation_box`]: `(paragraph, run)` indices and the run.
fn equation_run(sh: &Shape) -> Option<(usize, &Run)> {
    let body = sh.text.as_ref()?;
    if body.paragraphs.len() != 1 {
        return None;
    }
    let para = body.paragraphs.first()?;
    let mut found = None;
    for (i, r) in para.runs.iter().enumerate() {
        match &r.kind {
            RunKind::Math { .. } if found.is_none() => found = Some((i, r)),
            RunKind::Math { .. } => return None,
            RunKind::Text if r.text.trim().is_empty() => {}
            _ => return None,
        }
    }
    found
}

/// An equation in a text body: where its run is, the text positions it spans, its OMML.
struct MathAt<'a> {
    para: usize,
    run: usize,
    start: usize,
    end: usize,
    omml: &'a str,
    text: &'a str,
}

/// Every equation in `body`, in reading order.
fn math_runs(body: &TextBody) -> Vec<MathAt<'_>> {
    let mut out = Vec::new();
    for (pi, para) in body.paragraphs.iter().enumerate() {
        let mut off = 0;
        for (ri, run) in para.runs.iter().enumerate() {
            let start = off;
            off += run.char_len();
            if let RunKind::Math { omml } = &run.kind {
                out.push(MathAt { para: pi, run: ri, start, end: off, omml, text: &run.text });
            }
        }
    }
    out
}

/// TeX for an equation: from its OMML, else its linear text.
fn tex_of(m: &MathAt) -> String {
    math::omml_to_tex(m.omml).filter(|t| !t.is_empty()).unwrap_or_else(|| m.text.to_string())
}

/// The equations in `body` as `slide.inspect` lists them: `{tex, display, start, end}`, the
/// positions being `[paragraph, char]` as `text.select` and `insert.equation`'s `at` take them.
pub fn equations_json(body: &TextBody) -> Vec<Value> {
    math_runs(body)
        .iter()
        .map(|m| json!({"tex": tex_of(m), "display": math::omml::is_display(m.omml), "start": [m.para, m.start], "end": [m.para, m.end]}))
        .collect()
}

/// The equation's font size in an [`equation_box`].
pub fn equation_size(sh: &Shape) -> Option<f64> {
    equation_run(sh).map(|(_, r)| r.props.size.unwrap_or(DEFAULT_SIZE))
}

/// Set the equation's font size in [`equation_box`] `id` (its box is refitted by the caller).
pub(crate) fn set_equation_size(s: &mut Session, id: ShapeId, size: f64) -> Result<()> {
    let size = if size.is_finite() { size.clamp(1.0, 4000.0) } else { DEFAULT_SIZE };
    s.edit(|doc, sel| {
        if let Some(sh) = crate::shapes_mut(doc, sel).and_then(|l| deckcraft_model::find_shape_mut(l, id))
            && let Some(body) = sh.text.as_mut()
        {
            for r in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
                r.props.size = Some(size);
            }
            for p in &mut body.paragraphs {
                p.end_props.size = Some(size);
            }
        }
        Ok(())
    })
}

/// Size (`shape.resize`) on an equation box: the equation scales with the box — by the width
/// asked for, else the height — and the box is refitted around it. True when `id` was one.
pub(crate) fn resize_equation(s: &mut Session, id: ShapeId, w: Option<f64>, h: Option<f64>) -> Result<bool> {
    let st = s.doc()?;
    let Some(sh) = st.shape(id).filter(|sh| equation_box(sh)).cloned() else { return Ok(false) };
    let Some(size) = equation_size(&sh) else { return Ok(false) };
    let x = super::xfrm_of(&st.doc, &st.selection, &sh);
    let Some(ratio) = scale_ratio(&sh, (x.w, x.h), (w, h)) else { return Ok(true) };
    set_equation_size(s, id, size * ratio)?;
    super::text::fit_text_box(s, id)?;
    Ok(true)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "insert.equation",
            "Equation",
            ["Insert", "Symbols"],
            None,
            "{tex: TeX source, display?: bool, size?: pt, rect?: [x,y,w,h], id?, at?: [p, c]} — at the insertion point (replacing the selection), or in a new text box centred in `rect` (default: the slide); with `id`, replaces the equation in that shape's text that `at` touches (default: its first), keeping its formatting, an equation box keeping its centre. Display style (limits above and below, display fractions) by default in a box of its own, text style at the insertion point, the replaced equation's style with `id`. `size` is the equation's font size",
            has_slide,
            insert
        ),
        cmd!(
            query "equation.at",
            "Equation at Caret",
            [],
            None,
            "{} → {tex, display, start, end, id?} for the equation the caret touches or the selection covers, or the selected equation box, else null",
            has_slide,
            at
        ),
        cmd!(query "equation.check", "Check Equation", [], None, "{tex} → {ok, text, error}", always, check),
    ]
}

/// OMML and linear text for `tex`; a run needs a character to be laid out, selected and deleted.
fn convert(tex: &str, display: bool) -> Result<(String, String)> {
    let (omml, text) = math::tex_to_omml_runs(tex, MATH_RUN_PROPS, display).map_err(|e| bad(INSERT, e.to_string()))?;
    Ok((omml, if text.is_empty() { " ".to_string() } else { text }))
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let tex = str_param(p, "tex").ok_or_else(|| bad(INSERT, "missing `tex`"))?;
    if tex.trim().is_empty() {
        return Err(bad(INSERT, "the equation is empty"));
    }
    let size = f64_param(p, "size").map(|v| v.clamp(1.0, 4000.0));
    if let Some(id) = id_param(p, "id", INSERT)? {
        return replace(s, id, super::text::pos_param(p, "at"), tex, bool_param(p, "display"), size);
    }
    let inline = s.doc()?.selection.text.is_some();
    let display = super::bool_or(p, "display", !inline);
    let (omml, text) = convert(tex, display)?;
    let run = Run { text, props: RunProps::default(), kind: RunKind::Math { omml } };
    if inline {
        return super::text::insert_math(s, run, size);
    }
    // A text box of its own, centred in the area (the slide by default), that hugs the equation
    // (no wrapping, so the fit sets its width too): its handles then sit on the equation and
    // scale it. The box is centred again once it fits the equation.
    let area = match rect_param(p, "rect") {
        Some(r) => r,
        None => {
            let page = s.active().map(|d| d.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
            Xfrm::new(0.0, 0.0, page.width, page.height)
        }
    };
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    if !(cx.is_finite() && cy.is_finite()) {
        return Err(bad(INSERT, "`rect` is out of range"));
    }
    let mut shape = super::insert::new_text_box(Xfrm::new(cx - 150.0, cy - 25.0, 300.0, 50.0), "");
    if let Some(tb) = shape.text.as_mut() {
        let mut run = run;
        run.props.font = Some(MATH_FONT.into());
        run.props.size = size;
        let mut para = Paragraph::default();
        para.end_props.size = size;
        para.props.align = Some(Align::Center);
        para.runs = vec![run];
        tb.paragraphs = vec![para];
        tb.body.wrap = Some(false);
        // A small inset: the box is the equation plus room for the caret beside it; sizing
        // goes by the equation inside (`scale_ratio`), so the box scales it exactly.
        tb.body.inset_l = Some(BOX_INSET);
        tb.body.inset_r = Some(BOX_INSET);
        tb.body.inset_t = Some(BOX_INSET);
        tb.body.inset_b = Some(BOX_INSET);
    }
    shape.name = "Equation".into();
    let id = super::insert::add_shape(s, INSERT, shape, true)?;
    super::text::fit_text_box(s, id)?;
    centre_at(s, id, cx, cy)?;
    Ok(json!({"id": id}))
}

/// Move shape `id` so its box is centred on `(cx, cy)`, within the command's edit.
fn centre_at(s: &mut Session, id: ShapeId, cx: f64, cy: f64) -> Result<()> {
    s.edit(|doc, sel| {
        if let Some(sh) = crate::shapes_mut(doc, sel).and_then(|l| deckcraft_model::find_shape_mut(l, id))
            && let Some(x) = sh.xfrm.as_mut()
        {
            x.x = cx - x.w / 2.0;
            x.y = cy - x.h / 2.0;
        }
        Ok(())
    })
}

/// `insert.equation` with `id`: the equation in shape `id`'s text that `at` touches (its first
/// without `at`) becomes `tex`, keeping its run's formatting and, unless `display` says
/// otherwise, its style. An equation box is refitted around its new equation and keeps its
/// centre; a caret in that text goes after the equation.
fn replace(s: &mut Session, id: ShapeId, at: Option<Pos>, tex: &str, display: Option<bool>, size: Option<f64>) -> Result<Value> {
    let st = s.doc()?;
    let sh = st.shape(id).ok_or_else(|| bad(INSERT, format!("no shape {id} on this slide")))?;
    // The box's own frame (a group member's is in its group's space), which `centre_at` sets.
    let before = sh.xfrm.filter(|_| equation_box(sh));
    let found = sh.text.as_ref().map(math_runs).unwrap_or_default();
    let target = match at {
        Some((p, c)) => found.iter().find(|m| m.para == p && (m.start..=m.end).contains(&c)),
        None => found.first(),
    };
    let Some(m) = target else {
        return Err(bad(
            INSERT,
            match at {
                Some((p, c)) if !found.is_empty() => format!("no equation at [{p}, {c}] in shape {id}"),
                _ => format!("shape {id} holds no equation"),
            },
        ));
    };
    let (pi, ri, start) = (m.para, m.run, m.start);
    let (omml, text) = convert(tex, display.unwrap_or_else(|| math::omml::is_display(m.omml)))?;
    let end = start.saturating_add(text.chars().count());
    s.edit(|doc, sel| {
        let run = crate::shapes_mut(doc, sel)
            .and_then(|l| deckcraft_model::find_shape_mut(l, id))
            .and_then(|sh| sh.text.as_mut())
            .and_then(|b| b.paragraphs.get_mut(pi))
            .and_then(|p| p.runs.get_mut(ri))
            .ok_or_else(|| bad(INSERT, "the equation is gone"))?;
        run.kind = RunKind::Math { omml };
        run.text = text;
        if let Some(size) = size {
            run.props.size = Some(size);
        }
        if let Some(t) = sel.text.as_mut().filter(|t| t.shape == id && t.cell.is_none() && !t.notes) {
            t.anchor = (pi, end);
            t.caret = (pi, end);
        }
        Ok(())
    })?;
    if before.is_some()
        && let Some(size) = size
    {
        set_equation_size(s, id, size)?;
    }
    super::text::fit_text_box(s, id)?;
    if let Some(b) = before {
        centre_at(s, id, b.x + b.w / 2.0, b.y + b.h / 2.0)?;
    }
    Ok(json!({"id": id, "start": [pi, start], "end": [pi, end]}))
}

/// The equation the caret is in or beside, or that the selection covers (within its paragraph);
/// with no text being edited, the equation of the one selected equation box (with its `id`, so
/// the dialog can edit it in place).
fn at(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let Some(t) = &st.selection.text else {
        let [id] = st.selection.shapes.as_slice() else { return Ok(Value::Null) };
        let Some(sh) = st.shape(*id) else { return Ok(Value::Null) };
        let Some((ri, run)) = equation_run(sh) else { return Ok(Value::Null) };
        let RunKind::Math { omml } = &run.kind else { return Ok(Value::Null) };
        let start: usize = sh.text.as_ref().and_then(|t| t.paragraphs.first()).map(|p| p.runs.iter().take(ri).map(Run::char_len).sum()).unwrap_or(0);
        let tex = math::omml_to_tex(omml).filter(|t| !t.is_empty()).unwrap_or_else(|| run.text.clone());
        return Ok(json!({"tex": tex, "display": math::omml::is_display(omml), "start": [0, start], "end": [0, start + run.char_len()], "id": id}));
    };
    let Some(body) = super::text::body_of(st, t) else { return Ok(Value::Null) };
    let (a, b) = t.ordered();
    if a.0 != b.0 {
        return Ok(Value::Null);
    }
    let Some(para) = body.paragraphs.get(b.0) else { return Ok(Value::Null) };
    let mut off = 0;
    for run in &para.runs {
        let (ra, rb) = (off, off + run.char_len());
        off = rb;
        let RunKind::Math { omml } = &run.kind else { continue };
        let touches = if a == b { a.1 >= ra && a.1 <= rb } else { a.1 >= ra && b.1 <= rb };
        if touches {
            let tex = math::omml_to_tex(omml).filter(|t| !t.is_empty()).unwrap_or_else(|| run.text.clone());
            return Ok(json!({"tex": tex, "display": math::omml::is_display(omml), "start": [b.0, ra], "end": [b.0, rb]}));
        }
    }
    Ok(Value::Null)
}

/// Does `tex` parse? With its linear text, or the error and where it is.
fn check(_s: &mut Session, p: &Value) -> Result<Value> {
    let tex = str_param(p, "tex").unwrap_or("");
    Ok(match math::tex_to_omml(tex) {
        Ok((_, text)) => json!({"ok": true, "text": text}),
        Err(e) => json!({"ok": false, "error": {"pos": e.pos, "message": e.msg}}),
    })
}
