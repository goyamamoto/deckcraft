//! DeckCraft desktop app.
//!
//! Usage: `deckcraft [--control <port>] [--sample] [--show] [files…]`
//! A PowerPoint Show (`.ppsx`) given last starts its slide show, as a double-click does in PowerPoint.
//! `deckcraft --help` prints the flags and exits. `deckcraft --version` prints the version and exits.
//!
//! `--control <port>` (or `DECKCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"ui.inspect","params":{}}` → `{"id":1,"ok":true,"result":…}`.
//! See `deckcraft_ui_egui::control` for the methods.
//!
//! `log` records go to standard error and `<settings dir>/logs/deckcraft.log` (`RUST_LOG` sets the
//! levels); see [`logging`].
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod apple_events;
mod audio;
mod control_server;
#[cfg(all(unix, not(target_os = "macos")))]
mod display;
#[cfg(any(target_os = "windows", test))]
mod graphics;
mod logging;
#[cfg(target_os = "macos")]
mod mac_menu;
#[cfg(not(target_arch = "wasm32"))]
mod pasteboard;
#[cfg(target_os = "macos")]
mod screens;

use deckcraft_engine::Session;
use deckcraft_ui_egui::{Services, SlideApp, windows::Host};

/// The eframe app: a shim over the window host ([`Host`], one `SlideApp` per presentation window)
/// and, on macOS, the presentations opened from Finder and quit requests (see `apple_events`).
struct App(#[cfg(target_os = "macos")] fmv_macos_events::Inbox);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        apple_events::poll(&self.0, ctx);
        Host::with(|h| h.logic(egui::ViewportId::ROOT, ctx));
        // The last window was allowed to close (the host asked about unsaved changes first,
        // `SlideApp::close_requested`), or every window has: the app ends. eframe ends it when the
        // root viewport's close goes uncancelled, which the host leaves be once it is done.
        if Host::with(|h| h.exit_requested()) == Some(true) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    /// Forward native raw input to the window host once per pass: it goes to the window the input
    /// is for (`Host::raw_input_hook`), after the macOS adjustments below.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // macOS: egui draws a text field's caret only while its viewport has the focus, and the
        // focus event winit sends as the first window opens can go missing (the caret then showed
        // only once another window had taken the focus and given it back). AppKit knows better:
        // the app is active with a key window, and this viewport is the window in use.
        #[cfg(target_os = "macos")]
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            let ns = objc2_app_kit::NSApplication::sharedApplication(mtm);
            let key = ns.isActive() && ns.keyWindow().is_some();
            let vp = raw.viewport_id;
            let in_use = Host::with(|h| {
                let this = h.window_of_viewport_mut(vp).map(|w| w.id);
                this.is_some() && this == h.active().map(|w| w.id)
            })
            .unwrap_or(false);
            if in_use && raw.focused != key {
                log::debug!("viewport focus: egui had {}, AppKit says {key}", raw.focused);
                raw.focused = key;
            }
        }
        // macOS: the committed text the input method asked to replace with this frame's
        // composition or commit (再変換), from the vendored winit (`deckcraft_ui_egui::ime`).
        // It belongs to this frame's events, which winit queues with it: a frame without them
        // drops it, so a range left by one that never reached its commit can't make a later
        // commit elsewhere (a full-width space, the Character Viewer) replace unrelated text.
        // The input method talks to the focused window, whose input this is: another window's
        // frame (one playing a video, say) running first leaves the range alone.
        #[cfg(target_os = "macos")]
        {
            let ime = raw.events.iter().any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_))));
            if ime || raw.focused {
                let named = winit::platform::macos::take_ime_replacement_range();
                let vp = raw.viewport_id;
                Host::with(|h| {
                    if let Some(w) = h.window_of_viewport_mut(vp) {
                        w.app.ime.replace = named.filter(|_| ime);
                    }
                });
            }
        }
        Host::with(|h| h.raw_input_hook(ctx, raw));
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Host::with(|h| h.ui(egui::ViewportId::ROOT, ui));
    }
    fn on_exit(&mut self) {
        Host::with(|h| {
            if let Some(w) = h.active() {
                save_prefs(&w.app);
            }
        });
        // macOS (#279): end the process here, before eframe tears the window down. AccessKit's
        // macOS adapter subclasses the window's content view and restores its class on teardown;
        // AppKit's Touch Bar support observes that view (KVO), so in the next display cycle
        // `-[_NSTouchBarFinderObservation invalidate]` throws "not registered" and the quit ends
        // in a crash report. eframe calls `on_exit` on every way of quitting (⌘Q, the close
        // button, `app.quit`), once the unsaved-changes prompt has let the window close, and
        // nothing after it is needed: the preferences are saved and the log file is unbuffered.
        // `on_exit` only runs once the app exists, never on a failed start. Same as GridCraft #117.
        #[cfg(target_os = "macos")]
        std::process::exit(0);
    }
}

/// macOS: what the input method may read of the focused window's slide text after its frame (the
/// caret's paragraph and the selection), so it can reconvert committed text and give live
/// conversion its context, as Office for Mac lets it (observed in Word for Mac). The store in the
/// vendored winit is one for the process, so only the focused window writes it: the input method
/// talks to that window alone.
#[cfg(target_os = "macos")]
fn report_ime_surrounding(app: &SlideApp, focused: bool) {
    if !focused {
        return;
    }
    winit::platform::macos::set_ime_surrounding_text(
        app.ime.surrounding.as_ref().map(|s| winit::platform::macos::ImeSurroundingText { text: s.text.clone(), selected: s.selected.clone() }),
    );
}

/// The per-user settings directory: `ui.json`, `prefs.json` and `logs/` live here.
fn config_dir() -> Option<std::path::PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support/DeckCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("DeckCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join("deckcraft"))
    }
}

fn load_prefs(app: &mut SlideApp) {
    if std::env::var_os("DECKCRAFT_NO_PREFS").is_some() {
        return;
    }
    let Some(dir) = config_dir() else { return };
    let (ui, prefs) = deckcraft_ui_egui::settings::load(&dir);
    if let Some(ui) = ui {
        app.ui = ui;
    }
    if let Some(prefs) = prefs {
        app.session.prefs = prefs;
    }
}

fn save_prefs(app: &SlideApp) {
    if std::env::var_os("DECKCRAFT_NO_PREFS").is_some() {
        return;
    }
    if let Some(dir) = config_dir() {
        deckcraft_ui_egui::settings::save(&dir, &app.ui, &app.session.prefs);
    }
}

/// A blocking system file dialog has Cocoa text fields: while it is open, the Edit menu's rows are
/// AppKit's own, so the dialog's fields keep ⌘C and ⌘V (`mac_menu::with_native_editing`).
fn with_native_editing<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(target_os = "macos")]
    {
        mac_menu::with_native_editing(f)
    }
    #[cfg(not(target_os = "macos"))]
    {
        f()
    }
}

fn services() -> Services {
    Services {
        pick_open: Some(Box::new(|purpose: &str| {
            let d = rfd::FileDialog::new();
            let d = match purpose {
                "picture" => d.add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"]),
                "audio" => d.add_filter(
                    "Audio",
                    &["wav", "mp3", "m4a", "m4b", "aac", "flac", "ogg", "oga", "opus", "aif", "aiff", "aifc", "caf", "wma", "weba", "mka"],
                ),
                "video" => d.add_filter("Video", &["mp4", "m4v", "mov", "webm", "mkv", "wmv"]),
                "theme" => d.add_filter("Themes and presentations", &["thmx", "pptx", "potx", "ppsx"]).add_filter("Office Theme", &["thmx"]),
                _ => d
                    .add_filter("Presentations", &["deckcraft", "pptx", "potx", "ppsx"])
                    .add_filter("DeckCraft", &["deckcraft"])
                    .add_filter("PowerPoint", &["pptx", "potx", "ppsx"])
                    .add_filter("Outline", &["txt", "md"]),
            };
            with_native_editing(|| d.pick_file()).map(|p| p.to_string_lossy().to_string())
        })),
        pick_save: Some(Box::new(|name: &str| {
            with_native_editing(|| rfd::FileDialog::new().set_file_name(name).save_file()).map(|p| p.to_string_lossy().to_string())
        })),
        read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))),
        write: Some(Box::new(|p: &str, b: &[u8]| std::fs::write(p, b).map_err(|e| e.to_string()))),
        clipboard_image: Some(Box::new(pasteboard::image_png)),
        clipboard_html: Some(Box::new(pasteboard::html)),
        clipboard_text: Some(Box::new(pasteboard::text)),
        clipboard_set_html: Some(Box::new(pasteboard::set_html)),
        // Media is mixed and clocked by the output device; `DECKCRAFT_NO_AUDIO` plays silently.
        audio_out: std::env::var_os("DECKCRAFT_NO_AUDIO")
            .is_none()
            .then(|| Box::new(audio::CpalOut::default()) as Box<dyn deckcraft_media::AudioOut>),
        ..Default::default()
    }
}

const APP_ID: &str = "ai.storyteller.deckcraft";

const USAGE: &str = "\
Usage: deckcraft [--control PORT] [--sample] [--show] [FILE...]

  --control PORT  JSON control server on 127.0.0.1:PORT
  --sample        open the sample presentation
  --show          start the slide show
  --help, -h      print this help and exit
  --version, -V   print the version and exit

A PowerPoint Show (.ppsx) given last starts its slide show.
";

enum Startup {
    Help,
    Version,
    Run { control_port: Option<u16>, files: Vec<String>, sample: bool, show: bool },
}

/// `--help` and `--version` leave before the window and the log file.
fn startup(mut control_port: Option<u16>, args: impl IntoIterator<Item = String>) -> Startup {
    let mut files = Vec::new();
    let mut sample = false;
    let mut show = false;
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--show" => show = true,
            "--version" | "-V" => return Startup::Version,
            "--help" | "-h" => return Startup::Help,
            // The process serial number older macOS adds when Finder launches the app.
            a if a.starts_with("-psn_") => {}
            _ => files.push(a),
        }
    }
    Startup::Run { control_port, files, sample, show }
}

/// The East Asian editing language: the first Japanese, Chinese or Korean language among the
/// system's preferred languages (an English Mac in Japan lists `en-JP`, then `ja-JP`), as
/// PowerPoint takes Japanese editing from them; none without one. `DECKCRAFT_EAST_ASIAN_LANGUAGE`
/// (a language tag such as `ja-JP`, or `none`) overrides it, for tests and CI.
fn east_asian_editing() -> Option<deckcraft_engine::CjkLocale> {
    if let Ok(tag) = std::env::var("DECKCRAFT_EAST_ASIAN_LANGUAGE") {
        return deckcraft_engine::CjkLocale::from_lang(&tag);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let tags: Vec<String> = sys_locale::get_locales().take(16).collect();
        deckcraft_engine::east_asian_editing_language(tags.iter().map(String::as_str))
    }
    #[cfg(target_arch = "wasm32")]
    None
}

fn app_icon() -> Option<egui::IconData> {
    let png: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.deckcraft.png");
    eframe::icon_data::from_png_bytes(png).map_err(|e| log::warn!("app icon: {e}")).ok()
}

/// Install logging, parse startup arguments and environment, launch the native app, and
/// propagate an eframe startup failure.
fn main() -> eframe::Result {
    // First, so every start-up record (and the engine's panic hook, installed with the first
    // Session) is captured; see `logging`.
    let logger = logging::install();
    let (control_port, files, sample, show) =
        match startup(std::env::var("DECKCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok()), std::env::args().skip(1)) {
            Startup::Help => {
                print!("{USAGE}");
                return Ok(());
            }
            Startup::Version => {
                println!("deckcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            Startup::Run { control_port, files, sample, show } => (control_port, files, sample, show),
        };
    log::info!("DeckCraft {} ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    // The log file lives under the settings directory; opened after the arguments, so `--help` and
    // `--version` leave no file behind. Records logged until now are written to it first.
    if let Some(logger) = logger {
        match config_dir().map(|d| logger.attach_dir(&d.join(logging::LOG_DIR))) {
            Some(Ok(path)) => log::info!("log file {}", path.display()),
            Some(Err(e)) => log::warn!("no log file: {e}"),
            None => {
                logger.stderr_only();
                log::warn!("no log file: no settings directory (HOME, XDG_CONFIG_HOME or APPDATA is not set)");
            }
        }
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DeckCraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true)
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };
    options.viewport = options.viewport.with_app_id(APP_ID);
    if let Some(icon) = app_icon() {
        options.viewport = options.viewport.with_icon(icon);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    let x11 = match display::choose_from_env() {
        display::Choice::X11 => true,
        display::Choice::Default => false,
        display::Choice::None => {
            // winit's own error says little to someone who started DeckCraft from a console.
            log::error!("no display: DISPLAY and WAYLAND_DISPLAY are not set");
            eprintln!("DeckCraft needs a graphical session: neither DISPLAY nor WAYLAND_DISPLAY is set.");
            false
        }
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    if x11 {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        }));
    }
    // Before eframe creates the wgpu instance: default Windows to DirectX 12 with FXC (see graphics.rs).
    #[cfg(target_os = "windows")]
    graphics::configure(&mut options, eframe::wgpu::Backends::from_env(), eframe::wgpu::Dx12Compiler::from_env());
    // Registered before the event loop starts, so it catches the Finder event that launched us as
    // well as later ones. Lives until the event loop returns; the app creator only borrows it.
    #[cfg(target_os = "macos")]
    let apple_events = apple_events::AppleEvents::install();
    #[cfg(target_os = "macos")]
    let apple_events = &apple_events;
    // New windows start from the first window's chrome (title, size, icon, hidden title bar).
    let template = options.viewport.clone();
    eframe::run_native(
        "DeckCraft",
        options,
        Box::new(move |cc| {
            // Once for the egui context, which every presentation window's viewport shares (the
            // hooks key their state by viewport).
            deckcraft_ui_egui::install_ime_pass_hooks(&cc.egui_ctx);
            // Japanese editing tags typed Japanese `ja-JP` and draws Han text without a language
            // the Japanese way.
            let east_asian = east_asian_editing();
            log::info!("East Asian editing language: {east_asian:?}");
            // The system region picks the measurement unit (centimetres in Japan, inches in the
            // United States) and the paper notes pages print on, as PowerPoint takes them.
            let region = deckcraft_ui_egui::units::system_region();
            log::info!("Region: {} ({} on {})", region.tag(), region.units().id(), region.paper_id());
            let recovery = std::env::var_os("DECKCRAFT_NO_RECOVERY").is_none();
            // One `SlideApp` per presentation window: the first one below, the rest as the host
            // opens windows, each with the same desktop services and a blank session. AutoRecover
            // writes every window's unsaved presentations to the one recovery folder.
            let new_app = move || {
                let mut app = SlideApp::new(Session::new(), services());
                app.session.set_east_asian_editing(east_asian);
                app.session.set_region(region.clone());
                if recovery {
                    app.session.recovery_dir = deckcraft_engine::recovery::default_dir();
                }
                app.integrated_titlebar = cfg!(target_os = "macos");
                app
            };
            let mut app = new_app();
            load_prefs(&mut app);
            // AutoRecover: reopen what a previous run left unsaved (each in a window of its own,
            // once the host runs), then keep it current.
            if recovery
                && let Ok(v) = app.session.execute("file.recovery.open", &serde_json::json!({}))
                && let Some(n) = v.get("opened").and_then(|o| o.as_array()).map(Vec::len).filter(|n| *n > 0)
            {
                app.set_status(format!("Recovered {n} unsaved presentation{} from the last session", if n == 1 { "" } else { "s" }));
            }
            if sample && let Err(e) = deckcraft_engine::sample::open_sample(&mut app.session) {
                log::error!("sample: {e}");
            }
            // The native menu bar (macOS): built from the first window's state, then kept to the
            // window in use by the host.
            #[cfg(target_os = "macos")]
            let menu = mac_menu::install(&cc.egui_ctx, &mut app);
            let mut host = Host::new(app, template, Box::new(new_app));
            #[cfg(target_os = "macos")]
            {
                host.work_area = Some(Box::new(screens::work_area));
                host.after_ui = Some(Box::new(report_ime_surrounding));
                if let Some(menu) = menu {
                    host = host.with_menu(menu);
                }
            }
            if let Some(port) = control_port {
                host = host.with_control(control_server::start(port, cc.egui_ctx.clone()));
            }
            // Each file in a window of its own, as PowerPoint opens them; the first takes the
            // blank window (`Host::open_paths`). A .ppsx opened last starts its slide show (#862),
            // in its window; File › Open opens a .ppsx for editing.
            let ppsx_last = files.last().is_some_and(|f| f.to_ascii_lowercase().ends_with(".ppsx"));
            host.open_paths(files, &cc.egui_ctx);
            if let Some(w) = host.active_mut() {
                if show {
                    w.app.start_show(0, false);
                } else if ppsx_last {
                    let from = w.app.show_start_index();
                    w.app.start_show(from, false);
                }
            }
            Host::install(host);
            Ok(Box::new(App(
                #[cfg(target_os = "macos")]
                apple_events.connect(&cc.egui_ctx),
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #279: on macOS the process ends inside `on_exit`, before eframe would tear the window down.
    /// The check runs in a child (this test binary again), since on macOS it ends the process.
    #[test]
    fn on_exit_ends_the_process_on_macos() {
        if std::env::var_os("DECKCRAFT_TEST_ON_EXIT").is_some() {
            Host::install(Host::new(
                SlideApp::new(Session::new(), Services::default()),
                egui::ViewportBuilder::default(),
                Box::new(|| SlideApp::new(Session::new(), Services::default())),
            ));
            let mut app = App(
                #[cfg(target_os = "macos")]
                fmv_macos_events::Inbox::default(),
            );
            println!("calling on_exit");
            eframe::App::on_exit(&mut app);
            println!("on_exit returned");
            return;
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::on_exit_ends_the_process_on_macos", "--nocapture"])
            .env("DECKCRAFT_TEST_ON_EXIT", "1")
            .env("DECKCRAFT_NO_PREFS", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{stdout}");
        assert!(stdout.contains("calling on_exit"), "the child didn't run the test: {stdout}");
        assert_eq!(stdout.contains("on_exit returned"), !cfg!(target_os = "macos"), "{stdout}");
    }

    #[test]
    fn help_exits_instead_of_opening_a_file() {
        assert!(matches!(startup(None, ["--help".to_string()]), Startup::Help));
        assert!(matches!(startup(None, ["-h".to_string()]), Startup::Help));
        assert!(matches!(startup(None, ["talk.pptx".to_string(), "--help".to_string()]), Startup::Help));
        assert!(matches!(startup(None, ["--version".to_string()]), Startup::Version));
        let launch = startup(Some(9), ["--sample".to_string(), "--control".to_string(), "4242".to_string(), "talk.pptx".to_string()]);
        assert!(matches!(launch, Startup::Run { .. }));
        if let Startup::Run { control_port, files, sample, show } = launch {
            assert_eq!(control_port, Some(4242));
            assert_eq!(files, ["talk.pptx".to_string()]);
            assert!(sample);
            assert!(!show);
        }
    }

    #[test]
    fn finder_process_serial_number_is_not_a_file() {
        let launch = startup(None, ["-psn_0_1234567".to_string(), "talk.pptx".to_string()]);
        assert!(matches!(launch, Startup::Run { .. }));
        if let Startup::Run { files, .. } = launch {
            assert_eq!(files, ["talk.pptx".to_string()]);
        }
    }

    /// The `<string>` values of every `<array>` that follows a `<key>{key}</key>` line.
    fn plist_strings(plist: &str, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut in_array = false;
        for line in plist.lines() {
            let line = line.trim();
            if line == format!("<key>{key}</key>") {
                in_array = true;
                continue;
            }
            if in_array {
                for part in line.split("<string>").skip(1) {
                    if let Some((s, _)) = part.split_once("</string>") {
                        out.push(s.to_string());
                    }
                }
                if line.ends_with("</array>") {
                    in_array = false;
                }
            }
        }
        out
    }

    /// Every desktop offers the same file types: macOS (`CFBundleTypeExtensions` and the native
    /// type's extension in `Info.plist.in`), Windows (`SupportedTypes` in `deckcraft.wxs`) and
    /// Linux (`MimeType` in the desktop file). A format added to one installer must reach the
    /// others, and each must be one Finder, Explorer or the file manager can hand to DeckCraft.
    #[test]
    fn every_desktop_offers_the_same_file_types() {
        let plist = include_str!("../../../packaging/macos/Info.plist.in");
        let wxs = include_str!("../../../packaging/windows/deckcraft.wxs");
        let desktop = include_str!("../../../packaging/linux/ai.storyteller.deckcraft.desktop");

        let mut mac = plist_strings(plist, "CFBundleTypeExtensions");
        // The native type is declared by its UTI; its extension is in the exported type.
        mac.extend(plist_strings(plist, "public.filename-extension").into_iter().filter(|e| e == "deckcraft"));
        mac.sort();
        mac.dedup();
        assert!(mac.iter().any(|e| e == "pptx") && mac.iter().any(|e| e == "deckcraft"), "Info.plist.in: {mac:?}");
        // Every document type names a content type too, so "Change All" in Finder's Get Info works.
        let names = plist.matches("<key>CFBundleTypeName</key>").count();
        let content_types = plist.matches("<key>LSItemContentTypes</key>").count();
        assert!(names > 0 && names == content_types, "{names} document types, {content_types} with LSItemContentTypes");

        let mut win: Vec<String> = wxs
            .lines()
            .filter(|l| l.contains("SupportedTypes\" Name=\"."))
            .filter_map(|l| l.split("Name=\".").nth(1)?.split('"').next().map(str::to_string))
            .collect();
        win.sort();

        let mime_ext = |m: &str| match m {
            "application/x-deckcraft" => Some("deckcraft"),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
            "application/vnd.openxmlformats-officedocument.presentationml.template" => Some("potx"),
            "application/vnd.openxmlformats-officedocument.presentationml.slideshow" => Some("ppsx"),
            _ => None,
        };
        let mut linux: Vec<String> = desktop
            .lines()
            .find_map(|l| l.strip_prefix("MimeType="))
            .unwrap_or_default()
            .split(';')
            .filter(|m| !m.is_empty())
            .map(|m| mime_ext(m).unwrap_or(m).to_string()) // an unknown MIME type shows up in the diff
            .collect();
        linux.sort();

        assert_eq!(mac, win, "macOS and Windows offer different file types");
        assert_eq!(mac, linux, "macOS and Linux offer different file types");
    }
}
