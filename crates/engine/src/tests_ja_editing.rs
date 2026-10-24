//! Japanese text editing through `Session::execute` and `Session::key`: the language typed text
//! is tagged with, East Asian words, width-insensitive Find, text outlines in Japanese encodings
//! and the arrow keys in vertical text.

use serde_json::json;

use crate::Session;
use crate::tools::Mods;
use deckcraft_fonts::CjkLocale;
use deckcraft_model::ShapeId;
use deckcraft_model::text::TextDir;

const NONE: Mods = Mods { shift: false, alt: false, cmd: false };
const SHIFT: Mods = Mods { shift: true, alt: false, cmd: false };
const CTRL: Mods = Mods { shift: false, alt: false, cmd: true };
const ALT: Mods = Mods { shift: false, alt: true, cmd: false };

type Pos = (usize, usize);

/// A session editing a new text box holding `text`, its runs in `lang` (none: no language), the
/// caret at the end.
fn editing(text: &str, lang: Option<&str>) -> (Session, u32) {
    let mut s = Session::with_new();
    s.mac_keys = false;
    s.prefs.autocorrect = false;
    s.prefs.smart_quotes = false;
    s.execute("insert.textBox", &json!({"rect": [60.0, 60.0, 600.0, 300.0], "text": text})).unwrap();
    let id = s.doc().unwrap().selection.shapes[0].0;
    s.execute("text.edit", &json!({"id": id})).unwrap();
    if let Some(l) = lang {
        s.execute("text.selectAll", &json!({})).unwrap();
        s.execute("format.language", &json!({"lang": l})).unwrap();
        s.execute("text.move", &json!({"to": "end"})).unwrap();
    }
    (s, id)
}

/// The runs of the first paragraph: text, `lang`, `altLang`.
fn runs(s: &Session, id: u32) -> Vec<(String, Option<String>, Option<String>)> {
    let sh = s.doc().unwrap().shape(ShapeId(id)).unwrap();
    sh.text.as_ref().unwrap().paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.props.lang.clone(), r.props.alt_lang.clone())).collect()
}

fn run(text: &str, lang: Option<&str>, alt: Option<&str>) -> (String, Option<String>, Option<String>) {
    (text.to_string(), lang.map(String::from), alt.map(String::from))
}

fn sel(s: &Session) -> (Pos, Pos) {
    let t = s.doc().unwrap().selection.text.clone().unwrap();
    (t.anchor, t.caret)
}

fn caret(s: &Session) -> Pos {
    sel(s).1
}

fn press(s: &mut Session, key: &str, mods: Mods) {
    let r = s.key(key, mods).unwrap();
    assert_eq!(r["handled"], true, "{key} {mods:?} not handled");
}

/// PowerPoint tags typed text with the input language: Japanese typed into an English run is a
/// run of its own in Japanese with the English as its alternative language; Latin typed after it
/// stays in the Japanese run, as Latin typed with the Japanese input method does.
#[test]
fn japanese_typed_into_an_english_run_is_tagged_japanese() {
    let (mut s, id) = editing("Hello ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::Japanese);
    s.execute("text.insert", &json!({"text": "日本語"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("Hello ", Some("en-US"), None), run("日本語", Some("ja-JP"), Some("en-US"))]);
    s.execute("text.insert", &json!({"text": "abc"})).unwrap();
    s.execute("text.insert", &json!({"text": "です"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("Hello ", Some("en-US"), None), run("日本語abcです", Some("ja-JP"), Some("en-US"))], "never split again");
    // Latin typed into the English text stays English.
    s.execute("text.move", &json!({"to": "start"})).unwrap();
    s.execute("text.insert", &json!({"text": "Oh, "})).unwrap();
    assert_eq!(runs(&s, id)[0], run("Oh, Hello ", Some("en-US"), None));
    // Text with no language at all takes English as the alternative.
    let (mut s, id) = editing("x", None);
    s.execute("text.insert", &json!({"text": "テスト"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("x", None, None), run("テスト", Some("ja-JP"), Some("en-US"))]);
}

/// The input method's preedit and its result are tagged the same way, and the composition is
/// still one undo step; a Latin preedit (romaji on its way to kana) stays untagged.
#[test]
fn ime_commits_are_tagged_japanese() {
    let (mut s, id) = editing("Hello ", Some("en-US"));
    let undo0 = s.doc().unwrap().history.undo.len();
    for pre in ["n", "に", "にほ", "にほんご"] {
        s.compose_text(pre, None).unwrap();
    }
    assert_eq!(runs(&s, id)[1], run("にほんご", Some("ja-JP"), Some("en-US")), "the preedit is tagged");
    s.commit_text("日本語").unwrap();
    assert_eq!(runs(&s, id), vec![run("Hello ", Some("en-US"), None), run("日本語", Some("ja-JP"), Some("en-US"))]);
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(runs(&s, id), vec![run("Hello ", Some("en-US"), None)]);
    // A preedit that stays Latin is typed into the English run.
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    s.compose_text("ok", None).unwrap();
    s.commit_text("ok").unwrap();
    assert_eq!(runs(&s, id), vec![run("Hello ok", Some("en-US"), None)]);
}

/// Symbols PowerPoint draws with the East Asian font (™ ① ★ ♥ ■) are not East Asian text: typed,
/// inserted or pasted into an English run they stay in it, and so does the Latin after them. The
/// same symbol converted by the input method from kana is Japanese input.
#[test]
fn symbols_typed_into_an_english_run_stay_english() {
    for sym in ["™", "★", "①", "♥", "■", "×", "°"] {
        let (mut s, id) = editing("Hello ", Some("en-US"));
        s.execute("insert.symbol", &json!({"text": sym})).unwrap();
        s.execute("text.insert", &json!({"text": sym})).unwrap();
        s.execute("edit.paste", &json!({"text": format!(" Acme{sym} is a brand")})).unwrap();
        assert_eq!(runs(&s, id), vec![run(&format!("Hello {sym}{sym} Acme{sym} is a brand"), Some("en-US"), None)], "{sym}");
    }
    // Converted from kana by the input method, a symbol is Japanese input.
    let (mut s, id) = editing("Hello ", Some("en-US"));
    s.compose_text("ほし", None).unwrap();
    s.commit_text("★").unwrap();
    assert_eq!(runs(&s, id)[1], run("★", Some("ja-JP"), Some("en-US")));
    // An input method's result with no preedit (the emoji picker) is typed as it is.
    let (mut s, id) = editing("I ", Some("en-US"));
    s.commit_text("♥").unwrap();
    s.execute("text.insert", &json!({"text": " NY"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("I ♥ NY", Some("en-US"), None)]);
    // With a Japanese editing language the symbols still aren't Japanese text; full-width forms
    // and CJK punctuation are.
    let (mut s, id) = editing("A ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::Japanese);
    s.execute("text.insert", &json!({"text": "™"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("A ™", Some("en-US"), None)]);
    s.execute("text.insert", &json!({"text": "ＡＢＣ、"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("ＡＢＣ、", Some("ja-JP"), Some("en-US")));
}

/// Han text takes the East Asian editing language, or the one the input method's preedit showed
/// (kana converted to kanji is Japanese, Bopomofo Traditional Chinese); kana is always Japanese
/// and Hangul Korean; without any of them Han text is not guessed and stays in the run's language.
/// Text in a run already in an East Asian language stays in it.
#[test]
fn the_east_asian_editing_language_names_han_text() {
    let (mut s, id) = editing("A ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::SimplifiedChinese);
    s.execute("text.insert", &json!({"text": "中文"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("中文", Some("zh-CN"), Some("en-US")));
    s.execute("text.insert", &json!({"text": "汉字"})).unwrap();
    assert_eq!(runs(&s, id).len(), 2, "the run is already Chinese");
    let (mut s, id) = editing("A ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::SimplifiedChinese);
    s.execute("text.insert", &json!({"text": "かな"})).unwrap();
    assert_eq!(runs(&s, id)[1].1.as_deref(), Some("ja-JP"), "kana is Japanese");
    let (mut s, id) = editing("A ", Some("en-US"));
    s.execute("text.insert", &json!({"text": "한국어"})).unwrap();
    assert_eq!(runs(&s, id)[1].1.as_deref(), Some("ko-KR"), "Hangul is Korean");
    // Without an editing language, Han text typed or pasted stays in the run (it may be Chinese).
    let (mut s, id) = editing("Title ", Some("en-US"));
    s.execute("text.insert", &json!({"text": "中文标题"})).unwrap();
    s.execute("edit.paste", &json!({"text": "漢字。"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("Title 中文标题漢字。", Some("en-US"), None)]);
    // A pinyin preedit is Latin: its Han result takes the editing language, else stays untagged.
    let (mut s, id) = editing("A ", Some("en-US"));
    s.compose_text("zhongwen", None).unwrap();
    s.commit_text("中文").unwrap();
    assert_eq!(runs(&s, id), vec![run("A 中文", Some("en-US"), None)]);
    let (mut s, id) = editing("A ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::SimplifiedChinese);
    s.compose_text("zhongwen", None).unwrap();
    s.commit_text("中文").unwrap();
    assert_eq!(runs(&s, id)[1], run("中文", Some("zh-CN"), Some("en-US")));
    // The preedit's kana or Bopomofo names the converted Han text, whatever the editing language.
    let (mut s, id) = editing("A ", Some("en-US"));
    s.east_asian_editing = Some(CjkLocale::SimplifiedChinese);
    for pre in ["k", "か", "かん", "かんじ"] {
        s.compose_text(pre, None).unwrap();
    }
    s.commit_text("漢字").unwrap();
    assert_eq!(runs(&s, id)[1], run("漢字", Some("ja-JP"), Some("en-US")));
    let (mut s, id) = editing("A ", Some("en-US"));
    s.compose_text("ㄓㄨㄥ", None).unwrap();
    s.commit_text("中").unwrap();
    assert_eq!(runs(&s, id)[1], run("中", Some("zh-TW"), Some("en-US")));
    // A Japanese deck's Latin run (lang en-US, altLang ja-JP) gives its Japanese the English back.
    let (mut s, id) = editing("ok", Some("de-DE"));
    s.east_asian_editing = Some(CjkLocale::Japanese);
    s.execute("text.insert", &json!({"text": "。"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("。", Some("ja-JP"), Some("de-DE")));
}

/// Runs without a language of their own are in the deck's default language (`a:defPPr`): Han
/// text typed into a Chinese deck's text stays in it, and Japanese typed into a German deck's
/// text takes German as its alternative.
#[test]
fn typed_text_inherits_the_decks_default_language() {
    let deck_default = |s: &mut Session, lang: &str| {
        let lang = lang.to_string();
        s.edit(|doc, _| {
            let run = deckcraft_model::text::RunProps { lang: Some(lang), ..Default::default() };
            doc.default_text_style.default = Some(deckcraft_model::text::LevelStyle { run, ..Default::default() });
            Ok(())
        })
        .unwrap();
    };
    let (mut s, id) = editing("x", None);
    deck_default(&mut s, "zh-CN");
    s.execute("text.insert", &json!({"text": "汉字"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("x汉字", None, None)]);
    let (mut s, id) = editing("x", None);
    deck_default(&mut s, "de-DE");
    s.east_asian_editing = Some(CjkLocale::Japanese);
    s.execute("text.insert", &json!({"text": "日本語"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("日本語", Some("ja-JP"), Some("de-DE")));
    // In a Japanese deck the untagged text is Japanese: it stays one run and moves by words.
    let (mut s, id) = editing("", None);
    deck_default(&mut s, "ja-JP");
    s.execute("text.insert", &json!({"text": "日本語のテキスト"})).unwrap();
    assert_eq!(runs(&s, id), vec![run("日本語のテキスト", None, None)]);
    s.execute("text.move", &json!({"to": "start"})).unwrap();
    press(&mut s, "Right", CTRL);
    assert_eq!(caret(&s), (0, 3));
    s.execute("text.selectWord", &json!({"at": [0, 5]})).unwrap();
    assert_eq!(sel(&s), ((0, 4), (0, 8)));
}

/// Plain text pasted from another application is tagged like typed text; a saved file keeps
/// the language pair.
#[test]
fn pasted_japanese_is_tagged_and_saved_with_its_alternative_language() {
    let (mut s, id) = editing("See: ", Some("en-US"));
    s.execute("edit.paste", &json!({"text": "日本語のテキスト"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("日本語のテキスト", Some("ja-JP"), Some("en-US")));
    let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
    let back = deckcraft_pptx::import(&bytes).unwrap();
    let found = back
        .slides
        .iter()
        .flat_map(|sl| sl.shapes.iter())
        .filter_map(|sh| sh.text.as_ref())
        .flat_map(|t| t.paragraphs.iter())
        .flat_map(|p| p.runs.iter())
        .find(|r| r.text == "日本語のテキスト")
        .cloned()
        .unwrap();
    assert_eq!((found.props.lang.as_deref(), found.props.alt_lang.as_deref()), (Some("ja-JP"), Some("en-US")));
}

/// Latin typed after Japanese stays in the Japanese run; spelling checks its words in the run's
/// alternative language, as Office proofs Latin typed with the Japanese input method.
#[test]
fn latin_typed_after_japanese_is_still_spell_checked() {
    crate::spell::install("xx-JAALT", crate::spell::Dictionary::parse(crate::spell::tests::AFF.as_bytes(), crate::spell::tests::DIC.as_bytes()));
    let (mut s, id) = editing("slide ", Some("xx-JAALT"));
    s.east_asian_editing = Some(CjkLocale::Japanese);
    s.execute("text.insert", &json!({"text": "日本語"})).unwrap();
    s.execute("text.insert", &json!({"text": " thsi slidez slides"})).unwrap();
    assert_eq!(runs(&s, id)[1], run("日本語 thsi slidez slides", Some("ja-JP"), Some("xx-JAALT")));
    let v = s.execute("review.spelling", &json!({})).unwrap();
    let words: Vec<(&str, &str)> = v.as_array().unwrap().iter().map(|w| (w["word"].as_str().unwrap(), w["lang"].as_str().unwrap())).collect();
    assert_eq!(words, vec![("thsi", "xx-JAALT"), ("slidez", "xx-JAALT")]);
}

/// Japanese text moves and selects by runs of kanji, kana and katakana.
#[test]
fn ctrl_arrows_and_double_click_group_typed_japanese() {
    let (mut s, _) = editing("", None);
    s.execute("text.insert", &json!({"text": "日本語のテキストです。"})).unwrap();
    s.execute("text.move", &json!({"to": "start"})).unwrap();
    let mut seen = vec![caret(&s).1];
    for _ in 0..6 {
        press(&mut s, "Right", CTRL);
        seen.push(caret(&s).1);
    }
    assert_eq!(seen, vec![0, 3, 4, 8, 10, 11, 11]);
    s.execute("text.selectWord", &json!({"at": [0, 5]})).unwrap();
    assert_eq!(sel(&s), ((0, 4), (0, 8)));
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    press(&mut s, "Backspace", CTRL);
    press(&mut s, "Backspace", CTRL);
    let t = s.execute("text.get", &json!({})).unwrap();
    assert_eq!(t["text"], "日本語のテキスト");
}

/// Find and Replace match half- and full-width forms by default; `matchWidth` keeps them apart.
#[test]
fn find_and_replace_fold_widths_unless_told_not_to() {
    let (mut s, id) = editing("ＡＢＣ abc ｶﾞｲﾄﾞ ガイド", None);
    let hits = s.execute("text.find", &json!({"text": "abc"})).unwrap();
    let spans: Vec<(u64, u64)> = hits.as_array().unwrap().iter().map(|h| (h["start"].as_u64().unwrap(), h["end"].as_u64().unwrap())).collect();
    assert_eq!(spans, vec![(0, 3), (4, 7)]);
    let hits = s.execute("text.find", &json!({"text": "abc", "matchWidth": true})).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1);
    let hits = s.execute("text.find", &json!({"text": "ABC", "matchCase": true})).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1, "match case still tells ＡＢＣ from abc");
    let r = s.execute("text.replace", &json!({"text": "ガイド", "with": "案内"})).unwrap();
    assert_eq!(r["replaced"], 2);
    let t = s.execute("text.get", &json!({"id": id})).unwrap();
    assert_eq!(t["text"], "ＡＢＣ abc 案内 案内");
}

fn shift_jis(text: &str) -> Vec<u8> {
    encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
}

fn utf16(text: &str, le: bool) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| if le { u.to_le_bytes() } else { u.to_be_bytes() }).collect()
}

/// Text outlines in the encodings Japanese Windows saves: UTF-8 with or without a BOM,
/// UTF-16 with or without one, and Shift_JIS (CP932).
#[test]
fn text_outlines_are_read_in_japanese_encodings() {
    use crate::cmd::file::decode_text_in;
    let t = "タイトル\r\n\t本文の一行目\r\n次のスライド";
    let ja = Some(CjkLocale::Japanese);
    for le in [true, false] {
        let mut bom = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        bom.extend(utf16(t, le));
        assert_eq!(decode_text_in(&bom, None), t, "UTF-16 with a BOM, le {le}");
        assert_eq!(decode_text_in(&utf16(t, le), None), t, "UTF-16 without a BOM, le {le}");
    }
    let mut utf8 = vec![0xEF, 0xBB, 0xBF];
    utf8.extend(t.as_bytes());
    assert_eq!(decode_text_in(&utf8, None), t, "the BOM is not text");
    assert_eq!(decode_text_in(t.as_bytes(), None), t);
    assert_eq!(decode_text_in(&shift_jis(t), ja), t);
    assert_eq!(decode_text_in(&shift_jis(t), None), t, "Shift_JIS without Japanese editing");
    assert_eq!(decode_text_in(&shift_jis("ｶﾀｶﾅ\r\nテスト"), None), "ｶﾀｶﾅ\r\nテスト");
    // Kanji alone (slide titles) read as Japanese too.
    let kanji = "事業計画\r\n\t売上目標\r\n\t人員計画";
    assert_eq!(decode_text_in(&shift_jis(kanji), None), kanji);
    assert_eq!(decode_text_in(&shift_jis("2026年度Q3売上報告"), None), "2026年度Q3売上報告");
    // Western text in Windows-1252 stays Western: its capitals would read as half-width katakana,
    // a small accented letter (é à ç ê) and the letter after it as a kanji, ñ ö as user-defined
    // characters.
    for w in ["Ärger Über Öl", "Présentation\r\nRéunion\r\nStratégie\r\n", "Données clés\r\nDéveloppement", "año señor niño", "Rêve", "ça va"]
    {
        let western = encoding_rs::WINDOWS_1252.encode(w).0.into_owned();
        assert_eq!(decode_text_in(&western, None), w);
    }
    assert_eq!(decode_text_in(&encoding_rs::EUC_KR.encode("한국어 제목").0, Some(CjkLocale::Korean)), "한국어 제목");
    assert_eq!(decode_text_in(&[], None), "");
    assert_eq!(decode_text_in(&[0xFF, 0xFE, 0x41], None), "\u{FFFD}", "an odd byte is never a crash");
    // Opening the outline makes its slides.
    let mut s = Session::new();
    let data = crate::cmd::base64_encode(&shift_jis(t));
    s.execute("file.openBytes", &json!({"name": "outline.txt", "data": data})).unwrap();
    let doc = &s.doc().unwrap().doc;
    let texts: Vec<String> =
        doc.slides.iter().flat_map(|sl| sl.shapes.iter()).filter_map(|sh| sh.text.as_ref()).map(|t| t.text()).filter(|t| !t.is_empty()).collect();
    assert_eq!(texts, vec!["タイトル", "本文の一行目", "次のスライド"]);
    let mut s = Session::new();
    let data = crate::cmd::base64_encode(&utf8);
    s.execute("file.openBytes", &json!({"name": "outline.txt", "data": data})).unwrap();
    let first = s.doc().unwrap().doc.slides[0].shapes.iter().find_map(|sh| sh.text.as_ref().map(|t| t.text()).filter(|t| !t.is_empty())).unwrap();
    assert_eq!(first, "タイトル", "no BOM at the start of the first title");
}

/// A session editing a 600 × 300 pt text box turned `dir` holding two paragraphs (two lines,
/// none wrapped), caret at `at`.
fn vertical(dir: TextDir, text: &str, at: Pos) -> Session {
    let (mut s, id) = editing("x", None);
    s.execute("text.set", &json!({"id": id, "text": text})).unwrap();
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).unwrap();
        let sh = deckcraft_model::find_shape_mut(list, ShapeId(id)).unwrap();
        let body = &mut sh.text.as_mut().unwrap().body;
        body.vert = Some(dir);
        body.autofit = None;
        if let Some(x) = sh.xfrm.as_mut() {
            (x.w, x.h) = (600.0, 300.0);
        }
        Ok(())
    })
    .unwrap();
    s.execute("text.edit", &json!({"id": id, "at": [at.0, at.1]})).unwrap();
    s
}

/// In vertical text the arrows follow what is drawn, as in PowerPoint: in `eaVert` and `vert`
/// Down goes to the next character and Up to the previous one, Left to the next column (lines
/// run from right to left) and Right back; Shift extends the same way.
#[test]
fn arrow_keys_turn_with_vertical_text() {
    for dir in [TextDir::EaVertical, TextDir::Vertical, TextDir::StackedRtl] {
        let mut s = vertical(dir, "あいう\nかきく", (0, 0));
        press(&mut s, "ArrowDown", NONE);
        assert_eq!(caret(&s), (0, 1), "{dir:?}: Down is the next character");
        press(&mut s, "ArrowLeft", NONE);
        assert_eq!(caret(&s), (1, 1), "{dir:?}: Left is the next column");
        press(&mut s, "ArrowUp", NONE);
        assert_eq!(caret(&s), (1, 0), "{dir:?}: Up is the previous character");
        press(&mut s, "ArrowRight", NONE);
        assert_eq!(caret(&s), (0, 0), "{dir:?}: Right is the previous column");
        press(&mut s, "ArrowDown", SHIFT);
        press(&mut s, "ArrowDown", SHIFT);
        assert_eq!(sel(&s), ((0, 0), (0, 2)), "{dir:?}: Shift+Down extends down the column");
    }
    // vert270 runs up the box, its lines from left to right.
    let mut s = vertical(TextDir::Vertical270, "abc\ndef", (0, 0));
    press(&mut s, "ArrowUp", NONE);
    assert_eq!(caret(&s), (0, 1));
    press(&mut s, "ArrowRight", NONE);
    assert_eq!(caret(&s), (1, 1));
    press(&mut s, "ArrowDown", NONE);
    assert_eq!(caret(&s), (1, 0));
    // Stacked (wordArtVert) and Mongolian text lay their lines from left to right.
    for dir in [TextDir::Stacked, TextDir::MongolianVertical] {
        let mut s = vertical(dir, "あいう\nかきく", (0, 1));
        press(&mut s, "ArrowRight", NONE);
        assert_eq!(caret(&s), (1, 1), "{dir:?}: Right is the next line");
        press(&mut s, "ArrowLeft", NONE);
        assert_eq!(caret(&s), (0, 1), "{dir:?}: Left is the previous line");
        press(&mut s, "ArrowDown", NONE);
        assert_eq!(caret(&s), (0, 2), "{dir:?}: Down is the next character");
    }
    // text.move takes on-screen directions too.
    let mut s = vertical(TextDir::EaVertical, "あいう\nかきく", (0, 0));
    s.execute("text.move", &json!({"to": "down"})).unwrap();
    assert_eq!(caret(&s), (0, 1));
    // Horizontal text is unchanged.
    let mut s = vertical(TextDir::Horizontal, "abc\ndef", (0, 1));
    press(&mut s, "ArrowDown", NONE);
    assert_eq!(caret(&s), (1, 1));
}

/// The word and paragraph keys turn too: Ctrl+Down (Option+Down on the Mac) moves by word down
/// the column and Ctrl+Left to the next paragraph, the next column on.
#[test]
fn word_and_paragraph_keys_turn_with_vertical_text() {
    let mut s = vertical(TextDir::EaVertical, "abc def\nxyz", (0, 0));
    press(&mut s, "ArrowDown", CTRL);
    assert_eq!(caret(&s), (0, 4));
    press(&mut s, "ArrowUp", CTRL);
    assert_eq!(caret(&s), (0, 0));
    press(&mut s, "ArrowLeft", CTRL);
    assert_eq!(caret(&s), (1, 0));
    press(&mut s, "ArrowRight", CTRL);
    assert_eq!(caret(&s), (0, 0));
    s.mac_keys = true;
    press(&mut s, "ArrowDown", ALT);
    assert_eq!(caret(&s), (0, 4));
    press(&mut s, "ArrowLeft", ALT);
    assert_eq!(caret(&s), (0, 7), "Option+Left: the end of the paragraph");
}
