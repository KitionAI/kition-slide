//! In-place text editing: entering/leaving text, typing, deleting, caret movement and selection.

use std::sync::Arc;

use kition_slide_model::edit::{self as ed, Pos};
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::{AutoFit, Hyperlink, Run, RunKind, RunProps, TextBody, TextDir};
use kition_slide_model::{ShapeId, ShapeKind, SlideFile};
use serde_json::{Value, json};

use super::*;
use crate::{DocState, Result, Selection, Session, Target, TextSel};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "text.edit", "Edit Text", [], Some("Enter"), "{id?, at?: [paragraph, char], all?: bool (select all of the text), end?: bool, cell?: [row, col], notes?: bool}", has_slide, edit),
        cmd!(noundo "text.exit", "Stop Editing Text", [], None, "{}", has_doc, exit),
        cmd!("text.insert", "Type Text", [], None, "{text} (\\n = new paragraph, \\u000b = line break)", has_text, insert),
        cmd!(
            "text.delete",
            "Delete Text",
            [],
            Some("Backspace"),
            "{dir?: backward|forward|wordBackward|wordForward|lineBackward|lineForward} (line: to the line's start or end)",
            has_text,
            delete
        ),
        cmd!(noundo "text.move", "Move Insertion Point", [], None, "{to: left|right|up|down|wordLeft|wordRight|lineStart|lineEnd|paraStart|paraEnd|start|end, extend?: bool}", has_text, move_caret),
        cmd!(noundo "text.select", "Select Text", [], None, "{anchor: [p, c], caret: [p, c]}", has_text, select),
        cmd!(noundo "text.selectAll", "Select All Text", [], None, "{}", has_text, select_all),
        cmd!(noundo "text.selectWord", "Select Word", [], None, "{at?: [p, c]}", has_text, select_word),
        cmd!(noundo "text.selectParagraph", "Select Paragraph", [], None, "{at?: [p, c]}", has_text, select_para),
        cmd!(
            "text.set",
            "Set Text",
            [],
            None,
            "{id?, text, cell?: [r, c]} — replaces the whole text, keeping the first run's formatting",
            has_slide,
            set
        ),
        cmd!(query "text.get", "Get Text", [], None, "{id?} → {text, paragraphs}", has_slide, get),
    ]
}

/// The text body a text selection edits.
pub fn body_of<'a>(st: &'a DocState, t: &TextSel) -> Option<&'a TextBody> {
    if t.notes {
        return st.current_slide().map(|s| &s.notes);
    }
    let sh = st.shape(t.shape)?;
    if let (Some((r, c)), ShapeKind::Table(tb)) = (t.cell, &sh.kind) {
        return tb.cell(r, c).map(|x| &x.text);
    }
    sh.text.as_ref()
}

pub(crate) fn body_mut<'a>(doc: &'a mut SlideFile, sel: &Selection, t: &TextSel) -> Option<&'a mut TextBody> {
    if t.notes {
        return doc.slides.get_mut(sel.slide).map(|s| &mut Arc::make_mut(s).notes);
    }
    let shapes = crate::shapes_mut(doc, sel)?;
    let sh = kition_slide_model::find_shape_mut(shapes, t.shape)?;
    if let Some((r, c)) = t.cell {
        if let ShapeKind::Table(tb) = &mut sh.kind {
            return tb.cell_mut(r, c).map(|x| &mut x.text);
        }
        return None;
    }
    Some(sh.text.get_or_insert_with(TextBody::default))
}

/// Edit the active text body and selection; refits auto-sizing text boxes afterwards.
pub(crate) fn edit_text<T>(s: &mut Session, f: impl FnOnce(&mut TextBody, &mut TextSel) -> T) -> Result<T> {
    let t0 = s.doc()?.selection.text.clone().ok_or_else(|| bad("text", "no text insertion point"))?;
    // Materialise an inherited placeholder box before editing (so autofit can change it).
    let r = s.edit(|doc, sel| {
        let mut t = t0.clone();
        let body = body_mut(doc, sel, &t).ok_or_else(|| bad("text", "the text being edited is gone"))?;
        let r = f(body, &mut t);
        sel.text = Some(t);
        Ok(r)
    })?;
    if !t0.notes && t0.cell.is_none() {
        fit_text_box(s, t0.shape)?;
    }
    Ok(r)
}

/// Grow/shrink an auto-sizing text box (Resize shape to fit text) to its text.
pub fn fit_text_box(s: &mut Session, id: ShapeId) -> Result<()> {
    let st = s.doc()?;
    let Some(nx) = fitted_box(&st.doc, &st.selection, id) else { return Ok(()) };
    // Change the box in place without an extra undo step (it is part of the edit).
    let st = s.doc_mut()?;
    let mut d = (*st.doc).clone();
    if let Some(list) = crate::shapes_mut(&mut d, &st.selection)
        && let Some(target) = kition_slide_model::find_shape_mut(list, id)
    {
        target.xfrm = Some(nx);
    }
    st.doc = Arc::new(d);
    st.revision += 1;
    Ok(())
}

/// The box of shape `id` (on the part `sel` edits in `doc`) fitted to its text, when it sizes
/// itself to its text (Resize shape to fit text) and that changes it.
pub(crate) fn fitted_box(doc: &SlideFile, sel: &Selection, id: ShapeId) -> Option<kition_slide_geom::Xfrm> {
    let sh = kition_slide_model::find_shape(crate::shapes_of(doc, sel), id)?;
    let body = sh.text.as_ref()?;
    let ctx = ctx_for(doc, sel)?;
    let bp = resolve::body(&ctx, sh);
    if bp.autofit != Some(AutoFit::Shape) {
        return None;
    }
    let x = xfrm_of(doc, sel, sh);
    let geo = kition_slide_render::shape_geometry(sh, x.w, x.h);
    let fields = kition_slide_text::NoFields;
    let mut nx = x;
    // Vertical text runs down the box: its lines are as long as the box is high, and the box
    // grows sideways to hold its columns (PowerPoint keeps the height and fits the width).
    if matches!(bp.vert, Some(TextDir::Vertical | TextDir::Vertical270 | TextDir::Stacked | TextDir::EaVertical)) {
        if bp.wrap == Some(false) {
            // fit_width measures along the lines, which run down the box.
            nx.h = kition_slide_text::fit_width(&ctx, sh, body, geo.text_rect, &fields).max(20.0);
        }
        let geo = kition_slide_render::shape_geometry(sh, nx.w, nx.h);
        let need = kition_slide_text::fit_height(&ctx, sh, body, geo.text_rect, &fields);
        let extra = (nx.w - geo.text_rect.width()).max(0.0);
        nx.w = (need + extra).max(8.0);
    } else {
        if bp.wrap == Some(false) {
            nx.w = kition_slide_text::fit_width(&ctx, sh, body, geo.text_rect, &fields).max(20.0);
        }
        let geo = kition_slide_render::shape_geometry(sh, nx.w, nx.h);
        let need = kition_slide_text::fit_height(&ctx, sh, body, geo.text_rect, &fields);
        let extra = (nx.h - geo.text_rect.height()).max(0.0);
        nx.h = (need + extra).max(8.0);
    }
    // The box as the file stores it (C8-slide-5): a grown height of 93.59999999999998 pt is
    // saved as 93.6 pt and must be that in the model too.
    nx.w = kition_slide_geom::stored::extent(nx.w);
    nx.h = kition_slide_geom::stored::extent(nx.h);
    if (nx.h - x.h).abs() < 0.01 && (nx.w - x.w).abs() < 0.01 && sh.xfrm.is_some() {
        return None;
    }
    Some(nx)
}

pub(crate) fn ctx_for<'a>(doc: &'a SlideFile, sel: &Selection) -> Option<Ctx<'a>> {
    match sel.target {
        Target::Slides => doc.slides.get(sel.slide).and_then(|s| Ctx::for_slide(doc, s)),
        Target::Master { master } => doc.masters.get(master).map(|m| Ctx::for_master(doc, m)),
        Target::Layout { master, layout } => doc.masters.get(master).and_then(|m| m.layouts.get(layout).map(|l| Ctx::for_layout(doc, m, l))),
    }
}

// Restrict lookup to the selected part: imported ids can repeat across slides.
pub(crate) fn text_shape_transform(
    ctx: &Ctx,
    shapes: &[kition_slide_model::Shape],
    id: ShapeId,
    parent: kition_slide_geom::Affine,
    depth: usize,
) -> Option<kition_slide_geom::Affine> {
    fn visit(
        ctx: &Ctx,
        shape: &kition_slide_model::Shape,
        id: ShapeId,
        x: kition_slide_geom::Xfrm,
        parent: kition_slide_geom::Affine,
        depth: usize,
    ) -> Option<kition_slide_geom::Affine> {
        if depth > 32 {
            return None;
        }
        if shape.id == id {
            return Some(parent * x.affine());
        }
        if let ShapeKind::Group { children, child } = &shape.kind {
            let rect = kition_slide_geom::Rect::new(child.x, child.y, child.x + child.w, child.y + child.h);
            let unscaled =
                (child.w.abs() < 1e-9 || (x.w / child.w - 1.0).abs() < 1e-9) && (child.h.abs() < 1e-9 || (x.h / child.h - 1.0).abs() < 1e-9);
            for child_shape in children {
                let cx = resolve::xfrm(ctx, child_shape);
                // Match renderer group normalization: scale the child's frame, not its text.
                let (cx, group) = if unscaled || matches!(child_shape.kind, ShapeKind::Ink { .. }) {
                    (cx, kition_slide_geom::group_child_affine(&x, rect))
                } else {
                    (kition_slide_geom::group_child_xfrm(&x, rect, &cx), x.affine())
                };
                if let Some(found) = visit(ctx, child_shape, id, cx, parent * group, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    for shape in shapes {
        if let Some(found) = visit(ctx, shape, id, resolve::xfrm(ctx, shape), parent, depth) {
            return Some(found);
        }
    }
    None
}

/// Lay out the text being edited (for vertical caret movement and hit testing).
pub fn layout_for(st: &DocState, t: &TextSel) -> Option<kition_slide_text::TextLayout> {
    if t.notes {
        return None;
    }
    if let Some(cell) = t.cell {
        return super::table::cell_layout(st, t.shape, cell);
    }
    let sh = st.shape(t.shape)?;
    let body = sh.text.as_ref()?;
    let ctx = ctx_for(&st.doc, &st.selection)?;
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let geo = kition_slide_render::shape_geometry(sh, x.w, x.h);
    Some(kition_slide_text::layout_with_transform(
        &ctx,
        sh,
        body,
        &kition_slide_text::Opts {
            rect: geo.text_rect,
            fields: &kition_slide_text::NoFields,
            prompt_color: None,
            no_shrink: false,
            empty_bullets: false,
        },
        text_shape_transform(&ctx, st.shapes(), t.shape, kition_slide_geom::Affine::IDENTITY, 0).unwrap_or_else(|| x.affine()),
    ))
}

fn pos_param(p: &Value, key: &str) -> Option<Pos> {
    let a = p.get(key)?.as_array()?;
    Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize))
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    if bool_or(p, "notes", false) {
        let end = s.doc()?.current_slide().map(|x| ed::end(&x.notes)).unwrap_or((0, 0));
        s.select(|_, sel| {
            sel.text = Some(TextSel { notes: true, anchor: end, caret: end, ..Default::default() });
            sel.shapes.clear();
        })?;
        return ok();
    }
    let id = id_or_selected(s, p, "text.edit", "select a shape with text")?;
    let sh = shape_of(s, id)?;
    let cell = p.get("cell").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
    let cell = if let ShapeKind::Table(table) = &sh.kind {
        let requested = cell.or_else(|| super::table::selected_range(s.doc().ok()?, id).map(|range| range.0)).unwrap_or((0, 0));
        Some(table.cell_owner(requested.0, requested.1).ok_or_else(|| bad("text.edit", "no such cell"))?)
    } else {
        cell
    };
    let body = match (&sh.kind, cell) {
        (ShapeKind::Table(t), Some((r, c))) => t.cell(r, c).map(|x| x.text.clone()).ok_or_else(|| bad("text.edit", "no such cell"))?,
        (ShapeKind::Table(_), None) => return Err(bad("text.edit", "say which `cell` of the table")),
        (ShapeKind::Shape | ShapeKind::Connector { .. }, _) => sh.text.clone().unwrap_or_default(),
        _ => return Err(bad("text.edit", "this object doesn't hold text")),
    };
    let end = ed::end(&body);
    let all = bool_or(p, "all", false);
    let at = pos_param(p, "at").filter(|_| !all).unwrap_or(end);
    // A shape without a text body gets one on first edit.
    if sh.text.is_none() && cell.is_none() {
        s.edit(|doc, sel| {
            if let Some(list) = crate::shapes_mut(doc, sel)
                && let Some(x) = kition_slide_model::find_shape_mut(list, id)
            {
                x.text = Some(TextBody::default());
            }
            Ok(())
        })?;
    }
    s.select(|_, sel| {
        sel.shapes = vec![id];
        sel.cells = None;
        sel.text = Some(TextSel { shape: id, anchor: if all { (0, 0) } else { at }, caret: at, cell, notes: false });
    })?;
    Ok(json!({"id": id, "at": [at.0, at.1]}))
}

fn exit(s: &mut Session, _p: &Value) -> Result<Value> {
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
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let props = props.clone().or_else(|| if a != b { Some(ed::props_at(body, at)) } else { unlinked_at_link_end(body, at) });
        let c = ed::insert(body, at, &text, props);
        t.anchor = c;
        t.caret = c;
        c
    })?;
    autocorrect(s)?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

/// Typing right after a hyperlink doesn't extend it (as in PowerPoint): at the end of a linked
/// run, the props to type with are the run's without the link. `None` elsewhere.
pub(crate) fn unlinked_at_link_end(body: &TextBody, at: Pos) -> Option<RunProps> {
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

const AUTOCORRECT: &[(&str, &str)] = &[
    ("teh", "the"),
    ("adn", "and"),
    ("recieve", "receive"),
    ("seperate", "separate"),
    ("occured", "occurred"),
    ("thier", "their"),
    ("wich", "which"),
    ("(c)", "©"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("->", "→"),
    ("<-", "←"),
    ("--", "–"),
];

/// After a space or punctuation, replace a just-typed autocorrect entry.
fn autocorrect(s: &mut Session) -> Result<()> {
    if !s.prefs.autocorrect {
        return Ok(());
    }
    let st = s.doc()?;
    let Some(t) = st.selection.text.clone() else { return Ok(()) };
    let Some(body) = body_of(st, &t) else { return Ok(()) };
    let line: Vec<char> = body.paragraphs.get(t.caret.0).map(|p| p.text().chars().collect()).unwrap_or_default();
    let end = t.caret.1;
    if end == 0 || !line.get(end - 1).is_some_and(|c| c.is_whitespace() || ".,;:!?".contains(*c)) {
        return Ok(());
    }
    let word_end = end - 1;
    let mut start = word_end;
    while start > 0 && line.get(start - 1).is_some_and(|c| !c.is_whitespace()) {
        start -= 1;
    }
    let word: String = line.get(start..word_end).map(|w| w.iter().collect()).unwrap_or_default();
    let Some((_, rep)) = AUTOCORRECT.iter().find(|(k, _)| k.eq_ignore_ascii_case(&word)) else { return Ok(()) };
    let rep = if word.chars().next().is_some_and(char::is_uppercase) { ed::change_case(rep, "sentence") } else { rep.to_string() };
    let delta = rep.chars().count() as isize - word.chars().count() as isize;
    edit_text(s, |body, selection| {
        ed::delete(body, (t.caret.0, start), (t.caret.0, word_end));
        ed::insert(body, (t.caret.0, start), &rep, None);
        let c = (t.caret.0, (end as isize + delta).max(0) as usize);
        selection.caret = c;
        selection.anchor = c;
    })?;
    Ok(())
}

pub(crate) fn paste_body(s: &mut Session, src: &TextBody) -> Result<Value> {
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let c = ed::insert_body(body, at, src);
        t.anchor = c;
        t.caret = c;
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
        let props = ed::props_at(body, at);
        let src = TextBody {
            paragraphs: vec![kition_slide_model::text::Paragraph {
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

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("text.insert", "missing `text`"))?.to_string();
    insert_text(s, &text, None)
}

pub(crate) fn delete_dir(s: &mut Session, dir: &str) -> Result<Value> {
    let dir = dir.to_string();
    let st = s.doc()?;
    let t0 = st.selection.text.clone().ok_or_else(|| bad("text.delete", "no text insertion point"))?;
    // The line's start or end as laid out (a soft-wrapped line's own, not the paragraph's).
    let line_end = match dir.as_str() {
        "lineBackward" | "lineForward" => layout_for(st, &t0).map(|l| {
            let (a, b) = l.line_bounds(kition_slide_text::Pos { para: t0.caret.0, ch: t0.caret.1 });
            let r = if dir == "lineBackward" { a } else { b };
            (r.para, r.ch)
        }),
        _ => None,
    };
    // An empty paragraph's bullet as it shows (inherited from the layout too): Backspace at its
    // start takes the bullet away first (walkthrough E6).
    let bulleted = t0.cell.is_none() && !t0.notes && shows_bullet(st, &t0);
    edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = if a != b {
            ed::delete(body, a, b)
        } else {
            let other = match dir.as_str() {
                "forward" => ed::char_move(body, a, true),
                "wordBackward" => ed::word_move(body, a, false),
                "wordForward" => ed::word_move(body, a, true),
                "lineBackward" | "lineForward" => {
                    let forward = dir == "lineForward";
                    let len = body.paragraphs.get(a.0).map_or(0, |p| p.char_len());
                    // The paragraph's own edge when the text has no layout; one character on
                    // when the caret is at the edge already (it joins the lines, as macOS does).
                    let edge = line_end.unwrap_or(if forward { (a.0, len) } else { (a.0, 0) });
                    if edge == a { ed::char_move(body, a, forward) } else { edge }
                }
                _ => {
                    if a.1 == 0
                        && bulleted
                        && let Some(para) = body.paragraphs.get_mut(a.0)
                        && para.level == 0
                        && para.char_len() == 0
                    {
                        para.props.bullet = Some(kition_slide_model::text::Bullet::None);
                        t.anchor = a;
                        t.caret = a;
                        return;
                    }
                    // Backspace at the start of an indented/bulleted paragraph outdents first.
                    if a.1 == 0
                        && let Some(para) = body.paragraphs.get_mut(a.0)
                        && para.level > 0
                    {
                        para.level -= 1;
                        t.anchor = a;
                        t.caret = a;
                        return;
                    }
                    ed::char_move(body, a, false)
                }
            };
            ed::delete(body, a, other)
        };
        t.anchor = at;
        t.caret = at;
    })?;
    ok()
}

/// Whether the paragraph with the caret of `t` shows a bullet or a number (its own or the one it
/// inherits).
fn shows_bullet(st: &DocState, t: &TextSel) -> bool {
    let Some(sh) = st.shape(t.shape) else { return false };
    let Some(para) = sh.text.as_ref().and_then(|b| b.paragraphs.get(t.caret.0)) else { return false };
    let Some(ctx) = ctx_for(&st.doc, &st.selection) else { return false };
    !matches!(resolve::para(&ctx, sh, para).bullet, None | Some(kition_slide_model::text::Bullet::None))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    delete_dir(s, str_param(p, "dir").unwrap_or("backward"))
}

fn move_caret(s: &mut Session, p: &Value) -> Result<Value> {
    let to = str_param(p, "to").unwrap_or("right").to_string();
    let extend = bool_or(p, "extend", false);
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.move", "not editing text"))?;
    let body = body_of(st, &t).cloned().unwrap_or_default();
    let layout = layout_for(st, &t);
    let lp = |c: Pos| kition_slide_text::Pos { para: c.0, ch: c.1 };
    let caret = t.caret;
    let collapse_to = |left: bool| if left { t.ordered().0 } else { t.ordered().1 };
    let new = match to.as_str() {
        "left" if t.is_range() && !extend => collapse_to(true),
        "right" if t.is_range() && !extend => collapse_to(false),
        "left" => ed::char_move(&body, caret, false),
        "right" => ed::char_move(&body, caret, true),
        "wordLeft" => ed::word_move(&body, caret, false),
        "wordRight" => ed::word_move(&body, caret, true),
        "paraStart" => (caret.0, 0),
        "paraEnd" => (caret.0, body.paragraphs.get(caret.0).map(|x| x.char_len()).unwrap_or(0)),
        "start" => (0, 0),
        "end" => ed::end(&body),
        "up" | "down" | "lineStart" | "lineEnd" => match &layout {
            Some(l) => {
                let x = l.caret(lp(caret)).map(|c| c.0).unwrap_or(0.0);
                let r = match to.as_str() {
                    "up" => l.vertical(lp(caret), false, x),
                    "down" => l.vertical(lp(caret), true, x),
                    "lineStart" => l.line_bounds(lp(caret)).0,
                    _ => l.line_bounds(lp(caret)).1,
                };
                (r.para, r.ch)
            }
            None => match to.as_str() {
                "up" => (caret.0.saturating_sub(1), caret.1),
                "down" => ((caret.0 + 1).min(body.paragraphs.len().saturating_sub(1)), caret.1),
                "lineStart" => (caret.0, 0),
                _ => (caret.0, body.paragraphs.get(caret.0).map(|x| x.char_len()).unwrap_or(0)),
            },
        },
        other => return Err(bad("text.move", format!("unknown direction `{other}`"))),
    };
    let new = (new.0.min(body.paragraphs.len().saturating_sub(1)), new.1.min(body.paragraphs.get(new.0).map(|x| x.char_len()).unwrap_or(0)));
    s.select(|_, sel| {
        if let Some(tt) = sel.text.as_mut() {
            tt.caret = new;
            if !extend {
                tt.anchor = new;
            }
        }
    })?;
    Ok(json!({"caret": [new.0, new.1]}))
}

fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let a = pos_param(p, "anchor").ok_or_else(|| bad("text.select", "missing `anchor`"))?;
    let c = pos_param(p, "caret").unwrap_or(a);
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = a;
            t.caret = c;
        }
    })?;
    ok()
}

fn select_all(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectAll", "not editing text"))?;
    let end = body_of(st, &t).map(ed::end).unwrap_or((0, 0));
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = (0, 0);
            t.caret = end;
        }
    })?;
    ok()
}

fn select_word(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectWord", "not editing text"))?;
    let at = pos_param(p, "at").unwrap_or(t.caret);
    let body = body_of(st, &t).cloned().unwrap_or_default();
    let (a, b) = ed::word_at(&body, at);
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = a;
            t.caret = b;
        }
    })?;
    ok()
}

fn select_para(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectParagraph", "not editing text"))?;
    let at = pos_param(p, "at").unwrap_or(t.caret);
    let n = body_of(st, &t).and_then(|b| b.paragraphs.get(at.0)).map(|x| x.char_len()).unwrap_or(0);
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = (at.0, 0);
            t.caret = (at.0, n);
        }
    })?;
    ok()
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("text.set", "missing `text`"))?.to_string();
    let id = id_or_selected(s, p, "text.set", "missing `id` (or select a shape)")?;
    let cell = p.get("cell").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
    edit_shapes(s, &json!({"id": id}), "text.set", |sh| {
        let target = match (&mut sh.kind, cell) {
            (ShapeKind::Table(t), Some((r, c))) => t.cell_owner(r, c).and_then(|(r, c)| t.cell_mut(r, c)).map(|x| &mut x.text),
            (ShapeKind::Table(_), None) => None,
            (ShapeKind::Shape, _) => Some(sh.text.get_or_insert_with(TextBody::default)),
            // A file keeps text only in shapes and table cells: text set on a group, picture or
            // connector was accepted and then lost on saving.
            (kind, _) => {
                let what = match kind {
                    ShapeKind::Group { .. } => "a group (set the text of a shape inside it)",
                    ShapeKind::Picture { .. } => "a picture",
                    ShapeKind::Connector { .. } => "a connector",
                    ShapeKind::Chart(_) => "a chart",
                    _ => "an object that holds no text",
                };
                return Err(bad("text.set", format!("shape {id} is {what}; only shapes and table cells hold text")));
            }
        };
        let Some(body) = target else { return Err(bad("text.set", "no text here (tables need `cell`)")) };
        let keep = body.paragraphs.first().and_then(|p| p.runs.first()).map(|r| r.props.clone()).unwrap_or_default();
        let levels: Vec<(u8, kition_slide_model::text::ParaProps)> = body.paragraphs.iter().map(|p| (p.level, p.props.clone())).collect();
        let mut nb = TextBody::from_text(&text);
        for (i, para) in nb.paragraphs.iter_mut().enumerate() {
            // Leading tabs set the outline level.
            let tabs = para.runs.first().map(|r| r.text.chars().take_while(|c| *c == '\t').count()).unwrap_or(0);
            if tabs > 0
                && let Some(r) = para.runs.first_mut()
            {
                r.text = r.text.trim_start_matches('\t').to_string();
                para.level = tabs.min(8) as u8;
            } else if let Some((lv, pp)) = levels.get(i) {
                para.level = *lv;
                para.props = pp.clone();
            }
            for r in &mut para.runs {
                r.props = keep.clone();
            }
        }
        body.paragraphs = nb.paragraphs;
        Ok(())
    })?;
    fit_text_box(s, id)?;
    ok()
}

fn get(s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("id").is_some_and(|v| !v.is_null()) && id_param(p, "id").is_none() {
        return Err(bad("text.get", format!("`id` is {}: a shape id is a number, as slide.inspect lists it", p["id"])));
    }
    let st = s.doc()?;
    let id = match id_param(p, "id") {
        Some(i) => i,
        None => match &st.selection.text {
            Some(t) => t.shape,
            None => st.selection.shapes.first().copied().ok_or_else(|| bad("text.get", "missing `id`"))?,
        },
    };
    let sh = st.shape(id).ok_or_else(|| bad("text.get", "no such shape"))?;
    let t = sh.text.clone().unwrap_or_default();
    Ok(json!({"text": t.text(), "paragraphs": t.paragraphs.iter().map(|p| json!({"level": p.level, "text": p.text()})).collect::<Vec<_>>()}))
}

/// Apply run formatting to the text selection (or, when not editing, to all text of the
/// selected shapes). A caret without a range formats the word it's in (PowerPoint behaviour),
/// or sets the formatting for what is typed next at a word boundary.
pub(crate) fn format_run(s: &mut Session, f: &(dyn Fn(&mut RunProps) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    if let Some(t) = st.selection.text.clone() {
        let (a, b) = format_range(st, &t);
        let collapsed = a == b;
        edit_text(s, |body, _| {
            if collapsed {
                // Next typed text: put the props on an empty paragraph's end mark or keep pending.
                ed::format(body, a, b, f);
            } else {
                ed::format(body, a, b, f);
            }
        })?;
        if collapsed {
            // Remember as pending typing props by inserting nothing: handled via end mark for
            // empty paragraphs; mid-paragraph carets keep surrounding formatting.
        }
        return ok();
    }
    let ids = st.selection.shapes.clone();
    if ids.is_empty() {
        return Err(bad("format", "select text or a shape"));
    }
    let selected_cells = st.selection.cells;
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        members(sh, &|sh| {
            match &mut sh.kind {
                ShapeKind::Table(t) => {
                    for (r, row) in t.rows.iter_mut().enumerate() {
                        for (col, c) in row.cells.iter_mut().enumerate() {
                            if selected_cells.is_some_and(|(id, a, b)| {
                                id == sh.id
                                    && (r > a.0.max(b.0)
                                        || r.saturating_add(c.row_span.max(1) as usize) <= a.0.min(b.0)
                                        || col > a.1.max(b.1)
                                        || col.saturating_add(c.grid_span.max(1) as usize) <= a.1.min(b.1))
                            }) {
                                continue;
                            }
                            ed::format_all(&mut c.text, f);
                        }
                    }
                }
                // A chart's text (title, legend, axes, data labels) takes the change, as when
                // the whole chart is selected in PowerPoint.
                ShapeKind::Chart(c) => {
                    let mut rp = RunProps::default();
                    f(&mut rp);
                    let change = kition_slide_model::chart::ChartTextChange {
                        font: rp.font,
                        size: rp.size,
                        bold: rp.bold,
                        italic: rp.italic,
                        color: match rp.fill {
                            Some(kition_slide_model::Fill::Solid { color }) => Some(color),
                            _ => None,
                        },
                    };
                    if change.is_empty() {
                        return Err(bad("format", "a chart's text takes font, size, bold, italic and color"));
                    }
                    c.format_text(&change);
                }
                _ => {
                    let body = sh.text.get_or_insert_with(TextBody::default);
                    ed::format_all(body, f);
                }
            }
            Ok(())
        })
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}

/// What `format_run` formats while editing: the selection, or for a bare caret the word it's in
/// (without its trailing space). Empty when the caret is at a word boundary.
fn format_range(st: &DocState, t: &TextSel) -> (Pos, Pos) {
    let body = body_of(st, t).cloned().unwrap_or_default();
    let (mut a, mut b) = t.ordered();
    if a == b {
        let (wa, wb) = ed::word_at(&body, a);
        let inside = wa.1 < a.1 && a.1 < wb.1;
        if inside {
            a = wa;
            b = wb;
            // Don't include the trailing space.
            let line: Vec<char> = body.paragraphs.get(b.0).map(|p| p.text().chars().collect()).unwrap_or_default();
            while b.1 > a.1 && line.get(b.1 - 1) == Some(&' ') {
                b.1 -= 1;
            }
        }
    }
    (a, b)
}

/// Link the text being edited: the selection or the word at the caret, as `format_run` would.
/// With nothing to link (caret at a word boundary, empty paragraph) `display` is inserted at the
/// caret as linked text in the surrounding formatting, as PowerPoint's Insert Hyperlink does.
pub(crate) fn link_text(s: &mut Session, link: Hyperlink, display: &str) -> Result<Value> {
    let st = s.doc()?;
    let Some(t) = st.selection.text.clone() else { return Err(bad("link.insert", "not editing text")) };
    let (a, b) = format_range(st, &t);
    if a != b {
        return format_run(s, &move |rp| rp.link = Some(link.clone()));
    }
    if display.is_empty() {
        return Err(bad("link.insert", "nothing to link: select text or give `text`"));
    }
    let pos = edit_text(s, |body, t| {
        let mut props = ed::props_at(body, a);
        props.link = Some(link);
        let c = ed::insert(body, a, display, Some(props));
        t.anchor = c;
        t.caret = c;
        c
    })?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

/// Apply paragraph changes to the paragraphs touched by the selection (or all paragraphs of the
/// selected shapes).
pub(crate) fn format_para(s: &mut Session, f: &(dyn Fn(&mut kition_slide_model::text::Paragraph) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    if let Some(t) = st.selection.text.clone() {
        let (a, b) = t.ordered();
        edit_text(s, |body, _| {
            if body.paragraphs.is_empty() {
                body.paragraphs.push(Default::default());
            }
            for pi in a.0..=b.0 {
                if let Some(p) = body.paragraphs.get_mut(pi) {
                    f(p);
                }
            }
        })?;
        return ok();
    }
    let ids = st.selection.shapes.clone();
    if ids.is_empty() {
        return Err(bad("format", "select text or a shape"));
    }
    let selected_cells = st.selection.cells;
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        members(sh, &|sh| {
            match &mut sh.kind {
                ShapeKind::Table(t) => {
                    for (r, row) in t.rows.iter_mut().enumerate() {
                        for (col, c) in row.cells.iter_mut().enumerate() {
                            if selected_cells.is_some_and(|(id, a, b)| {
                                id == sh.id
                                    && (r > a.0.max(b.0)
                                        || r.saturating_add(c.row_span.max(1) as usize) <= a.0.min(b.0)
                                        || col > a.1.max(b.1)
                                        || col.saturating_add(c.grid_span.max(1) as usize) <= a.1.min(b.1))
                            }) {
                                continue;
                            }
                            c.text.paragraphs.iter_mut().for_each(f);
                        }
                    }
                }
                _ => {
                    let body = sh.text.get_or_insert_with(TextBody::default);
                    if body.paragraphs.is_empty() {
                        body.paragraphs.push(Default::default());
                    }
                    body.paragraphs.iter_mut().for_each(f);
                }
            }
            Ok(())
        })
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}

/// Apply body (text box) changes to the edited or selected shapes.
pub(crate) fn format_body(s: &mut Session, f: &(dyn Fn(&mut kition_slide_model::text::BodyProps) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    let ids: Vec<ShapeId> = match &st.selection.text {
        Some(t) if !t.notes => vec![t.shape],
        _ => st.selection.shapes.clone(),
    };
    if ids.is_empty() {
        return Err(bad("format", "select a shape"));
    }
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        members(sh, &|sh| {
            f(&mut sh.text.get_or_insert_with(TextBody::default).body);
            Ok(())
        })
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}

#[cfg(test)]
mod vertical_layout_tests {
    use super::*;
    use kition_slide_model::text::TextDir;
    use kition_slide_model::{Shape, Xfrm};

    #[test]
    fn grouped_vertical_caret_layout_matches_placed_text_fallback() {
        let mut session = Session::with_new();
        let state = session.doc_mut().unwrap();
        let mut doc = (*state.doc).clone();
        let mut body = TextBody::from_text(&char::from_u32(0x4e00).unwrap().to_string());
        body.body.vert = Some(TextDir::EaVertical);
        body.body.autofit = Some(AutoFit::None);
        let id = ShapeId(77);
        let child = Shape { id, xfrm: Some(Xfrm::new(0.0, 0.0, 100.0, 200.0)), text: Some(body), ..Default::default() };
        let mut xfrm = Xfrm::new(100.0, 100.0, 100.0, 200.0);
        xfrm.rot = 45.0;
        xfrm.flip_h = true;
        let group = Shape {
            id: ShapeId(78),
            xfrm: Some(xfrm),
            kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.0, 0.0, 100.0, 200.0) },
            ..Default::default()
        };
        Arc::make_mut(&mut doc.slides[0]).shapes = vec![group];
        state.doc = Arc::new(doc);
        let edited = layout_for(state, &TextSel { shape: id, ..Default::default() }).unwrap();
        let placed = kition_slide_render::place_slide(&state.doc, 0);
        let drawn = &placed.texts[0].layout;
        assert!(edited.runs.iter().all(|r| !r.upright));
        assert!(drawn.runs.iter().all(|r| !r.upright));
        assert_eq!(edited.inner, drawn.inner);
        assert_eq!(edited.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>(), drawn.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>());
        assert_eq!(edited.caret(kition_slide_text::Pos { para: 0, ch: 1 }), drawn.caret(kition_slide_text::Pos { para: 0, ch: 1 }));
    }

    #[test]
    fn nested_scaled_groups_use_the_rendered_text_orientation() {
        let mut session = Session::with_new();
        let state = session.doc_mut().unwrap();
        let mut doc = (*state.doc).clone();
        let mut body = TextBody::from_text(&char::from_u32(0x4e00).unwrap().to_string());
        body.body.vert = Some(TextDir::EaVertical);
        body.body.autofit = Some(AutoFit::None);
        let id = ShapeId(77);
        let child = Shape { id, xfrm: Some(Xfrm::new(0.0, 0.0, 100.0, 200.0)), text: Some(body), ..Default::default() };
        let mut inner_x = Xfrm::new(0.0, 0.0, 100.0, 200.0);
        inner_x.rot = 315.0;
        let inner = Shape {
            id: ShapeId(78),
            xfrm: Some(inner_x),
            kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.0, 0.0, 100.0, 200.0) },
            ..Default::default()
        };
        let mut outer_x = Xfrm::new(100.0, 100.0, 200.0, 200.0);
        outer_x.rot = 45.0;
        let outer = Shape {
            id: ShapeId(79),
            xfrm: Some(outer_x),
            kind: ShapeKind::Group { children: vec![inner], child: Xfrm::new(0.0, 0.0, 100.0, 200.0) },
            ..Default::default()
        };
        Arc::make_mut(&mut doc.slides[0]).shapes = vec![outer];
        state.doc = Arc::new(doc);
        let edited = layout_for(state, &TextSel { shape: id, ..Default::default() }).unwrap();
        let placed = kition_slide_render::place_slide(&state.doc, 0);
        assert!(placed.texts[0].layout.runs.iter().any(|r| r.upright));
        assert!(edited.runs.iter().any(|r| r.upright), "group scale is baked into frames before deciding orientation");
    }
}
