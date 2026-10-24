//! The conventions of the user's region that lengths and printing follow: the unit the rulers and
//! the length fields show (centimetres in Japan and other metric regions, inches in the United
//! States) and the paper notes pages and handouts print on (A4, or US Letter in the Americas that
//! use it). PowerPoint takes both from the system region, not from the interface language: an
//! English interface on a Mac set to Japan still measures in centimetres and prints on A4.
//!
//! Ported from WordCraft's `geom/src/region.rs` (a sibling Craft app). WordCraft measures Japan in
//! millimetres because Word does; PowerPoint uses centimetres there. The territory lists come from
//! Unicode CLDR's supplemental `measurementData` (Unicode License): `paperSize` US-Letter and
//! `measurementSystem` US. The front end detects the region (it knows the platform); everything
//! else only sees this plain value.

use serde::{Deserialize, Serialize};

use crate::Unit;

/// Territories whose default paper is US Letter (CLDR `paperSize type="US-Letter"`).
const LETTER: &[&str] = &["BZ", "CA", "CL", "CO", "CR", "GT", "MX", "NI", "PA", "PH", "PR", "SV", "US", "VE"];

/// Territories that measure in inches (CLDR `measurementSystem type="US"`).
const INCHES: &[&str] = &["LR", "MM", "US"];

/// The environment variable that names the region outright (`ja-JP`, `en_US.UTF-8`), for tests,
/// scripted runs and people who want another region than the system's.
pub const REGION_ENV: &str = "DECKCRAFT_REGION";

/// A region: a language and a territory, read from a locale tag like `ja-JP`, `de_AT.UTF-8` or
/// `en-US`. The default is `en-US` (Letter, inches).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Lower-case language subtag (`ja`); empty when the tag has none.
    pub language: String,
    /// Upper-case territory (`JP`, or a UN M.49 number like `419`); empty when the tag has none.
    pub territory: String,
    /// US Letter or not, when a separate paper locale says so (Unix `LC_PAPER`); otherwise the
    /// territory decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter: Option<bool>,
    /// Inches or not, when a separate measurement locale says so (Unix `LC_MEASUREMENT`);
    /// otherwise the territory decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inches: Option<bool>,
}

impl Default for Region {
    fn default() -> Self {
        Region { language: "en".into(), territory: "US".into(), letter: None, inches: None }
    }
}

impl Region {
    /// The region a BCP 47 or POSIX locale tag names (`ja-JP`, `ja_JP.UTF-8`, `zh-Hans-CN`,
    /// `en-US-u-ms-metric`). An empty or unreadable tag, `C` and `POSIX` are `en-US`; a bare `en`
    /// is the United States, any other bare language (`ja`, as a browser may say) has no territory.
    pub fn from_tag(tag: &str) -> Region {
        // Locale tags are short; anything long is junk and only its start is read.
        let tag: String = tag.chars().take(64).collect();
        let tag = tag.split(['.', '@']).next().unwrap_or_default().trim();
        if tag.is_empty() || tag.eq_ignore_ascii_case("c") || tag.eq_ignore_ascii_case("posix") {
            return Region::default();
        }
        let mut parts = tag.split(['-', '_']);
        let language = parts.next().unwrap_or_default();
        if !(2..=3).contains(&language.len()) || !language.chars().all(|c| c.is_ascii_alphabetic()) {
            return Region::default();
        }
        let language = language.to_ascii_lowercase();
        let mut territory = String::new();
        for p in parts {
            // A singleton starts the extensions (`-u-…`, `-x-…`): no territory after it.
            if p.len() == 1 {
                break;
            }
            let alpha = p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic());
            let digits = p.len() == 3 && p.chars().all(|c| c.is_ascii_digit());
            if alpha || digits {
                territory = p.to_ascii_uppercase();
                break;
            }
        }
        if territory.is_empty() && language == "en" {
            territory = "US".into();
        }
        Region { language, territory, letter: None, inches: None }
    }

    /// This region with another territory (from a platform setting that names only the
    /// territory, like Windows' home location). A value that isn't a territory code is ignored.
    pub fn with_territory(mut self, territory: &str) -> Region {
        let t = territory.trim();
        let ok = (t.len() == 2 && t.chars().all(|c| c.is_ascii_alphabetic())) || (t.len() == 3 && t.chars().all(|c| c.is_ascii_digit()));
        if ok {
            self.territory = t.to_ascii_uppercase();
        }
        self
    }

    /// This region with the paper and the measurement system taken from their own locale tags,
    /// when the platform sets them apart (Unix `LC_PAPER`, `LC_MEASUREMENT`).
    pub fn with_paper_and_measurement(mut self, paper: Option<&str>, measurement: Option<&str>) -> Region {
        if let Some(t) = paper {
            self.letter = Some(Region::from_tag(t).uses_letter());
        }
        if let Some(t) = measurement {
            self.inches = Some(Region::from_tag(t).units() == Unit::Inches);
        }
        self
    }

    /// The region from the locale variables `var` reads (the process environment) and, when none
    /// is set, the platform's first preferred locale `platform` gives. One tag for everything first
    /// ([`REGION_ENV`], then `LC_ALL`); otherwise Unix sets each convention on its own: the base
    /// (`LC_NUMERIC`, else `LANG`), paper (`LC_PAPER`) and measurement (`LC_MEASUREMENT`). Empty,
    /// `C` and `POSIX` values count as unset.
    pub fn from_locale_vars(var: impl Fn(&str) -> Option<String>, platform: impl FnOnce() -> Option<String>) -> Region {
        let var = |name: &str| var(name).filter(|v| usable_locale(v));
        if let Some(tag) = var(REGION_ENV).or_else(|| var("LC_ALL")) {
            return Region::from_tag(&tag);
        }
        let base = var("LC_NUMERIC").or_else(|| var("LANG")).unwrap_or_else(|| platform().unwrap_or_default());
        Region::from_tag(&base).with_paper_and_measurement(var("LC_PAPER").as_deref(), var("LC_MEASUREMENT").as_deref())
    }

    /// Do notes pages and handouts print on US Letter here (otherwise A4)?
    pub fn uses_letter(&self) -> bool {
        self.letter.unwrap_or_else(|| LETTER.contains(&self.territory.as_str()))
    }

    /// The default paper's id in the Print page's list: `letter` or `a4`.
    pub fn paper_id(&self) -> &'static str {
        if self.uses_letter() { "letter" } else { "a4" }
    }

    /// The default paper's portrait size in points: Letter (612 × 792) or A4 (595.28 × 841.89).
    pub fn paper_size(&self) -> (f64, f64) {
        if self.uses_letter() { (612.0, 792.0) } else { (595.28, 841.89) }
    }

    /// The unit lengths are shown in by default: inches where CLDR says so (the United States,
    /// Liberia, Myanmar), centimetres everywhere else, Japan included.
    pub fn units(&self) -> Unit {
        if self.inches.unwrap_or_else(|| INCHES.contains(&self.territory.as_str())) { Unit::Inches } else { Unit::Centimeters }
    }

    /// The region as a BCP 47 tag (`ja-JP`, or `ja` without a territory).
    pub fn tag(&self) -> String {
        if self.territory.is_empty() { self.language.clone() } else { format!("{}-{}", self.language, self.territory) }
    }
}

/// A locale variable that names a locale (not empty, `C` or `POSIX`).
fn usable_locale(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty() && !v.eq_ignore_ascii_case("c") && !v.eq_ignore_ascii_case("posix") && !v.starts_with("C.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_locale_tags() {
        assert_eq!(Region::from_tag("ja-JP"), Region { language: "ja".into(), territory: "JP".into(), ..Region::default() });
        assert_eq!(Region::from_tag("ja_JP.UTF-8").territory, "JP");
        assert_eq!(Region::from_tag("de_AT.UTF-8@euro").territory, "AT");
        assert_eq!(Region::from_tag("zh-Hans-CN").territory, "CN");
        assert_eq!(Region::from_tag("es-419").territory, "419");
        assert_eq!(Region::from_tag("en-u-ms-metric").territory, "US", "bare English is the US");
        assert_eq!(Region::from_tag("ja").territory, "");
        for junk in ["", "C", "POSIX", "1234", "x", "--", "ünïcode-ÄÄ", &"a".repeat(10_000)] {
            assert_eq!(Region::from_tag(junk), Region::default(), "{junk:?}");
        }
        assert_eq!(Region::from_tag("en-US").with_territory("jp").territory, "JP");
        assert_eq!(Region::from_tag("en-US").with_territory("Japan").territory, "US");
        assert_eq!(Region::from_tag("ja_JP.UTF-8").tag(), "ja-JP");
        assert_eq!(Region::from_tag("ja").tag(), "ja");
    }

    /// Units and paper follow the territory, not the language: an English Mac set to Japan
    /// (`en-JP`, as macOS lists it) measures in centimetres and prints on A4.
    #[test]
    fn units_and_paper_follow_the_territory() {
        for tag in ["ja-JP", "en-JP", "ja", "en-GB", "de-DE", "fr-FR", "zh-CN", "ko-KR"] {
            assert_eq!(Region::from_tag(tag).units(), Unit::Centimeters, "{tag}");
            assert_eq!((Region::from_tag(tag).paper_id(), Region::from_tag(tag).paper_size()), ("a4", (595.28, 841.89)), "{tag}");
        }
        let us = Region::from_tag("en-US");
        assert_eq!((us.units(), us.paper_id(), us.paper_size()), (Unit::Inches, "letter", (612.0, 792.0)));
        assert_eq!(Region::default(), us);
        // Canada prints on Letter but measures in centimetres.
        let ca = Region::from_tag("fr-CA");
        assert_eq!((ca.units(), ca.paper_id()), (Unit::Centimeters, "letter"));
        assert_eq!(Region::from_tag("es-MX").paper_id(), "letter");
        // Unix sets paper and measurement apart from the base locale.
        let mixed = Region::from_tag("ja_JP.UTF-8").with_paper_and_measurement(Some("en_US.UTF-8"), None);
        assert_eq!((mixed.paper_id(), mixed.units()), ("letter", Unit::Centimeters));
        let mixed = Region::from_tag("en_US.UTF-8").with_paper_and_measurement(Some("ja_JP.UTF-8"), Some("ja_JP.UTF-8"));
        assert_eq!((mixed.paper_id(), mixed.units()), ("a4", Unit::Centimeters));
        assert_eq!(Region::from_tag("ja-JP").with_paper_and_measurement(Some(""), Some("junk")).paper_id(), "letter", "junk is en-US");
    }

    #[test]
    fn region_from_locale_variables() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |n: &str| pairs.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string());
        let none = || None;
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "ja_JP.UTF-8")]), none).units(), Unit::Centimeters);
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "en_US.UTF-8")]), none).units(), Unit::Inches);
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "ja_JP.UTF-8"), ("LC_ALL", "en_US")]), none).units(), Unit::Inches);
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "en_US"), (REGION_ENV, "ja-JP")]), none).territory, "JP");
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "en_US"), ("LC_PAPER", "ja_JP")]), none).paper_id(), "a4");
        assert_eq!(Region::from_locale_vars(env(&[("LANG", "C.UTF-8")]), || Some("en-JP".into())).territory, "JP");
        assert_eq!(Region::from_locale_vars(env(&[]), || Some("en-JP".into())).units(), Unit::Centimeters);
        assert_eq!(Region::from_locale_vars(env(&[]), none), Region::default());
    }
}
