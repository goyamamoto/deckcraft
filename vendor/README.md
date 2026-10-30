# Vendored dependencies

Published third-party crates carried in the tree with a narrow local patch, so a platform fix can
ship before (or without) an upstream release. Each crate keeps its upstream licence and provenance
in its own directory; `patches/<crate>/` says what was changed and why, one file per change, with
the regression test that guards it.

| Crate | Version | Licence | Local patches | Guard |
|---|---|---|---|---|
| winit | 0.30.13, storytold/winit `a517b38` (the suite's branch the egui 0.37 fork builds against) | Apache-2.0 (`winit/LICENSE`) | `patches/winit/01-macos-ime-surrounding-text.md` | `apps/deckcraft/tests/it/winit_vendor.rs`, `apps/deckcraft/tests/it/winit_macos_ime_surrounding_text.rs` |

The winit patch was ported from WordCraft's vendored copy of the same winit release (a sibling
Craft app, AGENTS.md §2) and, with upstream's move to egui 0.37, onto storytold/winit's
`craft/0.30.13` branch, which already carries the suite's own input-method patches (W1–W27 in
`winit/PATCHES.md`); our patch is "D1" there. Its patch sites are marked `DeckCraft patch`.

The workspace manifest patches crates.io to `vendor/winit` (`[patch.crates-io]`), so every
crate in the build that depends on winit (eframe, egui-winit, accesskit_winit…) takes this copy;
`Cargo.lock` records it without a `source`. The directory is excluded from the workspace
(`exclude`), so it keeps its own lints, formatting and `unsafe` code (DeckCraft's own crates
forbid `unsafe`), and `cargo fmt --all`/`clippy --workspace` leave it alone. Only the macOS
sources differ from the published crate: Windows, Linux, FreeBSD and the web build the upstream
code unchanged.

Re-vendoring: rebase the patch commit (`patches/winit/source-delta.json` names it) onto the
storytold/winit revision upstream DeckCraft pins, then copy `Cargo.toml`, `build.rs`, `LICENSE`,
`README.md`, `PATCHES.md`, `FEATURES.md`, `src/`, `dpi/` (the fork's workspace member),
`examples/` and `tests/` (its manifest names them) from that commit (`git archive`; docs are left
out), and run the guards (`cargo test -p deckcraft --test it`, and `cargo test` in the fork for its
own). `source-delta.json` lists every file that differs from the storytold revision, with both
checksums, so a review can diff exactly those. Drop the override once storytold's winit (or a winit
release) carries the change.
