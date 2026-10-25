//! The native menu bar, alongside (never replacing) the ribbon: on macOS the DeckCraft, File,
//! Edit, View, Insert, Format, Arrange, Tools, Slide Show, Window and Help menus PowerPoint for
//! Mac has at the top of the screen, each item running the same command the ribbon does.
//!
//! The model and routing here are platform-free; the desktop app supplies the AppKit backend
//! (`apps/deckcraft/src/mac_menu.rs`, through `muda`). Adapted from WordCraft's `native_menu.rs`
//! (storytold/wordcraft d2d407bc), itself from storytold/wordcraft#985 by YTwsy after PhotoCraft's
//! native-menu design at 67e4b8a499b55ef728b1da700005382c45e4677d; see NOTICE.
//!
//! The menus are [`crate::menus::menu_tree`] (the one list the command palette and the parity
//! catalogue share), with the app menu, the Window menu's Minimize, Zoom and Bring All to Front
//! and File › Open Recent added here. Labels come from the commands in the interface language,
//! key equivalents from the commands' shortcuts, enabled state from the ribbon's rule and
//! checkmarks from the view and formatting state the ribbon reads.
//!
//! The menu is one for the process while the app has a window per presentation
//! ([`crate::windows::Host`]): it shows the state of the window in use, its items run in that
//! window, and the Window menu's list of windows is AppKit's own (the host registers the menu as
//! the windows menu).

use std::hash::{DefaultHasher, Hash, Hasher};

use egui::{Context, Key, Modifiers};
use serde_json::{Value, json};

use crate::{SlideApp, ViewMode, dialogs, i18n, menus, windows::WindowRequest};

/// Row ids of the app's own entries (not commands of the registry).
pub const QUIT: &str = "app.quit";
pub const HIDE: &str = "app.hide";
pub const HIDE_OTHERS: &str = "app.hideOthers";
pub const SHOW_ALL: &str = "app.showAll";
pub const MINIMIZE: &str = "window.minimize";
pub const ZOOM: &str = "window.zoom";

use crate::APP_NAME;

#[derive(Clone, Debug, PartialEq)]
pub struct MenuBar {
    pub menus: Vec<Menu>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: String,
    pub role: Role,
    pub children: Vec<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    App,
    Normal,
    Window,
    Help,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Item(Item),
    Separator,
    Submenu { label: String, children: Vec<Node> },
    Standard(Standard),
}

/// Items AppKit provides itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standard {
    Services,
    BringAllToFront,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Stable row id: the command's id, `dialog:<name>`, `recent:<path>`, or one of the app's own.
    pub id: String,
    pub command: String,
    pub params: Value,
    pub label: String,
    pub shortcut: Option<Chord>,
    pub enabled: bool,
    pub checked: Option<bool>,
}

impl MenuBar {
    pub fn items(&self) -> Vec<&Item> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Item>) {
            for node in nodes {
                match node {
                    Node::Item(it) => out.push(it),
                    Node::Submenu { children, .. } => walk(children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for menu in &self.menus {
            walk(&menu.children, &mut out);
        }
        out
    }

    pub fn find(&self, id: &str) -> Option<&Item> {
        self.items().into_iter().find(|it| it.id == id)
    }
}

/// A key equivalent: the native menu may take Command/Control shortcuts and function keys, never
/// ordinary typing (including Option characters). Parsed from the commands' shortcut spelling
/// (`Cmd+Shift+S`, `Cmd+`` `, `Shift+F9`, `F5`; `Cmd` is Command here).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The key as the shortcuts spell it: a capital letter, a digit, `F5`, `Enter`, `Space`,
    /// `Left`, or a punctuation character (`=`, `-`, `[`, `` ` ``…).
    pub key: String,
}

impl Chord {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (mods, key) = if let Some(m) = s.strip_suffix("++") { (m, "+") } else { s.rsplit_once('+').unwrap_or(("", s)) };
        if key.is_empty() {
            return None;
        }
        let parts: Vec<&str> = mods.split('+').filter(|p| !p.is_empty()).collect();
        let has = |m: &str| parts.iter().any(|p| p.eq_ignore_ascii_case(m));
        let key = match key {
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphabetic()) => k.to_ascii_uppercase(),
            "Return" => "Enter".into(),
            "Esc" => "Escape".into(),
            "ArrowLeft" => "Left".into(),
            "ArrowRight" => "Right".into(),
            "ArrowUp" => "Up".into(),
            "ArrowDown" => "Down".into(),
            k => k.to_string(),
        };
        let chord =
            Self { cmd: has("Cmd") || has("Mod") || has("Command"), ctrl: has("Ctrl"), alt: has("Alt") || has("Option"), shift: has("Shift"), key };
        chord.key_event()?;
        Some(chord)
    }

    /// Whether the chord may be a native key equivalent: a Command or Control shortcut, or a
    /// function key. Bare and Option-only keys are typing and stay with the editor.
    pub fn native_ok(&self) -> bool {
        self.key_event().is_some()
            && (self.cmd || self.ctrl || self.key.strip_prefix('F').and_then(|s| s.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n)))
    }

    pub fn modifiers(&self) -> Modifiers {
        Modifiers { command: self.cmd, mac_cmd: self.cmd, ctrl: self.ctrl, alt: self.alt, shift: self.shift }
    }

    /// The key press egui would have seen.
    pub fn key_event(&self) -> Option<egui::Event> {
        let key = match self.key.as_str() {
            "+" => Key::Plus,
            "=" => Key::Equals,
            "-" => Key::Minus,
            "[" => Key::OpenBracket,
            "]" => Key::CloseBracket,
            "\\" => Key::Backslash,
            "/" => Key::Slash,
            "'" => Key::Quote,
            ";" => Key::Semicolon,
            "," => Key::Comma,
            "." => Key::Period,
            "`" => Key::Backtick,
            "Left" => Key::ArrowLeft,
            "Right" => Key::ArrowRight,
            "Up" => Key::ArrowUp,
            "Down" => Key::ArrowDown,
            "0" => Key::Num0,
            "1" => Key::Num1,
            "2" => Key::Num2,
            "3" => Key::Num3,
            "4" => Key::Num4,
            "5" => Key::Num5,
            "6" => Key::Num6,
            "7" => Key::Num7,
            "8" => Key::Num8,
            "9" => Key::Num9,
            k => Key::from_name(k)?,
        };
        Some(egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: self.modifiers() })
    }
}

/// The editing commands a focused text field takes for itself.
fn editing(id: &str) -> bool {
    matches!(id, "edit.undo" | "edit.redo" | "edit.cut" | "edit.copy" | "edit.paste" | "edit.selectAll")
}

/// Rows of the app's own, not commands of the registry.
fn app_row(id: &str) -> bool {
    matches!(id, QUIT | HIDE | HIDE_OTHERS | SHOW_ALL | MINIMIZE | ZOOM)
}

/// A dialog, the command palette or a running slide show has the window: the menu's document
/// commands wait.
fn modal(app: &SlideApp) -> bool {
    app.dialog.as_ref().is_some_and(dialogs::Dialog::modal) || app.palette.is_some() || app.show.is_some()
}

/// A native key-down can share a frame with the command that opened a dialog: with the menu bar
/// installed, the keyboard dispatcher checks again during dispatch, after the earlier command has
/// actually run.
pub(crate) fn blocks_document(app: &SlideApp) -> bool {
    app.menu_bar_installed && modal(app)
}

/// The shortcut a command answers to, as the keyboard dispatcher knows it: the command's own
/// (engine specs and UI commands), or one the UI handles itself ([`menus::ui_shortcut`]).
pub fn shortcut_of(id: &str) -> Option<&'static str> {
    deckcraft_engine::find_command(id).and_then(|c| c.shortcut).or_else(|| crate::UI_COMMANDS.iter().find(|c| c.0 == id).and_then(|c| c.2)).or(
        match id {
            "app.openDialog" => Some("Cmd+O"),
            "app.saveAsDialog" => Some("Cmd+Shift+S"),
            "view.fit" => Some("Cmd+0"),
            _ => None,
        },
    )
}

/// Every shortcut a command claims, so the app's own rows (Hide, Minimize, Quit) take a Mac key
/// only when it is free: Replace keeps ⌘H, so Hide takes ⌃⌘H.
fn taken(chord: &Chord) -> bool {
    let engine = deckcraft_engine::command_specs().iter().filter_map(|c| c.shortcut);
    let ui = crate::UI_COMMANDS.iter().filter_map(|c| c.2);
    let own = ["Cmd+O", "Cmd+Shift+S", "Cmd+0", "Cmd+S", "Cmd+K", "Cmd+Alt+V"];
    engine.chain(ui).chain(own).filter_map(Chord::parse).any(|c| c == *chord)
}

fn free_shortcut(candidates: &[&str]) -> Option<Chord> {
    candidates.iter().filter_map(|s| Chord::parse(s)).find(|c| !taken(c))
}

/// Whether the UI runs `id` now: the ribbon's own rule for commands, a presentation for the UI
/// commands that act on one; the app's rows always, Quit unless a dialog is open; a focused text
/// field keeps its editing items.
fn allowed(app: &SlideApp, ctx: &Context, id: &str) -> bool {
    if matches!(id, HIDE | HIDE_OTHERS | SHOW_ALL | MINIMIZE | ZOOM) {
        return true;
    }
    if id == QUIT {
        return !app.dialog.as_ref().is_some_and(dialogs::Dialog::modal);
    }
    if editing(id) && ctx.text_edit_focused() {
        return true;
    }
    if modal(app) {
        return false;
    }
    let has_doc = app.session.active().is_some();
    if let Some(name) = id.strip_prefix("dialog:") {
        return has_doc || name == "autocorrect";
    }
    if id.starts_with("recent:") {
        return true;
    }
    if deckcraft_engine::find_command(id).is_some() {
        return crate::ribbon::enabled(app, id);
    }
    match id {
        "app.closePresentation"
        | "app.saveAsDialog"
        | "app.exportDialog"
        | "app.print"
        | "app.find"
        | "app.replace"
        | "app.pasteSpecial"
        | "show.start"
        | "show.presenter"
        | "review.thesaurusPane"
        | "view.newWindow" => has_doc,
        id if id.starts_with("view.") => has_doc || id == "view.dark",
        _ => crate::ribbon::enabled(app, id),
    }
}

/// The checkmark a row shows, from the state the ribbon reads: the view, the master views, the
/// rulers, gridlines and guides, bold, italic and underline at the selection.
fn checked(app: &SlideApp, id: &str, formatting: &Value) -> Option<bool> {
    let target = app.session.active().map(|d| d.selection.target);
    let slides = target.is_none_or(|t| t == deckcraft_engine::Target::Slides);
    let flag = |key: &str| Some(formatting.get(key).and_then(Value::as_bool).unwrap_or(false));
    match id {
        "view.normal" => Some(app.ui.view == ViewMode::Normal && slides),
        "view.sorter" => Some(app.ui.view == ViewMode::Sorter),
        "view.notesPage" => Some(app.ui.view == ViewMode::NotesPage),
        "view.outline" => Some(app.ui.view == ViewMode::Outline),
        "view.reading" => Some(app.ui.view == ViewMode::Reading),
        "view.slideMaster" => Some(matches!(target, Some(deckcraft_engine::Target::Master { .. } | deckcraft_engine::Target::Layout { .. }))),
        "view.handoutMaster" => Some(target == Some(deckcraft_engine::Target::Handout)),
        "view.notesMaster" => Some(target == Some(deckcraft_engine::Target::NotesMaster)),
        "view.ruler" => Some(app.ui.ruler),
        "view.gridlines" => Some(app.ui.gridlines),
        "view.guides" => Some(app.ui.guides),
        "format.bold" => flag("bold"),
        "format.italic" => flag("italic"),
        "format.underline" => flag("underline"),
        _ => None,
    }
}

/// Running a command returns to the presentation from the File tab, as choosing it on the ribbon
/// does; the app's pages (About, Preferences, the palette, Home) don't.
fn shows_document(id: &str) -> bool {
    !matches!(id, "app.about" | "app.preferences" | "app.palette" | "app.links" | "app.home" | "app.backstage" | "app.openDialog") && !app_row(id)
}

/// The menu bar as PowerPoint for Mac lays it out: DeckCraft, then [`menus::menu_tree`]'s menus
/// (File, Edit, View, Insert, Format, Arrange, Tools, Slide Show, Window, Help). Labels are the
/// commands' own, translated; shortcuts the commands'; checkmarks read the state the ribbon reads.
pub fn menu_bar(app: &mut SlideApp, ctx: &Context) -> MenuBar {
    // The same selection state as the ribbon, so a checkmark never becomes a second source of
    // truth.
    let formatting = if app.session.active().is_some() { crate::ribbon::fmt_state(app) } else { Value::Null };
    let app = &*app;
    let named = |s: &str| i18n::fmt(i18n::t(s), &[("app", APP_NAME)]);
    let item = |id: &str, label: &str| Item {
        id: id.into(),
        command: id.into(),
        params: json!({}),
        label: menus::menu_label(id, label).to_string(),
        shortcut: shortcut_of(id).and_then(Chord::parse).filter(Chord::native_ok),
        enabled: allowed(app, ctx, id),
        checked: checked(app, id, &formatting),
    };
    let own = |id: &str, label: String, candidates: &[&str]| {
        Node::Item(Item {
            id: id.into(),
            command: id.into(),
            params: json!({}),
            label,
            shortcut: free_shortcut(candidates),
            enabled: allowed(app, ctx, id),
            checked: None,
        })
    };
    let app_menu = Menu {
        title: APP_NAME.into(),
        role: Role::App,
        children: vec![
            Node::Item(item("app.about", "About DeckCraft")),
            Node::Separator,
            Node::Item(item("app.preferences", "Preferences…")),
            Node::Separator,
            Node::Standard(Standard::Services),
            Node::Separator,
            own(HIDE, named("Hide {app}"), &["Cmd+H", "Cmd+Ctrl+H"]),
            own(HIDE_OTHERS, i18n::t("Hide Others").to_string(), &["Cmd+Alt+H"]),
            own(SHOW_ALL, i18n::t("Show All").to_string(), &[]),
            Node::Separator,
            // Quit asks about unsaved changes as the window's close button does (every window, with
            // a window host) and never terminates through AppKit's own Quit, which would ask nothing.
            own(QUIT, named("Quit {app}"), &["Cmd+Q"]),
        ],
    };
    let mut bar = MenuBar { menus: vec![app_menu] };
    for (title, rows) in menus::menu_tree() {
        let role = match title {
            "Window" => Role::Window,
            "Help" => Role::Help,
            _ => Role::Normal,
        };
        let mut children = Vec::new();
        if role == Role::Window {
            // Minimize and Zoom are the app's own rows, not AppKit's `performMiniaturize:` and
            // `performZoom:` ones: macOS puts its tiling rows right after a `performZoom:` row,
            // where PowerPoint has them after Bring All to Front.
            children.push(own(MINIMIZE, i18n::t("Minimize").to_string(), &["Cmd+M", "Cmd+Ctrl+M"]));
            children.push(own(ZOOM, i18n::t("Zoom").to_string(), &[]));
            children.push(Node::Separator);
        }
        for (label, id) in rows {
            if id.is_empty() {
                children.push(Node::Separator);
                continue;
            }
            // About sits in the app menu on the Mac.
            if role == Role::Help && id == "app.about" {
                continue;
            }
            if let Some(name) = id.strip_prefix("dialog:") {
                let mut it = item(id, label);
                it.command = "app.dialog".into();
                it.params = json!({"id": name});
                children.push(Node::Item(it));
                continue;
            }
            children.push(Node::Item(item(id, label)));
            if id == "app.openDialog" {
                let recent = app
                    .ui
                    .recent
                    .iter()
                    .map(|path| {
                        let mut it = item("file.open", "Open…");
                        it.id = format!("recent:{path}");
                        it.label = path.clone();
                        it.params = json!({"path": path});
                        it.shortcut = None;
                        it.enabled = allowed(app, ctx, &it.id);
                        Node::Item(it)
                    })
                    .collect();
                children.push(Node::Submenu { label: i18n::t("Open Recent").to_string(), children: recent });
            }
        }
        if role == Role::Window {
            children.push(Node::Separator);
            children.push(Node::Standard(Standard::BringAllToFront));
        }
        // Help items that aren't commands yet are dropped rather than shown dead.
        children.retain(|n| !matches!(n, Node::Item(it) if it.command != "app.dialog" && !it.id.starts_with("recent:") && !app_row(&it.id) && !menus::is_known_command(&it.command)));
        bar.menus.push(Menu { title: i18n::tc("menu", title).to_string(), role, children });
    }
    bar
}

/// Only the shape, labels of submenus, and key equivalents require a rebuild. A checkmark,
/// enabled state or item label changes in place. Check items never change kind.
pub fn structure_key(bar: &MenuBar) -> u64 {
    fn walk(nodes: &[Node], h: &mut impl Hasher) {
        for node in nodes {
            std::mem::discriminant(node).hash(h);
            match node {
                Node::Item(it) => (&it.id, &it.shortcut, it.checked.is_some()).hash(h),
                Node::Submenu { label, children } => {
                    label.hash(h);
                    walk(children, h);
                }
                Node::Standard(s) => s.hash(h),
                Node::Separator => {}
            }
        }
    }
    let mut h = DefaultHasher::new();
    for m in &bar.menus {
        (&m.title, m.role).hash(&mut h);
        walk(&m.children, &mut h);
    }
    h.finish()
}

#[derive(Clone, Debug)]
pub enum Event {
    Click(String),
    Key(Chord),
}

/// The platform's menu: shown from the model, reporting what was chosen.
pub trait Backend {
    fn sync(&mut self, bar: &MenuBar);
    fn drain(&mut self) -> Vec<Event>;
    fn system_action(&mut self, id: &str);
}

/// The menu bar: its backend, the events it reported that no frame has taken yet, and the
/// clicks waiting to run.
pub struct NativeMenu {
    backend: Box<dyn Backend>,
    ctx: Context,
    pending: Vec<Event>,
    clicks: Vec<String>,
}

impl NativeMenu {
    pub fn new(backend: Box<dyn Backend>, ctx: Context) -> Self {
        Self { backend, ctx, pending: Vec::new(), clicks: Vec::new() }
    }

    /// Take the backend's events into the queue; true when something waits to be handled by
    /// the window in use (its frame takes them in [`raw_input`]).
    pub fn pull(&mut self) -> bool {
        self.pending.extend(self.backend.drain());
        !self.pending.is_empty() || !self.clicks.is_empty()
    }
}

/// The system clipboard's text, for a paste the menu asked for (egui-winit reads it for a real
/// ⌘V; a key equivalent AppKit took never reached winit).
fn clipboard_text(app: &mut SlideApp) -> String {
    app.services.clipboard_text.as_mut().and_then(|f| f()).unwrap_or_default()
}

/// A menu click in an egui text field is an egui editing event, not a slide command.
fn text_event(app: &mut SlideApp, id: &str) -> Option<egui::Event> {
    match id {
        "edit.copy" => Some(egui::Event::Copy),
        "edit.cut" => Some(egui::Event::Cut),
        "edit.paste" => Some(egui::Event::Paste(clipboard_text(app))),
        "edit.selectAll" => Chord::parse("Cmd+A")?.key_event(),
        "edit.undo" => Chord::parse("Cmd+Z")?.key_event(),
        "edit.redo" => Chord::parse("Cmd+Shift+Z")?.key_event(),
        _ => None,
    }
}

/// Put a swallowed key-down before its real key-up (even when both arrived in this frame).
/// Never invent a release: winit already delivers it, including for Command-V.
fn insert_press(raw: &mut egui::RawInput, event: egui::Event, key: Key) {
    let at = raw.events.iter().position(|e| matches!(e, egui::Event::Key { key: k, pressed: false, .. } if *k == key)).unwrap_or(raw.events.len());
    raw.events.insert(at, event);
}

/// Before the window in use begins its frame: the menu's key equivalents become the key presses
/// egui would have seen (⌘C, ⌘X and ⌘V the Copy, Cut and Paste events egui-winit makes of them,
/// which a text field and the slide both take), clicks queue for [`run`].
pub fn raw_input(menu: &mut NativeMenu, app: &mut SlideApp, raw: &mut egui::RawInput) {
    menu.pull();
    let ctx = menu.ctx.clone();
    for event in std::mem::take(&mut menu.pending) {
        match event {
            Event::Click(id) => {
                let bar = menu_bar(app, &ctx);
                let Some(it) = bar.find(&id).filter(|it| it.enabled).cloned() else { continue };
                // Copy, Cut and Paste run as the keys do: the text field with the focus, else the
                // slide (a picture when the clipboard holds no text, else the internal clipboard).
                if matches!(it.command.as_str(), "edit.copy" | "edit.cut" | "edit.paste") || (editing(&it.command) && ctx.text_edit_focused()) {
                    if let Some(event) = text_event(app, &it.command) {
                        raw.events.push(event);
                    }
                } else {
                    menu.clicks.push(id);
                }
            }
            Event::Key(chord) => {
                let Some(press @ egui::Event::Key { key, .. }) = chord.key_event() else { continue };
                let plain = chord.cmd && !chord.ctrl && !chord.alt && !chord.shift;
                let text = match key {
                    Key::C if plain => Some(egui::Event::Copy),
                    Key::X if plain => Some(egui::Event::Cut),
                    Key::V if plain => Some(egui::Event::Paste(clipboard_text(app))),
                    _ => None,
                };
                if let Some(event) = text {
                    insert_press(raw, event, key);
                    continue;
                }
                // The app's own rows (Quit, Hide, Minimize, Preferences) have keys no command has:
                // run them as a click. Everything else is the key press egui would have seen, which
                // the UI's own dispatch takes (its shortcuts, the engine's keys, a text field's).
                let bar = menu_bar(app, &ctx);
                if let Some(it) = bar.items().into_iter().find(|it| app_row(&it.id) && it.shortcut.as_ref() == Some(&chord)) {
                    if it.enabled {
                        menu.clicks.push(it.id.clone());
                    }
                    continue;
                }
                insert_press(raw, press, key);
            }
        }
    }
}

/// Called after egui begins the frame, so clipboard output and viewport requests survive: the
/// queued clicks run in the window in use.
pub fn run(menu: &mut NativeMenu, app: &mut SlideApp, ctx: &Context) {
    for id in std::mem::take(&mut menu.clicks) {
        // Recheck every queued click: a prior click can have opened a modal in this very frame.
        let bar = menu_bar(app, ctx);
        let Some(it) = bar.find(&id).filter(|it| it.enabled).cloned() else { continue };
        match id.as_str() {
            HIDE | HIDE_OTHERS | SHOW_ALL => menu.backend.system_action(&id),
            MINIMIZE => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            // Zoom toggles, as the Mac's does (winit zooms the window on macOS).
            ZOOM => {
                let zoomed = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!zoomed));
            }
            // With a window host every window closes in turn, each asking about its changes;
            // alone, the window asks and closes, as its close button does.
            QUIT if app.session.windows.host => app.window_requests.push(WindowRequest::Quit),
            QUIT => {
                if app.close_requested() {
                    app.quit_confirmed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            // Spelling opens its dialog, as the ribbon and the Quick Access Toolbar open it (the
            // command itself only lists the misspellings).
            "review.spelling" => {
                app.close_backstage();
                app.dialog = Some(dialogs::Dialog::new("spelling"));
            }
            _ => {
                if shows_document(&it.command) {
                    app.close_backstage();
                }
                // `run` shows an error on the status line itself.
                let ran = app.run(&it.command, it.params.clone()).is_ok();
                if ran && shows_document(&it.command) {
                    app.close_backstage();
                }
            }
        }
    }
}

/// After the frame of the window in use: the menu shows that window's state.
pub fn sync(menu: &mut NativeMenu, app: &mut SlideApp, ctx: &Context) {
    let bar = menu_bar(app, ctx);
    menu.backend.sync(&bar);
}

#[cfg(test)]
#[path = "native_menu_tests.rs"]
mod tests;
