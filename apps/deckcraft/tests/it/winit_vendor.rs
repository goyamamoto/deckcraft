//! The build takes winit from `vendor/winit` (`vendor/README.md`): a re-vendored or upgraded
//! copy without the patches would silently bring the platform bugs back.

#[test]
fn winit_resolves_to_the_vendored_copy() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let winit = lock.split("[[package]]").find(|p| p.contains("\nname = \"winit\"\n")).expect("winit is in Cargo.lock");
    assert!(winit.contains("\nversion = \"0.30.13\"\n"), "the vendored copy is 0.30.13: re-vendor and re-apply the patches");
    assert!(!winit.contains("\nsource = "), "winit must resolve to vendor/winit, not a registry");
    assert_eq!(lock.matches("\nname = \"winit\"\n").count(), 1, "one winit in the build: every user takes the vendored copy");

    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("winit = { path = \"vendor/winit\" }"), "the workspace no longer patches crates.io winit to vendor/winit");
    assert!(manifest.contains("exclude = [\"vendor/winit\"]"), "vendor/winit must stay out of the workspace (its own lints, its unsafe code)");

    let baseline = std::fs::read_to_string(root.join("vendor/patches/winit/00-vendor-baseline.md")).unwrap();
    assert!(baseline.contains("a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d"), "the baseline names the pristine archive");
    let vcs = std::fs::read_to_string(root.join("vendor/winit/.cargo_vcs_info.json")).unwrap();
    assert!(vcs.contains("e9809ef54b18499bb4f2cac945719ecc2a61061b"), "the vendored copy's upstream revision");
    // The crate's licence travels with it (Apache-2.0), and every changed file is listed.
    let licence = std::fs::read_to_string(root.join("vendor/winit/LICENSE")).unwrap();
    assert!(licence.contains("Apache License") && licence.contains("Version 2.0"), "winit's Apache-2.0 licence");
    let delta = std::fs::read_to_string(root.join("vendor/patches/winit/source-delta.json")).unwrap();
    for f in ["src/platform/macos.rs", "src/platform_impl/macos/mod.rs", "src/platform_impl/macos/view.rs"] {
        assert!(delta.contains(&format!("\"path\": \"{f}\"")), "{f} is listed as changed");
    }
}
