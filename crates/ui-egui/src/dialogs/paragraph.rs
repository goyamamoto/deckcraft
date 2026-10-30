use super::{
    Dialog, SlideApp, Ui, Value, buttons, check, choice, dialog_unit, finite_pt, json, json_text, labeled_text, length_value, pt_text, seed_length,
    seed_once, vec2,
};

/// An indent field's length in points (`None` when the text isn't a length).
fn indent_value(d: &Dialog, key: &str) -> Option<f64> {
    length_value(d, key, dialog_unit(d))
}

fn seed_paragraph_fields(d: &mut Dialog, unit: deckcraft_geom::Unit) {
    seed_once(d, "unit", unit.id().to_string());
    let unit = dialog_unit(d);
    seed_once(d, "align", json_text(d.params.get("align"), "left"));
    seed_length(d, "before", d.params.get("indentLeft").cloned().as_ref(), unit);
    seed_length(d, "after", d.params.get("indentRight").cloned().as_ref(), unit);
    seed_once(d, "special", json_text(d.params.get("special"), "none"));
    seed_length(d, "by", d.params.get("specialBy").cloned().as_ref(), unit);
    seed_once(d, "spaceBefore", pt_text(d.params.get("spaceBefore"), 0.0));
    seed_once(d, "spaceBeforeSeed", pt_text(d.params.get("spaceBefore"), 0.0));
    seed_once(d, "spaceAfter", pt_text(d.params.get("spaceAfter"), 0.0));
    seed_once(d, "spaceAfterSeed", pt_text(d.params.get("spaceAfter"), 0.0));
    seed_once(d, "lineKind", json_text(d.params.get("lineKind"), "multiple"));
    seed_once(d, "lineValue", pt_text(d.params.get("lineValue"), 1.0));
    seed_once(d, "page", json_text(d.params.get("page"), "indents"));
    // Asian Typography, as the paragraph has it. The seeds are what the dialog opened with (kept
    // across a trip to Tabs…): OK sends only the options that were changed.
    for (key, default) in ASIAN_TYPOGRAPHY_DEFAULTS {
        let value = json_text(d.params.get(key), default);
        seed_once(d, &format!("{key}Seed"), json_text(d.params.get(format!("{key}Seed").as_str()), &value));
        seed_once(d, key, value);
    }
}

/// The paragraph's Asian Typography options (`format.paragraphDialog` keys) and the dialog's value
/// for each when the paragraph doesn't say (PowerPoint's defaults): `1`/`0` for the switches.
const ASIAN_TYPOGRAPHY_DEFAULTS: [(&str, &str); 4] = [("eaLnBrk", "1"), ("latinLnBrk", "0"), ("hangingPunct", "1"), ("fontAlgn", "auto")];

/// The Asian Typography options changed since the dialog opened, as `format.paragraphDialog` params.
fn asian_typography_params(d: &mut Dialog) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (key, default) in ASIAN_TYPOGRAPHY_DEFAULTS {
        let value = d.get(key, default);
        if value == d.get(&format!("{key}Seed"), default) {
            continue;
        }
        let v = if key == "fontAlgn" { json!(value) } else { json!(value == "1") };
        out.insert(key.to_string(), v);
    }
    out
}

/// Text alignment's choices on the Asian Typography page, as (`fontAlgn`, English label): how
/// characters of different sizes on a line line up.
const FONT_ALIGNS: [(&str, &str); 5] = [("auto", "Auto"), ("t", "Top"), ("ctr", "Center"), ("base", "Baseline"), ("b", "Bottom")];

/// The Paragraph dialog's pages: Indents and Spacing, and with East Asian editing Asian
/// Typography. Returns whether the Asian Typography page is showing.
fn paragraph_pages(app: &SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    if !crate::ribbon::east_asian(app) {
        return false;
    }
    let page = d.get("page", "indents");
    ui.horizontal(|ui| {
        for (k, l) in [("indents", "Indents and Spacing"), ("asian", "Asian Typography")] {
            if ui.selectable_label(page == k, tl!(l)).clicked() {
                d.fields.insert("page".into(), k.into());
            }
        }
    });
    ui.separator();
    d.get("page", "indents") == "asian"
}

/// Paragraph ▸ Asian Typography: East Asian line breaking (kinsoku), Latin words breaking at any
/// letter, hanging punctuation, and how characters of different sizes line up.
fn asian_typography_page(ui: &mut Ui, d: &mut Dialog) {
    ui.label(egui::RichText::new(tl!("Line breaking")).strong());
    check(ui, d, tl!("Apply Asian line-breaking rules (kinsoku)"), "eaLnBrk", true);
    check(ui, d, tl!("Let Latin words break at any letter"), "latinLnBrk", false);
    check(ui, d, tl!("Hang punctuation past the line end"), "hangingPunct", true);
    ui.add_space(6.0);
    let choices: Vec<(&str, &str)> = FONT_ALIGNS.iter().map(|(k, l)| (*k, crate::i18n::tc("anchor", l))).collect();
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 20.0), egui::Label::new(tl!("Text alignment:")));
        choice(ui, d, "paragraph_font_align", tl!("Text alignment"), "fontAlgn", &choices);
    });
    ui.add_space(6.0);
}

fn paragraph_return_params(d: &mut Dialog) -> Value {
    let before = indent_value(d, "before").unwrap_or(0.0);
    let after = indent_value(d, "after").unwrap_or(0.0);
    let by = indent_value(d, "by").unwrap_or(0.0);
    let space_before = finite_pt(&d.get("spaceBefore", "0")).unwrap_or(0.0);
    let space_after = finite_pt(&d.get("spaceAfter", "0")).unwrap_or(0.0);
    let line_value = finite_pt(&d.get("lineValue", "1")).unwrap_or(1.0);
    let before_same = d.get("spaceBefore", "") == d.get("spaceBeforeSeed", "");
    let after_same = d.get("spaceAfter", "") == d.get("spaceAfterSeed", "");
    json!({
        "align": d.get("align", "left"),
        "indentLeft": before,
        "indentRight": after,
        "special": d.get("special", "none"),
        "specialBy": by,
        "spaceBefore": space_before,
        "spaceBeforeKind": if before_same { d.params.get("spaceBeforeKind").and_then(Value::as_str).unwrap_or("pts") } else { "pts" },
        "spaceAfter": space_after,
        "spaceAfterKind": if after_same { d.params.get("spaceAfterKind").and_then(Value::as_str).unwrap_or("pts") } else { "pts" },
        "lineKind": d.get("lineKind", "multiple"),
        "lineValue": line_value,
        "tabs": d.params.get("tabs").cloned().unwrap_or_else(|| json!([])),
        "defaultTab": d.params.get("defaultTab").cloned().unwrap_or_else(|| json!(72.0)),
        "page": d.get("page", "indents"),
        "eaLnBrk": d.get("eaLnBrk", "1") == "1",
        "eaLnBrkSeed": d.get("eaLnBrkSeed", "1") == "1",
        "latinLnBrk": d.get("latinLnBrk", "0") == "1",
        "latinLnBrkSeed": d.get("latinLnBrkSeed", "0") == "1",
        "hangingPunct": d.get("hangingPunct", "1") == "1",
        "hangingPunctSeed": d.get("hangingPunctSeed", "1") == "1",
        "fontAlgn": d.get("fontAlgn", "auto"),
        "fontAlgnSeed": d.get("fontAlgnSeed", "auto"),
    })
}

fn line_preset_label(kind: &str, value: &str) -> &'static str {
    if kind == "exactly" {
        "Exactly"
    } else {
        match value {
            "1" => "Single",
            "1.5" => "1.5 lines",
            "2" => "Double",
            _ => "Multiple",
        }
    }
}

/// Home › Paragraph launcher. One OK applies alignment, indentation and spacing together, and
/// with East Asian editing the Asian Typography page's options.
pub(super) fn paragraph_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    ui.set_min_width(460.0);
    seed_paragraph_fields(d, app.session.units());
    if paragraph_pages(app, ui, d) {
        asian_typography_page(ui, d);
        return paragraph_dialog_buttons(app, ui, d);
    }
    let aligns: Vec<(&str, &str)> = [
        ("left", "Left"),
        ("center", "Center"),
        ("right", "Right"),
        ("justify", "Justify"),
        ("justLow", "Justify low"),
        ("distributed", "Distributed"),
    ]
    .into_iter()
    .map(|(k, l)| (k, crate::i18n::tc("align", l)))
    .collect();
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 20.0), egui::Label::new(tl!("Alignment")));
        choice(ui, d, "paragraph_align", tl!("Alignment"), "align", &aligns);
    });
    ui.label(egui::RichText::new(tl!("Indentation")).strong());
    // The indents show their unit in the field (`1.27 cm`, `0.5"`).
    ui.horizontal(|ui| {
        let _ = labeled_text(ui, d, tl!("Before text"), "before", 72.0);
        ui.add_space(12.0);
        let _ = labeled_text(ui, d, tl!("After text"), "after", 72.0);
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 20.0), egui::Label::new(tl!("Special")));
        choice(
            ui,
            d,
            "paragraph_special",
            tl!("Special"),
            "special",
            &[("none", tl!("None")), ("first", tl!("First line")), ("hanging", tl!("Hanging"))],
        );
        ui.add_space(8.0);
        let _ = labeled_text(ui, d, tl!("By"), "by", 72.0);
    });
    ui.label(egui::RichText::new(tl!("Spacing")).strong());
    ui.horizontal(|ui| {
        let _ = labeled_text(ui, d, tl!("Spacing before"), "spaceBefore", 64.0);
        ui.label("pt");
        ui.add_space(12.0);
        let _ = labeled_text(ui, d, tl!("Spacing after"), "spaceAfter", 64.0);
        ui.label("pt");
    });
    let mut picked: Option<(&'static str, Option<&'static str>)> = None;
    ui.horizontal(|ui| {
        let field = ui.add_sized(vec2(110.0, 20.0), egui::Label::new(tl!("Line spacing")));
        let kind = d.get("lineKind", "multiple");
        let value = d.get("lineValue", "1");
        let shown = |label: &str| crate::i18n::tc("lineSpacing", label).to_string();
        let selected = shown(line_preset_label(&kind, &value));
        let combo = craft_ui::ComboBox::from_id_salt("paragraph_line").selected_text(selected).width(140.0).show_ui(ui, |ui| {
            for (label, k, v) in [
                ("Single", "multiple", Some("1")),
                ("1.5 lines", "multiple", Some("1.5")),
                ("Double", "multiple", Some("2")),
                ("Multiple", "multiple", None),
                ("Exactly", "exactly", None),
            ] {
                if ui.selectable_label(line_preset_label(&kind, &value) == label, shown(label)).clicked() {
                    picked = Some((k, v));
                }
            }
        });
        let _ = combo.response.labelled_by(field.id);
        ui.add_space(8.0);
        let _ = labeled_text(ui, d, tl!("At"), "lineValue", 64.0);
    });
    if let Some((kind, value)) = picked {
        d.fields.insert("lineKind".into(), kind.into());
        if let Some(v) = value {
            d.fields.insert("lineValue".into(), v.into());
        } else if kind == "exactly" && finite_pt(&d.get("lineValue", "1")).is_some_and(|n| n <= 9.99) {
            d.fields.insert("lineValue".into(), "18".into());
        } else if kind == "multiple" && finite_pt(&d.get("lineValue", "1")).is_some_and(|n| n > 9.99) {
            d.fields.insert("lineValue".into(), "1".into());
        }
    }
    paragraph_dialog_buttons(app, ui, d)
}

/// The Paragraph dialog's Tabs…, OK and Cancel, on either page: OK applies both pages in one step.
fn paragraph_dialog_buttons(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let mut open_tabs = false;
    ui.horizontal(|ui| {
        if ui.button(tl!("Tabs…")).clicked() {
            open_tabs = true;
        }
    });
    let (ok, cancel) = buttons(ui, "OK");
    if open_tabs {
        let mut tabs = Dialog::new("tabs");
        tabs.params = json!({
            "tabs": d.params.get("tabs").cloned().unwrap_or_else(|| json!([])),
            "defaultTab": d.params.get("defaultTab").cloned().unwrap_or_else(|| json!(72.0)),
            "returnId": "paragraph",
            "returnParams": paragraph_return_params(d),
        });
        app.dialog = Some(tabs);
        return true;
    }
    if ok {
        let Some(before_pt) = indent_value(d, "before") else {
            app.set_status("Indent before text must be a length");
            return false;
        };
        let Some(after_pt) = indent_value(d, "after") else {
            app.set_status("Indent after text must be a length");
            return false;
        };
        let Some(by_pt) = indent_value(d, "by") else {
            app.set_status("Special indent must be a length");
            return false;
        };
        let Some(space_before) = finite_pt(&d.get("spaceBefore", "")) else {
            app.set_status("Spacing before must be a number");
            return false;
        };
        let Some(space_after) = finite_pt(&d.get("spaceAfter", "")) else {
            app.set_status("Spacing after must be a number");
            return false;
        };
        let Some(line_value) = finite_pt(&d.get("lineValue", "")) else {
            app.set_status("Line spacing must be a number");
            return false;
        };
        let mut params = json!({
            "align": d.get("align", "left"),
            "indentLeft": before_pt,
            "indentRight": after_pt,
            "special": d.get("special", "none"),
            "specialBy": by_pt,
            "spaceBefore": space_before,
            "spaceAfter": space_after,
            "spaceBeforeKind": if d.get("spaceBefore", "") == d.get("spaceBeforeSeed", "") { d.params.get("spaceBeforeKind").and_then(Value::as_str).unwrap_or("pts") } else { "pts" },
            "spaceAfterKind": if d.get("spaceAfter", "") == d.get("spaceAfterSeed", "") { d.params.get("spaceAfterKind").and_then(Value::as_str).unwrap_or("pts") } else { "pts" },
        });
        if d.get("lineKind", "multiple") == "exactly" {
            params["lineSpacingPt"] = json!(line_value);
        } else {
            params["lineSpacing"] = json!(line_value);
        }
        if let Some(o) = params.as_object_mut() {
            o.extend(asian_typography_params(d));
        }
        if app.run("format.paragraphDialog", params).is_err() {
            return false;
        }
    }
    ok || cancel
}
