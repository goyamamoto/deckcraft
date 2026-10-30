//! Design tokens (light, PowerPoint-like; and dark) and fonts for the UI.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Brightness {
    #[default]
    Light,
    Dark,
}

/// Every colour the chrome uses.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    /// Window background (title bar, tab row, side panes).
    pub chrome: Color32,
    /// The ribbon card and popovers.
    pub card: Color32,
    pub card_border: Color32,
    /// Text fields. They have no outline, so the fill alone must set them apart from the card.
    pub text_field: Color32,
    /// Canvas around the slide.
    pub canvas: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub border: Color32,
    pub separator: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    /// DeckCraft's accent (tabs underline, selected thumbnail, primary buttons).
    pub accent: Color32,
    pub accent_text: Color32,
    pub contextual: Color32,
    pub selection: Color32,
    pub handle_fill: Color32,
    pub handle_stroke: Color32,
    pub guide: Color32,
    pub caret: Color32,
    pub text_selection: Color32,
    pub field_bg: Color32,
    /// Unchecked checkbox and radio boxes and slider rails, drawn without an outline, so they
    /// must stand apart from the card and the chrome.
    pub control: Color32,
    /// [`Tokens::control`] under the pointer and pressed: a step darker each in light mode.
    pub control_hover: Color32,
    pub control_pressed: Color32,
    pub shadow: Color32,
}

impl Tokens {
    pub fn light() -> Self {
        Tokens {
            dark: false,
            chrome: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            card: Color32::WHITE,
            card_border: Color32::from_rgb(0xE3, 0xE3, 0xE3),
            text_field: Color32::from_rgb(0xEB, 0xEB, 0xEB),
            canvas: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            text: Color32::from_rgb(0x24, 0x24, 0x24),
            text_dim: Color32::from_rgb(0x5C, 0x5C, 0x5C),
            text_faint: Color32::from_rgb(0x99, 0x99, 0x99),
            border: Color32::from_rgb(0xD6, 0xD6, 0xD6),
            separator: Color32::from_rgb(0xE1, 0xE1, 0xE1),
            hover: Color32::from_rgb(0xEB, 0xEB, 0xEB),
            pressed: Color32::from_rgb(0xDE, 0xDE, 0xDE),
            selected: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            accent: Color32::from_rgb(0xD9, 0x5A, 0x1A),
            accent_text: Color32::WHITE,
            contextual: Color32::from_rgb(0xC2, 0x4E, 0x14),
            selection: Color32::from_rgb(0x7A, 0x7A, 0x7A),
            handle_fill: Color32::WHITE,
            handle_stroke: Color32::from_rgb(0x6E, 0x6E, 0x6E),
            guide: Color32::from_rgb(0xE8, 0x5D, 0x2A),
            caret: Color32::BLACK,
            text_selection: Color32::from_rgba_unmultiplied(0x5A, 0x8F, 0xE0, 90),
            field_bg: Color32::WHITE,
            control: Color32::from_rgb(0xD6, 0xD6, 0xD6),
            control_hover: Color32::from_rgb(0xC8, 0xC8, 0xC8),
            control_pressed: Color32::from_rgb(0xBC, 0xBC, 0xBC),
            shadow: Color32::from_black_alpha(28),
        }
    }
    pub fn dark() -> Self {
        Tokens {
            dark: true,
            chrome: Color32::from_rgb(0x26, 0x26, 0x26),
            card: Color32::from_rgb(0x33, 0x33, 0x33),
            card_border: Color32::from_rgb(0x44, 0x44, 0x44),
            text_field: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            canvas: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            text: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            text_dim: Color32::from_rgb(0xBB, 0xBB, 0xBB),
            text_faint: Color32::from_rgb(0x88, 0x88, 0x88),
            border: Color32::from_rgb(0x4A, 0x4A, 0x4A),
            separator: Color32::from_rgb(0x48, 0x48, 0x48),
            hover: Color32::from_rgb(0x40, 0x40, 0x40),
            pressed: Color32::from_rgb(0x4C, 0x4C, 0x4C),
            selected: Color32::from_rgb(0x47, 0x47, 0x47),
            accent: Color32::from_rgb(0xF2, 0x6B, 0x1D),
            accent_text: Color32::WHITE,
            contextual: Color32::from_rgb(0xF5, 0x8A, 0x4B),
            selection: Color32::from_rgb(0xAA, 0xAA, 0xAA),
            handle_fill: Color32::WHITE,
            handle_stroke: Color32::from_rgb(0x6E, 0x6E, 0x6E),
            guide: Color32::from_rgb(0xF2, 0x8A, 0x4B),
            caret: Color32::BLACK,
            text_selection: Color32::from_rgba_unmultiplied(0x5A, 0x8F, 0xE0, 90),
            field_bg: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            control: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            control_hover: Color32::from_rgb(0x40, 0x40, 0x40),
            control_pressed: Color32::from_rgb(0x4C, 0x4C, 0x4C),
            shadow: Color32::from_black_alpha(90),
        }
    }
    pub fn for_brightness(b: Brightness) -> Self {
        match b {
            Brightness::Light => Self::light(),
            Brightness::Dark => Self::dark(),
        }
    }
    pub fn get(ctx: &egui::Context) -> Self {
        if ctx.global_style().visuals.dark_mode { Self::dark() } else { Self::light() }
    }
}

/// Apply tokens to egui's style.
pub fn apply(ctx: &egui::Context, t: &Tokens) {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.chrome;
    v.window_fill = t.card;
    v.extreme_bg_color = t.field_bg;
    v.text_edit_bg_color = Some(t.text_field);
    v.faint_bg_color = t.hover;
    v.window_stroke = Stroke::new(1.0, t.card_border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = t.accent.gamma_multiply(0.85);
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.hyperlink_color = t.accent;
    v.override_text_color = Some(t.text);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(5);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = t.control;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.control_hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.bg_fill = t.control_pressed;
    v.widgets.open.weak_bg_fill = t.pressed;
    v.window_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: t.shadow };
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: t.shadow };
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
    });
}

/// UI fonts: Inter (from craft-fonts) when embedded, a Japanese UI face for CJK (the platform's
/// own where installed, else BIZ UDPGothic), then egui's.
pub fn install_fonts(ctx: &egui::Context) {
    install_fonts_with(ctx, &[]);
}

/// [`install_fonts`], with `extra` faces from the font database after every interface font: the
/// installed or embedded faces that draw text the interface faces lack (Hangul, Simplified
/// Chinese, Devanagari…, #101). See [`crate::uifonts`].
pub fn install_fonts_with(ctx: &egui::Context, extra: &[std::sync::Arc<deckcraft_fonts::FontFace>]) {
    ctx.set_fonts(font_definitions(extra));
}

/// The interface font definitions; see [`install_fonts_with`].
pub fn font_definitions(extra: &[std::sync::Arc<deckcraft_fonts::FontFace>]) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mut names = vec![];
    for (i, f) in deckcraft_fonts::CRAFT_FONTS.iter().enumerate() {
        let wanted = (f.family == "Inter" && (f.style == "Regular" || f.style == "SemiBold" || f.style == "Bold"))
            || (f.family == "BIZ UDPGothic" && (f.style == "Regular" || f.style == "Bold"))
            || (f.family == "Noto Sans Arabic");
        if !wanted {
            continue;
        }
        let name = format!("craft-{i}-{}-{}", f.family, f.style);
        defs.font_data.insert(name.clone(), std::sync::Arc::new(FontData::from_static(f.bytes)));
        names.push((f.family, f.style, name));
    }
    let is_bold_face = |fam: &str, st: &str| matches!((fam, st), ("Inter", "SemiBold" | "Bold") | ("BIZ UDPGothic", "Bold"));
    let regular: Vec<String> = names.iter().filter(|(fam, st, _)| !is_bold_face(fam, st)).map(|(_, _, n)| n.clone()).collect();
    if let Some(list) = defs.families.get_mut(&FontFamily::Proportional) {
        for (k, n) in regular.iter().enumerate() {
            list.insert(k, n.clone());
        }
    }
    // A bold family for headings and selected tabs: Inter SemiBold, then the Japanese interface
    // face's bold (a Japanese interface's selected tab and headings stay bold; web builds don't
    // embed it), then the regular chain for every other script.
    let face = |family: &'static str, style: &'static str| {
        names.iter().filter(move |(fam, st, _)| *fam == family && *st == style).map(|(_, _, n)| n.clone())
    };
    let mut bold: Vec<String> = face("Inter", "SemiBold").chain(face("BIZ UDPGothic", "Bold")).collect();
    bold.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    defs.families.insert(FontFamily::Name("bold".into()), bold);
    let platform = add_platform_japanese_faces(&mut defs);
    add_fallback_faces(&mut defs, extra);
    // A Chinese, Japanese or Korean interface without Inter (a build without craft-fonts) puts its
    // East Asian face first: its own Latin letters, digits and brackets then go with its kana and
    // kanji, where egui's Ubuntu Light drew them thin and apart from the Japanese text.
    if !names.iter().any(|(fam, _, _)| *fam == "Inter") && crate::i18n::current().uses_cjk() {
        let cjk = platform.clone().map(|(regular, bold)| (regular.clone(), bold.unwrap_or(regular))).or_else(|| {
            let f = extra.iter().find(|f| ['あ', '漢', '한'].iter().any(|c| f.covers(*c)) && ['A', '1', '('].iter().all(|c| f.covers(*c)))?;
            let name = format!("fallback {} {} {}", f.family, f.style, f.id());
            Some((name.clone(), name))
        });
        if let Some((regular, bold)) = cjk {
            for (family, name) in [(FontFamily::Proportional, &regular), (FontFamily::Name("bold".into()), &bold)] {
                if let Some(list) = defs.families.get_mut(&family) {
                    list.retain(|n| n != name);
                    list.insert(0, name.clone());
                }
            }
            for name in [&regular, &bold] {
                if let Some(d) = defs.font_data.get_mut(name) {
                    let mut data = (**d).clone();
                    data.tweak.y_offset_factor = 0.0;
                    *d = std::sync::Arc::new(data);
                }
            }
        }
    }
    align_baselines(&mut defs);
    // The Japanese interface face (the first one when Inter isn't embedded) hints its en and em
    // dashes, bars thinner than a pixel, down to nothing, so they drew as gaps in interface text;
    // unhinted, they draw.
    for (name, d) in defs.font_data.iter_mut() {
        if name.contains("BIZ UDPGothic") {
            let mut data = (**d).clone();
            data.tweak.hinting = Some(false);
            *d = std::sync::Arc::new(data);
        }
    }
    defs
}

/// The platform's Japanese interface face, Hiragino Sans on macOS (Yu Gothic UI on Windows), when
/// installed and the interface draws kanji the Japanese way ([`crate::uifonts::interface_cjk`]):
/// it draws kana and kanji as the system's own menus do, ahead of BIZ UDPGothic, which stays for
/// what it lacks and draws them on Linux and the web. W4 goes with Inter Regular and W6 with
/// Inter SemiBold, whose strokes they match. The names of the regular and bold faces added.
fn add_platform_japanese_faces(defs: &mut FontDefinitions) -> Option<(String, Option<String>)> {
    use deckcraft_fonts::CjkLocale;
    if crate::uifonts::interface_cjk(deckcraft_fonts::default_cjk_locale()) != CjkLocale::Japanese {
        return None;
    }
    let db = deckcraft_fonts::FontDb::global();
    let mut add = |bold: bool, family: FontFamily| {
        let f = db.platform_interface_face(CjkLocale::Japanese, bold)?;
        let name = format!("platform {} {}", f.family, f.style);
        let mut data = FontData::from_static(static_bytes(&f));
        data.index = f.index();
        defs.font_data.insert(name.clone(), std::sync::Arc::new(data));
        let list = defs.families.get_mut(&family)?;
        // Before the bundled Japanese face, else after the first face (Inter, or egui's own), which
        // stays first: its baseline is the interface's, and Hiragino's line gap would make every
        // row taller.
        let at = list.iter().position(|n| n.contains("BIZ UDPGothic")).unwrap_or(1).min(list.len());
        list.insert(at, name.clone());
        Some(name)
    };
    let regular = add(false, FontFamily::Proportional)?;
    let bold = add(true, FontFamily::Name("bold".into()));
    // The bold family falls back to the regular chain, which now has the regular face too.
    Some((regular, bold))
}

/// An installed face's bytes for egui, which keeps font data `'static`: copied once and kept for
/// the process (a couple of faces), so installing the interface fonts again copies nothing.
fn static_bytes(f: &deckcraft_fonts::FontFace) -> &'static [u8] {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex, PoisonError};
    type Key = (String, String, u32, usize);
    static KEPT: LazyLock<Mutex<HashMap<Key, &'static [u8]>>> = LazyLock::new(Default::default);
    let key = (f.family.clone(), f.style.clone(), f.index(), f.data().len());
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    kept.entry(key).or_insert_with(|| Box::leak(f.data().to_vec().into_boxed_slice()))
}

/// Add `faces` after every other font of each interface family (the last fallbacks).
pub fn add_fallback_faces(defs: &mut FontDefinitions, faces: &[std::sync::Arc<deckcraft_fonts::FontFace>]) {
    for f in faces {
        let name = format!("fallback {} {} {}", f.family, f.style, f.id());
        if defs.font_data.contains_key(&name) {
            continue;
        }
        let mut data = FontData::from_owned(f.data().to_vec());
        data.index = f.index();
        defs.font_data.insert(name.clone(), std::sync::Arc::new(data));
        for list in defs.families.values_mut() {
            list.push(name.clone());
        }
    }
}

/// Puts every fallback face's baseline on its family's first face's. egui centres a fallback
/// face's line (ascent, descent and line gap) in the first face's, so a face with other vertical
/// metrics sits higher or lower: Japanese in Hiragino or BIZ UDPGothic next to Latin in Inter or
/// Ubuntu drew above the Latin baseline, and the half-width brackets and dots of 英語 (米国) and
/// フォント… looked dropped. Each fallback face gets the shift that undoes the centring, worked out
/// against the proportional family's first face (the interface text); that face itself is never
/// moved.
fn align_baselines(defs: &mut FontDefinitions) {
    // Ascent and line height in ems of the font size, at the face's own scale (egui draws its
    // emoji faces at 0.81).
    let metrics = |defs: &FontDefinitions, name: &str| {
        let d = defs.font_data.get(name)?;
        let s = d.tweak.scale;
        deckcraft_fonts::em_metrics(&d.font, d.index).map(|(a, de, l)| (a * s, (a - de + l) * s, s))
    };
    let mut families: Vec<FontFamily> = defs.families.keys().filter(|f| **f != FontFamily::Proportional).cloned().collect();
    families.push(FontFamily::Proportional);
    let anchor = defs.families.get(&FontFamily::Proportional).and_then(|l| l.first()).cloned();
    for family in families {
        let Some(list) = defs.families.get(&family).cloned() else { continue };
        let Some(((ascent, height, _), rest)) = list.split_first().and_then(|(first, rest)| Some((metrics(defs, first)?, rest))) else {
            continue;
        };
        for name in rest {
            if Some(name) == anchor.as_ref() {
                continue;
            }
            let Some((a, h, scale)) = metrics(defs, name) else { continue };
            // egui moves a face by its offset times the font size at the face's scale.
            let shift = ((ascent - a) - 0.5 * (height - h)) / scale.max(0.01);
            if let Some(d) = defs.font_data.get_mut(name)
                && shift.is_finite()
                && (d.tweak.y_offset_factor - shift).abs() > 1e-4
            {
                let mut data = (**d).clone();
                data.tweak.y_offset_factor = shift;
                *d = std::sync::Arc::new(data);
            }
        }
    }
}

/// Whether the interface fonts of `family` in `defs` draw `c` with a glyph of their own: one of the
/// family's faces maps it. egui's own `has_glyph` says no for every character of the face that also
/// draws its missing-glyph box (◻), which the Japanese interface faces have, so it can't be asked.
pub fn draws(defs: &FontDefinitions, family: &FontFamily, c: char) -> bool {
    defs.families
        .get(family)
        .is_some_and(|list| list.iter().filter_map(|n| defs.font_data.get(n)).any(|d| deckcraft_fonts::maps_char(&d.font, d.index, c)))
}

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn font(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}

pub fn to_color32(c: deckcraft_color::Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

#[cfg(test)]
mod tests {
    /// Each character of `text` laid out in the interface fonts at interface sizes, and whether it
    /// has ink (a glyph image).
    fn drawn(text: &str) -> Vec<(char, f32, bool)> {
        let ctx = egui::Context::default();
        ctx.set_fonts(super::font_definitions(&[]));
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        let mut out = vec![];
        ctx.fonts_mut(|f| {
            for size in [11.0, 12.0, 13.0, 16.0] {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Name("bold".into())] {
                    let g = f.layout_no_wrap(text.to_string(), egui::FontId::new(size, family), egui::Color32::WHITE);
                    for gl in g.rows.iter().flat_map(|r| &r.glyphs) {
                        out.push((gl.chr, size, gl.uv_rect.size.x > 0.0 && gl.uv_rect.size.y > 0.0));
                    }
                }
            }
        });
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        out
    }

    #[test]
    fn fallback_faces_share_the_interface_baseline() {
        // egui centres a fallback face's line in the first face's, so a face with other vertical
        // metrics (Hiragino or BIZ UDPGothic beside Inter or Ubuntu) drew off the baseline. Every
        // fallback face is shifted back onto it; the first face never moves.
        let defs = super::font_definitions(&[]);
        let list = defs.families.get(&egui::FontFamily::Proportional).unwrap().clone();
        let tweak = |n: &str| defs.font_data.get(n).unwrap().tweak.y_offset_factor;
        assert_eq!(tweak(&list[0]), 0.0);
        let ctx = egui::Context::default();
        ctx.set_fonts(defs.clone());
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        // 'A' comes from the first face, the emoji from a fallback one.
        let size = 13.0;
        let (a, e) = ctx.fonts_mut(|f| {
            let g = f.layout_no_wrap("A\u{1F600}".to_string(), egui::FontId::proportional(size), egui::Color32::WHITE);
            let glyphs: Vec<_> = g.rows.iter().flat_map(|r| r.glyphs.clone()).collect();
            (glyphs[0].pos.y, glyphs[1].pos.y)
        });
        // The fallback face that draws the emoji.
        let fallback = list.iter().skip(1).find(|n| {
            defs.font_data
                .get(*n)
                .is_some_and(|d| deckcraft_fonts::maps_char(&d.font, d.index, '\u{1F600}') && deckcraft_fonts::em_metrics(&d.font, d.index).is_some())
        });
        let shift = fallback.map(|n| tweak(n)).unwrap();
        assert!(shift != 0.0, "the emoji face's metrics differ from the first face's");
        assert!((e + shift * size - a).abs() <= 1.0, "baselines: {a} vs {e} + {}", shift * size);
    }

    #[test]
    fn light_checkboxes_and_slider_rails_stand_out_from_their_background() {
        // An unchecked checkbox and a slider rail are filled with the inactive widget fill and
        // have no outline, so in light mode they vanished into the white dialog (#766).
        let ctx = egui::Context::default();
        super::apply(&ctx, &super::Tokens::light());
        let v = ctx.global_style().visuals.clone();
        assert_eq!(v.widgets.inactive.bg_stroke, egui::Stroke::NONE, "the test assumes no outline");
        let fill = v.widgets.inactive.bg_fill;
        for (name, bg) in [("dialog", v.window_fill), ("panel", v.panel_fill)] {
            let diff = [fill.r().abs_diff(bg.r()), fill.g().abs_diff(bg.g()), fill.b().abs_diff(bg.b())];
            assert!(diff.iter().all(|d| *d >= 24), "control fill {fill:?} is too close to the {name} background {bg:?}");
        }
        // Hovering and pressing darken the control a step each, as they do buttons.
        let (hovered, active) = (v.widgets.hovered.bg_fill, v.widgets.active.bg_fill);
        assert!(fill.r() > hovered.r() && hovered.r() > active.r(), "idle {fill:?}, hovered {hovered:?}, active {active:?} don't darken in turn");
    }

    #[test]
    fn thin_punctuation_is_drawn_in_interface_text() {
        // En and em dashes (thin bars) were hinted down to nothing in the Japanese interface face,
        // so notes and the outline showed gaps where they were.
        for (c, size, ink) in drawn("\u{2013}\u{2014}-_\u{2026}\u{2022}\u{00b7}\u{2018}\u{2019}\u{201c}\u{201d}=~") {
            assert!(ink, "{c:?} ({:04X}) draws nothing at {size} px", c as u32);
        }
    }

    #[test]
    fn japanese_interface_text_takes_the_platforms_face_where_installed() {
        let db = deckcraft_fonts::FontDb::global();
        let Some(face) = db.platform_interface_face(deckcraft_fonts::CjkLocale::Japanese, false) else {
            return; // Linux, the web, or no Hiragino / Yu Gothic UI: BIZ UDPGothic draws it.
        };
        if crate::uifonts::interface_cjk(deckcraft_fonts::default_cjk_locale()) != deckcraft_fonts::CjkLocale::Japanese {
            return;
        }
        let defs = super::font_definitions(&[]);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Name("bold".into())] {
            let list = &defs.families[&family];
            let platform = list.iter().position(|n| n.starts_with("platform ")).expect("the platform face is in the chain");
            // Ahead of the bundled Japanese face and every fallback, after the Latin faces.
            if let Some(biz) = list.iter().position(|n| n.contains("BIZ UDPGothic")) {
                assert!(platform < biz, "{family:?}: {list:?}");
            }
            assert!(platform >= 1, "the Latin face stays first: {family:?}: {list:?}");
        }
        let defs_shift =
            defs.font_data.iter().find(|(n, _)| n.starts_with("platform ") && !n.contains("W6")).map_or(0.0, |(_, d)| d.tweak.y_offset_factor);
        // The face draws them, though egui's has_glyph says otherwise for the face that also has
        // its missing-glyph box (◻).
        assert!("あ漢A".chars().all(|c| super::draws(&defs, &egui::FontFamily::Proportional, c)));
        assert!(!super::draws(&defs, &egui::FontFamily::Proportional, '\u{0378}'), "unassigned: nothing draws it");
        let ctx = egui::Context::default();
        ctx.set_fonts(defs);
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        // Kana sit on the Latin baseline in the platform's face, its line gap (half an em in
        // Hiragino) and all: egui adds the face's offset when it paints.
        let shift = defs_shift;
        for size in [11.0, 12.0, 13.0, 16.0, 18.0] {
            let (a, ka) = ctx.fonts_mut(|f| {
                let g = f.layout_no_wrap("Aあ".to_string(), egui::FontId::proportional(size), egui::Color32::WHITE);
                let glyphs: Vec<_> = g.rows.iter().flat_map(|r| r.glyphs.clone()).collect();
                (glyphs[0].pos.y, glyphs[1].pos.y)
            });
            assert!((ka + shift * size - a).abs() <= 1.0, "{size} pt: {a} vs {ka} + {}", shift * size);
        }
        assert!(face.covers('あ'));
    }
}
