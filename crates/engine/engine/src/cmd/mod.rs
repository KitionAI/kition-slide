//! The command registry. Ids follow PowerPoint's ribbon and menu structure.

pub mod anchor;
pub mod animation;
pub mod arrange;
pub mod chart_spec;
pub mod design;
pub mod drawing;
pub mod edit;
pub mod file;
pub mod format;
pub mod ink;
pub mod insert;
pub mod inspect;
pub mod media;
pub mod merge;
pub mod presence;
pub mod review;
pub mod shape;
pub mod slide;
pub mod table;
pub mod text;
pub mod transition;

use kition_slide_geom::Xfrm;
use kition_slide_model::ShapeId;
use serde::Serialize;
use serde_json::Value;

use crate::{EngineError, Result, Session};

pub type Run = fn(&mut Session, &Value) -> Result<Value>;
pub type Enabled = fn(&Session) -> std::result::Result<(), String>;

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Where it lives: `["Home", "Font"]` (ribbon tab and group) or `["Arrange"]` (menu).
    pub menu: &'static [&'static str],
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub enabled: Enabled,
    pub run: Run,
    pub journal: bool,
    pub undoable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: Vec<&'static str>,
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

impl CommandSpec {
    pub fn info(&self, s: &Session) -> CommandInfo {
        let e = (self.enabled)(s);
        CommandInfo {
            id: self.id,
            label: self.label,
            menu: self.menu.to_vec(),
            shortcut: self.shortcut,
            params: self.params,
            enabled: e.is_ok(),
            disabled_reason: e.err(),
        }
    }
}

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}
pub fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no slide open".into())
}
pub fn has_slide(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| !d.doc.slides.is_empty() || d.selection.target != crate::Target::Slides) { Ok(()) } else { Err("no slides".into()) }
}
pub const NOTHING_SELECTED: &str = "nothing selected";
pub const SELECT_TWO: &str = "select two or more objects";
pub fn has_selection(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| !d.selection.shapes.is_empty()) { Ok(()) } else { Err(NOTHING_SELECTED.into()) }
}
pub fn has_two(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| d.selection.shapes.len() >= 2) { Ok(()) } else { Err(SELECT_TWO.into()) }
}
pub fn has_text(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| d.selection.text.is_some()) { Ok(()) } else { Err("no text insertion point".into()) }
}
/// Text editing or shapes with text selected (formatting commands apply to either).
pub fn has_text_or_shapes(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| d.selection.text.is_some() || !d.selection.shapes.is_empty()) {
        Ok(())
    } else {
        Err("select text or a shape".into())
    }
}
/// Whether the one the command runs for (the person, or an agent, E14a) has a step to undo.
pub fn can_undo(s: &Session) -> std::result::Result<(), String> {
    let agent = s.acts_as_agent();
    if s.active().is_some_and(|d| d.history.undo.iter().any(|e| e.agent == agent)) { Ok(()) } else { Err("nothing to undo".into()) }
}
pub fn can_redo(s: &Session) -> std::result::Result<(), String> {
    let agent = s.acts_as_agent();
    if s.active().is_some_and(|d| d.history.redo.iter().any(|e| e.agent == agent)) { Ok(()) } else { Err("nothing to redo".into()) }
}

macro_rules! cmd {
    ($id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $en:expr, $run:expr) => {
        $crate::cmd::CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: $en, run: $run, journal: true, undoable: true }
    };
    (query $id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $en:expr, $run:expr) => {
        $crate::cmd::CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: $en, run: $run, journal: false, undoable: false }
    };
    (noundo $id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $en:expr, $run:expr) => {
        $crate::cmd::CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: $en, run: $run, journal: true, undoable: false }
    };
}
pub(crate) use cmd;

pub fn command_specs() -> &'static [CommandSpec] {
    static SPECS: std::sync::OnceLock<Vec<CommandSpec>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(file::specs());
        v.extend(edit::specs());
        v.extend(slide::specs());
        v.extend(insert::specs());
        v.extend(shape::specs());
        v.extend(drawing::specs());
        v.extend(ink::specs());
        v.extend(arrange::specs());
        v.extend(merge::specs());
        v.extend(text::specs());
        v.extend(format::specs());
        v.extend(design::specs());
        v.extend(transition::specs());
        v.extend(animation::specs());
        v.extend(table::specs());
        v.extend(review::specs());
        v.extend(inspect::specs());
        v.extend(media::specs());
        v.extend(presence::specs());
        v.extend(anchor::specs());
        v
    })
}

pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    command_specs().iter().find(|c| c.id == id)
}

// ---------- param helpers ----------

pub(crate) fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}
pub(crate) fn f64_param(p: &Value, key: &str) -> Option<f64> {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite())
}
pub(crate) fn f64_or(p: &Value, key: &str, default: f64) -> f64 {
    f64_param(p, key).unwrap_or(default)
}
pub(crate) fn bool_param(p: &Value, key: &str) -> Option<bool> {
    p.get(key).and_then(Value::as_bool)
}
pub(crate) fn bool_or(p: &Value, key: &str, default: bool) -> bool {
    bool_param(p, key).unwrap_or(default)
}
pub(crate) fn str_param<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}
pub(crate) fn usize_param(p: &Value, key: &str) -> Option<usize> {
    p.get(key).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok())
}
/// A shape id as the schemas declare it: a number, or a string of one (`13` or `"13"`).
fn as_id(v: &Value) -> Option<ShapeId> {
    let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))?;
    u32::try_from(n).ok().map(ShapeId)
}
pub(crate) fn id_param(p: &Value, key: &str) -> Option<ShapeId> {
    p.get(key).and_then(as_id)
}
pub(crate) fn ids_param(p: &Value, key: &str) -> Option<Vec<ShapeId>> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(as_id).collect())
}
/// `id`, else the first selected shape. An `id` that is given but is not a shape id is an error:
/// falling back to the selection then would edit a shape nobody named.
pub(crate) fn id_or_selected(s: &mut Session, p: &Value, cmd: &str, none: &str) -> Result<ShapeId> {
    match p.get("id").filter(|v| !v.is_null()) {
        Some(v) => as_id(v).ok_or_else(|| bad(cmd, format!("`id` is {v}: a shape id is a number, as slide.inspect lists it"))),
        None => s.doc()?.selection.shapes.first().copied().ok_or_else(|| bad(cmd, none)),
    }
}
/// `[x, y, w, h]` in points.
pub(crate) fn rect_param(p: &Value, key: &str) -> Option<Xfrm> {
    let a = p.get(key)?.as_array()?;
    let f = |i: usize| a.get(i).and_then(Value::as_f64).filter(|v| v.is_finite());
    Some(Xfrm::new(f(0)?, f(1)?, f(2)?.abs(), f(3)?.abs()))
}
pub(crate) fn with_param(p: &Value, key: &str, v: Value) -> Value {
    let mut m = p.as_object().cloned().unwrap_or_default();
    m.insert(key.to_string(), v);
    Value::Object(m)
}

/// Targets: `ids` / `id` params, else the selection.
pub(crate) fn targets(s: &Session, p: &Value) -> Result<Vec<ShapeId>> {
    targets_for(s, p, "ids")
}

/// [`targets`] for command `cmd`. An id is a shape number (`24` or `"24"`) or the `KitDrawing` id
/// `drawing.insert` answers; one that names no shape here is an error, never the selection, so a
/// mistyped or foreign id cannot edit whatever happens to be selected.
pub(crate) fn targets_for(s: &Session, p: &Value, cmd: &str) -> Result<Vec<ShapeId>> {
    let given: Vec<&Value> = match (p.get("ids"), p.get("id")) {
        (Some(Value::Array(a)), _) if !a.is_empty() => a.iter().collect(),
        (_, Some(v)) if !v.is_null() => vec![v],
        _ => return Ok(s.doc()?.selection.shapes.clone()),
    };
    let st = s.doc()?;
    given
        .into_iter()
        .map(|v| {
            let text = match v {
                Value::String(t) => t.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(bad(cmd, "a shape id is a number or a string")),
            };
            drawing::local_id(st, &text)
                .ok_or_else(|| bad(cmd, format!("no shape `{text}` where this edits (inspect the slide for its shapes' ids)")))
        })
        .collect()
}

pub(crate) fn ok() -> Result<Value> {
    Ok(Value::Null)
}

/// Parse a colour param: `"#RRGGBB"`, `"accent1"`…, or `{"scheme":"accent2","lumMod":75000}`.
pub(crate) fn color_param(p: &Value, key: &str) -> Option<kition_slide_model::ColorRef> {
    let v = p.get(key)?;
    color_value(v)
}

pub(crate) fn color_value(v: &Value) -> Option<kition_slide_model::ColorRef> {
    use kition_slide_model::ColorRef;
    match v {
        Value::String(s) => {
            if let Some(slot) = kition_slide_color::SchemeSlot::from_xml(s) {
                return Some(ColorRef::scheme(slot));
            }
            if let Some(c) = kition_slide_color::Rgba::from_hex(s) {
                return Some(ColorRef::rgb(c));
            }
            kition_slide_color::preset(s).map(ColorRef::rgb)
        }
        Value::Object(o) => {
            let mut c = if let Some(s) = o.get("scheme").and_then(Value::as_str) {
                ColorRef::scheme(kition_slide_color::SchemeSlot::from_xml(s)?)
            } else {
                ColorRef::rgb(kition_slide_color::Rgba::from_hex(o.get("rgb").and_then(Value::as_str)?)?)
            };
            for (k, val) in o {
                if let Some(t) = kition_slide_color::ColorTransform::from_xml(k, val.as_i64().and_then(|x| i32::try_from(x).ok())) {
                    c.mods.push(t);
                }
            }
            Some(c)
        }
        _ => None,
    }
}

/// Look a shape up by id in the edited tree.
pub(crate) fn shape_of(s: &Session, id: ShapeId) -> Result<kition_slide_model::Shape> {
    s.doc()?.shape(id).cloned().ok_or_else(|| EngineError::Other(format!("no shape {id}")))
}

/// Effective box of a shape (placeholders inherit theirs). A table's box is at least as tall as its
/// rows are drawn (rows grow to fit their text), so handles and hit tests match what is shown.
pub fn xfrm_of(doc: &kition_slide_model::SlideFile, sel: &crate::Selection, shape: &kition_slide_model::Shape) -> Xfrm {
    let mut x = shape.xfrm.unwrap_or_else(|| ctx_xfrm(doc, sel, shape));
    if let kition_slide_model::ShapeKind::Table(t) = &shape.kind {
        let drawn: f64 = table_drawn_heights(doc, sel, t).iter().sum();
        if drawn.is_finite() && drawn > x.h {
            x.h = drawn;
        }
    }
    x
}

/// Shape `shape`'s box on the slide: a group member's box carried out of its groups' child spaces
/// (as slide.inspect lists it and its handles are drawn), any other shape's own box.
pub fn slide_xfrm_of(doc: &kition_slide_model::SlideFile, sel: &crate::Selection, shape: &kition_slide_model::Shape) -> Xfrm {
    let x = xfrm_of(doc, sel, shape);
    current_shapes(doc, sel)
        .and_then(|shapes| group_chain(shapes, shape.id, 0))
        .unwrap_or_default()
        .iter()
        .rev()
        .fold(x, |x, (group, child)| arrange::child_to_parent(group, child, &x))
}

/// Row heights of table `t` as drawn: a stored height is a minimum, rows grow to fit their text.
pub fn table_drawn_heights(doc: &kition_slide_model::SlideFile, sel: &crate::Selection, t: &kition_slide_model::Table) -> Vec<f64> {
    match text::ctx_for(doc, sel) {
        Some(ctx) => kition_slide_render::table_row_heights(&ctx, t, &kition_slide_text::NoFields),
        None => t.rows.iter().map(|r| r.height).collect(),
    }
}

fn ctx_xfrm(doc: &kition_slide_model::SlideFile, sel: &crate::Selection, shape: &kition_slide_model::Shape) -> Xfrm {
    use kition_slide_model::resolve::{self, Ctx};
    match sel.target {
        crate::Target::Slides => doc.slides.get(sel.slide).and_then(|s| Ctx::for_slide(doc, s)).map(|c| resolve::xfrm(&c, shape)),
        crate::Target::Master { master } => doc.masters.get(master).map(|m| resolve::xfrm(&Ctx::for_master(doc, m), shape)),
        crate::Target::Layout { master, layout } => {
            doc.masters.get(master).and_then(|m| m.layouts.get(layout).map(|l| resolve::xfrm(&Ctx::for_layout(doc, m, l), shape)))
        }
    }
    .unwrap_or_default()
}

/// Run `f` on each target shape (copy-on-write), materialising inherited boxes first.
pub(crate) fn edit_shapes(s: &mut Session, p: &Value, cmd: &str, f: impl Fn(&mut kition_slide_model::Shape) -> Result<()>) -> Result<Value> {
    let ids = targets_for(s, p, cmd)?;
    if ids.is_empty() {
        return Err(bad(cmd, "no shapes selected"));
    }
    s.edit(|doc, sel| {
        let boxes: Vec<(ShapeId, Xfrm)> = {
            let st_shapes: Vec<kition_slide_model::Shape> =
                ids.iter().filter_map(|id| current_shapes(doc, sel).and_then(|v| kition_slide_model::find_shape(v, *id)).cloned()).collect();
            st_shapes.iter().map(|sh| (sh.id, xfrm_of(doc, sel, sh))).collect()
        };
        // A group member's box is in its group's child space; the command sees it on the slide
        // (as slide.inspect lists it) and its result goes back into the group.
        let chains: Vec<(ShapeId, Vec<(Xfrm, Xfrm)>)> =
            ids.iter().map(|id| (*id, current_shapes(doc, sel).and_then(|v| group_chain(v, *id, 0)).unwrap_or_default())).collect();
        let shapes = crate::shapes_mut(doc, sel).ok_or_else(|| bad(cmd, "no slide"))?;
        let mut n = 0;
        for id in &ids {
            if let Some(sh) = kition_slide_model::find_shape_mut(shapes, *id) {
                if sh.xfrm.is_none() {
                    sh.xfrm = boxes.iter().find(|(i, _)| i == id).map(|(_, x)| *x);
                }
                let chain = chains.iter().find(|(i, _)| i == id).map(|(_, c)| c.as_slice()).unwrap_or_default();
                if chain.is_empty() {
                    f(sh)?;
                } else if let Some(x) = sh.xfrm {
                    let on_slide = chain.iter().rev().fold(x, |c, (g, ch)| arrange::child_to_parent(g, ch, &c));
                    sh.xfrm = Some(on_slide);
                    f(sh)?;
                    // Back into the group's space only when the command changed the box, so a
                    // box it left alone is kept exactly.
                    sh.xfrm = match sh.xfrm {
                        Some(now) if now == on_slide => Some(x),
                        other => other.map(|now| chain.iter().fold(now, |c, (g, ch)| arrange::parent_to_child(g, ch, &c))),
                    };
                }
                n += 1;
            }
        }
        if n == 0 {
            return Err(bad(cmd, "no such shape"));
        }
        Ok(Value::from(n))
    })
}

/// A box on the slide as the box shape `id` stores: in its group's child space for a group member
/// (the inverse of [`slide_xfrm_of`]), the box itself for a shape on the slide.
pub(crate) fn stored_xfrm_of(doc: &kition_slide_model::SlideFile, sel: &crate::Selection, id: ShapeId, x: Xfrm) -> Xfrm {
    current_shapes(doc, sel).and_then(|v| group_chain(v, id, 0)).unwrap_or_default().iter().fold(x, |c, (g, ch)| arrange::parent_to_child(g, ch, &c))
}

/// The transform from shape `sh`'s own space (`0..w`, `0..h` of its box) to the slide, as it is
/// drawn and its text is laid out: inside its groups for a group member.
pub fn shape_affine(st: &crate::DocState, sh: &kition_slide_model::Shape) -> kition_slide_geom::Affine {
    let x = xfrm_of(&st.doc, &st.selection, sh);
    if st.shapes().iter().any(|s| s.id == sh.id) {
        return x.affine();
    }
    text::ctx_for(&st.doc, &st.selection)
        .and_then(|ctx| text::text_shape_transform(&ctx, st.shapes(), sh.id, kition_slide_geom::Affine::IDENTITY, 0))
        .unwrap_or_else(|| x.affine())
}

/// The groups holding shape `id` on the edited slide, outermost first: empty for a shape on the
/// slide itself (or one that is not there).
pub fn groups_holding(shapes: &[kition_slide_model::Shape], id: ShapeId) -> Vec<ShapeId> {
    fn walk(shapes: &[kition_slide_model::Shape], id: ShapeId, depth: usize, out: &mut Vec<ShapeId>) -> bool {
        if depth > 32 {
            return false;
        }
        for sh in shapes {
            if sh.id == id {
                return true;
            }
            if let kition_slide_model::ShapeKind::Group { children, .. } = &sh.kind {
                out.push(sh.id);
                if walk(children, id, depth + 1, out) {
                    return true;
                }
                out.pop();
            }
        }
        false
    }
    let mut out = vec![];
    if walk(shapes, id, 0, &mut out) { out } else { vec![] }
}

/// The groups holding shape `id`, outermost first, as (group box, group child space); empty for a
/// shape on the slide itself, `None` when `id` is not among `shapes`.
fn group_chain(shapes: &[kition_slide_model::Shape], id: ShapeId, depth: usize) -> Option<Vec<(Xfrm, Xfrm)>> {
    if depth > 32 {
        return None;
    }
    for sh in shapes {
        if sh.id == id {
            return Some(vec![]);
        }
        if let kition_slide_model::ShapeKind::Group { children, child } = &sh.kind
            && let Some(mut inner) = group_chain(children, id, depth + 1)
        {
            inner.insert(0, (sh.xfrm.unwrap_or(*child), *child));
            return Some(inner);
        }
    }
    None
}

/// `f` on `sh`, or on each shape of a group (and of the groups in it): formatting a selected
/// group formats its shapes, as in PowerPoint; the group itself draws nothing of its own.
pub(crate) fn members(sh: &mut kition_slide_model::Shape, f: &dyn Fn(&mut kition_slide_model::Shape) -> Result<()>) -> Result<()> {
    fn rec(sh: &mut kition_slide_model::Shape, f: &dyn Fn(&mut kition_slide_model::Shape) -> Result<()>, depth: usize) -> Result<()> {
        match &mut sh.kind {
            kition_slide_model::ShapeKind::Group { children, .. } if depth < 32 => {
                for c in children.iter_mut() {
                    rec(c, f, depth + 1)?;
                }
                Ok(())
            }
            kition_slide_model::ShapeKind::Group { .. } => Ok(()),
            _ => f(sh),
        }
    }
    rec(sh, f, 0)
}

pub(crate) fn current_shapes<'a>(doc: &'a kition_slide_model::SlideFile, sel: &crate::Selection) -> Option<&'a Vec<kition_slide_model::Shape>> {
    match sel.target {
        crate::Target::Slides => doc.slides.get(sel.slide).map(|s| &s.shapes),
        crate::Target::Master { master } => doc.masters.get(master).map(|m| &m.shapes),
        crate::Target::Layout { master, layout } => doc.masters.get(master).and_then(|m| m.layouts.get(layout)).map(|l| &l.shapes),
    }
}

/// Base64 (standard alphabet) for binary payloads in JSON params and results.
pub fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= chunk.len() {
                out.push(A.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'A') as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.split_once("base64,").map(|(_, b)| b).unwrap_or(s);
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}
