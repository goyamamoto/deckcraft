use super::*;

/// A Slide Size field set to `pt`: the length as Slide Size shows it in `unit` (33.867 cm,
/// 13.333": [`slide_len_text`]), the exact points kept beside ([`put_dim`]).
pub(super) fn put_slide_dim(d: &mut Dialog, key: &str, pt: f64, unit: deckcraft_geom::Unit) {
    put_dim(d, key, pt, slide_len_text(pt, unit));
}

/// A Slide Size field in points: the exact value while the text is still the one it was filled
/// with, else the length typed (a number in `unit`, or one with its own unit: `190.5 mm`, `10"`,
/// full-width `２５．４`); `default` when it is not a length.
pub(super) fn slide_dim_points(d: &Dialog, key: &str, unit: deckcraft_geom::Unit, default: f64) -> f64 {
    length_value(d, key, unit).unwrap_or(default)
}

/// A slide's width or height as Slide Size shows it: a decimal more than other lengths, as
/// PowerPoint shows 33.867 cm and 13.333" for a widescreen slide.
fn slide_len_text(pt: f64, unit: deckcraft_geom::Unit) -> String {
    use deckcraft_geom::Unit;
    let places = if matches!(unit, Unit::Inches | Unit::Centimeters) { 3 } else { unit.places() };
    format!("{}{}", unit.number(pt, places), unit.suffix())
}
