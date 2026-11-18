//! Presence (Kit UI protocol section 7e): the person's selection as the KDM object ids every
//! replica shares, and where the shapes other people selected are on the current slide. Engine
//! shape ids are local to a view, so only object ids travel between people.

use std::collections::HashSet;

use kdm_model::kition_kit::kdm::ObjectId;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, Target};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "presence.selection",
            "Selection for Presence",
            [],
            None,
            "{} → {slide: object | null, objects: [object]} (the current slide's and the selected shapes' KDM object ids; none before the document has a replica)",
            has_doc,
            selection
        ),
        cmd!(
            query "presence.locate",
            "Locate Objects",
            [],
            None,
            "{ids: [object]} → [{id, x, y, width, height}] (the shapes of the current slide among ids, in slide points)",
            has_doc,
            locate
        ),
    ]
}

/// The current slide's object, when the person edits slides and the document has a replica.
fn current_slide(s: &Session) -> Result<Option<ObjectId>> {
    let st = s.doc()?;
    if st.selection.target != Target::Slides {
        return Ok(None);
    }
    let (Some(kdm), Some(slide)) = (st.kdm(), st.doc.slides.get(st.selection.slide)) else { return Ok(None) };
    Ok(kdm.model().ids().slides.object(slide.id).cloned())
}

fn selection(s: &mut Session, _p: &Value) -> Result<Value> {
    let Some(slide) = current_slide(s)? else { return Ok(json!({ "slide": null, "objects": [] })) };
    let st = s.doc()?;
    let ids = st.kdm().map(|kdm| kdm.model().ids());
    let objects: Vec<String> = st
        .selection
        .shapes
        .iter()
        .filter_map(|shape| ids.and_then(|ids| ids.shape(&slide, *shape)).map(|object| object.as_str().to_owned()))
        .collect();
    Ok(json!({ "slide": slide.as_str(), "objects": objects }))
}

fn locate(s: &mut Session, p: &Value) -> Result<Value> {
    let wanted: HashSet<&str> = p.get("ids").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let Some(slide) = current_slide(s)? else { return Ok(json!([])) };
    let st = s.doc()?;
    let (Some(kdm), Some(current)) = (st.kdm(), st.doc.slides.get(st.selection.slide)) else { return Ok(json!([])) };
    let ids = kdm.model().ids();
    let mut out = Vec::new();
    kition_slide_model::walk(&current.shapes, &mut |shape, _| {
        let Some(object) = ids.shape(&slide, shape.id) else { return };
        if !wanted.contains(object.as_str()) {
            return;
        }
        let x = xfrm_of(&st.doc, &st.selection, shape);
        out.push(json!({ "id": object.as_str(), "x": x.x, "y": x.y, "width": x.w, "height": x.h }));
    });
    Ok(Value::Array(out))
}
