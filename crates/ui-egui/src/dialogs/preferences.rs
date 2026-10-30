use super::{Dialog, SlideApp, Ui, Value, json, theme};

/// What the Preferences dialog can change, as it was when the dialog opened.
pub(super) fn preferences_snapshot(app: &SlideApp) -> Value {
    json!({
        "dark": app.ui.brightness == theme::Brightness::Dark,
        "uiScale": app.ui.ui_scale,
        "language": app.ui.language,
        "prefs": serde_json::to_value(&app.session.prefs).unwrap_or(Value::Null),
    })
}

/// Preferences ▸ General ▸ Interface language: follow the system (the default) or pick one. It
/// applies at once (through `app.language`), and Cancel puts the one the dialog opened with back.
pub(super) fn language_picker(app: &mut SlideApp, ui: &mut Ui) {
    use crate::i18n::{AUTO, Lang};
    let system = crate::i18n::system_lang();
    let auto_label = crate::i18n::fmt(tl!("Automatic ({language})"), &[("language", system.name())]);
    // `auto`, or a code whose catalog this build lacks: both follow the system.
    let chosen = Lang::from_code(&app.ui.language);
    let auto = chosen.is_none();
    let current = chosen.map_or_else(|| auto_label.clone(), |l| l.name().to_string());
    let mut pick = None;
    ui.horizontal(|ui| {
        let l = ui.label(tl!("Interface language:"));
        let combo = craft_ui::ComboBox::from_id_salt("interface_language").selected_text(current).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(auto, auto_label.as_str()).clicked() {
                pick = Some(AUTO);
            }
            for lang in Lang::for_picker() {
                if ui.selectable_label(chosen == Some(lang), lang.name()).clicked() {
                    pick = Some(lang.code());
                }
            }
        });
        // Named by its label for assistive technology (egui 0.37 leaves a combo box unnamed).
        let _ = combo.response.labelled_by(l.id);
    });
    if let Some(value) = pick {
        let _ = app.run("app.language", json!({ "value": value }));
    }
    ui.label(egui::RichText::new(tl!("Menus, dialogs and messages change; your presentations don't.")).small().weak());
}

/// Preferences ▸ General ▸ Measurement units: follow the system region (the default: centimetres
/// in Japan, inches in the United States, as PowerPoint) or pick one. It applies at once through
/// `tools.units`; Cancel puts the one the dialog opened with back (it is one of the preferences).
pub(super) fn units_picker(app: &mut SlideApp, ui: &mut Ui) {
    use crate::units::unit_label;
    let region_unit = app.session.region.units();
    let auto_label = crate::i18n::fmt(tl!("Automatic ({unit})"), &[("unit", tl!(unit_label(region_unit)))]);
    let chosen = app.session.prefs.units;
    let current = chosen.map_or_else(|| auto_label.clone(), |u| tl!(unit_label(u)).to_string());
    let mut pick: Option<&str> = None;
    ui.horizontal(|ui| {
        ui.label(tl!("Measurement units:"));
        let opened = craft_ui::ComboBox::from_id_salt("measurement_units").selected_text(current.as_str()).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), auto_label.as_str()).clicked() {
                pick = Some("auto");
            }
            for u in deckcraft_geom::Unit::CHOICES {
                if ui.selectable_label(chosen == Some(u), tl!(unit_label(u))).clicked() {
                    pick = Some(u.id());
                }
            }
        });
        opened.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, current.as_str()));
    });
    if let Some(value) = pick {
        let _ = app.run("tools.units", json!({ "value": value }));
    }
}

/// Puts back the settings the Preferences dialog opened with (Cancel, Esc, the close button).
pub(super) fn restore_preferences(app: &mut SlideApp, d: &Dialog) {
    let Some(before) = d.params.get("before") else { return };
    if let Some(dark) = before.get("dark").and_then(Value::as_bool)
        && dark != (app.ui.brightness == theme::Brightness::Dark)
    {
        let _ = app.run("view.dark", json!({ "on": dark }));
    }
    if let Some(scale) = before.get("uiScale").and_then(Value::as_f64) {
        app.ui.ui_scale = scale as f32;
    }
    if let Some(language) = before.get("language").and_then(Value::as_str) {
        app.ui.language = language.to_string();
    }
    if let Some(prefs) = before.get("prefs").and_then(|p| serde_json::from_value(p.clone()).ok()) {
        app.session.prefs = prefs;
    }
}
