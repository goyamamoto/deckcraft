use super::{
    Align, Anchor, AutoFit, Bullet, ParaProps, Result, Session, Spacing, TextDir, Value, bad, bool_or, color_param, current_para, f64_or, f64_param,
    format_body, format_para, json, ok, on_param, resolve, str_param,
};

pub(super) fn align_to(s: &mut Session, a: Align) -> Result<Value> {
    format_para(s, &move |p| p.props.align = Some(a))
}

pub(super) fn align(s: &mut Session, p: &Value) -> Result<Value> {
    let a = match str_param(p, "align").unwrap_or("left") {
        "center" | "centre" | "ctr" => Align::Center,
        "right" | "r" => Align::Right,
        "justify" | "just" => Align::Justify,
        "justifyLow" | "justLow" => Align::JustLow,
        "distributed" | "dist" => Align::Distributed,
        _ => Align::Left,
    };
    align_to(s, a)
}

/// Paragraph reading direction. Switching it mirrors left and right alignment, so text that sat
/// at the start edge stays there.
pub(super) fn rtl(s: &mut Session, p: &Value) -> Result<Value> {
    let on = on_param(p, current_para(s).rtl);
    format_para(s, &move |para| {
        if para.props.rtl.unwrap_or(false) != on {
            para.props.align = match para.props.align {
                None | Some(Align::Left) => Some(Align::Right),
                Some(Align::Right) => Some(Align::Left),
                other => other,
            };
        }
        para.props.rtl = Some(on);
    })
}

pub(super) fn bullets(s: &mut Session, p: &Value) -> Result<Value> {
    let cur = matches!(current_para(s).bullet, Some(Bullet::Char { .. }));
    let on = on_param(p, Some(cur));
    let ch = str_param(p, "char").unwrap_or("•").to_string();
    let col = color_param(p, "color");
    let size = f64_param(p, "size").map(|v| (v / 100.0).clamp(0.25, 4.0));
    let explicit_char = p.get("char").is_some();
    format_para(s, &move |para| {
        if on {
            para.props.bullet = Some(Bullet::Char { char: ch.clone() });
            para.props.bullet_font = if explicit_char { Some("Arial".into()) } else { para.props.bullet_font.clone() };
            if col.is_some() {
                para.props.bullet_color = col.clone();
            }
            if size.is_some() {
                para.props.bullet_size = size;
            }
            if para.props.indent.is_none_or(|i| i >= 0.0) && para.props.margin_left.is_none_or(|m| m == 0.0) {
                para.props.margin_left = Some(18.0 + 36.0 * para.level as f64);
                para.props.indent = Some(-18.0);
            }
        } else {
            para.props.bullet = Some(Bullet::None);
            para.props.indent = Some(0.0);
            para.props.margin_left = Some(36.0 * para.level as f64);
        }
    })
}

/// Every automatic numbering scheme DrawingML defines (ECMA-376 `ST_TextAutonumberScheme`).
pub const AUTONUM_SCHEMES: &[&str] = &[
    "alphaLcParenBoth",
    "alphaUcParenBoth",
    "alphaLcParenR",
    "alphaUcParenR",
    "alphaLcPeriod",
    "alphaUcPeriod",
    "arabicParenBoth",
    "arabicParenR",
    "arabicPeriod",
    "arabicPlain",
    "romanLcParenBoth",
    "romanUcParenBoth",
    "romanLcParenR",
    "romanUcParenR",
    "romanLcPeriod",
    "romanUcPeriod",
    "circleNumDbPlain",
    "circleNumWdBlackPlain",
    "circleNumWdWhitePlain",
    "arabicDbPeriod",
    "arabicDbPlain",
    "ea1ChsPeriod",
    "ea1ChsPlain",
    "ea1ChtPeriod",
    "ea1ChtPlain",
    "ea1JpnChsDbPeriod",
    "ea1JpnKorPlain",
    "ea1JpnKorPeriod",
    "arabic1Minus",
    "arabic2Minus",
    "hebrew2Minus",
    "thaiAlphaPeriod",
    "thaiAlphaParenR",
    "thaiAlphaParenBoth",
    "thaiNumPeriod",
    "thaiNumParenR",
    "thaiNumParenBoth",
    "hindiAlphaPeriod",
    "hindiNumPeriod",
    "hindiNumParenR",
    "hindiAlpha1Period",
];

/// Home ▸ Paragraph ▸ Numbering's gallery, in order: the seven Latin schemes, then the East
/// Asian ones PowerPoint adds with East Asian editing (circled ①②③, full-width １．２．３． and
/// kanji 一．二．三．). `true` marks the East Asian entries.
pub const NUMBERING_GALLERY: &[(&str, bool)] = &[
    ("arabicPeriod", false),
    ("arabicParenR", false),
    ("romanUcPeriod", false),
    ("romanLcPeriod", false),
    ("alphaUcPeriod", false),
    ("alphaLcParenR", false),
    ("alphaLcPeriod", false),
    ("circleNumDbPlain", true),
    ("arabicDbPeriod", true),
    ("ea1JpnChsDbPeriod", true),
];

pub(super) fn numbering_schemes(_s: &mut Session, _p: &Value) -> Result<Value> {
    let schemes: Vec<Value> = NUMBERING_GALLERY
        .iter()
        .map(|&(scheme, east_asian)| {
            let sample = (1..=3).map(|n| deckcraft_text::autonum(scheme, n)).collect::<Vec<_>>().join(" ");
            json!({"scheme": scheme, "sample": sample, "eastAsian": east_asian})
        })
        .collect();
    Ok(json!({ "schemes": schemes }))
}

pub(super) fn numbering(s: &mut Session, p: &Value) -> Result<Value> {
    let cur = matches!(current_para(s).bullet, Some(Bullet::AutoNum { .. }));
    let on = on_param(p, Some(cur));
    let scheme = match p.get("scheme") {
        None => "arabicPeriod".to_string(),
        Some(Value::String(name)) => AUTONUM_SCHEMES
            .iter()
            .find(|k| k.eq_ignore_ascii_case(name.trim()))
            .map(|k| (*k).to_string())
            .ok_or_else(|| bad("format.numbering", format!("unknown numbering `scheme` `{name}` (see format.numberingSchemes)")))?,
        Some(_) => return Err(bad("format.numbering", "`scheme` must be a string")),
    };
    let start = p.get("start").and_then(Value::as_u64).map(|v| v.clamp(1, 32767) as u32).unwrap_or(1);
    let col = color_param(p, "color");
    let size = f64_param(p, "size").map(|v| (v / 100.0).clamp(0.25, 4.0));
    format_para(s, &move |para| {
        if on {
            para.props.bullet = Some(Bullet::AutoNum { scheme: scheme.clone(), start_at: start });
            if col.is_some() {
                para.props.bullet_color = col.clone();
            }
            if size.is_some() {
                para.props.bullet_size = size;
            }
            para.props.margin_left = Some(27.0 + 36.0 * para.level as f64);
            para.props.indent = Some(-27.0);
        } else {
            para.props.bullet = Some(Bullet::None);
            para.props.indent = Some(0.0);
            para.props.margin_left = Some(36.0 * para.level as f64);
        }
    })
}

pub(super) fn level(s: &mut Session, d: i32) -> Result<Value> {
    format_para(s, &move |p| {
        let l = (p.level as i32 + d).clamp(0, 8) as u8;
        // Explicit indents shift with the level.
        if l != p.level {
            if let Some(m) = p.props.margin_left.as_mut() {
                *m = (*m + 36.0 * d as f64).max(0.0);
            }
            p.level = l;
        }
    })
}

pub(super) fn line_spacing(s: &mut Session, p: &Value) -> Result<Value> {
    let sp =
        if let Some(pt) = f64_param(p, "pt") { Spacing::Pts(pt.clamp(0.0, 1584.0)) } else { Spacing::Pct(f64_or(p, "lines", 1.0).clamp(0.0, 9.99)) };
    format_para(s, &move |para| para.props.line_spacing = Some(sp))
}

/// Home ▸ Paragraph ▸ Paragraph… ▸ Asian Typography, as PowerPoint offers it with East Asian
/// editing: East Asian line-breaking rules for the first and last characters of a line (kinsoku,
/// `eaLnBrk`), Latin words that may break anywhere (`latinLnBrk`), closing punctuation that may
/// hang past the margin (`hangingPunct`), and how runs of different sizes line up (`fontAlgn`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AsianTypography {
    east_asian_line_break: Option<bool>,
    latin_line_break: Option<bool>,
    hanging_punct: Option<bool>,
    font_align: Option<&'static str>,
}

const ASIAN_TYPOGRAPHY_KEYS: &[&str] = &["eaLnBrk", "latinLnBrk", "hangingPunct", "fontAlgn"];

impl AsianTypography {
    fn parse(p: &Value, cmd: &str) -> Result<Self> {
        let flag = |key: &str| match p.get(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(bad(cmd, format!("`{key}` must be a bool"))),
        };
        let font_align = match p.get("fontAlgn") {
            None => None,
            Some(Value::String(a)) => Some(match a.as_str() {
                "auto" => "auto",
                "t" | "top" => "t",
                "ctr" | "center" | "middle" => "ctr",
                "base" | "baseline" => "base",
                "b" | "bottom" => "b",
                _ => return Err(bad(cmd, "`fontAlgn` must be auto, t, ctr, base or b")),
            }),
            Some(_) => return Err(bad(cmd, "`fontAlgn` must be a string")),
        };
        Ok(AsianTypography {
            east_asian_line_break: flag("eaLnBrk")?,
            latin_line_break: flag("latinLnBrk")?,
            hanging_punct: flag("hangingPunct")?,
            font_align,
        })
    }

    fn write(&self, pp: &mut ParaProps) {
        if self.east_asian_line_break.is_some() {
            pp.east_asian_line_break = self.east_asian_line_break;
        }
        if self.latin_line_break.is_some() {
            pp.latin_line_break = self.latin_line_break;
        }
        if self.hanging_punct.is_some() {
            pp.hanging_punct = self.hanging_punct;
        }
        if let Some(a) = self.font_align {
            pp.font_align = Some(a.to_string());
        }
    }
}

/// The Asian Typography values a paragraph shows (resolved): PowerPoint's defaults where nothing
/// says otherwise — kinsoku and hanging punctuation on, Latin words kept whole, automatic
/// alignment.
fn asian_typography_params(pp: &ParaProps) -> Value {
    json!({
        "eaLnBrk": pp.east_asian_line_break.unwrap_or(true),
        "latinLnBrk": pp.latin_line_break.unwrap_or(false),
        "hangingPunct": pp.hanging_punct.unwrap_or(true),
        "fontAlgn": pp.font_align.as_deref().filter(|a| matches!(*a, "auto" | "t" | "ctr" | "base" | "b")).unwrap_or("auto"),
    })
}

pub(super) fn paragraph(s: &mut Session, p: &Value) -> Result<Value> {
    let asian = AsianTypography::parse(p, "format.paragraph")?;
    let a = str_param(p, "align").map(|a| match a {
        "center" => Align::Center,
        "right" => Align::Right,
        "justify" => Align::Justify,
        "distributed" => Align::Distributed,
        _ => Align::Left,
    });
    let ml = f64_param(p, "indentLeft");
    let ind = f64_param(p, "indentFirst");
    let sb = f64_param(p, "spaceBefore");
    let sa = f64_param(p, "spaceAfter");
    let ls = f64_param(p, "lineSpacing");
    let lsp = f64_param(p, "lineSpacingPt");
    format_para(s, &move |para| {
        if a.is_some() {
            para.props.align = a;
        }
        if ml.is_some() {
            para.props.margin_left = ml.map(|v| v.max(0.0));
        }
        if ind.is_some() {
            para.props.indent = ind;
        }
        if let Some(v) = sb {
            para.props.space_before = Some(Spacing::Pts(v.max(0.0)));
        }
        if let Some(v) = sa {
            para.props.space_after = Some(Spacing::Pts(v.max(0.0)));
        }
        if let Some(v) = ls {
            para.props.line_spacing = Some(Spacing::Pct(v.clamp(0.0, 9.99)));
        }
        if let Some(v) = lsp {
            para.props.line_spacing = Some(Spacing::Pts(v.max(0.0)));
        }
        asian.write(&mut para.props);
    })
}

const PARA_DIALOG_KEYS: &[&str] =
    &["align", "indentLeft", "indentRight", "indentFirst", "special", "specialBy", "spaceBefore", "spaceAfter", "lineSpacing", "lineSpacingPt"];

fn para_dialog_has_fields(p: &Value) -> bool {
    PARA_DIALOG_KEYS.iter().chain(ASIAN_TYPOGRAPHY_KEYS).any(|k| p.get(*k).is_some())
}

pub(super) fn finite_key(p: &Value, cmd: &str, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(cmd, format!("`{key}` must be a finite number")))?;
            Ok(Some(n))
        }
    }
}

pub(super) fn clamp_pt(v: f64) -> f64 {
    v.clamp(0.0, 1584.0)
}

fn parse_align(cmd: &str, s: &str) -> Result<Align> {
    Ok(match s {
        "left" | "l" => Align::Left,
        "center" | "ctr" => Align::Center,
        "right" | "r" => Align::Right,
        "justify" | "just" => Align::Justify,
        "justLow" => Align::JustLow,
        "distributed" | "dist" => Align::Distributed,
        _ => return Err(bad(cmd, "`align` must be left, center, right, justify, justLow or distributed")),
    })
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
        Align::JustLow => "justLow",
        Align::Distributed => "distributed",
    }
}

fn spacing_parts(s: Option<Spacing>) -> (&'static str, f64) {
    match s {
        Some(Spacing::Pct(v)) => ("pct", v),
        Some(Spacing::Pts(v)) => ("pts", v),
        None => ("pts", 0.0),
    }
}

fn paragraph_dialog_params(s: &Session) -> Value {
    let pp = current_para(s);
    let indent = pp.indent.unwrap_or(0.0);
    let (special, by) = if indent > 0.05 {
        ("first", indent)
    } else if indent < -0.05 {
        ("hanging", -indent)
    } else {
        ("none", 0.0)
    };
    let (line_kind, line_value) = match pp.line_spacing {
        Some(Spacing::Pts(v)) => ("exactly", v),
        Some(Spacing::Pct(v)) => ("multiple", v),
        None => ("multiple", 1.0),
    };
    let (before_kind, before) = spacing_parts(pp.space_before);
    let (after_kind, after) = spacing_parts(pp.space_after);
    let tabs: Vec<Value> = pp.tabs.clone().unwrap_or_default().iter().map(|t| json!({"pos": t.pos, "align": t.align})).collect();
    let mut v = json!({
        "align": align_name(pp.align.unwrap_or(Align::Left)),
        "indentLeft": pp.margin_left.unwrap_or(0.0),
        "indentRight": pp.margin_right.unwrap_or(0.0),
        "special": special,
        "specialBy": by,
        "spaceBefore": before,
        "spaceBeforeKind": before_kind,
        "spaceAfter": after,
        "spaceAfterKind": after_kind,
        "lineKind": line_kind,
        "lineValue": line_value,
        "tabs": tabs,
        "defaultTab": pp.default_tab.unwrap_or(72.0),
    });
    if let (Some(o), Value::Object(asian)) = (v.as_object_mut(), asian_typography_params(&pp)) {
        o.extend(asian);
    }
    v
}

fn write_spacing(kind: Option<&str>, value: f64) -> Spacing {
    if kind == Some("pct") { Spacing::Pct(value.clamp(0.0, 10.0)) } else { Spacing::Pts(clamp_pt(value)) }
}

pub(super) fn paragraph_dialog(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "format.paragraphDialog";
    if !para_dialog_has_fields(p) {
        s.ui_requests.push(crate::UiRequest::Dialog { id: "paragraph".into(), params: paragraph_dialog_params(s) });
        return ok();
    }
    let align = match p.get("align") {
        None => None,
        Some(Value::String(a)) => Some(parse_align(CMD, a)?),
        Some(_) => return Err(bad(CMD, "`align` must be a string")),
    };
    let indent_left = finite_key(p, CMD, "indentLeft")?.map(clamp_pt);
    let indent_right = finite_key(p, CMD, "indentRight")?.map(clamp_pt);
    let special_by = finite_key(p, CMD, "specialBy")?.map(|v| clamp_pt(v.max(0.0)));
    let indent = match p.get("special") {
        None => finite_key(p, CMD, "indentFirst")?.map(|v| v.clamp(-1584.0, 1584.0)),
        Some(Value::String(kind)) => {
            let by = special_by.unwrap_or(0.0);
            Some(match kind.as_str() {
                "none" => 0.0,
                "first" | "firstLine" => by,
                "hanging" | "hang" => -by,
                _ => return Err(bad(CMD, "`special` must be none, first or hanging")),
            })
        }
        Some(_) => return Err(bad(CMD, "`special` must be none, first or hanging")),
    };
    let space_before = finite_key(p, CMD, "spaceBefore")?;
    let space_after = finite_key(p, CMD, "spaceAfter")?;
    let before_kind = str_param(p, "spaceBeforeKind");
    let after_kind = str_param(p, "spaceAfterKind");
    for kind in [before_kind, after_kind].into_iter().flatten() {
        if !matches!(kind, "pts" | "pct") {
            return Err(bad(CMD, "spacing kind must be pts or pct"));
        }
    }
    let line_mult = finite_key(p, CMD, "lineSpacing")?.map(|v| v.clamp(0.0, 9.99));
    let line_pt = finite_key(p, CMD, "lineSpacingPt")?.map(clamp_pt);
    let asian = AsianTypography::parse(p, CMD)?;
    format_para(s, &move |para| {
        asian.write(&mut para.props);
        if align.is_some() {
            para.props.align = align;
        }
        if indent_left.is_some() {
            para.props.margin_left = indent_left;
        }
        if indent_right.is_some() {
            para.props.margin_right = indent_right;
        }
        if indent.is_some() {
            para.props.indent = indent;
        }
        if let Some(v) = space_before {
            para.props.space_before = Some(write_spacing(before_kind, v));
        }
        if let Some(v) = space_after {
            para.props.space_after = Some(write_spacing(after_kind, v));
        }
        if let Some(v) = line_mult {
            para.props.line_spacing = Some(Spacing::Pct(v));
        }
        if let Some(v) = line_pt {
            para.props.line_spacing = Some(Spacing::Pts(v));
        }
    })
}

pub(super) fn tab_align(s: &str) -> Option<&'static str> {
    Some(match s {
        "l" | "left" => "l",
        "ctr" | "center" => "ctr",
        "r" | "right" => "r",
        "dec" | "decimal" => "dec",
        _ => return None,
    })
}

pub(super) fn tabs_params(s: &Session) -> Value {
    let pp = current_para(s);
    let stops: Vec<Value> = pp.tabs.unwrap_or_default().iter().map(|t| json!({"pos": t.pos, "align": t.align})).collect();
    json!({"tabs": stops, "defaultTab": pp.default_tab.unwrap_or(72.0)})
}

pub(super) fn anchor(s: &mut Session, p: &Value) -> Result<Value> {
    let a = match str_param(p, "anchor").unwrap_or("top") {
        "middle" | "center" | "ctr" => Anchor::Middle,
        "bottom" | "b" => Anchor::Bottom,
        _ => Anchor::Top,
    };
    format_body(s, &move |b| b.anchor = Some(a))
}

/// A Text Direction name: PowerPoint's choices by the names `format.direction` documents, or
/// the `bodyPr vert` value itself.
fn parse_dir(s: &str) -> Option<TextDir> {
    Some(match s {
        "horizontal" => TextDir::Horizontal,
        "vertical" => TextDir::EaVertical,
        "rotate90" => TextDir::Vertical,
        "rotate270" => TextDir::Vertical270,
        "stacked" => TextDir::Stacked,
        "stackedRtl" => TextDir::StackedRtl,
        other => return TextDir::from_xml(other),
    })
}

/// The `format.direction` name of a text direction.
pub(super) fn dir_name(d: TextDir) -> &'static str {
    match d {
        TextDir::Horizontal => "horizontal",
        TextDir::EaVertical => "vertical",
        TextDir::Vertical => "rotate90",
        TextDir::Vertical270 => "rotate270",
        TextDir::Stacked => "stacked",
        TextDir::StackedRtl => "stackedRtl",
        TextDir::MongolianVertical => "mongolianVert",
    }
}

/// Home ▸ Paragraph ▸ Text Direction. With East Asian editing PowerPoint adds Vertical (`eaVert`:
/// kana and kanji upright, Latin text turned, columns from right to left) and the stacked form
/// that keeps every character upright with columns from right to left (`wordArtVertRtl`).
pub(super) fn direction(s: &mut Session, p: &Value) -> Result<Value> {
    let name = match p.get("dir") {
        None => "horizontal",
        Some(Value::String(d)) => d.as_str(),
        Some(_) => return Err(bad("format.direction", "`dir` must be a string")),
    };
    let d = parse_dir(name).ok_or_else(|| {
        bad("format.direction", format!("unknown `dir` `{name}`: horizontal, vertical, rotate90, rotate270, stacked, stackedRtl or mongolianVert"))
    })?;
    format_body(s, &move |b| b.vert = Some(d))
}

/// The edited or first selected shape's columns as drawn: the count and the spacing in points.
fn current_columns(s: &Session) -> (u32, f64) {
    let Ok(st) = s.doc() else { return (1, 0.0) };
    let id = match &st.selection.text {
        Some(t) if !t.notes => Some(t.shape),
        _ => st.selection.shapes.first().copied(),
    };
    let Some(sh) = id.and_then(|id| st.shape(id)) else { return (1, 0.0) };
    let body = match crate::cmd::text::ctx_for(&st.doc, &st.selection) {
        Some(c) => resolve::body(&c, sh),
        None => sh.text.as_ref().map(|b| b.body.clone()).unwrap_or_default(),
    };
    (body.columns.unwrap_or(1).clamp(1, 16), body.col_spacing.unwrap_or(0.0).max(0.0))
}

pub(super) fn columns(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "format.columns";
    let count = match p.get("count") {
        None => None,
        Some(v) => {
            let n = v
                .as_f64()
                .filter(|n| n.fract() == 0.0 && (1.0..=16.0).contains(n))
                .ok_or_else(|| bad(CMD, "`count` must be a whole number from 1 to 16"))?;
            Some(n as u32)
        }
    };
    let spacing = finite_key(p, CMD, "spacing")?.map(clamp_pt);
    if count.is_none() && spacing.is_none() {
        let (count, spacing) = current_columns(s);
        s.ui_requests.push(crate::UiRequest::Dialog { id: "columns".into(), params: json!({"count": count, "spacing": spacing}) });
        return ok();
    }
    format_body(s, &move |b| {
        if let Some(n) = count {
            b.columns = Some(n);
        }
        if let Some(sp) = spacing {
            b.col_spacing = Some(sp);
        }
    })
}

pub(super) fn autofit(s: &mut Session, p: &Value) -> Result<Value> {
    let m = match str_param(p, "mode").unwrap_or("none") {
        "shrink" => AutoFit::Shrink { font_scale: Some(1.0), line_reduction: 0.0 },
        "resize" | "shape" => AutoFit::Shape,
        _ => AutoFit::None,
    };
    format_body(s, &move |b| b.autofit = Some(m))
}

pub(super) fn wrap(s: &mut Session, p: &Value) -> Result<Value> {
    let on = bool_or(p, "on", true);
    format_body(s, &move |b| b.wrap = Some(on))
}

pub(super) fn margins(s: &mut Session, p: &Value) -> Result<Value> {
    let (l, t, r, b) = (f64_param(p, "left"), f64_param(p, "top"), f64_param(p, "right"), f64_param(p, "bottom"));
    format_body(s, &move |bp| {
        if l.is_some() {
            bp.inset_l = l.map(|v| v.max(0.0));
        }
        if t.is_some() {
            bp.inset_t = t.map(|v| v.max(0.0));
        }
        if r.is_some() {
            bp.inset_r = r.map(|v| v.max(0.0));
        }
        if b.is_some() {
            bp.inset_b = b.map(|v| v.max(0.0));
        }
    })
}
