//! Home ▸ Font and Paragraph, Format menu: character, paragraph and text box formatting, and
//! the Format Painter.

use deckcraft_model::edit as ed;
use deckcraft_model::resolve;
use deckcraft_model::text::{Align, Anchor, AutoFit, Bullet, Caps, ParaProps, RunProps, Spacing, Strike, TabStop, TextDir};
use deckcraft_model::{ColorRef, ColorTransform, Effects, Fill, Line, SchemeSlot, Shape, ShapeKind};
use serde_json::{Value, json};
use std::sync::Arc;

use super::text::{format_body, format_para, format_run};
use super::{
    CommandSpec, always, bad, base64_decode, bool_or, bool_param, cmd, color_param, edit_shapes, f64_or, f64_param, has_doc, has_text,
    has_text_or_shapes, ids_param, ok, str_param,
};
use crate::{Result, Session};
use deckcraft_model::ShapeId;

mod font;
mod painter;
mod paragraph;
mod ruler;
mod text_effects;

pub use font::SIZES;
pub(crate) use font::east_asian_family;
use font::{
    bold, caps, clear, color, font, font_dialog, grow, highlight, italic, language, language_all, shrink, size, spacing, strike, subscript,
    superscript, text_transparency, underline,
};
pub use painter::Painted;
pub(crate) use painter::apply_painted;
pub use painter::selection_language;
use painter::{font_add, font_list, font_rescan, painter_apply, painter_pick, state};
pub use paragraph::{AUTONUM_SCHEMES, NUMBERING_GALLERY};
use paragraph::{
    align, align_to, anchor, autofit, bullets, columns, direction, level, line_spacing, margins, numbering, numbering_schemes, paragraph,
    paragraph_dialog, rtl, wrap,
};
use ruler::{ruler, tabs};
use text_effects::{case, text_outline, text_shadow, text_warp};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "font.add", "Add Font", [], None, "{data: base64 TTF/OTF/TTC} → {faces} added for this session", always, font_add),
        cmd!(noundo "font.rescan", "Rescan Installed Fonts", ["Home", "Font"], None, "{} → {faces, families} (fonts installed since start-up)", always, font_rescan),
        cmd!(
            query "font.list",
            "Font List",
            [],
            None,
            "{lang?: interface language (en|ja|zh-Hans|zh-Hant|ko…, default en), eastAsian?: bool (list Asian text theme fonts; default: with East Asian editing), fonts?: bool, theme?: bool} → {eastAsian, theme: [{name, reference (+mj-lt|+mn-lt|+mj-ea|+mn-ea), role (headings|body), eastAsian}], fonts: [{name (in lang when the font has one), english, names, eastAsian}]}",
            always,
            font_list
        ),
        cmd!("format.bold", "Bold", ["Home", "Font"], Some("Cmd+B"), "{on?: bool}", has_text_or_shapes, bold),
        cmd!("format.italic", "Italic", ["Home", "Font"], Some("Cmd+I"), "{on?: bool}", has_text_or_shapes, italic),
        cmd!(
            "format.underline",
            "Underline",
            ["Home", "Font"],
            Some("Cmd+U"),
            "{on?: bool, style?: sng|dbl|heavy|dotted|dash|wavy}",
            has_text_or_shapes,
            underline
        ),
        cmd!(
            "format.strikethrough",
            "Strikethrough",
            ["Home", "Font"],
            Some("Cmd+Shift+X"),
            "{on?: bool, double?: bool}",
            has_text_or_shapes,
            strike
        ),
        cmd!("format.superscript", "Superscript", ["Home", "Font"], Some("Cmd+Shift+="), "{on?: bool}", has_text_or_shapes, superscript),
        cmd!("format.subscript", "Subscript", ["Home", "Font"], Some("Cmd+="), "{on?: bool}", has_text_or_shapes, subscript),
        cmd!(
            "format.font",
            "Font",
            ["Home", "Font"],
            None,
            "{family?, eastAsian? (alias ea)} — family sets the Latin font, and an East Asian family (Japanese, Chinese or Korean: a CJK name, or an installed face with kana, ideographs or Hangul) the East Asian font too; eastAsian sets only the East Asian font (+mn-ea/+mj-ea or a family). At least one is required",
            has_text_or_shapes,
            font
        ),
        cmd!("format.size", "Font Size", ["Home", "Font"], None, "{size: pt}", has_text_or_shapes, size),
        cmd!("format.grow", "Increase Font Size", ["Home", "Font"], Some("Cmd+Shift+."), "{}", has_text_or_shapes, grow),
        cmd!("format.shrink", "Decrease Font Size", ["Home", "Font"], Some("Cmd+Shift+,"), "{}", has_text_or_shapes, shrink),
        cmd!("format.color", "Font Color", ["Home", "Font"], None, "{color: #RRGGBB|accent1..6|tx1…|{scheme, lumMod…}}", has_text_or_shapes, color),
        cmd!(
            "format.textTransparency",
            "Text Transparency",
            ["Shape Format", "WordArt Styles"],
            None,
            "{value: 0..1} — 0 opaque, 1 invisible; keeps the text's colour (inherited colours included)",
            has_text_or_shapes,
            text_transparency
        ),
        cmd!(
            "format.language",
            "Language",
            ["Review", "Language"],
            None,
            "{lang?, noProof?: bool} (proofing language of the selected text, and Do Not Check Spelling; with nothing selected, the language of new text: the deck's default and the text styles that repeat it, text that had it only from there keeping it) → {default?: true}",
            has_doc,
            language
        ),
        cmd!(
            "format.languageAll",
            "Language for All Text",
            ["Review", "Language"],
            None,
            "{lang?, noProof?: bool} (proofing language of every text in the presentation: slides, tables, notes, masters and layouts, and the default for new text; one undo step)",
            has_doc,
            language_all
        ),
        cmd!("format.highlight", "Text Highlight Color", ["Home", "Font"], None, "{color? (absent = none)}", has_text_or_shapes, highlight),
        cmd!(
            "format.case",
            "Change Case",
            ["Home", "Font"],
            Some("Shift+F3"),
            "{mode?: cycle|sentence|lower|upper|title|toggle|halfWidth|fullWidth} — cycle (Shift+F3, the default) goes lowercase → Capitalize Each Word → UPPERCASE; halfWidth/fullWidth (East Asian editing's 半角/全角) convert ASCII, spaces and katakana between half- and full-width forms (sound marks composed); a caret in a word changes that word; any other mode is an error",
            has_text_or_shapes,
            case
        ),
        cmd!("format.caps", "Caps", [], None, "{caps: none|small|all}", has_text_or_shapes, caps),
        cmd!(
            "format.spacing",
            "Character Spacing",
            ["Home", "Font"],
            None,
            "{pt} (veryTight -3, tight -1.5, normal 0, loose 3, veryLoose 6)",
            has_text_or_shapes,
            spacing
        ),
        cmd!("format.clear", "Clear All Formatting", ["Home", "Font"], Some("Cmd+Space"), "{}", has_text_or_shapes, clear),
        cmd!(
            "format.fontDialog",
            "Font…",
            ["Home", "Font"],
            Some("Cmd+T"),
            "{family?, eastAsian?, size?: pt, bold?: bool, italic?: bool, underline?: none|sng|dbl|heavy|dotted|dash|wavy, color?, strike?: none|sng|dbl, baseline?: none|super|sub|<fraction>, caps?: none|small|all, spacing?: pt} — family is the Latin text font only, eastAsian the Asian text font only (the dialog's two boxes); with none of these, opens the Font dialog (params: the current values, family = Latin font, eastAsian = Asian font or \"\")",
            has_text_or_shapes,
            font_dialog
        ),
        cmd!("format.textShadow", "Text Shadow", ["Home", "Font"], None, "{on?: bool}", has_text_or_shapes, text_shadow),
        cmd!(
            "format.textOutline",
            "Text Outline",
            ["Shape Format", "WordArt Styles"],
            None,
            "{color?, width?: pt} (no color = none)",
            has_text_or_shapes,
            text_outline
        ),
        cmd!(
            "format.textWarp",
            "Text Transform",
            ["Shape Format", "WordArt Styles"],
            None,
            "{preset: a prstTxWarp name or none, adj?: [finite raw guide values, at most 8]} — none clears the transform; omitted adj uses the preset's defaults",
            has_text_or_shapes,
            text_warp
        ),
        cmd!(
            "format.align",
            "Align",
            ["Home", "Paragraph"],
            None,
            "{align: left|center|right|justify|justifyLow|distributed}",
            has_text_or_shapes,
            align
        ),
        cmd!(
            "format.rtl",
            "Right-to-Left Text Direction",
            ["Home", "Paragraph"],
            None,
            "{on?: bool} (off = left-to-right; switching mirrors left/right alignment)",
            has_text_or_shapes,
            rtl
        ),
        cmd!("format.alignLeft", "Align Left", ["Home", "Paragraph"], Some("Cmd+L"), "{}", has_text_or_shapes, |s, _| align_to(s, Align::Left)),
        cmd!("format.alignCenter", "Center", ["Home", "Paragraph"], Some("Cmd+E"), "{}", has_text_or_shapes, |s, _| align_to(s, Align::Center)),
        cmd!("format.alignRight", "Align Right", ["Home", "Paragraph"], Some("Cmd+R"), "{}", has_text_or_shapes, |s, _| align_to(s, Align::Right)),
        cmd!("format.justify", "Justify", ["Home", "Paragraph"], Some("Cmd+J"), "{}", has_text_or_shapes, |s, _| align_to(s, Align::Justify)),
        cmd!(
            "format.bullets",
            "Bullets",
            ["Home", "Paragraph"],
            None,
            "{on?: bool, char?: •|○|■|□|◆|➢|✓|–, color?, size?: % of text}",
            has_text_or_shapes,
            bullets
        ),
        cmd!(
            "format.numbering",
            "Numbering",
            ["Home", "Paragraph"],
            None,
            "{on?: bool, scheme?: an ST_TextAutonumberScheme name (arabicPeriod, the default; arabicParenR, romanUcPeriod, alphaLcParenR, circleNumDbPlain, arabicDbPeriod, ea1JpnChsDbPeriod… — see format.numberingSchemes), start?: n, color?, size?: % of text} — an unknown scheme is an error",
            has_text_or_shapes,
            numbering
        ),
        cmd!(
            query "format.numberingSchemes",
            "Numbering Gallery",
            [],
            None,
            "{} → {schemes: [{scheme, sample, eastAsian}]}: the Numbering gallery in order (eastAsian: shown with East Asian editing), each with its first three labels",
            always,
            numbering_schemes
        ),
        cmd!("format.indent", "Increase List Level", ["Home", "Paragraph"], Some("Tab"), "{}", has_text_or_shapes, |s, _| level(s, 1)),
        cmd!("format.outdent", "Decrease List Level", ["Home", "Paragraph"], Some("Shift+Tab"), "{}", has_text_or_shapes, |s, _| level(s, -1)),
        cmd!(
            "format.lineSpacing",
            "Line Spacing",
            ["Home", "Paragraph"],
            None,
            "{lines?: 1.0|1.5|2.0…, pt?: exactly}",
            has_text_or_shapes,
            line_spacing
        ),
        cmd!(
            "format.paragraph",
            "Paragraph…",
            ["Format"],
            None,
            "{align?, indentLeft?: pt, indentFirst?: pt (negative = hanging), spaceBefore?: pt, spaceAfter?: pt, lineSpacing?: lines, eaLnBrk?: bool, latinLnBrk?: bool, hangingPunct?: bool, fontAlgn?: auto|t|ctr|base|b} — the last four are Asian Typography: East Asian line breaking (kinsoku), Latin words may break anywhere, hanging punctuation, and text alignment of mixed sizes",
            has_text_or_shapes,
            paragraph
        ),
        cmd!(query "format.ruler", "Ruler", [], None, "{} → {x, y, width, rotated, marginLeft, indent, tabs: [{pos, align}], defaultTab} for the paragraph with the caret (x, y: the text area's top-left in slide pt)", has_text, ruler),
        cmd!(
            "format.paragraphDialog",
            "Paragraph…",
            ["Home", "Paragraph"],
            None,
            "{align?: left|center|right|justify|justLow|distributed, indentLeft?: pt, indentRight?: pt, special?: none|first|hanging, specialBy?: pt, indentFirst?: pt, spaceBefore?: pt, spaceAfter?: pt, spaceBeforeKind?: pts|pct, spaceAfterKind?: pts|pct, lineSpacing?: lines, lineSpacingPt?: pt, eaLnBrk?: bool, latinLnBrk?: bool, hangingPunct?: bool, fontAlgn?: auto|t|ctr|base|b} — with none of these, opens the Paragraph dialog (its params include the four Asian Typography values)",
            has_text_or_shapes,
            paragraph_dialog
        ),
        cmd!(
            "format.tabs",
            "Tabs…",
            ["Home", "Paragraph"],
            None,
            "{set?: [{pos, align?}] (replaces all), add?: {pos, align?: l|ctr|r|dec}, move?: {from, to}, remove?: pos, clear?: bool, defaultTab?: pt} on the selected paragraphs; with none of them, opens the Tabs dialog (pos in pt from the text area's left)",
            has_text_or_shapes,
            tabs
        ),
        cmd!("format.anchor", "Align Text", ["Home", "Paragraph"], None, "{anchor: top|middle|bottom}", has_text_or_shapes, anchor),
        cmd!(
            "format.direction",
            "Text Direction",
            ["Home", "Paragraph"],
            None,
            "{dir: horizontal (horz) | vertical (eaVert: East Asian vertical, characters upright, columns right to left) | rotate90 (vert) | rotate270 (vert270) | stacked (wordArtVert) | stackedRtl (wordArtVertRtl: every character upright, columns right to left) | mongolianVert} — any other value is an error",
            has_text_or_shapes,
            direction
        ),
        cmd!(
            "format.columns",
            "Columns",
            ["Home", "Paragraph"],
            None,
            "{count?: 1-16, spacing?: pt between columns}; a key left out keeps the shape's value, and with neither the Columns dialog opens",
            has_text_or_shapes,
            columns
        ),
        cmd!("format.autofit", "Autofit", ["Format Shape", "Text Box"], None, "{mode: none|shrink|resize}", has_text_or_shapes, autofit),
        cmd!("format.wrap", "Wrap Text in Shape", ["Format Shape", "Text Box"], None, "{on: bool}", has_text_or_shapes, wrap),
        cmd!(
            "format.margins",
            "Text Box Margins",
            ["Format Shape", "Text Box"],
            None,
            "{left?: pt, top?: pt, right?: pt, bottom?: pt}",
            has_text_or_shapes,
            margins
        ),
        cmd!(noundo "format.painter", "Format Painter", ["Home", "Clipboard"], Some("Cmd+Shift+C"), "{sticky?: bool}", has_text_or_shapes, painter_pick),
        cmd!("format.painterApply", "Paste Formatting", [], Some("Cmd+Shift+V"), "{ids?}", has_text_or_shapes, painter_apply),
        cmd!(query "format.state", "Formatting State", [], None, "{} → the effective formatting at the selection (font, size, bold, …); font is the East Asian font when the character described (the first selected, else the one before the caret) is East Asian, fontLatin and fontEa are both fonts (fontEa null when none applies), fontTheme headings|body when the font shown is the theme's (else null), dir the text direction, textLang the language of the text (inherited and the deck default included; altLang on East Asian text when only it is East Asian) and noProof", has_doc, state),
    ]
}

fn on_param(p: &Value, current: Option<bool>) -> bool {
    bool_param(p, "on").unwrap_or(!current.unwrap_or(false))
}

/// The effective run props at the selection (first selected run).
fn current_run(s: &Session) -> RunProps {
    let Ok(st) = s.doc() else { return RunProps::default() };
    let ctx = super::text::ctx_for(&st.doc, &st.selection);
    let (shape, para, run) = match &st.selection.text {
        Some(t) => {
            let Some(body) = super::text::body_of(st, t) else { return RunProps::default() };
            let a = t.ordered().0;
            let rp = if t.is_range() { ed::props_at(body, (a.0, a.1 + 1)) } else { ed::props_at(body, a) };
            let para = body.paragraphs.get(a.0).cloned().unwrap_or_default();
            (st.shape(t.shape).cloned().unwrap_or_default(), para, rp)
        }
        None => {
            let Some(sh) = st.selected_shapes().first().map(|x| (*x).clone()) else { return RunProps::default() };
            let para = sh.text.as_ref().and_then(|t| t.paragraphs.first().cloned()).unwrap_or_default();
            let rp = para.runs.first().map(|r| r.props.clone()).unwrap_or_default();
            (sh, para, rp)
        }
    };
    match ctx {
        Some(c) => resolve::run(&c, &shape, &para, &run),
        None => run,
    }
}

fn current_para(s: &Session) -> ParaProps {
    let Ok(st) = s.doc() else { return ParaProps::default() };
    let ctx = super::text::ctx_for(&st.doc, &st.selection);
    let (shape, para) = match &st.selection.text {
        Some(t) => {
            let para = super::text::body_of(st, t).and_then(|b| b.paragraphs.get(t.caret.0).cloned()).unwrap_or_default();
            (st.shape(t.shape).cloned().unwrap_or_default(), para)
        }
        None => {
            let Some(sh) = st.selected_shapes().first().map(|x| (*x).clone()) else { return ParaProps::default() };
            let para = sh.text.as_ref().and_then(|t| t.paragraphs.first().cloned()).unwrap_or_default();
            (sh, para)
        }
    };
    match ctx {
        Some(c) => resolve::para(&c, &shape, &para),
        None => para.props,
    }
}

#[cfg(test)]
#[path = "ruler_tests.rs"]
mod ruler_tests;

#[cfg(test)]
mod tests {
    use deckcraft_model::text::{Align, Caps, ParaProps, Spacing, Strike};
    use deckcraft_model::{ColorBase, Fill, SchemeSlot};
    use serde_json::json;

    use super::font::is_east_asian_family;
    use super::*;
    use crate::{Session, UiRequest};

    fn shape() -> Session {
        let mut s = Session::with_new();
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [40.0, 40.0, 240.0, 80.0], "text": "Hello"})).unwrap();
        s
    }

    fn props(s: &Session) -> RunProps {
        s.doc()
            .unwrap()
            .selected_shapes()
            .first()
            .and_then(|sh| sh.text.as_ref())
            .and_then(|t| t.paragraphs.first())
            .and_then(|p| p.runs.first())
            .unwrap()
            .props
            .clone()
    }

    #[test]
    fn font_dialog_applies_character_formatting_in_one_undo() {
        let mut s = shape();
        let before = s.doc().unwrap().history.undo.len();
        s.execute(
            "format.fontDialog",
            &json!({
                "family": "Liberation Serif",
                "size": 28,
                "bold": true,
                "italic": false,
                "underline": "wavy",
                "color": "#112233",
                "strike": "dbl",
                "baseline": "super",
                "caps": "small",
                "spacing": 3
            }),
        )
        .unwrap();
        assert_eq!(s.doc().unwrap().history.undo.len(), before + 1);
        assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Font…");
        let r = props(&s);
        assert_eq!(r.font.as_deref(), Some("Liberation Serif"));
        assert_eq!(r.size, Some(28.0));
        assert_eq!(r.bold, Some(true));
        assert_eq!(r.italic, Some(false));
        assert_eq!(r.underline.as_deref(), Some("wavy"));
        assert!(matches!(r.fill, Some(Fill::Solid { color }) if matches!(color.base, ColorBase::Rgb { .. })));
        assert_eq!(r.strike, Some(Strike::Double));
        assert_eq!(r.baseline, Some(0.3));
        assert_eq!(r.caps, Some(Caps::Small));
        assert_eq!(r.spacing, Some(3.0));
        s.execute("edit.undo", &json!({})).unwrap();
        let r = props(&s);
        assert!(r.font.is_none() && r.size.is_none() && r.bold.is_none() && r.underline.is_none() && r.fill.is_none());
        assert_eq!(r.strike, None);
        assert_eq!(r.caps, None);
    }

    #[test]
    fn font_dialog_changes_only_the_fields_it_is_given() {
        let mut s = shape();
        s.execute("format.bold", &json!({"on": true})).unwrap();
        s.execute("format.fontDialog", &json!({"size": 22, "color": {"scheme": "accent2", "lumMod": 50000}})).unwrap();
        let r = props(&s);
        assert_eq!(r.bold, Some(true));
        assert_eq!(r.size, Some(22.0));
        assert!(r.font.is_none() && r.italic.is_none());
        let Some(Fill::Solid { color }) = r.fill else { panic!("solid fill") };
        assert!(matches!(color.base, ColorBase::Scheme { slot: SchemeSlot::Accent2 }));
        assert!(color.mods.iter().any(|m| m.xml_name() == "lumMod"));
        s.execute("edit.undo", &json!({})).unwrap();
        let r = props(&s);
        assert_eq!(r.bold, Some(true));
        assert!(r.size.is_none() && r.fill.is_none());
    }

    #[test]
    fn font_dialog_with_no_fields_asks_the_ui_to_open() {
        let mut s = shape();
        s.execute("format.italic", &json!({"on": true})).unwrap();
        let rev = s.doc().unwrap().revision;
        let n = s.doc().unwrap().history.undo.len();
        s.ui_requests.clear();
        s.execute("format.fontDialog", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().revision, rev);
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        let Some(UiRequest::Dialog { id, params }) = s.ui_requests.last() else { panic!("dialog request") };
        assert_eq!(id, "font");
        assert_eq!(params["italic"], true);
        assert_eq!(params["bold"], false);
        assert!(params["family"].as_str().is_some_and(|f| !f.is_empty()));
        assert!(params["size"].as_f64().is_some_and(|n| n > 0.0));
    }

    #[test]
    fn font_dialog_rejects_bad_fields_without_editing() {
        let mut s = shape();
        let n = s.doc().unwrap().history.undo.len();
        let err = s.execute("format.fontDialog", &json!({"underline": "sparkle"})).unwrap_err();
        assert!(err.to_string().contains("underline"), "{err}");
        assert!(s.execute("format.fontDialog", &json!({"size": "large"})).is_err());
        assert!(s.execute("format.fontDialog", &json!({"bold": 1})).is_err());
        assert!(s.execute("format.fontDialog", &json!({"family": "  "})).is_err());
        assert!(s.execute("format.fontDialog", &json!({"color": "not-a-color"})).is_err());
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        assert!(props(&s).underline.is_none());
        s.execute("edit.deselect", &json!({})).unwrap();
        assert!(s.execute("format.fontDialog", &json!({"bold": true})).is_err());
    }

    fn para(s: &Session) -> ParaProps {
        s.doc().unwrap().selected_shapes().first().and_then(|sh| sh.text.as_ref()).and_then(|t| t.paragraphs.first()).unwrap().props.clone()
    }

    #[test]
    fn paragraph_dialog_applies_alignment_indent_and_spacing_in_one_undo() {
        let mut s = shape();
        let before = s.doc().unwrap().history.undo.len();
        s.execute(
            "format.paragraphDialog",
            &json!({
                "align": "justify",
                "indentLeft": 36.0,
                "indentRight": 18.0,
                "special": "hanging",
                "specialBy": 18.0,
                "spaceBefore": 6.0,
                "spaceAfter": 8.0,
                "lineSpacing": 1.5
            }),
        )
        .unwrap();
        assert_eq!(s.doc().unwrap().history.undo.len(), before + 1);
        assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Paragraph…");
        let p = para(&s);
        assert_eq!(p.align, Some(Align::Justify));
        assert_eq!(p.margin_left, Some(36.0));
        assert_eq!(p.margin_right, Some(18.0));
        assert_eq!(p.indent, Some(-18.0));
        assert_eq!(p.space_before, Some(Spacing::Pts(6.0)));
        assert_eq!(p.space_after, Some(Spacing::Pts(8.0)));
        assert_eq!(p.line_spacing, Some(Spacing::Pct(1.5)));
        s.execute("edit.undo", &json!({})).unwrap();
        let p = para(&s);
        assert_eq!(p.align, Some(Align::Center), "undo restores the alignment shape.insert wrote");
        assert!(p.margin_left.is_none() && p.indent.is_none() && p.space_before.is_none() && p.line_spacing.is_none());
    }

    #[test]
    fn paragraph_dialog_first_line_and_exact_spacing_replace_only_those_fields() {
        let mut s = shape();
        s.execute("format.paragraphDialog", &json!({"align": "right"})).unwrap();
        s.execute("format.paragraphDialog", &json!({"special": "first", "specialBy": 27.0, "lineSpacingPt": 20.0})).unwrap();
        let p = para(&s);
        assert_eq!(p.align, Some(Align::Right));
        assert_eq!(p.indent, Some(27.0));
        assert_eq!(p.line_spacing, Some(Spacing::Pts(20.0)));
        assert!(p.margin_left.is_none());
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(para(&s).align, Some(Align::Right));
        assert!(para(&s).indent.is_none());
    }

    #[test]
    fn paragraph_dialog_with_no_fields_asks_the_ui_to_open() {
        let mut s = shape();
        let rev = s.doc().unwrap().revision;
        let n = s.doc().unwrap().history.undo.len();
        s.ui_requests.clear();
        s.execute("format.paragraphDialog", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().revision, rev);
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        let Some(UiRequest::Dialog { id, params }) = s.ui_requests.last() else { panic!("dialog request") };
        assert_eq!(id, "paragraph");
        assert_eq!(params["align"], "center", "a new shape's text is centred");
        assert_eq!(params["special"], "none");
        assert_eq!(params["lineKind"], "multiple");
        assert!(params["lineValue"].as_f64().is_some_and(|v| v > 0.0));
    }

    #[test]
    fn paragraph_dialog_rejects_bad_fields_without_editing() {
        let mut s = shape();
        let n = s.doc().unwrap().history.undo.len();
        assert!(s.execute("format.paragraphDialog", &json!({"align": "sparkle"})).unwrap_err().to_string().contains("align"));
        assert!(s.execute("format.paragraphDialog", &json!({"special": "middle"})).is_err());
        assert!(s.execute("format.paragraphDialog", &json!({"indentLeft": "wide"})).is_err());
        assert!(s.execute("format.paragraphDialog", &json!({"lineSpacing": "double"})).is_err());
        assert!(s.execute("format.paragraphDialog", &json!({"spaceBeforeKind": "lines", "spaceBefore": 4.0})).is_err());
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        assert_eq!(para(&s).align, Some(Align::Center));
        assert!(para(&s).indent.is_none());
    }

    fn body(s: &Session) -> deckcraft_model::text::BodyProps {
        s.doc().unwrap().selected_shapes().first().and_then(|sh| sh.text.as_ref()).map(|t| t.body.clone()).unwrap_or_default()
    }

    #[test]
    fn columns_set_the_number_and_spacing_and_a_left_out_key_keeps_its_value() {
        let mut s = shape();
        let n = s.doc().unwrap().history.undo.len();
        s.execute("format.columns", &json!({"count": 4, "spacing": 36.0})).unwrap();
        assert_eq!((body(&s).columns, body(&s).col_spacing), (Some(4), Some(36.0)));
        assert_eq!(s.doc().unwrap().history.undo.len(), n + 1, "one undo step");
        s.execute("format.columns", &json!({"count": 2})).unwrap();
        assert_eq!((body(&s).columns, body(&s).col_spacing), (Some(2), Some(36.0)), "the spacing stays");
        s.execute("format.columns", &json!({"spacing": 9.0})).unwrap();
        assert_eq!((body(&s).columns, body(&s).col_spacing), (Some(2), Some(9.0)), "the number stays");
    }

    #[test]
    fn columns_with_no_fields_open_the_dialog_with_the_shapes_values() {
        let mut s = shape();
        s.execute("format.columns", &json!({"count": 3, "spacing": 27.0})).unwrap();
        let rev = s.doc().unwrap().revision;
        let n = s.doc().unwrap().history.undo.len();
        s.ui_requests.clear();
        s.execute("format.columns", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().revision, rev);
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        let Some(UiRequest::Dialog { id, params }) = s.ui_requests.last() else { panic!("dialog request") };
        assert_eq!(id, "columns");
        assert_eq!(params["count"], 3);
        assert_eq!(params["spacing"], 27.0);
    }

    #[test]
    fn columns_reject_a_bad_number_or_spacing_without_editing() {
        let mut s = shape();
        let n = s.doc().unwrap().history.undo.len();
        for p in [json!({"count": 0}), json!({"count": 17}), json!({"count": 2.5}), json!({"count": "two"}), json!({"spacing": "wide"})] {
            let err = s.execute("format.columns", &p).unwrap_err().to_string();
            assert!(err.contains("format.columns"), "{p}: {err}");
        }
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        assert_eq!(body(&s).columns, None);
    }

    #[test]
    fn tabs_dialog_sets_stops_in_one_undo_and_opens_with_no_fields() {
        let mut s = shape();
        let before = s.doc().unwrap().history.undo.len();
        s.execute(
            "format.tabs",
            &json!({"set": [{"pos": 144.0, "align": "center"}, {"pos": 72.0, "align": "dec"}, {"pos": 72.02, "align": "right"}], "defaultTab": 36.0}),
        )
        .unwrap();
        assert_eq!(s.doc().unwrap().history.undo.len(), before + 1);
        assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Tabs…");
        let p = para(&s);
        let tabs = p.tabs.unwrap();
        assert_eq!(tabs.len(), 2, "stops within 0.05 pt collapse");
        assert_eq!(tabs[0].pos, 72.02);
        assert_eq!(tabs[0].align, "r");
        assert_eq!(tabs[1].align, "ctr");
        assert_eq!(p.default_tab, Some(36.0));
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(para(&s).tabs.is_none() && para(&s).default_tab.is_none());

        let n = s.doc().unwrap().history.undo.len();
        s.ui_requests.clear();
        s.execute("format.tabs", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
        let Some(UiRequest::Dialog { id, params }) = s.ui_requests.last() else { panic!("dialog request") };
        assert_eq!(id, "tabs");
        assert!(params["tabs"].as_array().is_some_and(|a| a.is_empty()));
        assert!(s.execute("format.tabs", &json!({"set": [{"pos": 10.0, "align": "sparkle"}]})).is_err());
        assert!(s.execute("format.tabs", &json!({"set": "nope"})).is_err());
        assert_eq!(s.doc().unwrap().history.undo.len(), n);
    }

    fn text_shape(s: &mut Session, text: &str) -> deckcraft_model::ShapeId {
        let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [40.0, 40.0, 400.0, 80.0], "text": text})).unwrap();
        deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32)
    }

    fn runs_of(s: &Session, id: deckcraft_model::ShapeId) -> Vec<deckcraft_model::text::Run> {
        s.doc().unwrap().shape(id).unwrap().text.clone().unwrap().paragraphs[0].runs.clone()
    }

    fn undo_len(s: &Session) -> usize {
        s.doc().unwrap().history.undo.len()
    }

    #[test]
    fn an_east_asian_font_sets_both_fonts_and_a_latin_font_only_the_latin_one() {
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語abc");
        s.execute("format.font", &json!({"family": "テスト明朝"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!(r.font.as_deref(), Some("テスト明朝"));
        assert_eq!(r.font_ea.as_deref(), Some("テスト明朝"), "a Japanese font is the Japanese text's font too");
        s.execute("format.font", &json!({"family": "Liberation Serif"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!(r.font.as_deref(), Some("Liberation Serif"));
        assert_eq!(r.font_ea.as_deref(), Some("テスト明朝"), "a Latin font leaves the Japanese text's font");
        s.execute("format.font", &json!({"eastAsian": "BIZ UDPGothic"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("Liberation Serif"), Some("BIZ UDPGothic")));
        s.execute("format.font", &json!({"ea": "+mj-ea"})).unwrap();
        assert_eq!(runs_of(&s, id)[0].props.font_ea.as_deref(), Some("+mj-ea"));
        s.execute("format.font", &json!({"family": "テスト明朝", "eastAsian": "BIZ UDMincho"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("テスト明朝"), Some("BIZ UDMincho")), "an explicit Asian font wins");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(runs_of(&s, id)[0].props.font_ea.as_deref(), Some("+mj-ea"), "one undo step");

        let n = undo_len(&s);
        for bad in [json!({}), json!({"family": 3}), json!({"family": "  "}), json!({"eastAsian": ""}), json!({"family": "Inter", "ea": false})] {
            assert!(s.execute("format.font", &bad).is_err(), "{bad}");
        }
        assert_eq!(undo_len(&s), n);
    }

    #[test]
    fn east_asian_families_are_told_by_name_or_by_their_faces() {
        assert!(is_east_asian_family("テスト明朝"));
        assert!(is_east_asian_family("맑은 테스트"));
        assert!(!is_east_asian_family("Inter"));
        assert!(!is_east_asian_family("Liberation Serif"));
        assert!(!is_east_asian_family("+mn-lt"));
        assert!(!is_east_asian_family("  "));
        assert!(!is_east_asian_family("Qqq Not Installed Sans"));
        // An East Asian face installed under a Latin name (craft-fonts' BIZ UDPGothic, a Mac's
        // Hiragino or Apple SD Gothic Neo) is East Asian by what it maps.
        let db = deckcraft_fonts::FontDb::global();
        for fam in ["BIZ UDPGothic", "Hiragino Sans", "Hiragino Sans GB", "Apple SD Gothic Neo"] {
            if db.has_family(fam) {
                assert!(is_east_asian_family(fam), "{fam}");
                let mut s = Session::with_new();
                let id = text_shape(&mut s, "日本語");
                s.execute("format.font", &json!({"family": fam})).unwrap();
                assert_eq!(runs_of(&s, id)[0].props.font_ea.as_deref(), Some(fam));
            }
        }
    }

    #[test]
    fn the_font_box_names_the_east_asian_font_on_east_asian_text() {
        let mut s = Session::with_new();
        s.edit(|doc, _| {
            std::sync::Arc::make_mut(&mut doc.masters[0]).theme.fonts.minor.ea = "Theme EA".into();
            Ok(())
        })
        .unwrap();
        let id = text_shape(&mut s, "日本語abc");
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!(st["font"], "Theme EA", "a selected shape: its first character, 日");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 3]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!(st["font"], "Theme EA", "after 語");
        assert_eq!(st["fontLatin"], "Inter");
        assert_eq!(st["fontEa"], "Theme EA");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 6]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!(st["font"], "Inter", "after c");
        assert_eq!(st["fontEa"], "Theme EA");
        s.execute("text.select", &json!({"anchor": [0, 3], "caret": [0, 1]})).unwrap();
        assert_eq!(s.execute("format.state", &json!({})).unwrap()["font"], "Theme EA", "the first selected character, 本");

        // The Font dialog shows both.
        s.ui_requests.clear();
        s.execute("format.fontDialog", &json!({})).unwrap();
        let Some(UiRequest::Dialog { params, .. }) = s.ui_requests.last() else { panic!("dialog request") };
        assert_eq!(params["family"], "Inter");
        assert_eq!(params["eastAsian"], "Theme EA");

        // Without any East Asian font the box keeps the Latin font.
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!(st["font"], "Inter");
        assert!(st["fontEa"].is_null());
    }

    #[test]
    fn japanese_text_in_a_japanese_deck_shows_the_themes_jpan_font() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        let id = text_shape(&mut s, "日本語abc");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 1]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!(st["lang"], "ja-JP", "the deck's default language");
        assert_eq!(st["font"], "BIZ UDPGothic");
        assert_eq!(st["fontLatin"], "Inter");
    }

    /// The font box says when the font shown is the theme's heading or body font, for the Latin
    /// and for the East Asian font, as PowerPoint's box does.
    #[test]
    fn the_font_box_says_when_the_font_is_the_themes() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        let id = text_shape(&mut s, "日本語abc");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 6]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!((st["font"].as_str(), st["fontTheme"].as_str()), (Some("Inter"), Some("body")));
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!((st["font"].as_str(), st["fontTheme"].as_str()), (Some("BIZ UDPGothic"), Some("body")), "the theme's Jpan font");

        // Picking the theme's Asian heading font writes theme references.
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 6]})).unwrap();
        s.execute("format.font", &json!({"family": "+mj-lt", "eastAsian": "+mj-ea"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("+mj-lt"), Some("+mj-ea")));
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!((st["font"].as_str(), st["fontTheme"].as_str()), (Some("BIZ UDPGothic"), Some("headings")));
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 6]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!((st["font"].as_str(), st["fontTheme"].as_str()), (Some("Inter"), Some("headings")));

        // A font picked by name is not the theme's.
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 6]})).unwrap();
        s.execute("format.font", &json!({"family": "Liberation Serif", "eastAsian": "テスト明朝"})).unwrap();
        for at in [2, 6] {
            s.execute("text.edit", &json!({"id": id.0, "at": [0, at]})).unwrap();
            assert!(s.execute("format.state", &json!({})).unwrap()["fontTheme"].is_null(), "at {at}");
        }
    }

    /// The status bar names the language of the text at the caret: inherited from the deck's
    /// default (`a:defPPr`) when the run has none, the `altLang` on East Asian text when only it is
    /// East Asian, and the deck's default with nothing selected.
    #[test]
    fn the_text_language_includes_the_decks_default_and_alt_lang() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        assert_eq!(selection_language(&s), (Some("ja-JP".to_string()), false), "nothing selected: the deck's default");
        let id = text_shape(&mut s, "日本語abc");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 1]})).unwrap();
        let st = s.execute("format.state", &json!({})).unwrap();
        assert_eq!((st["textLang"].as_str(), st["noProof"].as_bool()), (Some("ja-JP"), Some(false)));

        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語abc");
        assert_eq!(selection_language(&s).0, None, "no language anywhere");
        s.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).unwrap();
            let sh = deckcraft_model::find_shape_mut(list, id).unwrap();
            let r = &mut sh.text.as_mut().unwrap().paragraphs[0].runs[0].props;
            r.lang = Some("en-US".into());
            r.alt_lang = Some("ja-JP".into());
            Ok(())
        })
        .unwrap();
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 2]})).unwrap();
        assert_eq!(s.execute("format.state", &json!({})).unwrap()["textLang"], "ja-JP", "after 本: East Asian text");
        s.execute("text.edit", &json!({"id": id.0, "at": [0, 5]})).unwrap();
        assert_eq!(s.execute("format.state", &json!({})).unwrap()["textLang"], "en-US", "after b");
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 6]})).unwrap();
        s.execute("format.language", &json!({"noProof": true})).unwrap();
        assert_eq!(selection_language(&s), (Some("ja-JP".to_string()), true), "the first selected character, 日");
    }

    /// Review ▸ Language ▸ Whole presentation also sets the deck's `a:defPPr`, the language of new
    /// text that format.language sets with nothing selected, so the status bar names the new
    /// language then: on a Japanese deck (which has one) and on a deck that had none.
    #[test]
    fn language_all_sets_the_decks_default_language_too() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        s.execute("edit.select", &json!({"ids": []})).unwrap();
        s.execute("format.languageAll", &json!({"lang": "en-US"})).unwrap();
        assert_eq!(selection_language(&s), (Some("en-US".to_string()), false), "a Japanese deck's default");

        let mut s = Session::with_new();
        s.execute("edit.select", &json!({"ids": []})).unwrap();
        assert_eq!(selection_language(&s).0, None);
        s.execute("format.languageAll", &json!({"lang": "es-ES", "noProof": true})).unwrap();
        let d = s.doc().unwrap().doc.default_text_style.default.clone().unwrap_or_default();
        assert_eq!((d.run.lang.as_deref(), d.run.no_proof), (Some("es-ES"), Some(true)), "made when missing");
        assert_eq!(selection_language(&s).0.as_deref(), Some("es-ES"));
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(selection_language(&s).0, None, "one undo step");
    }

    /// After Whole presentation every text style names its language. With nothing selected,
    /// format.language still sets the language of new text: the styles that repeat the old
    /// default take the new one, a box's style in another language stays, and existing text
    /// keeps its own.
    #[test]
    fn language_with_nothing_selected_after_the_whole_presentation() {
        let mut s = Session::with_new();
        let old = text_shape(&mut s, "Hallo");
        let german = text_shape(&mut s, "Tschüss");
        s.execute("edit.select", &json!({"ids": []})).unwrap();
        s.execute("format.languageAll", &json!({"lang": "de-DE"})).unwrap();
        s.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).unwrap();
            // A run without a language of its own, as typed text has.
            deckcraft_model::find_shape_mut(list, old).unwrap().text.as_mut().unwrap().paragraphs[0].runs[0].props.lang = None;
            // A box whose own list style names another language.
            let lv = deckcraft_model::text::LevelStyle { run: RunProps { lang: Some("de-CH".into()), ..Default::default() }, ..Default::default() };
            deckcraft_model::find_shape_mut(list, german).unwrap().text.as_mut().unwrap().list_style.set(0, lv);
            Ok(())
        })
        .unwrap();
        assert_eq!(s.execute("format.language", &json!({"lang": "fr-FR"})).unwrap()["default"], true);
        assert_eq!(selection_language(&s).0.as_deref(), Some("fr-FR"));
        assert_eq!(runs_of(&s, old)[0].props.lang.as_deref(), Some("de-DE"), "existing text keeps its language");
        let st = s.doc().unwrap();
        assert_eq!(st.doc.default_text_style.level(0).unwrap().run.lang.as_deref(), Some("fr-FR"));
        assert!(st.doc.masters.iter().all(|m| m.body_style.level(0).is_some_and(|l| l.run.lang.as_deref() == Some("fr-FR"))));
        let boxed = st.doc.slides.iter().find_map(|sl| deckcraft_model::find_shape(&sl.shapes, german)).unwrap();
        assert_eq!(boxed.text.as_ref().unwrap().list_style.level(0).unwrap().run.lang.as_deref(), Some("de-CH"), "another language stays");
        // New text takes the new language.
        let new = text_shape(&mut s, "Bonjour");
        s.execute("text.edit", &json!({"id": new.0})).unwrap();
        assert_eq!(s.execute("format.state", &json!({})).unwrap()["lang"], "fr-FR");
    }

    #[test]
    fn the_font_list_has_the_theme_fonts_and_the_installed_families() {
        let mut s = Session::with_new();
        let v = s.execute("font.list", &json!({})).unwrap();
        assert_eq!(v["eastAsian"], false);
        let theme: Vec<crate::fonts::ThemeFontChoice> = serde_json::from_value(v["theme"].clone()).unwrap();
        let refs: Vec<(&str, &str, &str)> = theme.iter().map(|t| (t.name.as_str(), t.reference.as_str(), t.role.as_str())).collect();
        assert_eq!(refs, [("Inter", "+mj-lt", "headings"), ("Inter", "+mn-lt", "body")], "Latin only without East Asian editing");
        let fonts: Vec<crate::fonts::FontChoice> = serde_json::from_value(v["fonts"].clone()).unwrap();
        assert!(!fonts.is_empty());
        assert!(fonts.iter().all(|f| !f.name.starts_with('.') && f.names.first() == Some(&f.english)), "hidden faces are left out");

        // With East Asian editing, the theme's Asian text fonts: its Jpan font where it names no a:ea.
        let mut s = Session::new();
        s.execute("file.new", &json!({"eastAsian": "ja-JP"})).unwrap();
        s.east_asian_editing = Some(deckcraft_fonts::CjkLocale::Japanese);
        let v = s.execute("font.list", &json!({"lang": "ja", "fonts": false})).unwrap();
        assert_eq!(v["eastAsian"], true);
        assert_eq!(v["fonts"], json!([]));
        let theme: Vec<crate::fonts::ThemeFontChoice> = serde_json::from_value(v["theme"].clone()).unwrap();
        let ea: Vec<(&str, &str)> = theme.iter().filter(|t| t.east_asian).map(|t| (t.name.as_str(), t.reference.as_str())).collect();
        assert_eq!(ea, [("BIZ UDPGothic", "+mj-ea"), ("BIZ UDPGothic", "+mn-ea")]);
        s.execute("design.fonts", &json!({"majorEa": "テスト見出し"})).unwrap();
        let v = s.execute("font.list", &json!({"theme": true, "fonts": false})).unwrap();
        assert_eq!(v["theme"][2]["name"], "テスト見出し", "the theme's own a:ea first");
        assert_eq!(v["theme"][3]["name"], "BIZ UDPGothic");
        // eastAsian: false leaves them out.
        let v = s.execute("font.list", &json!({"eastAsian": false, "fonts": false})).unwrap();
        assert_eq!(v["theme"].as_array().map(Vec::len), Some(2));

        for bad in [json!({"lang": 3}), json!({"lang": ""}), json!({"lang": "ja JP"}), json!({"eastAsian": "yes"}), json!({"fonts": 1})] {
            assert!(s.execute("font.list", &bad).is_err(), "{bad}");
        }
    }

    /// An installed Japanese font is listed by its Japanese name in a Japanese interface and by its
    /// English name in an English one, and is an Asian text font. Skipped where none is installed.
    #[test]
    fn installed_japanese_fonts_are_listed_by_their_name_in_the_interface_language() {
        let mut s = Session::with_new();
        let list = |s: &mut Session, lang: &str| -> Vec<crate::fonts::FontChoice> {
            serde_json::from_value(s.execute("font.list", &json!({"lang": lang, "theme": false})).unwrap()["fonts"].clone()).unwrap()
        };
        let (ja, en) = (list(&mut s, "ja"), list(&mut s, "en"));
        let mut checked = 0;
        for family in ["BIZ UDPGothic", "Hiragino Sans", "Hiragino Mincho ProN"] {
            let Some(j) = ja.iter().find(|f| f.english == family && f.name != family) else { continue };
            checked += 1;
            assert!(j.east_asian, "{family}");
            assert!(j.names.contains(&j.name) && j.names.contains(&family.to_string()), "{j:?}");
            assert!(en.iter().any(|f| f.name == family && f.english == family), "{family} by its English name in English");
            assert!(!en.iter().any(|f| f.name == j.name), "{} only in Japanese", j.name);
        }
        if checked == 0 {
            eprintln!("skipped: no Japanese font with a Japanese name installed");
        }
    }

    #[test]
    fn font_dialog_sets_the_latin_and_asian_text_fonts() {
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "日本語abc");
        let n = undo_len(&s);
        s.execute("format.fontDialog", &json!({"family": "Liberation Serif", "eastAsian": "テスト明朝", "bold": true})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref(), r.bold), (Some("Liberation Serif"), Some("テスト明朝"), Some(true)));
        assert_eq!(undo_len(&s), n + 1);
        // The Latin text font box sets only the Latin font, even to an East Asian family (OK
        // sends it unchanged too, so it must not overwrite the Asian text font).
        s.execute("format.fontDialog", &json!({"family": "BIZ UDMincho 風", "italic": true})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref(), r.italic), (Some("BIZ UDMincho 風"), Some("テスト明朝"), Some(true)));
        s.execute("format.fontDialog", &json!({"eastAsian": "BIZ UDPGothic"})).unwrap();
        let r = runs_of(&s, id)[0].props.clone();
        assert_eq!((r.font.as_deref(), r.font_ea.as_deref()), (Some("BIZ UDMincho 風"), Some("BIZ UDPGothic")));
        assert!(s.execute("format.fontDialog", &json!({"eastAsian": 7})).is_err());
        assert!(s.execute("format.fontDialog", &json!({"eastAsian": " "})).is_err());
        assert_eq!(undo_len(&s), n + 3);
    }

    #[test]
    fn text_direction_takes_east_asian_vertical_and_rejects_unknown_values() {
        let mut s = shape();
        let vert = |s: &Session| s.doc().unwrap().selected_shapes().first().and_then(|sh| sh.text.as_ref()).unwrap().body.vert;
        for (dir, want) in [
            ("vertical", TextDir::EaVertical),
            ("eaVert", TextDir::EaVertical),
            ("stackedRtl", TextDir::StackedRtl),
            ("wordArtVertRtl", TextDir::StackedRtl),
            ("mongolianVert", TextDir::MongolianVertical),
            ("rotate90", TextDir::Vertical),
            ("vert270", TextDir::Vertical270),
            ("wordArtVert", TextDir::Stacked),
            ("stacked", TextDir::Stacked),
            ("horz", TextDir::Horizontal),
        ] {
            s.execute("format.direction", &json!({"dir": dir})).unwrap();
            assert_eq!(vert(&s), Some(want), "{dir}");
        }
        s.execute("format.direction", &json!({"dir": "vertical"})).unwrap();
        assert_eq!(s.execute("format.state", &json!({})).unwrap()["dir"], "vertical");
        let n = undo_len(&s);
        let err = s.execute("format.direction", &json!({"dir": "sideways"})).unwrap_err().to_string();
        assert!(err.contains("sideways"), "{err}");
        assert!(s.execute("format.direction", &json!({"dir": 90})).is_err());
        assert_eq!(undo_len(&s), n);
        assert_eq!(vert(&s), Some(TextDir::EaVertical), "an unknown direction changes nothing");
        // PowerPoint gets vert="eaVert".
        let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
        let back = deckcraft_pptx::import(&bytes).unwrap();
        let sh = back.slides[0].shapes.iter().find(|sh| sh.ph.is_none()).unwrap();
        assert_eq!(sh.text.as_ref().unwrap().body.vert, Some(TextDir::EaVertical));
    }

    #[test]
    fn change_case_converts_half_and_full_width_across_runs() {
        let mut s = Session::with_new();
        let id = text_shape(&mut s, "ＡＢＣ１２３　ｶﾞ");
        s.execute("text.edit", &json!({"id": id.0})).unwrap();
        // ｶ and its sound mark ﾞ in two runs.
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 8]})).unwrap();
        s.execute("format.bold", &json!({"on": true})).unwrap();
        s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 9]})).unwrap();
        let n = undo_len(&s);
        s.execute("format.case", &json!({"mode": "halfWidth"})).unwrap();
        let text = |s: &Session| runs_of(s, id).iter().map(|r| r.text.clone()).collect::<String>();
        assert_eq!(text(&s), "ABC123 ｶﾞ");
        assert_eq!(runs_of(&s, id)[0].text, "ABC123 ｶ");
        assert_eq!(runs_of(&s, id)[0].props.bold, Some(true));
        assert_eq!(undo_len(&s), n + 1);
        s.execute("format.case", &json!({"mode": "fullWidth"})).unwrap();
        assert_eq!(text(&s), "ＡＢＣ１２３　ガ");
        assert_eq!(runs_of(&s, id)[0].text, "ＡＢＣ１２３　ガ", "the sound mark composes into the kana's run");
        let sel = s.doc().unwrap().selection.text.clone().unwrap();
        assert_eq!(sel.ordered(), ((0, 0), (0, 8)), "the selection still covers the converted text");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(text(&s), "ABC123 ｶﾞ");

        // Unknown modes are errors, not sentence case.
        let n = undo_len(&s);
        for bad in [json!({"mode": "hiragana"}), json!({"mode": "katakana"}), json!({"mode": "Sentence"}), json!({"mode": 2})] {
            assert!(s.execute("format.case", &bad).is_err(), "{bad}");
        }
        assert_eq!(undo_len(&s), n);
        assert_eq!(text(&s), "ABC123 ｶﾞ");

        // A selected shape converts all its text.
        s.execute("text.exit", &json!({})).unwrap();
        s.execute("format.case", &json!({"mode": "fullWidth"})).unwrap();
        assert_eq!(text(&s), "ＡＢＣ１２３　ガ");
    }

    #[test]
    fn numbering_takes_east_asian_schemes_and_rejects_unknown_ones() {
        let mut s = shape();
        let scheme = |s: &Session| match para(s).bullet {
            Some(Bullet::AutoNum { scheme, .. }) => scheme,
            other => panic!("numbered, got {other:?}"),
        };
        s.execute("format.numbering", &json!({"on": true, "scheme": "circleNumDbPlain"})).unwrap();
        assert_eq!(scheme(&s), "circleNumDbPlain");
        s.execute("format.numbering", &json!({"on": true, "scheme": "ARABICDBPERIOD"})).unwrap();
        assert_eq!(scheme(&s), "arabicDbPeriod", "names are matched without case and stored as the schema spells them");
        let n = undo_len(&s);
        assert!(s.execute("format.numbering", &json!({"on": true, "scheme": "kanjiPeriod"})).unwrap_err().to_string().contains("kanjiPeriod"));
        assert!(s.execute("format.numbering", &json!({"on": true, "scheme": 5})).is_err());
        assert_eq!(undo_len(&s), n);
        assert_eq!(scheme(&s), "arabicDbPeriod");
        s.execute("format.numbering", &json!({"on": true})).unwrap();
        assert_eq!(scheme(&s), "arabicPeriod");

        let g = s.execute("format.numberingSchemes", &json!({})).unwrap();
        let list = g["schemes"].as_array().unwrap();
        assert_eq!(list.len(), NUMBERING_GALLERY.len());
        assert_eq!(list[0], json!({"scheme": "arabicPeriod", "sample": "1. 2. 3.", "eastAsian": false}));
        let sample = |name: &str| list.iter().find(|e| e["scheme"] == name).map(|e| (e["sample"].clone(), e["eastAsian"].clone())).unwrap();
        assert_eq!(sample("circleNumDbPlain"), (json!("① ② ③"), json!(true)));
        assert_eq!(sample("arabicDbPeriod"), (json!("１． ２． ３．"), json!(true)));
        assert_eq!(sample("ea1JpnChsDbPeriod"), (json!("一． 二． 三．"), json!(true)));
        assert!(NUMBERING_GALLERY.iter().all(|(g, _)| AUTONUM_SCHEMES.contains(g)));
        assert_eq!(AUTONUM_SCHEMES.len(), 41);
    }

    #[test]
    fn asian_typography_options_apply_in_one_undo_and_show_in_the_paragraph_dialog() {
        let mut s = shape();
        s.ui_requests.clear();
        s.execute("format.paragraphDialog", &json!({})).unwrap();
        let Some(UiRequest::Dialog { params, .. }) = s.ui_requests.last().cloned() else { panic!("dialog request") };
        assert_eq!(
            (&params["eaLnBrk"], &params["latinLnBrk"], &params["hangingPunct"], &params["fontAlgn"]),
            (&json!(true), &json!(false), &json!(true), &json!("auto"))
        );
        let n = undo_len(&s);
        s.execute("format.paragraphDialog", &json!({"eaLnBrk": false, "latinLnBrk": true, "hangingPunct": false, "fontAlgn": "baseline"})).unwrap();
        assert_eq!(undo_len(&s), n + 1);
        let p = para(&s);
        assert_eq!(
            (p.east_asian_line_break, p.latin_line_break, p.hanging_punct, p.font_align.as_deref()),
            (Some(false), Some(true), Some(false), Some("base"))
        );
        s.ui_requests.clear();
        s.execute("format.paragraphDialog", &json!({})).unwrap();
        let Some(UiRequest::Dialog { params, .. }) = s.ui_requests.last().cloned() else { panic!("dialog request") };
        assert_eq!(
            (&params["eaLnBrk"], &params["latinLnBrk"], &params["hangingPunct"], &params["fontAlgn"]),
            (&json!(false), &json!(true), &json!(false), &json!("base"))
        );
        s.execute("format.paragraph", &json!({"hangingPunct": true, "fontAlgn": "ctr"})).unwrap();
        let p = para(&s);
        assert_eq!((p.east_asian_line_break, p.hanging_punct, p.font_align.as_deref()), (Some(false), Some(true), Some("ctr")));

        let n = undo_len(&s);
        assert!(s.execute("format.paragraph", &json!({"eaLnBrk": "yes"})).is_err());
        assert!(s.execute("format.paragraph", &json!({"fontAlgn": "justified"})).is_err());
        assert!(s.execute("format.paragraphDialog", &json!({"hangingPunct": 1})).is_err());
        assert!(s.execute("format.paragraphDialog", &json!({"fontAlgn": 3})).is_err());
        assert_eq!(undo_len(&s), n);

        // PowerPoint reads them back.
        let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
        let back = deckcraft_pptx::import(&bytes).unwrap();
        let sh = back.slides[0].shapes.iter().find(|sh| sh.ph.is_none()).unwrap();
        let p = &sh.text.as_ref().unwrap().paragraphs[0].props;
        assert_eq!(
            (p.east_asian_line_break, p.latin_line_break, p.hanging_punct, p.font_align.as_deref()),
            (Some(false), Some(true), Some(true), Some("ctr"))
        );
    }
}
