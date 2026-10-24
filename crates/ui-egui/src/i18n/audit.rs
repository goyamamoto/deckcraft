//! Finds interface text drawn without going through translation: the main window is laid out in
//! the pseudo-language (`pseudo.rs`) on every ribbon tab, and visible words outside `⟦…⟧` marks are
//! listed, also at a narrow width, where ribbon groups fold into buttons named by the group. The
//! list is printed, not enforced, while most literals still wait for `tl!`; what is translated
//! centrally (the tab strip, the status bar's counter) is enforced. Run it with
//! `cargo test -p deckcraft-ui-egui untranslated_interface_text -- --nocapture` to see the list
//! and, with `CRAFT_I18N_DIR` set, the strings each language has no translation for.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::pseudo;
use crate::SlideApp;

thread_local! {
    // Input time, advanced every frame so tooltips and animations move on.
    static TIME: Cell<f64> = const { Cell::new(0.0) };
}

fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
        _ => {}
    }
}

/// [`frames_at`] wide enough that every ribbon group is drawn as designed.
fn frames(ctx: &egui::Context, app: &mut SlideApp, n: usize) -> Vec<String> {
    frames_at(ctx, app, n, 2560.0)
}

/// Run `n` frames of a window `width` points wide and return every text drawn in the last.
fn frames_at(ctx: &egui::Context, app: &mut SlideApp, n: usize, width: f32) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0));
    let mut drawn = Vec::new();
    for _ in 0..n {
        TIME.set(TIME.get() + 0.1);
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(TIME.get()), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        drawn.clear();
        for s in &out.shapes {
            texts(&s.shape, &mut drawn);
        }
        out.textures_delta.clear();
    }
    drawn
}

/// Words (two or more ASCII letters in a row) drawn outside translation marks.
fn english_words(text: &str) -> bool {
    let plain = pseudo::unmarked(text);
    plain.as_bytes().windows(2).any(|w| w[0].is_ascii_alphabetic() && w[1].is_ascii_alphabetic())
}

/// Lists the interface text the main window draws untranslated on each ribbon tab, and enforces the
/// centrally translated parts.
#[test]
fn untranslated_interface_text() {
    let ctx = egui::Context::default();
    let mut session = deckcraft_engine::Session::new();
    deckcraft_engine::sample::open_sample(&mut session).unwrap();
    let mut app = SlideApp::new(session, crate::Services::default());
    // Fonts install over the first frames.
    frames(&ctx, &mut app, 3);
    // The document's own text and names are not interface text.
    let doc_text: BTreeSet<String> =
        app.session.active().map(|d| crate::uifonts::ui_text(d).lines().map(str::to_string).collect()).unwrap_or_default();
    let fonts: BTreeSet<String> = deckcraft_fonts::FontDb::global().families().into_iter().collect();
    pseudo::set(true);
    let mut found: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for tab in crate::ribbon::TABS {
        app.run("view.tab", json!({ "tab": tab })).unwrap();
        let mut drawn = frames(&ctx, &mut app, 2);
        for name in crate::ribbon::TABS.iter().chain(&["File"]) {
            let marked = format!("⟦{name}⟧");
            assert!(drawn.contains(&marked), "the {name} tab is drawn through translation ({tab} tab open)");
            assert!(!drawn.iter().any(|t| t == name), "the {name} tab is drawn untranslated ({tab} tab open)");
        }
        assert!(drawn.iter().any(|t| t.starts_with("⟦Slide ") && t.contains(" of ")), "the slide counter is translated: {tab}");
        // A narrow window folds ribbon groups into drop-down buttons named by the group
        // (`ribbon_fit`): their names go through translation too. Plan, then draw the plan.
        drawn.extend(frames_at(&ctx, &mut app, 6, 900.0));
        for text in drawn.iter().filter(|t| english_words(t)) {
            let plain = pseudo::unmarked(text);
            let plain = plain.trim();
            if doc_text.contains(plain) || fonts.contains(plain) || plain == "DeckCraft" {
                continue;
            }
            found.entry(plain.to_string()).or_default().insert(tab);
        }
    }
    pseudo::set(false);
    let mut report = String::new();
    for (text, tabs) in &found {
        report.push_str(&format!("{text:?}  [{}]\n", tabs.iter().copied().collect::<Vec<_>>().join(", ")));
    }
    eprintln!("{} interface texts on the ribbon tabs are drawn untranslated (wrap them in tl!):\n{report}", found.len());
    report_catalog_gaps(pseudo::looked_up());
}

/// The missing-translation report (craft-i18n's integration recipe): the embedded catalogs'
/// problems, and per language the interface strings with no translation (translators fill them in
/// craft-i18n's `deckcraft.tsv`). A missing translation shows in English, so the report never
/// fails; without embedded catalogs it is skipped. The strings are the ones the audit saw looked
/// up, the ribbon tabs, every command's label and the names the ribbon's galleries show from the
/// engine's tables ([`crate::ribbon::vocabulary`]).
fn report_catalog_gaps(mut keys: BTreeSet<String>) {
    if super::CRAFT_I18N.is_empty() {
        eprintln!("missing-translation report skipped: built without craft-i18n catalogs (set CRAFT_I18N_DIR to a craft-i18n checkout)");
        return;
    }
    let problems = super::I18N.check_all();
    if !problems.is_empty() {
        eprintln!("{} catalog rows are skipped (they show in English):\n{problems:#?}", problems.len());
    }
    keys.extend(crate::ribbon::TABS.iter().map(|t| t.to_string()));
    let session = deckcraft_engine::Session::with_new();
    keys.extend(session.commands().into_iter().map(|c| c.label.to_string()));
    keys.extend(crate::UI_COMMANDS.iter().map(|c| c.1.to_string()));
    // The names the ribbon's galleries and menus show from the engine's tables.
    keys.extend(crate::ribbon::vocabulary());
    // A pattern of placeholders and figures only has nothing to translate.
    let words = |s: &str| english_words(&s.split('{').map(|p| p.split_once('}').map_or(p, |(_, rest)| rest)).collect::<String>());
    for lang in super::Lang::all().filter(|l| *l != super::Lang::EN) {
        let missing: Vec<&String> = keys.iter().filter(|s| words(s) && !super::has(lang, s)).collect();
        if !missing.is_empty() {
            eprintln!("{} strings have no {} translation:\n{missing:#?}", missing.len(), lang.code());
        }
    }
}
