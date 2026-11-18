//! `drawing.*`: the Kit Harness Protocol's `KitDrawing 1.0` commands over Slide's shapes, through
//! the shared drawing model (`kition-office-core`). A shape is converted to a `DrawingObject`, the
//! operation applied to that, and only the members that changed are written back, so everything
//! the shared model does not describe (effects, style references, bullets, kept XML) stays.

use kition_office_core::model::{self as dm, DrawingObject};
use kition_office_core::ops::{DrawingError, DrawingOp, FrameChange};
use kition_slide_color::SchemeSlot;
use kition_slide_model::text::{Align, Anchor, AutoFit, Paragraph, Run, RunProps, TextBody};
use kition_slide_model::{Fill, Geom, Line, Shape, ShapeId, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "drawing.describe", "Describe Drawing Objects", [], None, "{ids?, id? (one shape instead of ids), onSlide?: int (the slide the ids are on, from 1)} (default: the selection, else every shape on the slide)", has_slide, describe),
        cmd!(
            "drawing.insert",
            "Insert Drawing Object",
            [],
            None,
            "{kind?: shape|textBox|chart, geometry?: {preset, adjust?}, frame?: {x,y,w,h,rot?,flipH?,flipV?}, placement?: {kind: slide}, fill?, stroke?, text?, paragraphs? (instead of text), body?, warp?, chart?: {kind?, title?, categories?, series?: [{name, values, color?}], legend?, dataLabels?, gridlines?, xTitle?, yTitle?}}",
            has_slide,
            |s, p| {
                let answer = with_font_notes(insert(s, p), p);
                with_design(s, answer, None)
            }
        ),
        cmd!("drawing.delete", "Delete Drawing Objects", [], None, "{ids?, id? (one object instead of ids)}", has_slide, |s, p| apply(
            s,
            "drawing.delete",
            p
        )),
        cmd!(
            "drawing.transform",
            "Transform Drawing Objects",
            [],
            None,
            "{ids?, id? (one object instead of ids), frame?, dx?, dy?, rotateBy?, flipH?, flipV?} (frame sets each object's frame; dx, dy, rotateBy, flipH and flipV change it)",
            has_slide,
            |s, p| { apply(s, "drawing.transform", p) }
        ),
        cmd!(
            "drawing.format",
            "Format Drawing Objects",
            [],
            None,
            "{ids?, id? (one object instead of ids), fill?, stroke?, geometry?, effects?: {shadow?, glow?}}",
            has_slide,
            |s, p| apply(s, "drawing.format", p)
        ),
        cmd!(
            "drawing.text",
            "Set Drawing Object Text",
            [],
            None,
            "{id, text? | paragraphs?, body?: {anchor, insets, wrap, autofit}, warp?: preset | null}",
            has_slide,
            |s, p| {
                let answer = with_font_notes(apply(s, "drawing.text", p), p);
                with_design(s, answer, p.get("id"))
            }
        ),
        cmd!("drawing.arrange", "Arrange Drawing Objects", [], None, "{ids, order: front|back|forward|backward}", has_slide, arrange),
        cmd!("drawing.reconnect", "Reconnect Connector", [], None, "{id, end: start|end, to?: id, site?: n, point?: [x, y]}", has_slide, |s, p| {
            apply(s, "drawing.reconnect", p)
        }),
        cmd!(
            "drawing.connect",
            "Connect Drawing Objects",
            [],
            None,
            "{from, to, fromSite?, toSite?, geometry?: {preset: straightConnector1|bentConnector3|curvedConnector3}}",
            has_slide,
            connect
        ),
        cmd!("drawing.group", "Group Drawing Objects", [], None, "{ids} (two or more objects on the slide, not in a group)", has_slide, group),
        cmd!("drawing.ungroup", "Ungroup Drawing Objects", [], None, "{ids} (groups)", has_slide, ungroup),
        cmd!(
            "drawing.chart",
            "Change Chart",
            [],
            None,
            "{id, ids? (one chart, instead of id), kind?, title?: text|null, legend?: top|bottom|left|right|topRight|null, dataLabels?, dataLabelPosition?, gridlines?, xTitle?, yTitle?, xAxis?: {min?, max?, major?, numberFormat?, hidden?}, yAxis?: {min?, max?, major?, numberFormat?, hidden?}, categories?, series?: [{name, values, color?}], seriesFormat?: [{index, color?, lineWidth?, marker?, smooth?, trendline?, errorBars?}]}",
            has_slide,
            |s, p| apply(s, "drawing.chart", p)
        ),
    ]
}

fn fail(cmd: &str, e: DrawingError) -> crate::EngineError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    bad(cmd, format!("{}{property}: {}", e.code, e.message))
}

/// The KDM object of what the shapes being edited are on (the slide, layout or master).
fn owner_object(st: &crate::DocState) -> Option<kdm_model::kition_kit::kdm::ObjectId> {
    let ids = st.kdm()?.model().ids();
    match st.selection.target {
        crate::Target::Slides => ids.slides.object(st.doc.slides.get(st.selection.slide)?.id).cloned(),
        crate::Target::Master { master } => ids.masters.object(st.doc.masters.get(master)?.id).cloned(),
        crate::Target::Layout { master, layout } => ids.layouts.object(st.doc.masters.get(master)?.layouts.get(layout)?.id).cloned(),
    }
}

/// A shape's `KitDrawing 1.0` id (DR2): its object in the replica, the same on every replica of
/// the slide file; this replica's shape number while the slide file has no replica yet.
pub fn object_id(st: &crate::DocState, id: ShapeId) -> String {
    if let Some(shape) = st.shape(id).filter(|shape| !shape.identity.is_empty()) {
        return super::inspect::shape_tool_id(shape);
    }
    owner_object(st).and_then(|owner| st.kdm()?.model().ids().shape(&owner, id).map(|o| o.as_str().to_owned())).unwrap_or_else(|| id.0.to_string())
}

/// The shape a `KitDrawing 1.0` id names on the slide being edited: its replica object's id, or
/// (as Slide's other commands take it) its number here.
pub(crate) fn local_id(st: &crate::DocState, text: &str) -> Option<ShapeId> {
    if let Some(identity) = text.strip_prefix("shape:").and_then(kition_slide_model::identity::canonical) {
        return all_shapes(st).into_iter().find(|shape| shape.identity == identity).map(|shape| shape.id);
    }
    if let Some(owner) = owner_object(st)
        && let Some(kdm) = st.kdm()
    {
        let ids = kdm.model().ids();
        if let Some(sh) = all_shapes(st).into_iter().find(|sh| ids.shape(&owner, sh.id).is_some_and(|o| o.as_str() == text)) {
            return Some(sh.id);
        }
    }
    text.parse::<u32>().ok().map(ShapeId).filter(|id| st.shape(*id).is_some())
}

/// `ids` (or `id`) as KitDrawing ids: strings, or numbers as Slide's other commands take them.
fn object_ids(st: &crate::DocState, p: &Value) -> std::result::Result<Option<Vec<ShapeId>>, DrawingError> {
    let list: Vec<&Value> = match (p.get("ids"), p.get("id")) {
        (Some(Value::Array(a)), _) => a.iter().collect(),
        (None, Some(v)) => vec![v],
        (None, None) => return Ok(None),
        _ => return Err(DrawingError::invalid("`ids` is a list of ids")),
    };
    list.into_iter()
        .map(|v| {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(DrawingError::invalid("an id is a string")),
            };
            local_id(st, &text).ok_or_else(|| DrawingError::not_found(&text))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map(Some)
}

/// A shape `KitDrawing` describes: an ordinary shape or text box, or a chart (1.1).
fn is_drawing(sh: &Shape) -> bool {
    matches!(sh.kind, ShapeKind::Shape | ShapeKind::Chart(_) | ShapeKind::Ink { .. } | ShapeKind::Group { .. } | ShapeKind::Connector { .. })
}

/// The groups that hold shape `id`, outermost first (empty for a shape on the slide itself).
fn groups_of(st: &crate::DocState, id: ShapeId) -> Vec<&Shape> {
    fn find<'a>(list: &'a [Shape], id: ShapeId, path: &mut Vec<&'a Shape>) -> bool {
        for sh in list {
            if sh.id == id {
                return true;
            }
            if let ShapeKind::Group { children, .. } = &sh.kind {
                path.push(sh);
                if find(children, id, path) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    let mut path = Vec::new();
    find(st.shapes(), id, &mut path);
    path
}

/// Where shape `sh` is on the slide: a group's member through its groups.
fn slide_frame(st: &crate::DocState, sh: &Shape) -> Xfrm {
    let groups = groups_of(st, sh.id);
    if groups.is_empty() {
        return xfrm_of(&st.doc, &st.selection, sh);
    }
    let mut f = sh.xfrm.unwrap_or_default();
    for g in groups.iter().rev() {
        if let (ShapeKind::Group { child, .. }, Some(gx)) = (&g.kind, g.xfrm) {
            f = super::arrange::child_to_parent(&gx, child, &f);
        }
    }
    f
}

/// Every shape on the slide being edited, groups' members too.
fn all_shapes(st: &crate::DocState) -> Vec<&Shape> {
    fn walk<'a>(list: &'a [Shape], out: &mut Vec<&'a Shape>) {
        for sh in list {
            out.push(sh);
            if let ShapeKind::Group { children, .. } = &sh.kind {
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(st.shapes(), &mut out);
    out
}

fn describe(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let named = object_ids(st, p).map_err(|e| fail("drawing.describe", e))?;
    let ids: Vec<ShapeId> = match named {
        Some(ids) => ids,
        None if !st.selection.shapes.is_empty() => st.selection.shapes.clone(),
        None => st.shapes().iter().filter(|sh| is_drawing(sh)).map(|sh| sh.id).collect(),
    };
    let mut objects = Vec::new();
    for id in ids {
        let sh = st.shape(id).ok_or_else(|| fail("drawing.describe", DrawingError::not_found(&object_id(st, id))))?;
        if is_drawing(sh) {
            objects.push(to_drawing(st, sh));
        }
    }
    let mut out = json!({ "objects": objects });
    if objects.len() > 1 {
        out["selection"] = dm::describe_many(&objects);
    }
    Ok(out)
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let DrawingOp::Insert(o) = DrawingOp::parse("drawing.insert", p).map_err(|e| fail("drawing.insert", e))? else {
        return Err(bad("drawing.insert", "not an insert"));
    };
    let mut o = *o;
    if p.get("frame").is_none() {
        let size = s.doc()?.doc.slide_size;
        o.frame = match o.kind {
            // As `insert.chart` places one.
            dm::ObjectKind::Chart => Xfrm::new(size.width * 0.15, size.height * 0.15, size.width * 0.7, size.height * 0.7),
            dm::ObjectKind::TextBox => Xfrm::new((size.width - 216.0) / 2.0, (size.height - 36.0) / 2.0, 216.0, 36.0),
            dm::ObjectKind::Shape | dm::ObjectKind::Ink | dm::ObjectKind::Group => {
                Xfrm::new((size.width - 144.0) / 2.0, (size.height - 144.0) / 2.0, 144.0, 144.0)
            }
        };
    }
    if o.placement.kind != dm::PlacementKind::Slide {
        return Err(fail("drawing.insert", DrawingError::unsupported("placement.kind", "objects on a slide are placed on the slide")));
    }
    if o.kind == dm::ObjectKind::Ink {
        return Err(fail("drawing.insert", DrawingError::unsupported("kind", "ink is drawn with drawing.ink")));
    }
    if let Some(spec) = o.chart.as_ref().filter(|_| o.kind == dm::ObjectKind::Chart) {
        kition_office_core::ops::check_frame(&o.frame).map_err(|e| fail("drawing.insert", e))?;
        let chart = chart_spec::new_chart(spec).map_err(|e| fail("drawing.insert", e))?;
        let id = insert::add_shape(s, Shape { xfrm: Some(o.frame), kind: ShapeKind::Chart(Box::new(chart)), ..Default::default() }, true)?;
        return Ok(json!({ "id": id.0.to_string() }));
    }
    let mut shape = Shape { xfrm: Some(o.frame), text_box: o.kind == dm::ObjectKind::TextBox, ..Default::default() };
    if shape.text_box {
        shape.fill = Some(Fill::None);
    } else {
        // A shape looks as `shape.insert` makes it: the theme's accent style, text centred.
        let line_like = matches!(&o.geometry, dm::Geometry::Preset { preset, .. } if kition_slide_geom::preset::is_line_like(preset));
        shape.style = Some(if line_like {
            kition_slide_model::ShapeStyle::line(SchemeSlot::Accent1)
        } else {
            kition_slide_model::ShapeStyle::accent(SchemeSlot::Accent1)
        });
        if !line_like {
            let mut body = TextBody { paragraphs: vec![], ..Default::default() };
            body.body.anchor = Some(Anchor::Middle);
            shape.text = Some(body);
        }
    }
    write_back(&mut shape, &DrawingObject { text: None, ..Default::default() }, &o).map_err(|e| fail("drawing.insert", e))?;
    if !shape.text_box
        && let Some(t) = shape.text.as_mut()
    {
        for para in t.paragraphs.iter_mut().filter(|p| p.props.align.is_none()) {
            para.props.align = Some(Align::Center);
        }
    }
    let id = insert::add_shape(s, shape, true)?;
    Ok(json!({ "id": id.0.to_string() }))
}

fn apply(s: &mut Session, cmd: &str, p: &Value) -> Result<Value> {
    let mut op = DrawingOp::parse(cmd, p).map_err(|e| fail(cmd, e))?;
    let st = s.doc()?;
    // Absolute frames match the drawing object's id, independent of the input address spelling.
    if let DrawingOp::Transform(FrameChange::Set(frames)) = &mut op {
        for (address, _) in frames {
            let local = local_id(st, address).ok_or_else(|| fail(cmd, DrawingError::not_found(address)))?;
            *address = object_id(st, local);
        }
    }
    let ids: Vec<ShapeId> =
        op.ids().iter().map(|id| local_id(st, id).ok_or_else(|| fail(cmd, DrawingError::not_found(id)))).collect::<Result<_>>()?;
    let mut changes = Vec::new();
    for id in &ids {
        let sh = st.shape(*id).ok_or_else(|| fail(cmd, DrawingError::not_found(&object_id(st, *id))))?;
        if !is_drawing(sh) {
            return Err(fail(cmd, DrawingError::unsupported("kind", format!("shape {} is not a shape, text box, or chart", object_id(st, *id)))));
        }
        let before = to_drawing(st, sh);
        let mut after = before.clone();
        op.apply(&mut after).map_err(|e| fail(cmd, e))?;
        if after.frame != before.frame && !groups_of(st, *id).is_empty() {
            return Err(fail(
                cmd,
                DrawingError::unsupported("frame", format!("object {} is in a group: move the group, or ungroup it", object_id(st, *id))),
            ));
        }
        changes.push((*id, before, after));
    }
    if let DrawingOp::Delete(_) = op {
        return s.execute("edit.delete", &json!({ "ids": ids }));
    }
    // A connector's glued ends (`drawing.reconnect`), by this replica's shape ids.
    let mut ends = Vec::new();
    for (id, before, after) in &changes {
        if after.connection == before.connection {
            continue;
        }
        let end = |e: &Option<kition_office_core::connect::End>| -> Result<Option<(ShapeId, u32)>> {
            let Some(e) = e else { return Ok(None) };
            let local = local_id(st, &e.id).filter(|l| l != id).ok_or_else(|| fail(cmd, DrawingError::not_found(&e.id)))?;
            Ok(Some((local, u32::try_from(e.site).unwrap_or(0))))
        };
        let c = after.connection.clone().unwrap_or_default();
        ends.push((*id, end(&c.start)?, end(&c.end)?));
    }
    // One transaction for every object (DR13): all or nothing.
    let frames: Vec<Value> = changes.iter().map(|(_, _, a)| json!(a.frame)).collect();
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad(cmd, "no slide"))?;
        for (id, before, after) in &changes {
            let sh = kition_slide_model::find_shape_mut(list, *id).ok_or_else(|| bad(cmd, "the shape disappeared"))?;
            write_back(sh, before, after).map_err(|e| fail(cmd, e))?;
            // A line glued at an end becomes a connector; its ends follow after every edit.
            if let Some((_, start, end)) = ends.iter().find(|(e, _, _)| e == id) {
                sh.kind = ShapeKind::Connector { start: *start, end: *end };
            }
        }
        Ok(())
    })?;
    if matches!(op, DrawingOp::Text { .. }) {
        for (id, _, _) in &changes {
            let _ = text::fit_text_box(s, *id);
        }
    }
    Ok(json!({ "frames": frames }))
}

/// `drawing.arrange` (`KitDrawing 1.4`, DR21): the objects move in the slide's stack, one
/// transaction. Members of a group move with their group.
fn arrange(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "drawing.arrange";
    let (named, order) = kition_office_core::arrange::Order::parse(p).map_err(|e| fail(CMD, e))?;
    let st = s.doc()?;
    let ids: Vec<ShapeId> = named.iter().map(|id| local_id(st, id).ok_or_else(|| fail(CMD, DrawingError::not_found(id)))).collect::<Result<_>>()?;
    let stack: Vec<String> = st.shapes().iter().map(|sh| sh.id.0.to_string()).collect();
    let moved: Vec<String> = ids.iter().map(|id| id.0.to_string()).collect();
    if let Some((id, _)) = ids.iter().zip(&moved).find(|(_, m)| !stack.contains(m)) {
        return Err(fail(CMD, DrawingError::unsupported("ids", format!("object {} is in a group: arrange the group", object_id(st, *id)))));
    }
    let next = kition_office_core::arrange::arrange(&stack, &moved, order).map_err(|e| fail(CMD, e))?;
    if next == stack {
        return Ok(json!({}));
    }
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad(CMD, "no slide"))?;
        let mut old: Vec<Shape> = std::mem::take(list);
        for id in &next {
            let i = old.iter().position(|sh| sh.id.0.to_string() == *id).ok_or_else(|| bad(CMD, "the shape disappeared"))?;
            list.push(old.remove(i));
        }
        Ok(())
    })?;
    Ok(json!({}))
}

/// `drawing.connect` (`KitDrawing 1.4`, DR25): a connector glued to two objects, Slide's
/// `shape.connect` (connectors follow their ends after every edit).
fn connect(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "drawing.connect";
    let c = kition_office_core::connect::Connect::parse(p).map_err(|e| fail(CMD, e))?;
    let st = s.doc()?;
    let from = local_id(st, &c.from).ok_or_else(|| fail(CMD, DrawingError::not_found(&c.from)))?;
    let to = local_id(st, &c.to).ok_or_else(|| fail(CMD, DrawingError::not_found(&c.to)))?;
    let mut params = json!({ "from": from.0, "to": to.0, "preset": c.preset });
    if let Some(i) = c.from_site {
        params["fromSite"] = json!(i);
    }
    if let Some(i) = c.to_site {
        params["toSite"] = json!(i);
    }
    let r = s.execute("shape.connect", &params)?;
    let id =
        r.get("id").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).map(ShapeId).ok_or_else(|| bad(CMD, "the connector was not made"))?;
    Ok(json!({ "id": object_id(s.doc()?, id) }))
}

/// `drawing.group` (`KitDrawing 1.4`, DR22): one group of objects on the slide, where the topmost
/// was; Slide's Group.
fn group(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "drawing.group";
    let named = kition_office_core::arrange::group_ids(p, 2).map_err(|e| fail(CMD, e))?;
    let st = s.doc()?;
    let mut ids = Vec::new();
    for text in &named {
        let id = local_id(st, text).ok_or_else(|| fail(CMD, DrawingError::not_found(text)))?;
        let top = st.shapes().iter().find(|sh| sh.id == id);
        match top {
            None => return Err(fail(CMD, DrawingError::unsupported("ids", format!("object {text} is in a group: ungroup it first")))),
            Some(sh) if !is_drawing(sh) => {
                return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {text} is not a drawing object"))));
            }
            Some(_) => ids.push(id.0),
        }
    }
    let r = s.execute("arrange.group", &json!({ "ids": ids }))?;
    let gid =
        r.get("id").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).map(ShapeId).ok_or_else(|| bad(CMD, "the group was not made"))?;
    Ok(json!({ "id": object_id(s.doc()?, gid) }))
}

/// `drawing.ungroup` (DR22): the groups' members back in their place; Slide's Ungroup.
fn ungroup(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "drawing.ungroup";
    let named = kition_office_core::arrange::group_ids(p, 1).map_err(|e| fail(CMD, e))?;
    let st = s.doc()?;
    let mut ids = Vec::new();
    for text in &named {
        let id = local_id(st, text).ok_or_else(|| fail(CMD, DrawingError::not_found(text)))?;
        if !st.shapes().iter().any(|sh| sh.id == id && matches!(sh.kind, ShapeKind::Group { .. })) {
            return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {text} is not a group on the slide"))));
        }
        ids.push(id.0);
    }
    let r = s.execute("arrange.ungroup", &json!({ "ids": ids }))?;
    let members: Vec<ShapeId> = r
        .get("ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|n| u32::try_from(n).ok())
        .map(ShapeId)
        .collect();
    let st = s.doc()?;
    Ok(json!({ "ids": members.into_iter().map(|id| object_id(st, id)).collect::<Vec<_>>() }))
}

/// After a top-level `drawing.insert` was written to the replica: its result names the new shape
/// by its `KitDrawing 1.0` id (its replica object, which the write just made).
/// The answer, with what format.font says (fix 25) for every run font this machine lacks, so a font
/// set through KitDrawing text is said as one set through format.font is.
/// `answer` with `design`: what a designer would change about the object the command made (the
/// answer's `id`) or changed (`id`), on the slide being edited (see [`crate::design`]).
fn with_design(s: &Session, answer: Result<Value>, id: Option<&Value>) -> Result<Value> {
    let answer = answer?;
    let Ok(st) = s.doc() else { return Ok(answer) };
    let named = answer.get("id").or(id).and_then(|v| match v {
        Value::String(text) => local_id(st, text),
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()).and_then(|n| local_id(st, &n.to_string())),
        _ => None,
    });
    let Some(shape) = named else { return Ok(answer) };
    let advice = crate::design::shape_advice(&st.doc, st.selection.slide, shape);
    if advice.is_empty() {
        return Ok(answer);
    }
    let mut answer = match answer {
        Value::Object(map) => Value::Object(map),
        Value::Null => serde_json::json!({}),
        other => serde_json::json!({ "result": other }),
    };
    answer["design"] = Value::Array(advice);
    Ok(answer)
}

fn with_font_notes(answer: Result<Value>, p: &Value) -> Result<Value> {
    let answer = answer?;
    let mut families: Vec<&str> = Vec::new();
    for paragraph in p.get("paragraphs").and_then(Value::as_array).into_iter().flatten() {
        for run in paragraph.get("runs").and_then(Value::as_array).into_iter().flatten() {
            if let Some(font) = run.get("font").and_then(Value::as_str)
                && !families.contains(&font)
            {
                families.push(font);
            }
        }
    }
    let notes: Vec<Value> = families.into_iter().filter_map(super::format::font_note).collect();
    if notes.is_empty() {
        return Ok(answer);
    }
    let mut answer = match answer {
        Value::Object(map) => Value::Object(map),
        Value::Null => serde_json::json!({}),
        other => serde_json::json!({ "result": other }),
    };
    answer["fonts"] = Value::Array(notes);
    Ok(answer)
}

pub(crate) fn insert_result(st: &crate::DocState, mut result: Value) -> Value {
    if let Some(id) = result.get("id").and_then(Value::as_str).and_then(|t| t.parse::<u32>().ok()) {
        result["id"] = Value::String(object_id(st, ShapeId(id)));
    }
    result
}

/// A Slide shape, described.
pub fn to_drawing(st: &crate::DocState, sh: &Shape) -> DrawingObject {
    if let ShapeKind::Group { children, .. } = &sh.kind {
        return DrawingObject {
            id: object_id(st, sh.id),
            kind: dm::ObjectKind::Group,
            frame: slide_frame(st, sh),
            members: Some(children.iter().filter(|c| is_drawing(c)).map(|c| object_id(st, c.id)).collect()),
            ..Default::default()
        };
    }
    if let ShapeKind::Ink { strokes } = &sh.kind {
        return DrawingObject {
            id: object_id(st, sh.id),
            kind: dm::ObjectKind::Ink,
            frame: slide_frame(st, sh),
            ink: Some(strokes.iter().map(super::ink::stroke_of).collect()),
            ..Default::default()
        };
    }
    let chart = match &sh.kind {
        ShapeKind::Chart(c) => Some(chart_spec::spec_of(c)),
        _ => None,
    };
    DrawingObject {
        id: object_id(st, sh.id),
        kind: if chart.is_some() {
            dm::ObjectKind::Chart
        } else if sh.text_box {
            dm::ObjectKind::TextBox
        } else {
            dm::ObjectKind::Shape
        },
        chart,
        frame: slide_frame(st, sh),
        placement: dm::Placement::default(),
        geometry: geometry_of(&sh.geom),
        fill: sh.fill.as_ref().map(fill_of),
        stroke: sh.line.as_ref().map(stroke_of),
        text: sh.text.as_ref().map(text_of),
        ink: None,
        members: None,
        effects: effects_of(sh),
        connection: match &sh.kind {
            ShapeKind::Connector { start, end } => Some(kition_office_core::connect::Connection {
                start: start.map(|(id, site)| kition_office_core::connect::End { id: object_id(st, id), site: site as usize }),
                end: end.map(|(id, site)| kition_office_core::connect::End { id: object_id(st, id), site: site as usize }),
            }),
            _ => None,
        },
    }
}

/// A shape's own shadow and glow (DR24); those its style gives it are not its own.
fn effects_of(sh: &Shape) -> Option<dm::Effects> {
    let e = sh.effects.as_ref()?;
    let out = dm::Effects {
        shadow: e.outer_shadow.as_ref().map(|s| dm::Shadow { color: color_of(&s.color), blur: s.blur, distance: s.dist, angle: s.dir }),
        glow: e.glow.as_ref().map(|g| dm::Glow { color: color_of(&g.color), size: g.radius }),
    };
    (!out.is_empty()).then_some(out)
}

/// Writes the members that differ between `before` and `after` into `sh`.
fn write_back(sh: &mut Shape, before: &DrawingObject, after: &DrawingObject) -> std::result::Result<(), DrawingError> {
    // The ink layer's strokes are in slide points: it is drawn on and erased, not moved or styled.
    if matches!(sh.kind, ShapeKind::Ink { .. }) && after != before {
        return Err(DrawingError::unsupported("kind", "ink is drawn with drawing.ink and erased with drawing.erase"));
    }
    // A group moves, resizes and turns with its members; its members keep their own looks.
    if matches!(sh.kind, ShapeKind::Group { .. }) && (DrawingObject { frame: before.frame, ..after.clone() }) != *before {
        return Err(DrawingError::unsupported("kind", "a group's members are formatted one by one"));
    }
    if after.frame != before.frame || sh.xfrm.is_none() {
        kition_office_core::ops::check_frame(&after.frame)?;
        sh.xfrm = Some(after.frame);
    }
    if after.chart != before.chart
        && let (ShapeKind::Chart(c), Some(b), Some(a)) = (&mut sh.kind, &before.chart, &after.chart)
    {
        chart_spec::apply_spec(c, b, a)?;
    }
    if matches!(sh.kind, ShapeKind::Chart(_)) {
        if after.geometry != before.geometry {
            return Err(DrawingError::unsupported("geometry", "a chart's frame is a rectangle"));
        }
        if after.text != before.text {
            return Err(DrawingError::unsupported("text", "a chart's text is its title and labels (drawing.chart)"));
        }
    }
    if after.geometry != before.geometry {
        sh.geom = match &after.geometry {
            dm::Geometry::Preset { preset, .. } => Geom::Preset { name: preset.clone(), adj: after.geometry.adjust_values() },
            dm::Geometry::Custom { paths } => Geom::Custom {
                paths: paths
                    .iter()
                    .map(|p| kition_slide_model::CustomPath {
                        w: p.w,
                        h: p.h,
                        d: p.d.clone(),
                        fill: if p.filled { kition_slide_geom::preset::FillMode::Norm } else { kition_slide_geom::preset::FillMode::None },
                        stroke: p.stroked,
                    })
                    .collect(),
                source: None,
            },
        };
    }
    if after.effects != before.effects {
        let (shadow, glow) = after.effects.as_ref().map_or((None, None), |e| (e.shadow.clone(), e.glow.clone()));
        // Kept even when emptied: it overrides a shadow the shape's style would give it.
        let e = sh.effects.get_or_insert_with(Default::default);
        e.outer_shadow = shadow
            .map(|s| {
                Ok(kition_slide_model::style::Shadow {
                    color: color_ref(&s.color)?,
                    blur: s.blur,
                    dist: s.distance,
                    dir: s.angle,
                    inner: false,
                    sx: 1.0,
                    sy: 1.0,
                    kx: 0.0,
                    ky: 0.0,
                    align: "ctr".into(),
                    rotate_with_shape: false,
                })
            })
            .transpose()?;
        e.glow = glow.map(|g| Ok::<_, DrawingError>(kition_slide_model::style::Glow { color: color_ref(&g.color)?, radius: g.size })).transpose()?;
    }
    if after.fill != before.fill {
        sh.fill = match &after.fill {
            None => None,
            Some(dm::Fill::None) => Some(Fill::None),
            Some(dm::Fill::Solid(c)) => Some(Fill::solid(color_ref(c)?)),
            Some(dm::Fill::Gradient(g)) => Some(Fill::Gradient(kition_slide_model::style::Gradient {
                stops: g
                    .stops
                    .iter()
                    .map(|st| Ok(kition_slide_model::style::GradientStop { pos: st.pos, color: color_ref(&st.color)? }))
                    .collect::<std::result::Result<Vec<_>, DrawingError>>()?,
                shape: match &g.path {
                    // From the centre: the fill-to rect a point in the middle.
                    Some(path) => kition_slide_model::style::GradientShape::Path { path: path.clone(), focus: [0.5, 0.5, 0.5, 0.5] },
                    None => kition_slide_model::style::GradientShape::Linear { angle: g.angle, scaled: false },
                },
                tile_rect: None,
                rotate_with_shape: true,
            })),
            Some(dm::Fill::Other(kind)) => return Err(DrawingError::unsupported("fill", format!("a {kind} fill is set in the Format pane"))),
        };
    }
    if after.stroke != before.stroke
        && let Some(stroke) = &after.stroke
    {
        let line = sh.line.get_or_insert_with(Line::default);
        if stroke.none {
            line.fill = Some(Fill::None);
        } else {
            if let Some(c) = &stroke.color {
                line.fill = Some(Fill::solid(color_ref(c)?));
            } else if matches!(line.fill, Some(Fill::None)) {
                line.fill = None;
            }
            if stroke.width.is_some() {
                line.width = stroke.width;
            }
            if let Some(d) = &stroke.dash {
                line.dash = Some(kition_slide_model::style::Dash::Preset { name: d.clone() });
            }
        }
    }
    if after.text != before.text
        && let Some(t) = &after.text
    {
        sh.text = Some(text_back(sh.text.take().unwrap_or_default(), t)?);
    }
    Ok(())
}

fn geometry_of(g: &Geom) -> dm::Geometry {
    match g {
        Geom::Preset { name, adj } => {
            let one = kition_slide_geom::preset::info(name).is_none_or(|i| i.defaults.len() <= 1);
            let adjust = adj
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
                .map(|(i, v)| (if one && i == 0 { "adj".to_string() } else { format!("adj{}", i + 1) }, *v))
                .collect();
            dm::Geometry::Preset { preset: name.clone(), adjust }
        }
        Geom::Custom { paths, .. } => dm::Geometry::Custom {
            paths: paths
                .iter()
                .map(|p| dm::CustomPath {
                    w: p.w,
                    h: p.h,
                    d: p.d.clone(),
                    filled: p.fill != kition_slide_geom::preset::FillMode::None,
                    stroked: p.stroke,
                })
                .collect(),
        },
    }
}

use kition_slide_model::chart_spec::{color_of, color_ref};

fn fill_of(f: &Fill) -> dm::Fill {
    match f {
        Fill::None => dm::Fill::None,
        Fill::Solid { color } => dm::Fill::Solid(color_of(color)),
        Fill::Gradient(g) => dm::Fill::Gradient(dm::Gradient {
            stops: g.stops.iter().map(|st| dm::GradientStop { pos: st.pos, color: color_of(&st.color) }).collect(),
            angle: match &g.shape {
                kition_slide_model::style::GradientShape::Linear { angle, .. } => *angle,
                kition_slide_model::style::GradientShape::Path { .. } => 0.0,
            },
            path: match &g.shape {
                kition_slide_model::style::GradientShape::Path { path, .. } => Some(path.clone()),
                kition_slide_model::style::GradientShape::Linear { .. } => None,
            },
        }),
        Fill::Picture(_) => dm::Fill::Other("picture".into()),
        Fill::Pattern(_) => dm::Fill::Other("pattern".into()),
        Fill::Group => dm::Fill::Other("group".into()),
        Fill::Background => dm::Fill::Other("background".into()),
    }
}

fn stroke_of(l: &Line) -> dm::Stroke {
    let none = matches!(l.fill, Some(Fill::None));
    dm::Stroke {
        none,
        color: match &l.fill {
            Some(Fill::Solid { color }) => Some(color_of(color)),
            _ => None,
        },
        width: if none { None } else { l.width },
        dash: match &l.dash {
            Some(kition_slide_model::style::Dash::Preset { name }) if !none => Some(name.clone()),
            _ => None,
        },
    }
}

fn text_of(t: &TextBody) -> dm::TextBody {
    let b = &t.body;
    dm::TextBody {
        paragraphs: t
            .paragraphs
            .iter()
            .map(|p| dm::Paragraph {
                align: p.props.align.map(|a| match a {
                    Align::Center => dm::Align::Center,
                    Align::Right => dm::Align::Right,
                    Align::Justify | Align::JustLow | Align::Distributed => dm::Align::Justify,
                    Align::Left => dm::Align::Left,
                }),
                runs: p
                    .runs
                    .iter()
                    .map(|r| dm::Run {
                        text: r.text.clone(),
                        font: r.props.font.clone(),
                        size: r.props.size,
                        bold: r.props.bold,
                        italic: r.props.italic,
                        underline: r.props.underline.as_deref().map(|u| u != "none"),
                        color: match &r.props.fill {
                            Some(Fill::Solid { color }) => Some(color_of(color)),
                            _ => None,
                        },
                    })
                    .collect(),
            })
            .collect(),
        anchor: match b.anchor {
            Some(Anchor::Middle) => dm::TextAnchor::Middle,
            Some(Anchor::Bottom) => dm::TextAnchor::Bottom,
            _ => dm::TextAnchor::Top,
        },
        insets: [b.inset_l.unwrap_or(7.2), b.inset_t.unwrap_or(3.6), b.inset_r.unwrap_or(7.2), b.inset_b.unwrap_or(3.6)],
        wrap: b.wrap.unwrap_or(true),
        autofit: match b.autofit {
            Some(AutoFit::Shrink { .. }) => dm::Autofit::Shrink,
            Some(AutoFit::Shape) => dm::Autofit::Resize,
            _ => dm::Autofit::None,
        },
        warp: b.warp.as_ref().filter(|p| *p != "textPlain").map(|p| dm::Warp { preset: p.clone(), adjust: b.warp_adjust.iter().cloned().collect() }),
    }
}

/// New text over an old body: each paragraph keeps the old paragraph's properties at its index
/// (bullets, spacing) and its runs take the old paragraph's first run's properties for whatever
/// the new run leaves unset, so replacing the words keeps the look.
fn text_back(old: TextBody, t: &dm::TextBody) -> std::result::Result<TextBody, DrawingError> {
    let before = text_of(&old);
    let mut body = old.clone();
    if t.anchor != before.anchor {
        body.body.anchor = Some(match t.anchor {
            dm::TextAnchor::Top => Anchor::Top,
            dm::TextAnchor::Middle => Anchor::Middle,
            dm::TextAnchor::Bottom => Anchor::Bottom,
        });
    }
    if t.insets != before.insets {
        let [l, tp, r, b] = t.insets;
        (body.body.inset_l, body.body.inset_t, body.body.inset_r, body.body.inset_b) = (Some(l), Some(tp), Some(r), Some(b));
    }
    if t.wrap != before.wrap {
        body.body.wrap = Some(t.wrap);
    }
    if t.autofit != before.autofit {
        body.body.autofit = Some(match t.autofit {
            dm::Autofit::None => AutoFit::None,
            dm::Autofit::Shrink => AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 },
            dm::Autofit::Resize => AutoFit::Shape,
        });
    }
    if t.warp != before.warp {
        body.body.warp = t.warp.as_ref().map(|w| w.preset.clone());
        body.body.warp_adjust = t.warp.as_ref().map(|w| w.adjust.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
    }
    if t.paragraphs != before.paragraphs {
        let template = |i: usize| old.paragraphs.get(i).or(old.paragraphs.last()).cloned().unwrap_or_default();
        let mut paras = Vec::with_capacity(t.paragraphs.len());
        for (i, p) in t.paragraphs.iter().enumerate() {
            let base = template(i);
            let first = base.runs.first().map(|r| r.props.clone()).unwrap_or_else(|| base.end_props.clone());
            let mut para = Paragraph { level: base.level, props: base.props.clone(), runs: Vec::new(), end_props: base.end_props.clone() };
            if let Some(a) = p.align {
                para.props.align = Some(match a {
                    dm::Align::Left => Align::Left,
                    dm::Align::Center => Align::Center,
                    dm::Align::Right => Align::Right,
                    dm::Align::Justify => Align::Justify,
                });
            }
            for r in &p.runs {
                let mut props: RunProps = first.clone();
                if let Some(font) = &r.font {
                    props.font = Some(font.clone());
                    // An East Asian family is the run's East Asian font too (see format.font).
                    if !kition_slide_fonts::east_asian_substitutes(font).is_empty() {
                        props.font_ea = Some(font.clone());
                    }
                }
                if r.size.is_some() {
                    props.size = r.size;
                }
                if r.bold.is_some() {
                    props.bold = r.bold;
                }
                if r.italic.is_some() {
                    props.italic = r.italic;
                }
                if let Some(u) = r.underline {
                    props.underline = Some(if u { "sng" } else { "none" }.into());
                }
                if let Some(c) = &r.color {
                    props.fill = Some(Fill::solid(color_ref(c)?));
                }
                para.runs.push(Run::with(r.text.clone(), props));
            }
            paras.push(para);
        }
        body.paragraphs = paras;
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::with_new()
    }

    #[test]
    fn shared_text_edits_preserve_private_text_3d_metadata() {
        let mut old = TextBody::from_text("Before");
        old.body.scene3d_xml = Some(r#"<a:scene3d><a:camera prst="obliqueBottomRight"/><a:lightRig rig="flat" dir="t"/></a:scene3d>"#.into());
        old.body.text3d_xml = Some(r#"<a:sp3d extrusionH="254000"/>"#.into());
        let mut changed = text_of(&old);
        changed.paragraphs[0].runs[0].text = "After".into();
        changed.anchor = dm::TextAnchor::Bottom;
        let updated = text_back(old.clone(), &changed).unwrap();
        assert_eq!(updated.paragraphs[0].runs[0].text, "After");
        assert_eq!(updated.body.anchor, Some(Anchor::Bottom));
        assert_eq!(updated.body.scene3d_xml, old.body.scene3d_xml);
        assert_eq!(updated.body.text3d_xml, old.body.text3d_xml);
    }

    /// This replica's shape for a KitDrawing id.
    fn sid(s: &Session, id: &str) -> ShapeId {
        local_id(s.doc().unwrap(), id).unwrap()
    }

    /// khp-bench r2 (a, b): `drawing.text` with only `body`, and `text` given as the text object
    /// `drawing.describe` returns, were refused with "give `text` or `paragraphs`".
    #[test]
    fn text_takes_body_alone_and_the_described_text_object() {
        let mut s = session();
        let given = json!({ "kind": "textBox", "frame": { "x": 80, "y": 190, "w": 800, "h": 90 }, "text": {
            "paragraphs": [{ "runs": [{ "text": "Q3 review", "size": 48, "bold": true }] }],
        } });
        let id = s.execute("drawing.insert", &given).unwrap()["id"].as_str().unwrap().to_string();
        let described = |s: &mut Session| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        let text = described(&mut s).text.unwrap();
        assert_eq!((text.text().as_str(), text.paragraphs[0].runs[0].size), ("Q3 review", Some(48.0)));
        s.execute("drawing.text", &json!({ "id": id, "body": { "anchor": "middle", "wrap": false } })).unwrap();
        let text = described(&mut s).text.unwrap();
        assert_eq!((text.text().as_str(), text.anchor, text.wrap), ("Q3 review", dm::TextAnchor::Middle, false));
    }

    #[test]
    fn absolute_frame_matches_the_resolved_numeric_or_persistent_shape() {
        for numeric in [true, false] {
            let mut s = session();
            let tool =
                s.execute("drawing.insert", &json!({ "frame": { "x": 10, "y": 20, "w": 30, "h": 40 } })).unwrap()["id"].as_str().unwrap().to_string();
            let local = sid(&s, &tool);
            let address = if numeric { json!(local.0) } else { json!(tool) };
            s.execute("drawing.transform", &json!({ "id": address, "frame": { "x": 100, "y": 120, "w": 130, "h": 140 } })).unwrap();
            let frame = s.doc().unwrap().shape(local).unwrap().xfrm.unwrap();
            assert_eq!((frame.x, frame.y, frame.w, frame.h), (100.0, 120.0, 130.0, 140.0), "address={address}");
            s.execute("edit.undo", &json!({})).unwrap();
            let original = s.doc().unwrap().shape(local).unwrap().xfrm.unwrap();
            assert_eq!((original.x, original.y, original.w, original.h), (10.0, 20.0, 30.0, 40.0));
        }
    }

    #[test]
    fn insert_describe_edit_undo_and_reopen() {
        let mut s = session();
        let r = s
            .execute(
                "drawing.insert",
                &json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 100, "y": 80, "w": 200, "h": 100 }, "fill": { "color": "#5645D4" }, "stroke": { "color": "accent2", "width": 3 }, "text": "Q3" }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let d = s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap();
        let o: DrawingObject = serde_json::from_value(d["objects"][0].clone()).unwrap();
        assert_eq!(o.geometry, dm::Geometry::preset("ellipse"));
        assert_eq!(o.fill, Some(dm::Fill::Solid(dm::Color::rgb(dm::Rgb(0x56, 0x45, 0xD4)))));
        assert_eq!(o.stroke.as_ref().unwrap().width, Some(3.0));
        assert_eq!(o.text.as_ref().unwrap().text(), "Q3");

        s.execute("drawing.transform", &json!({ "ids": [id], "dx": 10, "rotateBy": 30 })).unwrap();
        s.execute(
            "drawing.format",
            &json!({ "ids": [id], "fill": { "color": "accent1" }, "geometry": { "preset": "roundRect", "adjust": { "adj": 30000 } } }),
        )
        .unwrap();
        s.execute("drawing.text", &json!({ "id": id, "text": "Q4\nUp" })).unwrap();
        let after = |s: &Session| -> DrawingObject {
            let st = s.doc().unwrap();
            to_drawing(st, st.shape(local_id(st, &id).unwrap()).unwrap())
        };
        let o = after(&s);
        assert_eq!((o.frame.x, o.frame.rot), (110.0, 30.0));
        assert_eq!(o.geometry, dm::Geometry::Preset { preset: "roundRect".into(), adjust: [("adj".to_string(), 30000.0)].into() });
        assert_eq!(o.text.as_ref().unwrap().paragraphs.len(), 2);

        // Each command is one undo step.
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(after(&s).text.unwrap().text(), "Q3");
        s.execute("edit.redo", &json!({})).unwrap();

        // Save as PPTX, reopen, and edit again.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "d.pptx", None, file::open_slide_file("d.pptx", &bytes).unwrap());
        // A file opened without its replica starts a new one: its shapes have new ids (DR2).
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let o: DrawingObject =
            serde_json::from_value(d["objects"].as_array().unwrap().iter().find(|o| o["geometry"]["preset"] == "roundRect").unwrap().clone())
                .unwrap();
        let id = o.id.clone();
        assert_eq!((o.frame.x, o.frame.rot), (110.0, 30.0));
        assert_eq!(o.fill, Some(dm::Fill::Solid(dm::Color::theme("accent1"))));
        assert_eq!(o.text.as_ref().unwrap().text(), "Q4\nUp");
        back.execute("drawing.format", &json!({ "ids": [id], "stroke": { "none": true } })).unwrap();
        let d = back.execute("drawing.describe", &json!({ "ids": [id] })).unwrap();
        assert_eq!(d["objects"][0]["stroke"], json!({ "none": true }));
    }

    #[test]
    fn objects_move_in_the_slides_stack_in_one_step_and_reopen() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str| -> String {
            s.execute("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": 100, "y": 80, "w": 80, "h": 60 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, c) = (ins(&mut s, "rect"), ins(&mut s, "ellipse"), ins(&mut s, "triangle"));
        let order = |s: &mut Session| -> Vec<String> {
            let st = s.doc().unwrap();
            st.shapes().iter().filter(|sh| is_drawing(sh)).map(|sh| object_id(st, sh.id)).collect()
        };
        assert_eq!(order(&mut s)[order(&mut s).len() - 3..], [a.clone(), b.clone(), c.clone()]);
        s.execute("drawing.arrange", &json!({ "ids": [a], "order": "front" })).unwrap();
        let o = order(&mut s);
        assert_eq!(o[o.len() - 3..], [b.clone(), c.clone(), a.clone()]);
        s.execute("drawing.arrange", &json!({ "ids": [a], "order": "backward" })).unwrap();
        let o = order(&mut s);
        assert_eq!(o[o.len() - 3..], [b.clone(), a.clone(), c.clone()]);
        // One undo step each; an unknown id changes nothing.
        s.execute("edit.undo", &json!({})).unwrap();
        let o = order(&mut s);
        assert_eq!(o[o.len() - 3..], [b.clone(), c.clone(), a.clone()]);
        assert!(s.execute("drawing.arrange", &json!({ "ids": ["nope"], "order": "back" })).is_err());
        assert!(s.execute("drawing.arrange", &json!({ "ids": [a], "order": "up" })).is_err());
        // The stack is saved: the triangle is under the rectangle after reopening.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "d.pptx", None, file::open_slide_file("d.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let presets: Vec<String> =
            d["objects"].as_array().unwrap().iter().filter_map(|o| o["geometry"]["preset"].as_str().map(str::to_string)).collect();
        // The slide's placeholders come first; ours are the last three.
        let at = |p: &str| presets.iter().rposition(|x| x == p).unwrap();
        assert!(at("ellipse") < at("triangle") && at("triangle") < at("rect"), "{presets:?}");
    }

    #[test]
    fn objects_group_move_together_ungroup_and_reopen() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64| -> String {
            s.execute("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": 80, "w": 80, "h": 60 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, c) = (ins(&mut s, "rect", 100.0), ins(&mut s, "ellipse", 300.0), ins(&mut s, "triangle", 500.0));
        let obj = |s: &mut Session, id: &str| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        let g = s.execute("drawing.group", &json!({ "ids": [a, b] })).unwrap()["id"].as_str().unwrap().to_string();
        let group = obj(&mut s, &g);
        assert_eq!(group.kind, dm::ObjectKind::Group);
        assert_eq!(group.members.as_deref(), Some(&[a.clone(), b.clone()][..]), "members keep their ids");
        assert_eq!((group.frame.x, group.frame.w), (100.0, 280.0));
        // A member is still described where it is.
        assert_eq!(obj(&mut s, &a).frame.x, 100.0);
        // Moving the group carries its members; its look is its members'.
        s.execute("drawing.transform", &json!({ "ids": [g], "dx": 40 })).unwrap();
        assert!((obj(&mut s, &b).frame.x - 340.0).abs() < 1e-6);
        let e = s.execute("drawing.format", &json!({ "ids": [g], "fill": { "color": "#FF0000" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
        // A member, or one object, is not grouped again.
        assert!(s.execute("drawing.group", &json!({ "ids": [a, c] })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.execute("drawing.group", &json!({ "ids": [c] })).is_err());
        assert!(s.execute("drawing.ungroup", &json!({ "ids": [c] })).unwrap_err().to_string().contains("unsupported"));
        // Saved and reopened, the group is there with its members.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "g.pptx", None, file::open_slide_file("g.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let groups: Vec<&Value> = d["objects"].as_array().unwrap().iter().filter(|o| o["kind"] == "group").collect();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["members"].as_array().unwrap().len(), 2);
        // Ungrouping puts the members back where they appear, in one step.
        let r = s.execute("drawing.ungroup", &json!({ "ids": [g] })).unwrap();
        assert_eq!(r["ids"], json!([a, b]));
        assert!((obj(&mut s, &b).frame.x - 340.0).abs() < 1e-6);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &g).kind, dm::ObjectKind::Group);
    }

    #[test]
    fn gradient_fills_format_describe_and_reopen() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 80, "w": 200, "h": 100 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let gradient =
            json!({ "gradient": { "stops": [{ "pos": 0, "color": "#5645D4" }, { "pos": 1, "color": { "theme": "accent2" } }], "angle": 90 } });
        s.execute("drawing.format", &json!({ "ids": [id], "fill": gradient })).unwrap();
        let o: DrawingObject = serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap();
        let expected: dm::Fill = serde_json::from_value(gradient.clone()).unwrap();
        assert_eq!(o.fill.as_ref(), Some(&expected));
        let radial = json!({ "gradient": { "stops": [{ "pos": 0, "color": "#FFFFFF" }, { "pos": 1, "color": "#000000" }], "path": "circle" } });
        s.execute("drawing.format", &json!({ "ids": [id], "fill": radial })).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        // Saved and reopened, the gradient is there.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "g.pptx", None, file::open_slide_file("g.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let fills: Vec<dm::Fill> = d["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["geometry"]["preset"] == "rect")
            .filter_map(|o| serde_json::from_value(o["fill"].clone()).ok())
            .collect();
        assert!(fills.contains(&expected), "{fills:?}");
        assert!(
            s.execute("drawing.format", &json!({ "ids": [id], "fill": { "gradient": { "stops": [{ "pos": 0, "color": "#000000" }] } } })).is_err()
        );
    }

    #[test]
    fn shadows_and_glows_format_describe_and_reopen() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 80, "w": 200, "h": 100 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let obj = |s: &mut Session| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        s.execute(
            "drawing.format",
            &json!({ "ids": [id], "effects": { "shadow": { "color": { "rgb": "#000000", "alpha": 0.4 }, "blur": 4, "distance": 3, "angle": 45 }, "glow": { "color": "#5645D4", "size": 6 } } }),
        )
        .unwrap();
        let e = obj(&mut s).effects.unwrap();
        let shadow = e.shadow.clone().unwrap();
        assert_eq!((shadow.blur, shadow.distance, shadow.angle, shadow.color.alpha), (4.0, 3.0, 45.0, Some(0.4)));
        assert_eq!(e.glow.as_ref().map(|g| g.size), Some(6.0));
        // Removing the shadow keeps the glow; undo brings the shadow back.
        s.execute("drawing.format", &json!({ "ids": [id], "effects": { "shadow": null } })).unwrap();
        assert!(obj(&mut s).effects.is_some_and(|e| e.shadow.is_none() && e.glow.is_some()));
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s).effects, Some(e.clone()));
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "e.pptx", None, file::open_slide_file("e.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let reopened: Vec<DrawingObject> = d["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| serde_json::from_value::<DrawingObject>(o.clone()).ok())
            .filter(|o| o.effects.is_some())
            .collect();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened[0].effects, Some(e));
    }

    #[test]
    fn shape_text_takes_runs_and_alignment_and_reopens() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 80, "w": 240, "h": 120 }, "text": "x" }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let paragraphs = json!([
            { "align": "center", "runs": [{ "text": "Q3 " }, { "text": "results", "bold": true, "size": 18, "color": "#C00000" }] },
            { "runs": [{ "text": "up 12%", "italic": true, "underline": true, "font": "Arial" }] }
        ]);
        s.execute("drawing.text", &json!({ "id": id, "paragraphs": paragraphs })).unwrap();
        let check = |t: &dm::TextBody| {
            assert_eq!(t.text(), "Q3 results\nup 12%");
            assert_eq!(t.paragraphs[0].align, Some(dm::Align::Center));
            let r = &t.paragraphs[0].runs[1];
            assert_eq!((r.bold, r.size, r.color.clone()), (Some(true), Some(18.0), Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
            let r = &t.paragraphs[1].runs[0];
            assert_eq!((r.italic, r.underline, r.font.as_deref()), (Some(true), Some(true), Some("Arial")));
        };
        let o: DrawingObject = serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap();
        check(o.text.as_ref().unwrap());
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "t.pptx", None, file::open_slide_file("t.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let reopened: DrawingObject = serde_json::from_value(
            d["objects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["geometry"]["preset"] == "rect" && o["text"]["paragraphs"].as_array().is_some_and(|p| p.len() == 2))
                .unwrap()
                .clone(),
        )
        .unwrap();
        check(reopened.text.as_ref().unwrap());
    }

    #[test]
    fn a_connector_joins_two_shapes_follows_them_and_reopens() {
        let mut s = session();
        let ins = |s: &mut Session, x: f64, y: f64| -> String {
            s.execute("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b) = (ins(&mut s, 50.0, 50.0), ins(&mut s, 400.0, 250.0));
        let obj = |s: &mut Session, id: &str| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        let c = s.execute("drawing.connect", &json!({ "from": a, "to": b, "geometry": { "preset": "bentConnector3" } })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &c);
        let conn = o.connection.clone().unwrap();
        assert_eq!((conn.start.as_ref().unwrap().id.as_str(), conn.end.as_ref().unwrap().id.as_str()), (a.as_str(), b.as_str()));
        assert_eq!(o.geometry, dm::Geometry::preset("bentConnector3"));
        let (_, end) = kition_office_core::connect::line_ends(&o.frame);
        let site = |s: &mut Session, id: &str, k: usize| kition_office_core::connect::sites(&obj(s, id))[k];
        let at = site(&mut s, &b, conn.end.as_ref().unwrap().site);
        assert!((end - at).hypot() < 0.5, "{end:?} {at:?}");
        // b moves: the connector's end follows, in the same step.
        s.execute("drawing.transform", &json!({ "ids": [b], "dy": 100 })).unwrap();
        let (_, end) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        let at = site(&mut s, &b, conn.end.as_ref().unwrap().site);
        assert!((end - at).hypot() < 0.5, "{end:?} {at:?}");
        s.execute("edit.undo", &json!({})).unwrap();
        let (_, end) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        assert!((end.y - site(&mut s, &b, conn.end.as_ref().unwrap().site).y).abs() < 0.5);
        // Saved and reopened, it is still glued.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "c.pptx", None, file::open_slide_file("c.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let glued =
            d["objects"].as_array().unwrap().iter().filter(|o| o["connection"]["start"].is_object() && o["connection"]["end"].is_object()).count();
        assert_eq!(glued, 1);
        assert!(s.execute("drawing.connect", &json!({ "from": a, "to": a })).is_err());
    }

    #[test]
    fn charts_insert_change_undo_and_reopen_keeping_what_slide_does_not_model() {
        let mut s = session();
        let r = s
            .execute(
                "drawing.insert",
                &json!({ "kind": "chart", "frame": { "x": 60, "y": 40, "w": 480, "h": 300 }, "chart": { "kind": "line", "title": "Trend" } }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let describe = |s: &mut Session, id: &str| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        let o = describe(&mut s, &id);
        assert_eq!((o.kind, o.frame.w), (dm::ObjectKind::Chart, 480.0));
        let c = o.chart.unwrap();
        assert_eq!((c.kind, c.title.as_deref()), (kition_office_core::chart::ChartKind::Line, Some("Trend")));

        s.execute(
            "drawing.chart",
            &json!({ "id": id, "kind": "column", "categories": ["A", "B"], "series": [{ "name": "Sales", "values": [3, 4], "color": "#FF0000" }], "legend": null }),
        )
        .unwrap();
        let c = describe(&mut s, &id).chart.unwrap();
        assert_eq!((c.categories.len(), c.series.len(), c.legend), (2, 1, None));
        assert_eq!(c.series[0].color, Some(dm::Color::rgb(dm::Rgb(255, 0, 0))));
        // One step: undo takes the whole change back.
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(describe(&mut s, &id).chart.unwrap().kind, kition_office_core::chart::ChartKind::Line);
        s.execute("edit.redo", &json!({})).unwrap();
        // A chart's frame moves like any object; its geometry and text are not a shape's.
        s.execute("drawing.transform", &json!({ "ids": [id], "dx": 20 })).unwrap();
        assert_eq!(describe(&mut s, &id).frame.x, 80.0);
        let e = s.execute("drawing.format", &json!({ "ids": [id], "geometry": { "preset": "ellipse" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");

        // Save, reopen; mark the kept chart part with something Slide does not model.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "c.pptx", None, file::open_slide_file("c.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let o: DrawingObject =
            serde_json::from_value(d["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "chart").unwrap().clone()).unwrap();
        assert_eq!(o.chart.as_ref().unwrap().series[0].values, vec![Some(3.0), Some(4.0)]);
        let chart_id = sid(&back, &o.id);
        back.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).unwrap();
            if let ShapeKind::Chart(c) = &mut kition_slide_model::find_shape_mut(list, chart_id).unwrap().kind {
                let raw = c.raw.take().expect("the read part is kept");
                c.raw = Some(raw.replacen(
                    "</c:plotArea>",
                    "<c:extLst><c:ext uri=\"{00000000-0000-0000-0000-00000000C0DE}\"><kept/></c:ext></c:extLst></c:plotArea>",
                    1,
                ));
            }
            Ok(())
        })
        .unwrap();
        // Edit it: the title changes, the kept part goes on.
        back.execute("drawing.chart", &json!({ "id": o.id, "title": "Revenue" })).unwrap();
        let bytes = file::save_bytes(&back.doc().unwrap().doc, "pptx").unwrap();
        let again = file::open_slide_file("c.pptx", &bytes).unwrap();
        let raw = again.slides[0]
            .shapes
            .iter()
            .find_map(|sh| match &sh.kind {
                ShapeKind::Chart(c) => c.raw.clone(),
                _ => None,
            })
            .unwrap();
        assert!(raw.contains("Revenue") && raw.contains("C0DE"), "the edited part keeps what Slide does not model");
    }

    #[test]
    fn chart_axes_labels_and_series_looks_change_draw_and_reopen() {
        let mut s = session();
        let id =
            s.execute("drawing.insert", &json!({ "kind": "chart", "chart": { "kind": "lineMarkers" } })).unwrap()["id"].as_str().unwrap().to_string();
        let chart = |s: &mut Session| -> kition_office_core::chart::ChartSpec {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0]["chart"].clone()).unwrap()
        };
        let undo = s.active().unwrap().history.undo.len();
        s.execute(
            "drawing.chart",
            &json!({
                "id": id,
                "yAxis": { "min": 0, "max": 10, "major": 2, "numberFormat": "0.0" },
                "xAxis": { "hidden": true },
                "dataLabels": true,
                "dataLabelPosition": "t",
                "seriesFormat": [{ "index": 0, "lineWidth": 3, "marker": "diamond" }, { "index": 1, "smooth": true }]
            }),
        )
        .unwrap();
        assert_eq!(s.active().unwrap().history.undo.len(), undo + 1, "one step");
        let c = chart(&mut s);
        assert_eq!((c.y_axis.min, c.y_axis.max, c.y_axis.major, c.y_axis.number_format.as_deref()), (Some(0.0), Some(10.0), Some(2.0), Some("0.0")));
        assert!(c.x_axis.hidden);
        assert_eq!(c.data_label_position.as_deref(), Some("t"));
        assert_eq!((c.series[0].line_width, c.series[0].marker.as_deref(), c.series[1].smooth), (Some(3.0), Some("diamond"), true));
        // Drawn with its axes: the value axis the renderer finds has the scale.
        let st = s.doc().unwrap();
        let ShapeKind::Chart(model) = &st.shape(sid(&s, &id)).unwrap().kind else { panic!("a chart") };
        let (_, y) = model.main_axes().expect("axes");
        assert_eq!(model.fmt.axis(y).map(|a| (a.min, a.max, a.major_unit)), Some((Some(0.0), Some(10.0), Some(2.0))));

        // Saved and reopened, the chart reads the same.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "f.pptx", None, file::open_slide_file("f.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let reopened: kition_office_core::chart::ChartSpec =
            serde_json::from_value(d["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "chart").unwrap()["chart"].clone()).unwrap();
        assert_eq!(
            (reopened.y_axis.clone(), reopened.x_axis.hidden, reopened.data_label_position.clone()),
            (c.y_axis.clone(), true, c.data_label_position.clone())
        );
        assert_eq!(
            (reopened.series[0].line_width, reopened.series[0].marker.clone(), reopened.series[1].smooth),
            (Some(3.0), Some("diamond".to_string()), true)
        );

        // A chart read from a file: its gridlines and axis title change where it is drawn from.
        let rid = d["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "chart").unwrap()["id"].as_str().unwrap().to_string();
        back.execute("drawing.chart", &json!({ "id": rid, "gridlines": false, "yTitle": "Units", "yAxis": { "max": 20 } })).unwrap();
        let st = back.doc().unwrap();
        let ShapeKind::Chart(model) = &st.shape(sid(&back, &rid)).unwrap().kind else { panic!("a chart") };
        let y = model.fmt.axis(model.main_axes().unwrap().1).unwrap();
        assert!(y.major_grid.is_none(), "the gridlines go from the axis it is drawn with");
        assert_eq!((y.title.as_deref(), y.max), (Some("Units"), Some(20.0)));
        let bytes = file::save_bytes(&back.doc().unwrap().doc, "pptx").unwrap();
        let mut third = Session::new();
        file::add_opened(&mut third, "g.pptx", None, file::open_slide_file("g.pptx", &bytes).unwrap());
        let d = third.execute("drawing.describe", &json!({})).unwrap();
        let again: kition_office_core::chart::ChartSpec =
            serde_json::from_value(d["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "chart").unwrap()["chart"].clone()).unwrap();
        assert_eq!((again.gridlines, again.y_title.as_deref(), again.y_axis.max, again.y_axis.major), (false, Some("Units"), Some(20.0), Some(2.0)));
    }

    #[test]
    fn a_plain_insert_looks_like_shape_insert() {
        let mut s = session();
        let a = s.execute("drawing.insert", &json!({ "text": "Hi" })).unwrap()["id"].as_str().unwrap().to_string();
        let b = s.execute("shape.insert", &json!({ "preset": "rect", "text": "Hi" })).unwrap();
        let st = s.doc().unwrap();
        let (sa, sb) = (st.shape(sid(&s, &a)).unwrap(), st.shape(ShapeId(b["id"].as_u64().unwrap() as u32)).unwrap());
        assert_eq!(sa.style, sb.style);
        assert_eq!(sa.text.as_ref().unwrap().paragraphs[0].props.align, Some(Align::Center));
    }

    #[test]
    fn replacing_text_keeps_the_runs_look() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", &json!({ "kind": "textBox", "paragraphs": [{ "runs": [{ "text": "Big", "size": 40, "bold": true }] }] }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.execute("drawing.text", &json!({ "id": id, "text": "Still big" })).unwrap();
        let st = s.doc().unwrap();
        let sh = st.shape(sid(&s, &id)).unwrap();
        let run = &sh.text.as_ref().unwrap().paragraphs[0].runs[0];
        assert_eq!((run.text.as_str(), run.props.size, run.props.bold), ("Still big", Some(40.0), Some(true)));
        assert!(sh.text_box);
    }

    #[test]
    fn reconnect_moves_an_end_to_another_shape_and_glues_a_plain_line() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64, y: f64| -> String {
            s.execute("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, c) = (ins(&mut s, "rect", 50.0, 50.0), ins(&mut s, "rect", 500.0, 300.0), ins(&mut s, "rect", 50.0, 400.0));
        let k = s.execute("drawing.connect", &json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        let describe = |s: &mut Session, id: &str| -> DrawingObject {
            serde_json::from_value(s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0].clone()).unwrap()
        };
        s.execute("drawing.reconnect", &json!({ "id": k, "end": "end", "to": c, "site": 0 })).unwrap();
        let o = describe(&mut s, &k);
        assert_eq!(o.connection.as_ref().unwrap().end.as_ref().unwrap().id, c);
        let site = kition_office_core::connect::sites(&describe(&mut s, &c))[0];
        assert!((kition_office_core::connect::line_ends(&o.frame).1 - site).hypot() < 1.0, "{:?} {site:?}", o.frame);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(describe(&mut s, &k).connection.unwrap().end.unwrap().id, b);
        // A plain line glued at both ends is a connector, and stays one through saving.
        let line = ins(&mut s, "line", 300.0, 200.0);
        s.execute("drawing.reconnect", &json!({ "id": line, "end": "start", "to": a, "site": 3 })).unwrap();
        s.execute("drawing.reconnect", &json!({ "id": line, "end": "end", "to": c, "site": 1 })).unwrap();
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "r.pptx", None, file::open_slide_file("r.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        let glued =
            d["objects"].as_array().unwrap().iter().filter(|o| o["connection"]["start"].is_object() && o["connection"]["end"].is_object()).count();
        assert_eq!(glued, 2, "{d}");
    }

    #[test]
    fn art_text_warps_are_set_saved_and_removed() {
        let mut s = session();
        let id = s.execute("drawing.insert", &json!({ "kind": "textBox", "text": "Art text", "warp": "textArchUp" })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let warp = |s: &Session, id: &str| s.doc().unwrap().shape(sid(s, id)).unwrap().text.as_ref().unwrap().body.warp.clone();
        assert_eq!(warp(&s, &id).as_deref(), Some("textArchUp"));
        s.execute("drawing.text", &json!({ "id": id, "warp": "textWave1" })).unwrap();
        let d = s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap();
        assert_eq!(
            (d["objects"][0]["text"]["warp"]["preset"].as_str(), d["objects"][0]["text"]["paragraphs"][0]["runs"][0]["text"].as_str()),
            (Some("textWave1"), Some("Art text"))
        );
        // Adjust values are kept and saved.
        s.execute("drawing.text", &json!({ "id": id, "warp": { "preset": "textWave1", "adjust": { "adj1": 20000 } } })).unwrap();
        assert_eq!(s.doc().unwrap().shape(sid(&s, &id)).unwrap().text.as_ref().unwrap().body.warp_adjust, vec![("adj1".to_string(), 20000.0)]);
        // It survives saving and reopening.
        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "w.pptx", None, file::open_slide_file("w.pptx", &bytes).unwrap());
        let d = back.execute("drawing.describe", &json!({})).unwrap();
        assert!(
            d["objects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["text"]["warp"]["preset"] == "textWave1" && o["text"]["warp"]["adjust"]["adj1"] == 20000.0),
            "{d}"
        );
        s.execute("drawing.text", &json!({ "id": id, "warp": null })).unwrap();
        assert_eq!(warp(&s, &id), None);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(warp(&s, &id).as_deref(), Some("textWave1"));
    }

    #[test]
    fn many_objects_move_in_one_step_and_failures_change_nothing() {
        let mut s = session();
        let a = s.execute("drawing.insert", &json!({ "frame": { "x": 0, "y": 0, "w": 10, "h": 10 } })).unwrap()["id"].as_str().unwrap().to_string();
        let b = s.execute("drawing.insert", &json!({ "frame": { "x": 50, "y": 0, "w": 10, "h": 10 } })).unwrap()["id"].as_str().unwrap().to_string();
        let undo = s.doc().unwrap().history.undo.len();
        s.execute("drawing.transform", &json!({ "ids": [a, b], "dy": 5 })).unwrap();
        assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1);
        let d = s.execute("drawing.describe", &json!({ "ids": [a, b] })).unwrap();
        assert_eq!(d["selection"]["frame"], json!({ "mixed": true }));
        // An unknown id fails the whole command.
        let before = s.doc().unwrap().doc.clone();
        let e = s.execute("drawing.transform", &json!({ "ids": [a, "999999"], "dx": 1 })).unwrap_err();
        assert!(e.to_string().contains("not-found"), "{e}");
        assert_eq!(s.doc().unwrap().doc, before);
        let e = s.execute("drawing.format", &json!({ "ids": [a], "geometry": { "preset": "noSuch" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported (geometry.preset)"), "{e}");
        s.execute("drawing.delete", &json!({ "ids": [b] })).unwrap();
        assert!(local_id(s.doc().unwrap(), &b).is_none());
    }

    #[test]
    fn a_commenter_cannot_edit() {
        let mut s = session();
        let a = s.execute("drawing.insert", &json!({})).unwrap()["id"].as_str().unwrap().to_string();
        s.set_comment_only(Some("me".into()));
        let e = s.execute("drawing.transform", &json!({ "ids": [a], "dx": 1 })).unwrap_err();
        assert!(e.to_string().contains("read-only"), "{e}");
        assert!(s.execute("drawing.describe", &json!({ "ids": [a] })).is_ok());
    }

    #[test]
    fn an_inherited_placeholder_is_edited_on_its_slide_and_follows_the_theme() {
        let mut s = session();
        // The title slide's title: its box and look come from its layout and the theme.
        let st = s.doc().unwrap();
        let title = st.shapes().iter().find(|sh| sh.ph.is_some() && sh.xfrm.is_none()).expect("an inherited placeholder").id;
        let layouts_before = st.doc.masters.clone();
        let id = object_id(st, title);
        let inherited = s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"][0]["frame"].clone();
        assert!(inherited["w"].as_f64().unwrap() > 0.0, "the layout's box: {inherited}");
        // Moving it and giving it a theme fill change only this slide's shape.
        s.execute("drawing.transform", &json!({ "ids": [id], "dy": 20 })).unwrap();
        s.execute("drawing.format", &json!({ "ids": [id], "fill": { "color": "accent2" } })).unwrap();
        let st = s.doc().unwrap();
        assert_eq!(st.doc.masters, layouts_before, "the layout and master are untouched");
        let sh = st.shape(title).unwrap();
        assert_eq!(sh.xfrm.map(|x| x.y), Some(inherited["y"].as_f64().unwrap() + 20.0), "the box is now the slide's own");
        assert!(sh.ph.is_some(), "still a placeholder");
        // The fill stays a theme reference, so another theme recolours it.
        let o = s.execute("drawing.describe", &json!({ "ids": [id] })).unwrap();
        assert_eq!(o["objects"][0]["fill"]["color"]["theme"], "accent2");
    }
}
