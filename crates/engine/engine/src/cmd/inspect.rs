//! Inspection for agents: the slide file, slides and shapes as JSON (verify work without screenshots).

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::{Shape, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "file.inspect", "Inspect slide", [], None, "{} → slides with titles, layouts, shape counts; selection; size; theme", has_doc, document),
        cmd!(query "slide.inspect", "Inspect Slide", [], None, "{index?, select?: bool (false: a person's read leaves the current slide; an agent's read always does)} → the slide's shapes with ids, kinds, boxes, text, fills, animations, transition, notes", has_doc, slide),
        cmd!(query "shape.inspect", "Inspect Shape", [], None, "{id} → full shape JSON plus its effective box", has_doc, shape),
        cmd!(query "command.list", "List Commands", [], None, "{filter?: substring} → [{id, label, params, enabled}]", always, commands),
    ]
}

fn document(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let d = &st.doc;
    let slides: Vec<Value> = d
        .slides
        .iter()
        .enumerate()
        .map(|(i, sl)| {
            let layout = d.layout(sl.layout).map(|(_, l)| l.name.clone()).unwrap_or_default();
            json!({"index": i, "id": sl.id, "toolId": slide_tool_id(sl.id), "title": sl.title(), "layout": layout, "shapes": sl.shapes.len(), "hidden": sl.hidden, "transition": sl.transition.as_ref().map(|t| t.kind.clone()), "animations": sl.animations.len(), "notes": sl.notes_text()})
        })
        .collect();
    let m = d.masters.first();
    Ok(json!({
        "title": st.title(),
        "path": st.path,
        "dirty": st.is_dirty(),
        "slideSize": [d.slide_size.width, d.slide_size.height],
        "slides": slides,
        "sections": d.sections.iter().map(|x| json!({"name": x.name, "slides": x.slides.len()})).collect::<Vec<_>>(),
        "theme": m.map(|m| m.theme.name.clone()),
        "fonts": m.map(|m| json!({"major": m.theme.fonts.major.latin, "minor": m.theme.fonts.minor.latin})),
        "layouts": m.map(|m| m.layouts.iter().map(|l| json!({"id": l.id, "name": l.name})).collect::<Vec<_>>()),
        "selection": serde_json::to_value(&st.selection).unwrap_or_default(),
        "media": d.media.iter().map(|x| json!({"id": x.id, "name": x.name, "type": x.content_type, "bytes": x.data.len()})).collect::<Vec<_>>(),
        "undo": st.history.undo.last().map(|e| e.label.clone()),
        "redo": st.history.redo.last().map(|e| e.label.clone()),
    }))
}

pub fn shape_summary(ctx: Option<&Ctx>, sh: &Shape, depth: usize) -> Value {
    let x = ctx.map(|c| resolve::xfrm(c, sh)).or(sh.xfrm).unwrap_or_default();
    summary_at(ctx, sh, x, depth)
}

/// [`shape_summary`] with the shape's box `x` on the slide: a group's children are listed with
/// their boxes on the slide (mapped through the group's child space), so they follow the group
/// when it is moved or resized.
fn summary_at(ctx: Option<&Ctx>, sh: &Shape, x: kition_slide_geom::Xfrm, depth: usize) -> Value {
    let fill = ctx.map(|c| {
        let (f, ph) = resolve::fill(c, sh);
        match f {
            Some(kition_slide_model::Fill::Solid { color }) => json!(format!("#{}", c.color(&color, ph).hex())),
            Some(kition_slide_model::Fill::None) | None => json!("none"),
            Some(other) => json!(serde_json::to_value(&other).ok().and_then(|v| v.get("kind").cloned()).unwrap_or_default()),
        }
    });
    let mut v = json!({
        "id": sh.id,
        "name": sh.name,
        "kind": sh.kind_name(),
        "box": [round(x.x), round(x.y), round(x.w), round(x.h)],
        "rotation": x.rot,
    });
    if let Some(o) = v.as_object_mut() {
        if let Some(p) = &sh.ph {
            o.insert("placeholder".into(), json!(p.kind.xml()));
        }
        if let Some(n) = sh.geom.preset_name()
            && matches!(sh.kind, ShapeKind::Shape | ShapeKind::Connector { .. })
        {
            o.insert("preset".into(), json!(n));
        }
        if let Some(t) = &sh.text
            && !t.is_empty()
        {
            o.insert("text".into(), json!(t.text()));
        }
        if let Some(f) = fill {
            o.insert("fill".into(), f);
        }
        if sh.hidden {
            o.insert("hidden".into(), json!(true));
        }
        if !sh.descr.is_empty() {
            o.insert("altText".into(), json!(sh.descr));
        }
        match &sh.kind {
            ShapeKind::Table(t) => {
                o.insert("table".into(), json!({"rows": t.n_rows(), "cols": t.n_cols(), "style": t.style, "widths": t.cols, "heights": t.rows.iter().map(|r| r.height).collect::<Vec<_>>(), "cells": t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.text()).collect::<Vec<_>>()).collect::<Vec<_>>()}));
            }
            ShapeKind::Chart(c) => {
                o.insert("chart".into(), json!({"type": c.kind.label(), "title": c.title, "categories": c.categories, "series": c.series.iter().map(|s| json!({"name": s.name, "values": s.values})).collect::<Vec<_>>()}));
            }
            ShapeKind::Group { children, child } if depth < 8 => {
                let list = children
                    .iter()
                    .map(|c| summary_at(ctx, c, super::arrange::child_to_parent(&x, child, &c.xfrm.unwrap_or_default()), depth + 1))
                    .collect();
                o.insert("children".into(), Value::Array(list));
            }
            ShapeKind::Media(m) => {
                o.insert("media".into(), json!({"id": m.media, "video": m.video, "autoplay": m.autoplay, "loop": m.loop_play}));
            }
            _ => {}
        }
    }
    v
}

fn add_refs(st: &crate::DocState, v: &mut Value) {
    if let Some(id) = v.get("id").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()) {
        let r = super::drawing::object_id(st, kition_slide_model::ShapeId(id));
        if r != id.to_string() {
            v["ref"] = json!(r);
        }
    }
    if let Some(children) = v.get_mut("children").and_then(Value::as_array_mut) {
        for c in children {
            add_refs(st, c);
        }
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn slide(s: &mut Session, p: &Value) -> Result<Value> {
    // A person looking at a slide makes it the slide being edited (`select: false` reads only, as
    // the editor reads for its own display). An agent's read never moves the editor: it names
    // the slide its later calls act on itself (`onSlide`, a run's `target` or `at.part`).
    let select = p.get("select").and_then(Value::as_bool).unwrap_or(true) && !s.acts_as_agent();
    if select
        && let Some(i) = usize_param(p, "index")
        && let Some(d) = s.active_mut()
        && i < d.doc.slides.len()
        && (d.selection.slide != i || !matches!(d.selection.target, crate::Target::Slides))
    {
        d.selection.slide = i;
        d.selection.target = crate::Target::Slides;
        d.selection.shapes.clear();
        d.selection.text = None;
        d.selection.cells = None;
    }
    let st = s.doc()?;
    let i = usize_param(p, "index").unwrap_or(st.selection.slide);
    let sl = st.doc.slides.get(i).ok_or_else(|| bad("slide.inspect", format!("no slide {i}")))?;
    let ctx = Ctx::for_slide(&st.doc, sl);
    let mut shapes: Vec<Value> = sl.shapes.iter().map(|sh| shape_summary(ctx.as_ref(), sh, 0)).collect();
    for v in &mut shapes {
        add_tool_ids(v, sl);
    }
    // The object-model id of each shape (`ref`), for the query and batch tools; known for the
    // slide being edited.
    if i == st.selection.slide && matches!(st.selection.target, crate::Target::Slides) {
        for v in &mut shapes {
            add_refs(st, v);
        }
    }
    let bg = ctx.as_ref().map(|c| match resolve::background(c, Some(sl)) {
        (kition_slide_model::Fill::Solid { color }, ph) => json!(format!("#{}", c.color(&color, ph).hex())),
        (f, _) => serde_json::to_value(&f).ok().and_then(|v| v.get("kind").cloned()).unwrap_or_default(),
    });
    Ok(json!({
        "index": i,
        "id": sl.id,
        "toolId": slide_tool_id(sl.id),
        "layout": st.doc.layout(sl.layout).map(|(_, l)| l.name.clone()),
        "title": sl.title(),
        "background": bg,
        "shapes": shapes,
        "transition": serde_json::to_value(&sl.transition).unwrap_or_default(),
        "animations": serde_json::to_value(&sl.animations).unwrap_or_default(),
        "notes": sl.notes_text(),
        "comments": sl.comments.len(),
        "hidden": sl.hidden,
        "design": crate::design::slide_advice(&st.doc, i),
    }))
}

/// A slide's tool id (KitObjectModel OM9): `slide:<id>`, the id the object model, receipts and the
/// premise guard name it by.
pub fn slide_tool_id(slide: kition_slide_model::SlideId) -> String {
    format!("slide:{}", slide.0)
}

/// The persistent tool id, independent of the shape's slide and numeric address.
pub fn shape_tool_id(shape: &kition_slide_model::Shape) -> String {
    format!("shape:{}", shape.identity)
}

/// Lists each shape's tool id (`toolId`) beside its number (`id`), its group's members too.
fn add_tool_ids(v: &mut Value, slide: &kition_slide_model::Slide) {
    if let Some(id) = v.get("id").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
        && let Some(shape) = slide.shape(ShapeId(id))
    {
        v["toolId"] = json!(shape_tool_id(shape));
    }
    if let Some(children) = v.get_mut("children").and_then(Value::as_array_mut) {
        for child in children {
            add_tool_ids(child, slide);
        }
    }
}

fn shape(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_or_selected(s, p, "shape.inspect", "missing `id`")?;
    let st = s.doc()?;
    let sh = st.shape(id).ok_or_else(|| bad("shape.inspect", format!("no shape {id}")))?;
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let mut value = serde_json::to_value(sh).unwrap_or_default();
    value["toolId"] = json!(shape_tool_id(sh));
    Ok(json!({"shape": value, "box": x}))
}

fn commands(s: &mut Session, p: &Value) -> Result<Value> {
    let f = str_param(p, "filter").unwrap_or("").to_ascii_lowercase();
    Ok(Value::Array(
        s.commands()
            .into_iter()
            .filter(|c| f.is_empty() || c.id.to_ascii_lowercase().contains(&f) || c.label.to_ascii_lowercase().contains(&f))
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect(),
    ))
}
