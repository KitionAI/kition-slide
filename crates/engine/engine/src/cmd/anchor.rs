//! Comment anchors (Kit UI protocol 2.6 section 7k, capability `comment-anchors`): a host that
//! keeps comment threads itself stores where each one points as `{ ids, range?, quote?, page? }`,
//! and this kit makes and places them.
//!
//! The ids are KDM object ids, the same on every replica of the slide file (`KitPointer 1.0` PT7):
//! the selected shapes, then their slide. Engine ids are local to a view, and a slide's index
//! changes when slides move, so neither goes in an anchor. A point on a slide is the slide's id
//! followed by `pt:<x>,<y>` (slide points), an id no object has. A text range is in the first
//! shape's text, in characters, one between paragraphs (as `text.get` counts them).
//!
//! Placing an anchor finds the slide it is on now (after moves and reorders) and, on that slide,
//! the first of its shapes that still exists; a deleted shape leaves the thread on its slide.

use std::collections::HashMap;

use kition_slide_model::Shape;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, Target};

/// The prefix of the id that holds a point on a slide.
pub const POINT_PREFIX: &str = "pt:";
/// The protocol's limits (UI-V3): ids per anchor, characters of a quote.
const IDS_LIMIT: usize = 256;
const QUOTE_LIMIT: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "comment.anchor",
            "Comment Anchor",
            [],
            None,
            "{x?, y?: slide points} → {ids, range?, quote?, page} | null (the selected text or shapes as a comment anchor, else the point, else the current slide; makes the replica if there is none; null outside the slides)",
            has_slide,
            anchor
        ),
        cmd!(
            query "comment.place",
            "Place Comment Anchors",
            [],
            None,
            "{anchors: [{ids, range?}]} → [{found, slide: index | null, rect: {x, y, width, height} | null, point: {x, y} | null}] (found: the first id still exists; slide and rect where its marker goes now, in slide points; nothing is found before the document has a replica, and placing never makes one)",
            has_doc,
            place
        ),
    ]
}

/// The point a `pt:<x>,<y>` id names.
pub fn parse_point(id: &str) -> Option<(f64, f64)> {
    let (x, y) = id.strip_prefix(POINT_PREFIX)?.split_once(',')?;
    let (x, y) = (x.parse::<f64>().ok()?, y.parse::<f64>().ok()?);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// Characters before `(paragraph, char)` in a body, one between paragraphs.
fn offset(body: &kition_slide_model::TextBody, (para, ch): (usize, usize)) -> usize {
    let before: usize = body.paragraphs.iter().take(para).map(|p| p.char_len() + 1).sum();
    before + ch
}

/// A new comment is the person's gesture, as the first edit is: the replica is made now if the
/// document has none yet (a read-only session's editor refuses that, and there is no anchor).
fn anchor(s: &mut Session, p: &Value) -> Result<Value> {
    if s.doc()?.selection.target != Target::Slides || s.active_kdm(false).is_err() {
        return Ok(Value::Null);
    }
    let st = s.doc()?;
    let Some(kdm) = st.kdm() else { return Ok(Value::Null) };
    let ids = kdm.model().ids();
    let index = st.selection.slide;
    let Some(slide) = st.doc.slides.get(index) else { return Ok(Value::Null) };
    let Some(slide_object) = ids.slides.object(slide.id).cloned() else { return Ok(Value::Null) };
    let shape_object = |id| ids.shape(&slide_object, id).map(|o| o.as_str().to_owned());

    // Selected text in a shape: the shape, its slide, and the range.
    if let Some(text) = st.selection.text.as_ref().filter(|t| t.is_range() && !t.notes)
        && let Some(object) = shape_object(text.shape)
        && let Some(body) = crate::cmd::text::body_of(st, text)
    {
        let (a, b) = (offset(body, text.anchor), offset(body, text.caret));
        let (start, end) = (a.min(b), a.max(b));
        let quote: String = kition_slide_model::edit::text_range(body, text.anchor, text.caret).chars().take(QUOTE_LIMIT).collect();
        let mut out = json!({ "ids": [object, slide_object.as_str()], "page": index });
        // A table cell's text is not the shape's: the anchor is the table.
        if text.cell.is_none() {
            out["range"] = json!({ "start": start, "end": end });
            out["quote"] = json!(quote);
        }
        return Ok(out);
    }
    let mut objects: Vec<String> = st.selection.shapes.iter().filter_map(|id| shape_object(*id)).take(IDS_LIMIT - 1).collect();
    if !objects.is_empty() {
        objects.push(slide_object.as_str().to_owned());
        return Ok(json!({ "ids": objects, "page": index }));
    }
    let point = (f64_param(p, "x"), f64_param(p, "y"));
    if let (Some(x), Some(y)) = point {
        return Ok(json!({ "ids": [slide_object.as_str(), format!("{POINT_PREFIX}{},{}", x.round(), y.round())], "page": index }));
    }
    Ok(json!({ "ids": [slide_object.as_str()], "page": index }))
}

fn place(s: &mut Session, p: &Value) -> Result<Value> {
    let anchors = p.get("anchors").and_then(Value::as_array).cloned().unwrap_or_default();
    // A host's threads name the replica's objects: without one there is nothing to find. Placing
    // never makes the replica (a viewer's session must not change; the host decides when it is made).
    let st = s.doc()?;
    let Some(kdm) = st.kdm() else {
        return Ok(Value::Array(anchors.iter().map(|_| json!({ "found": false, "slide": null, "rect": null, "point": null })).collect()));
    };
    let ids = kdm.model().ids();

    // Every slide's object, and every shape's (group members included), with its slide.
    let mut slides: HashMap<&str, usize> = HashMap::new();
    let mut shapes: HashMap<String, (usize, &Shape)> = HashMap::new();
    for (index, slide) in st.doc.slides.iter().enumerate() {
        let Some(object) = ids.slides.object(slide.id) else { continue };
        slides.insert(object.as_str(), index);
        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
            if let Some(shape_object) = ids.shape(object, shape.id) {
                shapes.insert(shape_object.as_str().to_owned(), (index, shape));
            }
        });
    }
    let rect_of = |index: usize, shape: &Shape| {
        let mut sel = st.selection.clone();
        sel.slide = index;
        sel.target = Target::Slides;
        let x = xfrm_of(&st.doc, &sel, shape);
        json!({ "x": x.x, "y": x.y, "width": x.w, "height": x.h })
    };

    let placed = anchors
        .iter()
        .map(|anchor| {
            let list: Vec<&str> =
                anchor.get("ids").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let found = list.first().is_some_and(|first| shapes.contains_key(*first) || slides.contains_key(first));
            // The first shape that still exists, else the slide.
            if let Some((index, shape)) = list.iter().find_map(|id| shapes.get(*id)) {
                return json!({ "found": found, "slide": index, "rect": rect_of(*index, shape), "point": null });
            }
            match list.iter().find_map(|id| slides.get(id)) {
                Some(index) => {
                    let point = list.iter().find_map(|id| parse_point(id)).map(|(x, y)| json!({ "x": x, "y": y }));
                    json!({ "found": found, "slide": index, "rect": null, "point": point })
                }
                None => json!({ "found": false, "slide": null, "rect": null, "point": null }),
            }
        })
        .collect();
    Ok(Value::Array(placed))
}
