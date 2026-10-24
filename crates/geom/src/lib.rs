//! DeckCraft geometry: units, shape transforms and preset shape generators.
//!
//! The document model keeps positions in points (1/72 inch) as `f64`. Office Open XML stores EMU
//! (English Metric Units, 12 700 per point); [`emu_to_pt`] and [`pt_to_emu`] convert losslessly for
//! integer EMU values, so files round-trip exactly.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod morph;
pub mod preset;
pub mod region;
pub mod squircle;
pub mod warp;

pub use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape as KurboShape, Size, Vec2};
pub use region::Region;
use serde::{Deserialize, Serialize};

/// EMU per point.
pub const EMU_PER_PT: f64 = 12_700.0;
/// EMU per inch.
pub const EMU_PER_IN: f64 = 914_400.0;
/// EMU per centimetre.
pub const EMU_PER_CM: f64 = 360_000.0;

pub fn emu_to_pt(emu: i64) -> f64 {
    emu as f64 / EMU_PER_PT
}

/// Points to EMU, rounded. Non-finite input becomes 0; huge values saturate.
pub fn pt_to_emu(pt: f64) -> i64 {
    if !pt.is_finite() {
        return 0;
    }
    let v = (pt * EMU_PER_PT).round();
    if v >= i64::MAX as f64 {
        i64::MAX
    } else if v <= i64::MIN as f64 {
        i64::MIN
    } else {
        v as i64
    }
}

/// Display units for measurements (inches on US English systems, centimetres elsewhere; see
/// [`Region::units`]). Lengths are kept in points everywhere; only the rulers and the length
/// fields show them in a unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    #[default]
    Inches,
    Centimeters,
    Millimeters,
    Points,
    Pixels,
}

impl Unit {
    /// The units a person can choose to measure in (Preferences), in menu order.
    pub const CHOICES: [Unit; 4] = [Unit::Inches, Unit::Centimeters, Unit::Millimeters, Unit::Points];

    pub fn points_per_unit(self) -> f64 {
        match self {
            Unit::Inches => 72.0,
            Unit::Centimeters => 72.0 / 2.54,
            Unit::Millimeters => 72.0 / 25.4,
            Unit::Points => 1.0,
            Unit::Pixels => 0.75,
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Inches => "\"",
            Unit::Centimeters => " cm",
            Unit::Millimeters => " mm",
            Unit::Points => " pt",
            Unit::Pixels => " px",
        }
    }
    /// The unit's short name, as typed after a number (`in`, `cm`, `mm`, `pt`, `px`).
    pub fn abbrev(self) -> &'static str {
        match self {
            Unit::Inches => "in",
            Unit::Centimeters => "cm",
            Unit::Millimeters => "mm",
            Unit::Points => "pt",
            Unit::Pixels => "px",
        }
    }
    /// The unit's id in settings and command parameters (`inches`, `centimeters`…).
    pub fn id(self) -> &'static str {
        match self {
            Unit::Inches => "inches",
            Unit::Centimeters => "centimeters",
            Unit::Millimeters => "millimeters",
            Unit::Points => "points",
            Unit::Pixels => "pixels",
        }
    }
    /// The unit an id or short name names (`centimeters`, `cm`, `Centimetres`), any case.
    pub fn from_id(id: &str) -> Option<Unit> {
        match id.trim().to_ascii_lowercase().as_str() {
            "inches" | "inch" | "in" => Some(Unit::Inches),
            "centimeters" | "centimetres" | "centimeter" | "centimetre" | "cm" => Some(Unit::Centimeters),
            "millimeters" | "millimetres" | "millimeter" | "millimetre" | "mm" => Some(Unit::Millimeters),
            "points" | "point" | "pt" => Some(Unit::Points),
            "pixels" | "pixel" | "px" => Some(Unit::Pixels),
            _ => None,
        }
    }
    /// How many decimals a length in this unit is shown with at most: one for millimetres, two
    /// otherwise (`2.54 cm`, `25.4 mm`, `12 pt`, `1.25"`).
    pub fn places(self) -> usize {
        match self {
            Unit::Millimeters => 1,
            _ => 2,
        }
    }
    /// `pt` as a number in this unit with at most `places` decimals and no trailing zeros, without
    /// the unit (`2.54`, `1`, `-0.5`). Non-finite input is 0.
    pub fn number(self, pt: f64, places: usize) -> String {
        let v = if pt.is_finite() { pt / self.points_per_unit() } else { 0.0 };
        let s = format!("{v:.places$}", places = places.min(6));
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
        if s == "-0" { "0".to_string() } else { s.to_string() }
    }
    /// A length in points as this unit shows it: `2.08"`, `2.54 cm`, `25.4 mm`, `12 pt`.
    pub fn format(self, pt: f64) -> String {
        format!("{}{}", self.number(pt, self.places()), self.suffix())
    }
    /// Parses `2.5`, `2.5"`, `2.5 in`, `6 cm`, `30 pt`, `40px`, `5mm` into points. A bare number
    /// uses `self`. Full-width digits, letters and marks typed through a Japanese input method
    /// (`２．５ｃｍ`) read as their ASCII forms, and `″` or `”` as an inch mark.
    pub fn parse(self, text: &str) -> Option<f64> {
        let t: String = text.chars().take(64).map(narrow).collect::<String>().trim().to_ascii_lowercase();
        let (num, unit) = split_number(&t)?;
        let ppu = match unit.trim() {
            "" => self.points_per_unit(),
            "\"" | "in" | "inch" | "inches" => 72.0,
            "cm" => 72.0 / 2.54,
            "mm" => 72.0 / 25.4,
            "pt" | "pts" | "point" | "points" => 1.0,
            "px" => 0.75,
            _ => return None,
        };
        let v = num * ppu;
        v.is_finite().then_some(v)
    }
}

/// A character as the length parser reads it: full-width ASCII forms (U+FF01..U+FF5E) and the
/// ideographic space as their ASCII counterparts, the minus sign as `-`, and the double prime
/// and right double quote as the inch mark.
fn narrow(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        '\u{2212}' => '-',
        '\u{2033}' | '\u{201D}' => '"',
        c => c,
    }
}

fn split_number(t: &str) -> Option<(f64, &str)> {
    let end = t.char_indices().find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')).map(|(i, _)| i).unwrap_or(t.len());
    let n: f64 = t.get(..end)?.parse().ok()?;
    Some((n, t.get(end..).unwrap_or("")))
}

/// A shape's placement on its slide (or in its group's child space): offset, extent, rotation
/// (degrees clockwise) and flips. Rotation and flips are about the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Xfrm {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub rot: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub flip_h: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub flip_v: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Xfrm {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Xfrm { x, y, w, h, ..Default::default() }
    }
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.x + self.w, self.y + self.h)
    }
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    /// Local shape space (0..w, 0..h) → parent space, applying flips and rotation about the centre.
    pub fn affine(&self) -> Affine {
        let c = Vec2::new(self.w / 2.0, self.h / 2.0);
        let sx = if self.flip_h { -1.0 } else { 1.0 };
        let sy = if self.flip_v { -1.0 } else { 1.0 };
        Affine::translate(Vec2::new(self.x, self.y) + c)
            * Affine::rotate(self.rot.to_radians())
            * Affine::scale_non_uniform(sx, sy)
            * Affine::translate(-c)
    }
    /// Axis-aligned bounds of the rotated box in parent space.
    pub fn bounds(&self) -> Rect {
        let a = self.affine();
        let pts = [Point::new(0.0, 0.0), Point::new(self.w, 0.0), Point::new(self.w, self.h), Point::new(0.0, self.h)].map(|p| a * p);
        let mut r = Rect::from_points(pts[0], pts[1]);
        r = r.union_pt(pts[2]).union_pt(pts[3]);
        r
    }
    /// True when no dimension is NaN/inf.
    pub fn is_finite(&self) -> bool {
        [self.x, self.y, self.w, self.h, self.rot].iter().all(|v| v.is_finite())
    }
    /// Rotation normalised to 0..360.
    pub fn rot_norm(&self) -> f64 {
        let r = self.rot % 360.0;
        if r < 0.0 { r + 360.0 } else { r }
    }
}

/// Maps a group's child coordinate space (`ch_off`, `ch_ext`) onto the group's own box.
pub fn group_child_affine(group: &Xfrm, ch: Rect) -> Affine {
    let sx = if ch.width().abs() > 1e-9 { group.w / ch.width() } else { 1.0 };
    let sy = if ch.height().abs() > 1e-9 { group.h / ch.height() } else { 1.0 };
    group.affine() * Affine::scale_non_uniform(sx, sy) * Affine::translate(-ch.origin().to_vec2())
}

/// Distance from `p` to the segment `a`–`b`.
pub fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-12 {
        return (p - a).hypot();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).hypot()
}

/// Is `p` within `tol` of the outline of `path` (flattened)?
pub fn near_path(path: &BezPath, p: Point, tol: f64) -> bool {
    let mut hit = false;
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    kurbo::flatten(path.iter(), 0.25, |el| {
        if hit {
            return;
        }
        match el {
            PathEl::MoveTo(q) => {
                start = q;
                last = q;
            }
            PathEl::LineTo(q) => {
                if dist_to_segment(p, last, q) <= tol {
                    hit = true;
                }
                last = q;
            }
            PathEl::ClosePath => {
                if dist_to_segment(p, last, start) <= tol {
                    hit = true;
                }
                last = start;
            }
            _ => {}
        }
    });
    hit
}

/// Snap `v` to the nearest multiple of `step` (step <= 0 returns `v`).
pub fn snap(v: f64, step: f64) -> f64 {
    if step > 0.0 && step.is_finite() { (v / step).round() * step } else { v }
}

/// Flatten a path to move/line/close elements (tolerance 0.2).
pub fn preset_flatten(path: &BezPath, f: &mut dyn FnMut(PathEl)) {
    kurbo::flatten(path.iter(), 0.2, f);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emu_roundtrip_is_exact() {
        for emu in [0i64, 1, 12_699, 12_700, 914_400, 12_192_000, 6_858_000, -5, 123_456_789] {
            assert_eq!(pt_to_emu(emu_to_pt(emu)), emu);
        }
        assert_eq!(pt_to_emu(f64::NAN), 0);
        assert_eq!(pt_to_emu(f64::INFINITY), 0);
    }

    #[test]
    fn unit_parse_and_format() {
        assert_eq!(Unit::Inches.parse("2"), Some(144.0));
        assert_eq!(Unit::Inches.parse("2\""), Some(144.0));
        assert!((Unit::Inches.parse("2.54 cm").unwrap() - 72.0).abs() < 1e-9);
        assert_eq!(Unit::Inches.parse("36pt"), Some(36.0));
        assert_eq!(Unit::Inches.parse("abc"), None);
        assert_eq!(Unit::Inches.parse(""), None);
        assert_eq!(Unit::Inches.format(150.0), "2.08\"");
    }

    /// Centimetres for metric regions (PowerPoint in Japan): 72 pt is 2.54 cm, and a field
    /// takes an explicit unit, full-width input from a Japanese input method and odd inch marks.
    #[test]
    fn metric_units_format_and_parse() {
        assert_eq!(Unit::Centimeters.format(72.0), "2.54 cm");
        assert_eq!(Unit::Centimeters.format(960.0), "33.87 cm", "a widescreen slide's width");
        assert_eq!(Unit::Millimeters.format(72.0), "25.4 mm");
        assert_eq!(Unit::Points.format(12.0), "12 pt");
        assert_eq!(Unit::Inches.format(72.0), "1\"");
        assert_eq!(Unit::Centimeters.format(-0.001), "0 cm");
        assert_eq!(Unit::Centimeters.format(f64::NAN), "0 cm");
        assert_eq!(Unit::Centimeters.number(960.0, 3), "33.867");
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
        assert!(close(Unit::Centimeters.parse("2.54"), 72.0));
        assert!(close(Unit::Centimeters.parse("2.54 cm"), 72.0));
        assert!(close(Unit::Centimeters.parse("25.4mm"), 72.0));
        assert!(close(Unit::Centimeters.parse("1\""), 72.0));
        assert!(close(Unit::Centimeters.parse("1 in"), 72.0));
        assert!(close(Unit::Centimeters.parse("72 pt"), 72.0));
        assert!(close(Unit::Centimeters.parse("２．５４ｃｍ"), 72.0), "full-width input");
        assert!(close(Unit::Centimeters.parse("１″"), 72.0));
        assert!(close(Unit::Centimeters.parse("1”"), 72.0));
        assert!(close(Unit::Centimeters.parse("−2.54"), -72.0));
        assert!(close(Unit::Millimeters.parse("10"), 72.0 / 2.54));
        for bad in ["", "cm", "2 furlongs", "1e999", "--1", "1.2.3"] {
            assert_eq!(Unit::Centimeters.parse(bad), None, "{bad:?}");
        }
        assert_eq!(Unit::Centimeters.parse(&"9".repeat(100_000)), Some(Unit::Centimeters.parse(&"9".repeat(64)).unwrap()), "long input is cut");
        for u in Unit::CHOICES {
            assert_eq!(Unit::from_id(u.id()), Some(u));
            assert_eq!(Unit::from_id(u.abbrev()), Some(u));
            // What a field shows reads back within its rounding.
            let back = u.parse(&u.format(123.0)).unwrap();
            assert!((back - 123.0).abs() <= u.points_per_unit() * 0.051, "{u:?}: {back}");
        }
        assert_eq!(Unit::from_id("Centimetres"), Some(Unit::Centimeters));
        assert_eq!(Unit::from_id("furlongs"), None);
    }

    #[test]
    fn xfrm_affine_rotation_about_center() {
        let x = Xfrm { x: 10.0, y: 20.0, w: 100.0, h: 50.0, rot: 90.0, ..Default::default() };
        let c = x.affine() * Point::new(50.0, 25.0);
        assert!((c.x - 60.0).abs() < 1e-9 && (c.y - 45.0).abs() < 1e-9);
        let b = x.bounds();
        assert!((b.width() - 50.0).abs() < 1e-9 && (b.height() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn flips_mirror_local_space() {
        let x = Xfrm { x: 0.0, y: 0.0, w: 100.0, h: 50.0, flip_h: true, ..Default::default() };
        let p = x.affine() * Point::new(0.0, 0.0);
        assert!((p.x - 100.0).abs() < 1e-9 && p.y.abs() < 1e-9);
    }

    #[test]
    fn group_child_mapping() {
        let g = Xfrm::new(100.0, 100.0, 200.0, 100.0);
        let a = group_child_affine(&g, Rect::new(0.0, 0.0, 100.0, 50.0));
        let p = a * Point::new(100.0, 50.0);
        assert!((p.x - 300.0).abs() < 1e-9 && (p.y - 200.0).abs() < 1e-9);
    }

    #[test]
    fn segment_distance() {
        assert!((dist_to_segment(Point::new(5.0, 5.0), Point::new(0.0, 0.0), Point::new(10.0, 0.0)) - 5.0).abs() < 1e-9);
        assert!((dist_to_segment(Point::new(0.0, 3.0), Point::new(0.0, 0.0), Point::new(0.0, 0.0)) - 3.0).abs() < 1e-9);
    }
}
