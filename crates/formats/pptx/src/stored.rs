//! Values kept as XML must describe the same ids and timing as their saved parts.

use std::sync::Arc;

use kition_slide_model::{MediaItem, Shape, ShapeKind, Slide, SlideFile};

use crate::opc::{NS_A, NS_P, NS_R};
use crate::read::{Part, shapes::IdCtx, timing as read};
use crate::write::{shapes::IdMap, timing as write};
use crate::xml::{A, W};

pub(crate) fn drawing_ref(name: &str) -> String {
    use std::fmt::Write;
    let mut id = String::from("diagramDrawing_");
    for byte in name.as_bytes() {
        let _ = write!(id, "{byte:02x}");
    }
    id
}

pub fn settle_diagrams(doc: &mut SlideFile) -> bool {
    fn has_kept(shapes: &[Shape]) -> bool {
        shapes.iter().any(|s| s.diagram.is_some() || !s.prompts.is_empty() || has_kept(s.children()))
    }
    fn visit(shapes: &mut [Shape], media: &mut [MediaItem], kept: bool) -> bool {
        let mut changed = false;
        for shape in shapes {
            if !kept && shape.diagram.is_none() && !shape.prompts.is_empty() {
                shape.prompts.clear();
                changed = true;
            }
            if let Some(diagram) = shape.diagram.as_mut() {
                if let Ok(mut xml) = crate::xml::parse(diagram.frame.as_bytes()) {
                    crate::write::shapes::set_cnvpr_id(&mut xml.root, shape.id.0);
                    crate::write::shapes::set_creation_id(&mut xml.root, &shape.identity);
                    if let Some(xfrm) = &shape.xfrm {
                        crate::write::shapes::place(&mut xml.root, xfrm);
                    }
                    let stored = xml.root.to_xml();
                    if diagram.frame != stored {
                        diagram.frame = stored;
                        changed = true;
                    }
                }
                let drawing_ref = diagram
                    .drawing
                    .as_ref()
                    .and_then(|drawing| media.iter().find(|item| item.id == drawing.media))
                    .map(|item| drawing_ref(&item.name))
                    .unwrap_or_else(|| diagram.drawing_ref.clone());
                if diagram.drawing_ref != drawing_ref {
                    for part in &diagram.parts {
                        if !part.rel_type.ends_with("/diagramData") {
                            continue;
                        }
                        if let Some(item) = media.iter_mut().find(|item| item.id == part.media)
                            && let Ok(mut xml) = crate::xml::parse(&item.data)
                        {
                            fn reference(element: &mut crate::xml::El, id: &str, depth: usize) {
                                if depth > crate::xml::MAX_DEPTH {
                                    return;
                                }
                                if element.is("dataModelExt") {
                                    element.set_attr("relId", id);
                                }
                                for child in element.elements_mut() {
                                    reference(child, id, depth + 1);
                                }
                            }
                            reference(&mut xml.root, &drawing_ref, 0);
                            item.data = Arc::new(xml.root.to_xml().into_bytes());
                        }
                    }
                    diagram.drawing_ref = drawing_ref;
                    changed = true;
                }
            }
            let kept = kept || shape.diagram.is_some();
            if let Some(children) = shape.children_mut() {
                changed |= visit(children, media, kept);
            }
        }
        changed
    }
    let mut changed = false;
    for slide in &mut doc.slides {
        if has_kept(&slide.shapes) {
            changed |= visit(&mut Arc::make_mut(slide).shapes, &mut doc.media, false);
        }
    }
    changed
}

pub fn settle_slide(slide: &mut Slide) -> bool {
    let mut changed = shapes(&mut slide.shapes);
    if let Some(transition) = slide.transition.as_mut().filter(|t| t.raw.is_some()) {
        let mut xml = W::frag();
        xml.open("p:sld", A::new().a("xmlns:a", NS_A).a("xmlns:r", NS_R).a("xmlns:p", NS_P));
        // The sound is a separate media reference; its package relationship id is not content.
        write::transition_fragment(&mut xml, transition, transition.sound.map(|_| "sound"));
        xml.close("p:sld");
        if let Ok(mut tree) = crate::xml::parse(xml.s.as_bytes()) {
            let ns = tree.ns_decls();
            crate::read::resolve_mc(&mut tree.root, 0);
            if let Some(element) = tree.root.child("transition") {
                let stored = crate::read::keep_xml(element, &ns);
                if transition.raw.as_ref() != Some(&stored) {
                    transition.raw = Some(stored);
                    changed = true;
                }
            }
        }
    }
    if slide.animations.is_empty() && slide.timing.is_none() {
        return changed;
    }
    let ids = IdMap::build(&slide.shapes);
    let mut xml = W::frag();
    if let Some(kept) = slide.timing.as_ref().and_then(|kept| write::kept_xml(kept, &slide.animations, &ids)) {
        xml.raw(&kept);
    } else if !slide.animations.is_empty() {
        write::timing(&mut xml, &slide.animations, &slide.shapes, &ids);
    }
    let (animations, timing) = match crate::xml::parse(xml.s.as_bytes()) {
        Ok(tree) => {
            let ctx = IdCtx { map: ids.map.iter().map(|(shape, spid)| (*spid, *shape)).collect(), ..Default::default() };
            let part = Part {
                name: String::new(),
                rels: Default::default(),
                ns: vec![("xmlns:a".into(), NS_A.into()), ("xmlns:r".into(), NS_R.into()), ("xmlns:p".into(), NS_P.into())],
            };
            let animations = read::animations(&ctx, &tree.root, &slide.shapes);
            let timing = read::kept(&part, &ctx, &tree.root, &animations);
            (animations, timing)
        }
        Err(_) => (Vec::new(), None),
    };
    if slide.animations != animations || slide.timing != timing {
        slide.animations = animations;
        slide.timing = timing;
        changed = true;
    }
    changed
}

fn shapes(list: &mut [Shape]) -> bool {
    let mut changed = false;
    for shape in list {
        if let ShapeKind::Opaque { xml, .. } = &mut shape.kind
            && let Ok(mut tree) = crate::xml::parse(xml.as_bytes())
        {
            crate::write::shapes::set_cnvpr_id(&mut tree.root, shape.id.0);
            crate::write::shapes::set_creation_id(&mut tree.root, &shape.identity);
            let stored = tree.root.to_xml();
            if *xml != stored {
                *xml = stored;
                changed = true;
            }
        }
        if let Some(children) = shape.children_mut() {
            changed |= shapes(children);
        }
    }
    changed
}
