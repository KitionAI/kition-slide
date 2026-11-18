//! Defaults and ids of model-created objects, assigned in their creating transaction.

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::kdm::{ObjectId, Tx};
use kition_slide_model::{Shape, ShapeKind};
use serde_json::{Map, Value, json};

use crate::{KdmResult, kdm_err, root_id};

/// The types of the objects a deck stores only inside a container: the root's lists, an owner's
/// lists and owning references, a table's cells, a comment thread.
const STORED_TYPES: &[&str] =
    &["Master", "Layout", "Slide", "Shape", "TextBody", "Paragraph", "Table", "Row", "Col", "Animation", "Media", "Chart", "Comment"];

/// A batch that is saved. `existing` holds the stored objects the document had
/// before the batch: one the batch took out of every container (a raw `list.remove` without a
/// `list.insert` elsewhere) leaves the deck, with what it owns; one the batch created and placed
/// nowhere is refused.
pub fn settle_batch(
    tx: &mut Tx<'_>,
    floor: u32,
    blobs: &BTreeMap<String, u64>,
    missing: &BTreeMap<ObjectId, Value>,
    existing: &BTreeSet<ObjectId>,
) -> KdmResult<()> {
    drop_removed(tx, existing)?;
    settle(tx, floor, blobs, missing, true)
}

/// A raw operation prefix in an isolated, unsaveable premise plan. A creation may not have
/// reached its owning list yet, and an object taken out of its list may be inserted again by a
/// later operation; every other placement constraint still holds.
pub fn settle_premise_batch(
    tx: &mut Tx<'_>,
    floor: u32,
    blobs: &BTreeMap<String, u64>,
    missing: &BTreeMap<ObjectId, Value>,
    _existing: &BTreeSet<ObjectId>,
) -> KdmResult<()> {
    settle(tx, floor, blobs, missing, false)
}

/// The objects `replica` stores inside containers: what [`settle_batch`] takes as `existing`.
pub fn stored_objects(replica: &kition_kit::kdm::Replica) -> BTreeSet<ObjectId> {
    STORED_TYPES.iter().flat_map(|ty| replica.objects_of_type(ty)).collect()
}

/// Deletes the objects of `existing` that no container holds any more.
fn drop_removed(tx: &mut Tx<'_>, existing: &BTreeSet<ObjectId>) -> KdmResult<()> {
    let held = held(tx)?;
    for ty in STORED_TYPES {
        for object in tx.objects_of_type(ty) {
            if !held.contains(&object) && existing.contains(&object) {
                tx.delete(&object)?;
            }
        }
    }
    Ok(())
}

/// The objects a stored container holds: everything the root and the comment threads own.
fn held(tx: &Tx<'_>) -> KdmResult<BTreeSet<ObjectId>> {
    let mut held = BTreeSet::new();
    owned(tx, &root_id(), &mut held, 0)?;
    for thread in tx.objects_of_type("CommentThread") {
        owned(tx, &thread, &mut held, 0)?;
    }
    Ok(held)
}

fn settle(tx: &mut Tx<'_>, floor: u32, blobs: &BTreeMap<String, u64>, missing: &BTreeMap<ObjectId, Value>, rooted: bool) -> KdmResult<()> {
    let mut layouts_seen = BTreeSet::new();
    for layout in tx.objects_of_type("Layout") {
        let before = tx.get(&layout, "identity")?;
        let id = before
            .as_str()
            .and_then(kition_slide_model::identity::canonical)
            .unwrap_or_else(|| kition_slide_model::identity::from_seed(&format!("layout:{layout}")));
        if !layouts_seen.insert(id.clone()) {
            return Err(kdm_err("invalid placement: duplicate layout identity"));
        }
        if before != id {
            tx.set(&layout, "identity", json!(id))?;
        }
    }
    let slides = tx.list_items(&root_id(), "slides")?;
    let mut next = floor.max(255);
    for slide in &slides {
        next = next.max(tx.get(slide, "slideId")?.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0));
    }
    for object in tx.objects_of_type("Shape") {
        next = next.max(tx.get(&object, "address")?.get("number").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(0));
    }
    let first_layout = tx
        .list_items(&root_id(), "masters")?
        .into_iter()
        .find_map(|master| tx.list_items(&master, "layouts").ok()?.into_iter().find(|layout| tx.exists(layout)));
    // The shapes this batch moved from another slide, with that slide's id.
    let mut moved: BTreeMap<ObjectId, u64> = BTreeMap::new();
    // The shapes of slides: only they have a tool id (`key`); a layout's or master's shape is
    // identified by its object id.
    let mut on_slides: BTreeSet<ObjectId> = BTreeSet::new();
    for slide in slides {
        let id = match tx.get(&slide, "slideId")?.as_u64().and_then(|n| u32::try_from(n).ok()) {
            Some(id) => id,
            None => {
                let id = fresh(&mut next)?;
                tx.set(&slide, "slideId", json!(id))?;
                id
            }
        };
        let mut props = tx.get(&slide, "props")?.as_object().cloned().unwrap_or_default();
        let before = props.clone();
        for (key, value) in [("name", json!("")), ("hidden", json!(false)), ("showMasterShapes", json!(true)), ("fileId", json!(id))] {
            props.entry(key.to_owned()).or_insert(value);
        }
        if props != before {
            tx.set(&slide, "props", Value::Object(props))?;
        }
        if tx.get(&slide, "layout")?.is_null()
            && let Some(layout) = &first_layout
        {
            tx.set(&slide, "layout", json!(layout.as_str()))?;
        }
        if tx.get(&slide, "notesBody")?.is_null() {
            let body =
                tx.create("TextBody", Map::from_iter([("props".to_owned(), json!({"body": kition_slide_model::text::BodyProps::default()}))]))?;
            tx.set(&slide, "notesBody", json!(body.as_str()))?;
        }
        let mut objects = BTreeSet::new();
        collect_shapes(tx, &slide, "shapes", &mut objects, 0)?;
        on_slides.extend(objects.iter().cloned());
        let mut used = BTreeSet::new();
        for shape in &objects {
            let address = tx.get(shape, "address")?;
            if address["slide"] == id
                && let Some(number) = address["number"].as_u64().and_then(|n| u32::try_from(n).ok())
            {
                used.insert(number);
            }
        }
        for shape in objects {
            let address = tx.get(&shape, "address")?;
            if address["slide"] == id && address["number"].as_u64().is_some() {
                continue;
            }
            if let Some(from) = address["slide"].as_u64() {
                moved.insert(shape.clone(), from);
            }
            let number = match address["number"].as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n >= 2 && !used.contains(n)) {
                Some(number) => number,
                None => fresh(&mut next)?,
            };
            used.insert(number);
            tx.set(&shape, "address", json!({"slide": id, "number": number}))?;
        }
    }
    let mut identities = BTreeSet::new();
    for shape in tx.objects_of_type("Shape") {
        let before = tx.get(&shape, "key")?;
        if !on_slides.contains(&shape) {
            if !before.is_null() {
                tx.set(&shape, "key", Value::Null)?;
            }
        } else {
            settle_key(tx, &shape, &before, &mut identities)?;
        }
        if tx.get(&shape, "kind")?.is_null() {
            tx.set(&shape, "kind", json!("shape"))?;
        }
        if tx.map_entries(&shape, "look")?.is_empty() && tx.get(&shape, "props")?.is_null() && tx.get(&shape, "kind")? == "shape" {
            let default = Shape { kind: ShapeKind::Shape, ..Default::default() };
            let mut fields = serde_json::to_value(default).map_err(kdm_err)?.as_object().cloned().unwrap_or_default();
            for name in ["id", "name", "xfrm", "text"] {
                fields.remove(name);
            }
            for (name, value) in fields {
                tx.map_set(&shape, "look", &name, value)?;
            }
        }
    }
    for ty in ["Row", "Col"] {
        for object in tx.objects_of_type(ty) {
            if tx.get(&object, "identity")?.as_str().and_then(kition_slide_model::identity::canonical).is_none() {
                tx.set(&object, "identity", json!(kition_slide_model::identity::from_seed(&format!("{ty}:{}", object.as_str()))))?;
            }
        }
    }
    for table in tx.objects_of_type("Table") {
        for field in ["rows", "cols"] {
            let mut seen = BTreeSet::new();
            for object in tx.list_items(&table, field)? {
                let before = tx.get(&object, "identity")?;
                let mut id = before.as_str().and_then(kition_slide_model::identity::canonical).unwrap_or_default();
                let mut salt = 0u64;
                while id.is_empty() || !seen.insert(id.clone()) {
                    id = kition_slide_model::identity::from_seed(&format!("member:{object}/{salt}"));
                    salt = salt.saturating_add(1);
                }
                if before != id {
                    tx.set(&object, "identity", json!(id))?;
                }
            }
        }
    }
    carry_references(tx, &moved)?;
    validate_placement(tx, blobs, missing, rooted)?;
    for body in tx.objects_of_type("TextBody") {
        if tx.get(&body, "props")?.is_null() {
            tx.set(&body, "props", json!({"body": kition_slide_model::text::BodyProps::default()}))?;
        }
    }
    for paragraph in tx.objects_of_type("Paragraph") {
        if tx.get(&paragraph, "props")?.is_null() {
            tx.set(&paragraph, "props", json!({"level": 0}))?;
        }
    }
    Ok(())
}

/// A slide shape's tool id (`key`): its identity, a repeated or missing one (a shape the batch
/// created) derived from where the batch put it, its slide's id and its new number there (which
/// the batch's file ids determine), not drawn: the same batch on the same deck makes the same
/// ids, so a creation replays (kit-env) as the shape's old `<slide>/<n>` key did.
fn settle_key(tx: &mut Tx<'_>, shape: &ObjectId, before: &Value, identities: &mut BTreeSet<String>) -> KdmResult<()> {
    let identity = before.as_str().and_then(kition_slide_model::identity::canonical);
    let identity = match identity {
        Some(id) if identities.insert(id.clone()) => id,
        _ => {
            let address = tx.get(shape, "address")?;
            let seed = format!("created:{}/{}", address["slide"], address["number"]);
            let mut salt = 0u64;
            loop {
                let id = kition_slide_model::identity::from_seed(&format!("{seed}/{salt}"));
                if identities.insert(id.clone()) {
                    break id;
                }
                salt = salt.saturating_add(1);
            }
        }
    };
    if *before != identity {
        tx.set(shape, "key", json!(identity))?;
    }
    Ok(())
}

/// A shape a batch moved to another slide takes what its old slide holds about it along, as
/// `shape.move` does: the animations of it (a group's: of it or of what it holds) and the comment
/// threads anchored on it. What a batch creates or anchors itself is left as given, and checked.
fn carry_references(tx: &mut Tx<'_>, moved: &BTreeMap<ObjectId, u64>) -> KdmResult<()> {
    if moved.is_empty() {
        return Ok(());
    }
    let slides = tx.list_items(&root_id(), "slides")?;
    let mut shape_slides = BTreeMap::new();
    let mut by_id = BTreeMap::new();
    for slide in &slides {
        if let Some(id) = tx.get(slide, "slideId")?.as_u64() {
            by_id.insert(id, slide.clone());
        }
        let mut shapes = BTreeSet::new();
        collect_shapes(tx, slide, "shapes", &mut shapes, 0)?;
        for shape in shapes {
            shape_slides.insert(shape, slide.clone());
        }
    }
    // Each moved shape and what it holds, with the slide it left.
    let mut left: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
    for (shape, from) in moved {
        let Some(from) = by_id.get(from) else { continue };
        let mut held = BTreeSet::from([shape.clone()]);
        collect_shapes(tx, shape, "children", &mut held, 0)?;
        for object in held {
            left.insert(object, from.clone());
        }
    }
    for slide in &slides {
        let mut index = 0;
        while let Some(animation) = tx.list_items(slide, "animations")?.get(index).cloned() {
            let props = tx.get(&animation, "props")?;
            let target = props.get("shape").and_then(Value::as_str).map(ObjectId::from_string);
            let owner = target.as_ref().filter(|target| left.get(*target) == Some(slide)).and_then(|target| shape_slides.get(target));
            match owner {
                Some(owner) if owner != slide => {
                    // A new animation there, as `shape.move` and the saved file make it: an
                    // animation is its slide's, and has no identity of its own across slides.
                    let owner = owner.clone();
                    tx.list_remove(slide, "animations", index)?;
                    tx.delete(&animation)?;
                    let copy = tx.create("Animation", Map::from_iter([("props".to_owned(), props)]))?;
                    let end = tx.list_items(&owner, "animations")?.len();
                    tx.list_insert(&owner, "animations", end, &copy)?;
                }
                _ => index += 1,
            }
        }
    }
    for thread in tx.objects_of_type("CommentThread") {
        let mut anchor = tx.get(&thread, "anchor")?;
        let object = anchor.get("object").and_then(Value::as_str).map(ObjectId::from_string);
        let Some(object) = object else { continue };
        let was = anchor.get("slide").and_then(Value::as_str).map(ObjectId::from_string);
        if left.get(&object) != was.as_ref() {
            continue;
        }
        if let (Some(owner), Some(map)) = (shape_slides.get(&object), anchor.as_object_mut())
            && map.get("slide").and_then(Value::as_str) != Some(owner.as_str())
        {
            map.insert("slide".to_owned(), json!(owner.as_str()));
            tx.set(&thread, "anchor", anchor)?;
        }
    }
    Ok(())
}

/// Reject model placements which the file writer cannot represent, before issuing a receipt.
fn validate_placement(tx: &Tx<'_>, blobs: &BTreeMap<String, u64>, missing: &BTreeMap<ObjectId, Value>, rooted: bool) -> KdmResult<()> {
    for shape in tx.objects_of_type("Shape") {
        let kind = crate::look::member(tx, &shape, "kind")?;
        let kind = kind.get("type").and_then(Value::as_str).unwrap_or("shape");
        for (field, allowed) in [("children", "group"), ("body", "shape"), ("table", "table"), ("chart", "chart")] {
            let present = if field == "children" { !tx.list_items(&shape, field)?.is_empty() } else { !tx.get(&shape, field)?.is_null() };
            if present && kind != allowed {
                return Err(kdm_err(format!("invalid placement: {kind} cannot hold {field}")));
            }
        }
        if kind != "shape" && !tx.get(&shape, "text")?.is_null() {
            return Err(kdm_err("invalid placement: only ordinary shapes hold text"));
        }
    }
    for ty in ["Master", "Layout"] {
        for owner in tx.objects_of_type(ty) {
            collect_shapes(tx, &owner, "shapes", &mut BTreeSet::new(), 0)?;
        }
    }
    let slides = tx.list_items(&root_id(), "slides")?;
    let mut shape_slides = BTreeMap::new();
    for slide in &slides {
        let mut shapes = BTreeSet::new();
        collect_shapes(tx, slide, "shapes", &mut shapes, 0)?;
        for shape in &shapes {
            shape_slides.insert(shape.clone(), slide.clone());
        }
        for animation in tx.list_items(slide, "animations")? {
            let props = tx.get(&animation, "props")?;
            for field in ["shape", "trigger"] {
                let target = props.get(field).and_then(Value::as_str).map(ObjectId::from_string);
                if (field == "shape" || target.is_some()) && !target.is_some_and(|id| shapes.contains(&id)) {
                    return Err(kdm_err(format!("invalid placement: animation {field} is not on its slide")));
                }
            }
        }
    }
    for thread in tx.objects_of_type("CommentThread") {
        let anchor = tx.get(&thread, "anchor")?;
        let object = anchor.get("object").and_then(Value::as_str).map(ObjectId::from_string);
        let slide = anchor.get("slide").and_then(Value::as_str).map(ObjectId::from_string);
        let owner = object.as_ref().and_then(|object| if slides.contains(object) { Some(object) } else { shape_slides.get(object) });
        if owner.is_none() || owner != slide.as_ref() || tx.list_items(&thread, "comments")?.is_empty() {
            return Err(kdm_err("invalid placement: comment thread needs a slide anchor and a comment"));
        }
    }
    for table in tx.objects_of_type("Table") {
        let rows = tx.list_items(&table, "rows")?;
        let cols = tx.list_items(&table, "cols")?;
        for (key, cell) in tx.map_entries(&table, "cells")? {
            if !rows.iter().any(|row| cols.iter().any(|col| key == format!("{}:{}", row.as_str(), col.as_str()))) {
                return Err(kdm_err("invalid placement: cell is outside its table"));
            }
            if let Some(body) = cell.get("body").and_then(Value::as_str)
                && (!tx.exists(&ObjectId::from_string(body)) || tx.object_type(&ObjectId::from_string(body))? != "TextBody")
            {
                return Err(kdm_err("invalid placement: cell text body is missing"));
            }
        }
    }
    for media in tx.list_items(&root_id(), "media")? {
        let blob = tx.get(&media, "blob")?;
        let props = tx.get(&media, "props")?;
        if !blob.is_null() {
            let size = blob.get("hash").and_then(Value::as_str).and_then(|hash| blobs.get(hash)).copied();
            if size.is_none_or(|size| size == 0 || blob.get("size").and_then(Value::as_u64) != Some(size))
                || blob.get("mime").and_then(Value::as_str) != props.get("contentType").and_then(Value::as_str)
            {
                return Err(kdm_err("invalid placement: media blob bytes or metadata are unavailable"));
            }
        } else if props.get("link").and_then(Value::as_str).is_none_or(str::is_empty) && missing.get(&media) != Some(&props) {
            // Imported missing images already have a saved representation; unrelated edits keep it.
            return Err(kdm_err("invalid placement: media needs a blob or external link"));
        }
    }
    let held = held(tx)?;
    for ty in STORED_TYPES {
        if rooted && tx.objects_of_type(ty).iter().any(|object| !held.contains(object)) {
            return Err(kdm_err(format!("invalid placement: {ty} has no stored container")));
        }
    }
    Ok(())
}

fn owned(tx: &Tx<'_>, object: &ObjectId, held: &mut BTreeSet<ObjectId>, depth: usize) -> KdmResult<()> {
    if depth > 72 || !held.insert(object.clone()) {
        return Err(kdm_err("invalid placement: ownership cycle or repeated owner"));
    }
    let ty = tx.object_type(object)?;
    let lists: &[&str] = match ty.as_str() {
        "SlideFile" => &["masters", "slides", "media"],
        "Master" => &["layouts", "shapes"],
        "Layout" => &["shapes"],
        "Slide" => &["shapes", "animations"],
        "Shape" => &["children"],
        "Table" => &["rows", "cols"],
        "TextBody" => &["paragraphs"],
        "CommentThread" => &["comments"],
        _ => &[],
    };
    for field in lists {
        for item in tx.list_items(object, field)? {
            owned(tx, &item, held, depth + 1)?;
        }
    }
    let refs: &[&str] = match ty.as_str() {
        "Slide" => &["notesBody"],
        "Shape" => &["body", "table", "chart"],
        _ => &[],
    };
    for field in refs {
        if let Some(id) = tx.get(object, field)?.as_str() {
            owned(tx, &ObjectId::from_string(id), held, depth + 1)?;
        }
    }
    if ty == "Table" {
        for (_, cell) in tx.map_entries(object, "cells")? {
            if let Some(id) = cell.get("body").and_then(Value::as_str) {
                owned(tx, &ObjectId::from_string(id), held, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn collect_shapes(tx: &Tx<'_>, owner: &ObjectId, field: &str, found: &mut BTreeSet<ObjectId>, depth: usize) -> KdmResult<()> {
    if depth > 32 {
        return Err(kdm_err("invalid placement: shape nesting exceeds the file writer's 32 levels"));
    }
    for shape in tx.list_items(owner, field)? {
        if !found.insert(shape.clone()) {
            return Err(kdm_err("invalid placement: shape is listed twice"));
        }
        if !tx.list_items(&shape, "children")?.is_empty() {
            collect_shapes(tx, &shape, "children", found, depth + 1)?;
        }
    }
    Ok(())
}

fn fresh(next: &mut u32) -> KdmResult<u32> {
    *next = next.checked_add(1).filter(|n| *n < 1 << 30).ok_or_else(|| kdm_err("no file id remains"))?;
    Ok(*next)
}
