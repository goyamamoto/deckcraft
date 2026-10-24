//! Presentation windows: one window per open presentation, as PowerPoint has them.
//!
//! The desktop app installs a [`Host`] that owns every window's [`SlideApp`] (its session, canvas
//! and interface state) and the window it lives in: the first window is eframe's root viewport,
//! every other one a deferred child viewport the root declares each pass
//! (`egui::Context::show_viewport_deferred`). Each viewport's frame runs its own `SlideApp` through
//! [`Host::logic`] and [`Host::ui`]; the host lives in a thread-local because a child viewport's
//! callback can borrow nothing of the eframe app.
//!
//! What follows PowerPoint: File › New and File › Open leave the current window alone and put the
//! presentation in a new window, except that a window showing nothing, or an untouched new
//! presentation, takes it itself (as Office replaces its untouched Presentation1); a file already
//! open in another window comes to the front instead of opening twice; File › Close closes the
//! presentation's window when another is open, and the last window stays and shows the start
//! screen; the window's close button asks about unsaved changes and closes only that window;
//! quitting asks about each presentation in turn, and Cancel in any prompt keeps the app open.
//! View › Window: New Window opens a second window on the same presentation (`Presentation1:1`
//! and `Presentation1:2`: one presentation, one undo history, each window with its own slide and
//! selection, [`Session::swap_view`]), Arrange All tiles every window across the screen's work
//! area, Cascade stacks them a step apart, Switch Windows (and Next Window, ⌘`) brings one forward.
//!
//! A session holds one presentation under a host: one added to a window's session (New, Open, a
//! drop, a recovered file) is taken out again at once and handed to the host for a window of its
//! own ([`SlideApp::settle_documents`], [`WindowRequest::Adopt`]).
//!
//! Things every window shares: egui's one `Memory` (so fixed widget ids go through
//! [`crate::wid`]), the interface fonts (each window's borrowed faces are pooled before its frame),
//! the appearance, interface size and language, the recent list and the other settings, which the
//! window the user last changed them in hands to the rest ([`SlideApp::adopt_settings`]).
//!
//! Only the desktop app has a host: the web app, the CLI and the tests drive one `SlideApp`, and
//! `Session::windows` tells the engine so (`host: false`), so the window commands are disabled.

use std::cell::RefCell;
use std::sync::mpsc::Receiver;

use deckcraft_engine::{DocState, ViewSlot, WindowEntry, Windows};
use egui::{Pos2, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportId, pos2, vec2};
use serde_json::{Value, json};

use crate::native_menu::{self, NativeMenu};
use crate::{ControlRequest, SlideApp};

/// A window's id: stable while it is open, unique within the run, never 0.
pub type WindowId = u64;

/// The smallest window: the desktop app's minimum inner size.
pub const MIN_SIZE: Vec2 = vec2(900.0, 560.0);
/// How far down and right a window opened from another one appears, as macOS cascades windows.
const CASCADE: Vec2 = vec2(22.0, 22.0);
/// Room left on each side of a monitor whose work area isn't known (a taskbar, a panel).
const SYSTEM_BAR: f32 = 48.0;

/// What a window asks the host for, queued by its `SlideApp` and carried out by [`Host`].
#[derive(Debug)]
pub enum WindowRequest {
    /// A presentation that landed in this window's session goes to a new window.
    Adopt(Box<DocState>),
    /// Bring that window to the front (Switch Windows, Next Window, a file already open there).
    Focus(WindowId),
    ArrangeAll,
    Cascade,
    /// Close this window: File › Close with other windows open.
    Close,
    /// Quit the app: every window closes, asking about unsaved changes.
    Quit,
    /// View › New Window: another window on this window's presentation.
    NewWindow,
    /// File › Close on a presentation with several windows: the other windows on it close too.
    CloseDocumentWindows,
}

/// A window and the `SlideApp` it shows.
pub struct Window {
    pub id: WindowId,
    pub viewport: ViewportId,
    pub app: SlideApp,
    /// The child viewport's builder, fixed once it is created (egui only patches changes, so the
    /// same one is declared every pass; the title travels as a command).
    builder: ViewportBuilder,
    /// The window's frame and content rects and monitor as its last frame saw them.
    outer: Option<Rect>,
    inner: Option<Rect>,
    monitor: Option<Vec2>,
    /// Whether the window was on screen at its last frame (not minimized, not covered): eframe
    /// runs no frame of a child viewport that isn't, so such a window answers nothing until it
    /// shows again.
    visible: Option<bool>,
    /// The host frame count when the window last had the focus.
    focused_at: u64,
    /// Being closed: dropped once the root pass stops declaring it.
    closing: bool,
    /// The title last sent to the window (the Dock, Mission Control and the Window menu show it).
    sent_title: Option<String>,
    /// View › New Window: the presentation group this window belongs to, its number in it (the
    /// title's `:2`) and its view of the presentation while another window of the group holds the
    /// session (`Session::swap_view`).
    group: Option<u64>,
    view_number: Option<u32>,
    view: Option<ViewSlot>,
}

/// The windows on one presentation (View › New Window): they share one session, which the one
/// that last drew holds; the others keep their view and a placeholder session until their turn.
struct Group {
    id: u64,
    members: Vec<WindowId>,
    holder: WindowId,
    /// What the others last saw: the presentation's revision, dirty flag and file. A change
    /// repaints them.
    seen: (u64, bool, Option<String>),
    next_number: u32,
}

/// Quitting: the window whose Save Changes prompt the quit waits on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Quit {
    No,
    Asking(WindowId),
}

/// Makes the `SlideApp` for a new window: a blank session with the desktop services.
pub type NewApp = Box<dyn FnMut() -> SlideApp>;

/// The screen's work area in points (the desktop less the menu bar, Dock or taskbar) for the
/// window at that frame rect, where the platform tells.
pub type WorkArea = Box<dyn Fn(Option<Rect>) -> Option<Rect>>;

/// Runs after a window's frame with whether the window has the focus (the macOS input method's
/// view of the focused presentation).
pub type AfterUi = Box<dyn FnMut(&SlideApp, bool)>;

/// The open windows and what they share.
pub struct Host {
    windows: Vec<Window>,
    next_id: WindowId,
    frames: u64,
    active: WindowId,
    groups: Vec<Group>,
    next_group: u64,
    quit: Quit,
    new_app: NewApp,
    template: ViewportBuilder,
    pub work_area: Option<WorkArea>,
    pub after_ui: Option<AfterUi>,
    /// The native menu bar (macOS): one for the process, showing the window in use and running
    /// its items there.
    menu: Option<NativeMenu>,
    control_rx: Option<Receiver<ControlRequest>>,
    /// Every window has closed (or the last one may): the eframe app ends the process.
    exit: bool,
    /// The shared settings as last handed round, to notice a change.
    shared_settings: Option<String>,
    /// Frames left in which the settings are compared, after a frame with input.
    share_pending: u8,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

impl Host {
    /// A host with its first window: `first` in the root viewport, `template` the viewport
    /// builder new windows start from (title, chrome, icon), `new_app` making their apps.
    pub fn new(mut first: SlideApp, mut template: ViewportBuilder, new_app: NewApp) -> Self {
        // Where the first window opens is its own; new windows cascade from the window they come
        // from, or the system places them.
        template.position = None;
        template.maximized = None;
        first.session.windows.host = true;
        let window = Window::new(1, ViewportId::ROOT, first, ViewportBuilder::default());
        Host {
            windows: vec![window],
            next_id: 2,
            frames: 0,
            active: 1,
            groups: Vec::new(),
            next_group: 1,
            quit: Quit::No,
            new_app,
            template,
            work_area: None,
            after_ui: None,
            menu: None,
            control_rx: None,
            exit: false,
            shared_settings: None,
            share_pending: 0,
        }
    }

    /// The control channel's requests (`--control`): routed to the window each one names.
    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        self
    }

    /// The native menu bar the host installed ([`native_menu`]).
    pub fn with_menu(mut self, menu: NativeMenu) -> Self {
        self.menu = Some(menu);
        self
    }

    /// Whether the window `vp` shows is the one in use: the one the menu bar shows and acts on.
    fn is_active_viewport(&self, vp: ViewportId) -> bool {
        self.active().is_some_and(|w| w.viewport == vp)
    }

    /// Install the host for this thread (the main thread), replacing any earlier one.
    pub fn install(host: Host) {
        HOST.with(|h| *h.borrow_mut() = Some(host));
    }

    /// Run `f` on the installed host. `None` without one, or while it is already in use (a
    /// nested call), which never happens on eframe's thread.
    pub fn with<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
        HOST.with(|h| match h.try_borrow_mut() {
            Ok(mut host) => host.as_mut().map(f),
            Err(_) => {
                log::error!("window host borrowed twice");
                None
            }
        })
    }

    /// Take the host out again (the end of the run, or a test).
    pub fn take() -> Option<Host> {
        HOST.with(|h| h.borrow_mut().take())
    }

    /// Whether every window has closed, or the last one was allowed to: the process ends.
    pub fn exit_requested(&self) -> bool {
        self.exit
    }

    /// The windows, in the order they were opened (Switch Windows' order).
    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter().filter(|w| !w.closing)
    }

    /// The window that last had the focus (the first, before any did); `None` once every window
    /// has closed.
    pub fn active(&self) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == self.active && !w.closing).or_else(|| self.windows.iter().find(|w| !w.closing))
    }

    /// The window that last had the focus, to change.
    pub fn active_mut(&mut self) -> Option<&mut Window> {
        let id = self.active()?.id;
        self.get_mut(id)
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// The window shown in that viewport.
    pub fn window_of_viewport_mut(&mut self, vp: ViewportId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.viewport == vp && !w.closing)
    }

    fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    fn index_of_viewport(&self, vp: ViewportId) -> Option<usize> {
        self.windows.iter().position(|w| w.viewport == vp && !w.closing)
    }

    fn open_count(&self) -> usize {
        self.windows.iter().filter(|w| !w.closing).count()
    }

    /// The window whose id, or 1-based number in the Switch Windows list, is `n`.
    pub fn resolve(&self, n: u64) -> Option<WindowId> {
        self.snapshot(0).resolve(n)
    }

    /// What the engine sees of the windows from window `this` ([`deckcraft_engine::Windows`]).
    fn snapshot(&self, this: WindowId) -> Windows {
        let list = self
            .windows
            .iter()
            .filter(|w| !w.closing)
            .map(|w| {
                let app = &self.holder_of(w).app;
                WindowEntry {
                    id: w.id,
                    title: app.title_stem(),
                    path: app.session.active().and_then(|d| d.path.clone()),
                    document: w.group.unwrap_or(0),
                    view: w.view_number,
                }
            })
            .collect();
        Windows { host: true, this, list }
    }

    /// The window whose app holds `w`'s presentation: `w` itself, or the window of its group that
    /// drew last (View › New Window).
    fn holder_of<'a>(&'a self, w: &'a Window) -> &'a Window {
        w.group
            .and_then(|g| self.groups.iter().find(|group| group.id == g))
            .and_then(|group| self.windows.iter().find(|h| h.id == group.holder))
            .unwrap_or(w)
    }

    /// The window at `i` takes its turn with its presentation (View › New Window): the group's
    /// session moves from the window that drew last into this one, each keeping its own view.
    fn take_turn(&mut self, i: usize) {
        let Some(this) = self.windows.get(i) else { return };
        let id = this.id;
        let Some(g) = this.group else { return };
        let Some(gi) = self.groups.iter().position(|group| group.id == g) else { return };
        let holder = self.groups[gi].holder;
        if holder == id {
            return;
        }
        let Some(h) = self.windows.iter().position(|w| w.id == holder) else {
            self.groups[gi].holder = id;
            return;
        };
        let (a, b) = if h < i { (h, i) } else { (i, h) };
        let (first, rest) = self.windows.split_at_mut(b);
        let (Some(low), Some(high)) = (first.get_mut(a), rest.first_mut()) else { return };
        let (holder_w, this_w) = if h < i { (low, high) } else { (high, low) };
        holder_w.app.swap_document_state(&mut this_w.app);
        // The session carries the holder's view: keep it for the holder, give this window its own.
        if let Some(mut view) = this_w.view.take() {
            this_w.app.session.swap_view(&mut view);
            holder_w.view = Some(view);
        }
        this_w.app.view_number = this_w.view_number;
        holder_w.app.view_number = holder_w.view_number;
        self.groups[gi].holder = id;
    }

    /// After a window's frame: the others on its presentation see what changed (View › New Window).
    fn sync_group(&mut self, i: usize, ctx: &egui::Context) {
        let Some(this) = self.windows.get(i) else { return };
        let Some(g) = this.group else { return };
        let Some(group) = self.groups.iter_mut().find(|group| group.id == g) else { return };
        let now = doc_mark(&this.app);
        if group.seen == now {
            return;
        }
        group.seen = now;
        let id = this.id;
        for w in self.windows.iter().filter(|w| w.id != id && w.group == Some(g) && !w.closing) {
            ctx.request_repaint_of(w.viewport);
        }
    }

    /// View › New Window: another window on the window's presentation, opening a step from that
    /// window; the two (or more) share the presentation, each with its own slide and selection,
    /// and their titles are numbered (`Presentation1:1`, `Presentation1:2`), as PowerPoint's.
    fn new_window_on(&mut self, origin: WindowId, ctx: &egui::Context) {
        let Some(oi) = self.index(origin) else { return };
        self.take_turn(oi);
        if self.windows[oi].app.session.active().is_none() {
            return;
        }
        let view = self.windows[oi].app.session.clone_view();
        let g = match self.windows[oi].group {
            Some(g) => g,
            None => {
                let g = self.next_group;
                self.next_group = self.next_group.wrapping_add(1).max(1);
                let seen = doc_mark(&self.windows[oi].app);
                self.groups.push(Group { id: g, members: vec![origin], holder: origin, seen, next_number: 2 });
                self.windows[oi].group = Some(g);
                self.windows[oi].view_number = Some(1);
                self.windows[oi].app.view_number = Some(1);
                g
            }
        };
        let id = self.spawn(origin, None, ctx);
        let number = self.groups.iter_mut().find(|group| group.id == g).map(|group| {
            group.members.push(id);
            let n = group.next_number;
            group.next_number += 1;
            n
        });
        if let Some(w) = self.get_mut(id) {
            w.group = Some(g);
            w.view = Some(view);
            w.view_number = number;
            w.app.view_number = number;
        }
    }

    /// A window leaves its presentation group before closing: the session goes to another member
    /// when it held it, and a group of one is no group (its window's title loses its number).
    fn leave_group(&mut self, id: WindowId) {
        let Some(i) = self.index(id) else { return };
        let Some(g) = self.windows[i].group.take() else { return };
        let Some(gi) = self.groups.iter().position(|group| group.id == g) else { return };
        self.groups[gi].members.retain(|m| *m != id);
        let holder = self.groups[gi].holder;
        if holder == id
            && let Some(next) = self.groups[gi].members.first().copied()
            && let Some(ni) = self.index(next)
        {
            let (a, b) = if ni < i { (ni, i) } else { (i, ni) };
            let (first, rest) = self.windows.split_at_mut(b);
            if let (Some(low), Some(high)) = (first.get_mut(a), rest.first_mut()) {
                let (next_w, this_w) = if ni < i { (low, high) } else { (high, low) };
                this_w.app.swap_document_state(&mut next_w.app);
                if let Some(mut view) = next_w.view.take() {
                    next_w.app.session.swap_view(&mut view);
                }
            }
            self.groups[gi].holder = next;
        }
        self.windows[i].view = None;
        self.windows[i].view_number = None;
        self.windows[i].app.view_number = None;
        if self.groups[gi].members.len() <= 1 {
            for m in std::mem::take(&mut self.groups[gi].members) {
                if let Some(w) = self.get_mut(m) {
                    w.group = None;
                    w.view = None;
                    w.view_number = None;
                    w.app.view_number = None;
                }
            }
            self.groups.remove(gi);
        }
    }

    /// The window's frame, before it is drawn: the engine's view of the windows, the pooled
    /// fonts, the app's own logic, then the window's geometry, focus, title, close button and
    /// requests.
    pub fn logic(&mut self, vp: ViewportId, ctx: &egui::Context) {
        if self.exit {
            return;
        }
        let Some(i) = self.index_of_viewport(vp) else { return };
        self.take_turn(i);
        self.frames += 1;
        let id = self.windows[i].id;
        crate::set_window_salt(id);
        if vp == ViewportId::ROOT {
            self.drain_control(ctx);
            crate::set_window_salt(id);
            self.quit_tick(ctx);
            self.pass_wake_ups_on(ctx);
            // A menu event wakes the root viewport; the window in use takes it in its frame.
            if let Some(menu) = &mut self.menu
                && menu.pull()
                && let Some(w) = self.active()
                && w.viewport != ViewportId::ROOT
            {
                ctx.request_repaint_of(w.viewport);
            }
        }
        self.pass_screenshots_on(vp, ctx);
        self.pool_fonts();
        self.sync_untitled(i);
        let snapshot = self.snapshot(id);
        let frames = self.frames;
        let active = self.is_active_viewport(vp);
        let Host { menu, windows, .. } = self;
        let w = &mut windows[i];
        w.app.session.windows = snapshot;
        w.app.view_number = w.view_number;
        w.app.menu_bar_installed = menu.is_some();
        if active && let Some(menu) = menu {
            native_menu::run(menu, &mut w.app, ctx);
        }
        w.app.logic(ctx);
        // Where the window is, for cascading and tiling.
        let info = ctx.input(|i| i.viewport().clone());
        w.outer = info.outer_rect;
        w.inner = info.inner_rect;
        w.monitor = info.monitor_size;
        w.visible = info.visible();
        if info.focused == Some(true) {
            w.focused_at = frames;
            self.active = id;
        }
        let title = w.app.window_title();
        if w.sent_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd_to(vp, ViewportCommand::Title(title.clone()));
            w.sent_title = Some(title);
        }
        // The window's close button, or the presentation's own request to close (the Save Changes
        // prompt answered, `app.quit`): with unsaved changes the app asks first and the close
        // waits (`SlideApp::close_requested`). One of several windows on a presentation closes
        // without asking: the presentation stays open in the others.
        let closing = ctx.input(|i| i.viewport().close_requested());
        let in_group = w.group.is_some_and(|g| self.groups.iter().any(|group| group.id == g && group.members.len() > 1));
        let allowed = w.app.quit_confirmed || (closing && (in_group || w.app.close_requested()));
        if closing && !allowed {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }
        if allowed {
            // The window is gone (or the root now shows another presentation): its frame is over.
            self.close_window(id, ctx);
            if closing && vp == ViewportId::ROOT && !self.exit {
                // eframe ends the app when the root viewport's close isn't cancelled: the root
                // window stays, on the presentation it took over.
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::CancelClose);
            }
            return;
        }
        let requests = std::mem::take(&mut self.windows[i].app.window_requests);
        for req in requests {
            self.handle(id, req, ctx);
        }
        // A change made by a click shows in the settings on the next frame, which the click's
        // repaint brings without input of its own: look for one frame longer than the input.
        if ctx.input(|i| !i.events.is_empty()) {
            self.share_pending = 2;
        }
        if self.share_pending > 0 {
            self.share_pending -= 1;
            self.share_settings(id, ctx);
        }
    }

    /// The window's frame: draw its app, tell the host's hook, and (the root) declare the other
    /// windows so they stay open.
    pub fn ui(&mut self, vp: ViewportId, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.exit {
            return;
        }
        if let Some(i) = self.index_of_viewport(vp) {
            let id = self.windows[i].id;
            crate::set_window_salt(id);
            self.windows[i].app.ui(ui);
            self.sync_group(i, &ctx);
            if self.is_active_viewport(vp) {
                let Host { menu, windows, .. } = self;
                if let Some(menu) = menu {
                    native_menu::sync(menu, &mut windows[i].app, &ctx);
                }
            }
            let focused = ctx.input(|i| i.viewport().focused == Some(true));
            if let Some(after) = &mut self.after_ui {
                after(&self.windows[i].app, focused);
            }
            // Requests a command left during the draw (a ribbon button, a menu) are carried out
            // now, so a new window appears on the next pass.
            let requests = std::mem::take(&mut self.windows[i].app.window_requests);
            for req in requests {
                self.handle(id, req, &ctx);
            }
        }
        if vp == ViewportId::ROOT {
            self.declare_children(&ctx);
        }
        crate::set_window_salt(0);
    }

    /// Work another thread finishes (the control server, a file being read) wakes egui's root
    /// viewport, whichever window asked: a root frame that no input caused passes the wake-up on
    /// to the windows waiting on such work, so their frames pick the answer up.
    fn pass_wake_ups_on(&self, ctx: &egui::Context) {
        if self.windows.len() < 2 || ctx.input(|i| !i.events.is_empty()) {
            return;
        }
        for w in self.windows.iter().filter(|w| w.viewport != ViewportId::ROOT && !w.closing && w.app.waits_on_background()) {
            ctx.request_repaint_of(w.viewport);
        }
    }

    /// A screenshot (`ui.screenshot`) comes back as an event in whichever viewport's frame runs
    /// next, not necessarily the window that asked: one taken of another window goes to that
    /// window's app (this window's own frame handles its own).
    fn pass_screenshots_on(&mut self, vp: ViewportId, ctx: &egui::Context) {
        let foreign: Vec<(ViewportId, u64, std::sync::Arc<egui::ColorImage>)> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { viewport_id, .. } if *viewport_id != vp => {
                        crate::screenshot_event(e).map(|(token, image)| (*viewport_id, token, image))
                    }
                    _ => None,
                })
                .collect()
        });
        for (owner, token, image) in foreign {
            if let Some(w) = self.window_of_viewport_mut(owner) {
                w.app.screenshot_arrived(token, &image);
            }
        }
    }

    /// eframe's raw input hook, for the viewport the input is for: the menu bar's events go to
    /// the window in use (the one the menu shows), then the window's own hook runs with its
    /// presentation in hand.
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
        let Some(i) = self.index_of_viewport(raw.viewport_id) else { return };
        self.take_turn(i);
        if self.is_active_viewport(raw.viewport_id) {
            let Host { menu, windows, .. } = self;
            if let Some(menu) = menu {
                native_menu::raw_input(menu, &mut windows[i].app, raw);
            }
        }
        self.windows[i].app.raw_input_hook(raw);
    }

    /// The root pass: every other window is declared, so egui keeps it; a window being closed is
    /// left out, which closes it, and dropped.
    fn declare_children(&mut self, ctx: &egui::Context) {
        for w in &self.windows {
            if w.viewport == ViewportId::ROOT || w.closing {
                continue;
            }
            let vp = w.viewport;
            ctx.show_viewport_deferred(vp, w.builder.clone(), move |ui, _class| {
                Host::with(|h| {
                    h.logic(vp, ui.ctx());
                    h.ui(vp, ui);
                });
            });
        }
        self.windows.retain(|w| !w.closing);
    }

    /// Carry out what a window asked for.
    fn handle(&mut self, from: WindowId, req: WindowRequest, ctx: &egui::Context) {
        match req {
            WindowRequest::Adopt(doc) => {
                self.spawn(from, Some(*doc), ctx);
            }
            WindowRequest::Focus(id) => self.focus(id, ctx),
            WindowRequest::ArrangeAll => self.arrange_all(ctx),
            WindowRequest::Cascade => self.cascade_all(ctx),
            WindowRequest::Close => {
                if self.open_count() > 1 {
                    self.close_window(from, ctx);
                }
            }
            WindowRequest::Quit => self.request_quit(ctx),
            WindowRequest::NewWindow => self.new_window_on(from, ctx),
            WindowRequest::CloseDocumentWindows => {
                let Some(g) = self.get(from).and_then(|w| w.group) else { return };
                let others: Vec<WindowId> = self.windows.iter().filter(|w| w.id != from && w.group == Some(g) && !w.closing).map(|w| w.id).collect();
                for id in others {
                    self.close_window(id, ctx);
                }
            }
        }
    }

    /// A new window from `origin`, showing `doc` (or nothing yet: View › New Window fills it). The
    /// window takes the interface state of the window it came from, with its own view, and
    /// appears a step down and right of it.
    fn spawn(&mut self, origin: WindowId, doc: Option<DocState>, ctx: &egui::Context) -> WindowId {
        let mut app = (self.new_app)();
        let untitled = self.windows.iter().map(|w| w.app.session.untitled_count()).max().unwrap_or(0);
        let placement = match self.get(origin) {
            Some(o) => {
                app.ui = o.app.ui.clone();
                app.ui.zoom = None;
                app.ui.view = crate::ViewMode::Normal;
                app.ui.pane = None;
                app.ui.tab = "Home".into();
                app.ui.comment_focus = None;
                app.session.prefs = o.app.session.prefs.clone();
                app.integrated_titlebar = o.app.integrated_titlebar;
                cascade_from(o.outer, o.inner, self.work_area_of(o))
            }
            None => None,
        };
        app.session.set_untitled_count(untitled);
        app.session.windows.host = true;
        if let Some(doc) = doc {
            app.session.add_document(doc);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(2);
        let viewport = ViewportId::from_hash_of(("deckcraft-window", id));
        let mut builder = self.template.clone().with_title(app.window_title());
        if let Some((pos, size)) = placement {
            builder = builder.with_position(pos).with_inner_size(size);
        }
        let mut window = Window::new(id, viewport, app, builder);
        window.focused_at = self.frames;
        self.windows.push(window);
        self.active = id;
        ctx.request_repaint_of(ViewportId::ROOT);
        id
    }

    /// Open presentations handed to the app (the command line, Finder): each in a window of its
    /// own, the first one in the active window when it shows nothing yet.
    pub fn open_paths(&mut self, paths: impl IntoIterator<Item = String>, ctx: &egui::Context) {
        for p in paths {
            let Some(w) = self.active_mut() else { return };
            let id = w.id;
            match w.app.open_path(&p) {
                // Leave the File tab, as opening from the Open dialog does.
                Ok(()) => w.app.backstage = None,
                Err(e) => {
                    log::error!("{p}: {e}");
                    w.app.set_status(format!("Couldn't open {p}: {e}"));
                }
            }
            let requests = std::mem::take(&mut w.app.window_requests);
            for req in requests {
                self.handle(id, req, ctx);
            }
        }
    }

    /// Bring a window to the front (and out of the Dock or taskbar).
    fn focus(&mut self, id: WindowId, ctx: &egui::Context) {
        let Some(w) = self.get(id) else { return };
        let viewport = w.viewport;
        ctx.send_viewport_cmd_to(viewport, ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(viewport, ViewportCommand::Focus);
        ctx.request_repaint_of(viewport);
        self.active = id;
    }

    /// Close a window. The root viewport is the app's window: with others open, the window that
    /// last had the focus moves into it (the root takes that window's place on screen and the
    /// other viewport goes); the last window closing ends the app.
    fn close_window(&mut self, id: WindowId, ctx: &egui::Context) {
        let Some(i) = self.index(id) else { return };
        self.leave_group(id);
        if self.windows[i].viewport == ViewportId::ROOT {
            let successor = self.windows.iter().enumerate().filter(|(j, w)| *j != i && !w.closing).max_by_key(|(_, w)| w.focused_at).map(|(j, _)| j);
            let Some(j) = successor else {
                self.exit = true;
                return;
            };
            let moved = self.windows.remove(j);
            let Some(root) = self.windows.iter_mut().find(|w| w.id == id) else { return };
            let old = std::mem::replace(&mut root.app, moved.app);
            drop(old);
            root.id = moved.id;
            root.focused_at = self.frames;
            root.sent_title = None;
            root.group = moved.group;
            root.view_number = moved.view_number;
            root.view = moved.view;
            root.app.rehomed();
            // The root takes the closed window's place, so what the user sees is that window
            // closing and the other staying where it was.
            if let Some(outer) = moved.outer {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::OuterPosition(outer.min));
            }
            if let Some(inner) = moved.inner {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(inner.size()));
            }
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
            self.active = moved.id;
        } else {
            self.windows[i].closing = true;
            if self.active == id {
                self.active = self.windows.iter().filter(|w| !w.closing).max_by_key(|w| w.focused_at).map_or(1, |w| w.id);
            }
        }
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    /// Quit: close every window, asking about unsaved changes one window at a time (the prompt
    /// shows in the window it is about); Cancel in a prompt keeps the app open.
    pub fn request_quit(&mut self, ctx: &egui::Context) {
        if self.quit != Quit::No {
            return;
        }
        self.quit_next(ctx);
    }

    /// Quit without asking (`app.quit` from the control channel): every presentation is saved
    /// first when `save`, and one that can't be is the error.
    pub fn quit_now(&mut self, save: bool, ctx: &egui::Context) -> Result<(), String> {
        if save {
            for w in self.windows.iter_mut().filter(|w| !w.closing) {
                if !w.app.save_all() {
                    return Err("not every presentation could be saved".into());
                }
            }
        }
        for w in self.windows.iter_mut().filter(|w| !w.closing) {
            w.app.quit_confirmed = true;
        }
        self.quit = Quit::No;
        self.quit_next(ctx);
        Ok(())
    }

    fn quit_next(&mut self, ctx: &egui::Context) {
        loop {
            let Some(w) = self.windows.iter_mut().find(|w| !w.closing) else {
                self.exit = true;
                self.quit = Quit::No;
                return;
            };
            let id = w.id;
            if w.app.close_requested() {
                self.close_window(id, ctx);
                if self.exit {
                    self.quit = Quit::No;
                    return;
                }
                continue;
            }
            self.quit = Quit::Asking(id);
            self.focus(id, ctx);
            return;
        }
    }

    /// Each root frame while quitting: the prompt was answered (the window closes and the next
    /// one is asked) or cancelled (the quit is off).
    fn quit_tick(&mut self, ctx: &egui::Context) {
        let Quit::Asking(id) = self.quit else { return };
        let Some(w) = self.get_mut(id) else {
            self.quit = Quit::No;
            self.quit_next(ctx);
            return;
        };
        if w.app.quit_confirmed {
            self.close_window(id, ctx);
            self.quit = Quit::No;
            if !self.exit {
                self.quit_next(ctx);
            }
        } else if w.app.dialog.as_ref().is_none_or(|d| d.id != "quit") {
            self.quit = Quit::No;
        }
    }

    /// The control channel (`--control`): `ui.windows` lists the windows, `ui.focus` brings one
    /// forward and `app.quit` closes them all; every other request goes to the window its
    /// `window` parameter names (an id or a Switch Windows number), else to the window in use.
    /// A request that needs no frame of that window (a command, `document.inspect`, the files)
    /// runs at once, here in the first window's frame, with the window's presentation in hand;
    /// the rest (`ui.inspect`, screenshots, pointer and key input) is answered from the window's
    /// own frame. A window that is minimized or covered by other windows gets no frame until it
    /// shows again (eframe runs no pass for it; only the first window's logic always runs), so
    /// such a request that named no window goes to the first window instead while the one in use
    /// is hidden, and one that named a hidden window is answered with an error at once rather
    /// than left to time out.
    fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control_rx.take() else { return };
        // Whether each window is on screen now, as eframe reports every viewport to this frame
        // (a hidden window's own report is stale: it gets no frame).
        let hidden = |h: &Host, id: WindowId| {
            h.get(id).is_some_and(|w| {
                w.viewport != ViewportId::ROOT
                    && ctx.input(|i| i.raw.viewports.get(&w.viewport).and_then(|v| v.visible())).or(w.visible) == Some(false)
            })
        };
        while let Ok(req) = rx.try_recv() {
            if req.method == "ui.windows" {
                if req.take() {
                    let _ = req.reply.send(json!({"ok": true, "result": self.windows_json()}));
                }
                continue;
            }
            let active = self.active().map(|w| w.id);
            let (named, target) = match req.params.get("window") {
                None | Some(Value::Null) => (false, active),
                Some(Value::String(s)) if s == "active" => (false, active),
                Some(v) => (true, v.as_u64().and_then(|n| self.resolve(n))),
            };
            let Some(mut target) = target else {
                if req.take() {
                    let _ = req.reply.send(json!({"ok": false, "error": "no such window; `window` is an id or a number from ui.windows"}));
                }
                continue;
            };
            match req.method.as_str() {
                "ui.focus" => {
                    if req.take() {
                        self.focus(target, ctx);
                        let _ = req.reply.send(json!({"ok": true, "result": {"window": target}}));
                    }
                    continue;
                }
                "app.quit" => {
                    if req.take() {
                        // Automation quits without asking (`{"save": true}` saves first).
                        let save = req.params.get("save").and_then(Value::as_bool) == Some(true);
                        let _ = req.reply.send(match self.quit_now(save, ctx) {
                            Ok(()) => json!({"ok": true, "result": Value::Null}),
                            Err(e) => json!({"ok": false, "error": e}),
                        });
                    }
                    continue;
                }
                _ => {}
            }
            if frame_free(&req.method)
                && let Some(i) = self.index(target)
                && !self.windows[i].closing
            {
                // With the window's presentation in hand (View › New Window's windows take turns).
                self.take_turn(i);
                let w = &mut self.windows[i];
                crate::set_window_salt(w.id);
                w.app.control_request(ctx, &req);
                let requests = std::mem::take(&mut w.app.window_requests);
                let viewport = w.viewport;
                ctx.request_repaint_of(viewport);
                for r in requests {
                    self.handle(target, r, ctx);
                }
                continue;
            }
            if hidden(self, target) {
                if named {
                    if req.take() {
                        let _ = req.reply.send(json!({"ok": false, "error": format!("window {target} is hidden (minimized or covered by other windows) and gets no frames; bring it forward with ui.focus first")}));
                    }
                    continue;
                }
                if let Some(root) = self.windows.iter().find(|w| w.viewport == ViewportId::ROOT && !w.closing) {
                    target = root.id;
                }
            }
            if let Some(w) = self.get_mut(target) {
                w.app.control_inbox.push(req);
                ctx.request_repaint_of(w.viewport);
            }
        }
        self.control_rx = Some(rx);
    }

    /// The windows as `ui.windows` reports them.
    pub fn windows_json(&self) -> Value {
        let active = self.active().map(|w| w.id);
        json!({
            "active": active,
            "windows": self.windows.iter().filter(|w| !w.closing).map(|w| {
                let app = &self.holder_of(w).app;
                json!({
                    "id": w.id,
                    "title": app.title_stem(),
                    "path": app.session.active().and_then(|d| d.path.clone()),
                    "dirty": app.session.active().is_some_and(|d| d.is_dirty()),
                    "active": Some(w.id) == active,
                    "root": w.viewport == ViewportId::ROOT,
                    "document": w.group,
                    "viewNumber": w.view_number,
                })
            }).collect::<Vec<_>>(),
        })
    }

    /// The work area the window is on: the platform's answer, else its monitor less a system bar.
    fn work_area_of(&self, w: &Window) -> Option<Rect> {
        if let Some(f) = &self.work_area
            && let Some(r) = f(w.outer)
        {
            return Some(r);
        }
        let m = w.monitor.filter(|m| m.is_finite() && m.x > 0.0 && m.y > 0.0)?;
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(m.x, m.y - SYSTEM_BAR)))
    }

    /// Put a window's frame at `outer` (its content size follows from the frame's).
    fn place(&self, id: WindowId, outer: Rect, ctx: &egui::Context) {
        let Some(w) = self.get(id) else { return };
        let insets = match (w.outer, w.inner) {
            (Some(o), Some(i)) => (o.size() - i.size()).max(Vec2::ZERO),
            _ => Vec2::ZERO,
        };
        let size = (outer.size() - insets).max(MIN_SIZE);
        ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::OuterPosition(outer.min));
        ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::InnerSize(size));
        ctx.request_repaint_of(w.viewport);
    }

    /// View › Window › Arrange All: every window tiled across the work area of the active
    /// window's screen, side by side (a grid with more than three), in the Switch Windows order.
    fn arrange_all(&mut self, ctx: &egui::Context) {
        let Some(work) = self.active().and_then(|w| self.work_area_of(w)) else { return };
        let ids: Vec<WindowId> = self.windows.iter().filter(|w| !w.closing).map(|w| w.id).collect();
        for (id, rect) in ids.iter().zip(tile_grid(work, ids.len())) {
            self.place(*id, rect, ctx);
        }
    }

    /// View › Window › Cascade: every window a step down and right of the one before, the window
    /// in use on top.
    fn cascade_all(&mut self, ctx: &egui::Context) {
        let Some(work) = self.active().and_then(|w| self.work_area_of(w)) else { return };
        let active = self.active().map(|w| w.id);
        let ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| !w.closing && Some(w.id) != active)
            .chain(self.windows.iter().filter(|w| Some(w.id) == active))
            .map(|w| w.id)
            .collect();
        for (id, rect) in ids.iter().zip(cascade_rects(work, ids.len())) {
            self.place(*id, rect, ctx);
        }
        for id in ids {
            if let Some(w) = self.get(id) {
                ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::Focus);
            }
        }
    }

    /// The interface fonts are one set for every window: a face one window borrowed (a Korean
    /// title, a Thai note) joins every window's list, so whichever window installs the fonts next
    /// installs it too, and the lists never fight.
    fn pool_fonts(&mut self) {
        if self.windows.len() < 2 {
            return;
        }
        let mut faces: Vec<std::sync::Arc<deckcraft_fonts::FontFace>> = Vec::new();
        for w in &self.windows {
            for f in &w.app.ui_fonts.faces {
                if !faces.iter().any(|g| g.id() == f.id()) {
                    faces.push(f.clone());
                }
            }
        }
        for w in &mut self.windows {
            w.app.adopt_faces(&faces);
        }
    }

    /// Untitled presentations are numbered across the windows (`Presentation3` after another
    /// window's `Presentation2`).
    fn sync_untitled(&mut self, i: usize) {
        let max = self.windows.iter().map(|w| w.app.session.untitled_count()).max().unwrap_or(0);
        if let Some(w) = self.windows.get_mut(i) {
            w.app.session.set_untitled_count(max);
        }
    }

    /// The settings the user changed in the window they are using go to the other windows.
    fn share_settings(&mut self, id: WindowId, ctx: &egui::Context) {
        if self.windows.len() < 2 || self.active != id {
            return;
        }
        let Some(w) = self.get(id) else { return };
        let now = w.app.shared_settings();
        if self.shared_settings.as_deref() == Some(now.as_str()) {
            return;
        }
        let (ui, prefs) = (w.app.ui.clone(), w.app.session.prefs.clone());
        self.shared_settings = Some(now);
        for w in self.windows.iter_mut().filter(|w| w.id != id) {
            w.app.adopt_settings(&ui, &prefs);
            ctx.request_repaint_of(w.viewport);
        }
    }
}

impl Window {
    fn new(id: WindowId, viewport: ViewportId, app: SlideApp, builder: ViewportBuilder) -> Self {
        Window {
            id,
            viewport,
            app,
            builder,
            outer: None,
            inner: None,
            monitor: None,
            visible: None,
            focused_at: 0,
            closing: false,
            sent_title: None,
            group: None,
            view_number: None,
            view: None,
        }
    }

    /// View › New Window: this window's number among the windows on its presentation.
    pub fn view_number(&self) -> Option<u32> {
        self.view_number
    }
}

/// Control methods that need no frame of the window they act on: they run in the first window's
/// frame, so they reach a window that is covered or minimized too.
fn frame_free(method: &str) -> bool {
    matches!(
        method,
        "engine.execute" | "ui.menu.invoke" | "command" | "engine.commands" | "document.inspect" | "app.open" | "app.save" | "app.export"
    )
}

/// What the other windows on a presentation watch for: its revision, dirty flag and file.
fn doc_mark(app: &SlideApp) -> (u64, bool, Option<String>) {
    match app.session.active() {
        Some(d) => (d.revision, d.is_dirty(), d.path.clone()),
        None => (0, false, None),
    }
}

/// Where a window opened from one with frame `outer` and content `inner` goes: a step down and
/// right, at the same content size, back to the work area's corner when that would run off it.
/// `None` when the origin's place isn't known: the system places the window.
pub fn cascade_from(outer: Option<Rect>, inner: Option<Rect>, work: Option<Rect>) -> Option<(Pos2, Vec2)> {
    let outer = outer?;
    let size = inner.map_or(outer.size(), |i| i.size()).max(MIN_SIZE);
    let mut p = outer.min + CASCADE;
    if let Some(work) = work
        && (p.x + size.x > work.right() || p.y + size.y > work.bottom())
    {
        p = work.min + CASCADE;
    }
    Some((p, size))
}

/// Arrange All's tiles: `n` windows side by side across the work area, in rows of up to three, each
/// at least [`MIN_SIZE`] (more windows than fit overlap at the right and bottom).
pub fn tile_grid(work: Rect, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    let cols = n.min(3);
    let rows = n.div_ceil(cols);
    let w = (work.width() / cols as f32).max(MIN_SIZE.x);
    let h = (work.height() / rows as f32).max(MIN_SIZE.y);
    (0..n)
        .map(|i| {
            let (r, c) = (i / cols, i % cols);
            Rect::from_min_size(pos2(work.left() + c as f32 * w, work.top() + r as f32 * h), vec2(w, h))
        })
        .collect()
}

/// Cascade's places: `n` windows each a step down and right of the one before, all the same size,
/// the last one still within the work area.
pub fn cascade_rects(work: Rect, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    let steps = (n - 1) as f32;
    let size = (work.size() - CASCADE * steps).max(MIN_SIZE);
    (0..n).map(|i| Rect::from_min_size(work.min + CASCADE * i as f32, size)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::*;
    use crate::Services;
    use deckcraft_engine::Session;
    use egui::{Pos2, Rect, ViewportEvent, ViewportInfo, pos2, vec2};

    fn app() -> SlideApp {
        SlideApp::new(Session::new(), Services::default())
    }

    fn host() -> (Host, egui::Context) {
        let ctx = egui::Context::default();
        // Real child viewports, as eframe has them, not windows embedded in the root.
        ctx.set_embed_viewports(false);
        (Host::new(app(), ViewportBuilder::default(), Box::new(app)), ctx)
    }

    fn vp(h: &Host, id: WindowId) -> ViewportId {
        h.get(id).map(|w| w.viewport).unwrap_or_else(|| panic!("window {id}"))
    }

    fn ids(h: &Host) -> Vec<WindowId> {
        h.windows().map(|w| w.id).collect()
    }

    fn title(h: &Host, id: WindowId) -> String {
        h.get(id).unwrap().app.title_stem()
    }

    fn slides(h: &Host, id: WindowId) -> usize {
        h.get(id).unwrap().app.session.active().map(|d| d.doc.slides.len()).unwrap_or(0)
    }

    fn has_doc(h: &Host, id: WindowId) -> bool {
        h.get(id).unwrap().app.session.active().is_some()
    }

    fn run(h: &mut Host, id: WindowId, cmd: &str, params: Value) -> Result<Value, String> {
        h.get_mut(id).unwrap().app.run(cmd, params)
    }

    const R1: Rect = Rect::from_min_max(pos2(40.0, 40.0), pos2(1320.0, 840.0));
    const R2: Rect = Rect::from_min_max(pos2(300.0, 200.0), pos2(1300.0, 900.0));

    /// One pass of the window in viewport `vp`, as eframe runs it: with `rect` as its place on
    /// a 1920 × 1080 monitor, `focused`, `events` (a close request, say) and input `input`.
    /// Returns the viewport commands the pass sent, by the viewport they went to.
    fn pass(
        h: &mut Host,
        ctx: &egui::Context,
        vp: ViewportId,
        rect: Rect,
        focused: bool,
        events: Vec<ViewportEvent>,
        input: Vec<egui::Event>,
    ) -> BTreeMap<ViewportId, Vec<ViewportCommand>> {
        let info = ViewportInfo {
            parent: (vp != ViewportId::ROOT).then_some(ViewportId::ROOT),
            events,
            outer_rect: Some(rect),
            inner_rect: Some(rect),
            monitor_size: Some(vec2(1920.0, 1080.0)),
            focused: Some(focused),
            ..Default::default()
        };
        let mut raw = egui::RawInput {
            viewport_id: vp,
            focused,
            time: Some(ctx.input(|i| i.time) + 1.0 / 60.0),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, rect.size())),
            events: input,
            ..Default::default()
        };
        raw.viewports.insert(vp, info);
        h.raw_input_hook(&mut raw);
        let out = ctx.run_ui(raw, |ui| {
            h.logic(vp, ui.ctx());
            h.ui(vp, ui);
        });
        let sent = out.viewport_output.iter().map(|(id, v)| (*id, v.commands.clone())).collect();
        out.drop_without_applying_deltas();
        sent
    }

    /// [`pass`] for the window `id`.
    fn pass_id(
        h: &mut Host,
        ctx: &egui::Context,
        id: WindowId,
        rect: Rect,
        focused: bool,
        events: Vec<ViewportEvent>,
        input: Vec<egui::Event>,
    ) -> BTreeMap<ViewportId, Vec<ViewportCommand>> {
        let v = vp(h, id);
        pass(h, ctx, v, rect, focused, events, input)
    }

    /// Settle: a few passes of every window, none focused, the root last (it drops closed windows).
    fn settle(h: &mut Host, ctx: &egui::Context) {
        for _ in 0..3 {
            let others: Vec<ViewportId> = h.windows().map(|w| w.viewport).filter(|v| *v != ViewportId::ROOT).collect();
            for (i, v) in others.iter().enumerate() {
                pass(h, ctx, *v, Rect::from_min_size(pos2(100.0 * (i + 1) as f32, 80.0), vec2(1000.0, 700.0)), false, vec![], vec![]);
            }
            pass(h, ctx, ViewportId::ROOT, R1, false, vec![], vec![]);
        }
    }

    /// A saved presentation to open: `name.deckcraft` in a folder of this test run's.
    fn temp_deck(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("deckcraft-windows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{name}.deckcraft")).to_string_lossy().into_owned();
        let mut s = Session::with_new();
        s.execute("file.save", &json!({"path": p, "format": "deckcraft"})).unwrap();
        p
    }

    /// PowerPoint opens each presentation in a window of its own and leaves the current one
    /// alone, so nothing is asked; a window showing nothing, or an untouched new presentation,
    /// takes it itself; a file already open comes to the front instead of opening twice.
    #[test]
    fn new_and_open_go_to_their_own_windows_and_a_blank_window_takes_the_presentation() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        assert!(h.get(first).unwrap().app.session.windows.host, "the engine knows the host");
        assert!(h.get(first).unwrap().app.window_reusable(), "a window showing nothing");

        let alpha = temp_deck("alpha");
        run(&mut h, first, "file.open", json!({"path": alpha})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first], "the empty window took the file");
        assert_eq!(h.get(first).unwrap().app.session.active().unwrap().path.as_deref(), Some(alpha.as_str()));
        assert!(!h.get(first).unwrap().app.window_reusable());

        // Editing it, then New: a new window, and no Save Changes prompt here.
        run(&mut h, first, "slide.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 2, "{:?}", ids(&h));
        let second = ids(&h)[1];
        assert!(h.get(first).unwrap().app.dialog.is_none(), "nothing asked");
        assert_eq!(slides(&h, first), 2, "the first presentation stays, with its edit");
        assert!(h.get(first).unwrap().app.session.active().unwrap().is_dirty());
        assert!(has_doc(&h, second) && h.get(second).unwrap().app.session.active().unwrap().path.is_none(), "the new one is blank");
        assert_ne!(vp(&h, second), ViewportId::ROOT, "in a window of its own");
        assert_eq!(h.active().unwrap().id, second, "the new window is the one in use");
        assert!(h.get(second).unwrap().app.session.windows.host, "and knows the host from the start");
        assert_eq!(h.get(second).unwrap().app.session.documents().len(), 1, "one presentation per window");

        // An untouched new presentation gives way to a file opened into its window.
        let beta = temp_deck("beta");
        run(&mut h, second, "file.open", json!({"path": beta})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first, second], "no third window");
        assert_eq!(h.get(second).unwrap().app.session.active().unwrap().path.as_deref(), Some(beta.as_str()));

        // Open from the first window: a third window; the same file again brings it forward.
        let gamma = temp_deck("gamma");
        run(&mut h, first, "file.open", json!({"path": gamma})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 3);
        let third = ids(&h)[2];
        assert_eq!(h.get(third).unwrap().app.session.active().unwrap().path.as_deref(), Some(gamma.as_str()));
        let r = run(&mut h, first, "file.open", json!({"path": gamma})).unwrap();
        assert_eq!(r["alreadyOpen"], true);
        let sent = pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![], vec![]);
        assert!(sent.get(&vp(&h, third)).is_some_and(|c| c.contains(&ViewportCommand::Focus)), "the open window comes forward: {sent:?}");
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 3, "not opened twice");
        assert_eq!(h.active().unwrap().id, third);

        // A file that can't be opened: no window, the error on the status line of the asker.
        let err = run(&mut h, first, "file.open", json!({"path": "/nonexistent/none.deckcraft"})).unwrap_err();
        assert!(err.contains("none.deckcraft"), "{err}");
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 3);

        // The engine's view from one window: every window, in order, this one marked.
        let w = &h.get(second).unwrap().app.session.windows;
        assert_eq!((w.this, w.list.len(), w.list[0].id), (second, 3, first));
        assert_eq!(w.list[2].path.as_deref(), Some(gamma.as_str()));
        assert!(w.list[0].title.contains("alpha"), "{:?}", w.list[0]);
    }

    /// Presentations already in one session (recovered files, the command line) each get a
    /// window on the first frames, the first window keeping the first one; untitled presentations
    /// are numbered across the windows.
    #[test]
    fn presentations_in_one_session_spread_over_windows_and_numbering_goes_on() {
        let mut a = app();
        for _ in 0..3 {
            a.run("file.new", json!({})).unwrap();
        }
        assert_eq!(a.session.documents().len(), 3);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut h = Host::new(a, ViewportBuilder::default(), Box::new(app));
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 3, "{:?}", ids(&h));
        let titles: Vec<String> = ids(&h).iter().map(|id| title(&h, *id)).collect();
        assert_eq!(titles, ["Presentation1", "Presentation2", "Presentation3"]);
        for id in ids(&h) {
            assert_eq!(h.get(id).unwrap().app.session.documents().len(), 1);
        }
        let third = ids(&h)[2];
        run(&mut h, third, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(title(&h, ids(&h)[3]), "Presentation4", "numbered after every window's");
    }

    /// File › Close closes the presentation's window when another is open, as PowerPoint does;
    /// the last window stays and shows the start screen. The window's close button closes only
    /// its window; the root window (the app's) takes over the presentation last used when it is
    /// the one closing, and closing the last window ends the app.
    #[test]
    fn closing_takes_one_window_and_the_root_takes_over_the_last_used_presentation() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first], "the empty window took Presentation1");
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        run(&mut h, second, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let third = ids(&h)[2];
        assert_eq!(ids(&h).len(), 3);
        assert_eq!((title(&h, first), title(&h, second), title(&h, third)), ("Presentation1".into(), "Presentation2".into(), "Presentation3".into()));

        // File › Close in the third: its window goes; the others keep their presentations.
        run(&mut h, third, "file.close", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first, second]);
        assert!(has_doc(&h, first) && has_doc(&h, second));

        // The close button on the root window while the second was last in use: the second's
        // presentation moves into the root window, which takes the second window's place, and
        // the root's own close is cancelled (eframe would end the app otherwise).
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        let sent = pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![ViewportEvent::Close], vec![]);
        assert!(!h.exit_requested(), "another window is open");
        assert_eq!(ids(&h), vec![second], "the root window now holds the second presentation");
        assert_eq!(vp(&h, second), ViewportId::ROOT);
        assert_eq!(title(&h, second), "Presentation2");
        let root = sent.get(&ViewportId::ROOT).cloned().unwrap_or_default();
        assert!(root.contains(&ViewportCommand::OuterPosition(R2.min)), "takes the closed window's place: {root:?}");
        assert!(root.contains(&ViewportCommand::InnerSize(R2.size())));
        assert!(root.contains(&ViewportCommand::CancelClose), "the root stays open: {root:?}");
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 1);

        // File › Close in the last window keeps the window, empty, as before.
        run(&mut h, second, "file.close", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 1);
        assert!(!has_doc(&h, second));
        assert!(h.get(second).unwrap().app.backstage.is_some(), "the start screen shows");
        // The close button on the last window ends the app.
        let sent = pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![ViewportEvent::Close], vec![]);
        assert!(h.exit_requested());
        assert!(sent.get(&ViewportId::ROOT).is_none_or(|c| !c.contains(&ViewportCommand::CancelClose)), "{sent:?}");
    }

    /// The close button on a window with unsaved changes asks first, in that window; Cancel keeps
    /// it, Don't Save closes just that window.
    #[test]
    fn a_child_window_with_unsaved_changes_asks_before_it_closes() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        run(&mut h, second, "slide.new", json!({})).unwrap();
        let sent = pass_id(&mut h, &ctx, second, R2, true, vec![ViewportEvent::Close], vec![]);
        assert!(sent.get(&vp(&h, second)).is_some_and(|c| c.contains(&ViewportCommand::CancelClose)), "{sent:?}");
        assert_eq!(h.get(second).unwrap().app.dialog.as_ref().map(|d| d.id.as_str()), Some("quit"));
        assert!(h.get(first).unwrap().app.dialog.is_none(), "only the window closing asks");
        // Cancel.
        h.get_mut(second).unwrap().app.dialog = None;
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first, second], "Cancel keeps the window");

        // Don't Save: the prompt's button lets the window close.
        pass_id(&mut h, &ctx, second, R2, true, vec![ViewportEvent::Close], vec![]);
        h.get_mut(second).unwrap().app.quit_confirmed = true;
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first], "only that window closed");
        assert!(!h.exit_requested());
    }

    /// Quitting asks about each window with unsaved changes in turn, in that window, and Cancel
    /// in any prompt keeps the app open with every window; once each is answered the app ends.
    #[test]
    fn quitting_asks_each_dirty_window_in_turn_and_cancel_keeps_the_app() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        for id in [first, second] {
            run(&mut h, id, "slide.new", json!({})).unwrap();
        }
        h.request_quit(&ctx);
        assert_eq!(h.get(first).unwrap().app.dialog.as_ref().map(|d| d.id.as_str()), Some("quit"), "the first window asks");
        assert!(h.get(second).unwrap().app.dialog.is_none(), "one at a time");
        h.get_mut(first).unwrap().app.dialog = None;
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 2, "Cancel keeps everything");
        assert!(!h.exit_requested());
        assert_eq!(h.quit, Quit::No);

        h.request_quit(&ctx);
        h.get_mut(first).unwrap().app.quit_confirmed = true;
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![second], "the first closed; the second is asked");
        assert_eq!(h.get(second).unwrap().app.dialog.as_ref().map(|d| d.id.as_str()), Some("quit"));
        h.get_mut(second).unwrap().app.quit_confirmed = true;
        settle(&mut h, &ctx);
        assert!(h.exit_requested(), "every window answered: the app ends");
    }

    /// Arrange All tiles the windows side by side across the work area; Cascade stacks them a
    /// step apart with the window in use on top.
    #[test]
    fn arrange_all_and_cascade_place_the_windows() {
        let (mut h, ctx) = host();
        let work = Rect::from_min_size(pos2(0.0, 25.0), vec2(2400.0, 1375.0));
        h.work_area = Some(Box::new(move |_| Some(work)));
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        let (vp1, vp2) = (vp(&h, first), vp(&h, second));
        pass(&mut h, &ctx, vp2, R2, false, vec![], vec![]);
        pass(&mut h, &ctx, vp1, R1, true, vec![], vec![]);

        run(&mut h, first, "view.arrangeAll", json!({})).unwrap();
        let sent = pass(&mut h, &ctx, vp1, R1, true, vec![], vec![]);
        let tiles = tile_grid(work, 2);
        for (v, tile) in [(vp1, tiles[0]), (vp2, tiles[1])] {
            let c = sent.get(&v).cloned().unwrap_or_default();
            assert!(c.contains(&ViewportCommand::OuterPosition(tile.min)) && c.contains(&ViewportCommand::InnerSize(tile.size())), "{v:?}: {c:?}");
            assert!(c.contains(&ViewportCommand::Maximized(false)));
        }

        // Cascade: the other window first, the window in use (the first) on top.
        run(&mut h, first, "view.cascade", json!({})).unwrap();
        let sent = pass(&mut h, &ctx, vp1, R1, true, vec![], vec![]);
        let steps = cascade_rects(work, 2);
        let c2 = sent.get(&vp2).cloned().unwrap_or_default();
        let c1 = sent.get(&vp1).cloned().unwrap_or_default();
        assert!(c2.contains(&ViewportCommand::OuterPosition(steps[0].min)) && c2.contains(&ViewportCommand::InnerSize(steps[0].size())), "{c2:?}");
        assert!(c1.contains(&ViewportCommand::OuterPosition(steps[1].min)) && c1.contains(&ViewportCommand::Focus), "{c1:?}");
        assert!(c2.contains(&ViewportCommand::Focus));
    }

    /// Switch Windows and Next Window bring the named window forward and make it the one in use.
    #[test]
    fn switch_windows_and_next_window_bring_a_window_forward() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![], vec![]);
        assert_eq!(h.active().unwrap().id, first);
        run(&mut h, first, "view.switchWindows", json!({"window": 2})).unwrap();
        let sent = pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![], vec![]);
        let c = sent.get(&vp(&h, second)).cloned().unwrap_or_default();
        assert!(c.contains(&ViewportCommand::Focus) && c.contains(&ViewportCommand::Minimized(false)), "{c:?}");
        assert_eq!(h.active().unwrap().id, second);
        // Next Window from the second goes round to the first.
        assert_eq!(run(&mut h, second, "window.next", json!({})).unwrap()["window"], first);
        let sent = pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        assert!(sent.get(&ViewportId::ROOT).is_some_and(|c| c.contains(&ViewportCommand::Focus)), "{sent:?}");
        assert_eq!(h.active().unwrap().id, first);
    }

    /// The control channel: `ui.windows` lists the windows, and a request goes to the window its
    /// `window` names (an id, or a number in that list), else to the window in use; a window that
    /// isn't open is an error; `ui.focus` and `app.quit` are the host's own.
    #[test]
    fn control_requests_reach_the_named_window() {
        let (mut h, ctx) = host();
        let (tx, rx) = std::sync::mpsc::channel();
        h = h.with_control(rx);
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];

        let (req, reply) = ControlRequest::new("ui.windows", json!({}));
        tx.send(req).unwrap();
        settle(&mut h, &ctx);
        let v = reply.try_recv().unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["result"]["windows"].as_array().map(Vec::len), Some(2));
        assert_eq!(v["result"]["active"], second);
        assert_eq!(v["result"]["windows"][0]["title"], "Presentation1");

        // New from the channel opens a window too, as from the ribbon: a window of its own.
        let (req, reply) = ControlRequest::new("engine.execute", json!({"command": "file.new"}));
        tx.send(req).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(reply.try_recv().unwrap()["ok"], true);
        assert_eq!(ids(&h).len(), 3);
        let third = ids(&h)[2];
        run(&mut h, third, "file.close", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first, second]);

        let (req, reply) = ControlRequest::new("engine.execute", json!({"window": first, "command": "slide.new"}));
        tx.send(req).unwrap();
        let (req2, reply2) = ControlRequest::new("engine.execute", json!({"command": "slide.new"}));
        tx.send(req2).unwrap();
        let (req3, reply3) = ControlRequest::new("engine.execute", json!({"window": 99, "command": "slide.new"}));
        tx.send(req3).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(reply.try_recv().unwrap()["ok"], true);
        assert_eq!(reply2.try_recv().unwrap()["ok"], true);
        assert_eq!(reply3.try_recv().unwrap()["ok"], false);
        assert_eq!(slides(&h, first), 2, "the named window got the slide");
        assert_eq!(slides(&h, second), 2, "the window in use got the other");

        // ui.inspect says which window answered; ui.focus brings one forward.
        let (req, reply) = ControlRequest::new("ui.inspect", json!({"window": 1}));
        tx.send(req).unwrap();
        let (req2, reply2) = ControlRequest::new("ui.focus", json!({"window": first}));
        tx.send(req2).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(reply.try_recv().unwrap()["result"]["windowId"], first);
        assert_eq!(reply2.try_recv().unwrap()["result"]["window"], first);
        assert_eq!(h.active().unwrap().id, first);

        // app.quit closes every window without asking, even dirty ones.
        let (req, reply) = ControlRequest::new("app.quit", json!({}));
        tx.send(req).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(reply.try_recv().unwrap()["ok"], true);
        assert!(h.exit_requested());
    }

    /// A window that is minimized or covered gets no frame: a request that named no window goes
    /// to the first window while the one in use is hidden, and one that named the hidden window
    /// is answered with an error at once.
    #[test]
    fn control_requests_avoid_a_hidden_window() {
        let (mut h, ctx) = host();
        let (tx, rx) = std::sync::mpsc::channel();
        h = h.with_control(rx);
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        // The second window's last frame saw it covered; it is the one in use.
        let v2 = vp(&h, second);
        let info = ViewportInfo {
            parent: Some(ViewportId::ROOT),
            outer_rect: Some(R2),
            inner_rect: Some(R2),
            focused: Some(true),
            occluded: Some(true),
            minimized: Some(false),
            ..Default::default()
        };
        let mut raw =
            egui::RawInput { viewport_id: v2, focused: true, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, R2.size())), ..Default::default() };
        raw.viewports.insert(v2, info);
        let out = ctx.run_ui(raw, |ui| {
            h.logic(v2, ui.ctx());
            h.ui(v2, ui);
        });
        out.drop_without_applying_deltas();
        assert_eq!(h.active().unwrap().id, second);
        assert_eq!(h.get(second).unwrap().visible, Some(false));

        // A command needs no frame of its window: it runs at once, in the hidden window too.
        let (req, reply) = ControlRequest::new("engine.execute", json!({"command": "slide.new"}));
        tx.send(req).unwrap();
        let (req2, reply2) = ControlRequest::new("engine.execute", json!({"window": first, "command": "slide.new"}));
        tx.send(req2).unwrap();
        // What needs the window's frame goes to the first window unnamed, and is refused named.
        let (req3, reply3) = ControlRequest::new("ui.inspect", json!({}));
        tx.send(req3).unwrap();
        let (req4, reply4) = ControlRequest::new("ui.inspect", json!({"window": second}));
        tx.send(req4).unwrap();
        pass(&mut h, &ctx, ViewportId::ROOT, R1, false, vec![], vec![]);
        assert_eq!(reply.try_recv().unwrap()["ok"], true);
        assert_eq!(slides(&h, second), 2, "the hidden window in use got its slide all the same");
        assert_eq!(reply2.try_recv().unwrap()["ok"], true);
        assert_eq!(slides(&h, first), 2);
        let v = reply3.try_recv().unwrap();
        assert_eq!(v["result"]["windowId"], first, "the first window answered for the hidden one in use: {v}");
        let v = reply4.try_recv().unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("hidden"), "{v}");
    }

    /// A setting changed in the window in use (the appearance, the recent list…) reaches the
    /// other windows; what is a window's own (its ribbon tab) stays.
    #[test]
    fn settings_changed_in_one_window_reach_the_others() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "file.new", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        run(&mut h, second, "view.dark", json!({"on": true})).unwrap();
        run(&mut h, second, "view.tab", json!({"tab": "View"})).unwrap();
        // A frame with input in the window in use hands the settings on.
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![egui::Event::PointerMoved(pos2(10.0, 10.0))]);
        assert_eq!(h.get(first).unwrap().app.ui.brightness, crate::theme::Brightness::Dark);
        assert_eq!(h.get(first).unwrap().app.ui.tab, "Home", "the tab is the window's own");
    }

    /// Arrange All's tiles, Cascade's steps, and where a window opened from another goes.
    #[test]
    fn placement_geometry() {
        let work = Rect::from_min_size(pos2(0.0, 25.0), vec2(2400.0, 1375.0));
        let tiles = tile_grid(work, 2);
        assert_eq!(
            tiles,
            vec![Rect::from_min_size(pos2(0.0, 25.0), vec2(1200.0, 1375.0)), Rect::from_min_size(pos2(1200.0, 25.0), vec2(1200.0, 1375.0))]
        );
        let tiles = tile_grid(work, 4);
        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[3].min, pos2(0.0, 25.0 + 687.5), "a second row after three");
        assert_eq!(tiles[0].width(), MIN_SIZE.x, "never narrower than the minimum");
        let steps = cascade_rects(work, 3);
        assert_eq!(steps[0].min, work.min);
        assert_eq!(steps[2].min, work.min + CASCADE * 2.0);
        assert_eq!(steps[2].max, work.max, "the last one still fits");
        assert_eq!(steps[0].size(), steps[2].size());

        let outer = Rect::from_min_size(pos2(100.0, 100.0), vec2(1280.0, 800.0));
        let inner = Rect::from_min_size(pos2(100.0, 128.0), vec2(1280.0, 772.0));
        let big = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0));
        assert_eq!(cascade_from(Some(outer), Some(inner), Some(big)), Some((pos2(122.0, 122.0), vec2(1280.0, 772.0))));
        let far = cascade_from(Some(outer.translate(vec2(600.0, 200.0))), Some(inner), Some(big));
        assert_eq!(far, Some((pos2(22.0, 22.0), vec2(1280.0, 772.0))), "back to the corner when it would run off the screen");
        assert_eq!(cascade_from(None, None, None), None, "no place known: the system places it");
    }

    /// View › New Window: a second window on the same presentation, as PowerPoint's. An edit in
    /// either shows in both, one undo history serves both, each keeps its own slide, the titles
    /// are numbered `:1` and `:2`, closing one window closes a view (no prompt, the presentation
    /// stays open in the other), and File › Close closes the presentation's windows together. A
    /// command runs in a window during its frame, when it holds the presentation: the test gives
    /// a window a frame before running one there.
    #[test]
    fn new_window_shows_the_same_presentation_with_its_own_slide() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "slide.new", json!({})).unwrap();
        run(&mut h, first, "slide.new", json!({})).unwrap();
        run(&mut h, first, "slide.go", json!({"index": 2})).unwrap();
        let uid = h.get(first).unwrap().app.session.active().unwrap().uid;
        assert_eq!(run(&mut h, first, "view.newWindow", json!({})).unwrap()["pending"], "window");
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 2);
        let second = ids(&h)[1];
        assert_eq!((h.get(first).unwrap().view_number(), h.get(second).unwrap().view_number()), (Some(1), Some(2)));
        assert_eq!(h.get(first).unwrap().app.view_number, Some(1));
        assert!(h.get(first).unwrap().app.window_title().starts_with("Presentation1:1"));
        // The second window's turn: it holds the presentation, starting from the first's view.
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        let w = &h.get(second).unwrap().app.session.windows;
        assert_eq!(w.others_on_this_document().map(|e| e.id).collect::<Vec<_>>(), vec![first]);
        assert_eq!(w.list[1].title, "Presentation1", "the list names the presentation");
        assert_eq!(h.get(second).unwrap().app.session.active().unwrap().uid, uid, "the same presentation");
        assert_eq!(h.get(second).unwrap().app.session.active().unwrap().selection.slide, 2);

        // The second window goes to the first slide and adds one; the first window, on its next
        // frame, shows the new slide and its undo takes it back: one presentation, one history.
        run(&mut h, second, "slide.go", json!({"index": 0})).unwrap();
        run(&mut h, second, "slide.new", json!({})).unwrap();
        assert_eq!(slides(&h, second), 4);
        pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![], vec![]);
        assert_eq!(slides(&h, first), 4, "the first window shows the edit");
        assert_eq!(h.get(first).unwrap().app.session.active().unwrap().selection.slide, 2, "on its own slide");
        run(&mut h, first, "edit.undo", json!({})).unwrap();
        assert_eq!(slides(&h, first), 3);
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        assert_eq!(slides(&h, second), 3, "the second window shows the undo");
        assert!(h.get(second).unwrap().app.session.active().unwrap().is_dirty(), "the presentation is dirty in both");

        // The close button on one window closes that view without asking; the presentation stays.
        let sent = pass_id(&mut h, &ctx, second, R2, true, vec![ViewportEvent::Close], vec![]);
        assert!(sent.get(&vp(&h, second)).is_none_or(|c| !c.contains(&ViewportCommand::CancelClose)), "{sent:?}");
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![first]);
        assert_eq!(h.get(first).unwrap().app.session.active().unwrap().uid, uid, "the presentation moved back to the first window");
        assert!(h.get(first).unwrap().app.session.active().unwrap().is_dirty(), "with its unsaved change");
        assert_eq!(h.get(first).unwrap().app.view_number, None, "a lone window has no number");
        assert!(h.get(first).unwrap().app.session.windows.others_on_this_document().next().is_none());

        // New Window again, then File › Close in one: the presentation closes, both windows go,
        // and the last window stays empty, as a lone presentation's Close leaves it.
        run(&mut h, first, "view.newWindow", json!({})).unwrap();
        settle(&mut h, &ctx);
        let third = ids(&h)[1];
        assert_eq!(h.get(third).unwrap().view_number(), Some(2));
        pass_id(&mut h, &ctx, third, R2, true, vec![], vec![]);
        run(&mut h, third, "file.close", json!({})).unwrap();
        settle(&mut h, &ctx);
        assert_eq!(ids(&h).len(), 1, "the presentation's other window closed with it: {:?}", ids(&h));
        assert!(h.active().unwrap().app.session.active().is_none(), "the last window stays, empty");
        assert!(h.groups.is_empty());
    }

    /// The root window closing while its presentation is shown in another window: that window's
    /// presentation moves into the root (the root takes over the last-used window) and the group
    /// goes on with the remaining member only.
    #[test]
    fn closing_the_root_of_a_presentation_group_keeps_the_presentation() {
        let (mut h, ctx) = host();
        settle(&mut h, &ctx);
        let first = h.active().unwrap().id;
        run(&mut h, first, "file.new", json!({})).unwrap();
        run(&mut h, first, "slide.new", json!({})).unwrap();
        run(&mut h, first, "view.newWindow", json!({})).unwrap();
        settle(&mut h, &ctx);
        let second = ids(&h)[1];
        pass_id(&mut h, &ctx, second, R2, true, vec![], vec![]);
        pass(&mut h, &ctx, ViewportId::ROOT, R1, true, vec![ViewportEvent::Close], vec![]);
        settle(&mut h, &ctx);
        assert_eq!(ids(&h), vec![second]);
        assert_eq!(vp(&h, second), ViewportId::ROOT);
        assert_eq!(slides(&h, second), 2);
        assert_eq!(h.get(second).unwrap().app.view_number, None);
        assert!(h.groups.is_empty(), "a group of one is no group");
        assert!(!h.exit_requested());
    }

    /// Fixed widget ids are kept apart per window, and are the plain ids outside a window host.
    #[test]
    fn fixed_widget_ids_are_per_window() {
        crate::set_window_salt(0);
        assert_eq!(crate::wid("canvas"), egui::Id::new("canvas"));
        crate::set_window_salt(1);
        let one = crate::wid("canvas");
        crate::set_window_salt(2);
        let two = crate::wid("canvas");
        crate::set_window_salt(0);
        assert!(one != two && one != egui::Id::new("canvas"));
    }
}
