//! Persistent shape identities. Numeric shape ids remain addresses within one part.

use std::collections::HashSet;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::{Shape, SlideFile};

/// Canonical short GUID spelling used in tool ids and the model.
pub fn canonical(value: &str) -> Option<String> {
    let value = value.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(value);
    let text: String = value.chars().filter(|c| *c != '-').collect();
    (text.len() == 32 && text.bytes().all(|c| c.is_ascii_hexdigit()) && text.bytes().any(|c| c != b'0')).then(|| text.to_ascii_lowercase())
}

/// A new member identity for an edit, unique across independently editing replicas.
pub fn fresh() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Whether every member has a distinct, canonicalizable stored identity.
pub fn distinct<'a>(items: impl IntoIterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    items.into_iter().all(|item| canonical(item).is_some_and(|id| seen.insert(id)))
}

/// A deterministic seed for an identityless import. A changed value cannot claim continuity
/// from its old positions. A value that cannot be represented gets a fresh seed, never a guess.
pub fn import_seed(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).map(|value| from_seed(&format!("import/{value}"))).unwrap_or_else(|_| fresh())
}

/// Canonicalize existing member identities, repairing missing/duplicate ones deterministically.
/// Stored identities are scoped to their owner. A regenerated import id does not prove continuity.
pub fn members<'a>(items: impl IntoIterator<Item = &'a mut String>, seed: &str) -> bool {
    let mut seen = HashSet::new();
    let mut changed = false;
    for (at, slot) in items.into_iter().enumerate() {
        let mut id = canonical(slot).unwrap_or_default();
        let mut salt = 0u64;
        while id.is_empty() || seen.contains(&id) {
            id = from_seed(&format!("member/{seed}/{at}/{salt}"));
            salt = salt.saturating_add(1);
        }
        seen.insert(id.clone());
        if *slot != id {
            *slot = id;
            changed = true;
        }
    }
    changed
}

/// A deterministic UUID from a namespaced seed; also used for CRDT-created shapes.
pub fn from_seed(seed: &str) -> String {
    let mut bytes = Sha256::digest(format!("kition-slide/shape/1/{seed}").as_bytes());
    if let Some(byte) = bytes.get_mut(6) {
        *byte = (*byte & 0x0f) | 0x50;
    }
    if let Some(byte) = bytes.get_mut(8) {
        *byte = (*byte & 0x3f) | 0x80;
    }
    bytes.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// The standard DrawingML extension writes GUIDs with braces and hyphens.
pub fn guid(identity: &str) -> Option<String> {
    let id = canonical(identity)?;
    Some(format!("{{{}-{}-{}-{}-{}}}", id.get(..8)?, id.get(8..12)?, id.get(12..16)?, id.get(16..20)?, id.get(20..)?).to_ascii_uppercase())
}

/// Assign identities to legacy shapes once, preserving unique imported GUIDs. Repeated GUIDs
/// (including copies made by other editors) are repaired in deterministic document order.
pub fn settle(doc: &mut SlideFile) -> bool {
    settle_with(doc, &mut || None, None)
}

/// [`settle`], giving a shape that has no identity yet (one an edit made, or a copy) the one
/// `fresh` draws: unique across replicas, where two replicas creating a shape at one place from
/// one snapshot would derive the same deterministic one. Repeated identities are still repaired
/// deterministically, so that every replica repairs them alike.
///
/// With `old` (the document before an edit), the slides and masters the edit left as they were
/// (shared with `old`) keep their identities as they are and are settled first: an edit changes
/// nothing it did not touch.
pub fn settle_with(doc: &mut SlideFile, fresh: &mut dyn FnMut() -> Option<String>, old: Option<&SlideFile>) -> bool {
    fn tree(shapes: &mut [Shape], scope: &str, seen: &mut HashSet<String>, fresh: &mut dyn FnMut() -> Option<String>, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let mut changed = false;
        for (index, shape) in shapes.iter_mut().enumerate() {
            let path = format!("{scope}/{index}/{}", shape.id.0);
            let mut id = canonical(&shape.identity).unwrap_or_default();
            if id.is_empty()
                && let Some(drawn) = fresh().as_deref().and_then(canonical).filter(|drawn| !seen.contains(drawn))
            {
                id = drawn;
            }
            if id.is_empty() || seen.contains(&id) {
                let seed = format!("{path}/{}/{}/{}", shape.identity, shape.name, shape.descr);
                id = from_seed(&seed);
                let mut salt = 0u64;
                while seen.contains(&id) {
                    salt = salt.saturating_add(1);
                    id = from_seed(&format!("{seed}/{salt}"));
                }
            }
            seen.insert(id.clone());
            if shape.identity != id {
                shape.identity = id;
                changed = true;
            }
            if let crate::ShapeKind::Table(table) = &mut shape.kind {
                changed |= table.settle_ids();
            }
            if let Some(children) = shape.children_mut() {
                changed |= tree(children, &path, seen, fresh, depth + 1);
            }
        }
        changed
    }
    fn record(shapes: &[Shape], seen: &mut HashSet<String>) {
        crate::walk(shapes, &mut |shape, _| {
            if let Some(id) = canonical(&shape.identity) {
                seen.insert(id);
            }
        });
    }
    let kept_slides: HashSet<*const crate::Slide> = old.map(|old| old.slides.iter().map(Arc::as_ptr).collect()).unwrap_or_default();
    let kept_masters: HashSet<*const crate::Master> = old.map(|old| old.masters.iter().map(Arc::as_ptr).collect()).unwrap_or_default();
    let mut seen = HashSet::new();
    for master in doc.masters.iter().filter(|master| kept_masters.contains(&Arc::as_ptr(master))) {
        record(&master.shapes, &mut seen);
        for layout in &master.layouts {
            record(&layout.shapes, &mut seen);
        }
    }
    for slide in doc.slides.iter().filter(|slide| kept_slides.contains(&Arc::as_ptr(slide))) {
        record(&slide.shapes, &mut seen);
    }
    let section_seed =
        if distinct(doc.sections.iter().map(|section| section.identity.as_str())) { "sections".to_owned() } else { import_seed(&doc.sections) };
    let mut changed = members(doc.sections.iter_mut().map(|section| &mut section.identity), &section_seed);
    for master in &mut doc.masters {
        if kept_masters.contains(&Arc::as_ptr(master)) {
            continue;
        }
        let mut copy = (**master).clone();
        let mut edited = tree(&mut copy.shapes, &format!("master:{}", copy.id.0), &mut seen, fresh, 0);
        for layout in &mut copy.layouts {
            edited |= tree(&mut layout.shapes, &format!("layout:{}", layout.id.0), &mut seen, fresh, 0);
        }
        if edited {
            *master = Arc::new(copy);
            changed = true;
        }
    }
    for slide in &mut doc.slides {
        if kept_slides.contains(&Arc::as_ptr(slide)) {
            continue;
        }
        let mut copy = (**slide).clone();
        if tree(&mut copy.shapes, &format!("slide:{}", copy.id.0), &mut seen, fresh, 0) {
            *slide = Arc::new(copy);
            changed = true;
        }
    }
    let layouts: Vec<_> = doc.masters.iter().flat_map(|master| &master.layouts).collect();
    let layout_seed = if distinct(layouts.iter().map(|layout| layout.identity.as_str())) { String::new() } else { import_seed(&layouts) };
    let survivors: HashSet<_> = old
        .into_iter()
        .flat_map(|doc| &doc.masters)
        .flat_map(|master| &master.layouts)
        .filter_map(|layout| Some((layout.id, canonical(&layout.identity)?)))
        .collect();
    let mut seen: HashSet<_> = doc
        .masters
        .iter()
        .flat_map(|master| &master.layouts)
        .filter_map(|layout| canonical(&layout.identity).filter(|id| survivors.contains(&(layout.id, id.clone()))))
        .collect();
    for master in &mut doc.masters {
        let mut copy = (**master).clone();
        let mut edited = false;
        for (at, layout) in copy.layouts.iter_mut().enumerate() {
            let mut id = canonical(&layout.identity).unwrap_or_default();
            let survivor = survivors.contains(&(layout.id, id.clone()));
            if !survivor && (id.is_empty() || seen.contains(&id)) {
                id = if old.is_some() { fresh().and_then(|id| canonical(&id)).unwrap_or_default() } else { String::new() };
                let mut salt = 0u64;
                while id.is_empty() || seen.contains(&id) {
                    id = from_seed(&format!("layout/{layout_seed}/{}/{at}/{salt}", layout.id.0));
                    salt = salt.saturating_add(1);
                }
            }
            seen.insert(id.clone());
            if layout.identity != id {
                layout.identity = id;
                edited = true;
            }
        }
        if edited {
            *master = Arc::new(copy);
            changed = true;
        }
    }
    changed
}
