//! Font boxes in a Japanese setting: the font box names the East Asian font on Japanese text and
//! lists families by their name in the interface language, found by any of their names; the Font
//! dialog's Asian text font, the Asian text theme fonts and Replace Fonts. Fonts come from a test
//! font built in memory (a Japanese family name, kana and kanji), so nothing depends on the fonts
//! the machine has.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use deckcraft_engine::Session;
use deckcraft_engine::fonts::{FontChoice, ThemeFontChoice};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use super::{FontList, Offer, Pick, Row, rank, rows};
use crate::i18n::{Lang, fixture_lang, set_current};
use crate::{Services, SlideApp};

/// The test font's English and Japanese family names.
const ENGLISH: &str = "Qqq Fixture Gothic";
const JAPANESE: &str = "キューフィクスチャ ゴシック";

// ---------- a test font built in memory ----------

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

/// A `name` table of Windows (UTF-16) records: (language, name ID, text).
fn name_table(names: &[(u16, u16, &str)]) -> Vec<u8> {
    let mut recs: Vec<(u16, u16, Vec<u8>)> = names.iter().map(|&(l, id, t)| (l, id, t.encode_utf16().flat_map(u16::to_be_bytes).collect())).collect();
    recs.sort_by_key(|r| (r.0, r.1));
    let count = recs.len() as u16;
    let mut out = Vec::new();
    out.extend(be16(0));
    out.extend(be16(count));
    out.extend(be16(6 + 12 * count));
    let mut strings = Vec::new();
    for (l, id, bytes) in &recs {
        for v in [3, 1, *l, *id, bytes.len() as u16, strings.len() as u16] {
            out.extend(be16(v));
        }
        strings.extend_from_slice(bytes);
    }
    out.extend(strings);
    out
}

/// A format 4 `cmap` mapping each of `chars` (BMP) to glyph 1.
fn cmap_table(chars: &[char]) -> Vec<u8> {
    let mut cps: Vec<u16> = chars.iter().filter_map(|c| u16::try_from(u32::from(*c)).ok()).collect();
    cps.sort_unstable();
    cps.dedup();
    let seg = cps.len() as u16 + 1;
    let log = 15 - seg.leading_zeros() as u16;
    let search = 2 * (1u16 << log);
    let mut sub = Vec::new();
    for v in [4, 16 + 8 * seg, 0, seg * 2, search, log, seg * 2 - search] {
        sub.extend(be16(v));
    }
    for c in cps.iter().chain([&0xFFFF]) {
        sub.extend(be16(*c));
    }
    sub.extend(be16(0));
    for c in cps.iter().chain([&0xFFFF]) {
        sub.extend(be16(*c));
    }
    for c in &cps {
        sub.extend(be16(1u16.wrapping_sub(*c)));
    }
    sub.extend(be16(1));
    for _ in 0..seg {
        sub.extend(be16(0));
    }
    let mut out = Vec::new();
    for v in [0, 1, 3, 1] {
        out.extend(be16(v));
    }
    out.extend(12u32.to_be_bytes());
    out.extend(sub);
    out
}

/// The fonts crate's fixture (.notdef and a square) renamed `english`, with the Japanese family
/// name `japanese`, drawing kana, kanji and A.
fn fixture_font(english: &str, japanese: &str) -> Vec<u8> {
    let base: &[u8] = include_bytes!("../../fonts/tests/fixtures/legacy-regular.ttf");
    let n = u16::from_be_bytes([base[4], base[5]]) as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..n)
        .map(|i| {
            let r = &base[12 + 16 * i..28 + 16 * i];
            let off = u32::from_be_bytes(r[8..12].try_into().unwrap()) as usize;
            let len = u32::from_be_bytes(r[12..16].try_into().unwrap()) as usize;
            (r[0..4].try_into().unwrap(), base[off..off + len].to_vec())
        })
        .collect();
    let names = [(0x409, 1, english), (0x409, 2, "Regular"), (0x409, 4, english), (0x411, 1, japanese), (0x411, 4, japanese)];
    for (tag, data) in &mut tables {
        match &*tag {
            b"name" => *data = name_table(&names),
            b"cmap" => *data = cmap_table(&['A', 'あ', 'ア', '日', '本', '語', '中', '国']),
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

/// Add the test font to the font database (once; every test in this process shares it).
fn install_fixture_font() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        deckcraft_fonts::FontDb::global().add_font(fixture_font(ENGLISH, JAPANESE));
    });
}

// ---------- the list ----------

fn choice(name: &str, english: &str, names: &[&str], east_asian: bool) -> FontChoice {
    FontChoice { name: name.into(), english: english.into(), names: names.iter().map(|s| (*s).to_string()).collect(), east_asian }
}

fn sample_list() -> FontList {
    let theme = |name: &str, reference: &str, role: &str, east_asian: bool| ThemeFontChoice {
        name: name.into(),
        reference: reference.into(),
        role: role.into(),
        east_asian,
    };
    FontList {
        east_asian: true,
        theme: vec![
            theme("Inter", "+mj-lt", "headings", false),
            theme("Inter", "+mn-lt", "body", false),
            theme("BIZ UDPGothic", "+mj-ea", "headings", true),
            theme("BIZ UDPGothic", "+mn-ea", "body", true),
        ],
        fonts: Arc::new(vec![
            choice("Inter", "Inter", &["Inter"], false),
            choice("Lato", "Lato", &["Lato"], false),
            choice("ヒラギノ角ゴシック", "Hiragino Sans", &["Hiragino Sans", "ヒラギノ角ゴシック"], true),
            choice("ヒラギノ明朝", "Hiragino Mincho ProN", &["Hiragino Mincho ProN", "ヒラギノ明朝"], true),
        ]),
    }
}

fn names(list: &FontList, rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| match r {
            Row::Heading(h) => format!("# {h}"),
            Row::Theme(k) => super::theme_label(&list.theme[*k]),
            Row::Font(k) => list.fonts[*k].name.clone(),
        })
        .collect()
}

#[test]
fn typing_finds_a_family_by_any_of_its_names() {
    let list = sample_list();
    assert_eq!(names(&list, &rows(&list, Offer::All, "ゴシック")), ["ヒラギノ角ゴシック"], "its Japanese name");
    assert_eq!(names(&list, &rows(&list, Offer::All, "hiragino")), ["ヒラギノ角ゴシック", "ヒラギノ明朝"], "its English name");
    assert_eq!(names(&list, &rows(&list, Offer::All, "ｈｉｒａｇｉｎｏ ｓａｎｓ")), ["ヒラギノ角ゴシック"], "full-width letters fold");
    assert_eq!(names(&list, &rows(&list, Offer::All, "inter")), ["Inter (Headings)", "Inter (Body)", "Inter"], "theme entries first");
    assert_eq!(names(&list, &rows(&list, Offer::All, "biz")), ["BIZ UDPGothic (Asian Headings)", "BIZ UDPGothic (Asian Body)"]);
    assert!(rows(&list, Offer::All, "zzz").is_empty());
    // An exact name comes before names that only start with it.
    let mut l = sample_list();
    Arc::make_mut(&mut l.fonts).insert(0, choice("Lato Black", "Lato Black", &["Lato Black"], false));
    assert_eq!(names(&l, &rows(&l, Offer::Families, "lato")), ["Lato", "Lato Black"]);
    assert_eq!(rank("ゴシ", &["ヒラギノ角ゴシック"]), Some(2));
}

#[test]
fn the_lists_offer_the_theme_and_the_families_each_box_wants() {
    let list = sample_list();
    assert_eq!(
        names(&list, &rows(&list, Offer::All, "")),
        [
            "# Theme Fonts",
            "Inter (Headings)",
            "Inter (Body)",
            "BIZ UDPGothic (Asian Headings)",
            "BIZ UDPGothic (Asian Body)",
            "# All Fonts",
            "Inter",
            "Lato",
            "ヒラギノ角ゴシック",
            "ヒラギノ明朝"
        ]
    );
    assert_eq!(names(&list, &rows(&list, Offer::Latin, ""))[1..3], ["Inter (Headings)", "Inter (Body)"]);
    assert_eq!(
        names(&list, &rows(&list, Offer::EastAsian, "")),
        ["# Theme Fonts", "BIZ UDPGothic (Asian Headings)", "BIZ UDPGothic (Asian Body)", "# All Fonts", "ヒラギノ角ゴシック", "ヒラギノ明朝"],
        "Asian text: the Japanese, Chinese and Korean families"
    );
    assert_eq!(names(&list, &rows(&list, Offer::Families, "")), ["Inter", "Lato", "ヒラギノ角ゴシック", "ヒラギノ明朝"]);
    // A deck naming a font in English lights its Japanese entry up; theme references show as their entry.
    assert!(super::is_current(&list, &Row::Font(2), "Hiragino Sans"));
    assert!(super::is_current(&list, &Row::Theme(1), "Inter (Body)"));
    assert!(super::is_current(&list, &Row::Theme(3), "BIZ UDPGothic (Body)"), "the box names the Asian body font as the theme's body font");
    assert!(!super::is_current(&list, &Row::Theme(2), "BIZ UDPGothic (Body)"));
    assert_eq!(super::shown_value(&list, "+mn-ea"), "BIZ UDPGothic (Asian Body)");
    assert_eq!(super::shown_value(&list, "Lato"), "Lato");
    assert_eq!(Pick::Theme(list.theme[2].clone()).value(), "+mj-ea");
}

// ---------- the ribbon's font box ----------

fn harness_with(s: Session, lang: Option<Lang>) -> Harness<'static, SlideApp> {
    install_fixture_font();
    let app = SlideApp::new(s, Services::default());
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        move |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            if let Some(l) = lang {
                set_current(l);
            }
            app.ui(ui);
            set_current(Lang::EN);
        },
        app,
    );
    h.run_steps(4);
    h
}

/// A selected text box holding `text`.
fn session_with_text(text: &str) -> (Session, deckcraft_model::ShapeId) {
    let mut s = Session::with_new();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [80.0, 300.0, 400.0, 80.0], "text": text})).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let _ = s.execute("text.exit", &json!({}));
    (s, id)
}

fn japanese_ui() -> Lang {
    fixture_lang("ja", "@id\tlanguage.name\t日本語\n\tFont\tフォント\n\t{f} (Body)\t{f} (本文)\n")
}

fn run_props(h: &Harness<'_, SlideApp>, id: deckcraft_model::ShapeId) -> deckcraft_model::RunProps {
    let st = h.state().session.active().unwrap();
    st.shape(id).unwrap().text.as_ref().unwrap().paragraphs[0].runs[0].props.clone()
}

fn font_box_text(h: &Harness<'_, SlideApp>, name: &str) -> String {
    h.get_by_label(name).value().unwrap_or_default()
}

/// Type `text` into the font box named `name` (clicking selects its name, so typing replaces it).
fn type_into(h: &mut Harness<'_, SlideApp>, name: &str, text: &str) {
    // Long enough after any earlier click that this one isn't a double click (which selects a word).
    h.run_steps(30);
    h.get_by_label(name).click();
    h.run_steps(2);
    h.get_by_label(name).type_text(text);
    h.run_steps(2);
}

#[test]
fn a_japanese_interface_lists_and_writes_fonts_by_their_japanese_names() {
    let (s, id) = session_with_text("日本語abc");
    let mut h = harness_with(s, Some(japanese_ui()));
    assert_eq!(font_box_text(&h, "フォント"), "Inter (本文)", "the theme's body font, named in Japanese");
    // Typing part of the Japanese name or of the English name finds the font by its Japanese name.
    for query in ["フィクスチャ", "qqq fixture"] {
        type_into(&mut h, "フォント", query);
        assert!(h.query_by_label(JAPANESE).is_some(), "{query} lists {JAPANESE}");
        assert!(h.query_by_label(ENGLISH).is_none(), "not by its English name in a Japanese interface");
        // Escape puts the font back and keeps the selection.
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert_eq!(font_box_text(&h, "フォント"), "Inter (本文)");
        assert_eq!(h.state().session.active().unwrap().selection.shapes, [id]);
        assert!(h.query_by_label(JAPANESE).is_none(), "the matches close");
    }
    type_into(&mut h, "フォント", "フィクスチャ");
    let n = h.state().session.active().unwrap().history.undo.len();
    h.get_by_label(JAPANESE).click();
    h.run_steps(3);
    let r = run_props(&h, id);
    // Written by the name shown, as Japanese PowerPoint writes it; an East Asian font sets the
    // Japanese text's font too.
    assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some(JAPANESE), Some(JAPANESE)));
    assert_eq!(h.state().session.active().unwrap().history.undo.len(), n + 1);
}

#[test]
fn an_english_interface_lists_fonts_by_their_english_names_and_enter_picks_the_match() {
    let (s, id) = session_with_text("日本語abc");
    let mut h = harness_with(s, None);
    assert_eq!(font_box_text(&h, "Font"), "Inter (Body)");
    type_into(&mut h, "Font", "フィクスチャ");
    assert!(h.query_by_label(ENGLISH).is_some(), "its Japanese name finds it, shown in English");
    assert!(h.query_by_label(JAPANESE).is_none());
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let r = run_props(&h, id);
    assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some(ENGLISH), Some(ENGLISH)), "an installed Japanese face sets both fonts");
    // A name no font has is applied as typed; a Latin name leaves the Japanese text's font alone.
    type_into(&mut h, "Font", "Qqq Not Installed Serif");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let r = run_props(&h, id);
    assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("Qqq Not Installed Serif"), Some(ENGLISH)));
}

/// Japanese typed with the input method goes into the font box, not into the slide's text the
/// caret is in, also when the font list was open when the box was clicked.
#[test]
fn the_input_method_types_into_the_font_box() {
    let (mut s, id) = session_with_text("日本語abc");
    s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
    let mut h = harness_with(s, None);
    h.get_by_label("Font list").click();
    h.run_steps(3);
    h.run_steps(30);
    // A real click: the press and the release come in frames of their own.
    let at = h.get_by_label("Font").rect().center();
    for pressed in [None, Some(true), Some(false)] {
        h.event(match pressed {
            None => egui::Event::PointerMoved(at),
            Some(p) => egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: p, modifiers: egui::Modifiers::NONE },
        });
        h.run_steps(1);
    }
    h.run_steps(2);
    for ev in [egui::ImeEvent::Preedit { text: "フィクス".into(), active_range_chars: None }, egui::ImeEvent::Commit("フィクスチャ".into())]
    {
        h.event(egui::Event::Ime(ev));
        h.run_steps(1);
    }
    h.run_steps(2);
    let st = h.state().session.active().unwrap();
    assert_eq!(st.shape(id).unwrap().text.as_ref().unwrap().paragraphs[0].text(), "日本語abc", "the slide's text is untouched");
    assert_eq!(font_box_text(&h, "Font"), "フィクスチャ");
    assert!(h.query_by_label(ENGLISH).is_some(), "the match is listed");
}

#[test]
fn the_font_box_names_the_asian_font_on_japanese_text_and_offers_the_themes() {
    let (mut s, id) = session_with_text("日本語abc");
    s.east_asian_editing = Some(deckcraft_fonts::CjkLocale::Japanese);
    s.execute("design.fonts", &json!({"minorEa": "Qqq 本文 EA", "majorEa": "Qqq 見出し EA"})).unwrap();
    s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
    let mut h = harness_with(s, None);
    assert_eq!(font_box_text(&h, "Font"), "Qqq 本文 EA (Body)", "the caret is in Japanese text");
    h.state_mut().session.execute("text.edit", &json!({"id": id.0, "at": [0, 6]})).unwrap();
    h.run_steps(2);
    assert_eq!(font_box_text(&h, "Font"), "Inter (Body)", "after c, the Latin font");
    // The full list has the Asian text theme fonts; picking one writes the theme's references.
    h.state_mut().session.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 6]})).unwrap();
    h.run_steps(2);
    h.get_by_label("Font list").click();
    h.run_steps(3);
    assert!(h.query_by_label("Theme Fonts").is_none(), "headings are not entries");
    h.get_by_label("Qqq 見出し EA (Asian Headings)").click();
    h.run_steps(3);
    let r = run_props(&h, id);
    assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("+mj-lt"), Some("+mj-ea")));
    h.state_mut().session.execute("text.edit", &json!({"id": id.0, "at": [0, 1]})).unwrap();
    h.run_steps(2);
    assert_eq!(font_box_text(&h, "Font"), "Qqq 見出し EA (Headings)");
}

// ---------- dialogs ----------

#[test]
fn the_font_dialog_sets_the_asian_text_font_in_one_step() {
    let (mut s, id) = session_with_text("日本語abc");
    s.east_asian_editing = Some(deckcraft_fonts::CjkLocale::Japanese);
    let mut h = harness_with(s, None);
    let n = h.state().session.active().unwrap().history.undo.len();
    h.state_mut().run("format.fontDialog", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("font"));
    assert_eq!(font_box_text(&h, "Latin text font"), "Inter");
    type_into(&mut h, "Asian text font", "qqq fixture");
    h.get_by_label(ENGLISH).click();
    h.run_steps(2);
    assert_eq!(font_box_text(&h, "Asian text font"), ENGLISH);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let r = run_props(&h, id);
    assert_eq!(r.font_ea.as_deref(), Some(ENGLISH));
    assert_eq!(r.font, None, "the Latin text font didn't change, so OK leaves it to the theme");
    assert_eq!(h.state().session.active().unwrap().history.undo.len(), n + 1);
}

#[test]
fn the_font_dialog_has_no_asian_box_without_east_asian_editing() {
    let (s, _) = session_with_text("Hello");
    let mut h = harness_with(s, None);
    h.state_mut().run("format.fontDialog", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Latin text font").is_some());
    assert!(h.query_by_label("Asian text font").is_none());
}

fn open_theme_fonts(h: &mut Harness<'_, SlideApp>) {
    h.state_mut().run("design.fonts", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("themeFonts"));
}

#[test]
fn theme_fonts_take_asian_text_heading_and_body_fonts() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
    s.east_asian_editing = Some(deckcraft_fonts::CjkLocale::Japanese);
    let mut h = harness_with(s, None);
    let fonts = |h: &Harness<'_, SlideApp>| h.state().session.active().unwrap().doc.masters[0].theme.fonts.clone();
    let before = fonts(&h);
    let n = h.state().session.active().unwrap().history.undo.len();
    open_theme_fonts(&mut h);
    // Seeded from the theme's Japanese (Jpan) font, which it uses where it names no a:ea.
    assert_eq!(font_box_text(&h, "Asian heading font"), "BIZ UDPGothic");
    assert_eq!(font_box_text(&h, "Asian body font"), "BIZ UDPGothic");
    assert_eq!(h.query_all_by_label_contains("あア亜").count(), 2, "Japanese heading and body samples");
    let save = h.get_by_label("Save").rect();
    let win = h.get_by_label("Create New Theme Fonts").rect();
    assert!(win.contains(save.center()), "Save {save:?} is outside the dialog window {win:?}");
    // Saving unchanged changes nothing (the theme keeps naming no a:ea).
    h.get_by_label("Save").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(fonts(&h), before);
    assert_eq!(h.state().session.active().unwrap().history.undo.len(), n);

    open_theme_fonts(&mut h);
    type_into(&mut h, "Asian heading font", "フィクスチャ");
    h.get_by_label(ENGLISH).click();
    h.run_steps(2);
    h.get_by_label("Save").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    let after = fonts(&h);
    assert_eq!((after.major.ea.as_str(), after.minor.ea.as_str()), (ENGLISH, ""), "only the Asian heading font changed");
    assert_eq!((after.major.latin.as_str(), after.minor.latin.as_str()), (before.major.latin.as_str(), before.minor.latin.as_str()));
    assert_eq!(h.state().session.active().unwrap().history.undo.len(), n + 1);
}

#[test]
fn replace_fonts_lists_the_asian_font_and_finds_the_replacement_by_name() {
    let (mut s, id) = session_with_text("日本語abc");
    s.execute("edit.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("format.font", &json!({"eastAsian": "テスト明朝"})).unwrap();
    let mut h = harness_with(s, None);
    h.state_mut().dialog = Some(crate::dialogs::Dialog::new("replaceFonts"));
    h.run_steps(3);
    h.state_mut().dialog.as_mut().unwrap().fields.insert("from".into(), "テスト明朝".into());
    h.run_steps(1);
    type_into(&mut h, "Font name", "フィクスチャ");
    h.get_by_label(ENGLISH).click();
    h.run_steps(2);
    h.query_all_by_label("Replace").last().expect("the dialog's Replace button").click();
    h.run_steps(3);
    assert_eq!(run_props(&h, id).font_ea.as_deref(), Some(ENGLISH));
    assert!(h.query_by_label_contains("Replaced “テスト明朝”").is_some());
}

// ---------- the status bar ----------

#[test]
fn the_status_bar_names_the_language_of_japanese_text() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [80.0, 300.0, 400.0, 80.0], "text": "日本語"})).unwrap();
    s.execute("text.edit", &json!({"id": r["id"], "at": [0, 1]})).unwrap();
    let mut h = harness_with(s, None);
    // The run has no language of its own: the deck's default (a:defPPr) is Japanese. Screen readers
    // hear the status bar's "Language" with the language as its value.
    let lang = h.get_by_role_and_label(egui::accesskit::Role::Button, "Language");
    assert_eq!(lang.value().as_deref(), Some("日本語"), "the status bar names Japanese");
    lang.click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("language"));
}

/// Writes the font box's matches for ゴシック, in a Japanese fixture interface, to the PNG named by
/// `DECKCRAFT_PNG_OUT_FONTPICK`, for review.
#[test]
#[ignore = "writes a screenshot for review"]
fn screenshot_typed_matches() {
    let Ok(path) = std::env::var("DECKCRAFT_PNG_OUT_FONTPICK") else { return };
    let (s, _) = session_with_text("日本語のテキスト");
    let mut h = harness_with(s, Some(japanese_ui()));
    type_into(&mut h, "フォント", "ゴシック");
    h.run_steps(4);
    h.render().unwrap().save(path).unwrap();
}
