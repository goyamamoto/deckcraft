//! Finds interface text drawn without going through translation: the main window is laid out in
//! the pseudo-language (`pseudo.rs`) on every ribbon tab, and visible words outside `⟦…⟧` marks are
//! listed. The list is printed, not enforced, while most literals still wait for `tl!`; what is
//! translated centrally (the tab strip, the status bar's counter) is enforced. Run it with
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

/// Run `n` frames and return every text drawn in the last.
fn frames(ctx: &egui::Context, app: &mut SlideApp, n: usize) -> Vec<String> {
    // Wide enough that the ribbon's groups aren't cut off at the window's edge.
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(2560.0, 900.0));
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
        let drawn = frames(&ctx, &mut app, 2);
        for name in crate::ribbon::TABS.iter().chain(&["File"]) {
            let marked = format!("⟦{name}⟧");
            assert!(drawn.contains(&marked), "the {name} tab is drawn through translation ({tab} tab open)");
            assert!(!drawn.iter().any(|t| t == name), "the {name} tab is drawn untranslated ({tab} tab open)");
        }
        assert!(drawn.iter().any(|t| t.starts_with("⟦Slide ") && t.contains(" of ")), "the slide counter is translated: {tab}");
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

/// The same list for the window's other surfaces: every page of the File tab, the slide show (its
/// toolbar, rehearsal timer, Presenter View, See All Slides, Reading View, Go to Slide and end
/// screen), a status message and a toast. The File tab's side menu and the show's slide counter
/// are enforced; the rest is printed. Run it with
/// `cargo test -p deckcraft-ui-egui untranslated_shell_text -- --nocapture`.
#[test]
fn untranslated_shell_text() {
    let ctx = egui::Context::default();
    let mut session = deckcraft_engine::Session::new();
    deckcraft_engine::sample::open_sample(&mut session).unwrap();
    let mut app = SlideApp::new(session, crate::Services::default());
    frames(&ctx, &mut app, 3);
    // The document's text, font and theme names and the product's name are not interface text.
    let mut names: BTreeSet<String> =
        app.session.active().map(|d| crate::uifonts::ui_text(d).lines().map(str::to_string).collect()).unwrap_or_default();
    names.extend(deckcraft_fonts::FontDb::global().families());
    names.extend(deckcraft_model::theme::builtin_themes().into_iter().map(|t| t.name));
    names.insert("DeckCraft".into());
    pseudo::set(true);
    let mut found: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut note = |surface: &'static str, drawn: &[String]| {
        for text in drawn.iter().filter(|t| english_words(t)) {
            let plain = pseudo::unmarked(text);
            let plain = plain.trim();
            if !names.contains(plain) {
                found.entry(plain.to_string()).or_default().insert(surface);
            }
        }
    };
    for page in crate::backstage::Page::ALL {
        app.run("app.backstage", json!({ "page": page.id() })).unwrap();
        let drawn = frames(&ctx, &mut app, 2);
        let (top, bottom) = crate::backstage::entries(true);
        for nav in top.into_iter().chain(bottom) {
            let label = match nav {
                crate::backstage::Nav::Page(p) => p.label(),
                crate::backstage::Nav::Save => "Save",
                crate::backstage::Nav::Close => "Close",
                crate::backstage::Nav::Options => "Options",
            };
            assert!(drawn.contains(&format!("⟦{label}⟧")), "the File tab's {label} is drawn through translation ({} page)", page.id());
        }
        note(page.id(), &drawn);
    }
    app.close_backstage();
    type Setup = fn(&mut crate::show::Show);
    let shows: [(&'static str, Setup); 6] = [
        ("show", |s| s.rehearse = true),
        ("presenter", |s| s.presenter = true),
        ("grid", |s| s.grid = true),
        ("reading", |s| s.reading = true),
        ("zoom", |s| s.zoom_arm = true),
        ("end", |s| s.ended = true),
    ];
    for (surface, setup) in shows {
        let doc = app.session.active().map(|d| d.doc.clone()).unwrap();
        let mut show = crate::show::Show::new(&doc, 0, false, false);
        setup(&mut show);
        app.show = Some(show);
        let drawn = frames(&ctx, &mut app, 3);
        if surface == "reading" {
            assert!(drawn.iter().any(|t| t.starts_with("⟦Slide ") && t.contains(" of ")), "the Reading View's slide counter is translated");
        }
        note(surface, &drawn);
        app.show = None;
    }
    // Go to Slide's digits, typed during the show.
    let doc = app.session.active().map(|d| d.doc.clone()).unwrap();
    app.show = Some(crate::show::Show::new(&doc, 0, false, false));
    frames(&ctx, &mut app, 1);
    let input = egui::RawInput {
        events: vec![egui::Event::Key { key: egui::Key::Num2, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
        time: Some(TIME.get() + 0.1),
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(2560.0, 900.0))),
        ..Default::default()
    };
    TIME.set(TIME.get() + 0.1);
    let mut out = ctx.run_ui(input, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out.textures_delta.clear();
    let drawn = frames(&ctx, &mut app, 1);
    note("goto", &drawn);
    app.show = None;
    app.set_status("Saved");
    app.toast =
        crate::toast::missing_fonts_message(&["Qqq Sans".into(), "Zzz Serif".into()]).map(|text| crate::toast::Toast { text, at: crate::now_ms() });
    let drawn = frames(&ctx, &mut app, 2);
    note("messages", &drawn);
    pseudo::set(false);
    let mut report = String::new();
    for (text, surfaces) in &found {
        report.push_str(&format!("{text:?}  [{}]\n", surfaces.iter().copied().collect::<Vec<_>>().join(", ")));
    }
    eprintln!("{} texts on the File tab, the slide show and the messages are drawn untranslated (wrap them in tl!):\n{report}", found.len());
}

/// The missing-translation report (craft-i18n's integration recipe): the embedded catalogs'
/// problems, and per language the interface strings with no translation (translators fill them in
/// craft-i18n's `deckcraft.tsv`). A missing translation shows in English, so the report never
/// fails; without embedded catalogs it is skipped. The strings are the ones the audit saw looked
/// up, the ribbon tabs and every command's label.
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
    // A pattern of placeholders and figures only has nothing to translate.
    let words = |s: &str| english_words(&s.split('{').map(|p| p.split_once('}').map_or(p, |(_, rest)| rest)).collect::<String>());
    for lang in super::Lang::all().filter(|l| *l != super::Lang::EN) {
        let missing: Vec<&String> = keys.iter().filter(|s| words(s) && !super::has(lang, s)).collect();
        if !missing.is_empty() {
            eprintln!("{} strings have no {} translation:\n{missing:#?}", missing.len(), lang.code());
        }
    }
}
