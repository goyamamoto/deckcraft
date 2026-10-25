//! Adapted from WordCraft's `tests/mac_menu_appkit.rs` (storytold/wordcraft d2d407bc), after
//! storytold/wordcraft#985 (YTwsy) and PhotoCraft at 67e4b8a499b55ef728b1da700005382c45e4677d
//! (MIT; see NOTICE).
//!
//! The macOS menu bar end to end, on the main thread (`harness = false`): install it through the
//! real muda backend (`src/mac_menu.rs`), read AppKit's menu bar back, edit a presentation and see
//! the items update in place, then choose items the two ways AppKit can: a key equivalent (a real
//! ⇧F9 key-down through `-[NSMenu performKeyEquivalent:]`) must reach egui as the key press it was,
//! and a click (`-[NSMenu performActionForItemAtIndex:]`) must run as a click. A system file dialog
//! swaps the Edit rows for AppKit's own selectors and back.

// The app's own module, built into this test (cargo builds harness = false tests with cfg(test),
// so its unit tests compile here too without a harness to run them).
#[cfg(target_os = "macos")]
#[path = "../src/mac_menu.rs"]
#[allow(unused_imports, dead_code)]
mod mac_menu;

#[cfg(target_os = "macos")]
fn main() {
    use deckcraft_ui_egui::native_menu::{self, NativeMenu};
    use deckcraft_ui_egui::{Services, SlideApp};
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSMenu};
    use objc2_foundation::{NSPoint, NSString};
    use serde_json::json;

    let mtm = MainThreadMarker::new().expect("harness = false runs on the main thread");
    let nsapp = NSApplication::sharedApplication(mtm);
    nsapp.finishLaunching();

    let titles = |menu: &NSMenu| -> Vec<String> {
        (0..menu.numberOfItems()).filter_map(|i| menu.itemAtIndex(i)).filter(|it| !it.isSeparatorItem()).map(|it| it.title().to_string()).collect()
    };
    let submenu = |menu: &NSMenu, title: &str| -> Retained<NSMenu> {
        (0..menu.numberOfItems())
            .filter_map(|i| menu.itemAtIndex(i))
            .find(|it| it.title().to_string() == title)
            .and_then(|it| it.submenu())
            .unwrap_or_else(|| panic!("no {title} submenu in {:?}", titles(menu)))
    };
    let index = |menu: &NSMenu, title: &str| -> isize {
        (0..menu.numberOfItems())
            .find(|&i| menu.itemAtIndex(i).is_some_and(|it| it.title().to_string() == title))
            .unwrap_or_else(|| panic!("no {title} in {:?}", titles(menu)))
    };
    let main_menu = || nsapp.mainMenu().expect("a main menu");
    // Make `event` AppKit's current event, as the run loop does before dispatching it.
    let dequeue = |event: &NSEvent| {
        nsapp.postEvent_atStart(event, true);
        let mode = NSString::from_str("kCFRunLoopDefaultMode");
        let got = nsapp.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, None, &mode, true).expect("the posted event");
        assert_eq!(got.r#type(), event.r#type());
        got
    };
    let key_down = |chars: &str, code: u16, flags: NSEventModifierFlags| {
        let s = NSString::from_str(chars);
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::new(0.0, 0.0),
            flags,
            0.0,
            0,
            None,
            &s,
            &s,
            false,
            code,
        )
        .expect("a key event")
    };

    let mut app = SlideApp::new(deckcraft_engine::Session::with_new(), Services::default());
    let ctx = egui::Context::default();
    app.ui.language = "en".into();
    app.menu_bar_installed = true;
    let mut menu: NativeMenu = mac_menu::install(&ctx, &mut app).expect("the menu bar installed");
    let mut time = 0.0;
    let mut frame = |app: &mut SlideApp, menu: &mut NativeMenu, mut raw: egui::RawInput| {
        time += 1.0 / 60.0;
        raw.time = Some(time);
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0)));
        native_menu::raw_input(menu, app, &mut raw);
        app.raw_input_hook(&mut raw);
        let mut out = ctx.run_ui(raw, |ui| {
            native_menu::run(menu, app, ui.ctx());
            app.logic(ui.ctx());
            app.ui(ui);
            native_menu::sync(menu, app, ui.ctx());
        });
        out.textures_delta.clear();
    };
    // The fonts install over the first frames.
    frame(&mut app, &mut menu, Default::default());
    frame(&mut app, &mut menu, Default::default());

    // The Mac layout, as AppKit has it: PowerPoint for Mac's eleven menus.
    let bar = main_menu();
    let tops: Vec<String> = (0..bar.numberOfItems()).filter_map(|i| bar.itemAtIndex(i)?.submenu()).map(|m| m.title().to_string()).collect();
    assert_eq!(tops, [deckcraft_ui_egui::APP_NAME, "File", "Edit", "View", "Insert", "Format", "Arrange", "Tools", "Slide Show", "Window", "Help"]);
    let app_menu = submenu(&bar, deckcraft_ui_egui::APP_NAME);
    let names = titles(&app_menu);
    let (hide, quit) = (format!("Hide {}", deckcraft_ui_egui::APP_NAME), format!("Quit {}", deckcraft_ui_egui::APP_NAME));
    for want in ["About DeckCraft", hide.as_str(), quit.as_str()] {
        assert!(names.iter().any(|n| n == want), "{want} missing from the app menu: {names:?}");
    }
    // AppKit retitles a Preferences… row Settings… on recent systems.
    assert!(names.iter().any(|n| n == "Preferences…" || n == "Settings…"), "{names:?}");
    let quit = app_menu.itemAtIndex(index(&app_menu, &quit)).expect("Quit");
    assert_eq!(quit.keyEquivalent().to_string(), "q");
    assert_eq!(quit.keyEquivalentModifierMask(), NSEventModifierFlags::Command);
    assert_eq!(nsapp.windowsMenu().map(|m| m.title().to_string()).as_deref(), Some("Window"), "AppKit knows the Window menu");
    assert_eq!(nsapp.helpMenu().map(|m| m.title().to_string()).as_deref(), Some("Help"), "AppKit knows the Help menu");
    // The Window menu as PowerPoint for Mac has it; the tiling items and the open windows AppKit adds itself.
    let window = submenu(&bar, "Window");
    let names = titles(&window);
    let ours: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| ["Minimize", "Zoom", "New Window", "Arrange All", "Cascade", "Next Window", "Bring All to Front"].contains(n))
        .collect();
    assert_eq!(ours, ["Minimize", "Zoom", "New Window", "Arrange All", "Cascade", "Next Window", "Bring All to Front"], "{names:?}");
    assert!(!window.itemAtIndex(index(&window, "Arrange All")).expect("Arrange All").isEnabled(), "no window host in this test");

    // Editing the presentation enables Undo in place: the same NSMenu, no rebuild.
    let edit = submenu(&bar, "Edit");
    let undo_enabled = |bar: &NSMenu| submenu(bar, "Edit").itemAtIndex(index(&edit, "Undo")).expect("Undo").isEnabled();
    assert!(!undo_enabled(&bar));
    app.run("slide.new", json!({})).unwrap();
    frame(&mut app, &mut menu, Default::default());
    assert!(Retained::as_ptr(&bar) == Retained::as_ptr(&main_menu()), "no rebuild on an edit");
    assert!(undo_enabled(&main_menu()));

    // ⇧F9 (Gridlines) as a real key equivalent: AppKit takes it, and it reaches egui as the key
    // press it was, which toggles Gridlines once.
    let gridlines = app.ui.gridlines;
    let press = dequeue(&key_down("\u{F70C}", 101, NSEventModifierFlags::Shift | NSEventModifierFlags::Function));
    assert!(main_menu().performKeyEquivalent(&press), "⇧F9 is View › Gridlines' key equivalent");
    let mut raw = egui::RawInput::default();
    native_menu::raw_input(&mut menu, &mut app, &mut raw);
    let keys: Vec<(egui::Key, bool, bool)> = raw
        .events
        .iter()
        .filter_map(|e| match e {
            egui::Event::Key { key, pressed, modifiers, .. } => Some((*key, *pressed, modifiers.shift)),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [(egui::Key::F9, true, true)], "the key press");
    frame(&mut app, &mut menu, raw);
    assert_eq!(app.ui.gridlines, !gridlines, "⇧F9 toggled Gridlines once");

    // A click on View › Ruler runs as a click.
    let view = submenu(&main_menu(), "View");
    let ruler = app.ui.ruler;
    let _ = dequeue(&key_down("", 0, NSEventModifierFlags::empty()));
    view.performActionForItemAtIndex(index(&view, "Ruler"));
    let mut raw = egui::RawInput::default();
    native_menu::raw_input(&mut menu, &mut app, &mut raw);
    assert!(raw.events.is_empty(), "a click is no key press: {:?}", raw.events);
    frame(&mut app, &mut menu, raw);
    assert_eq!(app.ui.ruler, !ruler, "the click toggled Ruler");

    // A dialog disables the presentation's rows and Quit, in place; closing it brings them back.
    let insert = submenu(&main_menu(), "Insert");
    let new_slide = insert.itemAtIndex(index(&insert, "New Slide")).expect("New Slide");
    assert!(new_slide.isEnabled());
    app.run("app.about", json!({})).unwrap();
    frame(&mut app, &mut menu, Default::default());
    assert!(!new_slide.isEnabled(), "New Slide waits for the dialog");
    assert!(!quit.isEnabled(), "Quit cannot bypass a dialog");
    app.dialog = None;
    frame(&mut app, &mut menu, Default::default());
    assert!(new_slide.isEnabled());
    assert!(quit.isEnabled());

    // While a system file dialog is open, the Edit rows are AppKit's own selectors, so the dialog's
    // Cocoa fields keep ⌘C and ⌘V; afterwards the rows are ours again.
    let edit = submenu(&main_menu(), "Edit");
    let paste_action = || edit.itemAtIndex(index(&edit, "Paste")).expect("Paste").action();
    assert_ne!(paste_action(), Some(objc2::sel!(paste:)));
    mac_menu::with_native_editing(|| {
        assert_eq!(paste_action(), Some(objc2::sel!(paste:)));
        assert_eq!(edit.itemAtIndex(index(&edit, "Copy")).expect("Copy").action(), Some(objc2::sel!(copy:)));
        assert_eq!(edit.itemAtIndex(index(&edit, "Select All")).expect("Select All").action(), Some(objc2::sel!(selectAll:)));
    });
    assert_ne!(paste_action(), Some(objc2::sel!(paste:)));
    assert_eq!(titles(&edit).iter().filter(|t| *t == "Paste").count(), 1, "one Paste row after the swap back");

    // The interface language changes the menu bar, Window menu included.
    if deckcraft_ui_egui::i18n::Lang::from_code("ja").is_some() {
        app.ui.language = "ja".into();
        frame(&mut app, &mut menu, Default::default());
        assert_eq!(
            nsapp.windowsMenu().map(|m| m.title().to_string()).as_deref(),
            Some("ウィンドウ"),
            "the Window menu follows the interface language"
        );
    }
    println!("mac_menu_appkit: ok");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
