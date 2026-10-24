//! Interface language tests. Lookups are tested with fixture catalogs, never the embedded
//! catalogs' wording; tests that need the embedded catalogs skip without them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

/// Skips a test that needs the embedded catalogs when none were embedded (`CRAFT_I18N_DIR` unset,
/// or no `deckcraft.tsv` yet): the interface is then English only.
macro_rules! needs_catalogs {
    () => {
        if !catalogs_embedded() {
            return;
        }
    };
}

/// A language tag no catalog will ever have: `qaa`–`qtz` are reserved for local use (ISO 639-2).
const UNSUPPORTED: &str = "qaa-QM";

#[test]
fn the_interface_is_english_under_test() {
    // Whatever the machine's language: tests address widgets by their English labels.
    assert_eq!(system_lang(), Lang::EN);
    assert_eq!(Lang::from_pref(AUTO), Lang::EN);
    assert_eq!(Lang::from_pref("xx-unknown"), Lang::EN, "an unknown code follows the system");
    assert_eq!(std::thread::spawn(current).join().unwrap(), Lang::EN);
    assert_eq!(t("Insert"), "Insert");
    assert_eq!(tid("slide.new", "New Slide"), "New Slide");
    assert_eq!(message("Saved"), "Saved");
}

#[test]
fn lookups_fall_back_to_english() {
    let fx = fixture_lang("x-fixture", "\tFile\tFile-FX\n");
    assert_eq!(tr(fx, "File"), "File-FX");
    assert_eq!(tr(fx, "no such label"), "no such label");
    assert_eq!(tr(Lang::EN, "File"), "File");
    assert!(has(fx, "File") && !has(fx, "Save") && !has(Lang::EN, "File"));
}

#[test]
fn current_language_is_per_thread() {
    set_current(fixture_lang("x-fixture", "\tInsert\tInsert-FX\n\tHome\tHome-FX\n"));
    assert_eq!(t("Insert"), "Insert-FX");
    assert_eq!(crate::ribbon::tab_label("Home"), "Home-FX");
    std::thread::spawn(|| assert_eq!(current(), Lang::EN)).join().unwrap();
    set_current(Lang::EN);
    assert_eq!(t("Insert"), "Insert");
}

#[test]
fn placeholders_fill_without_reinterpreting_values() {
    let args = [("path", "a-{n}.pptx"), ("n", "2")];
    assert_eq!(fmt("{n}: {path}; {unknown}", &args), "2: a-{n}.pptx; {unknown}");
    assert_eq!(fmt("text {unfinished", &args), "text {unfinished");
    // A translation may put the placeholders in another order.
    let fx = fixture_lang("x-fixture", "\tSlide {n} of {count}\t{count}: {n}\n");
    assert_eq!(fmt(tr(fx, "Slide {n} of {count}"), &[("n", "3"), ("count", "9")]), "9: 3");
}

#[test]
fn command_labels_go_by_id_then_label() {
    set_current(fixture_lang("x-fixture", "@id\tview.pane\tTask Pane-ID\n\tTask Pane\tTask Pane-FX\n\tNew Slide\tNew Slide-FX\n"));
    assert_eq!(tid("view.pane", "Task Pane"), "Task Pane-ID");
    assert_eq!(tid("slide.new", "New Slide"), "New Slide-FX");
    assert_eq!(tid("slide.nope", "Untranslated"), "Untranslated");
    set_current(Lang::EN);
}

#[test]
fn two_line_labels_take_the_one_line_row() {
    set_current(fixture_lang("x-fixture", "\tSelection Pane\tSelection-FX\n\tDesign\\nIdeas\tDesign-Two\\nLines\n"));
    assert_eq!(label("Selection\nPane"), "Selection-FX");
    assert_eq!(label("Design\nIdeas"), "Design-Two\nLines", "a two-line row wins");
    assert_eq!(label("New\nSlide"), "New\nSlide", "no row: the English, line break kept");
    assert_eq!(label("Selection Pane"), "Selection-FX");
    set_current(Lang::EN);
    assert_eq!(label("Selection\nPane"), "Selection\nPane");
}

#[test]
fn messages_take_their_row_or_a_matching_template() {
    set_current(fixture_lang(
        "x-fixture",
        "@msg\tSaved\tSaved-FX\n@msg\tCouldn't open {name}: {e}\t{name}-FX: {e}\n@msg\tMatch {n} of {count}\t{count}/{n}-FX\n\tNo matches\tNo matches-FX\n",
    ));
    assert_eq!(message("Saved"), "Saved-FX");
    assert_eq!(message("No matches"), "No matches-FX", "a plain row serves a message too");
    assert_eq!(message("Couldn't open deck.pptx: not a zip: bad header"), "deck.pptx-FX: not a zip: bad header");
    assert_eq!(message("Match 2 of 7"), "7/2-FX");
    assert_eq!(message("Match 2 of 7 on slide 3"), "7 on slide 3/2-FX", "the last placeholder takes the rest");
    assert_eq!(message("Couldn't open"), "Couldn't open", "a template needs text for each placeholder");
    assert_eq!(message("Something else"), "Something else");
    set_current(Lang::EN);
    assert_eq!(message("Saved"), "Saved");
}

/// Several templates can read the same message; the one with the most literal text gives the
/// wording, whatever order the catalog lists them in (the missing-fonts toast has one per number
/// of names).
#[test]
fn the_most_specific_template_gives_a_message_its_wording() {
    set_current(fixture_lang(
        "x-fixture",
        "@msg\t{a} and {b} aren't installed.\t2: {a} + {b}\n@msg\t{a}, {b} and {c} aren't installed.\t3: {a} + {b} + {c}\n@msg\t{a} isn't installed.\t1: {a}\n",
    ));
    assert_eq!(message("Sora and Inter aren't installed."), "2: Sora + Inter");
    assert_eq!(message("Sora, Inter and Lato aren't installed."), "3: Sora + Inter + Lato", "not the two-name template");
    assert_eq!(message("Sora isn't installed."), "1: Sora");
    set_current(Lang::EN);
    let fonts: Vec<String> = ["Sora", "Inter", "Lato", "Aptos"].iter().map(|s| s.to_string()).collect();
    set_current(fixture_lang(
        "x-fixture",
        "@msg\t{a} and {b} aren't installed on this computer. Text that uses them is shown in a similar font.\t2-FX\n@msg\t{a}, {b} and {c} aren't installed on this computer. Text that uses them is shown in a similar font.\t3-FX\n@msg\t{a}, {b}, {c} and {n} more aren't installed on this computer. Text that uses them is shown in a similar font.\t{a}/{b}/{c}+{n}-FX\n",
    ));
    for (n, want) in [(2, "2-FX"), (3, "3-FX"), (4, "Sora/Inter/Lato+1-FX")] {
        let text = crate::toast::missing_fonts_message(&fonts[..n]).unwrap();
        assert_eq!(message(&text), want, "{text}");
    }
    set_current(Lang::EN);
}

#[test]
fn templates_match_only_whole_messages() {
    let m = |t: &str, s: &str| match_template(t, s).map(|a| a.into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(","));
    assert_eq!(m("Couldn't open {name}: {e}", "Couldn't open a.pptx: gone"), Some("name=a.pptx,e=gone".into()));
    assert_eq!(m("Saved {name}.", "Saved a.pptx."), Some("name=a.pptx".into()));
    assert_eq!(m("Saved {name}.", "Saved a.pptx"), None, "the closing literal is missing");
    assert_eq!(m("{a}{b}", "xy"), None, "placeholders in a row");
    assert_eq!(m("{a}", "xy"), None, "no literal text");
    assert_eq!(m("Slide {n}", "Slide "), None, "an empty value");
    assert_eq!(m("日本語 {n} 枚", "日本語 3 枚"), Some("n=3".into()));
    assert_eq!(m("{n} 枚", "é 枚"), Some("n=é".into()), "multi-byte values");
}

#[test]
fn catalog_escapes_are_read() {
    assert_eq!(unescape("a\\nb\\tc\\\\d\\"), "a\nb\tc\\d\\");
}

#[test]
fn placeholder_prompts_follow_the_interface_language() {
    use deckcraft_model::PhType;
    assert_eq!(placeholder_prompt(PhType::Title), "Click to add title");
    set_current(fixture_lang("x-fixture", "\tClick to add title\tTitle-FX\n"));
    assert_eq!(placeholder_prompt(PhType::Title), "Title-FX");
    assert_eq!(placeholder_prompt(PhType::Body), "Click to add text");
    set_current(Lang::EN);
}

#[test]
fn cjk_text_is_recognised() {
    assert!("新しいスライド".chars().all(is_cjk));
    assert!("한국어".chars().all(is_cjk));
    assert!(!"Slide".chars().any(is_cjk));
    assert!(fixture_lang("x-fixture", "@id\tlanguage.name\t日本語\n").uses_cjk());
    assert!(!Lang::EN.uses_cjk());
}

/// DeckCraft offers exactly the languages in `LANGUAGES` when craft-i18n is embedded, and English
/// alone when it isn't: a missing catalog never fails, it shows English.
#[test]
fn the_languages_offered_are_the_embedded_ones_of_the_list() {
    assert_eq!(LANGUAGES.first().map(|l| l.code), Some("en"), "English first: the source and the fallback");
    let all: Vec<&str> = Lang::all().map(Lang::code).collect();
    if CRAFT_I18N.is_empty() {
        assert_eq!(all, ["en"]);
        assert_eq!(Lang::from_code("ja"), None);
        assert_eq!(Lang::from_pref("ja"), Lang::EN, "a saved language without a catalog shows English");
        assert_eq!(normalize_pref("ja"), None);
        assert_eq!(Lang::for_picker(), [Lang::EN]);
        return;
    }
    for code in all.iter().skip(1) {
        assert!(LANGUAGES.iter().any(|l| l.code == *code), "{code} is listed");
    }
    assert!(Lang::from_code("pseudo").is_none(), "the pseudo locale is never a setting");
    let problems = I18N.check_all();
    if !problems.is_empty() {
        eprintln!("{} catalog rows are skipped (they show in English):\n{problems:#?}", problems.len());
    }
}

#[test]
fn system_locales_map_to_japanese() {
    needs_catalogs!();
    if Lang::from_code("ja").is_none() {
        return;
    }
    let ja = Lang::from_code("ja").unwrap();
    assert_eq!(lang_from_tag("ja-JP"), Some(ja));
    assert_eq!(lang_from_tag("ja_JP.UTF-8"), Some(ja));
    assert_eq!(lang_from_tag("C"), Some(Lang::EN));
    assert_eq!(lang_from_tag(UNSUPPORTED), None);
    assert_eq!(first_supported([UNSUPPORTED, "ja-JP", "en-US"]), Some(ja));
    assert_eq!(first_supported(["en-US", "ja-JP"]), Some(Lang::EN));
    assert_eq!(Lang::from_pref("JA"), ja);
    assert_eq!(normalize_pref("Ja"), Some("ja"));
    assert_eq!(normalize_pref("Auto"), Some(AUTO));
    assert!(ja.uses_cjk());
}

#[test]
fn the_language_setting_is_a_ui_command_and_persists() {
    let mut app = crate::SlideApp::new(deckcraft_engine::Session::with_new(), crate::Services::default());
    assert_eq!(app.ui.language, AUTO);
    let r = app.run("app.language", serde_json::json!({"value": "EN"})).unwrap();
    assert_eq!(app.ui.language, "en");
    assert_eq!(r["effective"], "en");
    assert_eq!(r["available"].as_array().map(Vec::len), Some(Lang::all().count()));
    assert!(app.run("app.language", serde_json::json!({"value": "xx"})).is_err());
    assert_eq!(app.ui.language, "en", "an unknown code keeps the setting");
    assert!(app.run("app.language", serde_json::json!({"value": 7})).is_err());
    let saved = serde_json::to_string(&app.ui).unwrap();
    let restored: crate::UiState = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.language, "en");
    // Settings saved before this option existed follow the system; a damaged one too.
    let legacy: crate::UiState = serde_json::from_str(r#"{"tab":"Home"}"#).unwrap();
    assert_eq!(legacy.language, AUTO);
    let damaged: crate::UiState = serde_json::from_value(serde_json::json!({"language": "x".repeat(500)})).unwrap();
    assert_eq!(damaged.sanitized().language, AUTO);
    // A code from a newer version is kept (it may have a catalog there) and shows the system's.
    let newer: crate::UiState = serde_json::from_value(serde_json::json!({"language": "qaa"})).unwrap();
    assert_eq!(newer.sanitized().language, "qaa");
    app.run("app.language", serde_json::json!({"value": "auto"})).unwrap();
    assert_eq!(app.ui.language, AUTO);
    // Reading without a value reports the current setting.
    assert_eq!(app.run("app.language", serde_json::json!({})).unwrap()["language"], AUTO);
    if let Some(ja) = Lang::from_code("ja") {
        app.run("app.language", serde_json::json!({"value": "ja"})).unwrap();
        assert_eq!(Lang::from_pref(&app.ui.language), ja);
    }
}

/// Preferences ▸ General shows the interface language; a language picked there applies at once
/// and Cancel puts back the one the dialog opened with.
#[test]
fn preferences_show_the_language_and_cancel_restores_it() {
    use egui_kittest::kittest::Queryable;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut crate::SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        crate::SlideApp::new(deckcraft_engine::Session::with_new(), crate::Services::default()),
    );
    h.run_steps(2);
    h.state_mut().run("app.preferences", serde_json::json!({})).unwrap();
    h.run_steps(3);
    assert!(h.query_all_by_label("Interface language:").next().is_some(), "the picker's label");
    assert!(h.query_all_by_role(egui::accesskit::Role::ComboBox).next().is_some(), "the language dropdown");
    // What the picker does when a language is chosen.
    h.state_mut().run("app.language", serde_json::json!({"value": "en"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ui.language, "en");
    h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Cancel").next().expect("Cancel").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().ui.language, AUTO, "Cancel restores the language");
}

/// `craft-i18n.rev` pins the craft-i18n commit CI and releases check out: one full SHA.
#[test]
fn craft_i18n_rev_holds_one_sha() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rev = std::fs::read_to_string(root.join("craft-i18n.rev")).unwrap();
    let rev = rev.trim();
    assert!(rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()), "craft-i18n.rev holds one full SHA: {rev:?}");
}

/// The macOS bundle declares the interface languages, so system panels follow them.
#[test]
fn the_macos_bundle_declares_the_interface_languages() {
    let plist = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/macos/Info.plist.in")).unwrap();
    let after = plist.split("<key>CFBundleLocalizations</key>").nth(1).expect("CFBundleLocalizations");
    let array = after.split("</array>").next().unwrap_or("");
    let declared: Vec<&str> = array.split("<string>").skip(1).filter_map(|s| s.split("</string>").next()).collect();
    let offered: Vec<&str> = LANGUAGES.iter().map(|l| l.code).collect();
    assert_eq!(declared, offered);
}

#[test]
fn the_picker_lists_english_first_then_names_alphabetically() {
    let order = Lang::for_picker();
    assert_eq!(order.first().map(|l| l.code()), Some(Lang::EN.code()));
    assert_eq!(order.len(), Lang::all().count());
    let keys: Vec<String> = order[1..].iter().map(|l| name_sort_key(l.name())).collect();
    assert!(keys.windows(2).all(|w| w[0] <= w[1]), "{keys:?}");
    assert_eq!(name_sort_key("Čeština"), "cestina");
}

/// A fixture Japanese interface: the ribbon still lays out (labels translated and measured, a
/// long Japanese label wrapped in two lines), the tabs keep their English ids (`view.tab Home`),
/// and the toolbar and commands keep English.
#[test]
fn a_japanese_interface_lays_out_the_ribbon_and_keeps_english_ids() {
    use egui_kittest::kittest::Queryable;
    let ja = fixture_lang(
        "ja",
        "@id\tlanguage.name\t日本語\n\tFile\tファイル\n\tHome\tホーム\n\tInsert\t挿入\n\tNew Slide\t新しいスライド\n\tSelection Pane\t選択ウィンドウ\n\tCut\t切り取り\n\tSlide {n} of {count}\tスライド {n}/{count}\n\tClick to add title\tタイトルを入力\n",
    );
    let mut session = deckcraft_engine::Session::new();
    deckcraft_engine::sample::open_sample(&mut session).unwrap();
    let app = crate::SlideApp::new(session, crate::Services::default());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 860.0)).build_ui_state(
        move |ui, app: &mut crate::SlideApp| {
            app.logic(ui.ctx());
            set_current(ja);
            app.ui(ui);
            set_current(Lang::EN);
        },
        app,
    );
    h.run_steps(4);
    let drawn = |h: &egui_kittest::Harness<'_, crate::SlideApp>, label: &str| h.query_all_by_label(label).count();
    // Translated, measured and named as drawn.
    assert_eq!(drawn(&h, "ファイル"), 1, "the File tab");
    assert_eq!(drawn(&h, "切り取り"), 1, "a small button");
    assert_eq!(drawn(&h, "Cut"), 0);
    let new_slide = h.query_all_by_label("新しいスライド").next().expect("New Slide, translated").rect();
    // Two lines are narrower than the label on one.
    let one_line = h.ctx.fonts_mut(|f| f.layout_no_wrap("新しいスライド".into(), crate::theme::font(11.0), egui::Color32::WHITE).size().x);
    assert!(new_slide.width() < one_line + 10.0, "{} vs {one_line}", new_slide.width());
    assert!(new_slide.height() <= crate::widgets::RIBBON_H, "inside the ribbon: {new_slide:?}");
    // Tabs keep their English ids.
    assert_eq!(h.state().ui.tab, "Home");
    h.state_mut().run("view.tab", serde_json::json!({"tab": "Insert"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ui.tab, "Insert");
    h.state_mut().run("view.tab", serde_json::json!({"tab": "Home"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ui.tab, "Home", "view.tab still takes the English name");
    assert_eq!(drawn(&h, "切り取り"), 1, "the Home tab is drawn again");
    // The toolbar keeps English labels, whatever it shows.
    assert!(h.state().ui.qat.iter().any(|q| q.label == "Home"));
    assert!(drawn(&h, "ホーム") >= 1, "the toolbar's Home button is drawn translated");
}
