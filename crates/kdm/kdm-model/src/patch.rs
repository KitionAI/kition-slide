//! Patching the view from the objects a change touched (Kit Standard 2.0, rule E11), rather than
//! reading the whole slide again.
//!
//! Most changes touch a few slides: a shape moved, text typed, a fill, a slide added or moved.
//! Those slides are read again ([`crate::read::read_slides`]), with the slide file's own fields
//! when they changed, and the rest of the view, its masters, layouts, media and every other slide,
//! is kept as it is, the same `Arc`s, so the engine's per-slide caches stay warm. Anything else
//! falls back to the full read, which stays the reference: masters, layouts, media, sections,
//! custom shows or comment threads changed; a slide was deleted (other slides' links name it) or a
//! shape a comment is anchored at; the view awaits a read repair; or the change lists an object
//! somewhere else in the view, or touches an object no slide read holds. The patched view is the one a full
//! read gives, up to the local ids it mints (`tests/patch.rs` compares them after every import).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ObjectId, Replica};
use kition_kit::session::BlobSource;
use kition_slide_model::text::{Action, Hyperlink, RunProps, TextBody};
use kition_slide_model::{MediaId, MediaItem, Shape, ShapeId, ShapeKind, SlideFile, SlideId};

use crate::SlideModel;
use crate::ids::{BodyOwner, Ids};
use crate::read::{self, Context, read_slides};
use crate::refs::Locals;

/// Where an object of the view is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The slide at this index itself.
    Slide(usize),
    /// A shape of the slide at this index (at any depth), and its local id.
    Shape(usize, ShapeId),
    /// Another part of the slide at this index: a table, row, column, animation, text body or
    /// paragraph.
    Part(usize),
    /// The slide itself.
    Root,
    /// A media item.
    Media,
    /// Elsewhere: a master, a layout, a comment.
    Other,
}

impl Place {
    fn slide(self) -> Option<usize> {
        match self {
            Place::Slide(index) | Place::Shape(index, _) | Place::Part(index) => Some(index),
            Place::Root | Place::Media | Place::Other => None,
        }
    }
}

/// The types a slide and its contents are made of.
const SLIDE_PARTS: &[&str] = &["Slide", "Shape", "TextBody", "Paragraph", "Table", "Row", "Col", "Animation"];

/// Patches `model` after `changed` objects changed in `replica`. Answers `false`, leaving `model`
/// as it was, when only a full read can say what the view is now.
pub(crate) fn patch(model: &mut SlideModel, replica: &Replica, changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> bool {
    let ids = &model.ids;
    if changed.is_empty() || !ids.relist.is_empty() {
        return false;
    }
    let p = &model.slide_file;
    let Some(places) = places(p, ids) else { return false };

    // The slides to read again, whether the slide file's own fields changed, the changed
    // objects the view did not hold, the slides deleted, and whether media changed or arrived.
    let mut dirty = BTreeSet::new();
    let mut root = false;
    let mut fresh = Vec::new();
    let mut deleted = BTreeSet::new();
    let mut media = false;
    for object in changed {
        let exists = replica.exists(object);
        match places.get(object).copied() {
            Some(Place::Part(index)) => {
                dirty.insert(index);
            }
            Some(Place::Shape(index, id)) => {
                // A comment anchored at a deleted shape moves to its slide: a full read places it.
                if !exists && p.slides.get(index).is_none_or(|slide| slide.comments.iter().any(|c| c.shape == Some(id))) {
                    return false;
                }
                dirty.insert(index);
            }
            Some(Place::Slide(index)) if exists => {
                dirty.insert(index);
            }
            // A deleted slide: its comment threads move, which only a full read places; the other
            // slides' links to it are checked once the view is read.
            Some(Place::Slide(index)) => {
                if p.slides.get(index).is_none_or(|slide| !slide.comments.is_empty()) {
                    return false;
                }
                deleted.insert(index);
            }
            Some(Place::Media) if exists => media = true,
            Some(Place::Root) if exists => root = true,
            Some(_) => return false,
            None if !exists => {
                // Never in the view (made and deleted since), unless a paragraph the view read.
                if ids.texts.contains_key(object) {
                    return false;
                }
            }
            None => match replica.object_type(object) {
                Ok(ty) if SLIDE_PARTS.contains(&ty.as_str()) => fresh.push(object),
                Ok(ty) if ty == "Media" => media = true,
                _ => return false,
            },
        }
    }
    dirty.retain(|index| !deleted.contains(index));
    if dirty.is_empty() && !root && !media && deleted.is_empty() {
        return fresh.is_empty();
    }

    // The slides' objects, in the view's order and in the order they now have.
    let mut before = Vec::with_capacity(p.slides.len());
    for slide in &p.slides {
        let Some(object) = ids.slides.object(slide.id) else { return false };
        before.push(object.clone());
    }
    let gone: HashSet<&ObjectId> = deleted.iter().filter_map(|index| before.get(*index)).collect();
    let order = if root {
        let masters: Option<Vec<ObjectId>> = p.masters.iter().map(|m| ids.masters.object(m.id).cloned()).collect();
        if masters.is_none() || masters != live(replica, "masters") {
            return false;
        }
        let Some(order) = live(replica, "slides") else { return false };
        let listed: HashSet<&ObjectId> = order.iter().collect();
        // A slide left the view only by being deleted.
        if before.iter().any(|object| !listed.contains(object) && !gone.contains(object))
            || order.iter().any(|object| gone.contains(object) || places.get(object).is_some_and(|place| !matches!(place, Place::Slide(_))))
        {
            return false;
        }
        order
    } else if deleted.is_empty() {
        before.clone()
    } else {
        return false;
    };
    // The media list: kept items stay, changed and new ones are read; one that left the view
    // (others may still show it) takes the full read.
    let media_order = if root || media {
        let Some(list) = live(replica, "media") else { return false };
        let listed: HashSet<&ObjectId> = list.iter().collect();
        if p.media.iter().any(|item| ids.media.object(item.id).is_none_or(|object| !listed.contains(object))) {
            return false;
        }
        Some(list)
    } else {
        None
    };

    let index: HashMap<&ObjectId, usize> = before.iter().enumerate().map(|(i, object)| (object, i)).collect();
    let slides: Vec<(ObjectId, Option<SlideId>)> = order
        .iter()
        .filter(|object| index.get(object).is_none_or(|i| dirty.contains(i)))
        .map(|object| (object.clone(), index.get(object).and_then(|i| p.slides.get(*i)).map(|slide| slide.id)))
        .collect();
    let objects: HashSet<ObjectId> = slides.iter().map(|(object, _)| object.clone()).collect();
    // Read again or gone: their ids are replaced.
    let mut purged = objects.clone();
    purged.extend(gone.iter().map(|object| (*object).clone()));
    let (guard, mut context) = context(p, ids, &places, |place| place.slide().is_some_and(|i| dirty.contains(&i)), &purged);
    for object in &gone {
        context.locals.slides.remove(*object);
    }

    let mut new_ids = model.ids.clone();
    let mut view = p.clone();
    let mut fetched = crate::MediaCache::new();
    let mut missing = BTreeSet::new();
    if let Some(list) = media_order {
        let kept: HashMap<&ObjectId, &MediaItem> = p.media.iter().filter_map(|item| ids.media.object(item.id).map(|object| (object, item))).collect();
        let mut next = context.used.iter().copied().max().unwrap_or(0).max(model.known.max).max(p.next_id);
        let mut items = Vec::with_capacity(list.len());
        for object in &list {
            match kept.get(object) {
                Some(item) if !changed.contains(object) => items.push((*item).clone()),
                old => {
                    let id = match (old, model.known.media.get(object)) {
                        (Some(item), _) => item.id,
                        (None, Some(id)) if !context.used.contains(&id.0) => *id,
                        _ => {
                            next += 1;
                            MediaId(next)
                        }
                    };
                    context.used.insert(id.0);
                    context.non_shapes.insert(id.0);
                    context.locals.media.insert(object.clone(), id);
                    new_ids.media.insert(object.clone(), id);
                    match read::read_media(replica, object, id, blobs, &model.media, &mut fetched, &mut missing) {
                        Ok(item) => items.push(item),
                        Err(_) => return false,
                    }
                }
            }
        }
        view.media = items;
        view.next_id = view.next_id.max(next);
    }

    let Ok(Some(read)) = read_slides(replica, &model.known, &guard, context, &slides, root.then_some(&mut view)) else {
        return false;
    };
    // A changed object no slide read holds: listed in a place this read cannot see.
    if fresh.iter().any(|object| !read.objects.contains(*object)) {
        return false;
    }
    let mut read_slides: HashMap<ObjectId, kition_slide_model::Slide> = HashMap::with_capacity(read.slides.len());
    for (object, slide) in read.slides {
        new_ids.slides.insert(object.clone(), slide.id);
        read_slides.insert(object, slide);
    }
    let mut out = Vec::with_capacity(order.len());
    let mut kept_slides = Vec::new();
    for object in &order {
        let old = index.get(object).and_then(|i| p.slides.get(*i));
        match (read_slides.remove(object), old) {
            (Some(mut slide), old) => {
                // Comment threads are read with the whole view; none changed, so a slide keeps its
                // own (a new slide has none).
                slide.comments = old.map(|old| old.comments.clone()).unwrap_or_default();
                out.push(Arc::new(slide));
            }
            (None, Some(old)) => {
                kept_slides.push(old.clone());
                out.push(old.clone());
            }
            (None, None) => return false,
        }
    }
    // A slide, master or layout kept as it was that links to a deleted slide: a full read reads
    // that link as none.
    if !deleted.is_empty() {
        let targets: HashSet<SlideId> = deleted.iter().filter_map(|index| p.slides.get(*index)).map(|slide| slide.id).collect();
        let links = |shapes: &[Shape]| shapes.iter().any(|shape| links_to(shape, &targets));
        if kept_slides.iter().any(|slide| links(&slide.shapes) || body_links_to(&slide.notes, &targets))
            || p.masters.iter().any(|master| links(&master.shapes) || master.layouts.iter().any(|layout| links(&layout.shapes)))
        {
            return false;
        }
        for target in &targets {
            new_ids.slides.remove_local(*target);
        }
    }
    view.slides = out;
    view.next_id = view.next_id.max(read.max).max(model.known.max);
    new_ids.replace_slides(&purged, read.ids, root);
    model.adopt(view, new_ids);
    model.missing.retain(|hash| !fetched.contains_key(hash));
    model.media.extend(fetched);
    model.missing.extend(missing);
    true
}

/// Whether `shape`, its text, or what it holds links to one of `targets`.
fn links_to(shape: &Shape, targets: &HashSet<SlideId>) -> bool {
    let link =
        |link: &Option<Hyperlink>| link.as_ref().is_some_and(|link| matches!(link.action, Action::Slide { slide } if targets.contains(&slide)));
    if link(&shape.click) || link(&shape.hover) || shape.text.as_ref().is_some_and(|text| body_links_to(text, targets)) {
        return true;
    }
    match &shape.kind {
        ShapeKind::Group { children, .. } => children.iter().any(|child| links_to(child, targets)),
        ShapeKind::Table(table) => table.rows.iter().any(|row| row.cells.iter().any(|cell| body_links_to(&cell.text, targets))),
        _ => false,
    }
}

/// Whether a run of `body` links to one of `targets`.
fn body_links_to(body: &TextBody, targets: &HashSet<SlideId>) -> bool {
    let link = |props: &RunProps| props.link.as_ref().is_some_and(|link| matches!(link.action, Action::Slide { slide } if targets.contains(&slide)));
    body.paragraphs.iter().any(|paragraph| link(&paragraph.end_props) || paragraph.runs.iter().any(|run| link(&run.props)))
}

/// The objects the slide's list `field` holds that exist, each at its first place.
fn live(replica: &Replica, field: &str) -> Option<Vec<ObjectId>> {
    let items = replica.list(&crate::root_id(), field).ok()?;
    let mut seen = HashSet::new();
    Some(items.into_iter().filter(|object| replica.exists(object) && seen.insert(object.clone())).collect())
}

/// Every object of the view and where it is; `None` if the ids do not describe the view.
fn places(p: &SlideFile, ids: &Ids) -> Option<HashMap<ObjectId, Place>> {
    let mut places = HashMap::new();
    places.insert(crate::root_id(), Place::Root);
    let mut slide_index = HashMap::new();
    for (index, slide) in p.slides.iter().enumerate() {
        let object = ids.slides.object(slide.id)?;
        places.insert(object.clone(), Place::Slide(index));
        slide_index.insert(object.clone(), index);
    }
    for master in &p.masters {
        places.insert(ids.masters.object(master.id)?.clone(), Place::Other);
        for layout in &master.layouts {
            places.insert(ids.layouts.object(layout.id)?.clone(), Place::Other);
        }
    }
    for item in &p.media {
        places.insert(ids.media.object(item.id)?.clone(), Place::Media);
    }
    for object in ids.comments.values() {
        places.insert(object.clone(), Place::Other);
    }
    let mut shapes = HashMap::new();
    for ((owner, id), object) in ids.shape_entries() {
        let place = slide_index.get(owner).map_or(Place::Other, |index| Place::Shape(*index, *id));
        places.insert(object.clone(), place);
        shapes.insert(object.clone(), place);
    }
    // A part goes where the shape or slide holding it is.
    let part = |place: Place| place.slide().map_or(Place::Other, Place::Part);
    let mut tables = HashMap::new();
    for (shape, table) in &ids.tables {
        let place = part(shapes.get(shape).copied().unwrap_or(Place::Other));
        tables.insert(table.table.clone(), place);
        for object in std::iter::once(&table.table).chain(&table.rows).chain(&table.cols) {
            places.insert(object.clone(), place);
        }
    }
    for (slide, animations) in &ids.animations {
        let place = slide_index.get(slide).map_or(Place::Other, |index| Place::Part(*index));
        for object in animations {
            places.insert(object.clone(), place);
        }
    }
    for (owner, body) in &ids.bodies {
        let place = match owner {
            BodyOwner::Shape(shape) => part(shapes.get(shape).copied().unwrap_or(Place::Other)),
            BodyOwner::Notes(slide) => slide_index.get(slide).map_or(Place::Other, |index| Place::Part(*index)),
            BodyOwner::Cell { table, .. } => tables.get(table).copied().unwrap_or(Place::Other),
        };
        for object in std::iter::once(&body.body).chain(&body.paragraphs).chain(body.absorbed.iter().map(|(paragraph, _)| paragraph)) {
            places.insert(object.clone(), place);
        }
    }
    Some(places)
}

/// The rest of the view, for reading the slides whose objects are `objects` again: the objects
/// it holds (all but those `read` places), and the ids and references it keeps.
fn context(
    p: &SlideFile,
    ids: &Ids,
    places: &HashMap<ObjectId, Place>,
    read: impl Fn(&Place) -> bool,
    objects: &HashSet<ObjectId>,
) -> (HashSet<ObjectId>, Context) {
    let guard: HashSet<ObjectId> = places.iter().filter(|(_, place)| !read(place)).map(|(object, _)| object.clone()).collect();
    let mut used = HashSet::new();
    let mut non_shapes = HashSet::new();
    let mut locals = Locals::default();
    let mut layouts = HashMap::new();
    for master in &p.masters {
        used.insert(master.id.0);
        non_shapes.insert(master.id.0);
        for layout in &master.layouts {
            used.insert(layout.id.0);
            non_shapes.insert(layout.id.0);
            if let Some(object) = ids.layouts.object(layout.id) {
                layouts.insert(object.clone(), layout.id);
            }
        }
    }
    for slide in &p.slides {
        used.insert(slide.id.0);
        non_shapes.insert(slide.id.0);
        if let Some(object) = ids.slides.object(slide.id) {
            locals.slides.insert(object.clone(), slide.id);
        }
    }
    for item in &p.media {
        used.insert(item.id.0);
        non_shapes.insert(item.id.0);
        if let Some(object) = ids.media.object(item.id) {
            locals.media.insert(object.clone(), item.id);
        }
    }
    for ((owner, id), object) in ids.shape_entries() {
        if !objects.contains(owner) {
            used.insert(id.0);
            locals.shapes.insert(object.clone(), *id);
        }
    }
    let fallback_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();
    (guard, Context { used, non_shapes, locals, layouts, fallback_layout })
}
