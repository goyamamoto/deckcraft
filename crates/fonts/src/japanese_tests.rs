//! Japanese font names and East Asian fallback: localized, full-width, full and PostScript names,
//! Mac Japanese records, weight-named families, stand-ins for missing Japanese fonts and the
//! language-aware cascade. The fonts are built in memory (`test_fonts`), so nothing depends on
//! the machine's fonts except the tests that say they skip.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use crate::test_fonts::{Name, Spec, font, sans, windows_names};
use crate::*;

/// "DeckCraft Kaku W3": typographic family DeckCraft Kaku (デッキ角ゴ), Windows family DeckCraft
/// Kaku W3 (デッキ角ゴ W3), PostScript DeckCraftKaku-W3, as Hiragino's fonts are named.
fn kaku(weight: &str, w: u16, bold: bool) -> Vec<u8> {
    let win = format!("DeckCraft Kaku {weight}");
    let ja_win = format!("デッキ角ゴ {weight}");
    let ps = format!("DeckCraftKaku-{weight}");
    let mut names = windows_names("DeckCraft Kaku", weight, &win, Some("デッキ角ゴ"), Some(&ja_win));
    names.push((3, 1, 0x409, 6, &ps));
    font(&Spec { names: &names, chars: &['漢', 'あ'], weight: w, fs_selection: if bold { 0x20 } else { 0x40 }, family_class: 8 << 8 })
}

fn bare_db() -> FontDb {
    let db = FontDb::without_bundled(Vec::new());
    db.set_system_fallback(false);
    db
}

fn check_localized(db: &FontDb) {
    for name in ["デッキ角ゴ W3", "デッキ角ゴ", "ＤｅｃｋＣｒａｆｔ Ｋａｋｕ Ｗ３", "deckcraft kaku w3", "DeckCraftKaku W3", "DeckCraftKaku-W3"]
    {
        assert!(db.has_family(name), "{name}");
        let f = db.face(name, "Regular");
        assert_eq!((f.family.as_str(), f.style.as_str()), ("DeckCraft Kaku", "W3"), "{name}");
        assert_eq!(db.document_face(name, "Regular").family, "DeckCraft Kaku", "{name}: installed, so not Calibri");
        assert_eq!(db.available_face(name, "Regular").map(|f| f.family.clone()).as_deref(), Some("DeckCraft Kaku"), "{name}");
        assert!(f.answers_to(name));
    }
    assert!(db.is_loaded("デッキ角ゴ W3"));
    assert_eq!(db.styles("デッキ角ゴ"), ["W3"]);
    let (english, names) = db.family_names("デッキ角ゴ").unwrap();
    assert_eq!(english, "DeckCraft Kaku");
    assert_eq!(names, [("ja-JP".to_string(), "デッキ角ゴ".to_string())]);
    let (english, names) = db.family_names("DeckCraft Kaku W3").unwrap();
    assert_eq!((english.as_str(), names), ("DeckCraft Kaku W3", vec![("ja-JP".to_string(), "デッキ角ゴ W3".to_string())]));
    assert!(db.localized_names("ja").contains(&("DeckCraft Kaku".to_string(), "デッキ角ゴ".to_string())));
    assert!(db.localized_names("zh-Hans").iter().all(|(e, _)| !e.starts_with("DeckCraft Kaku")));
    assert!(!db.has_family("デッキ丸ゴ") && db.family_names("デッキ丸ゴ").is_none());
}

#[test]
fn japanese_names_find_the_font_loaded_or_installed() {
    // Loaded directly…
    let db = bare_db();
    assert_eq!(db.add_font(kaku("W3", 300, false)), 1);
    check_localized(&db);
    // …and found by the system font scan under its Japanese names, then loaded.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = crate::test_fonts::temp_dir("localized");
        std::fs::write(dir.join("kaku-w3.ttf"), kaku("W3", 300, false)).unwrap();
        let db = FontDb::without_bundled(vec![dir.clone()]);
        db.set_system_fallback(false);
        assert!(db.has_family("デッキ角ゴ W3") && !db.is_loaded("デッキ角ゴ W3"), "cataloged, not loaded yet");
        check_localized(&db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn names_fold_full_width_letters_case_and_spaces() {
    assert_eq!(fold_font_name("ＭＳ Ｐゴシック"), fold_font_name("MS Pゴシック"));
    assert_eq!(fold_font_name(" ＭＳ\u{3000} Ｐゴシック "), "ms pゴシック");
    assert_eq!(fold_compact("Hiragino Kaku Gothic ProN W3"), fold_compact("HiraginoKakuGothicProNW3"));
    assert_ne!(fold_font_name("Yu Gothic"), fold_font_name("YuGothic"), "different fonts");
    assert_eq!(fold_font_name("Osaka−等幅"), "osaka-等幅");
    assert_ne!(fold_font_name("ＭＳ ゴシック"), fold_font_name("ＭＳ Ｐゴシック"));
    assert_eq!(fold_font_name(&"x".repeat(10_000)).len(), 256);
    assert_eq!(fold_font_name(""), "");
    assert_eq!(fold_font_name("\u{3000} "), "");
    assert!(name_language_matches("ja", "ja-JP") && name_language_matches("zh-Hans", "zh-Hans") && name_language_matches("zh", "zh-CN"));
    assert!(!name_language_matches("zh-Hant", "zh-Hans") && name_language_matches("zh-Hant", "zh-TW") && !name_language_matches("ko", "ja"));
}

/// Full-width and half-width forms of an installed font's own Japanese name both find it:
/// ＭＳ Ｐゴシック, as Japanese PowerPoint writes it, and MS Pゴシック.
#[test]
fn full_width_names_match_the_fonts_own_name() {
    let names = windows_names("DeckCraft PGothic", "Regular", "DeckCraft PGothic", None, Some("ＤＣ Ｐゴシック"));
    let db = bare_db();
    assert_eq!(db.add_font(sans(&names, &['あ'])), 1);
    for name in ["ＤＣ Ｐゴシック", "DC Pゴシック", "dc pゴシック", "DeckCraft PGothic"] {
        assert_eq!(db.face(name, "Bold").family, "DeckCraft PGothic", "{name}");
    }
    assert!(!db.has_family("ＤＣ ゴシック"), "the fixed-pitch sibling is another font");
}

/// Office's Yu Gothic and macOS's YuGothic are different fonts with names that differ only by a
/// space: each finds itself; a name spaced like neither (Yu  Gothic, YUGOTHIC) finds one of them
/// rather than nothing.
#[test]
fn names_spaced_differently_stay_apart_when_both_exist() {
    let db = bare_db();
    let office: [Name<'_>; 2] = [(3, 1, 0x409, 1, "DeckCraft Yu"), (3, 1, 0x409, 2, "Bold")];
    let mac: [Name<'_>; 2] = [(3, 1, 0x409, 1, "DeckCraftYu"), (3, 1, 0x409, 2, "Bold")];
    assert_eq!(db.add_font(sans(&office, &['あ'])), 1);
    assert_eq!(db.add_font(sans(&mac, &['あ'])), 1);
    assert_eq!(db.face("DeckCraft Yu", "Bold").family, "DeckCraft Yu");
    assert_eq!(db.face("DeckCraftYu", "Bold").family, "DeckCraftYu");
    assert_eq!(db.face("deckcraft  yu", "Bold").family, "DeckCraft Yu");
    assert!(db.has_family("Deck Craft Yu"), "spaced like neither: found without spaces");
    assert!(db.face("Deck Craft Yu", "Bold").family.starts_with("DeckCraft"));
    let one = bare_db();
    assert_eq!(one.add_font(sans(&office, &['あ'])), 1);
    assert_eq!(one.face("DeckCraftYu", "Bold").family, "DeckCraft Yu", "only one of them installed");
}

/// Fonts made for the Mac carry their Japanese names in Shift JIS Mac records only (Osaka's
/// Osaka−等幅 is its Regular-Mono face's full name). They are decoded and matched.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn mac_japanese_records_are_read() {
    let mono: [Name<'_>; 6] = [
        (1, 0, 0, 1, "DeckCraft Osaka"),
        (1, 0, 0, 2, "Regular-Mono"),
        (1, 0, 0, 4, "DeckCraft Osaka-Mono"),
        (1, 0, 0, 6, "DeckCraftOsaka-Mono"),
        (1, 1, 11, 1, "デッキ大阪"),
        (1, 1, 11, 4, "デッキ大阪−等幅"),
    ];
    let regular: [Name<'_>; 3] = [(1, 0, 0, 1, "DeckCraft Osaka"), (1, 0, 0, 2, "Regular"), (1, 1, 11, 1, "デッキ大阪")];
    let dir = crate::test_fonts::temp_dir("mac-names");
    std::fs::write(dir.join("a-regular.ttf"), sans(&regular, &['あ'])).unwrap();
    std::fs::write(dir.join("b-mono.ttf"), sans(&mono, &['あ'])).unwrap();
    let db = FontDb::without_bundled(vec![dir.clone()]);
    db.set_system_fallback(false);
    assert!(db.has_family("デッキ大阪") && db.has_family("デッキ大阪−等幅") && db.has_family("デッキ大阪-等幅"));
    assert_eq!(db.face("デッキ大阪", "Regular").style, "Regular");
    // The full name selects its face whatever style is asked (bold is the caller's).
    for style in ["Regular", "Bold"] {
        assert_eq!(db.face("デッキ大阪−等幅", style).style, "Regular-Mono", "{style}");
        assert_eq!(db.face("DeckCraftOsaka-Mono", style).style, "Regular-Mono", "{style}");
    }
    let (english, names) = db.family_names("デッキ大阪").unwrap();
    assert_eq!((english.as_str(), names), ("DeckCraft Osaka", vec![("ja".to_string(), "デッキ大阪".to_string())]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Bold of a weight-named family is its bold partner (Hiragino's W3 → W6, as macOS pairs them),
/// never a bold face that belongs to another Windows family (Yu Gothic Bold for Yu Gothic
/// Light: the Light is emboldened).
#[test]
fn bold_of_a_weight_named_family_is_its_partner() {
    let light_names = windows_names("DeckCraft Yu", "Light", "DeckCraft Yu Light", None, Some("デッキ游 Light"));
    let bold_names: [Name<'_>; 3] = [(3, 1, 0x409, 1, "DeckCraft Yu"), (3, 1, 0x409, 2, "Bold"), (3, 1, 0x411, 1, "デッキ游")];
    let light = font(&Spec { names: &light_names, chars: &['あ'], weight: 300, fs_selection: 0, family_class: 0 });
    let yu_bold = font(&Spec { names: &bold_names, chars: &['あ'], weight: 700, fs_selection: 0x20, family_class: 0 });
    let check = |db: &FontDb| {
        for name in ["DeckCraft Kaku W3", "デッキ角ゴ W3"] {
            assert_eq!(db.face(name, "Bold").style, "W6", "{name}");
            assert_eq!(db.face(name, "Bold Italic").style, "W6", "{name}: no italic, so the bold partner");
            assert_eq!(db.face(name, "Regular").style, "W3", "{name}");
        }
        assert_eq!(db.face("DeckCraft Kaku W6", "Bold").style, "W6", "already bold");
        assert_eq!(db.face("DeckCraft Kaku", "Bold").style, "W6");
        assert_eq!(db.styles("DeckCraft Kaku"), ["W3", "W6"], "W3 and W6 sort by weight");
        assert_eq!(db.face("DeckCraft Yu Light", "Bold").style, "Light", "Yu Gothic Bold is not Light's partner");
        assert_eq!(db.face("デッキ游 Light", "Bold").style, "Light");
        assert_eq!(db.face("DeckCraft Yu", "Bold").style, "Bold");
        assert_eq!(db.face("デッキ游", "Bold").style, "Bold");
    };
    let db = bare_db();
    for f in [kaku("W3", 300, false), kaku("W6", 600, true), light.clone(), yu_bold.clone()] {
        assert_eq!(db.add_font(f), 1);
    }
    check(&db);
    // Installed: looking up the W3 by its Windows name loads its file only; Bold then loads the
    // rest of the family to find the W6.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = crate::test_fonts::temp_dir("weights");
        for (file, data) in
            [("kaku-w3.ttf", kaku("W3", 300, false)), ("kaku-w6.ttf", kaku("W6", 600, true)), ("yu-l.ttf", light), ("yu-b.ttf", yu_bold)]
        {
            std::fs::write(dir.join(file), data).unwrap();
        }
        let db = FontDb::without_bundled(vec![dir.clone()]);
        db.set_system_fallback(false);
        assert_eq!(db.face("DeckCraft Kaku W3", "Regular").style, "W3");
        assert_eq!(db.face("DeckCraft Kaku W3", "Bold").style, "W6");
        check(&db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A weight-named face that is already bold (W6 and heavier, as Windows draws them: the caller
/// makes only lighter faces bolder) is its own bold, and a light one's bold is its partner,
/// whichever of the family's other weights earlier lookups happened to load: a title's weight
/// never changes with what the other slides contain.
#[test]
fn bold_of_a_weight_named_family_does_not_depend_on_earlier_lookups() {
    let weights = || [("W3", 300, false), ("W6", 600, true), ("W7", 700, true), ("W9", 900, true)].map(|(w, n, b)| (w, kaku(w, n, b)));
    let db = bare_db();
    for (_, f) in weights() {
        assert_eq!(db.add_font(f), 1);
    }
    for (name, bold) in [("DeckCraft Kaku W3", "W6"), ("DeckCraft Kaku W6", "W6"), ("DeckCraft Kaku W7", "W7"), ("デッキ角ゴ W9", "W9")] {
        assert_eq!(db.face(name, "Bold").style, bold, "{name}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = crate::test_fonts::temp_dir("bold-order");
        for (w, data) in weights() {
            std::fs::write(dir.join(format!("kaku-{w}.ttf")), data).unwrap();
        }
        let fresh = || FontDb::without_bundled(vec![dir.clone()]);
        // The W6's bold before and after something loads the whole family.
        let db = fresh();
        assert_eq!(db.face("DeckCraft Kaku W6", "Bold").style, "W6");
        assert_eq!(db.face("DeckCraft Kaku", "Regular").style, "W3");
        assert_eq!(db.face("DeckCraft Kaku W6", "Bold").style, "W6");
        // The W3's bold when only a heavier weight than its partner was loaded before.
        let db = fresh();
        assert_eq!(db.face("DeckCraft Kaku W7", "Regular").style, "W7");
        assert_eq!(db.face("DeckCraft Kaku W3", "Bold").style, "W6");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The styles of `family` loaded so far.
#[cfg(not(target_arch = "wasm32"))]
fn loaded_styles(db: &FontDb, family: &str) -> Vec<String> {
    let mut v: Vec<String> = db.read_faces().iter().filter(|f| f.family == family).map(|f| f.style.clone()).collect();
    v.sort();
    v
}

/// Bold text in a weight-named family, and a bold fallback drawn by one, load the file holding
/// the bold face and no other of the family's (Hiragino Sans is ten files, 59 MB); the fallback
/// takes the bold partner, as the run's own face does (W3 → W6, not the weight nearest Bold). A
/// lookup of the family by its own name still loads all of it.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bold_faces_load_only_the_file_they_need() {
    let dir = crate::test_fonts::temp_dir("bold-file");
    for (w, n, b) in [("W3", 300, false), ("W4", 400, false), ("W5", 500, true), ("W6", 600, true), ("W7", 700, true), ("W8", 800, true)] {
        std::fs::write(dir.join(format!("kaku-{w}.ttf")), kaku(w, n, b)).unwrap();
    }
    let sans = |style: &'static str, weight: u16| {
        let names: [Name<'_>; 2] = [(3, 1, 0x409, 1, "DeckCraft Sans"), (3, 1, 0x409, 2, style)];
        let fs_selection = if weight >= 700 { 0x20 } else { 0x40 };
        font(&Spec { names: &names, chars: &['漢'], weight, fs_selection, family_class: 8 << 8 })
    };
    std::fs::write(dir.join("sans-bold.ttf"), sans("Bold", 700)).unwrap();
    std::fs::write(dir.join("sans-black.ttf"), sans("Black", 900)).unwrap();
    let fresh = || FontDb::without_bundled(vec![dir.clone()]);
    // A run in the W3 and the W4, bold.
    let db = fresh();
    assert_eq!(db.face("DeckCraft Kaku W3", "Bold").style, "W6");
    assert_eq!(loaded_styles(&db, "DeckCraft Kaku"), ["W3", "W6"]);
    assert_eq!(db.face("デッキ角ゴ W4", "Bold").style, "W7");
    assert_eq!(loaded_styles(&db, "DeckCraft Kaku"), ["W3", "W4", "W6", "W7"]);
    // A bold fallback drawn by the W3: its partner.
    let db = fresh();
    assert_eq!(db.face("DeckCraft Kaku W3", "Regular").style, "W3");
    let f = db.fallback_styled('漢', u32::MAX, "Bold", Some(CjkLocale::Japanese)).unwrap();
    assert_eq!((f.family.as_str(), f.style.as_str()), ("DeckCraft Kaku", "W6"));
    assert_eq!(loaded_styles(&db, "DeckCraft Kaku"), ["W3", "W6"]);
    assert_eq!(db.face("DeckCraft Kaku", "W8").style, "W8", "the whole family by its own name");
    // A bold fallback drawn by a family with a Bold face: the Bold's file only.
    let db = fresh();
    assert_eq!(db.add_font(sans("Regular", 400)), 1);
    let f = db.fallback_styled('漢', u32::MAX, "Bold", None).unwrap();
    assert_eq!((f.family.as_str(), f.style.as_str()), ("DeckCraft Sans", "Bold"));
    assert_eq!(loaded_styles(&db, "DeckCraft Sans"), ["Bold", "Regular"]);
    assert_eq!(db.face("DeckCraft Sans", "Black").style, "Black", "the whole family by its own name");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The families the stand-in and fallback chains name, built in memory: a gothic, a Mincho and
/// a fixed-pitch gothic every platform's chain lists, and a Simplified Chinese and a Korean
/// face. Each draws kana, kanji, hanzi and Hangul alike, so only the choice tells them apart.
fn stand_ins() -> FontDb {
    let db = bare_db();
    let chars = ['あ', '漢', '汉', '한', 'Ａ', '　', '、', 'B'];
    // The Chinese and Korean faces first: a search by load order alone would pick them.
    for (family, class, weight, fs) in [
        ("WenQuanYi Micro Hei", 8 << 8, 400, 0x40),
        ("NanumGothic", 8 << 8, 400, 0x40),
        ("IPAexGothic", 8 << 8, 400, 0x40),
        ("IPAexMincho", 1 << 8, 400, 0x40),
        ("IPAGothic", 8 << 8, 400, 0x40),
    ] {
        let names = windows_names(family, "Regular", family, None, None);
        assert_eq!(db.add_font(font(&Spec { names: &names, chars: &chars, weight, fs_selection: fs, family_class: class })), 1);
    }
    let bold: [Name<'_>; 2] = [(3, 1, 0x409, 1, "IPAexGothic"), (3, 1, 0x409, 2, "Bold")];
    assert_eq!(db.add_font(font(&Spec { names: &bold, chars: &chars, weight: 700, fs_selection: 0x20, family_class: 8 << 8 })), 1);
    db
}

#[test]
fn missing_japanese_fonts_take_a_stand_in_of_their_kind() {
    let db = stand_ins();
    for (missing, stand_in) in [
        ("游ゴシック", "IPAexGothic"),
        ("Yu Gothic", "IPAexGothic"),
        ("ＭＳ Ｐゴシック", "IPAexGothic"),
        ("メイリオ", "IPAexGothic"),
        ("HGPｺﾞｼｯｸE", "IPAexGothic"),
        ("ヒラギノ角ゴ ProN W3", "IPAexGothic"),
        ("HG創英角ﾎﾟｯﾌﾟ体", "IPAexGothic"),
        ("ヒラギノ丸ゴ ProN W4", "IPAexGothic"),
        ("游明朝", "IPAexMincho"),
        ("ＭＳ Ｐ明朝", "IPAexMincho"),
        ("ＭＳ 明朝", "IPAexMincho"),
        ("ＭＳ ゴシック", "IPAGothic"),
        ("Osaka−等幅", "IPAGothic"),
        ("宋体", "WenQuanYi Micro Hei"),
        ("맑은 고딕", "NanumGothic"),
    ] {
        assert!(!db.has_family(missing), "{missing} is missing");
        // Latin letters too: a Japanese-named font is never drawn in Calibri/Carlito.
        assert_eq!(db.document_face(missing, "Regular").family, stand_in, "{missing}");
        assert_eq!(db.face(missing, "Regular").family, stand_in, "{missing}");
        assert_eq!(db.available_face(missing, "Regular").map(|f| f.family.clone()).as_deref(), Some(stand_in), "{missing}");
    }
    // Bold text in a missing gothic takes the stand-in's bold.
    assert_eq!(db.document_face("游ゴシック", "Bold").style, "Bold");
    // Latin fonts keep their rules; an unknown Latin name has no East Asian stand-in.
    assert!(db.available_face("Qqq Nonexistent", "Regular").is_none());
    assert_eq!(db.document_face("Century Gothic", "Regular").family, db.face(MISSING_FONT_FAMILY, "Regular").family);
    // No East Asian face installed at all: a Japanese-named font is drawn as any missing font
    // is (Calibri).
    let bare = bare_db();
    assert_eq!(bare.document_face("游ゴシック", "Regular").family, bare.face(MISSING_FONT_FAMILY, "Regular").family);
    assert!(bare.available_face("游ゴシック", "Regular").is_none());
}

/// Held by the tests that set the system's East Asian language or whose results depend on it
/// being unset, so they never see each other's setting.
static SYSTEM_LOCALE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The system's East Asian language for one test: set while it runs, put back to the default
/// (none) when it ends, failing or not.
struct SystemLocale {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl SystemLocale {
    fn set(l: Option<CjkLocale>) -> SystemLocale {
        let lock = SYSTEM_LOCALE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_default_cjk_locale(l);
        SystemLocale { _lock: lock }
    }
}

impl Drop for SystemLocale {
    fn drop(&mut self) {
        set_default_cjk_locale(None);
    }
}

/// A missing font named in Chinese characters no list knows (幼圆) stands in with a Japanese
/// gothic on a Japanese system, as Word for Mac draws it, and with a Chinese face on a Chinese
/// one; a name with kana keeps the Japanese gothic everywhere.
#[test]
fn missing_cjk_named_fonts_stand_in_by_the_systems_east_asian_language() {
    let db = stand_ins();
    let face = |name: &str| db.available_face(name, "Regular").map(|f| f.family.clone());
    let _system = SystemLocale::set(Some(CjkLocale::SimplifiedChinese));
    assert_eq!(face("幼圆").as_deref(), Some("WenQuanYi Micro Hei"));
    assert_eq!(db.document_face("方正兰亭黑简体", "Regular").family, "WenQuanYi Micro Hei");
    assert_eq!(face("デッキ創英ﾎﾟｯﾌﾟ体").as_deref(), Some("IPAexGothic"), "kana: Japanese");
    set_default_cjk_locale(Some(CjkLocale::Korean));
    assert_eq!(face("幼圆").as_deref(), Some("NanumGothic"));
    for japanese in [Some(CjkLocale::Japanese), None] {
        set_default_cjk_locale(japanese);
        assert_eq!(face("幼圆").as_deref(), Some("IPAexGothic"), "{japanese:?}");
    }
}

#[test]
fn craft_fonts_japanese_faces_stand_in_when_present() {
    if !japanese_fonts().any(|f| f.family == "BIZ UDPGothic") {
        eprintln!("skipped: built without craft-fonts' Japanese faces (set CRAFT_FONTS_DIR)");
        return;
    }
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    assert_eq!(db.document_face("メイリオ", "Regular").family, "BIZ UDPGothic");
    if japanese_fonts().any(|f| f.family.contains("Mincho")) {
        assert!(db.document_face("ＭＳ Ｐ明朝", "Regular").family.contains("Mincho"));
    }
    // Kana a Latin face lacks: the craft-fonts gothic, the first of the Japanese fallbacks.
    let latin = db.face(crate::FALLBACK_FAMILY, "Regular");
    if !latin.covers('あ') {
        assert_eq!(db.cascade_for(&['あ'], std::slice::from_ref(&latin), "Regular", None).family, "BIZ UDPGothic");
    }
}

/// A Latin face drawing only A and B, sans or serif.
fn latin(db: &FontDb, family: &str, serif: bool) -> Arc<FontFace> {
    let names = windows_names(family, "Regular", family, None, None);
    db.add_font(font(&Spec { names: &names, chars: &['B'], weight: 400, fs_selection: 0x40, family_class: if serif { 1 << 8 } else { 8 << 8 } }));
    db.face(family, "Regular")
}

/// East Asian characters the run's fonts lack take a face of the text's tradition: kanji in
/// Japanese text a Japanese face, in Chinese text a Chinese face (Han unification gives them the
/// same code point); kana always a Japanese face and Hangul a Korean one; a serif run's kanji a
/// Mincho; bold text a bold face.
#[test]
fn east_asian_fallback_follows_the_texts_tradition_genre_and_style() {
    // Han text with no tradition of its own falls back in the system's, which must be unset.
    let _system = SystemLocale::set(None);
    let db = stand_ins();
    let sans = latin(&db, "DeckCraft Latin Sans", false);
    let serif = latin(&db, "DeckCraft Latin Serif", true);
    assert!(!sans.is_serif() && serif.is_serif());
    let chain = std::slice::from_ref(&sans);
    let pick = |cluster: &[char], chain: &[Arc<FontFace>], style: &str, cjk: Option<CjkLocale>| {
        let f = db.cascade_for(cluster, chain, style, cjk);
        (f.family.clone(), f.style.clone())
    };
    let fam = |cluster: &[char], cjk| pick(cluster, chain, "Regular", cjk).0;
    assert_eq!(fam(&['漢'], Some(CjkLocale::Japanese)), "IPAexGothic");
    assert_eq!(fam(&['漢'], Some(CjkLocale::SimplifiedChinese)), "WenQuanYi Micro Hei");
    assert_eq!(fam(&['한'], Some(CjkLocale::Japanese)), "NanumGothic", "Hangul is always Korean");
    assert_eq!(fam(&['あ'], Some(CjkLocale::SimplifiedChinese)), "IPAexGothic", "kana is always Japanese");
    assert_eq!(fam(&['あ'], None), "IPAexGothic", "kana is always Japanese");
    assert_eq!(fam(&['、'], Some(CjkLocale::Japanese)), "IPAexGothic", "CJK punctuation follows the text");
    assert_eq!(fam(&['Ａ'], Some(CjkLocale::SimplifiedChinese)), "WenQuanYi Micro Hei", "full-width forms follow the text");
    // A serif run (Times New Roman, a Mincho) takes a Mincho for its kanji.
    assert_eq!(pick(&['漢'], std::slice::from_ref(&serif), "Regular", Some(CjkLocale::Japanese)).0, "IPAexMincho");
    // Bold text takes the family's bold face; the requested face is kept when it covers.
    assert_eq!(pick(&['漢'], chain, "Bold", Some(CjkLocale::Japanese)), ("IPAexGothic".to_string(), "Bold".to_string()));
    assert_eq!(pick(&['B'], chain, "Regular", Some(CjkLocale::Japanese)).0, "DeckCraft Latin Sans");
    // Han text with no tradition keeps the loaded-faces-first order (the Chinese face, loaded
    // first), as before.
    assert_eq!(fam(&['漢'], None), "WenQuanYi Micro Hei");
    // A styled generic fallback keeps the family's bold.
    let f = db.fallback_styled('漢', u32::MAX, "Bold", Some(CjkLocale::Japanese)).unwrap();
    assert_eq!((f.family.as_str(), f.style.as_str()), ("IPAexGothic", "Bold"));
    let f = db.fallback_styled('漢', u32::MAX, "Bold", Some(CjkLocale::SimplifiedChinese)).unwrap();
    assert_eq!((f.family.as_str(), f.style.as_str()), ("WenQuanYi Micro Hei", "Regular"), "no bold face: the caller emboldens");
}

/// The interface draws Japanese with a Japanese face first when the system's East Asian
/// language is Japanese or unknown, and Chinese first only for a Chinese system language.
#[test]
fn interface_fallback_prefers_the_systems_tradition() {
    let db = stand_ins();
    let any = |_: &FontFace| true;
    assert_eq!(db.interface_fallback('漢', Some(CjkLocale::Japanese), any).unwrap().family, "IPAexGothic");
    assert_eq!(db.interface_fallback('漢', Some(CjkLocale::SimplifiedChinese), any).unwrap().family, "WenQuanYi Micro Hei");
    assert_eq!(db.interface_fallback('한', Some(CjkLocale::Japanese), any).unwrap().family, "NanumGothic");
    // A face the interface can't use is passed over.
    assert_eq!(db.interface_fallback('漢', Some(CjkLocale::Japanese), |f: &FontFace| f.family != "IPAexGothic").unwrap().family, "IPAexMincho");
    assert!(db.interface_fallback('\u{10FFFD}', Some(CjkLocale::Japanese), any).is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn system_fallbacks_put_the_texts_tradition_first() {
    let pos = |list: &[&str], f: &str| list.iter().position(|x| *x == f).unwrap();
    let plain = crate::fontdb::system_fallbacks(None);
    assert!(pos(&plain, "PingFang SC") < pos(&plain, "Hiragino Sans"), "no tradition: the list as it was");
    let ja = crate::fontdb::system_fallbacks(Some(CjkLocale::Japanese));
    assert_eq!(ja.len(), plain.len());
    for (jp, other) in [
        ("Hiragino Sans", "PingFang SC"),
        ("Yu Gothic", "Microsoft YaHei"),
        ("Noto Sans CJK JP", "Noto Sans CJK SC"),
        ("Hiragino Sans", "Apple SD Gothic Neo"),
    ] {
        assert!(pos(&ja, jp) < pos(&ja, other), "{jp} before {other}");
    }
    assert!(pos(&ja, "Arial") < pos(&ja, "Hiragino Sans"), "Latin families keep their place");
    let ko = crate::fontdb::system_fallbacks(Some(CjkLocale::Korean));
    assert!(pos(&ko, "Apple SD Gothic Neo") < pos(&ko, "PingFang SC"));
}

/// The fonts Japanese decks name, when this machine has them, by their Japanese names (on a
/// Mac: Hiragino; with Office: Yu Gothic, Meiryo, MS PGothic). Skipped for fonts not there.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn installed_japanese_fonts_are_found_by_their_japanese_names() {
    let db = FontDb::global();
    for (japanese, english) in [
        ("ヒラギノ角ゴシック", "Hiragino Sans"),
        ("ヒラギノ角ゴ ProN W3", "Hiragino Kaku Gothic ProN W3"),
        ("ヒラギノ明朝 ProN", "Hiragino Mincho ProN"),
        ("游ゴシック", "Yu Gothic"),
        ("メイリオ", "Meiryo"),
        ("ＭＳ Ｐゴシック", "MS PGothic"),
        ("ＭＳ 明朝", "MS Mincho"),
        ("游ゴシック体", "YuGothic"),
    ] {
        if !db.has_family(english) {
            eprintln!("skipped: {english} is not installed");
            continue;
        }
        assert!(db.has_family(japanese), "{japanese}");
        let f = db.face(japanese, "Regular");
        assert!(f.answers_to(english), "{japanese} → {} {}", f.family, f.style);
    }
    if db.has_family("Hiragino Kaku Gothic ProN W3") && db.has_family("Hiragino Kaku Gothic ProN W6") {
        assert_eq!(db.face("ヒラギノ角ゴ ProN W3", "Bold").style, "W6");
    }
    if db.has_family("Hiragino Mincho ProN W3") {
        assert_eq!(db.face("ヒラギノ明朝 ProN W3", "Bold").style, "W6");
    }
}
