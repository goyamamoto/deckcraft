//! Preferences ▸ Measurement units: the unit the rulers and the length fields (Size and Position,
//! Slide Size, Paragraph indents, Tabs, the Size boxes) show. PowerPoint takes it from the system
//! region (centimetres in Japan and other metric regions, inches in the United States); DeckCraft
//! follows the region too ([`Session::region`]) unless a unit is chosen here. Command parameters
//! stay in points whatever the unit.

use deckcraft_geom::Unit;
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        noundo "tools.units",
        "Measurement Units",
        ["Tools"],
        None,
        "{value?: auto|inches|centimeters|millimeters|points (or in, cm, mm, pt)}: the unit rulers and length fields show; auto follows the system region → {units, effective, region, paper}",
        always,
        units
    )]
}

/// `tools.units {value?}`: set the measurement unit (`auto` follows the region) and report the
/// setting, the unit in effect, the region and its default paper.
fn units(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(v) = p.get("value").filter(|v| !v.is_null()) {
        let v = v.as_str().ok_or_else(|| bad("tools.units", "`value` must be auto or a unit: inches, centimeters, millimeters or points"))?;
        s.prefs.units = if v.trim().eq_ignore_ascii_case("auto") {
            None
        } else {
            let unit = Unit::from_id(v)
                .filter(|u| Unit::CHOICES.contains(u))
                .ok_or_else(|| bad("tools.units", format!("no measurement unit “{v}”: auto, inches, centimeters, millimeters or points")))?;
            Some(unit)
        };
    }
    Ok(json!({
        "units": s.prefs.units.map_or("auto", Unit::id),
        "effective": s.units().id(),
        "region": s.region.tag(),
        "paper": s.default_paper(),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use deckcraft_geom::{Region, Unit};
    use serde_json::json;

    use crate::Session;

    /// The unit follows the system region (PowerPoint: centimetres on a Japanese system, an
    /// English Mac set to Japan included), and Preferences can pick another one.
    #[test]
    fn units_follow_the_region_unless_chosen() {
        let mut s = Session::new();
        let r = s.execute("tools.units", &json!({})).unwrap();
        assert_eq!(r, json!({"units": "auto", "effective": "inches", "region": "en-US", "paper": "letter"}), "tests run as en-US");
        for tag in ["ja-JP", "en-JP"] {
            s.set_region(Region::from_tag(tag));
            let r = s.execute("tools.units", &json!({})).unwrap();
            assert_eq!((r["effective"].as_str(), r["paper"].as_str()), (Some("centimeters"), Some("a4")), "{tag}");
            assert_eq!(s.units(), Unit::Centimeters);
        }
        for (value, unit) in [("millimeters", Unit::Millimeters), ("pt", Unit::Points), ("Inches", Unit::Inches), ("cm", Unit::Centimeters)] {
            let r = s.execute("tools.units", &json!({"value": value})).unwrap();
            assert_eq!(r["units"], json!(unit.id()), "{value}");
            assert_eq!(r["effective"], json!(unit.id()));
            assert_eq!(s.units(), unit);
        }
        s.execute("tools.units", &json!({"value": "auto"})).unwrap();
        assert_eq!((s.prefs.units, s.units()), (None, Unit::Centimeters), "auto follows the region again");
    }

    #[test]
    fn bad_units_are_refused_and_keep_the_setting() {
        let mut s = Session::new();
        s.execute("tools.units", &json!({"value": "cm"})).unwrap();
        for bad in [json!("furlongs"), json!("pixels"), json!(""), json!(3), json!(true), json!(["cm"])] {
            assert!(s.execute("tools.units", &json!({"value": bad})).is_err(), "{bad}");
            assert_eq!(s.prefs.units, Some(Unit::Centimeters), "{bad}");
        }
        // No value (or null) only reports.
        assert_eq!(s.execute("tools.units", &json!({"value": null})).unwrap()["units"], "centimeters");
        assert_eq!(s.execute("tools.units", &json!(null)).unwrap()["units"], "centimeters");
    }

    /// PDF notes pages and handouts print on A4 in Japan and on Letter in the United States when
    /// no paper is named; a named paper wins, the Japanese B4 and B5 included.
    #[test]
    fn notes_pages_print_on_the_regions_paper() {
        let mut s = Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        let media = |s: &mut Session, p: serde_json::Value, name: &str| {
            let path = std::env::temp_dir().join(format!("deckcraft-paper-{}-{name}.pdf", std::process::id()));
            let mut p = p;
            p["path"] = json!(path.to_string_lossy());
            s.execute("file.export", &p).unwrap();
            let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).to_string();
            let _ = std::fs::remove_file(&path);
            text
        };
        let notes = json!({"layout": "notes"});
        assert!(media(&mut s, notes.clone(), "us").contains("/MediaBox[0 0 612 792]"), "Letter in the US");
        s.set_region(Region::from_tag("ja-JP"));
        assert!(media(&mut s, notes.clone(), "jp").contains("/MediaBox[0 0 595.28 841.89]"), "A4 in Japan");
        assert!(media(&mut s, json!({"layout": "handouts", "paper": "b5"}), "b5").contains("/MediaBox[0 0 515.91 728.5]"));
        assert!(media(&mut s, json!({"layout": "notes", "paper": "B4", "orientation": "landscape"}), "b4").contains("/MediaBox[0 0 1031.81 728.5]"));
        assert!(media(&mut s, json!({"layout": "notes", "paper": "letter"}), "named").contains("/MediaBox[0 0 612 792]"), "a named paper wins");
        assert_eq!(crate::cmd::file::paper_size("b5", false), Some((515.91, 728.5)));
    }

    /// The setting is saved with the preferences; a damaged value keeps the rest of them.
    #[test]
    fn the_unit_is_saved_with_the_preferences() {
        let prefs = crate::Prefs { units: Some(Unit::Millimeters), author: "Aiko".into(), ..Default::default() };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains("\"units\":\"millimeters\""), "{text}");
        let back: crate::Prefs = serde_json::from_str(&text).unwrap();
        assert_eq!(back.units, Some(Unit::Millimeters));
        assert!(!serde_json::to_string(&crate::Prefs::default()).unwrap().contains("units"), "auto is not written");
        let damaged: crate::Prefs = serde_json::from_str(r#"{"units": "furlongs", "author": "Aiko"}"#).unwrap();
        assert_eq!((damaged.units, damaged.author.as_str()), (None, "Aiko"));
        let damaged: crate::Prefs = serde_json::from_str(r#"{"units": 7, "author": "Aiko"}"#).unwrap();
        assert_eq!((damaged.units, damaged.author.as_str()), (None, "Aiko"));
        let pixels = crate::Prefs { units: Some(Unit::Pixels), ..Default::default() }.sanitized();
        assert_eq!(pixels.units, None, "pixels are not on offer");
    }
}
