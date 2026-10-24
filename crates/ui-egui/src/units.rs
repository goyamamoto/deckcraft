//! Lengths as the interface shows them. PowerPoint takes its measurement unit from the system
//! region: on a Japanese (metric) system the rulers, Size and Position, Slide Size, Paragraph
//! indents and Tabs show centimetres, in the United States inches. DeckCraft does the same
//! ([`system_region`], [`deckcraft_engine::Session::units`]), and Preferences ▸ Measurement units
//! (`tools.units`) picks inches, centimetres, millimetres or points by hand. Every length field
//! shows its unit (`2.54 cm`, `1"`) and takes a number in it or with an explicit unit (`2.5cm`,
//! `25 mm`, `1in`, `1"`, `72pt`, full-width `２．５ｃｍ`); command parameters stay in points.

use deckcraft_geom::{Region, Unit};
use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::{self, Tokens};

/// The system region (it picks the measurement unit and the default paper), in this order:
/// [`deckcraft_geom::region::REGION_ENV`] (`DECKCRAFT_REGION=ja-JP`); `en-US` for runs without
/// preferences (`DECKCRAFT_NO_PREFS`: agents' and screenshot runs behave the same on every
/// machine); on macOS, Windows and the web the platform's first preferred locale (macOS lists
/// `en-JP` for an English Mac set to Japan; a terminal's `LANG=en_US.UTF-8` doesn't say where the
/// Mac is); elsewhere the locale variables (`LC_ALL`, `LC_MEASUREMENT`, `LC_PAPER`, `LANG`), then
/// the platform. `en-US` in tests.
pub fn system_region() -> Region {
    if cfg!(test) {
        return Region::default();
    }
    let env = |name: &str| std::env::var(name).ok();
    let set = |name: &str| env(name).is_some_and(|v| !v.trim().is_empty());
    if !set(deckcraft_geom::region::REGION_ENV) && set("DECKCRAFT_NO_PREFS") {
        return Region::default();
    }
    let platform = || sys_locale::get_locale();
    if cfg!(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")) {
        Region::from_locale_vars(|name| if name == deckcraft_geom::region::REGION_ENV { env(name) } else { None }, platform)
    } else {
        Region::from_locale_vars(env, platform)
    }
}

/// A unit's name in menus, in English (draw it through `tl!`).
pub fn unit_label(u: Unit) -> &'static str {
    match u {
        Unit::Inches => "Inches",
        Unit::Centimeters => "Centimeters",
        Unit::Millimeters => "Millimeters",
        Unit::Points => "Points",
        Unit::Pixels => "Pixels",
    }
}

/// A length in points as a field shows it in `unit` (`2.54 cm`, `1"`, `25.4 mm`, `12 pt`).
pub fn len_text(pt: f64, unit: Unit) -> String {
    unit.format(pt)
}

/// A length typed into a field, in points: a number in `unit`, or one with its own unit (`2cm`,
/// `1"`, `72 pt`, full-width digits). `None` when it isn't a length.
pub fn parse_len(text: &str, unit: Unit) -> Option<f64> {
    unit.parse(text)
}

/// How a ruler is divided in a unit at a zoom: a numbered mark every `major_pt`, `ticks` ticks
/// between two numbered marks (a taller one halfway when `ticks` is even), and a number on every
/// `label_every`-th mark, so the numbers never crowd.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerScale {
    /// Points between two numbered marks: an inch, a centimetre, ten millimetres, 72 points.
    pub major_pt: f64,
    /// What the number grows by from one numbered mark to the next (1, or 10 mm, or 72 pt).
    pub major_value: f64,
    /// Tick intervals per numbered mark (1, 2 or 4).
    pub ticks: u32,
    /// Numbered marks per number shown (1, 2, 5 or 10).
    pub label_every: u32,
}

impl RulerScale {
    /// Points between two ticks.
    pub fn tick_pt(&self) -> f64 {
        self.major_pt / f64::from(self.ticks.max(1))
    }
    /// The number on the `k`-th tick from the origin when it carries one.
    pub fn label(&self, k: u32) -> Option<String> {
        let ticks = self.ticks.max(1);
        let major = k / ticks;
        (k.is_multiple_of(ticks) && major.is_multiple_of(self.label_every.max(1)))
            .then(|| Unit::Points.number(f64::from(major) * self.major_value, 0))
    }
    /// The `k`-th tick's height: numbered marks tallest, halfway marks next.
    pub fn tick_height(&self, k: u32) -> f32 {
        let ticks = self.ticks.max(1);
        if k.is_multiple_of(ticks) {
            8.0
        } else if ticks.is_multiple_of(2) && k.is_multiple_of(ticks / 2) {
            5.0
        } else {
            3.0
        }
    }
}

/// The ruler's divisions for `unit` at `px_per_pt` screen points per slide point: inches in
/// quarters, centimetres in quarters (2.5 mm), millimetres numbered every ten, points every 72.
/// Ticks closer than 4 px and numbers closer than their width are thinned out.
pub fn ruler_scale(unit: Unit, px_per_pt: f32) -> RulerScale {
    let k = if px_per_pt.is_finite() && px_per_pt > 0.0 { f64::from(px_per_pt) } else { 1.0 };
    let (major_pt, major_value, label_px) = match unit {
        Unit::Inches => (72.0, 1.0, 14.0),
        Unit::Centimeters => (72.0 / 2.54, 1.0, 14.0),
        Unit::Millimeters => (720.0 / 25.4, 10.0, 22.0),
        Unit::Points | Unit::Pixels => (72.0, 72.0, 22.0),
    };
    let major_px = major_pt * k;
    let ticks = [4, 2, 1].into_iter().find(|t| major_px / f64::from(*t) >= 4.0).unwrap_or(1);
    let label_every = [1, 2, 5, 10, 20, 50].into_iter().find(|n| major_px * f64::from(*n) >= label_px).unwrap_or(100);
    RulerScale { major_pt, major_value, ticks, label_every }
}

/// The step the spinner arrows move a length by: a tenth of an inch, a tenth of a centimetre, a
/// millimetre or a point.
pub fn spin_step(unit: Unit) -> f64 {
    match unit {
        Unit::Inches => 7.2,
        Unit::Centimeters | Unit::Millimeters => 72.0 / 25.4,
        Unit::Points | Unit::Pixels => 1.0,
    }
}

/// A length box with up and down arrows (the ribbon's Size boxes, the Format pane): it shows `pt`
/// in `unit` and returns the new length in points when the text is committed (focus leaves it)
/// or an arrow is clicked. Text that isn't a length is dropped and the box shows the length again.
pub fn length_spinner(ui: &mut Ui, id: &str, pt: f64, unit: Unit, width: f32) -> Option<f64> {
    let t = Tokens::get(ui.ctx());
    let key = ui.id().with(id);
    let mut text: String = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| len_text(pt, unit));
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).font(theme::font(12.0)));
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(key));
        }
        if r.lost_focus() {
            out = parse_len(&text, unit);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 22.0), Sense::hover());
        let up = ui.interact(Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y)), key.with("up"), Sense::click());
        let down = ui.interact(Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max), key.with("dn"), Sense::click());
        let p = ui.painter();
        let c = rect.center();
        let ink = Stroke::new(1.0, t.text_dim);
        p.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x, c.y - 6.0)], ink);
        p.line_segment([pos2(c.x, c.y - 6.0), pos2(c.x + 3.0, c.y - 3.0)], ink);
        p.line_segment([pos2(c.x - 3.0, c.y + 3.0), pos2(c.x, c.y + 6.0)], ink);
        p.line_segment([pos2(c.x, c.y + 6.0), pos2(c.x + 3.0, c.y + 3.0)], ink);
        if up.clicked() {
            out = Some(pt + spin_step(unit));
        }
        if down.clicked() {
            out = Some(pt - spin_step(unit));
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_run_in_the_us_region() {
        assert_eq!(system_region(), Region::default());
        assert_eq!(system_region().units(), Unit::Inches);
    }

    #[test]
    fn fields_show_and_take_lengths_in_the_unit() {
        assert_eq!(len_text(72.0, Unit::Centimeters), "2.54 cm");
        assert_eq!(len_text(72.0, Unit::Inches), "1\"");
        assert_eq!(parse_len("2.54", Unit::Centimeters).map(|v| (v * 1000.0).round()), Some(72_000.0));
        assert_eq!(parse_len("1in", Unit::Centimeters), Some(72.0));
        assert_eq!(parse_len("two", Unit::Centimeters), None);
    }

    /// Centimetre rulers number every centimetre and tick every 2.5 mm at a normal zoom; zoomed
    /// out, the ticks and numbers thin out instead of running together.
    #[test]
    fn ruler_divisions_follow_the_unit_and_zoom() {
        let inch = ruler_scale(Unit::Inches, 1.0);
        assert_eq!((inch.major_pt, inch.ticks, inch.label_every), (72.0, 4, 1));
        assert_eq!((inch.label(0).as_deref(), inch.label(4).as_deref(), inch.label(5)), (Some("0"), Some("1"), None));
        assert_eq!((inch.tick_height(0), inch.tick_height(2), inch.tick_height(1)), (8.0, 5.0, 3.0));
        let cm = ruler_scale(Unit::Centimeters, 1.0);
        assert!((cm.tick_pt() - 72.0 / 2.54 / 4.0).abs() < 1e-9, "{cm:?}");
        assert_eq!((cm.ticks, cm.label_every, cm.label(8).as_deref()), (4, 1, Some("2")));
        // 33.87 cm across a widescreen slide drawn 480 px wide: half-centimetre ticks, every
        // centimetre still numbered.
        let small = ruler_scale(Unit::Centimeters, 0.5);
        assert_eq!((small.ticks, small.label_every), (2, 1));
        let tiny = ruler_scale(Unit::Centimeters, 0.2);
        assert_eq!((tiny.ticks, tiny.label_every), (1, 5));
        assert_eq!((tiny.label(5).as_deref(), tiny.label(3)), (Some("5"), None));
        let mm = ruler_scale(Unit::Millimeters, 1.0);
        assert_eq!((mm.major_value, mm.label(4).as_deref()), (10.0, Some("10")));
        let pt = ruler_scale(Unit::Points, 1.0);
        assert_eq!(pt.label(4).as_deref(), Some("72"));
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let s = ruler_scale(Unit::Centimeters, bad);
            assert!(s.tick_pt() > 0.0 && s.label_every >= 1, "{bad}: {s:?}");
        }
        assert!(ruler_scale(Unit::Inches, 1e-6).label_every >= 50);
    }
}
