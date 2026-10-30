use super::arrange::arrange_menu;
use super::galleries::{cref_param, effects_menu, outline_menu, quick_styles, shapes_gallery, smartart_gallery};
use super::insert::text_box_button;
use super::{
    Color32, CornerRadius, Icon, Rect, Sense, SlideApp, Stroke, Tokens, Ui, Value, big, big_button, big_button_ex, color_split, dropdown, east_asian,
    enabled, fmt_state, group, icon_toggle, json, pos2, rows, run, scheme, small, small_button, small_button_ex, small_button_named, tip, toggle,
    vec2, widgets,
};

// ---------- Home ----------

fn clipboard_group(app: &mut SlideApp, ui: &mut Ui) {
    group(ui, "Clipboard", Icon::Paste, |ui| {
        let (main, arrow) = big_button_ex(ui, Icon::Paste, "Paste", enabled(app, "edit.paste"), true, false);
        if main.on_hover_text(tl!("Paste (⌘V)")).clicked() {
            run(app, "edit.paste", json!({}));
        }
        if let Some(a) = arrow {
            egui::Popup::menu(&a).show(|ui| {
                if ui.button(tl!("Paste")).clicked() {
                    run(app, "edit.paste", json!({}));
                }
                if ui.button(tl!("Paste and Match Formatting")).clicked() {
                    run(app, "edit.pasteText", json!({}));
                }
                if ui.button(tl!("Paste Special…")).clicked() {
                    run(app, "app.pasteSpecial", json!({}));
                }
                if ui.button(tl!("Paste Formatting")).clicked() {
                    run(app, "format.painterApply", json!({}));
                }
            });
        }
        rows(ui, |ui| {
            small(app, ui, Icon::Cut, "Cut", "edit.cut", json!({}));
            small(app, ui, Icon::Copy, "Copy", "edit.copy", json!({}));
            let on = app.session.painter.is_some();
            let en = enabled(app, "format.painter");
            let r = small_button(ui, Icon::FormatPainter, "Format", en, on).on_hover_text(tl!("Format Painter (double-click to keep it on)"));
            if r.double_clicked() {
                run(app, "format.painter", json!({"sticky": true}));
            } else if r.clicked() {
                if on {
                    app.session.painter = None;
                } else {
                    run(app, "format.painter", json!({}));
                }
            }
        });
    });
}

pub(super) fn layout_menu(app: &mut SlideApp, ui: &mut Ui, cmd: &str) {
    let layouts: Vec<(String, deckcraft_model::LayoutId)> = app
        .session
        .active()
        .and_then(|d| d.doc.masters.first().map(|m| m.layouts.iter().map(|l| (l.name.clone(), l.id)).collect()))
        .unwrap_or_default();
    egui::Grid::new(("layouts", cmd)).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (k, (name, id)) in layouts.iter().enumerate() {
            let idx = app.session.active().and_then(|d| d.doc.masters.first().and_then(|m| m.layouts.iter().position(|l| l.id == *id)));
            let resp = ui.vertical(|ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 54.0), Sense::click());
                if let (Some(d), Some(li)) = (app.session.active(), idx) {
                    let img = deckcraft_render::render_layout(
                        &d.doc,
                        0,
                        Some(li),
                        &deckcraft_render::RenderOpts {
                            scale: 192.0 / d.doc.slide_size.width.max(1.0),
                            edit: true,
                            size: Some((192, 108)),
                            prompt: Some(crate::i18n::placeholder_prompt),
                            ..Default::default()
                        },
                    );
                    let tex = crate::textures::upload_editor(ui.ctx(), format!("layout-{li}"), crate::textures::to_color_image(&img, false));
                    let at = crate::textures::device_pixel_rect(rect, ui.ctx().pixels_per_point());
                    ui.painter().image(tex.id(), at, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                let t = Tokens::get(ui.ctx());
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { t.accent } else { t.border }),
                    egui::StrokeKind::Outside,
                );
                ui.add_sized(vec2(96.0, 14.0), egui::Label::new(egui::RichText::new(name).size(10.5)).truncate());
                resp
            });
            if resp.inner.clicked() {
                run(app, cmd, json!({"layout": id.0}));
                ui.close();
            }
            if k % 4 == 3 {
                ui.end_row();
            }
        }
    });
}

/// Render the Home Slides group: New Slide, Layout, Reset, and Section controls.
fn slides_group(app: &mut SlideApp, ui: &mut Ui) {
    group(ui, "Slides", Icon::NewSlide, |ui| {
        let (main, arrow) = big_button_ex(ui, Icon::NewSlide, "New\nSlide", enabled(app, "slide.new"), true, false);
        if main.on_hover_text(tl!("New Slide (⇧⌘N)")).clicked() {
            run(app, "slide.new", json!({}));
        }
        if let Some(a) = arrow {
            egui::Popup::menu(&a).width(420.0).show(|ui| new_slide_menu(app, ui));
        }
        rows(ui, |ui| {
            let (main, arrow) = small_button_ex(ui, Icon::Layout, "Layout", enabled(app, "slide.layout"), false, true);
            if let Some(arrow) = arrow {
                widgets::split_menu(&main, &arrow, |ui| layout_menu(app, ui, "slide.layout"));
            }
            small(app, ui, Icon::ResetSlide, "Reset", "slide.reset", json!({}));
            let (main, arrow) = small_button_ex(ui, Icon::Section, "Section", enabled(app, "section.add"), false, true);
            if let Some(arrow) = arrow {
                widgets::split_menu(&main, &arrow, |ui| section_menu(app, ui));
            }
        });
    });
}

/// The New Slide button's menu: layouts, duplication, outline import and slide reuse.
fn new_slide_menu(app: &mut SlideApp, ui: &mut Ui) {
    layout_menu(app, ui, "slide.new");
    ui.separator();
    if ui.button(tl!("Duplicate Selected Slides")).clicked() {
        run(app, "slide.duplicate", json!({}));
    }
    if ui.button(tl!("Slides from Outline…")).clicked() {
        app.dialog = Some(crate::dialogs::Dialog::new("outline"));
    }
    let desktop = app.services.pick_open.is_some();
    for (label, keep) in [("Reuse Slides…", false), ("Reuse Slides (Keep Source Formatting)…", true)] {
        let r = ui.add_enabled(desktop, egui::Button::new(tl!(label)));
        if r.clicked() {
            run(app, "app.reuseSlidesDialog", json!({"keepFormatting": keep}));
        }
        r.on_disabled_hover_text(tl!("Choosing a presentation to reuse needs the desktop app"));
    }
}

/// The Section split button's menu.
fn section_menu(app: &mut SlideApp, ui: &mut Ui) {
    if ui.button(tl!("Add Section")).clicked() {
        run(app, "section.add", json!({}));
    }
    if ui.button(tl!("Remove All Sections")).clicked() {
        run(app, "section.removeAll", json!({}));
    }
}

/// What the font box shows for the formatting state `st`: the font at the selection (the East
/// Asian font on Japanese text), with "(Headings)" or "(Body)" after the name when it is the
/// theme's, as PowerPoint's box does.
pub fn font_box_text(st: &Value) -> String {
    let font = st.get("font").and_then(Value::as_str).unwrap_or("");
    match st.get("fontTheme").and_then(Value::as_str) {
        Some(role) if !font.is_empty() => crate::fontpick::role_label(font, role),
        _ => font.to_string(),
    }
}

/// Home › Font: apply a font picked in the font box. A family is written by the name the list
/// shows; the engine applies an East Asian family to Japanese text too (`a:ea`). A theme entry
/// writes the theme's reference: the Latin heading or body font (`+mj-lt`, `+mn-lt`), or for
/// Asian text both the Latin and the East Asian reference, as an East Asian font sets both.
pub fn apply_font_pick(app: &mut SlideApp, pick: &crate::fontpick::Pick) {
    use crate::fontpick::Pick;
    let params = match pick {
        Pick::Font(f) => json!({"family": f}),
        Pick::Theme(t) if t.east_asian => {
            let latin = if t.role == "headings" { "+mj-lt" } else { "+mn-lt" };
            json!({"family": latin, "eastAsian": t.reference})
        }
        Pick::Theme(t) => json!({"family": t.reference}),
    };
    run(app, "format.font", params);
}

/// Change Case's choices, as (English label, `format.case` mode): the five Latin cases, then
/// Half-width and Full-width with East Asian editing.
pub(crate) fn change_case_modes(east_asian: bool) -> Vec<(&'static str, &'static str)> {
    let mut modes = vec![
        ("Sentence case.", "sentence"),
        ("lowercase", "lower"),
        ("UPPERCASE", "upper"),
        ("Capitalize Each Word", "title"),
        ("tOGGLE cASE", "toggle"),
    ];
    if east_asian {
        modes.extend([("Half-width", "halfWidth"), ("Full-width", "fullWidth")]);
    }
    modes
}

/// Text Direction's choices: (`format.direction` / `format.state` name, English label, East Asian
/// only). Vertical (`eaVert`: kana and kanji upright, Latin turned) and the stacked form with every
/// character upright and columns from right to left (`wordArtVertRtl`) come with East Asian
/// editing, as in PowerPoint.
pub(crate) const TEXT_DIRECTIONS: [(&str, &str, bool); 6] = [
    ("horizontal", "Horizontal", false),
    ("vertical", "Vertical", true),
    ("rotate90", "Rotate all text 90°", false),
    ("rotate270", "Rotate all text 270°", false),
    ("stacked", "Stacked", false),
    ("stackedRtl", "Vertical (all characters upright)", true),
];

/// The Text Direction choices to offer: every one with East Asian editing, else the four Latin
/// ones and whichever the selection already has.
pub(crate) fn text_directions(east_asian: bool, current: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    TEXT_DIRECTIONS.iter().filter(move |(d, _, ea)| !ea || east_asian || *d == current).map(|(d, l, _)| (*d, *l))
}

/// A Text Direction choice's label in the interface language.
pub(crate) fn direction_label(label: &str) -> &str {
    crate::i18n::tc("direction", label)
}

/// Where the lines of text in direction `dir` (a `format.state` name) follow each other when they
/// run down the shape: `Some(true)` from right to left (`eaVert`, `vert`, `wordArtVertRtl`),
/// `Some(false)` from left to right (`vert270`, `wordArtVert`, `mongolianVert`), `None` for
/// horizontal text.
pub(crate) fn vertical_lines_rtl(dir: &str) -> Option<bool> {
    match dir {
        "vertical" | "rotate90" | "stackedRtl" => Some(true),
        "rotate270" | "stacked" | "mongolianVert" => Some(false),
        _ => None,
    }
}

/// Align Text's choices for direction `dir`, as (English label, `format.anchor` value). The
/// anchor is the edge the first line stands at, so in vertical text it reads across the columns:
/// Right / Center / Left when the columns run from right to left, as PowerPoint names them.
pub(crate) fn align_text_choices(dir: &str) -> [(&'static str, &'static str); 3] {
    match vertical_lines_rtl(dir) {
        Some(true) => [("Right", "top"), ("Center", "middle"), ("Left", "bottom")],
        Some(false) => [("Left", "top"), ("Center", "middle"), ("Right", "bottom")],
        None => [("Top", "top"), ("Middle", "middle"), ("Bottom", "bottom")],
    }
}

/// The tooltips of Align Left, Center and Align Right for direction `dir`. A line of text turned a
/// quarter turn runs down the shape, from the top (turned clockwise: `eaVert`, `vert`, stacked) or
/// from the bottom (`vert270`), so the buttons align along the column.
pub(crate) fn paragraph_align_labels(dir: &str) -> [&'static str; 3] {
    match dir {
        "rotate270" => ["Align Bottom", "Center", "Align Top"],
        _ if vertical_lines_rtl(dir).is_some() => ["Align Top", "Center", "Align Bottom"],
        _ => ["Align Left", "Center", "Align Right"],
    }
}

/// The Numbering gallery, as (scheme, sample `1. 2. 3.`): `format.numberingSchemes` in order, the
/// East Asian schemes (① ② ③, １． ２． ３．, 一． 二． 三．) only with East Asian editing.
pub(crate) fn numbering_gallery(app: &mut SlideApp) -> Vec<(String, String)> {
    let ea = east_asian(app);
    let v = app.session.execute("format.numberingSchemes", &json!({})).unwrap_or_default();
    v.get("schemes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| ea || !s.get("eastAsian").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|s| Some((s.get("scheme")?.as_str()?.to_string(), s.get("sample")?.as_str()?.to_string())))
        .collect()
}

/// Let the interface faces borrow a face for the samples' characters (circled and full-width
/// digits, kanji), so they don't draw as boxes when the interface faces lack them.
pub(crate) fn note_samples(app: &mut SlideApp, ctx: &egui::Context, samples: &[(String, String)]) {
    let text: String = samples.iter().map(|(_, s)| s.as_str()).collect();
    let before = app.ui_fonts.faces.len();
    app.ui_fonts.note(ctx, &text);
    if app.ui_fonts.faces.len() != before {
        ctx.request_repaint();
    }
}

/// A Text Direction menu: each choice ticked when it is the selection's.
fn text_direction_menu(app: &mut SlideApp, ui: &mut Ui, current: &str, east_asian: bool) {
    for (dir, label) in text_directions(east_asian, current) {
        if widgets::check_item_shown(ui, direction_label(label), true, current == dir).clicked() {
            run(app, "format.direction", json!({"dir": dir}));
            ui.close();
        }
    }
}

fn font_group(app: &mut SlideApp, ui: &mut Ui, st: &Value) {
    let en = enabled(app, "format.bold");
    group(ui, "Font", Icon::Bold, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let shown = font_box_text(st);
                let size = st.get("size").and_then(Value::as_f64);
                let ctx = ui.ctx().clone();
                let mut rescan = false;
                let picked = {
                    let mut fetch = || crate::fontpick::font_list(app, &ctx);
                    let mut extra = |ui: &mut Ui| {
                        // Fonts installed while DeckCraft runs (e.g. into ~/.local/share/fonts).
                        if ui.button(tl!("Rescan Installed Fonts")).clicked() {
                            rescan = true;
                            ui.close();
                        }
                    };
                    crate::fontpick::font_box(ui, "font", 150.0, &shown, en, tl!("Font"), crate::fontpick::Offer::All, &mut fetch, Some(&mut extra))
                        .pick
                };
                if rescan {
                    run(app, "font.rescan", json!({}));
                    crate::fontpick::forget_fonts(&ctx);
                }
                if let Some(p) = picked {
                    apply_font_pick(app, &p);
                }
                let size_text = size.map(|s| widgets::fmt_num(s, "")).unwrap_or_default();
                dropdown(ui, "size", &size_text, 52.0, en, |ui| {
                    for s in deckcraft_engine::cmd::format::SIZES.iter().chain([72.0, 80.0, 88.0, 96.0].iter()) {
                        if ui.button(widgets::fmt_num(*s, "")).clicked() {
                            run(app, "format.size", json!({"size": s}));
                            ui.close();
                        }
                    }
                });
                toggle(app, ui, Icon::FontGrow, "Increase Font Size", "format.grow", false);
                toggle(app, ui, Icon::FontShrink, "Decrease Font Size", "format.shrink", false);
                toggle(app, ui, Icon::ClearFormatting, "Clear All Formatting", "format.clear", false);
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                let b = |k: &str| st.get(k).and_then(Value::as_bool).unwrap_or(false);
                toggle(app, ui, Icon::Bold, "Bold", "format.bold", b("bold"));
                toggle(app, ui, Icon::Italic, "Italic", "format.italic", b("italic"));
                toggle(app, ui, Icon::Underline, "Underline", "format.underline", b("underline"));
                toggle(app, ui, Icon::Strikethrough, "Strikethrough", "format.strikethrough", b("strike"));
                toggle(app, ui, Icon::Superscript, "Superscript", "format.superscript", b("superscript"));
                toggle(app, ui, Icon::Subscript, "Subscript", "format.subscript", b("subscript"));
                let r = icon_toggle(ui, Icon::CharSpacing, "Character Spacing", en, false);
                egui::Popup::menu(&r).show(|ui| {
                    for (label, pt) in [("Very Tight", -3.0), ("Tight", -1.5), ("Normal", 0.0), ("Loose", 3.0), ("Very Loose", 6.0)] {
                        if ui.button(tl!(label)).clicked() {
                            run(app, "format.spacing", json!({"pt": pt}));
                        }
                    }
                });
                let r = icon_toggle(ui, Icon::ChangeCase, "Change Case", en, false);
                let ea = east_asian(app);
                egui::Popup::menu(&r).show(|ui| {
                    for (label, mode) in change_case_modes(ea) {
                        if ui.button(tl!(label)).clicked() {
                            run(app, "format.case", json!({"mode": mode}));
                        }
                    }
                });
                let sc = scheme(app);
                let (m, a) = color_split(ui, Icon::Highlight, "Text Highlight Color", Color32::from_rgb(255, 230, 0), en);
                if m.clicked() {
                    run(app, "format.highlight", json!({"color": "#FFFF00"}));
                }
                widgets::color_popup(&a).show(|ui| {
                    if let Some(c) = widgets::color_grid(ui, &sc, Some("No Color")) {
                        run(app, "format.highlight", json!({"color": c.map(|c| serde_json::to_value(cref_param(&c)).unwrap_or_default())}));
                    }
                });
                let (m, a) = color_split(ui, Icon::FontColor, "Font Color", Color32::from_rgb(0xD8, 0x3F, 0x3F), en);
                if m.clicked() {
                    run(app, "format.color", json!({"color": "#C00000"}));
                }
                widgets::color_popup(&a).show(|ui| {
                    if let Some(Some(c)) = widgets::color_grid(ui, &sc, None) {
                        run(app, "format.color", json!({"color": cref_param(&c)}));
                    }
                });
                if ui.add_enabled(en, egui::Button::new(tl!("Font…")).min_size(vec2(52.0, 22.0))).on_hover_text(tl!("Font")).clicked() {
                    run(app, "format.fontDialog", json!({}));
                }
            });
        });
    });
}

/// Render paragraph formatting controls using the current paragraph state.
fn paragraph_group(app: &mut SlideApp, ui: &mut Ui, st: &Value) {
    let en = enabled(app, "format.bullets");
    group(ui, "Paragraph", Icon::AlignLeft, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                let b = |k: &str| st.get(k).and_then(Value::as_bool).unwrap_or(false);
                let (m, a) = small_button_named(ui, Icon::Bullets, "", "Bullets", en, b("bullets"), true);
                if m.on_hover_text(tl!("Bullets")).clicked() {
                    run(app, "format.bullets", json!({}));
                }
                if let Some(a) = a {
                    // The arrow is named for its menu, as PowerPoint's is (`small_button_named` would
                    // call it "Bullets options").
                    a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, en, tl!("Bullet styles")));
                    egui::Popup::menu(&a).show(|ui| {
                        if ui.button(tl!("None")).clicked() {
                            run(app, "format.bullets", json!({"on": false}));
                        }
                        for ch in ["•", "○", "■", "□", "◆", "➢", "✓", "–", "★"] {
                            let label = crate::i18n::fmt(tl!("{bullet}  Bullet"), &[("bullet", ch)]);
                            if ui.button(egui::RichText::new(label).size(14.0)).clicked() {
                                run(app, "format.bullets", json!({"on": true, "char": ch}));
                            }
                        }
                        ui.separator();
                        if ui.button(tl!("Bullets and Numbering…")).clicked() {
                            let mut dlg = crate::dialogs::Dialog::new("bullets");
                            dlg.fields.insert("tab".into(), "bulleted".into());
                            app.dialog = Some(dlg);
                        }
                    });
                }
                let (m, a) = small_button_named(ui, Icon::Numbering, "", "Numbering", en, b("numbering"), true);
                if m.on_hover_text(tl!("Numbering")).clicked() {
                    run(app, "format.numbering", json!({}));
                }
                if let Some(a) = a {
                    a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, en, tl!("Numbering styles")));
                    egui::Popup::menu(&a).show(|ui| {
                        let gallery = numbering_gallery(app);
                        note_samples(app, ui.ctx(), &gallery);
                        if ui.button(tl!("None")).clicked() {
                            run(app, "format.numbering", json!({"on": false}));
                        }
                        for (sch, sample) in &gallery {
                            if ui.button(sample).clicked() {
                                run(app, "format.numbering", json!({"on": true, "scheme": sch}));
                            }
                        }
                        ui.separator();
                        if ui.button(tl!("Bullets and Numbering…")).clicked() {
                            let mut dlg = crate::dialogs::Dialog::new("bullets");
                            dlg.fields.insert("tab".into(), "numbered".into());
                            app.dialog = Some(dlg);
                        }
                    });
                }
                toggle(app, ui, Icon::DecreaseIndent, "Decrease List Level", "format.outdent", false);
                toggle(app, ui, Icon::IncreaseIndent, "Increase List Level", "format.indent", false);
                let r = icon_toggle(ui, Icon::LineSpacing, "Line Spacing", en, false);
                egui::Popup::menu(&r).show(|ui| {
                    for v in [1.0, 1.5, 2.0, 2.5, 3.0] {
                        if ui.button(format!("{v:.1}")).clicked() {
                            run(app, "format.lineSpacing", json!({"lines": v}));
                        }
                    }
                    if ui.button(tl!("Line Spacing Options…")).clicked() {
                        run(app, "format.paragraphDialog", json!({}));
                    }
                    if ui.button(tl!("Tabs…")).clicked() {
                        app.dialog = Some(crate::dialogs::Dialog::new("tabs"));
                    }
                });
                let r = icon_toggle(ui, Icon::Columns, "Columns", en, false);
                egui::Popup::menu(&r).show(|ui| {
                    for (n, label) in [(1, "One Column"), (2, "Two Columns"), (3, "Three Columns")] {
                        if ui.button(tl!(label)).clicked() {
                            run(app, "format.columns", json!({"count": n, "spacing": 18}));
                        }
                    }
                    if ui.button("More Columns…").clicked() {
                        run(app, "format.columns", json!({}));
                    }
                });
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                let al = st.get("align").and_then(Value::as_str).unwrap_or("");
                let dir = st.get("dir").and_then(Value::as_str).unwrap_or("horizontal");
                let ea = east_asian(app);
                // In vertical text a line runs down its column, so the alignment buttons say
                // where along the column the text sits.
                let [left, center, right] = paragraph_align_labels(dir);
                toggle(app, ui, Icon::AlignLeft, left, "format.alignLeft", al == "l");
                toggle(app, ui, Icon::AlignCenter, center, "format.alignCenter", al == "ctr");
                toggle(app, ui, Icon::AlignRight, right, "format.alignRight", al == "r");
                toggle(app, ui, Icon::Justify, "Justify", "format.justify", al == "just");
                if ea
                    && widgets::icon_toggle_shown(
                        ui,
                        Icon::Distributed,
                        &tip("format.align", "Distributed"),
                        enabled(app, "format.align"),
                        al == "dist",
                    )
                    .clicked()
                {
                    run(app, "format.align", json!({"align": "distributed"}));
                }
                let r = icon_toggle(ui, Icon::TextDirection, "Text Direction", en, false);
                egui::Popup::menu(&r).show(|ui| text_direction_menu(app, ui, dir, ea));
                let r = icon_toggle(ui, Icon::AlignText, "Align Text", en, false);
                egui::Popup::menu(&r).show(|ui| {
                    for (l, a) in align_text_choices(dir) {
                        if ui.button(crate::i18n::tc("anchor", l)).clicked() {
                            run(app, "format.anchor", json!({"anchor": a}));
                        }
                    }
                });
                let r = icon_toggle(ui, Icon::SmartArt, "Convert to SmartArt", enabled(app, "smartArt.fromText"), false);
                egui::Popup::menu(&r).show(|ui| smartart_gallery(app, ui, "smartArt.fromText", json!({}), "", ("", ""), None));
                if ui.add_enabled(en, egui::Button::new(tl!("Paragraph…")).min_size(vec2(88.0, 22.0))).on_hover_text(tl!("Paragraph")).clicked() {
                    run(app, "format.paragraphDialog", json!({}));
                }
            });
        });
    });
}

/// Render drawing controls for shapes, text, arrange, quick styles, fill, and outline.
fn drawing_group(app: &mut SlideApp, ui: &mut Ui) {
    group(ui, "Drawing", Icon::Shapes, |ui| {
        let en = enabled(app, "shape.insert");
        let r = big_button(ui, Icon::Shapes, "Shapes", en).on_hover_text(tl!("Shapes"));
        egui::Popup::menu(&r).show(|ui| {
            if let Some(p) = shapes_gallery(ui) {
                app.session.set_tool(deckcraft_engine::ToolKind::Shape { preset: p.to_string() });
            }
        });
        text_box_button(app, ui, "Text Box");
        let sel = enabled(app, "shape.fill");
        let r = big_button(ui, Icon::Arrange, "Arrange", sel);
        egui::Popup::menu(&r).show(|ui| arrange_menu(app, ui));
        let r = big_button(ui, Icon::QuickStyles, "Quick\nStyles", sel);
        egui::Popup::menu(&r).width(330.0).show(|ui| quick_styles(app, ui));
        rows(ui, |ui| {
            let sc = scheme(app);
            let (m, a) = small_button_ex(ui, Icon::ShapeFill, "Shape Fill", sel, false, true);
            if m.clicked() {
                let c = app.last_fill.clone();
                run(app, "shape.fill", json!({"color": c}));
            }
            if let Some(a) = a {
                widgets::color_popup(&a).show(|ui| {
                    if let Some(c) = widgets::color_grid(ui, &sc, Some("No Fill")) {
                        match c {
                            Some(c) => {
                                app.last_fill = cref_param(&c);
                                run(app, "shape.fill", json!({"color": cref_param(&c)}));
                            }
                            None => run(app, "shape.fill", json!({"none": true})),
                        }
                    }
                });
            }
            let (m, a) = small_button_ex(ui, Icon::ShapeOutline, "Shape Outline", sel, false, true);
            if m.clicked() {
                let c = app.last_outline.clone();
                run(app, "shape.line", json!({"color": c}));
            }
            if let Some(a) = a {
                widgets::color_popup(&a).show(|ui| outline_menu(app, ui, &sc));
            }
            let (main, arrow) = small_button_ex(ui, Icon::ShapeEffects, "Shape Effects", sel, false, true);
            if let Some(arrow) = arrow {
                widgets::split_menu(&main, &arrow, |ui| effects_menu(app, ui));
            }
        });
    });
}

pub(super) fn home(app: &mut SlideApp, ui: &mut Ui) {
    let st = fmt_state(app);
    clipboard_group(app, ui);
    slides_group(app, ui);
    font_group(app, ui, &st);
    paragraph_group(app, ui, &st);
    group(ui, "Images", Icon::Picture, |ui| {
        big(app, ui, Icon::Picture, "Picture", "app.insertPictureDialog", json!({}));
    });
    drawing_group(app, ui);
    // Editing, as in PowerPoint: Find, Replace and Replace Fonts, then the Selection Pane.
    group(ui, "Editing", Icon::Search, |ui| {
        rows(ui, |ui| {
            small(app, ui, Icon::Search, "Find", "app.find", json!({}));
            small(app, ui, Icon::Spelling, "Replace", "app.replace", json!({}));
            if small_button(ui, Icon::Fonts, "Replace Fonts…", enabled(app, "font.replace"), false).clicked() {
                app.dialog = Some(crate::dialogs::Dialog::new("replaceFonts"));
            }
        });
        // Also under Arrange, but that is disabled until something is selected (#132).
        big(app, ui, Icon::SelectionPane, "Selection\nPane", "view.pane", json!({"pane": "selection"}));
    });
    group(ui, "Designer", Icon::DesignIdeas, |ui| {
        big(app, ui, Icon::DesignIdeas, "Design\nIdeas", "view.pane", json!({"pane": "designer"}));
    });
}
