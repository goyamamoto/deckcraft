//! Ribbon groups that shrink to fit the window, the way PowerPoint's ribbon does: when a tab is
//! wider than the window, groups give up their small buttons' labels (compact), then fold into a
//! single drop-down button that opens the whole group (collapsed), starting from the right. The
//! longer labels of other interface languages (Japanese) need it at ordinary window widths.
//!
//! Each frame [`begin`] plans every group's level from the widths the groups measured on earlier
//! frames ([`record`]) and the width there is; [`crate::widgets::group`] reads its level with
//! [`level`]. Groups are known by their position on the tab and their (English) title, so two
//! groups with the same name keep their own widths. A group drawn as designed also estimates its
//! compact width from what its controls would give up ([`add_saving`]), so the planner can choose a
//! compact group over a collapsed one before it has ever been drawn compact. A width not measured
//! yet is estimated, and the plan settles once it is. Widths are measured again after the interface
//! language or scale changes. The ribbon still scrolls if even fully collapsed groups don't fit.
//!
//! Ported from WordCraft's `ribbon_fit.rs` (#662, #1285).

use std::collections::HashMap;

use egui::{Context, Id, LayerId};

/// The group as designed.
pub const FULL: u8 = 0;
/// Small buttons without their labels (tooltips and accessible names keep them).
pub const COMPACT: u8 = 1;
/// One drop-down button holding the whole group.
pub const COLLAPSED: u8 = 2;

/// Estimated width of a collapsed group (its button and separator) before it is measured.
const COLLAPSED_W: f32 = 64.0;

/// The least a group must give up to be folded: a group of one or two large buttons folded into a
/// drop-down holding the same buttons would only hide them.
const MIN_FOLD_GAIN: f32 = 16.0;

/// One group of a tab in drawing order: its title and measured widths at each level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupWidths {
    pub title: String,
    pub widths: [Option<f32>; 3],
}

/// Each group's level (by position) so that the row fits `avail`: groups shrink from the right,
/// first all to compact, then to collapsed, stopping as soon as the row fits; a group that would be
/// hardly narrower stays as it is (a group of large buttons gains little folded). Then whatever room
/// is left is given back from the left, until a group doesn't fit: a group never shows more than
/// one to its left, so the first groups are the last to shrink, as in PowerPoint.
pub fn plan(groups: &[GroupWidths], avail: f32) -> Vec<u8> {
    let full = |g: &GroupWidths| g.widths[0].unwrap_or(0.0);
    let width = |g: &GroupWidths, level: u8| -> f32 {
        match level {
            FULL => full(g),
            COMPACT => g.widths[1].unwrap_or(full(g)),
            _ => g.widths[2].unwrap_or(COLLAPSED_W.min(full(g))),
        }
    };
    let mut levels = vec![FULL; groups.len()];
    let mut total: f32 = groups.iter().map(full).sum();
    for target in [COMPACT, COLLAPSED] {
        for i in (0..groups.len()).rev() {
            if total <= avail {
                break;
            }
            let (Some(g), Some(level)) = (groups.get(i), levels.get_mut(i)) else { continue };
            let gain = width(g, *level) - width(g, target);
            if gain >= if target == COLLAPSED { MIN_FOLD_GAIN } else { 0.5 } {
                total -= gain;
                *level = target;
            }
        }
    }
    // Shrinking a wide group can make room the groups shrunk before it no longer need: give it
    // back, from the left.
    'give_back: for (g, level) in groups.iter().zip(levels.iter_mut()) {
        while *level > FULL {
            let wider = *level - 1;
            let next = total + width(g, wider) - width(g, *level);
            if !next.is_finite() || next > avail {
                break 'give_back;
            }
            total = next;
            *level = wider;
        }
    }
    levels
}

/// What this frame's ribbon is drawing.
#[derive(Clone, Default)]
struct Frame {
    tab: String,
    /// The plan, by position, and the titles it was made for.
    titles: Vec<String>,
    levels: Vec<u8>,
    drawn: Vec<String>,
}

/// Widths measured so far: (tab, position, group, level) → width, and each tab's group order.
#[derive(Clone, Default)]
struct Measured {
    /// The interface language and scale the widths were measured at.
    signature: String,
    widths: HashMap<(String, usize, String, u8), f32>,
    order: HashMap<String, Vec<String>>,
}

fn frame_id() -> Id {
    Id::new("ribbon_fit_frame")
}
fn measured_id() -> Id {
    Id::new("ribbon_fit_measured")
}
fn compact_id() -> Id {
    Id::new("ribbon_fit_compact")
}
fn saving_id() -> Id {
    Id::new("ribbon_fit_saving")
}
fn active_id() -> Id {
    Id::new("ribbon_fit_active")
}

/// What the measured widths depend on besides the groups themselves: the interface language, the
/// scale and the faces (the interface borrows a CJK face over the first frames, which changes how
/// wide the labels are; a probe in Latin and Japanese letters tells).
fn signature(ctx: &Context) -> String {
    let probe = ctx.fonts_mut(|f| f.layout_no_wrap("Ag あア漢".to_string(), crate::theme::font(11.0), egui::Color32::WHITE).size().x);
    format!("{}:{}:{probe:.2}", crate::i18n::current().code(), ctx.pixels_per_point())
}

/// Start drawing `tab` in a row `avail` points wide: plan the groups' levels.
pub fn begin(ctx: &Context, tab: &str, avail: f32) {
    let sig = signature(ctx);
    let (titles, groups) = ctx.data_mut(|d| {
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        if m.signature != sig {
            // Another language or scale: labels have new widths, so measure again.
            *m = Measured { signature: sig, ..Default::default() };
        }
        let titles = m.order.get(tab).cloned().unwrap_or_default();
        let groups: Vec<GroupWidths> = titles
            .iter()
            .enumerate()
            .map(|(i, t)| GroupWidths {
                title: t.clone(),
                widths: [FULL, COMPACT, COLLAPSED].map(|l| m.widths.get(&(tab.to_string(), i, t.clone(), l)).copied()),
            })
            .collect();
        (titles, groups)
    });
    let levels = if groups.iter().all(|g| g.widths[0].is_some()) { plan(&groups, avail) } else { Vec::new() };
    ctx.data_mut(|d| {
        d.insert_temp(frame_id(), Frame { tab: tab.to_string(), titles, levels, drawn: Vec::new() });
        d.insert_temp(active_id(), true);
    });
}

/// How many groups of this ribbon row are drawn so far: the next group's place, which tells apart
/// groups that share a title.
pub fn position(ctx: &Context) -> usize {
    ctx.data(|d| d.get_temp::<Frame>(frame_id()).map(|f| f.drawn.len())).unwrap_or(0)
}

/// The level the next group, `title`, draws at this frame.
pub fn level(ctx: &Context, title: &str) -> u8 {
    ctx.data(|d| {
        // Outside the ribbon row (a test drawing one group): as designed.
        if !d.get_temp::<bool>(active_id()).unwrap_or(false) {
            return None;
        }
        let f = d.get_temp::<Frame>(frame_id())?;
        let i = f.drawn.len();
        // Planned for another group here (one was added or removed): draw it as designed.
        if f.titles.get(i).map(String::as_str) != Some(title) {
            return None;
        }
        f.levels.get(i).copied()
    })
    .unwrap_or(FULL)
}

/// Group `title` drew at `level`, `width` points wide.
pub fn record(ctx: &Context, title: &str, level: u8, width: f32) {
    let changed = ctx.data_mut(|d| {
        let saving = d.remove_temp::<f32>(saving_id()).unwrap_or(0.0);
        // Only groups on the ribbon row between `begin` and `end` are planned.
        if !d.get_temp::<bool>(active_id()).unwrap_or(false) {
            return false;
        }
        let f = d.get_temp_mut_or_default::<Frame>(frame_id());
        let i = f.drawn.len();
        f.drawn.push(title.to_string());
        let tab = f.tab.clone();
        if !(width.is_finite() && width > 0.0) {
            return false;
        }
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        let before = m.widths.insert((tab.clone(), i, title.to_string(), level), width);
        // Drawn as designed: until it is drawn compact, its compact width is what its labels
        // would give back.
        if level == FULL && saving.is_finite() && saving > 0.0 {
            m.widths.entry((tab, i, title.to_string(), COMPACT)).or_insert((width - saving).max(COLLAPSED_W));
        }
        before.is_none_or(|b| (b - width).abs() > 0.5)
    });
    if changed {
        // A width measured for the first time (or anew) can change the plan: draw again with it,
        // so an idle window settles instead of keeping a plan made from estimates.
        ctx.request_repaint();
    }
}

/// The row is drawn: remember its group order for the next frame's plan.
pub fn end(ctx: &Context) {
    let changed = ctx.data_mut(|d| {
        d.insert_temp(active_id(), false);
        let drawn = std::mem::take(&mut d.get_temp_mut_or_default::<Frame>(frame_id()).drawn);
        let tab = d.get_temp_mut_or_default::<Frame>(frame_id()).tab.clone();
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        let changed = m.order.get(&tab) != Some(&drawn);
        if changed {
            m.order.insert(tab, drawn);
        }
        changed
    });
    if changed {
        // A new order (another tab, a group appeared): plan again with it next frame.
        ctx.request_repaint();
    }
}

/// The group being drawn: the layer its controls are drawn on and whether it is compact.
fn drawing(ui: &egui::Ui) -> Option<bool> {
    let (layer, on) = ui.ctx().data(|d| d.get_temp::<(LayerId, bool)>(compact_id()))?;
    (layer == ui.layer_id()).then_some(on)
}

/// Whether a control drawn in `ui` belongs to a compact group, so a small button drops its label.
/// Only controls on the group's own layer: a menu a compact group's button opens is drawn as usual.
pub fn compact(ui: &egui::Ui) -> bool {
    drawing(ui) == Some(true)
}

/// Mark the group about to be drawn in `ui` compact or not, and start counting what it could give
/// up.
pub fn set_compact(ui: &egui::Ui, on: bool) {
    let layer = ui.layer_id();
    ui.ctx().data_mut(|d| {
        d.insert_temp(compact_id(), (layer, on));
        d.remove_temp::<f32>(saving_id());
    });
}

/// The group's controls are drawn: what follows isn't in a group (what they could give up is kept).
pub fn end_compact(ctx: &Context) {
    ctx.data_mut(|d| d.remove::<(LayerId, bool)>(compact_id()));
}

/// A control drawn in `ui`, in a group drawn as designed, would be `w` points narrower in a compact
/// group. Controls in menus and drop-downs don't count; in a column ([`begin_column`]) only the
/// widest counts.
pub fn add_saving(ui: &egui::Ui, w: f32) {
    if w.is_finite() && w > 0.0 && drawing(ui) == Some(false) {
        ui.ctx().data_mut(|d| match d.get_temp::<f32>(column_id()) {
            Some(widest) => {
                d.insert_temp(column_id(), widest.max(w));
            }
            None => *d.get_temp_mut_or_default::<f32>(saving_id()) += w,
        });
    }
}

/// Controls stacked in a column follow (Cut, Copy, Format): the column narrows by its widest
/// label, not by the sum of them.
pub fn begin_column(ui: &egui::Ui) {
    if drawing(ui) == Some(false) {
        ui.ctx().data_mut(|d| d.insert_temp(column_id(), 0.0f32));
    }
}

/// The column is drawn: what its widest control gives up counts for the group.
pub fn end_column(ui: &egui::Ui) {
    ui.ctx().data_mut(|d| {
        if let Some(widest) = d.remove_temp::<f32>(column_id())
            && widest > 0.0
        {
            *d.get_temp_mut_or_default::<f32>(saving_id()) += widest;
        }
    });
}

fn column_id() -> Id {
    Id::new("ribbon_fit_column")
}

/// The level this frame planned for the first group called `title` (tests).
#[cfg(test)]
pub(crate) fn planned(ctx: &Context, title: &str) -> Option<u8> {
    ctx.data(|d| {
        let f = d.get_temp::<Frame>(frame_id())?;
        let i = f.titles.iter().position(|t| t == title)?;
        Some(f.levels.get(i).copied().unwrap_or(FULL))
    })
}

#[cfg(test)]
#[path = "ribbon_fit_tests.rs"]
mod tests;
