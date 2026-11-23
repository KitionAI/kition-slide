//! [`normalize`]: an id-blind form of a [`SlideFile`], for comparing views. Local ids are minted
//! by every read (never replicated), so two views of the same content — a round trip, or two
//! replicas that converged — almost never share them; nor are a view's own view properties.

use std::collections::HashMap;
use std::sync::Arc;

use kition_slide_model::SlideFile;
use serde_json::Value;

use crate::refs::{Fix, Ref, each_comment_shape, each_slide_list, walk};

/// An id-blind copy of `p`: every slide, layout, master, media and shape id renumbered from 1 in
/// document order (shapes per slide, layout or master), every reference to one rewritten to match,
/// `next_id` zeroed, and the view properties (the view's own, never replicated) left out. A
/// reference to nothing keeps its number. Two slides with the same
/// content normalize to the same value whatever ids either holds. Shape ids inside the notes and
/// handout masters are left as they are (they are that JSON's own business, written verbatim).
pub fn normalize(p: &SlideFile) -> SlideFile {
    let mut pool: Vec<_> = p.media.iter().collect();
    pool.sort_by(|a, b| (&a.name, &a.content_type, &a.link).cmp(&(&b.name, &b.content_type, &b.link)));
    let Ok(mut v) = serde_json::to_value(p) else { return p.clone() };
    // A shared pool has no stacking order; imports and concurrent inserts discover it differently.
    if let Some(Value::Array(media)) = v.get_mut("media") {
        media.sort_by_cached_key(|item| {
            (
                item.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                item.get("contentType").and_then(Value::as_str).unwrap_or_default().to_owned(),
                item.get("link").and_then(Value::as_str).map(str::to_owned),
            )
        });
    }
    let mut next = 0u32;
    let mut fresh = || {
        next += 1;
        next
    };
    let mut masters = HashMap::new();
    let mut layouts = HashMap::new();
    let mut slides = HashMap::new();
    let mut media = HashMap::new();
    let number = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    for m in array(&v, "masters") {
        if let Some(id) = m.get("id").and_then(number) {
            masters.entry(id).or_insert_with(&mut fresh);
        }
        for l in array(m, "layouts") {
            if let Some(id) = l.get("id").and_then(number) {
                layouts.entry(id).or_insert_with(&mut fresh);
            }
        }
    }
    for s in array(&v, "slides") {
        if let Some(id) = s.get("id").and_then(number) {
            slides.entry(id).or_insert_with(&mut fresh);
        }
    }
    for m in array(&v, "media") {
        if let Some(id) = m.get("id").and_then(number) {
            media.entry(id).or_insert_with(&mut fresh);
        }
    }
    let globals = Globals { slides: &slides, media: &media };

    if let Some(map) = v.as_object_mut() {
        map.insert("nextId".into(), Value::from(0));
        map.remove("viewProps");
        for key in ["notesMaster", "handoutMaster", "defaultTextStyle"] {
            if let Some(x) = map.get_mut(key) {
                globals.refs(x, None);
            }
        }
        for key in ["sections", "customShows"] {
            if let Some(x) = map.get_mut(key) {
                each_slide_list(x, &mut |list| {
                    if let Some(items) = list.as_array_mut() {
                        for item in items {
                            remap(item, &slides);
                        }
                    }
                });
            }
        }
        if let Some(Value::Array(fonts)) = map.get_mut("embeddedFonts") {
            for font in fonts {
                if let Some(id) = font.get_mut(1) {
                    remap(id, &media);
                }
            }
        }
        if let Some(Value::Array(items)) = map.get_mut("media") {
            for item in items {
                if let Some(id) = item.get_mut("id") {
                    remap(id, &media);
                }
            }
        }
        if let Some(Value::Array(items)) = map.get_mut("masters") {
            for m in items {
                let scope = number_shapes(m.get_mut("shapes"), &mut fresh);
                if let Some(id) = m.get_mut("id") {
                    remap(id, &masters);
                }
                if let Some(Value::Array(ls)) = m.get_mut("layouts") {
                    for l in ls {
                        let scope = number_shapes(l.get_mut("shapes"), &mut fresh);
                        if let Some(id) = l.get_mut("id") {
                            remap(id, &layouts);
                        }
                        globals.refs(l, Some(&scope));
                    }
                }
                // The master's own shapes and fields (its layouts are done, and hold no number of
                // the master's scope a second pass could touch: they were renumbered already).
                for key in ["shapes", "theme", "background", "titleStyle", "bodyStyle", "otherStyle"] {
                    if let Some(x) = m.get_mut(key) {
                        globals.refs(x, Some(&scope));
                    }
                }
            }
        }
        if let Some(Value::Array(items)) = map.get_mut("slides") {
            for s in items {
                let scope = number_shapes(s.get_mut("shapes"), &mut fresh);
                if let Some(id) = s.get_mut("id") {
                    remap(id, &slides);
                }
                if let Some(id) = s.get_mut("layout") {
                    remap(id, &layouts);
                }
                if let Some(Value::Array(animations)) = s.get_mut("animations") {
                    for a in animations {
                        for key in ["shape", "trigger"] {
                            if let Some(id) = a.get_mut(key) {
                                remap(id, &scope);
                            }
                        }
                    }
                }
                if let Some(comments) = s.get_mut("comments") {
                    each_comment_shape(comments, &mut |id| remap(id, &scope), 0);
                    // Comment ids are object ids (minted, never compared), and a replica reads a
                    // slide's threads oldest first.
                    if let Value::Array(items) = comments {
                        for item in items.iter_mut() {
                            if let Value::Object(map) = item {
                                map.remove("id");
                                map.remove("head");
                                if let Some(Value::Array(replies)) = map.get_mut("replies") {
                                    for reply in replies {
                                        if let Value::Object(reply) = reply {
                                            reply.remove("id");
                                        }
                                    }
                                }
                            }
                        }
                        items.sort_by_cached_key(|item| {
                            let text = |key: &str| item.get(key).map(Value::to_string).unwrap_or_default();
                            (text("date"), text("author"), text("text"), item.to_string())
                        });
                    }
                }
                // A kept timing tree's shapes (its `spid`s are the file's, not ids).
                if let Some(Value::Array(targets)) = s.get_mut("timing").and_then(|t| t.get_mut("targets")) {
                    for t in targets {
                        if let Some(id) = t.get_mut("shape") {
                            remap(id, &scope);
                        }
                    }
                }
                globals.refs(s, Some(&scope));
            }
        }
    }
    let Ok(mut out) = serde_json::from_value::<SlideFile>(v) else { return p.clone() };
    // Media bytes are not serialized: carry them over in the canonical pool order.
    for (item, original) in out.media.iter_mut().zip(pool) {
        item.data = Arc::clone(&original.data);
    }
    out
}

struct Globals<'a> {
    slides: &'a HashMap<u32, u32>,
    media: &'a HashMap<u32, u32>,
}

impl Globals<'_> {
    fn refs(&self, value: &mut Value, scope: Option<&HashMap<u32, u32>>) {
        walk(value, scope.is_some(), &mut |kind, v| {
            let Some(n) = v.as_u64().and_then(|n| u32::try_from(n).ok()) else { return Fix::Keep };
            let map = match kind {
                Ref::Media => Some(self.media),
                Ref::Slide => Some(self.slides),
                Ref::Shape => scope,
            };
            map.and_then(|m| m.get(&n)).map_or(Fix::Keep, |k| Fix::Set(Value::from(*k)))
        });
    }
}

fn array<'v>(v: &'v Value, key: &str) -> &'v [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn remap(id: &mut Value, map: &HashMap<u32, u32>) {
    if let Some(n) = id.as_u64().and_then(|n| u32::try_from(n).ok())
        && let Some(k) = map.get(&n)
    {
        *id = Value::from(*k);
    }
}

/// Renumbers the shapes of one shape tree (depth first), answering old id -> new id.
fn number_shapes(shapes: Option<&mut Value>, fresh: &mut dyn FnMut() -> u32) -> HashMap<u32, u32> {
    fn rec(shapes: &mut Value, map: &mut HashMap<u32, u32>, fresh: &mut dyn FnMut() -> u32, depth: usize) {
        let Some(items) = shapes.as_array_mut() else { return };
        if depth > 64 {
            return;
        }
        for shape in items {
            if let Some(id) = shape.get_mut("id")
                && let Some(n) = id.as_u64().and_then(|n| u32::try_from(n).ok())
            {
                let k = *map.entry(n).or_insert_with(&mut *fresh);
                *id = Value::from(k);
            }
            if let Some(children) = shape.get_mut("kind").and_then(|k| k.get_mut("children")) {
                rec(children, map, fresh, depth + 1);
            }
        }
    }
    let mut map = HashMap::new();
    if let Some(shapes) = shapes {
        rec(shapes, &mut map, fresh, 0);
    }
    map
}
