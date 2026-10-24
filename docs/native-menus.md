# Native macOS menu bar

> **Last reviewed:** 2026-10-25 · **Last updated:** 2026-10-25 · **Change:** major (first version: the menu bar adapted from WordCraft's, on the window host, with PowerPoint for Mac's menus) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

On macOS the desktop app installs the menu bar PowerPoint for Mac has at the top of the screen,
alongside the ribbon (never replacing it): DeckCraft, File, Edit, View, Insert, Format, Arrange,
Tools, Slide Show, Window and Help, in PowerPoint's order. Every item runs the same command the
ribbon does. Windows, Linux/BSD and the web keep the ribbon alone, as PowerPoint for Windows has no
menu bar.

| Menu | Items |
|---|---|
| DeckCraft | About DeckCraft, Preferences… (⌘,; macOS retitles the row Settings… on recent systems), Services, Hide DeckCraft (⌃⌘H: ⌘H is Replace's), Hide Others (⌥⌘H), Show All, Quit DeckCraft (⌘Q). macOS titles this menu after the bundle |
| File | New Presentation (⌘N), Open… (⌘O), Open Recent ▸, Close (⌘W), Save (⌘S), Save As… (⇧⌘S), Export…, Print… (⌘P), Properties |
| Edit | Undo (⌘Z), Redo (⌘Y), Cut, Copy, Paste (⌘X ⌘C ⌘V), Paste and Match Formatting (⇧⌘V), Paste Special… (⌥⌘V), Select All (⌘A), Duplicate (⌘D), Delete Slide, Find… (⌘F), Replace… (⌘H) |
| View | Normal, Slide Sorter, Notes Page, Outline View, Reading View, Slide Show (checked); Slide Master, Handout Master, Notes Master (checked); Ruler, Gridlines (⇧F9), Guides (⌥F9), Grid Options… (checked); Zoom…, Zoom In (⌘=), Zoom Out (⌘-), Fit to Window (⌘0) |
| Insert | New Slide, Duplicate Slide; Section, Text Box, Vertical Text Box, WordArt, Date and Time, Slide Number; Table…, Chart, Picture from File…, Photo Album…, Edit Photo Album…, Audio from File…, Video from File… |
| Format | Font…, Paragraph…, Bold, Italic, Underline (⌘B ⌘I ⌘U, checked); Align Left, Center, Align Right, Justify; Size and Position…, Right-to-Left Text Direction |
| Arrange | Bring to Front, Send to Back, Bring Forward, Send Backward; Group (⌥⌘G), Ungroup (⇧⌥⌘G), Regroup (⌥⌘J); Rotate Left 90°, Rotate Right 90°, Flip Horizontal, Flip Vertical |
| Tools | Spelling…, Thesaurus…, Language…; AutoCorrect Options… |
| Slide Show | Play from Start (⇧⌘↩), Play from Current Slide (⌘↩); Hide Slide, Set Up Show… |
| Window | Minimize (⌘M), Zoom; New Window, Arrange All, Cascade; Next Window (⌘`); Bring All to Front. Then what macOS adds to an app's windows menu (Fill, Center, Move & Resize ▸, Full Screen Tile ▸, Remove Window from Set, Move to iPad) and the open presentation windows, the one in front checked. Minimize and Zoom are the app's own rows, not AppKit's `performMiniaturize:`/`performZoom:` ones, because macOS puts its tiling rows right after a `performZoom:` row, where PowerPoint has them after Bring All to Front; so those two rows have no system icon |
| Help | Command Palette (⇧⌘P), Community Links (registered as the help menu, so macOS's menu search works) |

The menus are one list, `menus::menu_tree()` in `crates/ui-egui/src/menus.rs`, which the command
palette and the parity catalogue also use; the app menu, File › Open Recent and the Window menu's
Minimize, Zoom and Bring All to Front are added when the native bar is built. A row `dialog:<name>`
in the tree opens that dialog (Language…, AutoCorrect Options…). The documents of
[ui-parity.md](ui-parity.md) list what PowerPoint's menus hold that these don't yet.

## Command routing

`crates/ui-egui/src/native_menu.rs` owns a platform-free menu model (`MenuBar`, `Node`, `Item`,
`Chord`) built from the window in use (`menu_bar`): labels come from the commands in the interface
language (`menus::menu_label`), key equivalents from the commands' own shortcuts (engine specs, UI
commands and the keys the UI handles itself), enabled state from the ribbon's rule
(`ribbon::enabled`, a presentation for the UI commands that act on one), checkmarks from the view
state and the formatting state the ribbon reads (`format.state`). The desktop app injects
`apps/deckcraft/src/mac_menu.rs`, a macOS-only `muda` backend; the engine and the other platforms
have no AppKit dependency.

The menu is one for the process while the app has a window per presentation (`windows::Host`):
the host builds it from the window in use, runs its items in that window's frame, and syncs its
enabled and checked state after that window's frame. A menu event wakes the root viewport, which
asks the window in use to repaint and take it. Minimize minimizes the window in use; Quit asks
about unsaved changes in every window in turn (Cancel keeps the app open) and never goes through
AppKit's own Quit selector, which would ask nothing.

Menu actions use `SlideApp::run`, keeping the user dialogs and system clipboard handling, and
return to the slide from the File tab as the ribbon does. A dialog, the command palette or a
running slide show disables the presentation's rows, which are checked again when queued actions
run (a click can open a dialog that a click queued in the same frame must respect); a dialog also
disables Quit. Focused egui text fields receive their own copy, cut, paste, select-all, undo and
redo events, including in dialogs; those do not edit the slide. While a system file dialog is
open (a modal panel that blocks the app's frames) the Edit rows are AppKit's responder-chain items
(`mac_menu::with_native_editing`), so the panel's own fields keep ⌘C and ⌘V, and ours return when
it closes.

Key equivalents use only Command/Control shortcuts and function keys; ordinary typing and Option
characters are left to the editor (the File tab's Alt+F is not a menu key). A key equivalent
AppKit took is given back to egui as the key press it was, placed before its real key-up, which
winit still delivers, so DeckCraft's own dispatch (the UI's shortcuts, the engine's keys, a text
field's editing keys) runs exactly as for a typed shortcut; ⌘C, ⌘X and ⌘V become the Copy, Cut and
Paste events egui-winit makes of them, with the clipboard's text read through
`Services::clipboard_text`. The app's own rows (Hide, Minimize, Quit, Preferences) take a Mac key
only when no command claims it: Replace keeps ⌘H, so Hide is ⌃⌘H.

Enabled state, labels and checkmarks update in place; the tree is rebuilt only when its shape,
language or shortcuts change. If the menu bar cannot be installed, the ribbon alone remains.

## Provenance and checks

The model, backend and AppKit test are adapted from WordCraft's (storytold/wordcraft d2d407bc),
which adapted storytold/wordcraft#985 (YTwsy), which adapted
[PhotoCraft at 67e4b8a](https://github.com/storytold/photocraft/tree/67e4b8a499b55ef728b1da700005382c45e4677d);
the copied portions are covered by the MIT notice in `NOTICE`.

```sh
cargo test -p deckcraft-ui-egui --lib native_menu
cargo test -p deckcraft --test mac_menu_appkit
```

The UI tests (13) cover the layout and that every item is a registered command, chord parsing,
routing of clicks and key equivalents, checkmarks and enabled state, modal races, text fields,
paste and copy, Quit's prompt (and with a window host, that Quit is the host's), the recent files,
Minimize, Zoom and Hide, and a running slide show. The AppKit test runs on the main thread
without desktop keystrokes: it installs the real menu bar, reads NSMenu back (PowerPoint for Mac's
eleven menus, the Window menu's items, the windows and help menu registrations), performs a key
equivalent (⇧F9) and a click, checks a dialog disabling rows in place, the Edit rows' swap around a
file dialog, and the Japanese Window title. It is a no-op on other platforms.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-25 | major | First version, adapted from WordCraft's menu bar, on the window host: PowerPoint for Mac's eleven menus from `menu_tree()`, with a Tools menu and the Window menu's system rows |
