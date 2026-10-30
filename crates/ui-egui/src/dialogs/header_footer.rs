use super::date_time::{DATE_CALENDARS, DATE_LANGS, date_lang_for, date_rows};
use super::{DATE_KINDS, Dialog, SlideApp, Ui, check, field, json};

/// The automatic date the Header & Footer dialog starts from: the current slide's date field (its
/// type and language), else the deck's last format; Japanese when editing in Japanese.
fn header_footer_date_seed(app: &SlideApp) -> (String, String) {
    use deckcraft_model::text::RunKind;
    let Some(st) = app.session.active() else { return ("datetime1".into(), date_lang_for(app.session.east_asian_editing).into()) };
    let field = st
        .doc
        .slides
        .get(st.selection.slide)
        .and_then(|sl| sl.shapes.iter().find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date)))
        .and_then(|sh| sh.text.as_ref())
        .and_then(|t| {
            t.paragraphs.iter().flat_map(|p| &p.runs).find_map(|r| match &r.kind {
                RunKind::Field { field } if DATE_KINDS.contains(&field.as_str()) => Some((field.clone(), r.props.lang.clone())),
                _ => None,
            })
        });
    let saved = &st.doc.header_footer.date_format;
    let kind = match &field {
        Some((kind, _)) => kind.clone(),
        None if DATE_KINDS.contains(&saved.as_str()) => saved.clone(),
        None => "datetime1".into(),
    };
    let lang = field
        .and_then(|(_, lang)| lang)
        .and_then(|l| DATE_LANGS.iter().find(|(k, _)| k.eq_ignore_ascii_case(&l)).map(|(k, _)| (*k).to_string()))
        .unwrap_or_else(|| date_lang_for(app.session.east_asian_editing).to_string());
    (kind, lang)
}

/// The automatic date's format, language and (for Japanese) calendar, as Insert ▸ Date and Time
/// offers them: the formats are the chosen language's, computed for now.
fn date_format_picker(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) {
    use deckcraft_text::datetime::calendar_of;
    let lang = d.get("dateLang", "en-US");
    let rows = date_rows(&lang);
    let samples: String = rows.iter().map(|(_, s)| s.as_str()).collect();
    let borrowed = app.ui_fonts.faces.len();
    app.ui_fonts.note(ui.ctx(), &samples);
    if app.ui_fonts.faces.len() != borrowed {
        ui.ctx().request_repaint();
    }
    // Another language picked: a format it has no sample for becomes its first. In the language
    // the dialog opened with the slide's own format stays, even one that language isn't listed
    // with (an imported datetime4 in French, say), so Apply without a pick leaves it alone.
    let picked_lang = d.fields.get("dateLangSeed").is_some_and(|seed| *seed != lang);
    if picked_lang
        && !rows.iter().any(|(k, _)| *k == d.get("dateFormat", "datetime1"))
        && let Some((k, _)) = rows.first()
    {
        d.fields.insert("dateFormat".into(), (*k).to_string());
    }
    let format = d.get("dateFormat", "datetime1");
    let shown = rows.iter().find(|(k, _)| *k == format).map_or_else(|| format.clone(), |(_, s)| s.clone());
    egui::Grid::new("hf-date-format").num_columns(2).show(ui, |ui| {
        let l = ui.label(tl!("Format:"));
        craft_ui::ComboBox::from_id_salt("hf-date-format-combo")
            .selected_text(shown.as_str())
            .width(240.0)
            .show_ui(ui, |ui| {
                for (kind, sample) in &rows {
                    if ui.selectable_label(format == *kind, sample).clicked() {
                        d.fields.insert("dateFormat".into(), (*kind).to_string());
                        // Picked, even the one shown: Apply to All sets it on every slide.
                        d.fields.insert("datePicked".into(), "1".into());
                    }
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
        let lang_label = DATE_LANGS.iter().find(|(k, _)| *k == lang).map_or("English (United States)", |(_, l)| *l);
        let l = ui.label(tl!("Language:"));
        craft_ui::ComboBox::from_id_salt("hf-date-lang")
            .selected_text(tl!(lang_label))
            .width(240.0)
            .show_ui(ui, |ui| {
                for (k, l) in DATE_LANGS {
                    if ui.selectable_label(lang == *k, tl!(l)).clicked() {
                        d.fields.insert("dateLang".into(), (*k).to_string());
                    }
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
        if lang == "ja-JP" {
            let calendar = calendar_of(&format, Some(&lang));
            let shown = DATE_CALENDARS.iter().find(|(c, _)| *c == calendar).map_or("Gregorian", |(_, l)| *l);
            let l = ui.label(tl!("Calendar type:"));
            craft_ui::ComboBox::from_id_salt("hf-date-calendar")
                .selected_text(tl!(shown))
                .width(240.0)
                .show_ui(ui, |ui| {
                    for (c, l) in DATE_CALENDARS {
                        // The field type alone carries the calendar: picking one picks its first format.
                        if ui.selectable_label(calendar == c, tl!(l)).clicked()
                            && calendar != c
                            && let Some((k, _)) = rows.iter().find(|(k, _)| calendar_of(k, Some(&lang)) == c)
                        {
                            d.fields.insert("dateFormat".into(), (*k).to_string());
                            d.fields.insert("datePicked".into(), "1".into());
                        }
                    }
                })
                .response
                .labelled_by(l.id);
            ui.end_row();
        }
    });
}

/// Insert ▸ Header & Footer. The date is automatic, a field in a format and language picked as in
/// Insert ▸ Date and Time (the Japanese formats, era dates included, when editing in Japanese), or
/// fixed text. Apply sets the current slide, Apply to All every slide.
pub(super) fn header_footer_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let hf = app.session.active().map(|s| s.doc.header_footer.clone()).unwrap_or_default();
    ui.set_min_width(440.0);
    if !d.fields.contains_key("dateFormatSeed") {
        let (kind, lang) = header_footer_date_seed(app);
        d.fields.insert("dateFormat".into(), kind.clone());
        d.fields.insert("dateFormatSeed".into(), kind);
        d.fields.insert("dateLang".into(), lang.clone());
        d.fields.insert("dateLangSeed".into(), lang);
    }
    let date = check(ui, d, tl!("Date and time"), "date", hf.date);
    let auto = d.get("dateMode", if hf.date_text.is_empty() { "auto" } else { "fixed" }) == "auto";
    ui.add_enabled_ui(date, |ui| {
        ui.indent("hf-date", |ui| {
            if ui.radio(auto, tl!("Update automatically")).clicked() {
                d.fields.insert("dateMode".into(), "auto".into());
            }
            ui.add_enabled_ui(auto, |ui| ui.indent("hf-date-auto", |ui| date_format_picker(app, ui, d)));
            if ui.radio(!auto, tl!("Fixed")).clicked() {
                d.fields.insert("dateMode".into(), "fixed".into());
            }
            ui.add_enabled_ui(!auto, |ui| {
                let mut v = d.get("dateText", &hf.date_text);
                ui.indent("hf-date-fixed", |ui| {
                    // Named for assistive technology: the Fixed radio above is its visible label.
                    ui.add(egui::TextEdit::singleline(&mut v).desired_width(240.0)).accessible_name(tl!("Fixed date"))
                });
                d.fields.insert("dateText".into(), v);
            });
        });
    });
    let num_on = check(ui, d, tl!("Slide number"), "num", hf.slide_number);
    let foot = check(ui, d, tl!("Footer"), "footer", hf.footer);
    field(ui, d, tl!("  Footer text:"), "footerText", &hf.footer_text);
    let hide = check(ui, d, tl!("Don't show on title slide"), "hide", hf.hide_on_title);
    ui.add_space(6.0);
    let mut apply_all = false;
    let mut apply = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        apply_all = ui.button(tl!("Apply to All")).clicked();
        apply = ui.button(tl!("Apply")).clicked();
        if ui.button(tl!("Cancel")).clicked() {
            d.fields.insert("cancel".into(), "1".into());
        }
    });
    if apply || apply_all {
        let auto = d.get("dateMode", if hf.date_text.is_empty() { "auto" } else { "fixed" }) == "auto";
        let dt = if auto { String::new() } else { d.get("dateText", &hf.date_text) };
        let ft = d.get("footerText", "");
        let mut p =
            json!({"date": date, "dateText": dt, "slideNumber": num_on, "footer": foot, "footerText": ft, "hideOnTitle": hide, "all": apply_all});
        // A format the user picked, or a changed format or language, reaches the dates already
        // there; left alone, each keeps its own (an imported datetime4, say, is not rewritten).
        let (format, lang) = (d.get("dateFormat", "datetime1"), d.get("dateLang", "en-US"));
        let picked = d.fields.contains_key("datePicked");
        if auto && (picked || format != d.get("dateFormatSeed", "") || lang != d.get("dateLangSeed", "")) {
            p["dateFormat"] = json!(format);
            p["dateLanguage"] = json!(lang);
        }
        if let Err(e) = app.run("design.headerFooter", p) {
            app.set_status(e);
            return false;
        }
    }
    apply || apply_all || d.fields.contains_key("cancel")
}
