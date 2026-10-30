use super::{Dialog, SlideApp, Ui, Value, buttons, dialog_unit, field, i18n, json, len_text, length_value, parse_len, put_dim, vec2};

const TAB_KINDS: [(&str, &str); 4] = [("l", "Left"), ("ctr", "Center"), ("r", "Right"), ("dec", "Decimal")];

/// Format › Tabs: the default spacing and the tab stops of the selected paragraphs, edited as a
/// list and applied together with OK. Positions show and take the measurement unit (`2.54 cm`
/// in Japan, `1"` in the United States); the list keeps them in inches internally.
pub(super) fn tabs_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    if !d.fields.contains_key("stops") {
        // Opened from Paragraph (or by `format.tabs` with no fields) the stops come with the
        // dialog; otherwise they are read from the caret's paragraph.
        let r = if d.params.get("tabs").is_some() { d.params.clone() } else { app.session.execute("format.ruler", &json!({})).unwrap_or_default() };
        let stops: Vec<String> = r["tabs"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| Some(format!("{}:{}", t["pos"].as_f64()? / 72.0, t["align"].as_str().unwrap_or("l")))).collect())
            .unwrap_or_default();
        d.fields.insert("stops".into(), stops.join("|"));
        let unit = app.session.units();
        d.fields.insert("unit".into(), unit.id().into());
        let default_pt = r["defaultTab"].as_f64().filter(|v| v.is_finite()).unwrap_or(72.0);
        put_dim(d, "default", default_pt, len_text(default_pt, unit));
    }
    let unit = dialog_unit(d);
    let mut stops: Vec<(f64, String)> = d
        .get("stops", "")
        .split('|')
        .filter_map(|x| {
            let (p, a) = x.split_once(':')?;
            Some((p.parse::<f64>().ok().filter(|v| v.is_finite())?, a.to_string()))
        })
        .collect();
    field(ui, d, tl!("Default tab stops:"), "default", "");
    ui.add_space(6.0);
    ui.label(tl!("Tab stops:"));
    let mut remove = None;
    egui::Grid::new("tab_stops").num_columns(3).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
        if stops.is_empty() {
            ui.label(tl!("(none)"));
            ui.end_row();
        }
        for (i, (pos, align)) in stops.iter().enumerate() {
            let at = len_text(pos * 72.0, unit);
            ui.label(&at);
            ui.label(i18n::tc("align", TAB_KINDS.iter().find(|k| k.0 == align).map(|k| k.1).unwrap_or("Left")));
            let tip = crate::i18n::fmt(tl!("Clear the tab at {position}"), &[("position", &at)]);
            if ui.small_button(tl!("Clear")).on_hover_text(tip).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        stops.remove(i);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let l = ui.label(tl!("Tab stop position:"));
        let mut v = d.get("newpos", "");
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(60.0)).labelled_by(l.id);
        d.fields.insert("newpos".into(), v);
    });
    let cur = d.get("newalign", "l");
    ui.horizontal(|ui| {
        ui.label(tl!("Alignment:"));
        for (k, label) in TAB_KINDS {
            if ui.radio(cur == k, i18n::tc("align", label)).clicked() {
                d.fields.insert("newalign".into(), k.into());
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button(tl!("Set")).clicked()
            && let Some(p) = parse_len(&d.get("newpos", ""), unit).map(|pt| pt / 72.0).filter(|v| v.is_finite() && *v >= 0.0 && *v <= 56.0)
        {
            stops.retain(|(q, _)| (q - p).abs() > 0.005);
            stops.push((p, d.get("newalign", "l")));
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            d.fields.insert("newpos".into(), String::new());
        }
        if ui.button(tl!("Clear All")).clicked() {
            stops.clear();
        }
    });
    d.fields.insert("stops".into(), stops.iter().map(|(p, a)| format!("{p}:{a}")).collect::<Vec<_>>().join("|"));
    let (ok, cancel) = buttons(ui, "OK");
    if ok {
        let set: Vec<Value> = stops.iter().map(|(p, a)| json!({"pos": p * 72.0, "align": a})).collect();
        let dt = length_value(d, "default", unit).filter(|v| v.is_finite()).unwrap_or(72.0).clamp(1.44, 4032.0);
        if let Err(e) = app.run("format.tabs", json!({"set": set, "defaultTab": dt})) {
            app.set_status(e);
        } else if let Some(ret) = d.params.get_mut("returnParams").and_then(Value::as_object_mut) {
            // Back in Paragraph, its Tabs… shows what was just applied.
            ret.insert("tabs".into(), json!(set));
            ret.insert("defaultTab".into(), json!(dt));
        }
    }
    ok || cancel
}
