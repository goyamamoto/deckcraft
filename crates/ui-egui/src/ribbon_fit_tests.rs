//! The ribbon fitting its groups to the window: the planner, and the real ribbon in the app.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn g(title: &str, full: f32, compact: Option<f32>, collapsed: Option<f32>) -> GroupWidths {
    GroupWidths { title: title.into(), widths: [Some(full), compact, collapsed] }
}

#[test]
fn everything_full_when_it_fits() {
    let gs = [g("A", 100.0, Some(80.0), Some(60.0)), g("B", 100.0, Some(80.0), Some(60.0))];
    assert_eq!(plan(&gs, 200.0), vec![FULL, FULL]);
}

#[test]
fn rightmost_groups_compact_first_then_collapse() {
    let gs = [g("A", 100.0, Some(80.0), Some(60.0)), g("B", 100.0, Some(80.0), Some(60.0)), g("C", 100.0, Some(80.0), Some(60.0))];
    // 300 → C compact (280) fits 285.
    assert_eq!(plan(&gs, 285.0), vec![FULL, FULL, COMPACT]);
    // All compact is 240; then C collapses (220) to fit 230.
    assert_eq!(plan(&gs, 230.0), vec![COMPACT, COMPACT, COLLAPSED]);
    // Nothing fits: everything collapsed (the row scrolls).
    assert!(plan(&gs, 10.0).iter().all(|l| *l == COLLAPSED));
}

#[test]
fn groups_that_still_fit_are_given_back() {
    // A wide gallery group in the middle and big-button groups that don't shrink.
    let gs = [
        g("Font", 300.0, Some(250.0), Some(60.0)),
        g("Transition to This Slide", 650.0, Some(650.0), Some(60.0)),
        g("Effect Options", 60.0, Some(60.0), Some(60.0)),
        g("Timing", 60.0, Some(60.0), Some(60.0)),
    ];
    // 1070 → 900: the gallery collapses; the others stay as designed.
    assert_eq!(plan(&gs, 900.0), vec![FULL, COLLAPSED, FULL, FULL]);
}

#[test]
fn same_titles_keep_their_own_levels() {
    let gs = [g("Placeholders", 300.0, Some(300.0), Some(60.0)), g("Placeholders", 100.0, Some(100.0), Some(60.0))];
    assert_eq!(plan(&gs, 400.0), vec![FULL, FULL]);
    // The second group folds by its own width (100 → 60), which is enough.
    assert_eq!(plan(&gs, 380.0), vec![FULL, COLLAPSED]);
}

/// A group to the right never shows more than one to its left, even where it alone would fit.
#[test]
fn the_first_groups_are_the_last_to_shrink() {
    let gs =
        [g("Font", 340.0, Some(340.0), Some(60.0)), g("Paragraph", 280.0, Some(280.0), Some(80.0)), g("Editing", 190.0, Some(100.0), Some(70.0))];
    // 810 in 700: Editing compact (720), then Editing folds (690).
    assert_eq!(plan(&gs, 700.0), vec![FULL, FULL, COLLAPSED]);
    // 500: Paragraph folds too (490 with Editing compact), and Editing stays no wider than it.
    let p = plan(&gs, 500.0);
    assert_eq!(p[..2], [FULL, COLLAPSED]);
    assert_eq!(p[2], COLLAPSED);
    // A group that would be no narrower folded stays as designed.
    let big = [g("Designer", 60.0, Some(60.0), Some(64.0)), g("Editing", 190.0, Some(100.0), Some(70.0))];
    assert_eq!(plan(&big, 130.0), vec![FULL, COLLAPSED]);
}

#[test]
fn unmeasured_widths_are_estimated() {
    let gs = [g("A", 200.0, None, None), g("B", 200.0, None, None)];
    // Compact unknown counts as full, so the planner collapses B (estimated 64) and A, which then
    // fits as designed, is given back.
    assert_eq!(plan(&gs, 300.0), vec![FULL, COLLAPSED]);
    // Hostile numbers don't panic.
    let odd = [g("X", f32::NAN, None, None), g("Y", f32::INFINITY, Some(f32::NAN), None)];
    let _ = plan(&odd, 100.0);
    let _ = plan(&odd, f32::NAN);
    assert!(plan(&[], 100.0).is_empty());
}

/// The real ribbon in the app.
mod ui {
    use super::super::*;
    use crate::{Services, SlideApp};
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use serde_json::json;

    fn harness(width: f32, tab: &str) -> Harness<'static, SlideApp> {
        let mut session = deckcraft_engine::Session::new();
        deckcraft_engine::sample::open_sample(&mut session).unwrap();
        let mut app = SlideApp::new(session, Services::default());
        app.run("view.tab", json!({ "tab": tab })).unwrap();
        let mut h = Harness::builder().with_size(egui::vec2(width, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        // Measure, plan, measure the planned levels, plan again.
        h.run_steps(8);
        h
    }

    /// The buttons drawn in the ribbon's band (below the tabs, above the slide area), on screen.
    fn ribbon_buttons(h: &Harness<'static, SlideApp>) -> Vec<(String, egui::Rect)> {
        let top = crate::ribbon::TITLE_H.min(crate::ribbon::MAC_TITLE_BAR) + 28.0;
        let bottom = top + crate::widgets::RIBBON_H + 16.0;
        h.query_all_by_role(Role::Button)
            .filter_map(|n| {
                let r = n.rect();
                let label = n.accesskit_node().label().unwrap_or_default();
                (r.min.y >= top && r.max.y <= bottom + 4.0 && r.min.y >= 0.0).then_some((label, r))
            })
            .collect()
    }

    /// Nothing on any tab runs past the window's edge: groups shrink or fold instead (the Home tab's
    /// Editing group was cut off at 1440 pt in Japanese).
    #[test]
    fn no_ribbon_button_runs_past_the_window_edge() {
        for width in [1100.0, 1440.0] {
            for tab in crate::ribbon::TABS {
                let h = harness(width, tab);
                for (label, r) in ribbon_buttons(&h) {
                    assert!(r.max.x <= width + 0.5, "{tab} at {width}: {label:?} ends at {} past the window", r.max.x);
                }
            }
        }
    }

    /// Wide enough, every group is drawn as designed.
    #[test]
    fn a_wide_window_shows_every_group_as_designed() {
        let h = harness(2560.0, "Home");
        for title in ["Clipboard", "Slides", "Font", "Paragraph", "Drawing", "Editing", "Designer"] {
            assert_eq!(planned(&h.ctx, title), Some(FULL), "{title}");
        }
        assert!(h.query_by_label("Replace Fonts…").is_some());
    }

    /// A narrow window folds the Home tab's last groups; a folded group's button opens the group,
    /// which stays open while its controls are used and runs their commands.
    #[test]
    fn a_folded_group_opens_and_runs_its_commands() {
        let mut h = harness(900.0, "Home");
        assert_eq!(planned(&h.ctx, "Editing"), Some(COLLAPSED), "Editing folds at 900 pt");
        assert_eq!(planned(&h.ctx, "Clipboard"), Some(FULL), "the first groups stay");
        assert!(h.query_by_label("Replace Fonts…").is_none(), "the folded group's buttons are hidden");
        h.get_by_role_and_label(Role::Button, "Editing").click();
        h.run_steps(3);
        h.get_by_label("Selection Pane").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.pane.as_deref(), Some("selection"), "Selection Pane ran from the folded group");
        assert!(h.query_by_label("Replace Fonts…").is_some(), "the drop-down stays open after a click inside");
        h.get_by_role_and_label(Role::Button, "Editing").click();
        h.run_steps(3);
        assert!(h.query_by_label("Replace Fonts…").is_none(), "the button closes it again");
    }

    /// A compact group's small buttons show only their icons, and keep their names for screen
    /// readers and tooltips; menus they open keep their labels.
    #[test]
    fn compact_buttons_keep_their_names() {
        let mut compact_seen = false;
        // The window widths at which Clipboard is left compact move with the groups' measured
        // widths, which changed with egui 0.37: scan a range of them.
        for width in [1100.0, 1250.0, 1300.0, 1350.0, 1400.0, 1450.0] {
            let h = harness(width, "Home");
            if planned(&h.ctx, "Clipboard") != Some(COMPACT) {
                continue;
            }
            compact_seen = true;
            let cut = h.get_by_label("Cut").rect();
            assert!(cut.width() <= 26.0, "Cut shows only its icon at {width}: {} wide", cut.width());
        }
        assert!(compact_seen, "the Clipboard group is compact at one of the widths");
        // A label dropped in a compact group is still what the Section menu's items say.
        let mut h = harness(1300.0, "Home");
        if planned(&h.ctx, "Slides") == Some(COMPACT) {
            h.get_by_label("Section").click();
            h.run_steps(3);
            assert!(h.query_by_label("Add Section").is_some_and(|n| n.rect().width() > 40.0), "menu items keep their labels");
        }
    }
}
