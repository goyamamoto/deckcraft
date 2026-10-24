//! Fonts a presentation uses that this machine doesn't have, so opening it can say so (its text
//! draws in a stand-in; see `FontDb::document_face`).

use deckcraft_fonts::FontDb;
use deckcraft_model::resolve::{self, Ctx};
use deckcraft_model::text::{Bullet, TextBody};
use deckcraft_model::{Presentation, Shape, ShapeKind};

/// Load each embedded face into the font database, so text in that family draws with the file's
/// font on a machine that doesn't have it. A face already loaded (bundled or installed) stays;
/// the part bytes are kept either way. MicroType Express subsets have no TrueType or OpenType
/// font to load and are left for the round trip.
pub fn install_embedded_fonts(p: &Presentation) {
    let db = FontDb::global();
    for font in &p.embedded_fonts {
        for id in font.faces().into_iter().flatten() {
            let Some(item) = p.media(id) else { continue };
            let Some(sfnt) = deckcraft_fonts::embedded_sfnt(&item.data) else { continue };
            db.add_font(sfnt.to_vec());
        }
    }
}

/// The font families the slides' text is drawn in (theme fonts resolved, bullet fonts and the
/// East Asian and complex-script fonts included) that aren't installed or bundled under any of
/// their names (an installed font named メイリオ or ＭＳ Ｐゴシック is there), sorted, without
/// duplicates (case-insensitive).
pub fn missing_fonts(p: &Presentation) -> Vec<String> {
    let db = FontDb::global();
    let mut out: Vec<String> = Vec::new();
    for family in used_fonts(p) {
        if !db.has_family(&family) && !out.iter().any(|f| f.eq_ignore_ascii_case(&family)) {
            out.push(family);
        }
    }
    out.sort_by_key(|f| f.to_lowercase());
    out
}

/// The families the slides' text runs and bullets are drawn in: each run's Latin font, and its
/// East Asian (`a:ea`) and complex-script (`a:cs`) fonts when it has text in those slots, as
/// PowerPoint's Replace Fonts lists ＭＳ Ｐゴシック beside Calibri.
pub fn used_fonts(p: &Presentation) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |f: String| {
        let f = f.trim().to_string();
        if !f.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&f)) {
            out.push(f);
        }
    };
    for slide in &p.slides {
        let Some(ctx) = Ctx::for_slide(p, slide) else { continue };
        deckcraft_model::walk(&slide.shapes, &mut |s, _| {
            if let Some(body) = &s.text {
                body_fonts(&ctx, s, body, &mut add);
            }
            if let ShapeKind::Table(t) = &s.kind {
                for cell in t.rows.iter().flat_map(|r| &r.cells) {
                    body_fonts(&ctx, s, &cell.text, &mut add);
                }
            }
        });
    }
    out
}

/// The fonts of each run with text: its Latin font, and the East Asian font of a run with
/// Japanese, Chinese or Korean text (its `a:ea`, the theme's, or the theme's font for its
/// language) and the complex-script font of a run with Arabic, Hebrew, Thai… text, as
/// PowerPoint's Replace Fonts lists the East Asian fonts a deck uses too.
fn body_fonts(ctx: &Ctx, s: &Shape, body: &TextBody, add: &mut dyn FnMut(String)) {
    use deckcraft_fonts::FontSlot;
    for para in &body.paragraphs {
        let has_text = para.runs.iter().any(|r| !r.text.trim().is_empty());
        if !has_text {
            continue;
        }
        for run in para.runs.iter().filter(|r| !r.text.trim().is_empty()) {
            let r = resolve::run(ctx, s, para, &run.props);
            add(resolve::font_family(ctx, &r));
            let has = |slot: FontSlot| run.text.chars().any(|c| deckcraft_fonts::script::slot_of(c) == Some(slot));
            if has(FontSlot::EastAsian)
                && let Some(f) = crate::cmd::format::east_asian_family(ctx, &r)
            {
                add(f);
            }
            if has(FontSlot::ComplexScript) {
                let major = r.font.as_deref().is_some_and(|f| f.starts_with("+mj"));
                add(ctx.master.theme.font(r.font_cs.as_deref().unwrap_or(if major { "+mj-cs" } else { "+mn-cs" })));
            }
        }
        let pp = resolve::para(ctx, s, para);
        if matches!(pp.bullet, Some(Bullet::Char { .. }))
            && let Some(f) = pp.bullet_font.as_deref()
        {
            add(ctx.master.theme.font(f));
        }
    }
}

/// Replace the font family `from` with `to` everywhere it is named explicitly: text runs (and
/// their east Asian, complex script and symbol faces), bullet fonts, paragraph end marks, list
/// styles of shapes, layouts and masters, the master text styles, the deck default style, speaker
/// notes, tables and the theme fonts. Names compare case-insensitively; theme references
/// (`+mn-lt`) are left alone. Returns how many names changed.
pub fn replace_font(p: &mut Presentation, from: &str, to: &str) -> usize {
    use std::sync::Arc;
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() || to.is_empty() || from.starts_with('+') || from == to {
        return 0;
    }
    let mut n = 0usize;
    let mut fix = |f: &mut Option<String>| {
        if f.as_deref().is_some_and(|x| x.trim().eq_ignore_ascii_case(from)) {
            *f = Some(to.to_string());
            n = n.saturating_add(1);
        }
    };
    replace_list(&mut p.default_text_style, &mut fix);
    for slide in &mut p.slides {
        let sl = Arc::make_mut(slide);
        replace_shapes(&mut sl.shapes, &mut fix);
        replace_body(&mut sl.notes, &mut fix);
    }
    let mut theme_hits = 0usize;
    let masters = p.masters.iter_mut().chain(p.notes_master.iter_mut()).chain(p.handout_master.iter_mut());
    for m in masters {
        let m = Arc::make_mut(m);
        replace_shapes(&mut m.shapes, &mut fix);
        for l in [&mut m.title_style, &mut m.body_style, &mut m.other_style] {
            replace_list(l, &mut fix);
        }
        for lay in &mut m.layouts {
            replace_shapes(&mut lay.shapes, &mut fix);
        }
        for set in [&mut m.theme.fonts.major, &mut m.theme.fonts.minor] {
            for face in [&mut set.latin, &mut set.ea, &mut set.cs].into_iter().chain(set.scripts.iter_mut().map(|s| &mut s.typeface)) {
                if face.trim().eq_ignore_ascii_case(from) {
                    *face = to.to_string();
                    theme_hits = theme_hits.saturating_add(1);
                }
            }
        }
    }
    n.saturating_add(theme_hits)
}

fn replace_shapes(shapes: &mut [Shape], fix: &mut dyn FnMut(&mut Option<String>)) {
    for s in shapes {
        if let Some(body) = &mut s.text {
            replace_body(body, fix);
        }
        if let ShapeKind::Table(t) = &mut s.kind {
            for cell in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                replace_body(&mut cell.text, fix);
            }
        }
        if let Some(children) = s.children_mut() {
            replace_shapes(children, fix);
        }
    }
}

fn replace_body(body: &mut TextBody, fix: &mut dyn FnMut(&mut Option<String>)) {
    replace_list(&mut body.list_style, fix);
    for para in &mut body.paragraphs {
        fix(&mut para.props.bullet_font);
        replace_run(&mut para.end_props, fix);
        for run in &mut para.runs {
            replace_run(&mut run.props, fix);
        }
    }
}

fn replace_list(l: &mut deckcraft_model::text::ListStyle, fix: &mut dyn FnMut(&mut Option<String>)) {
    for lvl in l.levels.iter_mut().flatten().chain(l.default.iter_mut()) {
        fix(&mut lvl.para.bullet_font);
        replace_run(&mut lvl.run, fix);
    }
}

fn replace_run(r: &mut deckcraft_model::text::RunProps, fix: &mut dyn FnMut(&mut Option<String>)) {
    for f in [&mut r.font, &mut r.font_ea, &mut r.font_cs, &mut r.font_sym] {
        fix(f);
    }
}

/// The languages whose font names the font list knows each family by, besides its English name:
/// Japanese, Simplified and Traditional Chinese and Korean (`name` table records in those
/// languages). A family with a name in one of them is an East Asian font.
pub const FONT_NAME_LANGUAGES: [&str; 4] = ["ja", "zh-Hans", "zh-Hant", "ko"];

/// One installed family in the font box and the Font dialog's lists.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontChoice {
    /// The name the list shows, and the one written when it is picked: the family's name in the
    /// interface language when the font has one (游ゴシック, ヒラギノ角ゴシック in a Japanese
    /// interface), else its English name. Japanese PowerPoint writes fonts by these names.
    pub name: String,
    /// Its English family name (the font's own name when it has no English one).
    pub english: String,
    /// Every name typing finds it by: the English name, then its Japanese, Chinese and Korean
    /// names, each once.
    pub names: Vec<String>,
    /// A Japanese, Chinese or Korean font, listed for Asian text: it has a name in one of those
    /// languages, or its name is a CJK font's ([`deckcraft_fonts::is_cjk_family`]).
    pub east_asian: bool,
}

/// The font list for an interface in `ui_lang` (`en`, `ja`, `zh-Hans`…): `families` (English
/// names, as [`FontDb::families`] lists them) with each one's names in other languages
/// (`localized`: per language, (English, localized) pairs as [`FontDb::localized_names`] gives
/// them). Hidden system faces (a name starting with `.`) are left out; each family is shown by its
/// name in `ui_lang` when it has one; the list is sorted by the shown name, ignoring case, and a
/// shown name appears once.
pub fn font_choices(families: &[String], localized: &[(String, Vec<(String, String)>)], ui_lang: &str) -> Vec<FontChoice> {
    use deckcraft_fonts::{fold_font_name, name_language_matches};
    use std::collections::{HashMap, HashSet};
    let mut names_of: HashMap<String, Vec<(&str, &str)>> = HashMap::new();
    for (lang, pairs) in localized {
        for (english, local) in pairs {
            let local = local.trim();
            if !local.is_empty() {
                names_of.entry(fold_font_name(english)).or_default().push((lang.as_str(), local));
            }
        }
    }
    let cjk_lang = |l: &str| FONT_NAME_LANGUAGES.iter().any(|c| name_language_matches(c, l));
    let mut out: Vec<FontChoice> = Vec::with_capacity(families.len());
    let mut seen = HashSet::new();
    for family in families {
        let english = family.trim();
        if english.is_empty() || english.starts_with('.') {
            continue;
        }
        let locals = names_of.get(&fold_font_name(english)).map(Vec::as_slice).unwrap_or(&[]);
        let shown = locals.iter().find(|(l, _)| name_language_matches(ui_lang, l)).map_or(english, |(_, n)| *n);
        if !seen.insert(fold_font_name(shown)) {
            continue;
        }
        let mut names = vec![english.to_string()];
        for (_, n) in locals {
            if !names.iter().any(|m| m == n) {
                names.push((*n).to_string());
            }
        }
        let east_asian = locals.iter().any(|(l, _)| cjk_lang(l)) || names.iter().any(|n| deckcraft_fonts::is_cjk_family(n));
        out.push(FontChoice { name: shown.to_string(), english: english.to_string(), names, east_asian });
    }
    out.sort_by_cached_key(|c| c.name.to_lowercase());
    out
}

/// [`font_choices`] for the fonts this machine has (bundled, installed and added), named for an
/// interface in `ui_lang`.
pub fn installed_font_choices(ui_lang: &str) -> Vec<FontChoice> {
    use deckcraft_fonts::name_language_matches;
    let db = FontDb::global();
    let mut localized: Vec<(String, Vec<(String, String)>)> = FONT_NAME_LANGUAGES.iter().map(|l| ((*l).to_string(), db.localized_names(l))).collect();
    let english_ui = ui_lang.trim().is_empty() || name_language_matches("en", ui_lang);
    if !english_ui && !FONT_NAME_LANGUAGES.iter().any(|l| name_language_matches(ui_lang, l)) {
        localized.push((ui_lang.to_string(), db.localized_names(ui_lang)));
    }
    font_choices(&db.families(), &localized, ui_lang)
}

/// A theme font in the font box's theme section: the theme's heading or body font for Latin
/// text, or for Asian text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFontChoice {
    /// The face the theme names.
    pub name: String,
    /// The theme reference written when it is picked (`+mj-lt`, `+mn-lt`, `+mj-ea`, `+mn-ea`), so
    /// the text follows a later change of theme fonts, as PowerPoint's theme fonts do.
    pub reference: String,
    /// `headings` or `body`.
    pub role: String,
    /// The Asian text font (`a:ea`).
    pub east_asian: bool,
}

/// The theme section of the font list: the heading and body fonts for Latin text and, with East
/// Asian editing (`east_asian`), those for Asian text: the theme's `a:ea` faces, or where it names
/// none, its font for that language's script (`Jpan` for Japanese), the one the text is drawn in.
pub fn theme_font_choices(fonts: &deckcraft_model::theme::FontScheme, east_asian: Option<deckcraft_fonts::CjkLocale>) -> Vec<ThemeFontChoice> {
    let mut out = Vec::new();
    let mut add = |name: &str, reference: &str, role: &str, east_asian: bool| {
        let name = name.trim();
        if !name.is_empty() {
            out.push(ThemeFontChoice { name: name.to_string(), reference: reference.to_string(), role: role.to_string(), east_asian });
        }
    };
    add(&fonts.major.latin, "+mj-lt", "headings", false);
    add(&fonts.minor.latin, "+mn-lt", "body", false);
    if let Some(cjk) = east_asian {
        let ea = |set: &deckcraft_model::theme::FontSet| -> String {
            if set.ea.trim().is_empty() { set.script(cjk.theme_script()).unwrap_or_default().to_string() } else { set.ea.clone() }
        };
        add(&ea(&fonts.major), "+mj-ea", "headings", true);
        add(&ea(&fonts.minor), "+mn-ea", "body", true);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Session;
    use deckcraft_model::ShapeId;
    use deckcraft_model::text::{RunProps, TextBody};
    use serde_json::json;
    use std::sync::Arc;

    /// A deck whose first slide holds one run of `text` set by `props` (its Latin font one this
    /// machine has unless `props` names another).
    fn deck(text: &str, props: RunProps) -> Presentation {
        let mut p = Presentation::default();
        let have = FontDb::global().face(deckcraft_fonts::FALLBACK_FAMILY, "Regular").family.clone();
        let slide = Arc::make_mut(&mut p.slides[0]);
        let mut body = TextBody::from_text(text);
        body.paragraphs[0].runs[0].props = RunProps { font: props.font.clone().or(Some(have)), ..props };
        slide.shapes[0].text = Some(body);
        p
    }

    #[test]
    fn missing_east_asian_fonts_are_listed_for_text_in_them() {
        let ea = RunProps { font_ea: Some("Qqq 存在しない明朝".into()), ..Default::default() };
        let p = deck("日本語のテキスト", ea.clone());
        assert_eq!(missing_fonts(&p), ["Qqq 存在しない明朝"]);
        assert!(used_fonts(&p).contains(&"Qqq 存在しない明朝".to_string()));
        // A run without East Asian text isn't drawn in its East Asian font.
        assert!(missing_fonts(&deck("Latin only", ea)).is_empty());
        // Complex-script text names its `a:cs` font.
        let cs = RunProps { font_cs: Some("Qqq Missing Naskh".into()), ..Default::default() };
        assert_eq!(missing_fonts(&deck("سلام", cs)), ["Qqq Missing Naskh"]);
    }

    /// Japanese text with no East Asian font of its own is drawn in the theme's Japanese font
    /// (`a:font script="Jpan"`) when its `lang` or `altLang` is ja-JP, and that font is listed.
    #[test]
    fn japanese_text_lists_the_themes_japanese_font() {
        let mut p = deck("日本語", RunProps { lang: Some("ja-JP".into()), ..Default::default() });
        let m = Arc::make_mut(&mut p.masters[0]);
        m.theme.fonts.minor.ea.clear();
        m.theme.fonts.minor.scripts.push(deckcraft_model::theme::ScriptFont { script: "Jpan".into(), typeface: "Qqq テーマ ゴシック".into() });
        let master = p.masters[0].clone();
        assert_eq!(missing_fonts(&p), ["Qqq テーマ ゴシック"]);
        let with = |props: RunProps| {
            let mut q = deck("日本語", props);
            q.masters[0] = master.clone();
            missing_fonts(&q)
        };
        assert_eq!(with(RunProps { lang: Some("en-US".into()), alt_lang: Some("ja-JP".into()), ..Default::default() }), ["Qqq テーマ ゴシック"]);
        assert!(with(RunProps { lang: Some("en-US".into()), ..Default::default() }).is_empty(), "not Japanese text");
        assert!(with(RunProps { lang: Some("zh-CN".into()), ..Default::default() }).is_empty());
    }

    /// An installed font named in Japanese, as Japanese PowerPoint writes it, isn't missing.
    /// Skipped where the font isn't installed.
    #[test]
    fn installed_fonts_named_in_japanese_are_not_missing() {
        let db = FontDb::global();
        let mut checked = 0;
        for (japanese, english) in
            [("ヒラギノ角ゴシック", "Hiragino Sans"), ("メイリオ", "Meiryo"), ("ＭＳ Ｐゴシック", "MS PGothic"), ("游ゴシック", "Yu Gothic")]
        {
            if !db.has_family(english) {
                continue;
            }
            checked += 1;
            let p = deck("日本語 text", RunProps { font: Some(japanese.into()), font_ea: Some(japanese.into()), ..Default::default() });
            assert!(missing_fonts(&p).is_empty(), "{japanese}");
        }
        if checked == 0 {
            eprintln!("skipped: no Japanese font installed");
        }
    }

    fn text_shape(s: &mut Session, text: &str) -> ShapeId {
        let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [40.0, 40.0, 400.0, 80.0], "text": text})).unwrap();
        ShapeId(r["id"].as_u64().unwrap() as u32)
    }

    fn used(s: &mut Session) -> Vec<String> {
        serde_json::from_value(s.execute("font.used", &json!({})).unwrap()).unwrap()
    }

    #[test]
    fn east_asian_fonts_are_listed_reported_missing_and_replaced() {
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語 and English");
        s.execute("format.font", &json!({"eastAsian": "テスト明朝"})).unwrap();
        let fonts = used(&mut s);
        assert!(fonts.iter().any(|f| f == "テスト明朝"), "{fonts:?}");
        assert!(fonts.iter().any(|f| f == "Inter"), "{fonts:?}");
        assert!(super::missing_fonts(&s.doc().unwrap().doc).iter().any(|f| f == "テスト明朝"), "a missing Japanese font is reported");

        let r = s.execute("font.replace", &json!({"from": "テスト明朝", "to": "BIZ UDPGothic"})).unwrap();
        assert!(r["replaced"].as_u64().is_some_and(|n| n >= 1), "{r}");
        let run = s.doc().unwrap().shape(id).unwrap().text.clone().unwrap().paragraphs[0].runs[0].props.clone();
        assert_eq!(run.font_ea.as_deref(), Some("BIZ UDPGothic"));
        assert!(!used(&mut s).iter().any(|f| f == "テスト明朝"));

        // A complex-script run lists its complex-script font.
        let other = text_shape(&mut s, "مرحبا");
        s.execute("edit.deselect", &json!({})).unwrap();
        s.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).unwrap();
            let sh = deckcraft_model::find_shape_mut(list, other).unwrap();
            sh.text.as_mut().unwrap().paragraphs[0].runs[0].props.font_cs = Some("Test Naskh".into());
            Ok(())
        })
        .unwrap();
        assert!(used(&mut s).iter().any(|f| f == "Test Naskh"));
    }

    #[test]
    fn a_japanese_decks_theme_font_is_listed_for_its_japanese_text() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        text_shape(&mut s, "日本語");
        assert!(used(&mut s).iter().any(|f| f == "BIZ UDPGothic"));
        // The deck's default style (a:defPPr) takes part in Replace Fonts.
        s.edit(|doc, _| {
            if let Some(d) = doc.default_text_style.default.as_mut() {
                d.run.font_ea = Some("Old EA".into());
            }
            Ok(())
        })
        .unwrap();
        s.execute("font.replace", &json!({"from": "Old EA", "to": "BIZ UDMincho"})).unwrap();
        assert_eq!(s.doc().unwrap().doc.default_text_style.default.clone().and_then(|d| d.run.font_ea).as_deref(), Some("BIZ UDMincho"));
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
    }

    /// Made-up families: a Japanese one with Japanese names (typographic and Windows family), a
    /// Korean one, a CJK font known by name only, Latin ones and a hidden system face.
    fn sample_choices(ui: &str) -> Vec<FontChoice> {
        let families = strings(&["Qqq Gothic", "Qqq Gothic W3", "Inter", ".Qqq Interface", "Qqq Hangul", "BIZ UDPGothic", "Lato", "  "]);
        let localized = vec![
            ("ja".to_string(), pairs(&[("Qqq Gothic", "キューゴシック"), ("Qqq Gothic W3", "キューゴシック W3"), ("qqq hangul", "キュー韓国")])),
            ("zh-Hans".to_string(), Vec::new()),
            ("zh-Hant".to_string(), Vec::new()),
            ("ko".to_string(), pairs(&[("Qqq Hangul", "큐 고딕")])),
        ];
        font_choices(&families, &localized, ui)
    }

    #[test]
    fn the_font_list_names_families_in_the_interface_language() {
        let ja = sample_choices("ja");
        let shown: Vec<&str> = ja.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            shown,
            ["BIZ UDPGothic", "Inter", "Lato", "キューゴシック", "キューゴシック W3", "キュー韓国"],
            "sorted by the shown name; the hidden face left out"
        );
        let gothic = ja.iter().find(|c| c.english == "Qqq Gothic").unwrap();
        assert_eq!(gothic.names, ["Qqq Gothic", "キューゴシック"]);
        assert!(gothic.east_asian);
        let hangul = ja.iter().find(|c| c.english == "Qqq Hangul").unwrap();
        assert_eq!(hangul.names, ["Qqq Hangul", "キュー韓国", "큐 고딕"], "every name it is found by");
        assert!(ja.iter().find(|c| c.english == "BIZ UDPGothic").unwrap().east_asian, "a CJK font by its name");
        assert!(!ja.iter().find(|c| c.english == "Inter").unwrap().east_asian);

        let en = sample_choices("en");
        let shown: Vec<&str> = en.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(shown, ["BIZ UDPGothic", "Inter", "Lato", "Qqq Gothic", "Qqq Gothic W3", "Qqq Hangul"]);
        assert!(en.iter().find(|c| c.name == "Qqq Gothic").unwrap().east_asian);
        let ko = sample_choices("ko-KR");
        assert!(ko.iter().any(|c| c.name == "큐 고딕" && c.english == "Qqq Hangul"));

        // Two families shown by the same name appear once.
        let localized = vec![("ja".to_string(), pairs(&[("Qqq A", "同じ名前"), ("Qqq B", "同じ名前")]))];
        let same = font_choices(&strings(&["Qqq A", "Qqq B"]), &localized, "ja");
        assert_eq!(same.len(), 1);
        assert!(font_choices(&[], &[], "ja").is_empty());
    }

    #[test]
    fn the_theme_section_lists_asian_text_fonts_with_east_asian_editing() {
        use deckcraft_fonts::CjkLocale;
        use deckcraft_model::theme::{FontScheme, FontSet, ScriptFont};
        let jpan = |face: &str| vec![ScriptFont { script: "Jpan".into(), typeface: face.into() }];
        let fonts = FontScheme {
            name: "Test".into(),
            major: FontSet { latin: "Lato".into(), scripts: jpan("Qqq 見出し"), ..Default::default() },
            minor: FontSet { latin: "Roboto".into(), ea: "Qqq 本文".into(), scripts: jpan("Qqq 本文 Jpan"), ..Default::default() },
        };
        let rows = |l: Option<CjkLocale>| -> Vec<(String, String, String, bool)> {
            theme_font_choices(&fonts, l).into_iter().map(|c| (c.name, c.reference, c.role, c.east_asian)).collect()
        };
        let row = |n: &str, r: &str, role: &str, ea: bool| (n.to_string(), r.to_string(), role.to_string(), ea);
        assert_eq!(rows(None), [row("Lato", "+mj-lt", "headings", false), row("Roboto", "+mn-lt", "body", false)]);
        assert_eq!(
            rows(Some(CjkLocale::Japanese)),
            [
                row("Lato", "+mj-lt", "headings", false),
                row("Roboto", "+mn-lt", "body", false),
                row("Qqq 見出し", "+mj-ea", "headings", true),
                row("Qqq 本文", "+mn-ea", "body", true),
            ]
        );
        // No Korean font in the theme: only the body's own a:ea.
        assert_eq!(rows(Some(CjkLocale::Korean)).len(), 3);
    }
}
