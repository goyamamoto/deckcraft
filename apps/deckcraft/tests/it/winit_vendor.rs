//! The build takes winit from `vendor/winit` (`vendor/README.md`): storytold/winit at the revision
//! upstream DeckCraft pins, with DeckCraft's macOS patch on top. A re-vendored or upgraded copy
//! without the patch would silently bring the platform bugs back, and one edited by hand would
//! no longer be the reviewed copy.

use std::path::Path;

/// The storytold/winit revision vendored (`craft/0.30.13`, the one egui 0.37's fork is built
/// against) and the patch commit on top of it (`deckcraft/ime-surrounding`, "D1").
const SOURCE: &str = "https://github.com/storytold/winit";
const SOURCE_REV: &str = "a517b3846ee8457d84ee4975110bfda6797c44dc";
const PATCH_COMMIT: &str = "d354e8a83ad1b3fe453d2b8ada8829028590d891";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn winit_resolves_to_the_vendored_copy() {
    let root = root();
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let winit = lock.split("[[package]]").find(|p| p.contains("\nname = \"winit\"\n")).expect("winit is in Cargo.lock");
    assert!(winit.contains("\nversion = \"0.30.13\"\n"), "the vendored copy is 0.30.13: re-vendor and re-apply the patches");
    assert!(!winit.contains("\nsource = "), "winit must resolve to vendor/winit, not a registry or a git checkout");
    assert_eq!(lock.matches("\nname = \"winit\"\n").count(), 1, "one winit in the build: every user takes the vendored copy");

    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("winit = { path = \"vendor/winit\" }"), "the workspace no longer patches crates.io winit to vendor/winit");
    assert!(manifest.contains("exclude = [\"vendor/winit\"]"), "vendor/winit must stay out of the workspace (its own lints, its unsafe code)");
    assert!(manifest.contains(SOURCE_REV), "the workspace manifest names the storytold/winit revision it vendors");

    let vendored = std::fs::read_to_string(root.join("vendor/winit/Cargo.toml")).unwrap();
    assert!(vendored.contains("\nname = \"winit\"\nversion = \"0.30.13\"\n"), "vendor/winit is winit 0.30.13");
    // The crate's licence travels with it (Apache-2.0), and the provenance is written down.
    let licence = std::fs::read_to_string(root.join("vendor/winit/LICENSE")).unwrap();
    assert!(licence.contains("Apache License") && licence.contains("Version 2.0"), "winit's Apache-2.0 licence");
    let baseline = std::fs::read_to_string(root.join("vendor/patches/winit/00-vendor-baseline.md")).unwrap();
    assert!(baseline.contains(SOURCE_REV), "the baseline names the storytold/winit revision");
    let patch = std::fs::read_to_string(root.join("vendor/patches/winit/01-macos-ime-surrounding-text.md")).unwrap();
    assert!(patch.contains(SOURCE_REV) && patch.contains(PATCH_COMMIT), "the patch note names its base and its commit");
    let patches = std::fs::read_to_string(root.join("vendor/winit/PATCHES.md")).unwrap();
    assert!(patches.contains("`deckcraft/ime-surrounding`") && patches.contains("`D1`"), "the fork's patch list carries DeckCraft's patch");
    let notice = std::fs::read_to_string(root.join("NOTICE")).unwrap();
    assert!(notice.contains(SOURCE_REV) && notice.contains("vendor/winit"), "NOTICE names the vendored revision");
}

/// `source-delta.json` lists every file that differs from the storytold revision, with its
/// checksum there (`null`: new) and in `vendor/winit`: the vendored files are exactly the
/// reviewed ones, and the macOS sources the patch changes are among them.
#[test]
fn vendored_winit_matches_the_recorded_source_delta() {
    let root = root();
    let text = std::fs::read_to_string(root.join("vendor/patches/winit/source-delta.json")).unwrap();
    let delta: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(delta["crate"], "winit");
    assert_eq!(delta["version"], "0.30.13");
    assert_eq!(delta["source"], SOURCE);
    assert_eq!(delta["source_rev"], SOURCE_REV);
    assert_eq!(delta["patch_commit"], PATCH_COMMIT);
    let files = delta["changed_source_files"].as_array().expect("changed_source_files is a list");
    let hex64 = |v: &serde_json::Value| v.as_str().is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut paths = Vec::new();
    for f in files {
        let path = f["path"].as_str().expect("each file has a path");
        assert!(!path.starts_with('/') && !path.contains(".."), "{path}: relative to vendor/winit");
        assert!(f["patches"].as_array().is_some_and(|p| !p.is_empty()), "{path}: names the patch that changed it");
        for p in f["patches"].as_array().into_iter().flatten() {
            let n = p.as_str().unwrap_or_default();
            let note = std::fs::read_dir(root.join("vendor/patches/winit"))
                .unwrap()
                .filter_map(|e| e.ok())
                .any(|e| e.file_name().to_string_lossy().starts_with(&format!("{n}-")));
            assert!(note, "{path}: patch {n} has a note in vendor/patches/winit");
        }
        assert!(f["pristine_sha256"].is_null() || hex64(&f["pristine_sha256"]), "{path}: the pristine checksum, or null for a new file");
        assert!(hex64(&f["vendor_sha256"]), "{path}: the vendored checksum");
        let bytes = std::fs::read(root.join("vendor/winit").join(path)).unwrap_or_else(|e| panic!("vendor/winit/{path}: {e}"));
        assert_eq!(sha256_hex(&lf(&bytes)), f["vendor_sha256"].as_str().unwrap_or_default(), "vendor/winit/{path} differs from the reviewed copy");
        assert_ne!(f["pristine_sha256"], f["vendor_sha256"], "{path} is listed as changed");
        paths.push(path);
    }
    for f in
        ["src/platform/macos.rs", "src/platform_impl/macos/mod.rs", "src/platform_impl/macos/view.rs", "src/platform_impl/macos/ime_surrounding.rs"]
    {
        assert!(paths.contains(&f), "{f} is listed as changed");
    }
}

/// The file's text with CRLF line ends read as LF (a Windows checkout may convert them).
fn lf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            continue;
        }
        out.push(*b);
    }
    out
}

/// SHA-256 (FIPS 180-4) as lowercase hex: enough for a handful of source files, without a
/// dependency.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[test]
fn sha256_matches_the_standard_vectors() {
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(lf(b"a\r\nb\rc\n"), b"a\nb\rc\n");
}
