use super::*;

/// Create New Theme Fonts' fields: the scheme's name, its Latin heading and body fonts and its
/// Asian text heading and body fonts (`a:ea`; where the theme names none, its font for the
/// editing language's script, `Jpan` for Japanese, the face Japanese text is drawn in). The Asian
/// rows show with East Asian editing, an East Asian interface, or a theme that names them.
pub(super) fn seed_theme_fonts(app: &mut SlideApp, ctx: &egui::Context, d: &mut Dialog) {
    if d.fields.contains_key("seeded") {
        return;
    }
    let text = |k: &str| d.params.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let name = text("name").unwrap_or_else(|| "Custom".into());
    let major = text("major").unwrap_or_else(|| "Inter".into());
    let minor = text("minor").unwrap_or_else(|| major.clone());
    let list = crate::fontpick::font_list(app, ctx);
    let ea = |key: &str, role: &str| text(key).or_else(|| list.theme_east_asian(role).map(|t| t.name.clone())).unwrap_or_default();
    let (major_ea, minor_ea) = (ea("majorEa", "headings"), ea("minorEa", "body"));
    let asian = list.east_asian || text("majorEa").is_some() || text("minorEa").is_some();
    for (k, v) in [
        ("schemeName", name),
        ("major", major),
        ("minor", minor),
        ("majorEa", major_ea.clone()),
        ("majorEaSeed", major_ea),
        ("minorEa", minor_ea.clone()),
        ("minorEaSeed", minor_ea),
        ("asian", if asian { "1" } else { "" }.to_string()),
        ("seeded", "1".to_string()),
    ] {
        d.fields.insert(k.into(), v);
    }
}

fn theme_font_payload(d: &mut Dialog) -> Result<Value, String> {
    let name = d.get("schemeName", "").trim().to_string();
    if name.is_empty() {
        return Err("Enter a scheme name.".into());
    }
    let major = d.get("major", "").trim().to_string();
    if major.is_empty() {
        return Err("Choose a heading font.".into());
    }
    let minor = d.get("minor", "").trim().to_string();
    if minor.is_empty() {
        return Err("Choose a body font.".into());
    }
    let mut payload = json!({"name": name, "major": major, "minor": minor});
    // The Asian text fonts only when they changed: opening and saving keeps a theme that names
    // none (its Japanese text keeps taking the Jpan font).
    for (key, seed) in [("majorEa", "majorEaSeed"), ("minorEa", "minorEaSeed")] {
        let v = d.get(key, "").trim().to_string();
        if !v.is_empty() && v != d.get(seed, "").trim() {
            payload[key] = json!(v);
        }
    }
    Ok(payload)
}

/// Create New Theme Fonts. Returns true when the dialog should close.
pub(super) fn theme_fonts_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    use crate::fontpick::Offer;
    let ctx = ui.ctx().clone();
    seed_theme_fonts(app, &ctx, d);
    let list = crate::fontpick::font_list(app, &ctx);
    let asian = d.get("asian", "") == "1";
    let heading = |ui: &mut Ui, s: &str| {
        ui.label(egui::RichText::new(tl!(s)).font(theme::bold(12.0)));
    };
    if asian {
        heading(ui, "Latin text");
    }
    font_field(ui, d, &list, ("Heading:", 130.0), "Heading font", "major", Offer::Families, 240.0);
    font_field(ui, d, &list, ("Body:", 130.0), "Body font", "minor", Offer::Families, 240.0);
    if asian {
        ui.add_space(4.0);
        heading(ui, "Asian text");
        font_field(ui, d, &list, ("Heading:", 130.0), "Asian heading font", "majorEa", Offer::EastAsian, 240.0);
        font_field(ui, d, &list, ("Body:", 130.0), "Asian body font", "minorEa", Offer::EastAsian, 240.0);
    }
    ui.add_space(4.0);
    // Theme faces are not egui families; naming an unbound family panics. The lines show the
    // chosen names at heading and body sizes.
    let sample = |template: &str, key: &str, size: f32, ui: &mut Ui, d: &mut Dialog| {
        let text = crate::i18n::fmt(tl!(template), &[(key, &d.get(key, "")), ("sample", east_asian_sample(app))]);
        ui.label(egui::RichText::new(text).size(size));
    };
    sample("Heading sample  {major}", "major", 20.0, ui, d);
    sample("Body sample  {minor}", "minor", 14.0, ui, d);
    if asian {
        sample("Asian heading sample  {sample}  {majorEa}", "majorEa", 17.0, ui, d);
        sample("Asian body sample  {sample}  {minorEa}", "minorEa", 13.0, ui, d);
    }
    ui.add_space(4.0);
    let mut name = d.get("schemeName", "");
    ui.horizontal(|ui| {
        ui.add_sized(vec2(130.0, 18.0), egui::Label::new(tl!("Name:")));
        let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0).id_salt("schemeName"));
        let shown = name.clone();
        resp.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::Role::TextInput, true, tl!("Scheme name"));
            info.current_text_value = Some(shown.clone());
            info
        });
    });
    d.fields.insert("schemeName".into(), name);
    let note =
        if asian { "Heading and body fonts for Latin and Asian text in this presentation." } else { "Heading and body fonts for this presentation." };
    ui.label(egui::RichText::new(tl!(note)).size(11.0).color(Tokens::get(ui.ctx()).text_dim));
    let (ok, cancel) = buttons(ui, "Save");
    if ok {
        match theme_font_payload(d) {
            Ok(payload) => return app.run("design.fonts", payload).is_ok(),
            Err(msg) => {
                app.set_status(msg);
                return false;
            }
        }
    }
    cancel
}
