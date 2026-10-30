//! Dialogs (Insert Table, Slide Size, Font, Paragraph, Tabs, Header & Footer, Hyperlink, Set Up Show, Zoom, …) and the
//! About window. The start screen is File › Home (`backstage.rs`).

use deckcraft_color::SchemeSlot;
use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::SlideApp;
use crate::i18n;
use crate::theme::{self, Tokens};
use crate::units::{len_text, parse_len};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Dialog {
    pub id: String,
    pub params: Value,
    /// Field values while the dialog is open.
    #[serde(skip)]
    pub fields: std::collections::HashMap<String, String>,
}

impl Dialog {
    pub fn new(id: &str) -> Self {
        Dialog { id: id.into(), params: Value::Null, fields: Default::default() }
    }
    pub fn modal(&self) -> bool {
        true
    }
    fn get(&mut self, k: &str, default: &str) -> String {
        self.fields.entry(k.into()).or_insert_with(|| default.to_string()).clone()
    }
}

/// A dialog's window title, in English (the window shows it translated).
fn title(id: &str) -> &'static str {
    match id {
        "table" => "Insert Table",
        "slideSize" => "Slide Size",
        "sizePosition" => "Size and Position",
        "headerFooter" => "Header and Footer",
        "hyperlink" => "Insert Hyperlink",
        "setupShow" => "Set Up Slide Show",
        "zoom" => "Zoom",
        "comment" => "New Comment",
        "altText" => "Alt Text",
        "outline" => "Slides from Outline",
        "renameSection" => "Rename Section",
        "reuseSlides" => "Reuse Slides",
        "renameShape" => "Rename",
        "renameLayout" => "Rename Layout",
        "paragraph" => "Paragraph",
        "tabs" => "Tabs",
        "font" => "Font",
        "about" => "About DeckCraft",
        "preferences" => "Preferences",
        "autocorrect" => "AutoCorrect",
        "gridOptions" => "Grid Options",
        "columns" => "Columns",
        "export" => "Export",
        "symbol" => "Symbol",
        "photoAlbum" => "Photo Album",
        "replaceFonts" => "Replace Font",
        "equation" => "Equation",
        "dateTime" => "Date and Time",
        "spelling" => "Spelling",
        "pasteSpecial" => "Paste Special",
        "find" => "Find and Replace",
        "accessibility" => "Accessibility Checker",
        "language" => "Language",
        "customShow" => "Custom Shows",
        "chartData" => "Chart Data",
        "trim" => "Trim Media",
        "smartart" => "Choose a SmartArt Graphic",
        "bullets" => "Bullets and Numbering",
        "action" => "Action Settings",
        "quit" => crate::APP_NAME,
        "themeColors" => "Create New Theme Colors",
        "themeFonts" => "Create New Theme Fonts",
        _ => crate::APP_NAME,
    }
}

/// Render the active dialog, clearing it when dismissed and preserving or restoring application
/// state as its close path requires.
pub fn show(app: &mut SlideApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    if d.id == "themeFonts" {
        // Seeded first: its size depends on whether it has the Asian text rows.
        seed_theme_fonts(app, ctx, &mut d);
    }
    // The Asian text rows and samples make Theme Fonts taller and a little wider.
    let (theme_fonts_w, theme_fonts_h) = if d.fields.get("asian").is_some_and(|v| v == "1") { (60.0, 150.0) } else { (0.0, 0.0) };
    // The window's id is the dialog's, so a translated title never changes it.
    let mut window = egui::Window::new(tl!(title(&d.id)))
        .id(crate::wid(("dialog", d.id.clone())))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .open(&mut open);
    // Two columns of colour rows plus Save must fit the first frame (Theme Colors), as must Save
    // in Theme Fonts. A later resize clips the buttons (the window size sticks) and clicks miss them.
    if d.id == "themeColors" {
        window = window.default_size(vec2(760.0, 420.0)).min_width(720.0).min_height(420.0);
    } else if d.id == "themeFonts" {
        window = window
            .default_size(vec2(460.0 + theme_fonts_w, 320.0 + theme_fonts_h))
            .min_width(420.0 + theme_fonts_w)
            .min_height(300.0 + theme_fonts_h);
    }
    window.show(ctx, |ui| {
        ui.set_min_width(match d.id.as_str() {
            "themeColors" => 700.0,
            "themeFonts" => 400.0 + theme_fonts_w,
            _ => 340.0,
        });
        if d.id == "themeColors" {
            ui.set_min_height(360.0);
        } else if d.id == "themeFonts" {
            ui.set_min_height(280.0 + theme_fonts_h);
        }
        close = body(app, ui, &mut d);
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if open && !close && !escape {
        app.dialog = Some(d);
    } else if !close && d.id == "preferences" {
        // Esc or the window's close button: the same as Cancel.
        restore_preferences(app, &d);
    } else if app.dialog.is_none()
        && let Some(ret) = d.params.get("returnParams").filter(|v| v.is_object()).cloned()
    {
        // Tabs… opened from Paragraph: closing it (OK, Cancel, Esc or the window) returns there.
        let id = d.params.get("returnId").and_then(Value::as_str).unwrap_or("paragraph");
        let mut back = Dialog::new(id);
        back.params = ret;
        app.dialog = Some(back);
    }
}

fn buttons(ui: &mut Ui, ok_label: &str) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let t = Tokens::get(ui.ctx());
        if ui.add(egui::Button::new(egui::RichText::new(tl!(ok_label)).color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0))).clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            ok = true;
        }
        if ui.add(egui::Button::new(tl!("Cancel")).min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

/// Draw a labelled single-line field bound to `key`, falling back to `default`, and store the
/// edited value in the dialog.
fn field(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: &str) {
    let mut v = d.get(key, default);
    ui.horizontal(|ui| {
        let l = ui.add_sized(vec2(120.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(180.0)).labelled_by(l.id);
    });
    d.fields.insert(key.into(), v);
}

fn check(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: bool) -> bool {
    let mut v = d.get(key, if default { "1" } else { "0" }) == "1";
    ui.checkbox(&mut v, label);
    d.fields.insert(key.into(), if v { "1".into() } else { "0".into() });
    v
}

fn num(d: &mut Dialog, key: &str, default: f64) -> f64 {
    let text = d.get(key, "");
    crate::widgets::ascii_number_text(&text).trim().trim_end_matches(['"', '%', ' ']).parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(default)
}

fn num_text(v: f64) -> String {
    crate::widgets::fmt_num(v, "")
}

fn field_of(d: &Dialog, key: &str) -> String {
    d.fields.get(key).cloned().unwrap_or_default()
}

fn parse_shown(s: &str) -> Option<f64> {
    let s = crate::widgets::ascii_number_text(s);
    let t = s.trim().trim_end_matches(['%', '"']).trim();
    if t.is_empty() {
        return None;
    }
    t.parse().ok().filter(|v: &f64| v.is_finite())
}

fn put_dim(d: &mut Dialog, key: &str, exact: f64, shown: String) {
    d.fields.insert(key.into(), shown.clone());
    d.fields.insert(format!("seed{key}"), shown.clone());
    d.fields.insert(format!("prev{key}"), shown);
    d.fields.insert(format!("{key}Pt"), exact.to_string());
}

fn edited(d: &Dialog, key: &str) -> bool {
    field_of(d, key) != field_of(d, &format!("seed{key}"))
}

fn exact_of(d: &Dialog, key: &str) -> Option<f64> {
    parse_shown(&field_of(d, &format!("{key}Pt")))
}

/// A length field's value in points: the exact length it was filled with while its text is
/// unchanged ([`put_dim`]), else the text read as a length in `unit` (or with its own unit:
/// `2.5 cm`, `1"`, `72 pt`).
fn length_value(d: &Dialog, key: &str, unit: deckcraft_geom::Unit) -> Option<f64> {
    if edited(d, key) { parse_len(&field_of(d, key), unit) } else { exact_of(d, key) }
}

/// The unit a dialog's length fields were filled in ([`size_position::seed_size_position`]).
fn dialog_unit(d: &Dialog) -> deckcraft_geom::Unit {
    deckcraft_geom::Unit::from_id(&field_of(d, "unit")).unwrap_or_default()
}

/// Draw a labelled single-line text row bound to a dialog key and store the edited value.
fn text_row(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str) {
    let mut v = field_of(d, key);
    ui.horizontal(|ui| {
        let l = ui.add_sized(vec2(148.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(96.0)).labelled_by(l.id);
    });
    d.fields.insert(key.into(), v);
}

// The date and time field types in the dialogs' order; re-exported for the date tests.
pub(crate) use deckcraft_text::datetime::DATE_KINDS;

mod about;
mod alt_text;
mod body;
mod columns;
mod date_time;
mod equation;
mod export;
mod find;
mod font;
mod grid_options;
mod header_footer;
mod hyperlink;
mod language;
mod paragraph;
mod photo_album;
mod preferences;
mod size_position;
mod slide_size;
mod tabs;
mod theme_colors;
mod theme_fonts;
mod trim;
mod zoom;

pub use about::{about, paint_logo, version_line};
use body::body;
#[cfg(test)]
pub(crate) use date_time::{date_lang_for, date_rows};
pub use equation::EquationPreview;
#[cfg(test)]
use hyperlink::{
    CurrentLink, HyperlinkSeed, common_click, hyperlink_command, hyperlink_seed_of, hyperlink_should_apply, link_covering, seed_from, seed_of_link,
};
pub use language::selected_language;
use preferences::restore_preferences;
use theme_fonts::seed_theme_fonts;
pub use zoom::parse_zoom_percent;

fn seed_once(d: &mut Dialog, key: &str, val: String) {
    d.fields.entry(key.to_string()).or_insert(val);
}

fn json_text(v: Option<&Value>, default: &str) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map(|x| crate::widgets::fmt_num(x, "")).unwrap_or_else(|| default.to_string()),
        Some(Value::Bool(true)) => "1".into(),
        Some(Value::Bool(false)) => "0".into(),
        _ => default.to_string(),
    }
}

fn color_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("scheme").or_else(|| o.get("rgb")).and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Draw a uniquely identified accessible choice combo box named `name` and store the selected
/// key; the caller supplies the visible label, and `name` and the choices in the interface
/// language.
fn choice(ui: &mut Ui, d: &mut Dialog, salt: &str, name: &str, key: &str, choices: &[(&str, &str)]) {
    let cur = d.get(key, "");
    let label = choices.iter().find(|(v, _)| *v == cur).map(|(_, l)| *l).unwrap_or(cur.as_str());
    let combo = craft_ui::ComboBox::from_id_salt(salt).selected_text(label).width(140.0).show_ui(ui, |ui| {
        for (v, l) in choices {
            if ui.selectable_label(cur == *v, *l).clicked() {
                d.fields.insert((*key).into(), (*v).into());
            }
        }
    });
    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
}

fn labeled_text(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, width: f32) -> String {
    let mut v = d.get(key, "");
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(&mut v).desired_width(width)).labelled_by(l.id);
    d.fields.insert(key.into(), v.clone());
    v
}

/// A labelled font box of a dialog bound to the field `key`: typing edits the field and lists the
/// matching fonts; picking one sets it. `name` is the box's accessible name (English; shown
/// translated).
#[allow(clippy::too_many_arguments)]
fn font_field(
    ui: &mut Ui,
    d: &mut Dialog,
    list: &crate::fontpick::FontList,
    label: (&str, f32),
    name: &str,
    key: &str,
    offer: crate::fontpick::Offer,
    width: f32,
) {
    let value = d.get(key, "");
    let shown = crate::fontpick::shown_value(list, &value);
    let r = ui
        .horizontal(|ui| {
            ui.add_sized(vec2(label.1, 20.0), egui::Label::new(tl!(label.0)));
            crate::fontpick::font_box(ui, key, width, &shown, true, tl!(name), offer, &mut || list.clone(), None)
        })
        .inner;
    if let Some(p) = r.pick {
        d.fields.insert(key.into(), p.value());
    } else if let Some(text) = r.edited {
        d.fields.insert(key.into(), text);
    }
}

/// A short sample of the East Asian editing language's text, for the Asian text font samples.
fn east_asian_sample(app: &SlideApp) -> &'static str {
    use deckcraft_fonts::CjkLocale;
    let lang =
        app.session.east_asian_editing.or_else(deckcraft_fonts::default_cjk_locale).or_else(|| CjkLocale::from_lang(crate::i18n::current().code()));
    match lang {
        Some(CjkLocale::Korean) => "가나다",
        Some(CjkLocale::SimplifiedChinese) => "中文字体",
        Some(CjkLocale::TraditionalChinese) => "中文字體",
        _ => "あア亜",
    }
}

fn finite_pt(text: &str) -> Option<f64> {
    crate::widgets::ascii_number_text(text).trim().trim_end_matches("pt").trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn pt_text(v: Option<&Value>, default: f64) -> String {
    match v.and_then(Value::as_f64).filter(|n| n.is_finite()) {
        Some(n) => crate::widgets::fmt_num(n, ""),
        None => crate::widgets::fmt_num(default, ""),
    }
}

/// Fill a length field once with `v` (points) in `unit`; the exact length stays behind it until
/// the text is edited ([`length_value`]).
fn seed_length(d: &mut Dialog, key: &str, v: Option<&Value>, unit: deckcraft_geom::Unit) {
    if !d.fields.contains_key(key) {
        let pt = v.and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0);
        put_dim(d, key, pt, len_text(pt, unit));
    }
}

pub mod credits;
#[cfg(test)]
mod font_dialog_tests;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod updates_ui;

#[cfg(test)]
mod size_position_tests;

#[cfg(test)]
mod columns_dialog_tests;

#[cfg(test)]
mod photo_album_tests;

#[cfg(test)]
mod theme_colors_tests;

#[cfg(test)]
mod theme_fonts_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod spelling_tests;

#[cfg(test)]
mod paragraph_dialog_tests;

#[cfg(test)]
mod preferences_tests;
