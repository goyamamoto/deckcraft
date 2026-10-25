//! Equations: Insert ▸ Equation from TeX (`insert.equation`), and what the Equation dialog asks
//! (`equation.at`, `equation.check`). The equation is stored as Office Math (OMML) with its
//! linear text, as PowerPoint stores it; `deckcraft-math` does the TeX, OMML and layout.

use deckcraft_math as math;
use deckcraft_model::text::{Align, Paragraph, Run, RunKind, RunProps};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, has_slide, str_param};
use crate::{Result, Session};

/// The font equations are set in: PowerPoint's math font, which the font database stands in for
/// (Caladea) where it is not installed.
pub const MATH_FONT: &str = "Cambria Math";

/// The DrawingML run properties every OMML run carries, as PowerPoint writes them: the math font
/// by name (`xmlns:a` is the slide's).
const MATH_RUN_PROPS: &str = "<a:rPr><a:latin typeface=\"Cambria Math\"/></a:rPr>";

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
    // A text box of its own, centred on the slide, the equation centred in it.
    let rect = super::insert::default_rect(s, 300.0, 50.0);
    let mut shape = super::insert::new_text_box(rect, "");
    if let Some(tb) = shape.text.as_mut() {
        let mut run = run;
        run.props.font = Some(MATH_FONT.into());
        let mut para = Paragraph::default();
        para.props.align = Some(Align::Center);
        para.runs = vec![run];
        tb.paragraphs = vec![para];
    }
    shape.name = "Equation".into();
    super::insert::place_text_box(s, "insert.equation", shape, false)
}

/// The equation the caret is in or beside, or that the selection covers (within its paragraph).
fn at(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let Some(t) = &st.selection.text else { return Ok(Value::Null) };
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
