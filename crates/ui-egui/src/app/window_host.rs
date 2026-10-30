//! The window host's side of a presentation window ([`crate::windows::Host`]): one presentation
//! per window, what a window asks of the host, and what windows on one presentation hand over.

use super::{SlideApp, UiState, dialogs, json, windows};

impl SlideApp {
    /// After File › Close took the presentation out of this window: with other windows open the
    /// window closes with it, as PowerPoint closes a presentation's window; the last window stays
    /// and shows the start screen. The other windows on the same presentation (View › New Window)
    /// close too.
    pub(super) fn after_close(&mut self) {
        let w = &self.session.windows;
        if !w.host || self.session.active().is_some() {
            return;
        }
        if w.others_on_this_document().next().is_some() {
            self.window_requests.push(windows::WindowRequest::CloseDocumentWindows);
        }
        if w.others().next().is_some() {
            self.window_requests.push(windows::WindowRequest::Close);
        }
    }

    /// With a window host (the desktop app), a session holds one presentation: one that was just
    /// added (New, Open, a drop, a recovered file) goes to a window of its own, as PowerPoint opens
    /// each presentation in one, and this window keeps the one it showed. A file opened into a
    /// window showing an untouched new presentation (no file, no change) takes that window itself
    /// and the blank one goes, as Office replaces its untouched Presentation1; New beside an
    /// untouched one opens a window all the same (Presentation2 beside Presentation1). A file
    /// also open in another window brings that window forward instead. Several presentations in
    /// one session (recovered files, the command line) spread over windows in their order.
    pub(crate) fn settle_documents(&mut self) {
        if !self.session.windows.host {
            return;
        }
        while self.session.documents().len() > 1 {
            let n = self.session.documents().len();
            let extra = 1;
            let path = self.session.documents().get(extra).and_then(|d| d.path.clone());
            if let Some(open) = path.as_deref().and_then(|p| self.window_showing(p)) {
                // The copy just read is dropped (a fresh presentation, with no recovery entry).
                let _ = self.session.take_document(extra);
                self.window_requests.push(windows::WindowRequest::Focus(open));
                continue;
            }
            let blank = self.session.documents().first().is_some_and(|d| d.path.is_none() && !d.is_dirty());
            if n == 2 && path.is_some() && blank {
                self.session.close_document(0);
                continue;
            }
            let Some(doc) = self.session.take_document(extra) else { return };
            self.window_requests.push(windows::WindowRequest::Adopt(Box::new(doc)));
            // The window keeps its own presentation and comes back from the File tab, as opening
            // from the Open dialog does.
            self.backstage = None;
        }
    }

    /// The other window already showing the file at `path`, if any (the paths as given, or the
    /// same file by its real path).
    pub(crate) fn window_showing(&self, path: &str) -> Option<u64> {
        if !self.session.windows.host {
            return None;
        }
        let real = std::fs::canonicalize(path).ok();
        self.session
            .windows
            .others()
            .find(|w| {
                w.path.as_deref().is_some_and(|p| p == path || real.as_ref().is_some_and(|r| std::fs::canonicalize(p).ok().as_ref() == Some(r)))
            })
            .map(|w| w.id)
    }

    /// Whether this window may take a new or opened presentation itself: it shows nothing, or an
    /// untouched new presentation (no file, no unsaved change).
    pub fn window_reusable(&self) -> bool {
        match self.session.documents() {
            [] => true,
            [d] => d.path.is_none() && !d.is_dirty(),
            _ => false,
        }
    }

    /// The window's close button: with unsaved changes the window asks first (Save, Don't Save,
    /// Cancel) and may not close yet; true once it may.
    pub fn close_requested(&mut self) -> bool {
        if self.quit_confirmed {
            return true;
        }
        if self.session.documents().iter().any(|d| d.is_dirty()) {
            self.show = None;
            if self.dialog.as_ref().is_none_or(|d| d.id != "quit") {
                self.dialog = Some(dialogs::Dialog::new("quit"));
            }
            return false;
        }
        true
    }

    /// The name the window shows: the presentation's, else the app's.
    pub fn title_stem(&self) -> String {
        self.session.active().map(|d| d.title()).unwrap_or_else(|| crate::APP_NAME.into())
    }

    /// The window's title: the presentation's name, numbered when several windows show it
    /// (`Presentation1:2`), and the app's name after it.
    pub fn window_title(&self) -> String {
        let stem = self.title_stem();
        match (self.session.active(), self.view_number) {
            (Some(_), Some(n)) => format!("{stem}:{n} - {}", crate::APP_NAME),
            (Some(_), None) => format!("{stem} - {}", crate::APP_NAME),
            (None, _) => stem,
        }
    }

    /// Whether the window waits on work it polls for, so the host wakes it: a presentation being
    /// read in the background, control requests routed here.
    pub fn waits_on_background(&self) -> bool {
        self.opening.is_some() || !self.control_inbox.is_empty() || self.capture.pending() > 0
    }

    /// View › New Window: the windows on one presentation take turns holding the session and what
    /// goes with the presentation (a file being read, files picked for it); each window keeps its
    /// own interface state.
    pub(crate) fn swap_document_state(&mut self, other: &mut SlideApp) {
        std::mem::swap(&mut self.session, &mut other.session);
        std::mem::swap(&mut self.opening, &mut other.opening);
        std::mem::swap(&mut self.pending_files, &mut other.pending_files);
        std::mem::swap(&mut self.fonts_noted, &mut other.fonts_noted);
        std::mem::swap(&mut self.last_recovery, &mut other.last_recovery);
        // The session counts the compositions the application settled; what was last seen of
        // that count goes with it, so a settle the other window already announced to its input
        // method is not taken for a new one here.
        std::mem::swap(&mut self.ime_settles_seen, &mut other.ime_settles_seen);
        self.anim_preview = None;
        other.anim_preview = None;
    }

    /// This window hands its presentation to another window on it (View › New Window, the window
    /// host's turns): a composition in progress is settled here first, while this window holds the
    /// session, and counted as this window's own. Its input method holds the marked text, so its
    /// next pass interrupts it in this window's viewport and drops a pre-edit or commit still
    /// queued for it ([`Self::note_settled_composition`]), rather than composing it a second time
    /// after the text the settle kept. The window taking the presentation has nothing to interrupt:
    /// the count it receives with the session is already seen.
    pub(crate) fn settle_composition_for_handover(&mut self) {
        let before = self.session.composition_settles();
        self.session.settle_composition();
        let after = self.session.composition_settles();
        if after != before {
            self.ime_settles_seen = after;
            self.ime_settle_owed = true;
        }
    }

    /// The app moved to another window (the first window took over a closed window's
    /// presentation): what was its old window's is reset.
    pub(crate) fn rehomed(&mut self) {
        self.canvas_rect = None;
        self.canvas_region = None;
        self.slide_rect = None;
        self.thumb_drag = None;
        self.restyle = true;
    }

    /// Take the settings the user changed in another window ([`windows::Host`]): the appearance,
    /// the interface size and language, the recent list, the Quick Access Toolbar, smart guides,
    /// the editing preferences. What is this window's own stays: its ribbon tab, view, zoom, panes.
    pub fn adopt_settings(&mut self, ui: &UiState, prefs: &deckcraft_engine::Prefs) {
        if self.ui.brightness != ui.brightness {
            self.ui.brightness = ui.brightness;
            self.restyle = true;
        }
        self.ui.ui_scale = ui.ui_scale;
        self.ui.language = ui.language.clone();
        self.ui.recent = ui.recent.clone();
        self.ui.qat = ui.qat.clone();
        self.ui.smart_guides = ui.smart_guides;
        self.ui.presenter_view = ui.presenter_view;
        self.session.prefs = prefs.clone();
    }

    /// The settings every window shares, as text, to notice a change ([`Self::adopt_settings`]).
    pub(crate) fn shared_settings(&self) -> String {
        let u = &self.ui;
        serde_json::to_string(&json!({
            "brightness": u.brightness,
            "uiScale": u.ui_scale,
            "language": u.language,
            "recent": u.recent,
            "qat": u.qat,
            "smartGuides": u.smart_guides,
            "presenterView": u.presenter_view,
            "prefs": self.session.prefs,
        }))
        .unwrap_or_default()
    }

    /// Take the interface faces another window borrowed ([`windows::Host`] pools them), so the
    /// fonts this window installs cover every window's text. They are installed in this
    /// window's next UI pass ([`Self::ui_setup`]), as the faces it borrows itself are
    /// ([`uifonts::UiFonts::apply`]): the first install's discarded pass would drop the input
    /// that pass carries.
    pub(crate) fn adopt_faces(&mut self, faces: &[std::sync::Arc<deckcraft_fonts::FontFace>]) {
        if self.ui_fonts.adopt(faces) {
            self.faces_adopted = true;
        }
    }
}
