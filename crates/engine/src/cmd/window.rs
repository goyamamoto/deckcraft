//! View › Window: the presentation windows (`window.next`, `view.arrangeAll`, `view.cascade`,
//! `view.switchWindows`).
//!
//! The session owns no window: the desktop app does, one per presentation
//! (`deckcraft_ui_egui::windows`), and reports the open ones in [`Session::windows`] before each
//! frame. These commands check their parameters against that list and leave a
//! [`UiRequest::Window`] the front end carries out. Without a window host (headless sessions, the
//! browser, the CLI) they are disabled, except Next Window, which then cycles the session's own
//! presentations as before.

use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, usize_param};
use crate::{Result, Session, UiRequest};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            noundo "window.next",
            "Next Window",
            ["Window"],
            Some("Cmd+`"),
            "{window?: id or 1-based number in the Switch Windows list (desktop app) | index?: presentation index (no window host)} → {window}",
            next_enabled,
            next_window
        ),
        cmd!(query "view.arrangeAll", "Arrange All", ["View", "Window"], None, "{} → {windows}", needs_host, arrange_all),
        cmd!(query "view.cascade", "Cascade", ["View", "Window"], None, "{} → {windows}", needs_host, cascade),
        cmd!(
            query "view.switchWindows",
            "Switch Windows",
            ["View", "Window"],
            None,
            "{window: id or 1-based number in the Switch Windows list} → {window}",
            needs_host,
            switch_windows
        ),
    ]
}

/// The View › Window commands work in the desktop app, which has windows ([`Session::windows`]).
fn needs_host(s: &Session) -> std::result::Result<(), String> {
    if s.windows.host { Ok(()) } else { Err("only in the desktop app, which has windows".into()) }
}

/// Next Window cycles the windows (any of them, also one showing nothing) or, without a window
/// host, the session's presentations.
fn next_enabled(s: &Session) -> std::result::Result<(), String> {
    if s.windows.host { Ok(()) } else { has_doc(s) }
}

/// The window `p["window"]` names, checked against the host's list.
fn window_param(s: &Session, p: &Value, cmd: &str) -> Result<u64> {
    let n = p
        .get("window")
        .and_then(Value::as_u64)
        .ok_or_else(|| bad(cmd, "`window` (a window id, or its number in the Switch Windows list) is required"))?;
    s.windows.resolve(n).ok_or_else(|| bad(cmd, format!("no window {n} is open")))
}

fn ask(s: &mut Session, action: &str, window: Option<u64>) {
    s.ui_requests.push(UiRequest::Window { action: action.into(), window });
}

fn next_window(s: &mut Session, p: &Value) -> Result<Value> {
    if s.windows.host {
        let to = match p.get("window") {
            Some(_) => Some(window_param(s, p, "window.next")?),
            None => s.windows.next(),
        };
        if let Some(to) = to.filter(|to| *to != s.windows.this) {
            ask(s, "switch", Some(to));
            return Ok(json!({"window": to}));
        }
        return Ok(json!({"window": s.windows.this}));
    }
    let n = s.documents().len();
    if n == 0 {
        return Ok(json!({"window": 0}));
    }
    let i = usize_param(p, "index").unwrap_or_else(|| (s.active_index().unwrap_or(0) + 1) % n);
    s.set_active(i.min(n - 1));
    Ok(json!({"window": i.min(n - 1)}))
}

fn arrange_all(s: &mut Session, _p: &Value) -> Result<Value> {
    ask(s, "arrangeAll", None);
    Ok(json!({"windows": s.windows.count()}))
}

fn cascade(s: &mut Session, _p: &Value) -> Result<Value> {
    ask(s, "cascade", None);
    Ok(json!({"windows": s.windows.count()}))
}

fn switch_windows(s: &mut Session, p: &Value) -> Result<Value> {
    let to = window_param(s, p, "view.switchWindows")?;
    ask(s, "switch", Some(to));
    Ok(json!({"window": to}))
}
