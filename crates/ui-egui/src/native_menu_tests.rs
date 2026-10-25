//! Adapted from WordCraft's native menu tests (storytold/wordcraft d2d407bc), after
//! storytold/wordcraft#985 and PhotoCraft's native-menu tests; see NOTICE.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::Services;
use deckcraft_engine::Session;

#[derive(Default)]
struct State {
    events: Vec<Event>,
    bar: Option<MenuBar>,
    system: Vec<String>,
}

struct Fake(Rc<RefCell<State>>);

impl Backend for Fake {
    fn sync(&mut self, bar: &MenuBar) {
        self.0.borrow_mut().bar = Some(bar.clone());
    }
    fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }
    fn system_action(&mut self, id: &str) {
        self.0.borrow_mut().system.push(id.into());
    }
}

struct Setup {
    app: SlideApp,
    ctx: Context,
    menu: NativeMenu,
    state: Rc<RefCell<State>>,
    time: f64,
}

/// An app with a new presentation, warmed up (the first frames install the fonts).
fn setup() -> Setup {
    let ctx = Context::default();
    let state = Rc::new(RefCell::new(State::default()));
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.ui.language = "en".into();
    app.menu_bar_installed = true;
    let menu = NativeMenu::new(Box::new(Fake(state.clone())), ctx.clone());
    let mut s = Setup { app, ctx, menu, state, time: 0.0 };
    s.frame(Default::default());
    s.frame(Default::default());
    s
}

fn click(state: &Rc<RefCell<State>>, id: &str) {
    state.borrow_mut().events.push(Event::Click(id.into()));
}
fn key(state: &Rc<RefCell<State>>, chord: &str) {
    state.borrow_mut().events.push(Event::Key(Chord::parse(chord).unwrap()));
}

impl Setup {
    /// One frame as the window host runs it: the menu's events, the app's logic and interface.
    fn frame(&mut self, mut raw: egui::RawInput) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        raw.time = Some(self.time);
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0)));
        let Setup { app, ctx, menu, .. } = self;
        raw_input(menu, app, &mut raw);
        app.raw_input_hook(&mut raw);
        let mut output = ctx.run_ui(raw, |ui| {
            run(menu, app, ui.ctx());
            app.logic(ui.ctx());
            app.ui(ui);
            sync(menu, app, ui.ctx());
        });
        output.textures_delta.clear();
        output
    }

    fn bar(&mut self) -> MenuBar {
        menu_bar(&mut self.app, &self.ctx)
    }
}

fn slides(app: &SlideApp) -> usize {
    app.session.active().map(|d| d.doc.slides.len()).unwrap_or(0)
}

/// Every shape's text on the current slide, groups included.
fn deck_text(app: &SlideApp) -> String {
    let Some(slide) = app.session.active().and_then(|d| d.current_slide()) else { return String::new() };
    let mut out = String::new();
    deckcraft_model::walk(&slide.shapes, &mut |sh, _| {
        if let Some(t) = &sh.text {
            out.push_str(&t.text());
            out.push('\n');
        }
    });
    out
}

/// The menu bar as PowerPoint for Mac orders it, every item a registered command (or a dialog, a
/// recent file, or one of the app's own rows), each row id once.
#[test]
fn layout_has_mac_roles_and_only_existing_commands() {
    let mut s = setup();
    let bar = s.bar();
    assert_eq!(
        bar.menus.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(),
        [crate::APP_NAME, "File", "Edit", "View", "Insert", "Format", "Arrange", "Tools", "Slide Show", "Window", "Help"]
    );
    assert_eq!(bar.menus[0].role, Role::App);
    assert_eq!(bar.menus[9].role, Role::Window);
    assert_eq!(bar.menus[10].role, Role::Help);
    let mut ids = std::collections::HashSet::new();
    for it in bar.items() {
        assert!(ids.insert(&it.id), "duplicate row {}", it.id);
        assert!(menus::is_known_command(&it.command) || app_row(&it.id), "{} runs {}", it.id, it.command);
    }
    assert_eq!(bar.find(QUIT).unwrap().shortcut, Chord::parse("Cmd+Q"));
    assert_eq!(bar.find("app.preferences").unwrap().shortcut, Chord::parse("Cmd+,"));
    assert_eq!(bar.find(HIDE).unwrap().shortcut, Chord::parse("Cmd+Ctrl+H"), "Replace keeps ⌘H");
    assert_eq!(bar.find(MINIMIZE).unwrap().shortcut, Chord::parse("Cmd+M"));
    assert!(bar.find(ZOOM).unwrap().shortcut.is_none(), "Zoom has no key, as in PowerPoint");
    assert_eq!(bar.find("app.about").unwrap().label, "About DeckCraft");
    assert_eq!(bar.find(QUIT).unwrap().label, format!("Quit {}", crate::APP_NAME));
    assert_eq!(bar.find("file.save").unwrap().shortcut, Chord::parse("Cmd+S"));
    assert_eq!(bar.find("app.openDialog").unwrap().shortcut, Chord::parse("Cmd+O"));
    assert_eq!(bar.find("view.gridlines").unwrap().shortcut, Chord::parse("Shift+F9"));
    assert!(bar.find("view.guides").unwrap().shortcut.is_some(), "a function key with Option is a key equivalent too");
    assert!(bar.find("app.backstage").is_none(), "Alt+F is typing, and the File tab isn't a menu row");
    assert!(!s.app.ui.ribbon_collapsed, "installing a menu doesn't hide the ribbon");
    // Window, as PowerPoint for Mac has it: Minimize, Zoom; New Window, Arrange All, Cascade;
    // Next Window; Bring All to Front. The window list and the tiling items are AppKit's.
    let window = &bar.menus[9];
    let rows: Vec<&str> = window
        .children
        .iter()
        .map(|n| match n {
            Node::Item(it) => it.id.as_str(),
            Node::Standard(Standard::BringAllToFront) => "front",
            Node::Separator => "-",
            _ => "?",
        })
        .collect();
    assert_eq!(rows, [MINIMIZE, ZOOM, "-", "view.newWindow", "view.arrangeAll", "view.cascade", "-", "window.next", "-", "front"]);
    assert!(!bar.find("view.arrangeAll").unwrap().enabled, "no window host here: the window commands are off");
    assert!(bar.find("view.newWindow").unwrap().enabled);
    // Help holds the palette and the links; About is the app menu's.
    let help: Vec<&str> = bar.menus[10].children.iter().filter_map(|n| if let Node::Item(it) = n { Some(it.id.as_str()) } else { None }).collect();
    assert_eq!(help, ["app.palette", "app.links"]);
    // The Tools dialogs run through app.dialog with their name.
    let language = bar.find("dialog:language").unwrap();
    assert_eq!((language.command.as_str(), &language.params), ("app.dialog", &json!({"id": "language"})));
}

/// Enabled state and checkmarks follow the presentation; the tree's shape stays, so the native
/// menu updates in place. Another interface language rebuilds it.
#[test]
fn state_changes_keep_structure_and_language_refreshes_it() {
    let mut s = setup();
    let before = s.bar();
    assert!(!before.find("edit.undo").unwrap().enabled);
    assert_eq!(before.find("view.ruler").unwrap().checked, Some(false));
    assert_eq!(before.find("view.normal").unwrap().checked, Some(true));
    s.app.run("slide.new", json!({})).unwrap();
    s.app.run("view.ruler", json!({})).unwrap();
    s.app.run("view.sorter", json!({})).unwrap();
    let after = s.bar();
    assert_eq!(structure_key(&before), structure_key(&after));
    assert!(after.find("edit.undo").unwrap().enabled);
    assert_eq!(after.find("view.ruler").unwrap().checked, Some(true));
    assert_eq!((after.find("view.normal").unwrap().checked, after.find("view.sorter").unwrap().checked), (Some(false), Some(true)));
    if i18n::Lang::from_code("ja").is_none() {
        return; // built without the craft-i18n catalogs
    }
    s.app.ui.language = "ja".into();
    s.frame(Default::default());
    let japanese = s.bar();
    assert_eq!(japanese.menus[1].title, "ファイル");
    assert_ne!(structure_key(&after), structure_key(&japanese), "titles are part of the structure");
    assert!(japanese.find(QUIT).unwrap().label.contains(crate::APP_NAME));
}

/// The three shortcut spellings the commands use parse to the keys egui names; bare and Option
/// typing never become key equivalents.
#[test]
fn chords_parse_the_commands_spelling() {
    for (s, cmd, shift, key) in [
        ("Cmd+Shift+S", true, true, "S"),
        ("Cmd+`", true, false, "`"),
        ("Cmd+=", true, false, "="),
        ("Shift+F9", false, true, "F9"),
        ("F5", false, false, "F5"),
        ("Cmd+0", true, false, "0"),
        ("Cmd+Enter", true, false, "Enter"),
    ] {
        let c = Chord::parse(s).unwrap_or_else(|| panic!("{s}"));
        assert_eq!((c.cmd, c.shift, c.key.as_str()), (cmd, shift, key), "{s}");
        assert!(c.key_event().is_some(), "{s}");
    }
    for s in ["B", "Alt+Q", "Shift+A", "Alt+F"] {
        assert!(!Chord::parse(s).unwrap().native_ok(), "{s}");
    }
    for s in ["Cmd+B", "Ctrl+F", "F12", "Alt+F9", "Cmd+Shift+Enter"] {
        assert!(Chord::parse(s).unwrap().native_ok(), "{s}");
    }
    assert_ne!(Chord::parse("Cmd+M"), Chord::parse("Cmd+Ctrl+M"));
    assert!(Chord::parse("").is_none());
}

/// Two clicks queued while the menu was enabled: the first opens a dialog, so the second must not
/// run; the synced menu shows the row disabled; once the dialog closes the row works again.
#[test]
fn modal_blocks_queued_clicks_and_stale_enabled_rows() {
    let mut s = setup();
    let ruler = s.app.ui.ruler;
    click(&s.state, "app.about");
    click(&s.state, "view.ruler");
    s.frame(Default::default());
    assert_eq!(s.app.dialog.as_ref().map(|d| d.id.as_str()), Some("about"));
    assert_eq!(s.app.ui.ruler, ruler);
    assert!(!s.state.borrow().bar.as_ref().unwrap().find("view.ruler").unwrap().enabled);
    assert!(!s.state.borrow().bar.as_ref().unwrap().find(QUIT).unwrap().enabled, "Quit cannot bypass a dialog");
    s.app.dialog = None;
    click(&s.state, "view.ruler");
    s.frame(Default::default());
    assert_eq!(s.app.ui.ruler, !ruler);
}

/// A key equivalent AppKit took reaches the UI as the key press it was, placed before its real
/// release, and runs what the key runs: ⇧F9 toggles Gridlines.
#[test]
fn key_equivalent_has_one_press_before_its_real_release() {
    let mut s = setup();
    let gridlines = s.app.ui.gridlines;
    let mut release = Chord::parse("Shift+F9").unwrap().key_event().unwrap();
    if let egui::Event::Key { pressed, .. } = &mut release {
        *pressed = false;
    }
    key(&s.state, "Shift+F9");
    let mut raw = egui::RawInput { events: vec![release], ..Default::default() };
    raw_input(&mut s.menu, &mut s.app, &mut raw);
    assert!(matches!(raw.events.as_slice(), [egui::Event::Key { pressed: true, .. }, egui::Event::Key { pressed: false, .. }]), "{:?}", raw.events);
    s.frame(raw);
    assert_eq!(s.app.ui.gridlines, !gridlines);
    // A command's key: ⌘D duplicates the selected shape (the engine's own shortcut).
    let title = s.app.session.active().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.app.run("edit.select", json!({"ids": [title]})).unwrap();
    let shapes = s.app.session.active().unwrap().current_slide().unwrap().shapes.len();
    key(&s.state, "Cmd+D");
    s.frame(Default::default());
    assert_eq!(s.app.session.active().unwrap().current_slide().unwrap().shapes.len(), shapes + 1);
}

/// ⌘V from the menu pastes the clipboard's text once, as the real key does: a text box on the
/// slide; the release that winit still delivers is not a second paste.
#[test]
fn paste_equivalent_pastes_the_clipboard_text_once() {
    for same_frame in [false, true] {
        let mut s = setup();
        s.app.services.clipboard_text = Some(Box::new(|| Some("pasted words".into())));
        let mut release = Chord::parse("Cmd+V").unwrap().key_event().unwrap();
        if let egui::Event::Key { pressed, .. } = &mut release {
            *pressed = false;
        }
        key(&s.state, "Cmd+V");
        s.frame(egui::RawInput { events: if same_frame { vec![release.clone()] } else { vec![] }, ..Default::default() });
        if !same_frame {
            s.frame(egui::RawInput { events: vec![release], ..Default::default() });
        }
        assert_eq!(deck_text(&s.app).matches("pasted words").count(), 1, "same frame: {same_frame}");
    }
    // Edit › Paste clicked does the same.
    let mut s = setup();
    s.app.services.clipboard_text = Some(Box::new(|| Some("clicked paste".into())));
    click(&s.state, "edit.paste");
    s.frame(Default::default());
    assert_eq!(deck_text(&s.app).matches("clicked paste").count(), 1);
}

/// Edit › Copy on a selected shape puts its text on the system clipboard, as ⌘C does.
#[test]
fn copy_click_puts_the_selection_on_the_clipboard() {
    let mut s = setup();
    let title = s.app.session.active().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.app.run("text.set", json!({"id": title, "text": "copy me"})).unwrap();
    s.app.run("edit.select", json!({"ids": [title]})).unwrap();
    click(&s.state, "edit.copy");
    let output = s.frame(Default::default());
    assert!(
        output.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t.contains("copy me"))),
        "{:?}",
        output.platform_output.commands
    );
}

/// Quit asks about unsaved changes as the window's close button does; Cancel keeps the app. With
/// a window host, Quit is the host's: every window closes in turn.
#[test]
fn quit_asks_about_unsaved_changes_and_the_host_takes_it() {
    let mut s = setup();
    s.app.run("slide.new", json!({})).unwrap();
    click(&s.state, QUIT);
    s.frame(Default::default());
    assert_eq!(s.app.dialog.as_ref().map(|d| d.id.as_str()), Some("quit"));
    assert!(!s.app.quit_confirmed);
    s.app.dialog = None;
    key(&s.state, "Cmd+Q");
    s.frame(Default::default());
    assert_eq!(s.app.dialog.as_ref().map(|d| d.id.as_str()), Some("quit"), "the key equivalent asks too");
    assert!(!s.app.quit_confirmed);

    let mut h = setup();
    h.app.session.windows.host = true;
    click(&h.state, QUIT);
    h.frame(Default::default());
    assert!(!h.app.quit_confirmed);
    assert!(matches!(h.app.window_requests.as_slice(), [WindowRequest::Quit]), "{:?}", h.app.window_requests);
}

/// A command chosen from the menu returns from the File tab to the slide, as the ribbon does;
/// the app's own pages don't.
#[test]
fn document_actions_leave_the_file_tab() {
    let mut s = setup();
    s.app.open_backstage(crate::backstage::Page::Info);
    assert!(s.app.backstage_open());
    click(&s.state, "app.preferences");
    s.frame(Default::default());
    assert!(s.app.backstage_open(), "Preferences is a dialog over the File tab");
    s.app.dialog = None;
    click(&s.state, "view.ruler");
    s.frame(Default::default());
    assert!(!s.app.backstage_open());
    assert!(s.app.ui.ruler);
}

/// A focused text field takes Edit › Copy, Cut, Paste and Select All itself; the slide is not
/// touched.
#[test]
fn text_field_edits_never_touch_the_slide() {
    let mut s = setup();
    s.app.services.clipboard_text = Some(Box::new(|| Some("field".into())));
    let before = deck_text(&s.app);
    let mut field = String::new();
    {
        // Whole seconds apart: egui's text fields record an undo point once the text has been
        // still for a while.
        let mut draw = |s: &mut Setup| {
            s.time += 1.0;
            let mut raw = egui::RawInput { time: Some(s.time), ..Default::default() };
            let Setup { app, ctx, menu, .. } = s;
            raw_input(menu, app, &mut raw);
            app.raw_input_hook(&mut raw);
            let mut output = ctx.run_ui(raw, |ui| {
                run(menu, app, ui.ctx());
                ui.add(egui::TextEdit::singleline(&mut field).id(egui::Id::new("field"))).request_focus();
                sync(menu, app, ui.ctx());
            });
            output.textures_delta.clear();
            output
        };
        draw(&mut s);
        draw(&mut s);
        key(&s.state, "Cmd+V");
        draw(&mut s);
        click(&s.state, "edit.selectAll");
        draw(&mut s);
        click(&s.state, "edit.copy");
        let output = draw(&mut s);
        assert!(output.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "field")));
        click(&s.state, "edit.cut");
        draw(&mut s);
        click(&s.state, "edit.undo");
        draw(&mut s);
    }
    assert_eq!(field, "field");
    assert_eq!(deck_text(&s.app), before);
}

/// File › Open Recent lists the recent files with their path; a row whose file is gone does
/// nothing but say so.
#[test]
fn recent_rows_carry_the_path() {
    let mut s = setup();
    s.app.ui.recent = vec!["/synthetic/one & two.pptx".into()];
    let bar = s.bar();
    let row = bar.find("recent:/synthetic/one & two.pptx").unwrap();
    assert_eq!((row.command.as_str(), &row.params), ("file.open", &json!({"path": "/synthetic/one & two.pptx"})));
    click(&s.state, &row.id);
    s.frame(Default::default());
    assert_eq!(slides(&s.app), 1, "nothing opened");
    assert!(s.app.status.as_ref().is_some_and(|(m, _)| m.contains("one & two")), "{:?}", s.app.status);
}

/// Minimize minimizes and Zoom zooms the window the menu belongs to, in its own frame; Hide goes
/// to the system.
#[test]
fn minimize_zoom_and_hide_reach_the_window_and_the_system() {
    let mut s = setup();
    click(&s.state, MINIMIZE);
    let out = s.frame(Default::default());
    let root = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
    assert!(root.contains(&egui::ViewportCommand::Minimized(true)), "{root:?}");
    click(&s.state, ZOOM);
    let out = s.frame(Default::default());
    let root = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
    assert!(root.contains(&egui::ViewportCommand::Maximized(true)), "{root:?}");
    click(&s.state, HIDE);
    key(&s.state, "Cmd+Alt+H");
    s.frame(Default::default());
    assert_eq!(s.state.borrow().system, [HIDE, HIDE_OTHERS]);
}

/// With a slide show running, the menu's document commands wait; Quit still works.
#[test]
fn a_running_show_disables_document_commands() {
    let mut s = setup();
    s.app.start_show(0, false);
    let bar = s.bar();
    assert!(!bar.find("view.ruler").unwrap().enabled);
    assert!(!bar.find("slide.new").unwrap().enabled);
    assert!(bar.find(QUIT).unwrap().enabled);
}
