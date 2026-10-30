use super::{Align2, Color32, CornerRadius, Dialog, Rect, Sense, SlideApp, Stroke, Tokens, Ui, Value, json, pos2, theme, vec2};
use deckcraft_math::{Math, TexError};

/// The Equation dialog's preview picture: the TeX it shows, its texture and its size in points.
pub struct EquationPreview {
    pub source: String,
    pub dark: bool,
    pub display: bool,
    pub texture: egui::TextureHandle,
    pub size: egui::Vec2,
}

/// Point size the preview is set at (the inserted equation takes the text's own size).
const EQUATION_PREVIEW_PT: f64 = 22.0;

/// Put the TeX editor's caret after its text (a seeded or picked equation is edited at its end).
fn equation_caret_to_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let end = egui::text::CCursor::new(text.chars().count());
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
    state.store(ctx, id);
}
const EQUATION_DIALOG_W: f32 = 480.0;

/// The dialog's first fields: the style, and the TeX given in the params or of the equation at
/// the caret (which is then selected, so Insert replaces it).
fn seed_equation(app: &mut SlideApp, d: &mut Dialog) {
    d.fields.insert("seeded".into(), "1".into());
    // Display style for an equation in a box of its own, text style at an insertion point.
    let inline = app.session.active().is_some_and(|st| st.selection.text.is_some());
    d.fields.insert("display".into(), if inline { "0" } else { "1" }.into());
    if let Some(v) = d.params.get("display").and_then(Value::as_bool) {
        d.fields.insert("display".into(), if v { "1" } else { "0" }.into());
    }
    let given = d.params.get("tex").and_then(Value::as_str).map(str::to_string);
    if let Some(tex) = given {
        d.fields.insert("tex".into(), tex);
    } else if let Ok(at) = app.run("equation.at", json!({}))
        && let Some(tex) = at.get("tex").and_then(Value::as_str)
    {
        // Editing: the equation is selected, so Insert replaces it. A selected equation box
        // (not being edited) is entered first, and left again when the dialog closes.
        d.fields.insert("tex".into(), tex.to_string());
        if let Some(v) = at.get("display").and_then(Value::as_bool) {
            d.fields.insert("display".into(), if v { "1" } else { "0" }.into());
        }
        if let Some(id) = at.get("id") {
            let _ = app.run("text.edit", json!({"id": id, "at": at["start"]}));
            d.fields.insert("exit".into(), "1".into());
        }
        let _ = app.run("text.select", json!({"anchor": at["start"], "caret": at["end"]}));
    }
}

/// The TeX editor under its prompt, with the keyboard on the first frame; returns its response
/// and the TeX as typed (kept in the dialog's fields).
fn tex_editor(ui: &mut Ui, d: &mut Dialog, t: &Tokens, edit_id: egui::Id) -> (egui::Response, String) {
    let mut tex = d.get("tex", "");
    ui.set_min_width(EQUATION_DIALOG_W);
    let prompt = ui.label(tl!("Type the equation in TeX; the preview follows as you type."));
    // A steady caret, two points wide, in the accent colour: the place being edited must be
    // easy to find in a line of TeX.
    {
        let cursor = &mut ui.visuals_mut().text_cursor;
        cursor.blink = false;
        cursor.stroke = Stroke::new(2.0, t.accent);
    }
    // A framed field with a focus ring, and no hint text inside it: the caret at the start of
    // an empty editor was lost on a hint's first glyph (the examples are on the line below).
    let focused = ui.memory(|m| m.has_focus(edit_id));
    let frame = egui::Frame::new()
        .fill(t.field_bg)
        .stroke(Stroke::new(if focused { 1.5 } else { 1.0 }, if focused { t.accent } else { t.border }))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 6));
    let edit = egui::TextEdit::multiline(&mut tex)
        .id(edit_id)
        .frame(frame)
        .desired_rows(3)
        .desired_width(EQUATION_DIALOG_W)
        .font(egui::FontId::monospace(14.0));
    // The prompt above names the field: it has no hint text to name it (see the frame above).
    let r = ui.add(edit).labelled_by(prompt.id);
    if !d.fields.contains_key("focused") {
        r.request_focus();
        d.fields.insert("focused".into(), "1".into());
        // The focus takes effect next frame; ask for it now, else the caret waits for the
        // next mouse move.
        ui.ctx().request_repaint();
    }
    if r.changed() {
        d.fields.remove("error");
    }
    d.fields.insert("tex".into(), tex.clone());
    (r, tex)
}

/// Draw `tex` again for the preview when it, the theme's brightness or the style changed since
/// the picture was drawn.
fn refresh_equation_preview(app: &mut SlideApp, ui: &Ui, t: &Tokens, m: &Math, tex: &str, display: bool) {
    let dark = app.ui.brightness == theme::Brightness::Dark;
    if !app.equation_preview.as_ref().is_none_or(|p| p.source != tex || p.dark != dark || p.display != display) {
        return;
    }
    let c = t.text;
    let color = deckcraft_color::Rgba::rgb(c.r(), c.g(), c.b());
    let ppp = f64::from(ui.ctx().pixels_per_point());
    if let Some(img) = deckcraft_render::render_equation(m, deckcraft_engine::cmd::equation::MATH_FONT, EQUATION_PREVIEW_PT, ppp, color, display) {
        let size = vec2(img.width as f32 / ppp as f32, img.height as f32 / ppp as f32);
        let ci = crate::textures::to_color_image(&img, false);
        let texture = match app.equation_preview.as_mut() {
            Some(p) => {
                crate::textures::replace(ui.ctx(), &mut p.texture, ci);
                p.texture.clone()
            }
            None => crate::textures::upload(ui.ctx(), "equation-preview", ci),
        };
        app.equation_preview = Some(EquationPreview { source: tex.to_string(), dark, display, texture, size });
    }
}

/// The preview box: the last good drawing, faded while the TeX doesn't parse, or "Preview" when
/// there is no TeX.
fn paint_equation_preview(app: &SlideApp, ui: &mut Ui, t: &Tokens, parsed: &Result<Math, TexError>, empty: bool) {
    let (area, _) = ui.allocate_exact_size(vec2(EQUATION_DIALOG_W, 110.0), Sense::hover());
    ui.painter().rect(area, CornerRadius::same(4), t.field_bg, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    match (parsed, empty) {
        (_, true) => {
            ui.painter().text(area.center(), Align2::CENTER_CENTER, tl!("Preview"), theme::font(13.0), t.text_faint);
        }
        _ => {
            if let Some(p) = app.equation_preview.as_ref() {
                let fit = (1.0f32).min((area.width() - 16.0) / p.size.x.max(1.0)).min((area.height() - 12.0) / p.size.y.max(1.0));
                let size = p.size * fit;
                let rect = Rect::from_center_size(area.center(), size);
                let tint = if parsed.is_ok() { Color32::WHITE } else { Color32::from_white_alpha(110) };
                ui.painter().image(p.texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
            }
        }
    }
}

/// The line under the preview: the insert's or the TeX's error, else a few TeX examples.
fn equation_message(ui: &mut Ui, d: &Dialog, t: &Tokens, parsed: &Result<Math, TexError>, empty: bool) {
    let error = match (parsed, d.fields.get("error")) {
        (_, Some(e)) => Some(e.clone()),
        (Err(e), None) if !empty => Some(e.to_string()),
        _ => None,
    };
    let error_h = 18.0;
    match &error {
        Some(e) => {
            ui.add_sized(
                vec2(EQUATION_DIALOG_W, error_h),
                egui::Label::new(egui::RichText::new(e).color(Color32::from_rgb(200, 48, 48)).size(12.0)).truncate(),
            );
        }
        None => {
            ui.add_sized(
                vec2(EQUATION_DIALOG_W, error_h),
                egui::Label::new(
                    egui::RichText::new(tl!(
                        "Fractions \\frac{a}{b}, scripts x^2 y_i, roots \\sqrt{x}, \\sum_{i=1}^n, \\left( \\right), \\begin{pmatrix}, Greek \\alpha."
                    ))
                    .color(t.text_dim)
                    .size(12.0),
                )
                .truncate(),
            );
        }
    }
}

/// The equations inserted before, with Clear; returns the one picked to edit again.
fn recent_equations(app: &mut SlideApp, ui: &mut Ui, t: &Tokens) -> Option<String> {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Recent")).font(theme::bold(12.0)));
        if !app.ui.equations.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(tl!("Clear")).clicked() {
                    app.ui.equations.clear();
                }
            });
        }
    });
    let mut pick: Option<String> = None;
    egui::ScrollArea::vertical().id_salt(crate::wid("equation-recent")).max_height(96.0).show(ui, |ui| {
        ui.set_min_width(EQUATION_DIALOG_W);
        if app.ui.equations.is_empty() {
            ui.label(egui::RichText::new(tl!("Equations you insert are listed here.")).color(t.text_faint).size(12.0));
        }
        for h in &app.ui.equations {
            let one_line: String = h.split_whitespace().collect::<Vec<_>>().join(" ");
            if ui.add(egui::Button::selectable(false, egui::RichText::new(one_line).font(egui::FontId::monospace(12.0))).truncate()).clicked() {
                pick = Some(h.clone());
            }
        }
    });
    pick
}

/// Insert (also ⌘↩ / Ctrl+Enter, when `can`) and Cancel; returns (insert, cancel).
fn equation_buttons(ui: &mut Ui, t: &Tokens, can: bool) -> (bool, bool) {
    let mut insert = false;
    let mut cancel = false;
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let b = egui::Button::new(egui::RichText::new(tl!("Insert")).color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0));
        if ui.add_enabled(can, b).clicked() {
            insert = true;
        }
        if ui.add(egui::Button::new(tl!("Cancel")).min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
        if can && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
            insert = true;
        }
    });
    (insert, cancel)
}

/// Insert the equation (an error stays in the dialog) or cancel, leaving an equation box the
/// dialog entered; returns whether the dialog closes.
fn finish_equation(app: &mut SlideApp, d: &mut Dialog, insert: bool, cancel: bool) -> bool {
    if insert {
        let tex = d.get("tex", "");
        let display = d.get("display", "1") == "1";
        match app.run("insert.equation", json!({"tex": tex, "display": display})) {
            Ok(_) => {
                app.ui.remember_equation(&tex);
                if d.fields.contains_key("exit") {
                    let _ = app.run("text.exit", json!({}));
                }
                return true;
            }
            Err(e) => {
                d.fields.insert("error".into(), e);
            }
        }
    }
    if cancel && d.fields.contains_key("exit") {
        let _ = app.run("text.exit", json!({}));
    }
    cancel
}

/// Insert ▸ Equation: TeX typed above, the equation drawn below as you type (the last good
/// drawing stays while an edit is half-typed, with the error named), Insert (⌘↩) puts it at the
/// insertion point or in a text box of its own, and the equations inserted before are listed to
/// pick up again. Opened with the caret in or beside an equation, it edits that one.
pub(super) fn equation_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    if !d.fields.contains_key("seeded") {
        seed_equation(app, d);
    }
    let edit_id = crate::wid("equation-tex");
    if d.fields.get("seeded").is_some_and(|v| v == "1") {
        // The first frame: the caret after the seeded TeX (the editor's state outlives the dialog).
        d.fields.insert("seeded".into(), "2".into());
        equation_caret_to_end(ui.ctx(), edit_id, &d.get("tex", ""));
    }
    let (r, tex) = tex_editor(ui, d, &t, edit_id);
    let parsed = deckcraft_math::parse_tex(&tex);
    let empty = tex.trim().is_empty();
    // Display style (as TeX's `\displaystyle` or a displayed equation: limits above and below
    // large operators, full-size fraction parts) or text style, as in a line of text.
    let mut display = d.get("display", "1") == "1";
    if ui.checkbox(&mut display, tl!("Display style")).changed() {
        d.fields.insert("display".into(), if display { "1" } else { "0" }.into());
    }
    // Preview.
    if let Ok(m) = &parsed
        && !empty
    {
        refresh_equation_preview(app, ui, &t, m, &tex, display);
    }
    paint_equation_preview(app, ui, &t, &parsed, empty);
    equation_message(ui, d, &t, &parsed, empty);
    // Recent equations.
    if let Some(h) = recent_equations(app, ui, &t) {
        equation_caret_to_end(ui.ctx(), edit_id, &h);
        d.fields.insert("tex".into(), h);
        d.fields.remove("error");
    }
    // Insert (also ⌘↩ / Ctrl+Enter) and Cancel.
    let (insert, cancel) = equation_buttons(ui, &t, parsed.is_ok() && !empty);
    // The keyboard stays in the TeX editor while the dialog is open: a click on the Display
    // style switch, a Recent row or Clear would otherwise take the focus (and the caret) with it.
    if !insert && !cancel && !r.has_focus() {
        r.request_focus();
        ui.ctx().request_repaint();
    }
    finish_equation(app, d, insert, cancel)
}
