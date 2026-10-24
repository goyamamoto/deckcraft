//! Which font draws each character of East Asian text: the theme's font for the run's tradition
//! filling an empty `a:ea`, the slots of the characters Latin and East Asian fonts share, and
//! synthetic bold and italic decided per face. The layout tests need two installed or bundled
//! Japanese faces and skip without them.

use super::*;
use deckcraft_model::defaults;
use deckcraft_model::text::{Paragraph, Run};
use deckcraft_model::theme::{FontSet, ScriptFont};
use deckcraft_model::{LayoutType, Presentation, Xfrm};

const J: Option<CjkLocale> = Some(CjkLocale::Japanese);

/// A slide with one text box (no bullet, no indent) holding `runs`, under a theme whose body and
/// heading font sets are `minor` and `major`.
fn deck(runs: Vec<Run>, minor: FontSet, major: FontSet) -> (Presentation, Shape) {
    let mut p = Presentation::default();
    let lay = defaults::layout_of_kind(&p, LayoutType::TitleAndContent).unwrap();
    let mut s = defaults::new_slide(&mut p, lay).expect("slide");
    let mut body = s.shapes.remove(1);
    body.xfrm = Some(Xfrm::new(0.0, 0.0, 2000.0, 300.0));
    let mut t = TextBody::from_text("x");
    t.paragraphs = vec![Paragraph { runs, ..Default::default() }];
    let pp = &mut t.paragraphs[0].props;
    pp.bullet = Some(Bullet::None);
    pp.margin_left = Some(0.0);
    pp.indent = Some(0.0);
    body.text = Some(t);
    s.shapes = vec![body.clone()];
    p.slides = vec![Arc::new(s)];
    for m in &mut p.masters {
        let m = Arc::make_mut(m);
        m.theme.fonts.minor = minor.clone();
        m.theme.fonts.major = major.clone();
    }
    (p, body)
}

fn lay(p: &Presentation, sh: &Shape) -> TextLayout {
    let ctx = Ctx::for_slide(p, &p.slides[0]).unwrap();
    layout(
        &ctx,
        sh,
        sh.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, 2000.0, 300.0), fields: &NoFields, prompt_color: None, no_shrink: false },
    )
}

/// A theme font set with an empty `a:ea` and the given script fonts.
fn font_set(latin: &str, scripts: &[(&str, &str)]) -> FontSet {
    FontSet {
        latin: latin.into(),
        ea: String::new(),
        cs: String::new(),
        scripts: scripts.iter().map(|(s, f)| ScriptFont { script: (*s).into(), typeface: (*f).into() }).collect(),
    }
}

fn run(text: &str, f: impl FnOnce(&mut RunProps)) -> Run {
    let mut r = Run::new(text);
    f(&mut r.props);
    r
}

/// The family drawing each character, in paragraph order.
fn families(l: &TextLayout) -> Vec<(char, String)> {
    let mut v: Vec<(usize, char, String)> = l.runs.iter().flat_map(|r| r.cells.iter().map(|(i, c)| (*i, *c, r.face.family.clone()))).collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|(_, c, f)| (c, f)).collect()
}

fn family_of(l: &TextLayout, ch: char) -> String {
    families(l).into_iter().find(|(c, _)| *c == ch).map(|(_, f)| f).unwrap_or_default()
}

/// Installed or bundled Japanese families that draw `sample` (and its full-width letters and
/// curly quotes), never the default Latin face.
fn japanese_families() -> Vec<String> {
    let db = FontDb::global();
    let sample: Vec<char> = "漢字かなカナＰｏｗｅｒ（）“”1(".chars().collect();
    [
        "BIZ UDPGothic",
        "BIZ UDMincho",
        "Shippori Mincho",
        "Hiragino Sans",
        "Hiragino Mincho ProN",
        "Hiragino Maru Gothic ProN",
        "Hiragino Kaku Gothic ProN",
        "Osaka",
        "IPAexGothic",
        "IPAexMincho",
        "IPAGothic",
        "TakaoGothic",
        "VL Gothic",
    ]
    .iter()
    .filter(|f| db.has_family(f))
    .map(|f| db.face(f, "Regular"))
    .filter(|f| sample.iter().all(|c| f.covers(*c)))
    .map(|f| f.family.clone())
    .fold(Vec::new(), |mut v, f| {
        if !v.contains(&f) {
            v.push(f);
        }
        v
    })
}

#[test]
fn a_runs_tradition_comes_from_its_language_then_the_system_then_its_paragraph() {
    let s = Some(CjkLocale::SimplifiedChinese);
    let k = Some(CjkLocale::Korean);
    // lang or altLang ja-JP: Japanese, and the theme's Jpan font fills an empty a:ea.
    assert_eq!(run_traditions(Some("ja-JP"), None, None, None), (J, J));
    assert_eq!(run_traditions(Some("en-US"), Some("ja-JP"), None, None), (J, J));
    assert_eq!(run_traditions(Some("ja-JP"), Some("en-US"), k, k), (J, J));
    // A Chinese run keeps its own tradition on a Japanese system too, where PowerPoint falls back
    // to a Japanese face (a deliberate difference, see `run_traditions`).
    assert_eq!(run_traditions(Some("zh-CN"), None, J, None), (s, s));
    assert_eq!(run_traditions(Some("ko-KR"), None, None, None), (k, k));
    // An en-US run's kanji fall back to the system's tradition, never to the theme font.
    assert_eq!(run_traditions(Some("en-US"), None, J, None), (J, None));
    assert_eq!(run_traditions(Some("en-US"), Some("en-US"), None, J), (J, None));
    assert_eq!(run_traditions(Some("en-US"), None, None, None), (None, None));
    // No language at all: the system's East Asian language, else the paragraph's kana/Hangul.
    assert_eq!(run_traditions(None, None, J, k), (J, J));
    assert_eq!(run_traditions(None, None, None, k), (k, k));
    assert_eq!(run_traditions(Some(""), None, None, J), (J, J));
    assert_eq!(run_traditions(None, None, None, None), (None, None));
}

#[test]
fn an_empty_ea_takes_the_theme_font_of_the_runs_tradition() {
    let minor = font_set("Body Latin", &[("Jpan", "Body Jpan"), ("Hans", "Body Hans"), ("Hang", "Body Hang"), ("Arab", "Body Arab")]);
    let major = font_set("Head Latin", &[("Jpan", "Head Jpan")]);
    let (p, _) = deck(vec![Run::new("x")], minor, major);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let ea = |f: &dyn Fn(&mut RunProps)| {
        let mut r = RunProps::default();
        f(&mut r);
        make_style(&ctx, &r, 1.0, None, None).ea
    };
    let some = |s: &str| Some(s.to_string());
    assert_eq!(ea(&|r| r.lang = some("ja-JP")), some("Body Jpan"));
    assert_eq!(ea(&|r| r.lang = some("zh-CN")), some("Body Hans"));
    assert_eq!(ea(&|r| r.lang = some("ko-KR")), some("Body Hang"));
    // Typed with a Japanese input method into English text: altLang.
    assert_eq!(
        ea(&|r| {
            r.lang = some("en-US");
            r.alt_lang = some("ja-JP");
        }),
        some("Body Jpan")
    );
    // English text keeps no East Asian font, whatever the system's language.
    assert_eq!(ea(&|r| r.lang = some("en-US")), None);
    // No Hant entry in the theme: nothing to fill with.
    assert_eq!(ea(&|r| r.lang = some("zh-TW")), None);
    // Headings take the major set's entry, by their Latin or their East Asian reference.
    assert_eq!(
        ea(&|r| {
            r.lang = some("ja-JP");
            r.font = some("+mj-lt");
        }),
        some("Head Jpan")
    );
    assert_eq!(
        ea(&|r| {
            r.lang = some("ja-JP");
            r.font_ea = some("+mj-ea");
        }),
        some("Head Jpan")
    );
    // A typeface named in a:ea, or by the theme's a:ea, wins.
    assert_eq!(
        ea(&|r| {
            r.lang = some("ja-JP");
            r.font_ea = some("Explicit EA");
        }),
        some("Explicit EA")
    );
    let mut with_ea = font_set("Body Latin", &[("Jpan", "Body Jpan")]);
    with_ea.ea = "Theme EA".into();
    let (p2, _) = deck(vec![Run::new("x")], with_ea, FontSet::default());
    let ctx2 = Ctx::for_slide(&p2, &p2.slides[0]).unwrap();
    let r = RunProps { lang: some("ja-JP"), ..Default::default() };
    assert_eq!(make_style(&ctx2, &r, 1.0, None, None).ea, some("Theme EA"));
}

#[test]
fn japanese_runs_draw_in_the_theme_jpan_font() {
    let fams = japanese_families();
    let [a, b, ..] = fams.as_slice() else {
        eprintln!("skipped: needs two Japanese faces, found {fams:?}");
        return;
    };
    let text = "漢字とかな";
    let draw = |jpan: &str, lang: &str| {
        let r = run(text, |p| p.lang = Some(lang.into()));
        let (p, sh) = deck(vec![r], font_set("Arial", &[("Jpan", jpan)]), FontSet::default());
        families(&lay(&p, &sh))
    };
    for jpan in [a, b] {
        for (c, f) in draw(jpan, "ja-JP") {
            assert_eq!(&f, jpan, "{c:?} of a ja-JP run");
        }
    }
    // An en-US run doesn't consult the theme's Jpan font: changing it changes nothing.
    assert_eq!(draw(a, "en-US"), draw(b, "en-US"));
}

#[test]
fn ascii_and_full_width_take_powerpoints_slots() {
    let fams = japanese_families();
    let Some(ea) = fams.first() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let r = run("第1章 日本(Japan) ＰｏｗｅｒＰｏｉｎｔ", |p| {
        p.lang = Some("ja-JP".into());
        p.font_ea = Some(ea.clone());
    });
    let (p, sh) = deck(vec![r], font_set("Arial", &[]), FontSet::default());
    let l = lay(&p, &sh);
    let latin = family_of(&l, 'J');
    assert_ne!(&latin, ea, "the Latin letters are drawn with the Latin face");
    for (c, f) in families(&l) {
        let want = if "第章日本ＰｏｗｅｒＰｏｉｎｔ".contains(c) { ea } else { &latin };
        assert_eq!(&f, want, "{c:?}");
    }
}

#[test]
fn curly_quotes_follow_the_runs_language() {
    let fams = japanese_families();
    let Some(ea) = fams.first() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    let draw = |lang: &str| {
        let r = run("と“Hi”", |p| {
            p.lang = Some(lang.into());
            p.font_ea = Some(ea.clone());
        });
        let (p, sh) = deck(vec![r], font_set("Arial", &[]), FontSet::default());
        lay(&p, &sh)
    };
    let ja = draw("ja-JP");
    assert_eq!(&family_of(&ja, '“'), ea);
    assert_eq!(&family_of(&ja, '”'), ea);
    let en = draw("en-US");
    let latin = family_of(&en, 'H');
    assert_eq!(family_of(&en, '“'), latin);
    assert_eq!(family_of(&en, '”'), latin);
}

#[test]
fn synthetic_bold_and_italic_are_decided_per_face() {
    let fams = japanese_families();
    let Some(ea) = fams.first() else {
        eprintln!("skipped: no Japanese face");
        return;
    };
    for (bold, italic) in [(true, false), (false, true), (true, true)] {
        let r = run("Bold 漢字", |p| {
            p.lang = Some("ja-JP".into());
            p.font_ea = Some(ea.clone());
            p.bold = Some(bold);
            p.italic = Some(italic);
        });
        let (p, sh) = deck(vec![r], font_set("Arial", &[]), FontSet::default());
        let l = lay(&p, &sh);
        let kanji = l.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == '漢')).unwrap();
        let latin = l.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == 'B')).unwrap();
        assert_ne!(kanji.face.family, latin.face.family);
        for r in [kanji, latin] {
            assert_eq!(r.fake_bold, bold && r.face.weight < 550.0, "{} weight {}", r.face.family, r.face.weight);
            assert_eq!(r.fake_italic, italic && !r.face.italic, "{}", r.face.family);
        }
    }
    // The rule itself, whichever faces are installed.
    let db = FontDb::global();
    let regular = db.face(ea, "Regular");
    assert_eq!(synthetic_style(true, true, &regular), (regular.weight < 550.0, !regular.italic));
    assert_eq!(synthetic_style(false, false, &regular), (false, false));
}

#[test]
fn kana_in_a_paragraph_without_a_language_selects_the_jpan_font() {
    // Only without a system East Asian language: the paragraph's kana then tell the tradition.
    if deckcraft_fonts::default_cjk_locale().is_some() {
        return;
    }
    let fams = japanese_families();
    let [a, b, ..] = fams.as_slice() else {
        eprintln!("skipped: needs two Japanese faces, found {fams:?}");
        return;
    };
    for jpan in [a, b] {
        let (p, sh) = deck(vec![Run::new("漢字とかな")], font_set("Arial", &[("Jpan", jpan)]), FontSet::default());
        for (c, f) in families(&lay(&p, &sh)) {
            assert_eq!(&f, jpan, "{c:?}");
        }
    }
    // Kanji alone tell no tradition: the theme font isn't used.
    let draw = |jpan: &str| {
        let (p, sh) = deck(vec![Run::new("漢字")], font_set("Arial", &[("Jpan", jpan)]), FontSet::default());
        families(&lay(&p, &sh))
    };
    assert_eq!(draw(a), draw(b));
}
