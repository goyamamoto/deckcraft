use super::autocorrect::autocorrect;
use super::layout::{ctx_for, pos_param};
use super::{
    Align, Paragraph, Pos, Result, Run, RunKind, RunProps, Session, ShapeId, ShapeKind, TextBody, TextSel, Value, bad, body_of, bool_or, ed,
    edit_text, id_param, json, ok, resolve, shape_of,
};

pub(super) fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    if bool_or(p, "notes", false) {
        let end = s.doc()?.current_slide().map(|x| ed::end(&x.notes)).unwrap_or((0, 0));
        s.select(|_, sel| {
            sel.text = Some(TextSel { notes: true, anchor: end, caret: end, ..Default::default() });
            sel.shapes.clear();
            sel.cells = None;
        })?;
        return ok();
    }
    let id = match id_param(p, "id", "text.edit")? {
        Some(i) => i,
        None => s.doc()?.selection.shapes.first().copied().ok_or_else(|| bad("text.edit", "select a shape with text"))?,
    };
    let sh = shape_of(s, id)?;
    let cell = p.get("cell").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
    let body = match (&sh.kind, cell) {
        (ShapeKind::Table(t), Some((r, c))) => t.cell(r, c).map(|x| x.text.clone()).ok_or_else(|| bad("text.edit", "no such cell"))?,
        (ShapeKind::Table(_), None) => return Err(bad("text.edit", "say which `cell` of the table")),
        (ShapeKind::Shape | ShapeKind::Connector { .. }, _) => sh.text.clone().unwrap_or_default(),
        _ => return Err(bad("text.edit", "this object doesn't hold text")),
    };
    let at = pos_param(p, "at").unwrap_or_else(|| ed::end(&body));
    // A shape without a text body gets one on first edit.
    if sh.text.is_none() && cell.is_none() {
        s.edit(|doc, sel| {
            if let Some(list) = crate::shapes_mut(doc, sel)
                && let Some(x) = deckcraft_model::find_shape_mut(list, id)
            {
                x.text = Some(TextBody::default());
            }
            Ok(())
        })?;
    }
    s.select(|_, sel| {
        sel.shapes = vec![id];
        sel.text = Some(TextSel { shape: id, anchor: at, caret: at, cell, notes: false, line_end: None });
        sel.cells = None;
    })?;
    Ok(json!({"id": id, "at": [at.0, at.1]}))
}

pub(super) fn exit(s: &mut Session, _p: &Value) -> Result<Value> {
    s.select(|_, sel| {
        if let Some(t) = sel.text.take()
            && !t.notes
        {
            sel.shapes = vec![t.shape];
        }
    })?;
    ok()
}

/// Smart quotes and simple autocorrect applied to typed text.
fn typed(s: &Session, body: &TextBody, at: Pos, text: &str) -> String {
    if !s.prefs.smart_quotes || text.chars().count() != 1 {
        return text.to_string();
    }
    let prev = body.paragraphs.get(at.0).and_then(|p| p.text().chars().nth(at.1.wrapping_sub(1)));
    let opening = prev.is_none_or(|c| c.is_whitespace() || "([{".contains(c));
    match text {
        "\"" => (if opening { "“" } else { "”" }).to_string(),
        "'" => (if opening { "‘" } else { "’" }).to_string(),
        _ => text.to_string(),
    }
}

pub(crate) fn insert_text(s: &mut Session, text: &str, props: Option<RunProps>) -> Result<Value> {
    let inherited = inherited_aligns_for_edit(s);
    let lang = inherited_lang(s);
    let editing = s.east_asian_editing;
    let text = {
        let st = s.doc()?;
        match &st.selection.text {
            Some(t) => {
                let body = body_of(st, t).cloned().unwrap_or_default();
                typed(s, &body, t.ordered().0, text)
            }
            None => text.to_string(),
        }
    };
    let east_asian = east_asian_lang_of(&text, editing);
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let props = props.clone().or_else(|| if a != b { Some(ed::insert_props_at(body, at)) } else { unlinked_at_link_end(body, at) });
        let props = tagged_props(body, at, props, east_asian, lang.as_deref());
        let c = ed::insert(body, at, &text, props);
        t.anchor = c;
        t.caret = c;
        let last = body.paragraphs.len().saturating_sub(1);
        for pi in at.0..=c.0.min(last) {
            if let Some(para) = body.paragraphs.get_mut(pi) {
                promote_rtl_if_needed(para, &inherited);
            }
        }
        c
    })?;
    autocorrect(s, text.chars().count() == 1)?;
    // AutoCorrect can move the caret: a replaced word, or a list lead it removed.
    let pos = s.doc()?.selection.text.as_ref().map_or(pos, |t| t.caret);
    Ok(json!({"caret": [pos.0, pos.1]}))
}

/// Per outline level (0..=8), whether shape `id` on the current slide already resolves an
/// alignment for its paragraphs (its own list style, the layout's or the master's). All `false`
/// when the shape can't be resolved (notes, a shape that is gone).
pub(super) fn inherited_aligns(s: &Session, id: ShapeId) -> [bool; 9] {
    let mut out = [false; 9];
    let Ok(st) = s.doc() else { return out };
    let Some(sh) = st.shape(id) else { return out };
    let Some(ctx) = ctx_for(&st.doc, &st.selection) else { return out };
    for (lvl, set) in out.iter_mut().enumerate() {
        let para = deckcraft_model::text::Paragraph { level: lvl as u8, ..Default::default() };
        *set = resolve::para(&ctx, sh, &para).align.is_some();
    }
    out
}

/// The aligns [`inherited_aligns`] gives for the text being edited (none for notes).
fn inherited_aligns_for_edit(s: &Session) -> [bool; 9] {
    match s.doc().ok().and_then(|st| st.selection.text.clone()) {
        Some(t) if !t.notes => inherited_aligns(s, t.shape),
        _ => [false; 9],
    }
}

/// A paragraph with no direction of its own whose first strong character is right-to-left
/// (Arabic, Hebrew, Kurdish…) becomes right-to-left, and right-aligned when nothing (the
/// paragraph or what it inherits, `inherited`) sets its alignment.
pub(super) fn promote_rtl_if_needed(para: &mut deckcraft_model::text::Paragraph, inherited: &[bool; 9]) {
    if para.props.rtl.is_none() && deckcraft_model::bidi::first_strong_is_rtl(&para.text()) {
        para.props.rtl = Some(true);
        let inherits_align = inherited.get(usize::from(para.level)).copied().unwrap_or(false);
        if para.props.align.is_none() && !inherits_align {
            para.props.align = Some(Align::Right);
        }
    }
}

/// Typing right after a hyperlink doesn't extend it (as in PowerPoint): at the end of a linked
/// run, the props to type with are the run's without the link. `None` elsewhere.
pub(super) fn unlinked_at_link_end(body: &TextBody, at: Pos) -> Option<RunProps> {
    let mut props = ed::props_at(body, at);
    props.link.as_ref()?;
    let len = body.paragraphs.get(at.0).map_or(0, |p| p.text().chars().count());
    // props_at(c + 1) is the props of the character at c.
    if at.1 < len && ed::props_at(body, (at.0, at.1 + 1)).link == props.link {
        return None;
    }
    props.link = None;
    Some(props)
}

/// The East Asian language `text` is written in: Japanese when it has kana, Korean when it has
/// Hangul, Traditional Chinese when it has Bopomofo, else (Han, CJK punctuation, full-width forms)
/// the East Asian editing language `editing`. `None` when `text` has no East Asian character
/// ([`deckcraft_model::words::is_east_asian`]; the symbols PowerPoint draws with the East Asian
/// font, ™ ① ★ ■ ×, are not East Asian text), and for Han text without an editing language: the
/// characters don't tell Chinese from Japanese, so it is not guessed.
pub(crate) fn east_asian_lang_of(text: &str, editing: Option<deckcraft_fonts::CjkLocale>) -> Option<deckcraft_fonts::CjkLocale> {
    use deckcraft_fonts::{CjkLocale, script};
    let mut east_asian = false;
    let (mut hangul, mut bopomofo) = (false, false);
    for c in text.chars() {
        if !deckcraft_model::words::is_east_asian(c) {
            continue;
        }
        match script::script_of(c).as_ref() {
            Some(b"Hira" | b"Kana") => return Some(CjkLocale::Japanese),
            Some(b"Hang") => hangul = true,
            Some(b"Bopo") => bopomofo = true,
            _ => {}
        }
        east_asian = true;
    }
    if hangul {
        return Some(CjkLocale::Korean);
    }
    if bopomofo {
        return Some(CjkLocale::TraditionalChinese);
    }
    editing.filter(|_| east_asian)
}

/// The East Asian language of `text` an input method composes or commits, its preedits having
/// shown `shown` (kana on its way to kanji: Japanese; a pinyin preedit shows none):
/// [`east_asian_lang_of`], with `shown` naming Han text before the editing language does. A
/// symbol PowerPoint draws with the East Asian font that such a preedit is converted to (★ from
/// ほし) is in that language too: PowerPoint tags typed text with the input language.
pub(super) fn composed_lang_of(
    text: &str,
    shown: Option<deckcraft_fonts::CjkLocale>,
    editing: Option<deckcraft_fonts::CjkLocale>,
) -> Option<deckcraft_fonts::CjkLocale> {
    use deckcraft_fonts::{FontSlot, script};
    east_asian_lang_of(text, shown.or(editing)).or_else(|| shown.filter(|_| text.chars().any(|c| script::slot_of(c) == Some(FontSlot::EastAsian))))
}

/// The formatting text in East Asian language `lang` is typed with at a run formatted `props` (in
/// language `inherited` where it names none: its paragraph's, its list style's or the deck's
/// default). PowerPoint tags typed text with the input language: Japanese typed, committed by the
/// input method or pasted as plain text into a run whose language isn't East Asian (`en-US`, or
/// none) becomes a run of its own in Japanese, with the run's language as its alternative
/// (`lang="ja-JP" altLang="en-US"`). Text typed into a run already in an East Asian language
/// stays in it, so Latin typed with the Japanese input method stays in the `ja-JP` run, as in
/// PowerPoint. `None` when the formatting stays as it is.
pub(crate) fn east_asian_props(props: &RunProps, inherited: Option<&str>, lang: deckcraft_fonts::CjkLocale) -> Option<RunProps> {
    let current = props.lang.as_deref().or(inherited).map(str::trim).filter(|l| !l.is_empty());
    if current.and_then(deckcraft_fonts::CjkLocale::from_lang).is_some() {
        return None;
    }
    let mut p = props.clone();
    p.alt_lang = Some(current.unwrap_or("en-US").to_string());
    p.lang = Some(lang.lang().to_string());
    Some(p)
}

/// The language the text being edited inherits where its runs name none: from its paragraph,
/// its list styles or the deck's default (`a:defPPr`). `None` for notes and where nothing names
/// one.
pub(crate) fn inherited_lang(s: &Session) -> Option<String> {
    let st = s.doc().ok()?;
    let t = st.selection.text.as_ref().filter(|t| !t.notes)?;
    let sh = st.shape(t.shape)?;
    let ctx = ctx_for(&st.doc, &st.selection)?;
    let blank = Paragraph::default();
    let para = body_of(st, t).and_then(|b| b.paragraphs.get(t.ordered().0.0)).unwrap_or(&blank);
    resolve::run(&ctx, sh, para, &RunProps::default()).lang
}

/// The formatting text is inserted with at `at` (`props`, as [`deckcraft_model::edit::insert`]
/// takes it: `None` for the run at `at`), tagged with its East Asian language `lang`
/// ([`east_asian_props`]; `inherited` is the language of runs that name none).
pub(super) fn tagged_props(
    body: &TextBody,
    at: Pos,
    props: Option<RunProps>,
    lang: Option<deckcraft_fonts::CjkLocale>,
    inherited: Option<&str>,
) -> Option<RunProps> {
    let Some(lang) = lang else { return props };
    let base = props.clone().unwrap_or_else(|| ed::insert_props_at(body, at));
    east_asian_props(&base, inherited, lang).or(props)
}

pub(crate) fn paste_body(s: &mut Session, src: &TextBody) -> Result<Value> {
    let inherited = inherited_aligns_for_edit(s);
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let c = ed::insert_body(body, at, src);
        t.anchor = c;
        t.caret = c;
        let last = body.paragraphs.len().saturating_sub(1);
        for pi in at.0..=c.0.min(last) {
            if let Some(para) = body.paragraphs.get_mut(pi) {
                promote_rtl_if_needed(para, &inherited);
            }
        }
        c
    })?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

pub(crate) fn insert_field(s: &mut Session, field: &str, text: &str) -> Result<Value> {
    let field = field.to_string();
    let text = if text.is_empty() { "#".to_string() } else { text.to_string() };
    edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let props = ed::insert_props_at(body, at);
        let src = TextBody {
            paragraphs: vec![deckcraft_model::text::Paragraph {
                runs: vec![Run { text: text.clone(), props, kind: RunKind::Field { field: field.clone() } }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let c = ed::insert_body(body, at, &src);
        t.anchor = c;
        t.caret = c;
    })?;
    ok()
}

/// Insert an equation run at the insertion point (replacing the selection), in the formatting
/// there (at `size` when given) but in the math font, and leave the caret after it.
pub(crate) fn insert_math(s: &mut Session, mut run: Run, size: Option<f64>) -> Result<Value> {
    let pos = edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let mut props = ed::insert_props_at(body, at);
        props.font = Some(crate::cmd::equation::MATH_FONT.into());
        if size.is_some() {
            props.size = size;
        }
        props.link = None;
        props.hover = None;
        run.props = props;
        let src = TextBody { paragraphs: vec![Paragraph { runs: vec![run], ..Default::default() }], ..Default::default() };
        let c = ed::insert_body(body, at, &src);
        t.anchor = c;
        t.caret = c;
        c
    })?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

#[cfg(test)]
mod rtl_promote_tests {
    use deckcraft_model::text::{Align, Paragraph, Run};

    use super::promote_rtl_if_needed;

    fn para(text: &str) -> Paragraph {
        Paragraph { runs: vec![Run::new(text)], ..Default::default() }
    }

    #[test]
    fn right_aligns_only_when_no_alignment_is_set_or_inherited() {
        let mut p = para("مرحبا");
        promote_rtl_if_needed(&mut p, &[false; 9]);
        assert_eq!((p.props.rtl, p.props.align), (Some(true), Some(Align::Right)));
        let mut p = para("مرحبا");
        promote_rtl_if_needed(&mut p, &[true; 9]);
        assert_eq!((p.props.rtl, p.props.align), (Some(true), None));
        let mut p = para("Hello مرحبا");
        promote_rtl_if_needed(&mut p, &[false; 9]);
        assert_eq!((p.props.rtl, p.props.align), (None, None));
    }
}
