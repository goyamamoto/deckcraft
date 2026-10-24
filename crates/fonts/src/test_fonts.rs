//! Test fonts built in memory from the legacy-name fixture (`tests/fixtures/legacy-regular.ttf`:
//! .notdef and a square): the same glyphs with the `name` records, character map, weight and
//! style bits a test needs, so tests about Japanese font names and East Asian fallback never
//! depend on the fonts installed on the machine. Nothing is written to the repository.
#![allow(clippy::unwrap_used, clippy::expect_used)]

/// A `name` record: (platform, encoding, language, name ID, text). Windows records are written
/// in UTF-16, Mac Roman ones in ASCII and Mac Japanese ones (platform 1, encoding 1) in Shift JIS.
pub(crate) type Name<'a> = (u16, u16, u16, u16, &'a str);

/// What a test font is made of.
pub(crate) struct Spec<'a> {
    pub names: &'a [Name<'a>],
    /// Characters drawn with the square (all BMP).
    pub chars: &'a [char],
    pub weight: u16,
    /// OS/2 `fsSelection`: 0x40 regular, 0x20 bold, 0x01 italic.
    pub fs_selection: u16,
    /// OS/2 `sFamilyClass` (class in the high byte: 1 oldstyle serif, 8 sans serif).
    pub family_class: i16,
}

/// Windows English and Japanese family records for a face: ID 1 (and 4) `win_family`, ID 2
/// `win_style`, ID 16/17 typographic family and style when given, and the Japanese (0x0411)
/// versions of the families.
pub(crate) fn windows_names<'a>(
    family: &'a str,
    style: &'a str,
    win_family: &'a str,
    ja_family: Option<&'a str>,
    ja_win_family: Option<&'a str>,
) -> Vec<Name<'a>> {
    let mut v: Vec<Name<'a>> = vec![(3, 1, 0x409, 1, win_family), (3, 1, 0x409, 2, "Regular"), (3, 1, 0x409, 4, win_family)];
    if family != win_family {
        v.push((3, 1, 0x409, 16, family));
        v.push((3, 1, 0x409, 17, style));
    }
    if let Some(j) = ja_win_family {
        v.push((3, 1, 0x411, 1, j));
        v.push((3, 1, 0x411, 4, j));
    }
    if let Some(j) = ja_family {
        v.push((3, 1, 0x411, 16, j));
    }
    v
}

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn name_table(names: &[Name<'_>]) -> Vec<u8> {
    let mut recs: Vec<(u16, u16, u16, u16, Vec<u8>)> = names
        .iter()
        .map(|&(p, e, l, id, text)| {
            let bytes = match (p, e) {
                (1, 1) => encoding_rs::SHIFT_JIS.encode(text).0.into_owned(),
                (1, _) => text.as_bytes().to_vec(),
                _ => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            };
            (p, e, l, id, bytes)
        })
        .collect();
    recs.sort_by_key(|r| (r.0, r.1, r.2, r.3));
    let count = recs.len() as u16;
    let mut out = Vec::new();
    out.extend(be16(0));
    out.extend(be16(count));
    out.extend(be16(6 + 12 * count));
    let mut strings = Vec::new();
    for (p, e, l, id, bytes) in &recs {
        for v in [*p, *e, *l, *id, bytes.len() as u16, strings.len() as u16] {
            out.extend(be16(v));
        }
        strings.extend_from_slice(bytes);
    }
    out.extend(strings);
    out
}

/// A format 4 `cmap` mapping each of `chars` to glyph 1.
fn cmap_table(chars: &[char]) -> Vec<u8> {
    let mut cps: Vec<u16> = chars.iter().filter_map(|c| u16::try_from(u32::from(*c)).ok()).filter(|c| *c != 0xFFFF).collect();
    cps.sort_unstable();
    cps.dedup();
    let seg = cps.len() as u16 + 1;
    let log = 15 - seg.leading_zeros() as u16;
    let search = 2 * (1u16 << log);
    let mut sub = Vec::new();
    sub.extend(be16(4));
    sub.extend(be16(16 + 8 * seg));
    sub.extend(be16(0));
    sub.extend(be16(seg * 2));
    sub.extend(be16(search));
    sub.extend(be16(log));
    sub.extend(be16(seg * 2 - search));
    for c in cps.iter().chain([&0xFFFF]) {
        sub.extend(be16(*c));
    }
    sub.extend(be16(0));
    for c in cps.iter().chain([&0xFFFF]) {
        sub.extend(be16(*c));
    }
    for c in cps.iter() {
        sub.extend(be16(1u16.wrapping_sub(*c)));
    }
    sub.extend(be16(1));
    for _ in 0..seg {
        sub.extend(be16(0));
    }
    let mut out = Vec::new();
    out.extend(be16(0));
    out.extend(be16(1));
    out.extend(be16(3));
    out.extend(be16(1));
    out.extend(12u32.to_be_bytes());
    out.extend(sub);
    out
}

/// The fixture font with `spec`'s names, characters, weight and style bits.
pub(crate) fn font(spec: &Spec<'_>) -> Vec<u8> {
    let base: &[u8] = include_bytes!("../tests/fixtures/legacy-regular.ttf");
    let n = u16::from_be_bytes([base[4], base[5]]) as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..n)
        .map(|i| {
            let r = &base[12 + 16 * i..28 + 16 * i];
            let off = u32::from_be_bytes(r[8..12].try_into().unwrap()) as usize;
            let len = u32::from_be_bytes(r[12..16].try_into().unwrap()) as usize;
            (r[0..4].try_into().unwrap(), base[off..off + len].to_vec())
        })
        .collect();
    let mut chars = spec.chars.to_vec();
    chars.push('A');
    for (tag, data) in &mut tables {
        match &*tag {
            b"name" => *data = name_table(spec.names),
            b"cmap" => *data = cmap_table(&chars),
            b"OS/2" => {
                data[4..6].copy_from_slice(&be16(spec.weight));
                data[30..32].copy_from_slice(&spec.family_class.to_be_bytes());
                data[62..64].copy_from_slice(&be16(spec.fs_selection));
            }
            _ => {}
        }
    }
    tables.sort_by_key(|t| t.0);
    let count = tables.len() as u16;
    let log = 15 - count.leading_zeros() as u16;
    let search = 16 * (1u16 << log);
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    for v in [count, search, log, count * 16 - search] {
        out.extend(be16(v));
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        let sum = data.chunks(4).fold(0u32, |s, c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            s.wrapping_add(u32::from_be_bytes(w))
        });
        out.extend(tag);
        out.extend(sum.to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        body.extend(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend(body);
    out
}

/// A regular-weight sans face drawing `chars`, named by `names`.
pub(crate) fn sans(names: &[Name<'_>], chars: &[char]) -> Vec<u8> {
    font(&Spec { names, chars, weight: 400, fs_selection: 0x40, family_class: 8 << 8 })
}

/// A fresh temporary folder for font files.
pub(crate) fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("deckcraft-fonts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn built_fonts_parse_with_their_names_and_characters() {
    use skrifa::MetadataProvider;
    let names = windows_names("DeckCraft Kaku", "W3", "DeckCraft Kaku W3", Some("デッキ角ゴ"), Some("デッキ角ゴ W3"));
    let data = font(&Spec { names: &names, chars: &['漢', 'あ'], weight: 300, fs_selection: 0x40, family_class: 8 << 8 });
    let f = skrifa::FontRef::new(&data).unwrap();
    assert!(f.charmap().map('漢').is_some() && f.charmap().map('あ').is_some() && f.charmap().map('A').is_some());
    assert!(f.charmap().map('B').is_none());
    let ja: Vec<String> =
        f.localized_strings(skrifa::string::StringId::FAMILY_NAME).filter(|s| s.language() == Some("ja-JP")).map(|s| s.to_string()).collect();
    assert_eq!(ja, ["デッキ角ゴ W3"]);
    assert_eq!(f.attributes().weight.value(), 300.0);
}
