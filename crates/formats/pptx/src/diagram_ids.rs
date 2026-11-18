//! IDs of the editable shapes materialised from a diagram's separate drawing part.

use std::collections::{HashMap, HashSet};

use kition_slide_model::{Shape, ShapeId, ShapeKind, Slide};

use crate::xml::{A, El, W};

pub(crate) const URI: &str = "https://kition.ai/ns/slide/diagram-ids/1";

fn ids(shapes: &[Shape], out: &mut Vec<u32>) {
    for shape in shapes {
        out.push(shape.id.0);
        ids(shape.children(), out);
    }
}

pub(crate) fn write(w: &mut W, shapes: &[Shape]) {
    let mut groups = W::frag();
    fn visit(w: &mut W, shapes: &[Shape]) {
        for shape in shapes {
            if shape.diagram.is_some() {
                w.open("k:group", A::new().a("id", shape.id.0));
                kition_slide_model::walk(shape.children(), &mut |child, _| {
                    w.empty("k:shape", A::new().a("id", child.id.0).a("identity", &child.identity));
                });
                w.close("k:group");
            } else {
                visit(w, shape.children());
            }
        }
    }
    visit(&mut groups, shapes);
    if groups.s.is_empty() {
        return;
    }
    w.open("p:ext", A::new().a("uri", URI));
    w.open("k:diagramIds", A::new().a("xmlns:k", URI));
    w.raw(&groups.s);
    w.close("k:diagramIds");
    w.close("p:ext");
}

fn extension(root: &El) -> Option<&El> {
    root.child("extLst")?.children_named("ext").find(|ext| ext.attr("uri") == Some(URI))?.child("diagramIds")
}

pub(crate) fn reserved(root: &El) -> HashSet<u32> {
    extension(root)
        .into_iter()
        .flat_map(|ext| ext.children_named("group").take(20_000))
        .flat_map(|group| group.children_named("shape").take(20_000))
        .filter_map(|shape| shape.u32("id").filter(|id| (2..crate::read::shapes::MAX_KEPT_ID).contains(id)))
        .collect()
}

pub(crate) fn read(root: &El, slide: &mut Slide, media: &[kition_slide_model::MediaItem]) {
    let Some(ext) = extension(root) else { return };
    fn fixed(shapes: &[Shape], used: &mut HashSet<u32>) {
        for shape in shapes {
            used.insert(shape.id.0);
            if shape.diagram.is_none() {
                fixed(shape.children(), used);
            }
        }
    }
    fn find(shapes: &mut [Shape], id: u32) -> Option<&mut Shape> {
        for shape in shapes {
            if shape.id.0 == id {
                return Some(shape);
            }
            if let Some(children) = shape.children_mut()
                && let Some(found) = find(children, id)
            {
                return Some(found);
            }
        }
        None
    }
    fn remap(shapes: &mut [Shape], map: &HashMap<u32, u32>) {
        let id = |id: &mut ShapeId| {
            if let Some(new) = map.get(&id.0) {
                id.0 = *new;
            }
        };
        for shape in shapes {
            id(&mut shape.id);
            if let ShapeKind::Connector { start, end, .. } = &mut shape.kind {
                for (target, _) in [start, end].into_iter().flatten() {
                    id(target);
                }
            }
            if let Some(children) = shape.children_mut() {
                remap(children, map);
            }
        }
    }
    let mut used = HashSet::new();
    fixed(&slide.shapes, &mut used);
    for group in ext.children_named("group").take(20_000) {
        let Some(shape) = group.u32("id").and_then(|id| find(&mut slide.shapes, id)).filter(|shape| shape.diagram.is_some()) else { continue };
        let mut before = Vec::new();
        ids(shape.children(), &mut before);
        let after: Vec<u32> = group.children_named("shape").take(20_000).filter_map(|shape| shape.u32("id")).collect();
        let unique: HashSet<_> = after.iter().copied().collect();
        if before.len() != after.len()
            || unique.len() != after.len()
            || after.iter().any(|id| !(2..crate::read::shapes::MAX_KEPT_ID).contains(id) || used.contains(id))
        {
            continue;
        }
        let map: HashMap<_, _> = before.into_iter().zip(after).collect();
        if let Some(children) = shape.children_mut() {
            remap(children, &map);
            let identities: HashMap<_, _> = group.children_named("shape").filter_map(|e| Some((e.u32("id")?, e.attr("identity")?))).collect();
            fn restore(shapes: &mut [Shape], identities: &HashMap<u32, &str>) {
                for shape in shapes {
                    if let Some(identity) = identities.get(&shape.id.0).and_then(|id| kition_slide_model::identity::canonical(id)) {
                        shape.identity = identity;
                    }
                    if let Some(children) = shape.children_mut() {
                        restore(children, identities);
                    }
                }
            }
            restore(children, &identities);
        }
        used.extend(unique);
        let fingerprint = kition_slide_model::shapes_fingerprint(shape.children(), media);
        if let Some(diagram) = shape.diagram.as_mut() {
            diagram.fingerprint = fingerprint;
        }
    }
}
