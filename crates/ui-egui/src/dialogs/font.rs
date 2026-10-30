use super::*;

fn seed_font_fields(d: &mut Dialog) {
    seed_once(d, "family", json_text(d.params.get("family"), ""));
    seed_once(d, "familySeed", json_text(d.params.get("family"), ""));
    seed_once(d, "eastAsian", json_text(d.params.get("eastAsian"), ""));
    seed_once(d, "eastAsianSeed", json_text(d.params.get("eastAsian"), ""));
    seed_once(d, "size", json_text(d.params.get("size"), "18"));
    seed_once(d, "bold", json_text(d.params.get("bold"), "0"));
    seed_once(d, "italic", json_text(d.params.get("italic"), "0"));
    seed_once(d, "underline", json_text(d.params.get("underline"), "none"));
    seed_once(d, "strike", json_text(d.params.get("strike"), "none"));
    seed_once(d, "baseline", json_text(d.params.get("baseline"), "none"));
    seed_once(d, "caps", json_text(d.params.get("caps"), "none"));
    seed_once(d, "spacing", json_text(d.params.get("spacing"), "0"));
    let shown = d.params.get("color").map(color_label).unwrap_or_default();
    seed_once(d, "color", shown.clone());
    seed_once(d, "colorSeed", shown);
}

/// Home › Font › Font…, the shape menu and Cmd+T. One OK applies every field together.
pub(super) fn font_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    use crate::fontpick::Offer;
    ui.set_min_width(460.0);
    seed_font_fields(d);
    let list = crate::fontpick::font_list(app, ui.ctx());
    // As in PowerPoint with East Asian editing, Latin and Asian text take separate fonts. The
    // Asian box also shows when the text already has an Asian text font.
    let asian = list.east_asian || !d.get("eastAsianSeed", "").trim().is_empty();
    font_field(ui, d, &list, ("Latin text font:", 150.0), "Latin text font", "family", Offer::Latin, 280.0);
    if asian {
        font_field(ui, d, &list, ("Asian text font:", 150.0), "Asian text font", "eastAsian", Offer::EastAsian, 280.0);
    }
    ui.horizontal(|ui| {
        let _ = labeled_text(ui, d, tl!("Size"), "size", 64.0);
        ui.add_space(16.0);
        check(ui, d, tl!("Bold"), "bold", false);
        check(ui, d, tl!("Italic"), "italic", false);
    });
    let (none, single, double) = (tl!("None"), tl!("Single"), tl!("Double"));
    ui.horizontal(|ui| {
        ui.add_sized(vec2(88.0, 20.0), egui::Label::new(tl!("Underline")));
        choice(
            ui,
            d,
            "font_dialog_underline",
            tl!("Underline"),
            "underline",
            &[
                ("none", none),
                ("sng", single),
                ("dbl", double),
                ("heavy", tl!("Heavy")),
                ("dotted", tl!("Dotted")),
                ("dash", tl!("Dash")),
                ("wavy", tl!("Wavy")),
            ],
        );
        ui.add_space(8.0);
        let _ = labeled_text(ui, d, tl!("Color"), "color", 110.0);
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(88.0, 20.0), egui::Label::new(tl!("Strikethrough")));
        choice(ui, d, "font_dialog_strike", tl!("Strikethrough"), "strike", &[("none", none), ("sng", single), ("dbl", double)]);
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(88.0, 20.0), egui::Label::new(tl!("Baseline")));
        choice(
            ui,
            d,
            "font_dialog_baseline",
            tl!("Baseline"),
            "baseline",
            &[("none", none), ("super", tl!("Superscript")), ("sub", tl!("Subscript"))],
        );
        ui.add_space(8.0);
        ui.label(tl!("Caps"));
        choice(ui, d, "font_dialog_caps", tl!("Caps"), "caps", &[("none", none), ("small", tl!("Small caps")), ("all", tl!("All caps"))]);
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(88.0, 20.0), egui::Label::new(tl!("Spacing")));
        let mut spacing = d.get("spacing", "0");
        let edited = ui.add(egui::TextEdit::singleline(&mut spacing).desired_width(64.0).hint_text("pt"));
        let mut picked = None;
        let presets = [("-3", tl!("Very tight")), ("-1.5", tl!("Tight")), ("0", tl!("Normal")), ("3", tl!("Loose")), ("6", tl!("Very loose"))];
        let combo = egui::ComboBox::from_id_salt("font_dialog_spacing")
            .selected_text(presets.iter().find(|(v, _)| *v == spacing).map_or(spacing.as_str(), |(_, l)| *l))
            .width(120.0)
            .show_ui(ui, |ui| {
                for (v, l) in presets {
                    if ui.selectable_label(spacing == v, l).clicked() {
                        picked = Some(v.to_string());
                    }
                }
            });
        combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Spacing")));
        let _ = edited;
        if let Some(v) = picked {
            d.fields.insert("spacing".into(), v);
        } else {
            d.fields.insert("spacing".into(), spacing);
        }
    });
    let bold = d.get("bold", "0") == "1";
    let italic = d.get("italic", "0") == "1";
    let underline = d.get("underline", "none");
    let strike = d.get("strike", "none");
    let shown = finite_pt(&d.get("size", "18")).unwrap_or(18.0).clamp(12.0, 28.0) as f32;
    let sample_text = if asian { format!("AaBbCcDd  {}", east_asian_sample(app)) } else { "AaBbCcDd".to_string() };
    let mut sample = egui::RichText::new(sample_text).size(shown);
    if bold {
        sample = sample.strong();
    }
    if italic {
        sample = sample.italics();
    }
    if underline != "none" {
        sample = sample.underline();
    }
    if strike != "none" {
        sample = sample.strikethrough();
    }
    ui.add_space(6.0);
    ui.label(sample);
    let (ok, cancel) = buttons(ui, "OK");
    if ok {
        // The fonts only when they changed: OK must not turn theme fonts into fixed names, or set
        // an Asian text font the user didn't choose. The Latin text font sets only the Latin font.
        let family = d.get("family", "").trim().to_string();
        let family_changed = family != d.get("familySeed", "").trim();
        if family_changed && family.is_empty() {
            app.set_status("Choose a font");
            return false;
        }
        let east_asian = d.get("eastAsian", "").trim().to_string();
        let east_asian_changed = !east_asian.is_empty() && east_asian != d.get("eastAsianSeed", "").trim();
        let Some(size) = finite_pt(&d.get("size", "")) else {
            app.set_status("Font size must be a number");
            return false;
        };
        let Some(spacing) = finite_pt(&d.get("spacing", "0")) else {
            app.set_status("Character spacing must be a number");
            return false;
        };
        let mut params = json!({
            "size": size,
            "bold": d.get("bold", "0") == "1",
            "italic": d.get("italic", "0") == "1",
            "underline": d.get("underline", "none"),
            "strike": d.get("strike", "none"),
            "baseline": d.get("baseline", "none"),
            "caps": d.get("caps", "none"),
            "spacing": spacing,
        });
        if family_changed {
            params["family"] = json!(family);
        }
        if east_asian_changed {
            params["eastAsian"] = json!(east_asian);
        }
        let color = d.get("color", "");
        let seed = d.get("colorSeed", "");
        if color == seed {
            if let Some(c) = d.params.get("color").filter(|v| !v.is_null()) {
                params["color"] = c.clone();
            }
        } else if !color.trim().is_empty() {
            params["color"] = json!(color.trim());
        }
        let _ = app.run("format.fontDialog", params);
    }
    ok || cancel
}
