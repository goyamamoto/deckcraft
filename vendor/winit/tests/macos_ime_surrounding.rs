//! DeckCraft patch (`01-macos-ime-surrounding-text.md`): native AppKit driver for the text the app
//! reports around the caret. Each scenario runs in a fresh process on the main thread and calls
//! the real `WinitView` the way an input method does (`NSTextInputClient`), outside any key event.
//!
//! - `reported`: the view answers from the reported text, follows the input method's edits at
//!   once, hands the app the replacement ranges and never a W26 deletion; `unmarkText` commits;
//!   `reset_ime` and disallowing IME don't.
//! - `unreported`: with nothing reported, storytold's answers and its W26 deletion are unchanged.
//! - `transition`: the view's own document forgets the text when the app stops reporting, so a
//!   range named in the reported text's offsets deletes nothing.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::{Cell, RefCell};
    use std::process::Command;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use block2::{Block, RcBlock};
    use core_foundation::base::CFTypeRef;
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoopGetMain, CFRunLoopRef};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSAttributedString, NSNotFound, NSRange, NSString};
    use rwh_06::{HasWindowHandle, RawWindowHandle};
    use winit::application::ApplicationHandler;
    use winit::event::{Ime, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
    use winit::platform::macos::{
        ime_surrounding_text, set_ime_surrounding_text, take_ime_replacement_range,
        ActivationPolicy, EventLoopBuilderExtMacOS, ImeSurroundingText, WindowExtMacOS,
    };
    use winit::window::{Window, WindowId};

    extern "C" {
        fn CFRunLoopPerformBlock(run_loop: CFRunLoopRef, mode: CFTypeRef, block: &Block<dyn Fn()>);
    }

    fn later(action: impl FnOnce() + 'static) {
        let action = Cell::new(Some(action));
        let block = RcBlock::new(move || {
            if let Some(action) = action.take() {
                action();
            }
        });
        unsafe { CFRunLoopPerformBlock(CFRunLoopGetMain(), kCFRunLoopDefaultMode as _, &block) };
    }

    const NONE: NSRange = NSRange::new(NSNotFound as usize, 0);

    fn marked(view: &AnyObject, text: &str, selected: NSRange, replacement: NSRange) {
        let text = NSString::from_str(text);
        let _: () = unsafe {
            objc2::msg_send![view, setMarkedText: &*text
                selectedRange: selected
                replacementRange: replacement]
        };
    }

    fn inserted(view: &AnyObject, text: &str, replacement: NSRange) {
        let text = NSString::from_str(text);
        let _: () =
            unsafe { objc2::msg_send![view, insertText: &*text replacementRange: replacement] };
    }

    fn unmark(view: &AnyObject) {
        let _: () = unsafe { objc2::msg_send![view, unmarkText] };
    }

    fn selected_range(view: &AnyObject) -> (usize, usize) {
        let range: NSRange = unsafe { objc2::msg_send![view, selectedRange] };
        (range.location, range.length)
    }

    fn marked_range(view: &AnyObject) -> (usize, usize) {
        let range: NSRange = unsafe { objc2::msg_send![view, markedRange] };
        (range.location, range.length)
    }

    fn has_marked_text(view: &AnyObject) -> bool {
        unsafe { objc2::msg_send![view, hasMarkedText] }
    }

    /// The text the view answers for `range`, with the range it covers.
    fn substring(view: &AnyObject, range: NSRange) -> Option<(String, (usize, usize))> {
        let mut actual = NSRange::new(0, 0);
        let text: Option<Retained<NSAttributedString>> = unsafe {
            objc2::msg_send_id![view, attributedSubstringForProposedRange: range
                actualRange: &mut actual]
        };
        text.map(|text| (text.string().to_string(), (actual.location, actual.length)))
    }

    fn report(text: &str, selected: std::ops::Range<usize>) {
        set_ime_surrounding_text(Some(ImeSurroundingText { text: text.into(), selected }));
    }

    fn reported() -> (String, std::ops::Range<usize>) {
        let s = ime_surrounding_text().expect("text reported");
        (s.text, s.selected)
    }

    const NOT_FOUND: (usize, usize) = (NSNotFound as usize, 0);

    type Log = Rc<RefCell<Vec<String>>>;

    fn note(log: &Log, line: impl Into<String>) {
        log.borrow_mut().push(line.into());
    }

    fn drive(scenario: &str, view: &AnyObject, window: &Window, log: &Log) {
        match scenario {
            "reported" => {
                // Reconversion of the selected 日本: the view reads the selection and the text.
                report("a日本b", 1..3);
                assert_eq!(selected_range(view), (1, 2));
                assert_eq!(marked_range(view), NOT_FOUND);
                assert_eq!(substring(view, NSRange::new(0, 4)), Some(("a日本b".into(), (0, 4))));
                assert_eq!(substring(view, NSRange::new(3, 10)), Some(("b".into(), (3, 1))));
                assert_eq!(substring(view, NONE), None);
                note(log, "reconvert");
                marked(view, "にほん", NSRange::new(0, 3), NSRange::new(1, 2));
                assert_eq!(take_ime_replacement_range(), Some(1..3), "the app takes the range");
                assert_eq!(marked_range(view), (1, 3));
                assert_eq!(selected_range(view), (1, 3));
                assert_eq!(substring(view, NSRange::new(0, 5)), Some(("aにほんb".into(), (0, 5))));
                assert_eq!(reported(), ("aにほんb".into(), 1..4));
                inserted(view, "二本", NONE);
                assert!(!has_marked_text(view));
                assert_eq!(marked_range(view), NOT_FOUND);
                assert_eq!(selected_range(view), (3, 0));
                assert_eq!(reported(), ("a二本b".into(), 3..3));
                assert_eq!(take_ime_replacement_range(), None, "no range named by the commit");

                // Dismissed: 日本 is back and selected.
                report("a日本b", 1..3);
                note(log, "dismiss");
                marked(view, "にほん", NSRange::new(0, 3), NSRange::new(1, 2));
                marked(view, "", NSRange::new(0, 0), NONE);
                assert!(!has_marked_text(view));
                assert_eq!(reported(), ("a日本b".into(), 1..3));
                assert_eq!(selected_range(view), (1, 2));
                assert_eq!(take_ime_replacement_range(), Some(1..3));

                // Live conversion re-marks 日本ご, naming committed text before its marked text.
                report("a日本b", 3..3);
                note(log, "live");
                marked(view, "ご", NSRange::new(1, 0), NONE);
                assert_eq!(marked_range(view), (3, 1));
                assert_eq!(selected_range(view), (4, 0));
                marked(view, "にほんご", NSRange::new(4, 0), NSRange::new(1, 3));
                assert_eq!(marked_range(view), (1, 4));
                assert_eq!(reported(), ("aにほんごb".into(), 5..5));
                inserted(view, "日本語", NONE);
                assert_eq!(reported(), ("a日本語b".into(), 4..4));
                assert_eq!(take_ime_replacement_range(), Some(1..4));

                // A commit naming a range without marked text (press-and-hold, dictation).
                report("cafe", 4..4);
                note(log, "replace");
                inserted(view, "é", NSRange::new(3, 1));
                assert_eq!(take_ime_replacement_range(), Some(3..4));
                assert_eq!(reported(), ("café".into(), 4..4));

                // `unmarkText` commits the marked text as it stands.
                report("ab", 1..1);
                note(log, "unmark");
                marked(view, "かな", NSRange::new(2, 0), NONE);
                unmark(view);
                assert!(!has_marked_text(view));
                assert_eq!(reported(), ("aかなb".into(), 3..3));

                // `reset_ime` cancels without committing; the reported text keeps what was shown
                // until the app reports again.
                note(log, "reset");
                marked(view, "かな", NSRange::new(2, 0), NONE);
                window.reset_ime();
                assert!(!has_marked_text(view));
                assert_eq!(reported(), ("aかなかなb".into(), 5..5));

                // Disallowing IME discards the composition without committing it either.
                note(log, "disallow");
                marked(view, "に", NSRange::new(1, 0), NONE);
                window.set_ime_allowed(false);
                assert!(!has_marked_text(view));
                window.set_ime_allowed(true);
                note(log, "end");
                set_ime_surrounding_text(None);
            },
            "unreported" => {
                // storytold's document: the caret after the text inserted, W26's deletion.
                note(log, "plain");
                inserted(view, "abc", NONE);
                assert_eq!(selected_range(view), (3, 0));
                assert_eq!(substring(view, NSRange::new(0, 3)), Some(("abc".into(), (0, 3))));
                inserted(view, "é", NSRange::new(2, 1));
                assert_eq!(take_ime_replacement_range(), None, "nothing for the app");
                marked(view, "x", NSRange::new(1, 0), NONE);
                assert_eq!(marked_range(view), (3, 1));
                assert_eq!(selected_range(view), (4, 0));
                unmark(view);
                note(log, "end");
            },
            "transition" => {
                report("", 0..0);
                note(log, "reported");
                inserted(view, "abc", NONE);
                assert_eq!(reported(), ("abc".into(), 3..3));
                set_ime_surrounding_text(None);
                note(log, "unreported");
                // A range in the reported text's offsets: the view's own document has forgotten
                // the text, so nothing is deleted.
                inserted(view, "é", NSRange::new(2, 1));
                assert_eq!(take_ime_replacement_range(), None);
                assert_eq!(substring(view, NSRange::new(0, 3)), None);
                note(log, "end");
            },
            _ => unreachable!("{scenario}"),
        }
    }

    /// What the app saw, with the driver's step markers between.
    fn expected(scenario: &str) -> Vec<&'static str> {
        match scenario {
            "reported" => vec![
                "reconvert",
                "preedit にほん",
                "preedit ",
                "commit 二本 delete None",
                "dismiss",
                "preedit にほん",
                "preedit ",
                "live",
                "preedit ご",
                "preedit にほんご",
                "preedit ",
                "commit 日本語 delete None",
                "replace",
                "commit é delete None",
                "unmark",
                "preedit かな",
                "preedit ",
                "commit かな delete None",
                "reset",
                "preedit かな",
                "preedit ",
                "disallow",
                "preedit に",
                "disabled",
                "end",
            ],
            "unreported" => vec![
                "plain",
                "commit abc delete None",
                "commit é delete Some((1, 0))",
                "preedit x",
                "preedit ",
                "end",
            ],
            "transition" => {
                vec![
                    "reported",
                    "commit abc delete None",
                    "unreported",
                    "commit é delete None",
                    "end",
                ]
            },
            _ => unreachable!("{scenario}"),
        }
    }

    struct App {
        scenario: String,
        proxy: EventLoopProxy<()>,
        window: Option<Rc<Window>>,
        log: Log,
        done: Rc<Cell<bool>>,
    }

    impl ApplicationHandler<()> for App {
        fn user_event(&mut self, event_loop: &ActiveEventLoop, _: ()) {
            self.done.set(true);
            event_loop.exit();
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let window = Rc::new(
                event_loop.create_window(Window::default_attributes().with_visible(false)).unwrap(),
            );
            window.set_ime_allowed(true);
            let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() else {
                panic!("AppKit view required");
            };
            let view =
                unsafe { Retained::<AnyObject>::retain(handle.ns_view.as_ptr().cast()) }.unwrap();
            self.window = Some(Rc::clone(&window));
            let (scenario, proxy, log) =
                (self.scenario.clone(), self.proxy.clone(), Rc::clone(&self.log));
            later(move || {
                drive(&scenario, &view, &window, &log);
                later(move || proxy.send_event(()).unwrap());
            });
        }

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::Ime(Ime::Preedit(text, _)) => {
                    note(&self.log, format!("preedit {text}"))
                },
                WindowEvent::Ime(Ime::Commit(text)) => {
                    let delete = self
                        .window
                        .as_ref()
                        .unwrap()
                        .take_ime_delete_surrounding()
                        .map(|d| (d.before_bytes, d.after_bytes));
                    note(&self.log, format!("commit {text} delete {delete:?}"));
                },
                WindowEvent::Ime(Ime::Disabled) => note(&self.log, "disabled"),
                _ => {},
            }
        }
    }

    fn child(scenario: String) {
        let mut builder = EventLoop::builder();
        builder.with_activation_policy(ActivationPolicy::Accessory).with_default_menu(false);
        let event_loop = builder.build().unwrap();
        let done = Rc::new(Cell::new(false));
        let log = Log::default();
        let mut app = App {
            scenario: scenario.clone(),
            proxy: event_loop.create_proxy(),
            window: None,
            log: Rc::clone(&log),
            done: Rc::clone(&done),
        };
        event_loop.run_app(&mut app).unwrap();
        assert!(done.get(), "the driver didn't finish");
        let log = log.borrow();
        assert_eq!(*log, expected(&scenario), "{scenario}");
    }

    pub fn run() {
        if let Some(scenario) = std::env::args().nth(1).filter(|s| !s.starts_with('-')) {
            child(scenario);
            return;
        }
        for scenario in ["reported", "unreported", "transition"] {
            let mut child =
                Command::new(std::env::current_exe().unwrap()).arg(scenario).spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("native scenario timed out: {scenario}");
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            assert!(status.success(), "native scenario failed: {scenario}");
            println!("test macos_ime_surrounding::{scenario} ... ok");
        }
    }
}

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}
