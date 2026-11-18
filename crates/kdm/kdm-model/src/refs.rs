//! Ids stored as data, inside the JSON of a `json` register: written as the object ids they name,
//! read back as the reader's local ids.
//!
//! [`walk`] finds them by where the model's serde form puts them (`tests/id_fields.rs` lists every
//! model field that holds an id, so a new one fails a test until it is handled here or explicitly):
//! - a media id under the keys `media`, `poster`, `preview` and `sound` (a picture fill, a picture
//!   bullet, a media clip and its poster, an object's preview picture, a transition's or an
//!   animation's sound);
//! - a slide id at `slide` of a `{"kind": "slide"}` object (a hyperlink action);
//! - a shape id at the first element of `start`/`end` of a `{"type": "connector"}` object, scoped to
//!   the slide, layout or master holding the connector.
//!
//! Animations (`shape`, `trigger`), comments (`shape`), sections, custom shows and embedded fonts
//! are translated explicitly by their writers and readers, with the helpers below.
//!
//! A written id names its object as a JSON string; an id that names nothing (a file's dangling
//! reference) stays the number it was, on both sides. A string whose object the reader does not
//! hold (deleted by a merge) reads as "none": `null` where the field is optional, `0` (never a
//! minted id) where it is required.

use std::collections::HashMap;

use kition_kit::kdm::ObjectId;
use kition_slide_model::{MediaId, ShapeId, SlideId};
use serde_json::Value;

use crate::ids::Ids;

const MAX_DEPTH: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ref {
    Media,
    Slide,
    Shape,
}

pub(crate) enum Fix {
    Keep,
    Set(Value),
    /// The reference names nothing: `null` where optional, `0` where required.
    Clear,
}

fn scalar(value: &Value) -> bool {
    value.is_number() || value.is_string()
}

/// Calls `f` on every id stored as data in `value` (see the module docs) and applies its answer.
/// Connector ends are visited only when `connectors` is set (shape ids in a notes or handout
/// master's JSON are that JSON's own business).
pub(crate) fn walk(value: &mut Value, connectors: bool, f: &mut dyn FnMut(Ref, &Value) -> Fix) {
    walk_at(value, connectors, f, 0);
}

fn walk_at(value: &mut Value, connectors: bool, f: &mut dyn FnMut(Ref, &Value) -> Fix, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                walk_at(item, connectors, f, depth + 1);
            }
        }
        Value::Object(map) => {
            let slide_action = map.get("kind").and_then(Value::as_str) == Some("slide");
            let connector = connectors && map.get("type").and_then(Value::as_str) == Some("connector");
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    "media" | "poster" | "preview" | "sound" if scalar(child) => {
                        let cleared = if key == "media" { Value::from(0) } else { Value::Null };
                        apply(child, f(Ref::Media, child), cleared);
                    }
                    "slide" if slide_action && scalar(child) => apply(child, f(Ref::Slide, child), Value::from(0)),
                    "start" | "end" if connector => {
                        let fix = match child.as_array().and_then(|pair| pair.first()) {
                            Some(id) if scalar(id) => f(Ref::Shape, id),
                            _ => Fix::Keep,
                        };
                        match fix {
                            Fix::Keep => {}
                            Fix::Set(id) => {
                                if let Some(first) = child.as_array_mut().and_then(|pair| pair.first_mut()) {
                                    *first = id;
                                }
                            }
                            Fix::Clear => *child = Value::Null,
                        }
                    }
                    _ => walk_at(child, connectors, f, depth + 1),
                }
            }
        }
        _ => {}
    }
}

fn apply(slot: &mut Value, fix: Fix, cleared: Value) {
    match fix {
        Fix::Keep => {}
        Fix::Set(value) => *slot = value,
        Fix::Clear => *slot = cleared,
    }
}

fn local_number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn object_value(object: &ObjectId) -> Value {
    Value::String(object.as_str().to_owned())
}

/// Local ids to object ids, for a write. `scope`: the slide, layout or master holding `value`'s
/// connectors (`None`: leave connector ends alone).
pub(crate) fn to_objects(value: &mut Value, ids: &Ids, scope: Option<&ObjectId>) {
    walk(value, scope.is_some(), &mut |kind, v| {
        let Some(n) = local_number(v) else { return Fix::Keep };
        let object = match kind {
            Ref::Media => ids.media.object(MediaId(n)),
            Ref::Slide => ids.slides.object(SlideId(n)),
            Ref::Shape => scope.and_then(|owner| ids.shape(owner, ShapeId(n))),
        };
        object.map_or(Fix::Keep, |o| Fix::Set(object_value(o)))
    });
}

/// The object id of shape `id` in `scope`, or the number itself when it names nothing there.
pub(crate) fn shape_out(value: &mut Value, ids: &Ids, scope: &ObjectId) {
    if let Some(n) = local_number(value)
        && let Some(object) = ids.shape(scope, ShapeId(n))
    {
        *value = object_value(object);
    }
}

/// Object ids to the reader's local ids.
#[derive(Default)]
pub(crate) struct Locals {
    pub media: HashMap<ObjectId, MediaId>,
    pub slides: HashMap<ObjectId, SlideId>,
    pub shapes: HashMap<ObjectId, ShapeId>,
}

pub(crate) fn to_locals(value: &mut Value, locals: &Locals, connectors: bool) {
    walk(value, connectors, &mut |kind, v| {
        let Value::String(s) = v else { return Fix::Keep };
        let object = ObjectId::from_string(s.as_str());
        let local = match kind {
            Ref::Media => locals.media.get(&object).map(|m| m.0),
            Ref::Slide => locals.slides.get(&object).map(|s| s.0),
            Ref::Shape => locals.shapes.get(&object).map(|s| s.0),
        };
        local.map_or(Fix::Clear, |n| Fix::Set(Value::from(n)))
    });
}

/// A shape reference read back: `Some(number)` (a local id, or a kept dangling number), or `None`
/// when it names an object the reader does not hold.
pub(crate) fn shape_in(value: &Value, locals: &Locals) -> Option<Value> {
    match value {
        Value::String(s) => locals.shapes.get(&ObjectId::from_string(s.as_str())).map(|id| Value::from(id.0)),
        other => Some(other.clone()),
    }
}

/// A list of slide ids (a section's, a custom show's), written as object ids.
pub(crate) fn slides_out(list: &mut Value, ids: &Ids) {
    if let Some(items) = list.as_array_mut() {
        for item in items {
            if let Some(n) = local_number(item)
                && let Some(object) = ids.slides.object(SlideId(n))
            {
                *item = object_value(object);
            }
        }
    }
}

/// Reads a list of slide ids back, dropping the slides the reader does not hold.
pub(crate) fn slides_in(list: &mut Value, locals: &Locals) {
    if let Some(items) = list.as_array_mut() {
        items.retain_mut(|item| match item {
            Value::String(s) => match locals.slides.get(&ObjectId::from_string(s.as_str())) {
                Some(id) => {
                    *item = Value::from(id.0);
                    true
                }
                None => false,
            },
            _ => true,
        });
    }
}

/// `[{name, slides: [...]}, ...]` (sections, custom shows): every `slides` list through `f`.
pub(crate) fn each_slide_list(value: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    if let Some(items) = value.as_array_mut() {
        for item in items {
            if let Some(slides) = item.get_mut("slides") {
                f(slides);
            }
        }
    }
}

/// Comments (and their replies): `shape` through `f`.
pub(crate) fn each_comment_shape(value: &mut Value, f: &mut dyn FnMut(&mut Value), depth: usize) {
    if depth > 32 {
        return;
    }
    if let Some(items) = value.as_array_mut() {
        for item in items {
            if let Some(shape) = item.get_mut("shape")
                && !shape.is_null()
            {
                f(shape);
            }
            if let Some(replies) = item.get_mut("replies") {
                each_comment_shape(replies, f, depth + 1);
            }
        }
    }
}

/// `doc` with its media items renumbered by `map` (a local media id to the one it takes), and every
/// media id it stores as data with them: the keys [`walk`] finds and the embedded fonts. `None`
/// when the model's serde form does not carry `doc` exactly, so that no reference can be lost.
pub fn renumber_media(doc: &kition_slide_model::SlideFile, map: &HashMap<MediaId, MediaId>) -> Option<kition_slide_model::SlideFile> {
    let rewrite = |map: &HashMap<MediaId, MediaId>| -> Option<kition_slide_model::SlideFile> {
        let mut value = serde_json::to_value(doc).ok()?;
        walk(&mut value, true, &mut |kind, v| match (kind, local_number(v)) {
            (Ref::Media, Some(n)) => map.get(&MediaId(n)).map_or(Fix::Keep, |to| Fix::Set(Value::from(to.0))),
            _ => Fix::Keep,
        });
        let mut out: kition_slide_model::SlideFile = serde_json::from_value(value).ok()?;
        let number = |id: MediaId| map.get(&id).copied().unwrap_or(id);
        // The media list and the fonts hold ids outside `walk`'s keys; their bytes are not serialized.
        out.media = doc.media.iter().map(|item| kition_slide_model::MediaItem { id: number(item.id), ..item.clone() }).collect();
        out.embedded_fonts = doc.embedded_fonts.iter().map(|(name, id)| (name.clone(), number(*id))).collect();
        Some(out)
    };
    if rewrite(&HashMap::new()).as_ref() != Some(doc) {
        return None;
    }
    rewrite(map)
}
