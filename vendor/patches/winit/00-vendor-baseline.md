# winit vendor baseline

The source is storytold/winit at `a517b3846ee8457d84ee4975110bfda6797c44dc` (branch
`craft/0.30.13`: winit 0.30.13 with the craft suite's patches, listed in `vendor/winit/PATCHES.md`),
the revision upstream DeckCraft's `[patch.crates-io]` pins since its move to egui 0.37. Apache-2.0
licensed (`vendor/winit/LICENSE`). `Cargo.toml`, `build.rs`, `LICENSE`, `README.md`, `PATCHES.md`,
`FEATURES.md`, `src/`, `dpi/`, `examples/` and `tests/` are copied from that revision; its docs are
not.

The baseline carries no behavioural change of ours. Every change is one of the numbered patch files
that follow, each marked `DeckCraft patch` in the source (`source-delta.json` lists the files).
Before egui 0.37 the baseline was the published winit 0.30.13 crate (archive SHA-256
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`).
