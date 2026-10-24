# Control protocol

Start the app with `--control <port>`. The server listens on `127.0.0.1` only and speaks JSON
lines: one request object per line, one reply per line.

A line that is not a JSON object with a string `method` (an HTTP request, text that isn't JSON,
bytes that aren't UTF-8) gets one error reply and the server closes the connection; nothing sent
after it runs. Blank lines are skipped.

The connected MCP client requires the reply's numeric `id` to match its request, `ok` to be a
boolean, and exactly one of `result` (success) or a string `error` (failure). It caps each reply at
16 MiB including its terminating newline, accepts CRLF, and rejects EOF inside a frame. Invalid
replies invalidate the connection and leave the command's outcome unknown; the client does not
replay it. Unsolicited bytes already buffered between calls force a reconnect before sending.

The server bounds what a client can hold: 16 connections at once (more get `{"ok":false,"error":"busy: …"}`
and are closed), 16 MiB per request line (longer closes the connection), 64 queued requests (more are
answered `busy`), 10 minutes idle and 30 seconds per blocked reply write before the socket is closed, and
60 seconds waiting for the UI (`"error":"timeout"`). A request still queued when that wait expires is
abandoned: the UI skips it, so a retry cannot run it twice. If the UI had already started it, the reply
says the outcome is unknown instead of `timeout`.

```json
{"id": 1, "method": "engine.execute", "params": {"command": "shape.insert", "params": {"preset": "roundRect", "rect": [100, 100, 240, 120], "text": "Hello"}}}
{"id": 1, "ok": true, "result": {"id": 258}}
```

Coordinates are slide points (1/72 inch; a 16:9 slide is 960 × 540) unless noted.

| Method | Params | What it does |
|---|---|---|
| `engine.execute` / `ui.menu.invoke` / `command` | `{command, params}` | Run any command (see `engine.commands`; `deckcraft-cli describe COMMAND` documents one) |
| `engine.commands` | — | Every command: id, label, menu path, shortcut, params doc, enabled / disabled reason |
| `document.inspect` | — | Slides, sections, layouts, shapes, selection |
| `ui.inspect` | — | UI state, active tool, window/canvas/slide rects, dialog, `home` (the File tab open over a presentation), `backstage` (its page, also the start screen), slide show state (`grid`, `zoomArm`, `zoom`, `blank`, `presenterPaused`, `presenterSecs`, `notesPx`, `custom`, `goto`; `cells` while See All Slides is open), documents, perf (fps, render ms) |
| `ui.inspect` | — | UI state, active tool, window/canvas/slide rects, dialog, `home` (the File tab open over a presentation), `backstage` (its page, also the start screen), slide show state, `animPreview` (canvas playback: slide, time, duration), documents, perf (fps, render ms) |
| `ui.tool.select` | `{tool, preset?}` | `select`, `text`, `shape` + preset, any preset name, `curve`, `freeform`, `scribble`, `pen`, `highlighter`, `eraser` |
| `ui.pointer` | `{events: [{kind: down\|drag\|up\|move\|doubleclick\|tripleclick\|contextclick, x, y, mods?}], mods?}` | Drive the active tool in slide points through the same code path as the mouse (`contextclick`: the right-button press that selects before the context menu) |
| `ui.key` / `ui.text` | `{key, shift?, alt?, cmd?}` / `{text}` | Keyboard input (also drives a running slide show: digits then Enter go to that slide, G is See All Slides, Z arms zoom, T pauses the presenter timer, R resets it, `[` and `]` change notes size, Esc clears a slide number, the grid and zoom before it leaves the show) |
| `ui.pointer` | `{events: [{kind: down\|drag\|up\|move\|doubleclick\|tripleclick, x, y, mods?}], mods?}` | Drive the active tool in slide points through the same code path as the mouse |
| `ui.key` / `ui.text` | `{key, shift?, alt?, cmd?}` / `{text}` | Keyboard input (also drives a running slide show; Escape stops the canvas animation preview) |
| `ui.pointer` | `{events: [{kind: down\|drag\|up\|move\|doubleclick\|tripleclick, x, y, mods?, pressure?}], mods?}` | Drive the active tool in slide points through the same code path as the mouse. `pressure` is `0..=1` for a pen; omit it for a mouse |
| `ui.key` / `ui.text` | `{key, shift?, alt?, cmd?}` / `{text}` | Keyboard input (also drives a running slide show) |
| `ui.ime` | `{preedit, active?: [start, end]}` / `{commit}` | An input method's composition through egui, as a real one arrives: shown in place on the slide or in a focused field; `{"preedit": ""}` cancels |
| `ui.move` / `ui.click` / `ui.drag` | screen points | Real egui pointer input — reaches every widget, menu and pane |
| `ui.set` | `{tab?, view?, pane?, notes?, zoom?, brightness?, ruler?, gridlines?, guides?, …}` | UI state (returns the full state) |
| `ui.dialog.open` / `ui.dialog.close` | `{id, fields?}` | Open or close a dialog by id; `fields` pre-fills its fields (About: `{"tab": "contributors"}` or `"models"`) |
| `ui.screenshot` | `{path?}` | PNG of the whole window (base64 when no path) |
| `ui.render` | `{path?, slide?, scale?}` | Render a slide headlessly |
| `ui.resize` / `ui.focus` | `{width, height}` / `{window?}` | Window control: resize the window that answers; bring a window forward (the one `window` names, else the one in use) |
| `ui.windows` | — | The open presentation windows: `active` (the window in use) and `windows` (`id`, `title`, `path`, `dirty`, `active`, `root`, and for View › New Window `document` and `viewNumber`), in the order they were opened |
| `show.start` / `show.end` | `{from?: slide index, custom?: name, reading?}` | Slide show. `custom` plays that custom show from its first slide and does not change Set Up. With no `from` and no `custom`, playback starts at the first slide of the show order. A `from` that is not a slide of the deck (negative, not a whole number, past the end) is refused; a hidden slide plays |
| `app.open` / `app.save` / `app.export` / `app.quit` | `{path, …}` | Files (export takes the `file.export` params, e.g. `{"path": "deck.pdf", "layout": "handouts", "perPage": 3}` or `{"path": "deck.gif", "width": "large", "secondsPerSlide": 3, "loop": false}`, or `{"path": "slide.png", "transparentBackground": true}` for a PNG without the slide background) |

The MCP server (`deckcraft-cli mcp`, see [mcp.md](mcp.md)) wraps the same methods for Claude
and other agents; `deckcraft-cli app --method METHOD [PARAMS]` sends one request from a shell.

## Windows

The desktop app opens each presentation in a window of its own (File › New and File › Open open a
new window; View › New Window opens a second window on the same presentation). Every request may
carry a `window` parameter: a window id or its 1-based number in the `ui.windows` list; without
one (or with `"active"`) the request goes to the window in use. Commands (`engine.execute`,
`engine.commands`, `document.inspect`, `app.open`, `app.save`, `app.export`) run at once with that
window's presentation, whether or not the window is on screen. `ui.inspect`, `ui.screenshot` and
the pointer and key methods run in the window's own frame and act on and answer from that window;
`ui.inspect` reports the window as `windowId` (and `viewNumber` for a second window on one
presentation). A window that is minimized or covered by other windows (a terminal over the whole
screen, say) gets no frame until it shows again, so such a request that named no window goes to
the first window while the one in use is hidden, and one naming a hidden window is refused at once
("window N is hidden…"): bring it forward with `ui.focus` first.
`ui.windows`, `ui.focus` and `app.quit` are the host's own: `app.quit` closes every window without
asking (`{"save": true}` saves each presentation first and fails if one can't be).
The window commands are ordinary commands: `view.newWindow`, `view.arrangeAll`, `view.cascade`,
`view.switchWindows {window}` and `window.next {window?}`.
