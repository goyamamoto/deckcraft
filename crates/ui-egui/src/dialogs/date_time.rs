use super::*;

pub(super) const DATE_LANGS: &[(&str, &str)] = &[
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("fr-FR", "French (France)"),
    ("ja-JP", "Japanese"),
    ("sv-SE", "Swedish"),
];

/// The calendars the Date and Time dialog offers for Japanese, as (calendar, label).
pub(super) const DATE_CALENDARS: [(deckcraft_text::datetime::Calendar, &str); 2] =
    [(deckcraft_text::datetime::Calendar::Gregorian, "Gregorian"), (deckcraft_text::datetime::Calendar::JapaneseEra, "Japanese era")];

/// The language the Date and Time dialog opens in: Japanese when the system's East Asian
/// editing language is Japanese (PowerPoint's dialog starts in the editing language, and a
/// Japanese system edits in Japanese), else English (United States).
fn default_date_lang() -> &'static str {
    date_lang_for(deckcraft_fonts::default_cjk_locale())
}

/// [`default_date_lang`] for an East Asian editing language.
pub(crate) fn date_lang_for(east_asian: Option<deckcraft_fonts::CjkLocale>) -> &'static str {
    if east_asian == Some(deckcraft_fonts::CjkLocale::Japanese) { "ja-JP" } else { "en-US" }
}

/// Samples `field_text` can compute for `lang`, in the order of the field types. English and
/// Japanese have all thirteen (Japanese with its era dates and 時分秒 times); other languages
/// keep the numeric date and the 24-hour times.
pub(crate) fn date_rows(lang: &str) -> Vec<(&'static str, String)> {
    let now = deckcraft_text::datetime::DateTime::now_local();
    let mut rows = Vec::new();
    for kind in DATE_KINDS {
        if let Some(now) = now
            && let Some(sample) = deckcraft_text::datetime::field_text(kind, Some(lang), now)
        {
            rows.push((kind, sample));
        }
    }
    if rows.is_empty() {
        rows.push(("datetime1", "datetime1".into()));
    }
    rows
}

/// Insert › Date and Time: the formats of the chosen language, computed for now. In Japanese a
/// Calendar type shows whether the chosen format counts years by the Western calendar or the
/// Japanese era (和暦; datetime3, 6 and 7), and picking a calendar picks its first format: the
/// field type alone carries the calendar. OK applies `insert.dateTime`; the return value says
/// whether the dialog should close.
pub(super) fn date_time_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    use deckcraft_text::datetime::calendar_of;
    ui.set_min_width(440.0);
    ui.label(tl!("Available formats:"));
    let lang = d.get("language", default_date_lang());
    let rows = date_rows(&lang);
    // The Japanese samples need a CJK interface face (borrowed from the font database when the
    // interface faces lack one), or they draw as boxes.
    let samples: String = rows.iter().map(|(_, s)| s.as_str()).collect();
    let borrowed = app.ui_fonts.faces.len();
    app.ui_fonts.note(ui.ctx(), &samples);
    if app.ui_fonts.faces.len() != borrowed {
        ui.ctx().request_repaint();
    }
    if !rows.iter().any(|(k, _)| *k == d.get("format", "datetime1"))
        && let Some((k, _)) = rows.first()
    {
        d.fields.insert("format".into(), (*k).to_string());
    }
    let format = d.get("format", "datetime1");
    egui::ScrollArea::vertical().max_height(280.0).min_scrolled_height(220.0).auto_shrink([false, false]).show(ui, |ui| {
        for (kind, sample) in &rows {
            if ui.selectable_label(format == *kind, sample).clicked() {
                d.fields.insert("format".into(), (*kind).to_string());
            }
        }
    });
    let lang_label = DATE_LANGS.iter().find(|(k, _)| *k == lang).map(|(_, l)| *l).unwrap_or("English (United States)");
    let japanese = lang == "ja-JP";
    egui::Grid::new("date-options").num_columns(2).show(ui, |ui| {
        ui.label(tl!("Language:"));
        let opened = egui::ComboBox::from_id_salt("date-lang").selected_text(tl!(lang_label)).width(240.0).show_ui(ui, |ui| {
            for (k, l) in DATE_LANGS {
                if ui.selectable_label(lang == *k, tl!(l)).clicked() {
                    d.fields.insert("language".into(), (*k).to_string());
                }
            }
        });
        // Each choice is named by its field, its value the choice shown.
        let shown = tl!(lang_label).to_string();
        opened.response.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Language"));
            info.current_text_value = Some(shown.clone());
            info
        });
        ui.end_row();
        if japanese {
            let calendar = calendar_of(&format, Some(&lang));
            let shown = DATE_CALENDARS.iter().find(|(c, _)| *c == calendar).map_or("Gregorian", |(_, l)| *l);
            ui.label(tl!("Calendar type:"));
            let opened = egui::ComboBox::from_id_salt("date-calendar").selected_text(tl!(shown)).width(240.0).show_ui(ui, |ui| {
                for (c, l) in DATE_CALENDARS {
                    if ui.selectable_label(calendar == c, tl!(l)).clicked()
                        && calendar != c
                        && let Some((k, _)) = rows.iter().find(|(k, _)| calendar_of(k, Some(&lang)) == c)
                    {
                        d.fields.insert("format".into(), (*k).to_string());
                    }
                }
            });
            let shown = tl!(shown).to_string();
            opened.response.widget_info(move || {
                let mut info = egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Calendar type"));
                info.current_text_value = Some(shown.clone());
                info
            });
            ui.end_row();
        }
    });
    if !japanese && !lang.starts_with("en") {
        ui.label(
            egui::RichText::new(tl!(
                "Month names and AM/PM are listed for English and Japanese. Other languages list the numeric date and 24-hour time."
            ))
            .size(11.0)
            .color(Tokens::get(ui.ctx()).text_dim),
        );
    }
    let update = check(ui, d, tl!("Update automatically"), "update", true);
    let (ok, cancel) = buttons(ui, "OK");
    if ok {
        let lang = d.get("language", default_date_lang());
        let mut format = d.get("format", "datetime1");
        let probe = deckcraft_text::datetime::DateTime::new(2026, 10, 9, 20, 5, 7);
        if deckcraft_text::datetime::field_text(&format, Some(&lang), probe).is_none() {
            format = "datetime1".into();
        }
        return app.run("insert.dateTime", json!({"format": format, "update": update, "language": lang})).is_ok();
    }
    cancel
}
