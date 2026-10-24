//! Chinese, Japanese and Korean text: which of the four East Asian writing traditions a run is
//! in. Han characters are shared (Unicode's Han unification), so the same kanji is drawn with a
//! Japanese, a Simplified Chinese, a Traditional Chinese or a Korean face depending on the text's
//! language, as PowerPoint picks the theme's `Jpan`/`Hans`/`Hant`/`Hang` font and its fallback
//! faces by the run's language.

use std::sync::RwLock;

/// An East Asian writing tradition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CjkLocale {
    Japanese,
    SimplifiedChinese,
    TraditionalChinese,
    Korean,
}

impl CjkLocale {
    /// The tradition a BCP 47 language tag is written in: `ja`/`ja-JP` → Japanese, `zh-CN`,
    /// `zh-SG`, `zh-Hans`(-…) and plain `zh` → Simplified, `zh-TW`, `zh-HK`, `zh-MO` and
    /// `zh-Hant`(-…) → Traditional, `ko`/`ko-KR` → Korean; `None` for other languages.
    pub fn from_lang(tag: &str) -> Option<CjkLocale> {
        let t = tag.trim().replace('_', "-").to_ascii_lowercase();
        let mut parts = t.split('-');
        match parts.next()? {
            "ja" => Some(CjkLocale::Japanese),
            "ko" => Some(CjkLocale::Korean),
            "zh" | "cmn" | "yue" => {
                // A script subtag decides; without one, the region (Taiwan, Hong Kong, Macau).
                let rest: Vec<&str> = parts.collect();
                if rest.contains(&"hans") {
                    Some(CjkLocale::SimplifiedChinese)
                } else if rest.contains(&"hant") || rest.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")) {
                    Some(CjkLocale::TraditionalChinese)
                } else {
                    Some(CjkLocale::SimplifiedChinese)
                }
            }
            _ => None,
        }
    }

    /// The ISO 15924 tag a theme names this tradition's font by (`a:font script="Jpan"`).
    pub fn theme_script(self) -> &'static str {
        match self {
            CjkLocale::Japanese => "Jpan",
            CjkLocale::SimplifiedChinese => "Hans",
            CjkLocale::TraditionalChinese => "Hant",
            CjkLocale::Korean => "Hang",
        }
    }

    /// The BCP 47 language PowerPoint writes for text in this tradition (`lang="ja-JP"`).
    pub fn lang(self) -> &'static str {
        match self {
            CjkLocale::Japanese => "ja-JP",
            CjkLocale::SimplifiedChinese => "zh-CN",
            CjkLocale::TraditionalChinese => "zh-TW",
            CjkLocale::Korean => "ko-KR",
        }
    }
}

/// The East Asian editing language of this machine (the first Chinese, Japanese or Korean
/// language among the system's preferred languages); `None` until the app sets it, and in tests.
static DEFAULT: RwLock<Option<CjkLocale>> = RwLock::new(None);

/// Sets the tradition Han text without a language of its own is drawn in (the app does this at
/// start from the system's preferred languages).
pub fn set_default_cjk_locale(l: Option<CjkLocale>) {
    if let Ok(mut d) = DEFAULT.write() {
        *d = l;
    }
}

/// The tradition Han text without a language of its own is drawn in; `None` when the system
/// takes no East Asian language.
pub fn default_cjk_locale() -> Option<CjkLocale> {
    DEFAULT.read().ok().and_then(|d| *d)
}

/// The first Chinese, Japanese or Korean language among `tags` (the system's preferred
/// languages in order: an English Mac in Japan lists `en-JP`, `ja-JP`), as its tradition.
pub fn east_asian_of<'a>(tags: impl IntoIterator<Item = &'a str>) -> Option<CjkLocale> {
    tags.into_iter().find_map(CjkLocale::from_lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_map_to_their_tradition() {
        assert_eq!(CjkLocale::from_lang("ja-JP"), Some(CjkLocale::Japanese));
        assert_eq!(CjkLocale::from_lang("JA"), Some(CjkLocale::Japanese));
        assert_eq!(CjkLocale::from_lang("zh-CN"), Some(CjkLocale::SimplifiedChinese));
        assert_eq!(CjkLocale::from_lang("zh"), Some(CjkLocale::SimplifiedChinese));
        assert_eq!(CjkLocale::from_lang("zh-Hans-HK"), Some(CjkLocale::SimplifiedChinese));
        assert_eq!(CjkLocale::from_lang("zh-TW"), Some(CjkLocale::TraditionalChinese));
        assert_eq!(CjkLocale::from_lang("zh_HK"), Some(CjkLocale::TraditionalChinese));
        assert_eq!(CjkLocale::from_lang("zh-Hant"), Some(CjkLocale::TraditionalChinese));
        assert_eq!(CjkLocale::from_lang("ko-KR"), Some(CjkLocale::Korean));
        assert_eq!(CjkLocale::from_lang("en-US"), None);
        assert_eq!(CjkLocale::from_lang(""), None);
        assert_eq!(CjkLocale::Japanese.theme_script(), "Jpan");
        assert_eq!(CjkLocale::TraditionalChinese.lang(), "zh-TW");
    }

    #[test]
    fn the_first_east_asian_preferred_language_wins() {
        assert_eq!(east_asian_of(["en-JP", "ja-JP", "zh-Hans-JP"]), Some(CjkLocale::Japanese));
        assert_eq!(east_asian_of(["en-US", "fr-FR"]), None);
    }
}
