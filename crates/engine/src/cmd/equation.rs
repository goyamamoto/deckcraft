//! Equations: Insert ▸ Equation from TeX (`insert.equation`), and what the Equation dialog asks
//! (`equation.at`, `equation.check`). The equation is stored as Office Math (OMML) with its
//! linear text, as PowerPoint stores it; `deckcraft-math` does the TeX, OMML and layout.

use deckcraft_math as math;
use deckcraft_model::text::{Align, Paragraph, Run, RunKind, RunProps};
use deckcraft_model::{Shape, ShapeId};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, has_slide, str_param};
use crate::{Result, Session};

/// The font equations are set in: PowerPoint's math font, which the font database stands in for
/// (Caladea) where it is not installed.
pub const MATH_FONT: &str = "Cambria Math";

/// The DrawingML run properties every OMML run carries, as PowerPoint writes them: the math font
/// by name (`xmlns:a` is the slide's).
const MATH_RUN_PROPS: &str = "<a:rPr><a:latin typeface=\"Cambria Math\"/></a:rPr>";

/// The size an equation run has when it names none.
const DEFAULT_SIZE: f64 = 18.0;

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
    let ratio = match (w, h) {
        (Some(w), _) if x.w > 1e-9 => w / x.w,
        (_, Some(h)) if x.h > 1e-9 => h / x.h,
        _ => return Ok(true),
    };
    if !ratio.is_finite() || ratio <= 0.0 {
        return Ok(true);
    }
    set_equation_size(s, id, size * ratio.clamp(0.01, 100.0))?;
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
            "{tex: TeX source} — at the insertion point (replacing the selection), or in a new text box",
            has_slide,
            insert
        ),
        cmd!(
            query "equation.at",
            "Equation at Caret",
            [],
            None,
            "{} → {tex, start, end} for the equation the caret touches or the selection covers, else null",
            has_slide,
            at
        ),
        cmd!(query "equation.check", "Check Equation", [], None, "{tex} → {ok, text, error}", always, check),
    ]
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let tex = str_param(p, "tex").ok_or_else(|| bad("insert.equation", "missing `tex`"))?;
    if tex.trim().is_empty() {
        return Err(bad("insert.equation", "the equation is empty"));
    }
    let (omml, text) = math::tex_to_omml_runs(tex, MATH_RUN_PROPS).map_err(|e| bad("insert.equation", e.to_string()))?;
    // A run needs a character to be laid out, selected and deleted.
    let text = if text.is_empty() { " ".to_string() } else { text };
    let run = Run { text, props: RunProps::default(), kind: RunKind::Math { omml } };
    if s.doc()?.selection.text.is_some() {
        return super::text::insert_math(s, run);
    }
    // A text box of its own, centred on the slide, that hugs the equation (no wrapping, so the
    // fit sets its width too): its handles then sit on the equation and scale it.
    let rect = super::insert::default_rect(s, 300.0, 50.0);
    let mut shape = super::insert::new_text_box(rect, "");
    if let Some(tb) = shape.text.as_mut() {
        let mut run = run;
        run.props.font = Some(MATH_FONT.into());
        let mut para = Paragraph::default();
        para.props.align = Some(Align::Center);
        para.runs = vec![run];
        tb.paragraphs = vec![para];
        tb.body.wrap = Some(false);
        // No insets: the box is exactly the equation, so sizing the box scales it exactly.
        tb.body.inset_l = Some(0.0);
        tb.body.inset_r = Some(0.0);
        tb.body.inset_t = Some(0.0);
        tb.body.inset_b = Some(0.0);
    }
    shape.name = "Equation".into();
    super::insert::place_text_box(s, "insert.equation", shape, false)
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
        return Ok(json!({"tex": tex, "start": [0, start], "end": [0, start + run.char_len()], "id": id}));
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
            return Ok(json!({"tex": tex, "start": [b.0, ra], "end": [b.0, rb]}));
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
