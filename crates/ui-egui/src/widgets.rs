//! Ribbon widgets: large and small buttons, split buttons, groups, dropdowns, spinners, the colour
//! picker and galleries.
//!
//! The widgets take English labels and tooltips and draw them in the interface language
//! ([`crate::i18n`]), measuring the translated text; their accessible names are what is drawn.
//! Callers keep passing English (and the English goes on to the Quick Access Toolbar and commands);
//! the `_shown` variants take text already translated (a label with a runtime value, a command's
//! label with its shortcut). Ribbon groups shrink to fit the window ([`crate::ribbon_fit`]).

use deckcraft_color::{ColorScheme, Rgba, SchemeSlot};
use deckcraft_model::ColorRef;
use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};

/// Height of the ribbon's content area.
pub const RIBBON_H: f32 = 66.0;

fn bg(ui: &Ui, r: &Response, selected: bool, t: &Tokens) -> Option<Color32> {
    if !r.sense.senses_click() && !r.sense.senses_drag() {
        return None;
    }
    if selected {
        Some(t.selected)
    } else if r.is_pointer_button_down_on() {
        Some(t.pressed)
    } else if r.hovered() && ui.is_enabled() {
        Some(t.hover)
    } else {
        None
    }
}

/// A large ribbon button: 32 px icon over a one- or two-line label.
pub fn big_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    big_button_ex(ui, icon, label, enabled, false, false).0
}

/// Width of the dropdown target at the right of a large split button. It is reserved outside the
/// whole label area, so no visible line of a one- or two-line label can open the menu.
const SPLIT_ARROW_W: f32 = 14.0;

/// Return the chevron-only sub-rectangle at the right edge of a split button.
fn split_arrow_rect(rect: Rect) -> Rect {
    Rect::from_min_max(pos2(rect.max.x - SPLIT_ARROW_W, rect.min.y + 38.0), rect.max)
}

/// Width of a large button: padding plus, for a split button, the reserved arrow strip.
fn big_button_width(text_w: f32, split: bool) -> f32 {
    (text_w + 10.0 + if split { SPLIT_ARROW_W } else { 0.0 }).max(40.0)
}

/// Large button; with `split`, the label row ends in a separate dropdown target (returns its
/// response). Like Office's split buttons, the icon and the label text run the main command; only
/// the chevron at the end of the label row opens the menu. `label` is English, with `\n` where it
/// breaks; the translation is wrapped by [`wrap_label`].
pub fn big_button_ex(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, split: bool, selected: bool) -> (Response, Option<Response>) {
    let label = crate::i18n::label(label);
    big_button_paint(ui, icon, &label, enabled, split, selected, false)
}

/// A large button showing `shown` as given, already in the interface language: for a label with a
/// runtime value (Slides Per Page: 3).
pub fn big_button_shown(ui: &mut Ui, icon: Icon, shown: &str, enabled: bool) -> Response {
    big_button_paint(ui, icon, shown, enabled, false, false, false).0
}

/// Draws a large button labelled `label` (already translated). `menu` draws a drop-down chevron
/// after the label of a button that only opens a menu (a split button's chevron has its own strip).
fn big_button_paint(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, split: bool, selected: bool, menu: bool) -> (Response, Option<Response>) {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(11.0);
    let menu = menu && !split;
    let lines: Vec<&str> = wrap_label(label);
    let text_w = lines.iter().map(|l| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), t.text).size().x)).fold(0.0f32, f32::max);
    // A menu-only button's chevron follows its one-line label (beside a second line, the first
    // line's width already makes room for it).
    let w = big_button_width(text_w + if menu && lines.len() == 1 { 10.0 } else { 0.0 }, split);
    // The whole button runs the main command; the narrow strip at the end of the label row is the
    // dropdown. A split button therefore has two named targets, not a third unnamed row target.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, RIBBON_H - 2.0), if enabled { Sense::click() } else { Sense::hover() });
    // Named for screen readers and UI tests ("Ink to\nShape" reads "Ink to Shape").
    let spoken = label.replace('\n', " ");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &spoken));
    let painter = ui.painter_at(rect.expand(1.0));
    let arrow_rect = split.then(|| split_arrow_rect(rect));
    let main_resp = resp.clone();
    let arrow_resp = arrow_rect.map(|r| ui.interact(r, resp.id.with("arrow"), if enabled { Sense::click() } else { Sense::hover() }));
    if split && let Some(a) = &arrow_resp {
        a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, crate::i18n::fmt(tl!("{name} options"), &[("name", &spoken)])));
    }
    if let Some(c) = bg(ui, &main_resp, selected, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    if let (Some(ar), Some(a)) = (arrow_rect, &arrow_resp)
        && let Some(c) = bg(ui, a, false, &t)
    {
        painter.rect_filled(ar, CornerRadius::same(4), c);
    }
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 19.0), vec2(30.0, 30.0));
    icons::paint(&painter, icon_rect, icon, t.text, !enabled);
    let col = if enabled { t.text } else { t.text_faint };
    let label_rect = if split { Rect::from_min_max(rect.min, pos2(rect.max.x - SPLIT_ARROW_W, rect.max.y)) } else { rect };
    let mut y = rect.min.y + 39.0;
    for (i, l) in lines.iter().enumerate() {
        let last = i + 1 == lines.len();
        let label_w = ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), col).size().x);
        let extra = if menu && last { 9.0 } else { 0.0 };
        let x = label_rect.center().x - (label_w + extra) / 2.0;
        painter.text(pos2(x, y), Align2::LEFT_TOP, l, font.clone(), col);
        if menu && last {
            chevron(&painter, pos2(x + label_w + 5.0, y + 7.0), 3.0, col);
        }
        y += 12.5;
    }
    if split {
        // The chevron sits in the reserved strip beside the label's last line, never on its text.
        let chevron_y = rect.min.y + 39.0 + 12.5 * lines.len().saturating_sub(1) as f32 + 7.0;
        chevron(&painter, pos2(rect.max.x - 7.0, chevron_y), 3.0, col);
    }
    (if split { main_resp } else { resp }, arrow_resp)
}

/// A big button's label in one or two lines: at its `\n`, else (when it is long) at the space
/// nearest the middle, else, for CJK text, which has no spaces, between the characters nearest the
/// middle that may break ([`cjk_break`]).
fn wrap_label(label: &str) -> Vec<&str> {
    if let Some((a, b)) = label.split_once('\n') {
        return vec![a, b];
    }
    if label_width(label) <= 8 {
        return vec![label];
    }
    // One katakana word (アクション, オーディオ) reads badly broken; up to six letters it stays whole.
    if label_width(label) <= 12 && label.chars().all(|c| matches!(c, '\u{30A0}'..='\u{30FF}')) {
        return vec![label];
    }
    let words: Vec<&str> = label.split(' ').collect();
    if words.len() >= 2 {
        // Break near the middle.
        let mut best = 1;
        let mut best_diff = usize::MAX;
        for k in 1..words.len() {
            let a: usize = words.iter().take(k).map(|w| label_width(w) + 1).sum();
            let b: usize = words.iter().skip(k).map(|w| label_width(w) + 1).sum();
            let d = a.abs_diff(b);
            if d < best_diff {
                best_diff = d;
                best = k;
            }
        }
        let split_at: usize = words.iter().take(best).map(|w| w.len() + 1).sum::<usize>().saturating_sub(1);
        if let (Some(a), Some(b)) = (label.get(..split_at), label.get(split_at + 1..)) {
            return vec![a, b];
        }
    }
    if let Some(at) = cjk_break(label)
        && let (Some(a), Some(b)) = (label.get(..at), label.get(at..))
    {
        return vec![a, b];
    }
    vec![label]
}

/// A label's width in Latin letters: a CJK character is about two.
fn label_width(s: &str) -> usize {
    s.chars().map(|c| if crate::i18n::is_cjk(c) { 2 } else { 1 }).sum()
}

/// Where to break a CJK label that has no space (a byte index), or `None` for text without CJK.
/// The break nearest the middle, preferring a change of script (`新しい|スライド`), never before
/// closing punctuation, small kana or the long-vowel mark, never after an opening bracket and
/// never inside a Latin word or number.
fn cjk_break(label: &str) -> Option<usize> {
    if !label.chars().any(crate::i18n::is_cjk) {
        return None;
    }
    // No line starts with these (kinsoku), nor, in a label, with ん/ン, which ends a syllable
    // (スクリーン|ショット, not スクリー|ンショット).
    const NO_START: &str = "、。，．,.・：；:;？！?!）)」』】〕］]｝}〉》ー…‥ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々ゝゞヽヾ%％んン";
    // No line ends with these.
    const NO_END: &str = "（(「『【〔［[｛{〈《";
    let script = |c: char| match c {
        '\u{3040}'..='\u{309F}' => 1,
        '\u{30A0}'..='\u{30FF}' | '\u{FF66}'..='\u{FF9F}' => 2,
        c if crate::i18n::is_cjk(c) => 3,
        _ => 0,
    };
    let total = label_width(label);
    let mut best: Option<(usize, usize)> = None;
    let mut before = 0;
    let mut chars = label.char_indices().peekable();
    while let Some((_, prev)) = chars.next() {
        before += label_width(prev.encode_utf8(&mut [0; 4]));
        let Some(&(at, next)) = chars.peek() else { break };
        if NO_START.contains(next) || NO_END.contains(prev) || (prev.is_ascii_alphanumeric() && next.is_ascii_alphanumeric()) {
            continue;
        }
        let score = before.abs_diff(total - before) * 2 + usize::from(script(prev) == script(next));
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, at));
        }
    }
    best.map(|(_, at)| at)
}

pub fn chevron(p: &egui::Painter, c: egui::Pos2, s: f32, col: Color32) {
    p.line_segment([pos2(c.x - s, c.y - s * 0.5), pos2(c.x, c.y + s * 0.5)], Stroke::new(1.2, col));
    p.line_segment([pos2(c.x, c.y + s * 0.5), pos2(c.x + s, c.y - s * 0.5)], Stroke::new(1.2, col));
}

/// A small ribbon button: 16 px icon + label in one row (label may be empty).
pub fn small_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool) -> Response {
    small_button_ex(ui, icon, label, enabled, selected, false).0
}

/// Draw a small named button, optionally with a separately named options chevron, and return
/// both click targets.
pub fn small_button_ex(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool, split: bool) -> (Response, Option<Response>) {
    small_button_named(ui, icon, label, if label.is_empty() { "Options" } else { label }, enabled, selected, split)
}

/// A small ribbon button with the tooltip `tip` (already in the interface language: a command's
/// label with its shortcut).
pub fn small_button_tip(ui: &mut Ui, icon: Icon, label: &str, tip: &str, enabled: bool) -> Response {
    let label = tl!(label);
    small_button_paint(ui, icon, SmallButtonText { label, name: label, tip: Some(tip) }, enabled, false, false).0
}

/// A small button whose accessible name is independent of its visible label. Split arrows are
/// named after their own command, not a shared "More options", so assistive technology can tell
/// Bullets, Numbering and the shape-format controls apart. `label` and `name` are English, shown and
/// spoken in the interface language.
pub fn small_button_named(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    name: &str,
    enabled: bool,
    selected: bool,
    split: bool,
) -> (Response, Option<Response>) {
    small_button_paint(ui, icon, SmallButtonText { label: tl!(label), name: tl!(name), tip: None }, enabled, selected, split)
}

/// The text of a small button, already in the interface language: the visible `label`, the
/// accessible `name`, and the tooltip `tip`.
#[derive(Clone, Copy)]
struct SmallButtonText<'a> {
    label: &'a str,
    name: &'a str,
    tip: Option<&'a str>,
}

/// Draws a small button labelled `label` and named `name` (both already translated). In a compact
/// ribbon group ([`crate::ribbon_fit`]) the button shows only its icon; its name stays in the
/// tooltip and for screen readers. `tip` is the tooltip; without one a compact button shows its
/// label there.
fn small_button_paint(
    ui: &mut Ui,
    icon: Icon,
    text: SmallButtonText<'_>,
    enabled: bool,
    selected: bool,
    split: bool,
) -> (Response, Option<Response>) {
    let SmallButtonText { label, name, tip } = text;
    let t = Tokens::get(ui.ctx());
    let font = theme::font(12.0);
    let label_w = if label.is_empty() { 0.0 } else { ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), t.text).size().x) + 5.0 };
    let compact = crate::ribbon_fit::compact(ui);
    if !compact {
        crate::ribbon_fit::add_saving(ui, label_w);
    }
    let tw = if compact { 0.0 } else { label_w };
    let arrow_w = if split { 12.0 } else { 0.0 };
    let w = 24.0 + tw + arrow_w;
    // As in `big_button_ex`, a split row is two targets and must not add a third named node.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), if enabled && !split { Sense::click() } else { Sense::hover() });
    if !name.is_empty() && !split {
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    }
    let painter = ui.painter_at(rect);
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - arrow_w, rect.max.y));
    let main_resp =
        if split { ui.interact(main_rect, resp.id.with("m"), if enabled { Sense::click() } else { Sense::hover() }) } else { resp.clone() };
    let arrow_resp = split.then(|| {
        ui.interact(
            Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max),
            resp.id.with("a"),
            if enabled { Sense::click() } else { Sense::hover() },
        )
    });
    if split {
        main_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
        if let Some(a) = &arrow_resp {
            a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, crate::i18n::fmt(tl!("{name} options"), &[("name", name)])));
        }
    }
    if let Some(c) = bg(ui, &main_resp, selected, &t) {
        painter.rect_filled(main_rect, CornerRadius::same(4), c);
    }
    if let Some(a) = &arrow_resp
        && let Some(c) = bg(ui, a, false, &t)
    {
        painter.rect_filled(Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max), CornerRadius::same(4), c);
    }
    icons::paint(&painter, Rect::from_center_size(pos2(rect.min.x + 12.0, rect.center().y), vec2(17.0, 17.0)), icon, t.text, !enabled);
    if !label.is_empty() && !compact {
        painter.text(pos2(rect.min.x + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, if enabled { t.text } else { t.text_faint });
    }
    if split {
        chevron(&painter, pos2(rect.max.x - 6.0, rect.center().y), 2.8, t.text_dim);
    }
    let main = if split { main_resp } else { resp };
    let main = match tip {
        Some(tip) => main.on_hover_text(tip),
        None if compact && !label.is_empty() => main.on_hover_text(label),
        None => main,
    };
    (main, arrow_resp)
}

/// Open one menu from either half of a split control that has no separate main action.
///
/// Both halves keep their own names for assistive technology, but the popup has one stable id
/// and one anchor spanning the complete painted control. Clicking the icon/label or the chevron
/// therefore opens and closes the same menu, and neither named target is a dead end.
pub fn split_menu(main: &Response, arrow: &Response, add: impl FnOnce(&mut Ui)) {
    split_popup(main, arrow).show(|ui| add(ui));
}

/// The shared popup behind [`split_menu`], for callers that adjust its width or close behavior.
pub fn split_popup<'a>(main: &'a Response, arrow: &'a Response) -> egui::Popup<'a> {
    // Pointer clicks read Context input, so resolve them before taking the data lock below.
    let main_clicked = main.clicked();
    let arrow_clicked = arrow.clicked();
    let toggle = (main_clicked || arrow_clicked).then_some(egui::SetOpenCommand::Toggle);
    let id = split_popup_id(main);
    // The popup's geometry and identity are shared, but keyboard entry belongs to the half that
    // opened it. Remember that origin while focus moves into the menu (or the Eyedropper runs).
    let anchor = main.ctx.data_mut(|data| {
        let key = id.with("anchor");
        if arrow_clicked {
            data.insert_temp(key, arrow.id);
        } else if main_clicked {
            data.insert_temp(key, main.id);
        }
        data.get_temp::<egui::Id>(key).unwrap_or(main.id)
    });
    egui::Popup::menu(main).id(id).anchor(main.rect.union(arrow.rect)).anchor_widget(anchor).open_memory(toggle)
}

/// A dropdown-only split whose menu contains a colour grid, retaining the Eyedropper hand-back.
pub fn split_color_popup<'a>(main: &'a Response, arrow: &'a Response) -> egui::Popup<'a> {
    let id = split_popup_id(main);
    let mut popup = split_popup(main, arrow).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if crate::eyedropper::result_waits_for(&main.ctx, id) {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    if popup.is_open() {
        crate::eyedropper::note_open_menu(&main.ctx, id);
    }
    popup
}

/// Derive the popup id from the main button response so a split menu follows that button.
fn split_popup_id(main: &Response) -> egui::Id {
    main.id.with("split-menu")
}

/// A menu choice ticked in the icon column (Align to Slide…), its text lined up with the
/// [`small_button`] items around it.
pub fn check_item(ui: &mut Ui, label: &str, enabled: bool, on: bool) -> Response {
    check_item_shown(ui, tl!(label), enabled, on)
}

/// [`check_item`] showing `label` as given: for a label already translated in a context of its
/// own ([`crate::i18n::tc`]).
pub fn check_item_shown(ui: &mut Ui, label: &str, enabled: bool, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(12.0);
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), t.text).size().x) + 5.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0 + tw, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, label));
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &resp, false, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    if on {
        icons::paint(&painter, Rect::from_center_size(pos2(rect.min.x + 12.0, rect.center().y), vec2(17.0, 17.0)), Icon::Check, t.text, !enabled);
    }
    painter.text(pos2(rect.min.x + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, if enabled { t.text } else { t.text_faint });
    resp
}

/// A square icon toggle (B, I, U, alignment…), named by its English `tip`.
pub fn icon_toggle(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, on: bool) -> Response {
    icon_toggle_shown(ui, icon, tl!(tip), enabled, on)
}

/// [`icon_toggle`] named by `tip` as given, already in the interface language (a command's label
/// with its shortcut).
pub fn icon_toggle_shown(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &resp, on, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    icons::paint(&painter, rect.shrink(3.0), icon, t.text, !enabled);
    // Named for screen readers (and tests) by its tooltip.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// Icon with a colour bar under it and a dropdown arrow (Font Color, Highlight, Shape Fill…),
/// named by its English `tip` in the interface language.
pub fn color_split(ui: &mut Ui, icon: Icon, tip: &str, color: Color32, enabled: bool) -> (Response, Response) {
    let t = Tokens::get(ui.ctx());
    let tip = tl!(tip);
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 22.0), Sense::hover());
    let main = ui.interact(Rect::from_min_size(rect.min, vec2(23.0, 22.0)), resp.id.with("m"), if enabled { Sense::click() } else { Sense::hover() });
    main.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    let arrow = ui.interact(
        Rect::from_min_max(pos2(rect.min.x + 23.0, rect.min.y), rect.max),
        resp.id.with("a"),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    arrow.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, crate::i18n::fmt(tl!("{name} options"), &[("name", tip)])));
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &main, false, &t) {
        painter.rect_filled(main.rect, CornerRadius::same(4), c);
    }
    if let Some(c) = bg(ui, &arrow, false, &t) {
        painter.rect_filled(arrow.rect, CornerRadius::same(4), c);
    }
    icons::paint(&painter, Rect::from_min_size(rect.min + vec2(3.0, 1.0), vec2(17.0, 15.0)), icon, t.text, !enabled);
    painter.rect_filled(
        Rect::from_min_size(rect.min + vec2(4.0, 17.0), vec2(15.0, 3.5)),
        CornerRadius::ZERO,
        if enabled { color } else { t.text_faint },
    );
    chevron(&painter, pos2(rect.max.x - 5.5, rect.center().y), 2.6, t.text_dim);
    (main.on_hover_text(tip), arrow)
}

/// A ribbon group: contents, then a thin vertical separator. `title` (English) names the group.
/// When the window is too narrow for the tab ([`crate::ribbon_fit`]) the group's small buttons drop
/// their labels, or the whole group folds into one drop-down button showing its title and `icon`.
pub fn group(ui: &mut Ui, title: &str, icon: Icon, add: impl FnOnce(&mut Ui)) {
    let start = ui.cursor().min.x;
    let position = crate::ribbon_fit::position(ui.ctx());
    let level = crate::ribbon_fit::level(ui.ctx(), title);
    if level == crate::ribbon_fit::COLLAPSED {
        collapsed_group(ui, title, position, icon, add);
    } else {
        // Not folded: a drop-down left open while it was folded doesn't come back by itself.
        let open_id = collapsed_open_id(ui, title, position);
        ui.data_mut(|d| d.remove::<bool>(open_id));
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            crate::ribbon_fit::set_compact(ui, level == crate::ribbon_fit::COMPACT);
            add(ui);
            crate::ribbon_fit::end_compact(ui.ctx());
        });
    }
    separator(ui);
    // Up to where the next group starts.
    let width = ui.cursor().min.x - start + ui.spacing().item_spacing.x;
    crate::ribbon_fit::record(ui.ctx(), title, level, width);
}

/// Where a folded group remembers whether its drop-down is open.
fn collapsed_open_id(ui: &Ui, title: &str, position: usize) -> egui::Id {
    ui.scope_id().with(("collapsed_group", title, position))
}

/// A group folded into one drop-down button (the window is too narrow): the button shows the
/// group's icon and title, and its drop-down holds the whole group as designed. The drop-down
/// stays open while it is used: only a click outside it, or on the button, closes it; clicks in the
/// menus and lists its controls open (layouts, colours, font sizes) count as inside.
fn collapsed_group(ui: &mut Ui, title: &str, position: usize, icon: Icon, add: impl FnOnce(&mut Ui)) {
    let shown = crate::i18n::label(title);
    let open_id = collapsed_open_id(ui, title, position);
    let was_open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let (resp, _) = big_button_paint(ui, icon, &shown, true, false, was_open, true);
    let resp = resp.on_hover_text(shown.replace('\n', " "));
    let mut open = was_open;
    if resp.clicked() {
        open = !open;
    } else if open && ui.input(|i| i.pointer.any_click()) {
        // Popups (the drop-down, its controls' menus and lists) sit above the ribbon; a click on
        // the window below them is outside.
        let pos = ui.input(|i| i.pointer.interact_pos());
        let layer = pos.and_then(|p| ui.ctx().layer_id_at(p));
        if !layer.is_some_and(|l| matches!(l.order, egui::Order::Foreground | egui::Order::Tooltip)) {
            open = false;
        }
    }
    // Esc closes an open list inside first, then the drop-down.
    let child_open = egui::Popup::is_any_open(ui.ctx());
    let mut visible = open;
    if open {
        egui::Popup::menu(&resp).open_bool(&mut visible).close_behavior(egui::PopupCloseBehavior::IgnoreClicks).show(|ui| {
            ui.horizontal_top(|ui| {
                ui.set_min_height(RIBBON_H);
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                add(ui);
            });
        });
        if !visible && child_open && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            visible = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, visible));
}

pub fn separator(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(13.0, RIBBON_H - 6.0), Sense::hover());
    ui.painter().line_segment([pos2(rect.center().x, rect.min.y + 6.0), pos2(rect.center().x, rect.max.y - 4.0)], Stroke::new(1.0, t.separator));
}

/// Two or three rows of small controls stacked in one ribbon column.
pub fn rows<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        ui.add_space(0.0);
        crate::ribbon_fit::begin_column(ui);
        let r = add(ui);
        crate::ribbon_fit::end_column(ui);
        r
    })
    .inner
}

/// A text dropdown field (font name, size): shows `current`, opens a popup with `add_items`.
pub fn dropdown(ui: &mut Ui, id: &str, current: &str, width: f32, enabled: bool, add_items: impl FnOnce(&mut Ui)) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(4),
        t.field_bg,
        Stroke::new(1.0, if resp.hovered() { t.text_faint } else { t.border }),
        egui::StrokeKind::Inside,
    );
    let clip = rect.shrink2(vec2(6.0, 0.0));
    painter.with_clip_rect(Rect::from_min_max(clip.min, pos2(clip.max.x - 12.0, clip.max.y))).text(
        pos2(rect.min.x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        current,
        theme::font(12.0),
        if enabled { t.text } else { t.text_faint },
    );
    chevron(&painter, pos2(rect.max.x - 9.0, rect.center().y), 3.0, t.text_dim);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, if current.is_empty() { id } else { current }));
    let _ = id;
    egui::Popup::menu(&resp).width(width.max(140.0)).show(|ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, add_items);
    });
    resp
}

/// The drop-down for a `color_grid`: a menu that stays open when its hex field is clicked
/// (egui menus close on any click inside). Picking a colour closes it (`color_grid` calls
/// `ui.close()`); other buttons in it must close it themselves.
pub fn color_popup(button: &Response) -> egui::Popup<'_> {
    let mut popup = egui::Popup::menu(button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    // A colour the Eyedropper took for this menu: open it so `color_grid` hands the colour back.
    if crate::eyedropper::result_waits_for(&button.ctx, button.id) {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    if popup.is_open() {
        crate::eyedropper::note_open_menu(&button.ctx, button.id);
    }
    popup
}

/// The colour grid popup body: theme colours with tints/shades, standard colours, extra entries.
/// Returns the picked colour (`Some(None)` = "No Fill"/"Automatic" when `none_label` is given).
pub fn color_grid(ui: &mut Ui, scheme: &ColorScheme, none_label: Option<&str>) -> Option<Option<ColorRef>> {
    // The Eyedropper's colour, handed back as if its swatch had been clicked.
    if let Some(c) = crate::eyedropper::take_result(ui.ctx()).and_then(|hex| Rgba::from_hex(&hex)) {
        ui.close();
        return Some(Some(ColorRef::rgb(c)));
    }
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    ui.set_min_width(220.0);
    ui.label(egui::RichText::new(tl!("Theme Colors")).font(theme::bold(12.0)));
    let slots = [
        SchemeSlot::Lt1,
        SchemeSlot::Dk1,
        SchemeSlot::Lt2,
        SchemeSlot::Dk2,
        SchemeSlot::Accent1,
        SchemeSlot::Accent2,
        SchemeSlot::Accent3,
        SchemeSlot::Accent4,
        SchemeSlot::Accent5,
        SchemeSlot::Accent6,
    ];
    let cell = 18.0;
    let sw = |ui: &mut Ui, c: Rgba, tip: &str| -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
        ui.painter().rect(
            r.shrink(1.0),
            CornerRadius::same(2),
            theme::to_color32(c),
            Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { t.accent } else { t.border }),
            egui::StrokeKind::Inside,
        );
        resp.on_hover_text(tl!(tip)).clicked()
    };
    egui::Grid::new("theme_colors").spacing(vec2(3.0, 1.0)).show(ui, |ui| {
        for slot in slots {
            let alias = match slot {
                SchemeSlot::Lt1 => SchemeSlot::Bg1,
                SchemeSlot::Dk1 => SchemeSlot::Tx1,
                SchemeSlot::Lt2 => SchemeSlot::Bg2,
                SchemeSlot::Dk2 => SchemeSlot::Tx2,
                s => s,
            };
            if sw(ui, scheme.get(slot), slot.label()) {
                out = Some(Some(ColorRef::scheme(alias)));
            }
        }
        ui.end_row();
        for row in 0..5 {
            for slot in slots {
                let base = scheme.get(slot);
                let vars = deckcraft_color::theme_variants(base);
                let Some((mods, label)) = vars.get(row) else { continue };
                let c = deckcraft_color::apply(base, mods);
                if sw(ui, c, label) {
                    let alias = match slot {
                        SchemeSlot::Lt1 => SchemeSlot::Bg1,
                        SchemeSlot::Dk1 => SchemeSlot::Tx1,
                        SchemeSlot::Lt2 => SchemeSlot::Bg2,
                        SchemeSlot::Dk2 => SchemeSlot::Tx2,
                        s => s,
                    };
                    let mut r = ColorRef::scheme(alias);
                    r.mods = mods.clone();
                    out = Some(Some(r));
                }
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Standard Colors")).font(theme::bold(12.0)));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (c, name) in deckcraft_color::STANDARD_COLORS {
            if sw(ui, c, name) {
                out = Some(Some(ColorRef::rgb(c)));
            }
        }
    });
    ui.separator();
    if let Some(n) = none_label
        && ui.button(tl!(n)).clicked()
    {
        out = Some(None);
    }
    // "More Colors…": a hex field.
    let id = ui.scope_id().with("hex");
    let mut hex: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.label(tl!("More Colors… #"));
        let r = ui.add(egui::TextEdit::singleline(&mut hex).desired_width(70.0).hint_text("RRGGBB"));
        if r.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Some(c) = Rgba::from_hex(&hex)
        {
            out = Some(Some(ColorRef::rgb(c)));
        }
    });
    ui.data_mut(|d| d.insert_temp(id, hex));
    if ui.button(tl!("Eyedropper")).on_hover_text(tl!("Click the slide to take the colour under the pointer")).clicked() {
        crate::eyedropper::request(ui.ctx());
        ui.close();
    }
    if out.is_some() {
        ui.close();
    }
    out
}

/// A row of gallery tiles drawn by `paint_tile`, with tooltips from `tip` shown as given (the
/// caller translates interface vocabulary such as transition names; theme names are the
/// document's and stay as they are); returns the clicked index.
pub fn gallery(
    ui: &mut Ui,
    n: usize,
    tile: Vec2,
    selected: Option<usize>,
    mut paint_tile: impl FnMut(&egui::Painter, Rect, usize),
    mut tip: impl FnMut(usize) -> String,
) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let mut clicked = None;
    for i in 0..n {
        let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
        let painter = ui.painter_at(rect.expand(2.0));
        let sel = selected == Some(i);
        if sel || resp.hovered() {
            painter.rect_filled(rect.expand(1.0), CornerRadius::same(4), if sel { t.selected } else { t.hover });
        }
        paint_tile(&painter, rect.shrink(3.0), i);
        if sel {
            painter.rect_stroke(rect.expand(1.0), CornerRadius::same(4), Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
        if resp.on_hover_text(tip(i)).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// A number field with up/down steppers, named by the visible label of its row. Returns the new
/// value when committed.
pub fn spinner(ui: &mut Ui, id: &str, label: &str, value: f64, step: f64, suffix: &str, width: f32) -> Option<f64> {
    let t = Tokens::get(ui.ctx());
    let key = ui.scope_id().with(id);
    // The row's label, in the interface language (callers pass it English or already translated).
    let label = tl!(label);
    // What is being typed, kept while the field has the keyboard.
    let typing = ui.data_mut(|d| d.get_temp::<String>(key));
    let mut text: String = typing.clone().unwrap_or_else(|| fmt_num(value, suffix));
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let r = ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(width).font(theme::font(12.0)))
            .accessible_name(crate::i18n::fmt(tl!("{label} value"), &[("label", label)]));
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(key));
        }
        // egui reports a lost focus on two frames running; only the one that still has what was
        // typed commits it (the other would commit the value shown again).
        if r.lost_focus() && typing.is_some() {
            let num: String = ascii_number_text(&text).chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
            if let Some(v) = num.parse::<f64>().ok().filter(|v| v.is_finite()) {
                out = Some(v);
            }
        }
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 22.0), Sense::hover());
        let up = ui.interact(Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y)), key.with("up"), Sense::click());
        up.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::fmt(tl!("Increase {label}"), &[("label", label)])));
        let down = ui.interact(Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max), key.with("dn"), Sense::click());
        down.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::fmt(tl!("Decrease {label}"), &[("label", label)])));
        let p = ui.painter();
        let c = rect.center();
        p.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x, c.y - 6.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x, c.y - 6.0), pos2(c.x + 3.0, c.y - 3.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x - 3.0, c.y + 3.0), pos2(c.x, c.y + 6.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x, c.y + 6.0), pos2(c.x + 3.0, c.y + 3.0)], Stroke::new(1.0, t.text_dim));
        if up.clicked() {
            out = Some(value + step);
        }
        if down.clicked() {
            out = Some(value - step);
        }
    });
    out
}

/// What was typed into a number field, in ASCII. A Japanese input method in kana mode commits
/// full-width forms (`１８`, `－１．５`, `２４ｐｔ`, `５０％`): the full-width ASCII block
/// (U+FF01–FF5E) becomes ASCII, the ideographic space a space and the minus sign (U+2212) and
/// the small and full-width hyphen-minus forms `-`, so every number field reads them as typed.
pub fn ascii_number_text(s: &str) -> std::borrow::Cow<'_, str> {
    if s.is_ascii() {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.chars()
            .map(|c| match c {
                '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                '\u{3000}' => ' ',
                '\u{2212}' | '\u{FE63}' => '-',
                _ => c,
            })
            .collect(),
    )
}

/// A number typed into a field ([`ascii_number_text`] forms accepted, spaces around it ignored);
/// `None` unless it is a finite number.
pub fn parse_number(s: &str) -> Option<f64> {
    ascii_number_text(s).trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn fmt_num(v: f64, suffix: &str) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    format!("{s}{suffix}")
}

/// The text a kept-open text box shows: the typed draft while the stored value is still what the
/// draft started from, otherwise the stored value (another document, a reorder or an undo changed it).
pub fn text_draft(stored: Option<(String, String)>, current: &str) -> String {
    match stored {
        Some((base, buf)) if base == current => buf,
        _ => current.to_string(),
    }
}

/// Section header inside panes.
pub fn section(ui: &mut Ui, title: &str, open: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let p = ui.painter();
    let c = pos2(rect.min.x + 8.0, rect.center().y);
    if open {
        chevron(p, c, 3.5, t.text);
    } else {
        p.line_segment([pos2(c.x - 2.0, c.y - 4.0), pos2(c.x + 2.0, c.y)], Stroke::new(1.2, t.text));
        p.line_segment([pos2(c.x + 2.0, c.y), pos2(c.x - 2.0, c.y + 4.0)], Stroke::new(1.2, t.text));
    }
    p.text(pos2(rect.min.x + 18.0, rect.center().y), Align2::LEFT_CENTER, tl!(title), theme::bold(13.0), t.text);
    if resp.clicked() { !open } else { open }
}

/// A button with an optional label and a painted dropdown chevron (the UI font has no ▾).
pub fn drop_button(ui: &mut Ui, label: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let shown = tl!(label);
    let r = ui
        .add_enabled(enabled, egui::Button::new(if label.is_empty() { String::new() } else { format!("{shown}   ") }).min_size(size))
        .on_hover_text(if label.is_empty() { tl!("Options") } else { shown });
    let col = if enabled { t.text_dim } else { t.text_faint };
    chevron(ui.painter(), egui::pos2(r.rect.max.x - 8.0, r.rect.center().y), 3.0, col);
    r
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::cell::RefCell;

    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    /// Office's large split button: the icon and the visible label text run the main command;
    /// only the chevron at the end of the label row opens the options menu. The old geometry made
    /// the whole label row the arrow target, so clicking "Paste" opened its menu instead of
    /// pasting.
    #[test]
    fn a_large_split_buttons_label_runs_the_command_and_its_chevron_opens_the_options() {
        use std::cell::Cell;
        use std::rc::Rc;

        let main_clicks = Rc::new(Cell::new(0u32));
        let arrow_clicks = Rc::new(Cell::new(0u32));
        let (main_counter, arrow_counter) = (main_clicks.clone(), arrow_clicks.clone());
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(200.0, 100.0)).build_ui(move |ui| {
            theme::install_fonts(ui.ctx());
            let (main, arrow) = big_button_ex(ui, Icon::Paste, "Paste", true, true, false);
            if main.clicked() {
                main_counter.set(main_counter.get() + 1);
            }
            if arrow.is_some_and(|a| a.clicked()) {
                arrow_counter.set(arrow_counter.get() + 1);
            }
        });
        harness.run();
        let main_rect = harness.get_by_role_and_label(Role::Button, "Paste").rect();
        let arrow_rect = harness.get_by_role_and_label(Role::Button, "Paste options").rect();
        let label = egui::pos2(main_rect.center().x, main_rect.min.y + 45.0);
        assert!(main_rect.contains(label) && !arrow_rect.contains(label), "the visible label is outside the chevron strip");

        /// Nested large-split-button test helper: move and press/release the primary pointer at a
        /// position.
        fn click(harness: &mut egui_kittest::Harness<'_, ()>, pos: egui::Pos2) {
            harness.input_mut().events.push(egui::Event::PointerMoved(pos));
            harness.run();
            for pressed in [true, false] {
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run();
            }
        }
        click(&mut harness, label);
        assert_eq!(main_clicks.get(), 1, "clicking the label runs the main command");
        assert_eq!(arrow_clicks.get(), 0, "clicking the label does not open the menu");

        click(&mut harness, arrow_rect.center());
        assert_eq!(main_clicks.get(), 1, "clicking the chevron does not run the main command");
        assert_eq!(arrow_clicks.get(), 1, "clicking the chevron opens the options");
    }

    /// A two-line split label reserves the arrow strip outside *all* visible text. The click
    /// lands on the last line's widest glyph pixels (the old overlap) and runs the main command;
    /// only the named arrow opens the menu.
    #[test]
    fn a_two_line_split_labels_last_text_pixels_run_the_command() {
        use std::cell::Cell;
        use std::rc::Rc;

        let main_clicks = Rc::new(Cell::new(0u32));
        let arrow_clicks = Rc::new(Cell::new(0u32));
        let last_line = Rc::new(RefCell::new(None));
        let (main_counter, arrow_counter) = (main_clicks.clone(), arrow_clicks.clone());
        let (last_line_out, last_line_cell) = (last_line.clone(), last_line.clone());
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(220.0, 100.0)).build_ui(move |ui| {
            theme::install_fonts(ui.ctx());
            let label = "Send\nBackward";
            let (main, arrow) = big_button_ex(ui, Icon::SendBackward, label, true, true, false);
            let font = theme::font(11.0);
            let col = Tokens::get(ui.ctx()).text;
            let lines = wrap_label(label);
            let rect = main.rect;
            let label_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - SPLIT_ARROW_W, rect.max.y));
            let last = lines.last().copied().unwrap_or_default();
            let last_w = ui.fonts_mut(|f| f.layout_no_wrap(last.to_string(), font.clone(), col).size().x);
            let y = rect.min.y + 39.0 + 12.5 * lines.len().saturating_sub(1) as f32;
            let x = label_rect.center().x - last_w / 2.0;
            *last_line_cell.borrow_mut() = Some(Rect::from_min_size(pos2(x, y), vec2(last_w, 12.0)));
            if main.clicked() {
                main_counter.set(main_counter.get() + 1);
            }
            if arrow.is_some_and(|a| a.clicked()) {
                arrow_counter.set(arrow_counter.get() + 1);
            }
        });
        harness.run();
        let arrow_rect = harness.get_by_role_and_label(Role::Button, "Send Backward options").rect();
        let text = last_line_out.borrow().expect("last line laid out");
        assert!(text.max.x < arrow_rect.min.x, "the reserved strip is outside all visible text: {text:?} vs {arrow_rect:?}");
        let label_pixels = pos2(text.max.x - 1.0, text.center().y);
        assert!(!arrow_rect.contains(label_pixels));

        /// Nested two-line-split-button test helper: move and press/release the primary pointer at a
        /// position.
        fn click(harness: &mut egui_kittest::Harness<'_, ()>, pos: egui::Pos2) {
            harness.input_mut().events.push(egui::Event::PointerMoved(pos));
            harness.run();
            for pressed in [true, false] {
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run();
            }
        }
        click(&mut harness, label_pixels);
        assert_eq!(main_clicks.get(), 1, "the last line's real text pixels run the main command");
        assert_eq!(arrow_clicks.get(), 0, "the last line's text does not open the menu");

        click(&mut harness, arrow_rect.center());
        assert_eq!(main_clicks.get(), 1, "the arrow does not run the main command");
        assert_eq!(arrow_clicks.get(), 1, "the arrow opens the options");
    }

    /// Small split arrows carry their own command name; bullets and numbering must not both be
    /// announced as a generic "More options" target.
    #[test]
    fn small_split_arrows_are_named_for_their_own_commands() {
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 100.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let _ = small_button_named(ui, Icon::Bullets, "", "Bullets", true, false, true);
            let _ = small_button_named(ui, Icon::Numbering, "", "Numbering", true, false, true);
        });
        harness.run();
        for name in ["Bullets", "Bullets options", "Numbering", "Numbering options"] {
            assert!(harness.get_by_role_and_label(Role::Button, name).rect().width() > 0.0, "{name} is a named target");
        }
        assert!(harness.query_by_role_and_label(Role::Button, "More options").is_none(), "no shared arrow name remains");
    }

    /// The custom ribbon dropdown is a real combo box in the accessibility tree, named by its
    /// current value (0.37's strict unnamed-input check would otherwise reject it).
    #[test]
    fn the_dropdown_has_a_combo_box_role_and_a_name() {
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 200.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            dropdown(ui, "font-size", "18 pt", 120.0, true, |ui| {
                ui.label("12 pt");
                ui.label("18 pt");
            });
        });
        harness.run();
        let combo = harness.get_by_role_and_label(Role::ComboBox, "18 pt");
        assert!(combo.rect().width() > 100.0);
    }

    /// #32: clicking the "More Colors" hex field closed the colour menu before anything could be
    /// typed. It stays open now; Enter picks the colour and closes it.
    #[test]
    fn the_hex_field_keeps_the_color_menu_open() {
        let picked = RefCell::new(None);
        let scheme = deckcraft_model::Theme::default().colors;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 500.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let r = ui.button("Pick");
            color_popup(&r).show(|ui| {
                if let Some(c) = color_grid(ui, &scheme, None) {
                    *picked.borrow_mut() = Some(c);
                }
            });
        });
        harness.get_by_label("Pick").click();
        harness.run();
        harness.get_by_role(Role::TextInput).click();
        harness.run();
        assert!(harness.query_by_role(Role::TextInput).is_some(), "the menu is still open after clicking the hex field");
        harness.get_by_role(Role::TextInput).type_text("00FF00");
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(*picked.borrow(), Some(Some(ColorRef::rgb(Rgba::from_hex("00FF00").unwrap()))));
        assert!(harness.query_by_role(Role::TextInput).is_none(), "picking a colour closes the menu");
    }

    /// Choosing Eyedropper closes the menu and asks for it; the colour it takes comes back through
    /// the same menu, as if its swatch had been clicked.
    #[test]
    fn the_eyedroppers_colour_comes_back_through_the_menu_that_asked() {
        let picked = RefCell::new(None);
        let scheme = deckcraft_model::Theme::default().colors;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 500.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let r = ui.button("Pick");
            color_popup(&r).show(|ui| {
                if let Some(c) = color_grid(ui, &scheme, None) {
                    *picked.borrow_mut() = Some(c);
                }
            });
        });
        harness.get_by_label("Pick").click();
        harness.run();
        harness.get_by_label("Eyedropper").click();
        harness.run();
        assert!(harness.query_by_label("Eyedropper").is_none(), "choosing it closes the menu");
        assert!(picked.borrow().is_none(), "nothing is picked yet");
        // The app starts the Eyedropper from this request; here the colour is taken at once.
        assert!(harness.ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("deckcraft.eyedropper.request"))).unwrap_or(false));
        let menu = crate::eyedropper::open_menu(&harness.ctx).expect("the open menu noted its button");
        crate::eyedropper::store_result(&harness.ctx, menu, "00FF00");
        harness.run();
        assert_eq!(*picked.borrow(), Some(Some(ColorRef::rgb(Rgba::from_hex("00FF00").unwrap()))));
        assert!(harness.query_by_label("Eyedropper").is_none(), "the menu closes again");
        harness.run();
        assert_eq!(picked.borrow().iter().count(), 1, "the colour is handed back once");
    }

    /// English labels wrap as before: at their `\n`, else at the space nearest the middle once
    /// longer than eight letters.
    #[test]
    fn english_labels_wrap_at_spaces() {
        assert_eq!(wrap_label("New\nSlide"), ["New", "Slide"]);
        assert_eq!(wrap_label("Paste"), ["Paste"]);
        assert_eq!(wrap_label("Find All"), ["Find All"], "eight characters stay on one line");
        assert_eq!(wrap_label("Selection Pane"), ["Selection", "Pane"]);
        assert_eq!(wrap_label("Reuse Slides from File"), ["Reuse Slides", "from File"]);
    }

    /// CJK labels have no spaces: a long one breaks between characters near the middle, at a
    /// change of script when there is one, never before closing punctuation, small kana or the
    /// long-vowel mark, and never inside a Latin word.
    #[test]
    fn cjk_labels_wrap_between_characters() {
        assert_eq!(wrap_label("貼り付け"), ["貼り付け"], "four characters fit on one line");
        assert_eq!(wrap_label("オーディオ"), ["オーディオ"], "a short katakana word stays whole");
        assert_eq!(wrap_label("アクション"), ["アクション"]);
        assert_eq!(wrap_label("スクリーンショット"), ["スクリーン", "ショット"], "a long one still breaks, not before ン");
        assert_eq!(wrap_label("新しいスライド"), ["新しい", "スライド"]);
        assert_eq!(wrap_label("スライド ショー"), ["スライド", "ショー"], "a space still wins");
        for label in ["スライドショーの設定", "アニメーションウィンドウ", "フォント（詳細）…", "PDFとして書き出し", "図形の書式設定"]
        {
            let lines = wrap_label(label);
            assert_eq!(lines.len(), 2, "{label}");
            assert_eq!(lines.concat(), label);
            let second = lines[1].chars().next().unwrap();
            assert!(!"ーッャュョ）…、。".contains(second), "{label}: a line starts with {second}");
            assert!(!lines[0].ends_with('（'), "{label}");
            assert!(!(lines[0].ends_with(|c: char| c.is_ascii_alphabetic()) && lines[1].starts_with(|c: char| c.is_ascii_alphabetic())), "{label}");
        }
        // Widths balance: neither line holds most of the label.
        let lines = wrap_label("アニメーションウィンドウ");
        assert!(label_width(lines[0]).abs_diff(label_width(lines[1])) <= 4, "{lines:?}");
    }

    /// #413: a draft is kept while the stored text is unchanged ...
    #[test]
    fn text_draft_keeps_typing_while_the_stored_text_is_unchanged() {
        let stored = Some(("old".to_string(), "old plus typing".to_string()));
        assert_eq!(text_draft(stored, "old"), "old plus typing");
    }

    /// ... and dropped when the stored text changes underneath it.
    #[test]
    fn text_draft_drops_a_stale_draft_when_the_stored_text_changes() {
        let stored = Some(("old".to_string(), "old plus typing".to_string()));
        assert_eq!(text_draft(stored, "undone"), "undone");
        assert_eq!(text_draft(None, "fresh"), "fresh");
    }

    /// Numbers typed through a Japanese input method in kana mode arrive full-width.
    #[test]
    fn number_fields_read_full_width_digits_signs_and_units() {
        assert_eq!(ascii_number_text("１８"), "18");
        assert_eq!(ascii_number_text("－１．５ｐｔ"), "-1.5pt");
        assert_eq!(ascii_number_text("\u{2212}2\u{3000}"), "-2 ");
        assert_eq!(ascii_number_text("５０％"), "50%");
        assert!(matches!(ascii_number_text("12.5"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(parse_number(" １２．５ "), Some(12.5));
        assert_eq!(parse_number("\u{3000}－３\u{3000}"), Some(-3.0));
        assert_eq!(parse_number("十八"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("ｉｎｆ"), None);
        assert_eq!(parse_number(&"９".repeat(400)), None);
    }

    /// The ribbon's number boxes (the Format pane's margins…) commit full-width digits, once.
    #[test]
    fn spinner_commits_full_width_digits() {
        let got = RefCell::new(Vec::new());
        // The harness draws in egui's default fonts, which have no full-width forms; what is read
        // from the field is tested here, not how it is drawn.
        let mut h = egui_kittest::Harness::builder().with_size(vec2(200.0, 60.0)).allow_missing_glyphs().build_ui(|ui| {
            if let Some(v) = spinner(ui, "size", "Size", 12.0, 1.0, " pt", 60.0) {
                got.borrow_mut().push(v);
            }
        });
        h.run();
        h.get_by_role(Role::TextInput).click();
        h.run();
        h.key_press(egui::Key::End);
        for _ in 0.."12 pt".len() {
            h.key_press(egui::Key::Backspace);
        }
        h.run();
        assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some(""));
        // What a Japanese input method commits in kana mode.
        h.event(egui::Event::Text("１８ｐｔ".into()));
        h.run();
        assert!(got.borrow().is_empty(), "nothing commits while typing");
        h.key_press(egui::Key::Enter);
        h.run();
        // egui reports the lost focus on two frames; the value shown again isn't committed after it.
        assert_eq!(*got.borrow(), vec![18.0]);
    }
}
