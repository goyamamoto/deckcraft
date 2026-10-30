//! Font boxes: Home › Font's font box and the font fields of the Font, Create New Theme Fonts and
//! Replace Fonts dialogs. Each lists the theme's fonts, then every installed family by its name in
//! the interface language (ヒラギノ角ゴシック in a Japanese interface, Hiragino Sans in an English
//! one), as the engine's `font.list` gives them. Typing in the box lists the families whose name in
//! any language matches (`ゴシック`, `hiragino`); Enter or a click picks one, and a name nothing
//! matches is taken as typed, as PowerPoint takes a font that isn't installed. A picked family is
//! written by the name the list shows, as Japanese PowerPoint writes 游ゴシック.

use std::sync::Arc;

use deckcraft_engine::fonts::{FontChoice, ThemeFontChoice};
use deckcraft_fonts::fold_font_name;
use egui::{Rect, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::SlideApp;
use crate::theme::{self, Tokens};

/// The font list as `font.list` gives it for the interface language.
#[derive(Clone, Default)]
pub struct FontList {
    /// The Asian text fonts apply (East Asian editing or an East Asian interface).
    pub east_asian: bool,
    pub theme: Vec<ThemeFontChoice>,
    pub fonts: Arc<Vec<FontChoice>>,
}

impl FontList {
    /// The theme's Asian text font for `role` (`headings`, `body`), if the list has one.
    pub fn theme_east_asian(&self, role: &str) -> Option<&ThemeFontChoice> {
        self.theme.iter().find(|t| t.east_asian && t.role == role)
    }
}

/// Is the interface set up for East Asian text: an East Asian editing language, or an interface in
/// Japanese, Chinese or Korean? The font boxes then offer Asian text fonts, as PowerPoint's do with
/// East Asian editing.
pub fn east_asian_ui(app: &SlideApp) -> bool {
    app.session.east_asian_editing.is_some() || crate::i18n::current().uses_cjk()
}

/// How long the installed families stay cached (seconds of interface time). Rescanning from the
/// font box refreshes them at once ([`forget_fonts`]).
const FONTS_TTL: f64 = 10.0;

#[derive(Clone)]
struct CachedFonts {
    lang: String,
    at: f64,
    fonts: Arc<Vec<FontChoice>>,
}

fn cache_id() -> egui::Id {
    crate::wid("fontpick.fonts")
}

/// Drop the cached families, so the next list asks the engine again (after `font.rescan`).
pub fn forget_fonts(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<CachedFonts>(cache_id()));
}

/// The font list for the current interface language: the theme section of the selection's master
/// (asked for every time; it is small) and the installed families (cached for a few seconds,
/// since listing them reads every font's names).
pub fn font_list(app: &mut SlideApp, ctx: &egui::Context) -> FontList {
    let lang = crate::i18n::current().code().to_string();
    let east_asian = east_asian_ui(app);
    let now = ctx.input(|i| i.time);
    let cached = ctx.data(|d| d.get_temp::<CachedFonts>(cache_id())).filter(|c| c.lang == lang && (now - c.at).abs() < FONTS_TTL);
    let fonts = match cached {
        Some(c) => c.fonts,
        None => {
            let v = app.session.execute("font.list", &json!({"lang": lang, "theme": false})).unwrap_or(Value::Null);
            let fonts: Arc<Vec<FontChoice>> = Arc::new(v.get("fonts").cloned().and_then(|f| serde_json::from_value(f).ok()).unwrap_or_default());
            ctx.data_mut(|d| d.insert_temp(cache_id(), CachedFonts { lang: lang.clone(), at: now, fonts: fonts.clone() }));
            fonts
        }
    };
    let v = app.session.execute("font.list", &json!({"lang": lang, "fonts": false, "eastAsian": east_asian})).unwrap_or(Value::Null);
    let theme = v.get("theme").cloned().and_then(|t| serde_json::from_value(t).ok()).unwrap_or_default();
    FontList { east_asian, theme, fonts }
}

/// A theme font's entry: its face and whether it is the heading or body font, for Latin or Asian
/// text.
pub fn theme_label(t: &ThemeFontChoice) -> String {
    let template = match (t.role.as_str(), t.east_asian) {
        ("headings", false) => tl!("{f} (Headings)"),
        (_, false) => tl!("{f} (Body)"),
        ("headings", true) => tl!("{f} (Asian Headings)"),
        (_, true) => tl!("{f} (Asian Body)"),
    };
    crate::i18n::fmt(template, &[("f", &t.name)])
}

/// How the font box names the theme's heading (`role` `headings`) or body font `font` the text is
/// in, Latin or Asian: "Inter (Body)", as PowerPoint's box does.
pub fn role_label(font: &str, role: &str) -> String {
    let template = if role == "headings" { tl!("{f} (Headings)") } else { tl!("{f} (Body)") };
    crate::i18n::fmt(template, &[("f", font)])
}

/// What a font box shows for a value it holds: a theme reference (`+mj-lt`) as its theme entry,
/// anything else as it is.
pub fn shown_value(list: &FontList, value: &str) -> String {
    match list.theme.iter().find(|t| t.reference == value) {
        Some(t) => theme_label(t),
        None => value.to_string(),
    }
}

/// What a font box picked.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// A family, by the name the list shows (or as typed when nothing matched).
    Font(String),
    /// One of the theme's fonts.
    Theme(ThemeFontChoice),
}

impl Pick {
    /// The value a dialog field keeps: the family name, or the theme reference.
    pub fn value(&self) -> String {
        match self {
            Pick::Font(f) => f.clone(),
            Pick::Theme(t) => t.reference.clone(),
        }
    }
}

/// Which entries a font box offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    /// The theme section (Latin and, with East Asian editing, Asian text fonts) and every family.
    All,
    /// The theme's Latin fonts and every family (the Font dialog's Latin text font).
    Latin,
    /// The theme's Asian text fonts and the Japanese, Chinese and Korean families.
    EastAsian,
    /// Every family, no theme section (theme fonts, Replace Fonts).
    Families,
}

impl Offer {
    fn takes_theme(self, t: &ThemeFontChoice) -> bool {
        match self {
            Offer::All => true,
            Offer::Latin => !t.east_asian,
            Offer::EastAsian => t.east_asian,
            Offer::Families => false,
        }
    }
    fn takes(self, f: &FontChoice) -> bool {
        self != Offer::EastAsian || f.east_asian
    }
}

/// How well `query` (folded) finds a font by one of `names`: 0 exact, 1 a name starts with it,
/// 2 a name contains it; `None` when no name has it. Names compare folded (case, full-width
/// letters and spacing ignored), so ｈｉｒａｇｉｎｏ finds Hiragino.
fn rank(query: &str, names: &[&str]) -> Option<u8> {
    names
        .iter()
        .filter_map(|n| {
            let n = fold_font_name(n);
            if n == query {
                Some(0)
            } else if n.starts_with(query) {
                Some(1)
            } else if n.contains(query) {
                Some(2)
            } else {
                None
            }
        })
        .min()
}

/// An entry of a font box's list.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Heading(&'static str),
    Theme(usize),
    Font(usize),
}

/// The rows `offer` lists for `query`: with nothing typed, the theme section and then every family
/// under headings; with a query, its matches only (best first, theme entries before families).
fn rows(list: &FontList, offer: Offer, query: &str) -> Vec<Row> {
    let q = fold_font_name(query);
    let theme: Vec<usize> = list.theme.iter().enumerate().filter(|(_, t)| offer.takes_theme(t)).map(|(k, _)| k).collect();
    if q.is_empty() {
        let mut out = Vec::with_capacity(list.fonts.len() + theme.len() + 2);
        if !theme.is_empty() {
            out.push(Row::Heading("Theme Fonts"));
            out.extend(theme.into_iter().map(Row::Theme));
            out.push(Row::Heading("All Fonts"));
        }
        out.extend(list.fonts.iter().enumerate().filter(|(_, f)| offer.takes(f)).map(|(k, _)| Row::Font(k)));
        return out;
    }
    let mut hits: Vec<(u8, bool, usize, Row)> = Vec::new();
    for k in theme {
        if let Some(t) = list.theme.get(k)
            && let Some(r) = rank(&q, &[&t.name])
        {
            hits.push((r, false, k, Row::Theme(k)));
        }
    }
    for (k, f) in list.fonts.iter().enumerate().filter(|(_, f)| offer.takes(f)) {
        let names: Vec<&str> = std::iter::once(f.name.as_str()).chain(f.names.iter().map(String::as_str)).collect();
        if let Some(r) = rank(&q, &names) {
            hits.push((r, true, k, Row::Font(k)));
        }
    }
    hits.sort_by_key(|h| (h.0, h.1, h.2));
    hits.into_iter().map(|h| h.3).collect()
}

/// The pick a row stands for.
fn pick_of(list: &FontList, row: &Row) -> Option<Pick> {
    match row {
        Row::Heading(_) => None,
        Row::Theme(k) => list.theme.get(*k).cloned().map(Pick::Theme),
        Row::Font(k) => list.fonts.get(*k).map(|f| Pick::Font(f.name.clone())),
    }
}

/// Is `row` the font a box holds? A family matches by any of its names (a deck naming Hiragino
/// Sans lights ヒラギノ角ゴシック up in a Japanese interface); a theme entry by its reference, or by
/// its label, or as the font box shows the theme's font ([`role_label`]).
fn is_current(list: &FontList, row: &Row, current: &str) -> bool {
    let cur = fold_font_name(current);
    if cur.is_empty() {
        return false;
    }
    match row {
        Row::Heading(_) => false,
        Row::Theme(k) => list.theme.get(*k).is_some_and(|t| {
            t.reference == current || fold_font_name(&theme_label(t)) == cur || fold_font_name(&role_label(&t.name, &t.role)) == cur
        }),
        Row::Font(k) => list.fonts.get(*k).is_some_and(|f| fold_font_name(&f.name) == cur || f.names.iter().any(|n| fold_font_name(n) == cur)),
    }
}

/// A font box's state between frames.
#[derive(Clone, Default)]
struct BoxState {
    /// The field's text while it has focus.
    text: String,
    /// Typing since the field took focus: the matches list under it shows.
    typed: bool,
    /// Where the matches list was drawn last frame (a click in it takes focus from the field).
    list: Option<Rect>,
    /// The full list was open last frame (it scrolls to the current font as it opens).
    open: bool,
    /// The field just took focus: its name is selected until the click that focused it ends (the
    /// field puts the cursor where it was pressed and released).
    select_all: bool,
}

/// Row height of the font lists.
const ROW_H: f32 = 20.0;

/// The width of a font box's lists: wider than the box, so theme entries ("BIZ UDPGothic (Asian
/// Headings)") fit on their row.
fn list_width(box_width: f32) -> f32 {
    (box_width + 110.0).max(260.0)
}

/// One list row: a heading, or a selectable entry named by its label (for screen readers and
/// tests). Returns the click response for entries.
fn draw_row(ui: &mut Ui, list: &FontList, row: &Row, current: &str, width: f32) -> Option<egui::Response> {
    match row {
        Row::Heading(h) => {
            let t = Tokens::get(ui.ctx());
            let (r, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
            ui.painter().text(pos2(r.min.x + 4.0, r.center().y), egui::Align2::LEFT_CENTER, tl!(h), theme::bold(11.0), t.text_dim);
            None
        }
        Row::Theme(k) => {
            let label = list.theme.get(*k).map(theme_label).unwrap_or_default();
            Some(entry(ui, is_current(list, row, current), &label, width))
        }
        Row::Font(k) => {
            let label = list.fonts.get(*k).map(|f| f.name.clone()).unwrap_or_default();
            Some(entry(ui, is_current(list, row, current), &label, width))
        }
    }
}

/// A list entry: its name on one line from the left (cut short when too long), lit when it is the
/// font the box holds or under the pointer.
fn entry(ui: &mut Ui, selected: bool, label: &str, width: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(r, egui::CornerRadius::same(3), if selected { t.selected } else { t.hover });
    }
    let font = theme::font(12.0);
    let galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), font, t.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width((width - 12.0).max(10.0));
        f.layout_job(job)
    });
    ui.painter().galley(pos2(r.min.x + 6.0, r.center().y - galley.size().y / 2.0), galley, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    resp
}

/// Draw `rows` in a scrolling list (only the visible rows are laid out). `scroll_to` scrolls a row
/// into view first. Returns the picked row.
fn list_ui(ui: &mut Ui, list: &FontList, rows: &[Row], current: &str, width: f32, scroll_to: Option<usize>) -> Option<Pick> {
    let mut picked = None;
    let spacing = ui.spacing().item_spacing.y;
    let mut area = egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]);
    if let Some(k) = scroll_to {
        area = area.vertical_scroll_offset(((k as f32) - 3.0).max(0.0) * (ROW_H + spacing));
    }
    area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
        for row in rows.get(range).unwrap_or(&[]) {
            if draw_row(ui, list, row, current, width).is_some_and(|r| r.clicked()) {
                picked = pick_of(list, row);
            }
        }
    });
    picked
}

/// What a font box did this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxResult {
    /// The font picked from the list, or by Enter (the best match for the text typed, or the text
    /// itself when nothing matches).
    pub pick: Option<Pick>,
    /// The field's text, when typing changed it: a dialog keeps what is typed, as its OK applies
    /// a name typed without Enter.
    pub edited: Option<String>,
}

/// A font box: a field showing `current` (a family name, or the font box's theme label) with a
/// list arrow. `fetch` gives the list when it is needed (the arrow is open or the user is typing).
/// `name` names the field for screen readers (already translated). `extra` adds items under the
/// full list (Rescan Installed Fonts).
#[allow(clippy::too_many_arguments)]
pub fn font_box(
    ui: &mut Ui,
    id: &str,
    width: f32,
    current: &str,
    enabled: bool,
    name: &str,
    offer: Offer,
    fetch: &mut dyn FnMut() -> FontList,
    extra: Option<&mut dyn FnMut(&mut Ui)>,
) -> BoxResult {
    let state_id = ui.scope_id().with(("fontbox", id));
    let te_id = state_id.with("edit");
    let mut st: BoxState = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();
    let had_focus = ui.memory(|m| m.has_focus(te_id));
    if !had_focus && !st.typed {
        st.text = current.to_string();
    }
    let mut out = None;
    let mut edited = None;
    let t = Tokens::get(ui.ctx());
    let mut field_rect = Rect::NOTHING;
    let arrow = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // The field and its arrow share one outline, painted under them once placed, like
            // the font size box's.
            let outline = ui.painter().add(egui::Shape::Noop);
            // The box takes Escape itself (it puts the font back and gives up focus below): left
            // to egui, the focus would go before the frame starts and the slide would take the
            // Escape too, dropping the selection the font was for.
            let keys = egui::EventFilter { tab: false, horizontal_arrows: true, vertical_arrows: true, escape: true };
            let edit = egui::TextEdit::singleline(&mut st.text)
                .id(te_id)
                .desired_width(width - 18.0)
                .min_size(vec2(0.0, 22.0))
                .font(theme::font(12.0))
                .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 6, right: 4, top: 4, bottom: 2 }))
                .text_color(if enabled { t.text } else { t.text_faint })
                .event_filter(keys);
            let resp = ui.add_enabled(enabled, edit);
            let shown = st.text.clone();
            let access = name.to_string();
            resp.widget_info(move || {
                let mut info = egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, access.clone());
                info.current_text_value = Some(shown.clone());
                info
            });
            field_rect = resp.rect;
            // Clicking into the box selects its name, so typing replaces it.
            st.select_all |= resp.gained_focus();
            if st.select_all && resp.has_focus() {
                if let Some(mut s) = egui::TextEdit::load_state(ui.ctx(), te_id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(st.text.chars().count()));
                    s.cursor.set_char_range(Some(all));
                    s.store(ui.ctx(), te_id);
                }
                st.select_all = ui.input(|i| i.pointer.any_down());
            } else {
                st.select_all = false;
            }
            if resp.changed() {
                st.typed = true;
                edited = Some(st.text.clone());
            }
            let focused = resp.has_focus();
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter {
                let text = st.text.trim().to_string();
                if !text.is_empty() && text != current {
                    let list = fetch();
                    let found = rows(&list, offer, &text);
                    out = Some(found.iter().find_map(|r| pick_of(&list, r)).unwrap_or(Pick::Font(text)));
                }
                st.typed = false;
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) && (focused || had_focus) {
                st.typed = false;
                st.text = current.to_string();
                ui.memory_mut(|m| m.surrender_focus(te_id));
            } else if !focused && st.typed {
                // A click in the matches list takes focus from the field: keep the list.
                let over = ui.input(|i| i.pointer.latest_pos()).is_some_and(|p| st.list.is_some_and(|r| r.contains(p)));
                if over {
                    ui.memory_mut(|m| m.request_focus(te_id));
                } else {
                    st.typed = false;
                }
            }
            let (ar, aresp) = ui.allocate_exact_size(vec2(18.0, resp.rect.height().max(22.0)), if enabled { Sense::click() } else { Sense::hover() });
            let arrow_name = crate::i18n::fmt(tl!("{name} list"), &[("name", name)]);
            aresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &arrow_name));
            let lit = enabled && (resp.hovered() || aresp.hovered() || focused);
            let border = egui::Stroke::new(1.0, if lit { t.text_faint } else { t.border });
            let shape = egui::epaint::RectShape::new(resp.rect.union(ar), egui::CornerRadius::same(4), t.field_bg, border, egui::StrokeKind::Inside);
            ui.painter().set(outline, shape);
            if aresp.hovered() {
                ui.painter().rect_filled(ar.shrink(2.0), egui::CornerRadius::same(3), t.hover);
            }
            crate::widgets::chevron(ui.painter(), ar.center(), 3.0, if enabled { t.text_dim } else { t.text_faint });
            aresp
        })
        .inner;
    // The full list, from the arrow.
    let mut opened = false;
    egui::Popup::menu(&arrow).anchor(field_rect.union(arrow.rect)).width(list_width(width)).show(|ui| {
        opened = true;
        let list = fetch();
        let all = rows(&list, offer, "");
        let scroll_to = (!st.open).then(|| all.iter().position(|r| is_current(&list, r, current))).flatten();
        let w = ui.available_width().max(width);
        if let Some(p) = list_ui(ui, &list, &all, current, w, scroll_to) {
            out = Some(p);
            ui.close();
        }
        if let Some(extra) = extra {
            ui.separator();
            extra(ui);
        }
    });
    st.open = opened;
    // The matches for what is typed, under the field.
    st.list = None;
    if st.typed && out.is_none() {
        let list = fetch();
        let found = rows(&list, offer, &st.text);
        if !found.is_empty() {
            let w = list_width(width);
            let shown =
                egui::Area::new(state_id.with("matches")).order(egui::Order::Foreground).fixed_pos(field_rect.left_bottom()).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(w);
                        list_ui(ui, &list, &found, current, w, None)
                    })
                });
            st.list = Some(shown.response.rect);
            if let Some(p) = shown.inner.inner {
                out = Some(p);
            }
        }
    }
    if out.is_some() {
        st.typed = false;
        st.text = current.to_string();
        ui.memory_mut(|m| m.surrender_focus(te_id));
    }
    ui.data_mut(|d| d.insert_temp(state_id, st));
    BoxResult { pick: out, edited }
}

#[cfg(test)]
#[path = "fontpick_tests.rs"]
mod tests;
