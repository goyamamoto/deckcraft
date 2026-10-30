use super::{
    Dialog, SlideApp, Ui, Value, buttons, check, dialog_unit, edited, exact_of, field_of, json, len_text, length_value, num_text, parse_len,
    parse_shown, put_dim, text_row,
};

fn publish_dim(d: &mut Dialog, key: &str, exact: f64, shown: String) {
    d.fields.insert(key.into(), shown.clone());
    d.fields.insert(format!("seed{key}"), shown);
    d.fields.insert(format!("{key}Pt"), exact.to_string());
}

fn selection_xfrm(app: &SlideApp) -> Option<deckcraft_geom::Xfrm> {
    let st = app.session.active()?;
    let id = *st.selection.shapes.first()?;
    let sh = st.shape(id)?;
    Some(deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, sh))
}

fn seed_size_position(app: &SlideApp, d: &mut Dialog) {
    if d.fields.contains_key("baseW") {
        return;
    }
    let num = |k: &str| d.params.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
    let sel = selection_xfrm(app);
    let x = num("x").or_else(|| sel.map(|b| b.x)).unwrap_or(0.0);
    let y = num("y").or_else(|| sel.map(|b| b.y)).unwrap_or(0.0);
    let w = num("w").or_else(|| sel.map(|b| b.w)).unwrap_or(72.0);
    let h = num("h").or_else(|| sel.map(|b| b.h)).unwrap_or(72.0);
    let rot = num("rot").or_else(|| sel.map(|b| b.rot)).unwrap_or(0.0);
    let sw = num("scaleW").unwrap_or(100.0);
    let sh = num("scaleH").unwrap_or(100.0);
    let lock = d.params.get("lockAspect").and_then(Value::as_bool).unwrap_or(false);
    // Lengths in the measurement unit, kept for as long as the dialog is open.
    let unit = app.session.units();
    d.fields.insert("unit".into(), unit.id().into());
    put_dim(d, "x", x, len_text(x, unit));
    put_dim(d, "y", y, len_text(y, unit));
    put_dim(d, "w", w, len_text(w, unit));
    put_dim(d, "h", h, len_text(h, unit));
    put_dim(d, "rot", rot, num_text(rot));
    put_dim(d, "scaleW", sw, num_text(sw));
    put_dim(d, "scaleH", sh, num_text(sh));
    d.fields.insert("baseW".into(), w.to_string());
    d.fields.insert("baseH".into(), h.to_string());
    let bit = if lock { "1" } else { "0" };
    d.fields.insert("lock".into(), bit.into());
    d.fields.insert("prevlock".into(), bit.into());
}

fn scale_of(pt: f64, base: f64) -> f64 {
    if base > 0.0 { pt / base * 100.0 } else { 100.0 }
}

fn current_points(d: &Dialog, key: &str) -> Option<f64> {
    length_value(d, key, dialog_unit(d))
}

/// Keep scale tied to size, and the other side tied when aspect is locked.
fn reconcile_size_fields(d: &mut Dialog) {
    let lock = field_of(d, "lock") == "1";
    let was_locked = field_of(d, "prevlock") == "1";
    let base_w = parse_shown(&field_of(d, "baseW")).unwrap_or(1.0);
    let base_h = parse_shown(&field_of(d, "baseH")).unwrap_or(1.0);
    let ar = if base_h > 0.0 { base_w / base_h } else { 1.0 };
    let unit = dialog_unit(d);
    let (w_s, h_s, sw_s, sh_s) = (field_of(d, "w"), field_of(d, "h"), field_of(d, "scaleW"), field_of(d, "scaleH"));
    let (prev_w, prev_h, prev_sw, prev_sh) = (field_of(d, "prevw"), field_of(d, "prevh"), field_of(d, "prevscaleW"), field_of(d, "prevscaleH"));
    if w_s != prev_w {
        if let Some(w_pt) = parse_len(&w_s, unit) {
            d.fields.insert("wPt".into(), w_pt.to_string());
            let pct = scale_of(w_pt, base_w);
            publish_dim(d, "scaleW", pct, num_text(pct));
            if lock {
                let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
                publish_dim(d, "h", h_pt, len_text(h_pt, unit));
                let hp = scale_of(h_pt, base_h);
                publish_dim(d, "scaleH", hp, num_text(hp));
            }
        }
    } else if h_s != prev_h {
        if let Some(h_pt) = parse_len(&h_s, unit) {
            d.fields.insert("hPt".into(), h_pt.to_string());
            let pct = scale_of(h_pt, base_h);
            publish_dim(d, "scaleH", pct, num_text(pct));
            if lock {
                let w_pt = h_pt * ar;
                publish_dim(d, "w", w_pt, len_text(w_pt, unit));
                let wp = scale_of(w_pt, base_w);
                publish_dim(d, "scaleW", wp, num_text(wp));
            }
        }
    } else if sw_s != prev_sw {
        if let Some(pct) = parse_shown(&sw_s) {
            let w_pt = base_w * pct / 100.0;
            d.fields.insert("scaleWPt".into(), pct.to_string());
            publish_dim(d, "w", w_pt, len_text(w_pt, unit));
            if lock {
                let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
                publish_dim(d, "h", h_pt, len_text(h_pt, unit));
                publish_dim(d, "scaleH", pct, num_text(pct));
            }
        }
    } else if sh_s != prev_sh {
        if let Some(pct) = parse_shown(&sh_s) {
            let h_pt = base_h * pct / 100.0;
            d.fields.insert("scaleHPt".into(), pct.to_string());
            publish_dim(d, "h", h_pt, len_text(h_pt, unit));
            if lock {
                let w_pt = h_pt * ar;
                publish_dim(d, "w", w_pt, len_text(w_pt, unit));
                publish_dim(d, "scaleW", pct, num_text(pct));
            }
        }
    } else if lock
        && !was_locked
        && let Some(w_pt) = current_points(d, "w")
    {
        let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
        publish_dim(d, "h", h_pt, len_text(h_pt, unit));
        let hp = scale_of(h_pt, base_h);
        publish_dim(d, "scaleH", hp, num_text(hp));
        let wp = scale_of(w_pt, base_w);
        publish_dim(d, "scaleW", wp, num_text(wp));
    }
    for key in ["x", "y", "w", "h", "rot", "scaleW", "scaleH"] {
        let v = field_of(d, key);
        d.fields.insert(format!("prev{key}"), v);
    }
    d.fields.insert("prevlock".into(), if lock { "1".into() } else { "0".into() });
}

/// A length field's value in points, or `error` (a whole message, translated when shown).
fn shown_points(d: &Dialog, key: &str, error: &str) -> Result<f64, String> {
    length_value(d, key, dialog_unit(d)).ok_or_else(|| error.to_string())
}

/// A number field's value, or `error` (a whole message, translated when shown).
fn shown_number(d: &Dialog, key: &str, error: &str) -> Result<f64, String> {
    if edited(d, key) {
        return parse_shown(&field_of(d, key)).ok_or_else(|| error.to_string());
    }
    exact_of(d, key).ok_or_else(|| error.to_string())
}

fn size_position_value(d: &Dialog) -> Result<Value, String> {
    let x = shown_points(d, "x", "Horizontal position must be a length")?;
    let y = shown_points(d, "y", "Vertical position must be a length")?;
    let rot = shown_number(d, "rot", "Rotation must be a number")?;
    let base_w = parse_shown(&field_of(d, "baseW")).unwrap_or(1.0);
    let base_h = parse_shown(&field_of(d, "baseH")).unwrap_or(1.0);
    let mut w = if edited(d, "w") {
        shown_points(d, "w", "Width must be a length")?
    } else if edited(d, "scaleW") {
        base_w * shown_number(d, "scaleW", "Scale width must be a number")? / 100.0
    } else {
        shown_points(d, "w", "Width must be a length")?
    };
    let mut h = if edited(d, "h") {
        shown_points(d, "h", "Height must be a length")?
    } else if edited(d, "scaleH") {
        base_h * shown_number(d, "scaleH", "Scale height must be a number")? / 100.0
    } else {
        shown_points(d, "h", "Height must be a length")?
    };
    if field_of(d, "lock") == "1" {
        let ar = if base_h > 0.0 { base_w / base_h } else { 1.0 };
        let width_drives = edited(d, "w") || edited(d, "scaleW") || (!edited(d, "h") && !edited(d, "scaleH"));
        if width_drives {
            h = if ar > 0.0 { w / ar } else { h };
        } else {
            w = h * ar;
        }
    }
    Ok(json!({"x": x, "y": y, "w": w, "h": h, "rot": rot, "lockAspect": field_of(d, "lock") == "1"}))
}

pub(super) fn size_position_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    seed_size_position(app, d);
    ui.label(egui::RichText::new(tl!("Size")).strong());
    text_row(ui, d, tl!("Height:"), "h");
    text_row(ui, d, tl!("Width:"), "w");
    text_row(ui, d, tl!("Rotation (°):"), "rot");
    text_row(ui, d, tl!("Scale height (%):"), "scaleH");
    text_row(ui, d, tl!("Scale width (%):"), "scaleW");
    check(ui, d, tl!("Lock aspect ratio"), "lock", false);
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Position")).strong());
    ui.label(tl!("From the top-left corner of the slide"));
    text_row(ui, d, crate::i18n::tc("position", "Horizontal:"), "x");
    text_row(ui, d, crate::i18n::tc("position", "Vertical:"), "y");
    reconcile_size_fields(d);
    let (ok, cancel) = buttons(ui, "OK");
    if ok {
        match size_position_value(d) {
            Ok(p) => return app.run("shape.sizePosition", p).is_ok(),
            Err(msg) => {
                app.set_status(msg);
                return false;
            }
        }
    }
    cancel
}
