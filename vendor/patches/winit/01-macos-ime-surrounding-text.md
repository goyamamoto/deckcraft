# macOS: the input method keeps its conversion and reads the text around the caret (再変換)

**Base:** storytold/winit `craft/0.30.13` at `a517b3846ee8457d84ee4975110bfda6797c44dc` (winit
0.30.13 + the craft suite's patches W1–W27, the pin of DeckCraft's upstream with egui 0.37).
**Commit:** `d354e8a83ad1b3fe453d2b8ada8829028590d891` on branch `deckcraft/ime-surrounding` ("D1"
in its `PATCHES.md`). **Files:** `src/platform/macos.rs`, `src/platform_impl/macos/mod.rs`,
`src/platform_impl/macos/view.rs`, new `src/platform_impl/macos/ime_surrounding.rs`.
**Guards in the fork:** `tests/ime_patches.rs` (`macos_reported_surrounding_text_answers_the_input_method`
and the `ime_surrounding` unit tests, which run on every platform) and the native AppKit driver
`tests/macos_ime_surrounding.rs`. **Guard here:** `apps/deckcraft/tests/it/winit_macos_ime_surrounding_text.rs`.
Ported from DeckCraft's vendored winit 0.30.13 patch of the same name (from WordCraft's), with the
parts storytold's branch already does dropped (below).

storytold's W22 gives input methods a caret: `ImeDocument`, the text typed since the caret last
moved, with the marked text after it. That serves press-and-hold and dictation next to the caret,
but it isn't the document: `attributedSubstringForProposedRange:actualRange:` knows only what was
typed since the last click, and `selectedRange` reports a collapsed caret, never a selection. So the
macOS Japanese input method still can't reconvert committed text (select it, press かな twice or
choose 再変換): it reads no selection and no text for it. Live conversion gets no context before the
last click either. Office for Mac answers all of these (observed in Word for Mac).

The patch adds a process-wide store the application fills once a frame (eframe never hands the
application a `Window`, and one window takes keyboard input at a time). The public API is the old
patch's, with the same meaning:

- `winit::platform::macos::ImeSurroundingText { text, selected }` and
  `set_ime_surrounding_text(Option<…>)`: the caret's paragraph and the selection in it, in UTF-16
  code units. `None` (the default) restores storytold's answers.
- `selectedRange` answers the reported selection — or, while there is marked text, the input
  method's own selection within it, offset by where the marked text lies.
- `attributedSubstringForProposedRange:actualRange:` answers the requested slice of the reported
  text, clamped, and fills `actualRange`.
- `markedRange` lies at the marked text's place in the reported text: where the range it replaced
  starts — the named replacement range (mapped through the marked text it replaces, which
  `setMarkedText:` reads at its old place before replacing it), else the marked text, else the
  selection.
- `setMarkedText:…replacementRange:`, and `insertText:replacementRange:` in each arm that delivers
  text — a commit of marked text, text an input method inserts without marking it (outside a key
  event, or other text than the key's), the key's own text when it is committed (below) — records
  a replacement range that isn't `NSNotFound`; `winit::platform::macos::take_ime_replacement_range()`
  hands it to the application, which selects that text before applying the `Ime::Preedit` or
  `Ime::Commit` that follows in the same frame, so the composition or the commit takes its place
  (`deckcraft_ui_egui::ime`, `text.compose`). The latest range named since the last take wins.
- `winit::platform::macos::ime_surrounding_text()` reads the store (tests); `ImeMarked` and
  `ImeSurroundingText::apply` stay public for DeckCraft's behaviour test.

The store follows the input method's own edits at once, before the application reports the text
again at the next frame (`ImeSurroundingText::apply`, `WinitView::ime_update_surrounding`): an input
method may read `selectedRange` and `attributedSubstringForProposedRange:` right after
`setMarkedText:` (one that takes back a suggestion with empty marked text and reads the text before
the caret again would otherwise read the dismissed suggestion as committed text; `NSTextView`
answers the changed state at once).

- `setMarkedText:` puts the new marked text in place of the range it replaces — the named
  replacement range, else the marked text, else the selection — and selects the input method's
  `selectedRange` within it. A range reaching into the marked text counts it as the committed
  text it stands for (`ImeMarked::replaced`: a reconverted selection, or nothing), as the
  application's `committed_range` does. Empty marked text takes the marked text out and selects
  what it stood for, as `text.compose` puts a replaced selection back. The marked text being
  replaced is read where it lies before it is replaced (live conversion re-marking the words
  before its marked text: ご marked after 日本, then 日本ご marked again).
- A commit puts the committed text in place of the named range, else the marked text, else the
  selection, the caret after it, and drops the marked text at once (storytold keeps it until
  `keyDown:` ends, so `selectedRange` would still answer within it).
- The key's own text that the key event delivers (`PlainInsert::KeyText`) is typed at the
  selection too; the store follows it (new: the old patch left it to the next frame).
- `unmarkText` commits the marked text as it stands — the `NSTextInputClient` contract, what
  `NSTextView` does — where storytold only clears it: an `Ime::Commit` of the marked text follows
  the empty `Ime::Preedit`.

The in-place state is a bridge to the next frame, which overwrites it with what the slide holds.
`DECKCRAFT_IME_TRACE=<file>` writes each `keyDown:` (and whether the key is sent), `insertText:`,
`setMarkedText:`, `unmarkText`, `doCommandBySelector:`, `set_ime_allowed` and `reset_ime` call
(text, ranges, IME state, whether text is reported) to the file: an input method's real calls can't
be reproduced without typing. Every site is marked `DeckCraft patch`.

## How it sits with storytold's W22, W24 and W26

**One mode at a time.** Whether the application reports text (`reported_surrounding`) decides
every answer and every replacement together: `selectedRange`, `markedRange`,
`attributedSubstringForProposedRange:` and the replacement ranges come from the store while text is
reported, and from storytold's `ImeDocument` and W26 otherwise. `hasMarkedText` is storytold's in
both. Whenever reporting starts or stops, the view resets its `ImeDocument` (its offsets keep
growing, its text is forgotten), so it never answers with the slide's text after the app stopped
reporting, and a range named in the store's offsets finds no text to delete. `ImeDocument` is still
fed while text is reported (storytold's lines are untouched); nothing reads it then. An app that
never calls `set_ime_surrounding_text` never changes mode: storytold's behaviour, byte for byte.

**No double replacement (W26).** While text is reported, `replaced()` returns `None` without
touching `ImeDocument`, so W26 stands down completely:

- `setMarkedText:` queues no empty `Ime::Commit` with a deletion before the preedit;
- every `queue_commit(text, delete)` gets `delete = None`, so it queues a plain `Ime::Commit`, the
  per-commit handler is never installed and `WindowExtMacOS::take_ime_delete_surrounding` returns
  `None` for every commit — egui-winit `craft/0.37` therefore emits no
  `ImeEvent::DeleteSurroundingBytes`;
- the range goes to `take_ime_replacement_range` instead (recorded before the event is queued),
  which the application applies by selecting it before the Preedit/Commit.

Our path is kept rather than W26's because it also covers the marked text taking the selection's
place during reconversion: W26 can only delete text `ImeDocument` saw typed since the caret last
moved and reaching the caret, so a reconverted selection of older text is never replaced, and what
it does delete it deletes outright (an empty `Ime::Commit` before the preedit), so dismissing the
conversion can't put it back; the application's composition stands for the selected text and puts
it back on a dismissal. It also covers live conversion naming committed text before its marked
text, which `committed_range` maps through the composition.
The key's own text naming a range is committed while text is reported (W26 deletes ahead of the
key event; only the application's handling of a commit applies our range). The native driver
checks the stand-down: with it removed, the reconversion dismissal and the press-and-hold commit
each carry a W26 deletion as well as the range.

**Kept from storytold, dropped from ours.**

- *Committing on `pending_commit`* (W6, rust-windowing/winit#4650) replaces our "commit while
  marked text remains"; storytold's `in_key_down` / `committed_in_key_down` replace our
  `in_key_event`, its `key_characters` our `key_text`.
- *Text typed without a composition* (W22's `plain_insert`) replaces our two branches for an
  input method's own text: outside a key event (emoji picker, dictation) and other text than the
  key's (U+3000 for Space, the rest of a conversion committed after its first clause). Storytold
  requires `ime_allowed` (and reports `Ime::Enabled`) where ours required the IME state; theirs is
  kept.
- *Discarding marked text when IME is disallowed* (W7) and on `reset_ime` (W24): our patch
  committed the marked text when the application disallowed IME. Dropped: egui 0.37's egui-winit
  toggles `set_ime_allowed(false)` / `(true)` and calls `reset_ime_with_request` to interrupt a
  composition the application has already settled itself (DeckCraft's engine keeps the composed
  text: `settle_composition` / `end_composition`), and egui 0.37 no longer forwards
  `Ime::Disabled`, so a commit from winit there would type the text twice or arrive as a stale
  event. While text is reported, the store keeps the discarded marked text as committed text
  (what DeckCraft keeps) until the next report. If AppKit calls `unmarkText` from inside
  `discardMarkedText`, it doesn't commit (`ime_discarding`), so W24 stays "cancel without
  committing".

**Kept from ours, where storytold differs** (only while text is reported, since storytold's
behaviour must not change otherwise):

- `unmarkText` commits (above). Storytold drops the marked text and keeps `pending_commit`, so
  that an input method calling `unmarkText` before `insertText:` (some Chinese input methods,
  rust-windowing/winit#4651) gets its `insertText:` committed. `pending_commit` is left as
  storytold leaves it, so that `insertText:` is still committed; with a replacement range naming
  the unmarked text it replaces it, as in `NSTextView`. One that names no range would type the
  text after the marked text — as it would in `NSTextView`, which commits on `unmarkText` too.
- The key's own text inserted after the input method committed in the same key event (a syllable
  or a conversion committed by a space or a punctuation key, then that key's character) is an
  `Ime::Commit`. storytold sends it as the key's text, but `keyDown:` doesn't send the key once
  something was committed (`committed_in_key_down`), so it is lost; winit 0.30.13 committed it,
  its marked text standing until the key event ended. Not changed when nothing is reported; worth
  checking with a trace and proposing to storytold.

## Committing a conversion clause by clause

Committing the first clause of a multi-clause conversion keeps the rest composing, as Office for
Mac does: storytold's `pending_commit` commits the first clause and the following `setMarkedText:`
starts the rest; text an input method commits within a key event that isn't the key's own
characters (the rest of the conversion, a full-width space) is committed and the key (Return)
isn't sent (`plain_insert`); with text reported, the rest's marked text is placed after the
committed clause because the commit drops the old marked text at once. DeckCraft also keeps the
IME allowed every frame the slide's text has the keyboard (`ImeState::rect` in
`deckcraft_ui_egui::ime`), so egui doesn't turn it off between frames. DeckCraft's handling of the
events is pinned by the ui-egui test
`committing_the_first_clause_keeps_the_rest_of_the_composition`.

## Verified, and not

`cargo check`, `cargo clippy --all-targets` (deny warnings) and `cargo doc` (deny warnings) for
aarch64-apple-darwin; `cargo check --tests` for aarch64-linux-android; `cargo test`: 32 library
tests (7 new `ime_surrounding` tests next to storytold's `ime_document` ones), 52 `ime_patches`
tests (44 storytold + 7 `ime_surrounding` + 1 source guard), the native drivers
`ime_reset_dispatch` (storytold, unchanged), `macos_termination` and the new
`macos_ime_surrounding` (reported, unreported, transition). DeckCraft's behaviour test
`the_reported_text_follows_the_input_methods_edits_at_once` passes unchanged against the fork.
The native driver calls the real `WinitView` as an input method does, outside key events; no real
input method was typed: the Japanese input method's reconversion, live conversion, the
clause-by-clause commit, Return, the key's text after a commit and press-and-hold under reported
text need a manual check (with `DECKCRAFT_IME_TRACE`).
