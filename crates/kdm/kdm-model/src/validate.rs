//! Deterministic repair after a merge, an undo or a redo ([`kition_kit::session::KitModel::validate`]):
//! state only, so every replica writes the same repair for the same merged state.
//!
//! - A slide whose layout is gone gets the first layout of the master that listed it (or, if that
//!   master is gone too, of the first master).
//! - An animation of a deleted shape is deleted; a trigger, a connector end or a comment anchor at
//!   a deleted shape is cleared; section and custom show entries of deleted slides, and embedded
//!   fonts of deleted media, are dropped; a shape's `table` or `media` ref to a deleted object is
//!   cleared (the reader shows an empty table, or a picture without its image, rather than dropping
//!   the shape); so is a shape's `body` or a slide's `notesBody` naming a deleted text body (the
//!   reader then shows no text, or the 0.3.x register if one is left).
//! - List entries naming deleted objects are removed, and so is every entry after the first of an
//!   object listed twice in one list (two peers re-listed it at once): the reader reads it once, at
//!   its first place, so the view does not change.
//!
//! A change that deleted nothing can only have broken what it touched (a reference it wrote to
//! an object deleted before, an entry it listed twice), so the repair then checks only the
//! changed objects. A deletion can break references to what it deleted: with the session's view
//! of the state before the change ([`validate_with_view`]), a deletion of a slide's objects checks
//! only what can name them (that slide, the root, every animation); any other deletion, or one
//! without the view ([`validate`]), checks everything. Either way the repair is a function of the
//! merged state, what changed and the view: it deletes an object (an animation of a deleted
//! shape) only in the checks that follow a deletion.

use std::collections::BTreeSet;

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::refs::{each_comment_shape, each_slide_list};
use crate::{Ids, KdmResult, root_id};

/// The repair after `changed` objects changed. See the module docs.
pub(crate) fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    if changed.is_empty() || changed.iter().any(|object| !tx.exists(object)) || needs_full(tx, changed)? {
        return validate_all(tx);
    }
    let mut wrote = false;
    for object in changed {
        let ty = tx.object_type(object)?;
        wrote |= match ty.as_str() {
            "SlideFile" => root_fields(tx, object)?,
            "Master" => lists(tx, object, &["layouts", "shapes"])?,
            "Layout" => lists(tx, object, &["shapes"])?,
            "Slide" => {
                let masters = live_masters(tx)?;
                slide_layout(tx, object, &masters)?
                    | slide_comments(tx, object)?
                    | slide_notes(tx, object)?
                    | lists(tx, object, &["shapes", "animations"])?
            }
            "Animation" => animation(tx, object)?,
            "Shape" => shape(tx, object)? | lists(tx, object, &["children"])?,
            "Table" => lists(tx, object, &["rows", "cols"])?,
            "TextBody" => lists(tx, object, &["paragraphs"])?,
            _ => false,
        };
    }
    Ok(wrote)
}

/// What local writes left for the next repair after a deletion (local writes are not repaired, so
/// a connector or animation may still name a shape the person deleted): the objects they wrote,
/// the slides (their objects) whose objects they deleted, or `full` when that is not known (a new
/// or opened replica, a migration, the deletion of an object no slide holds).
#[derive(Clone, Debug)]
pub(crate) struct LocalRepair {
    full: bool,
    objects: BTreeSet<ObjectId>,
    slides: BTreeSet<ObjectId>,
}

impl LocalRepair {
    /// Nothing known: the next repair after a deletion checks everything.
    pub(crate) fn unknown() -> Self {
        Self { full: true, objects: BTreeSet::new(), slides: BTreeSet::new() }
    }

    /// Nothing left: everything local was repaired.
    pub(crate) fn settled() -> Self {
        Self { full: false, objects: BTreeSet::new(), slides: BTreeSet::new() }
    }

    /// Notes a local write of `changed` (`exists` after it), with `ids` the view before it.
    pub(crate) fn wrote(&mut self, changed: &BTreeSet<ObjectId>, exists: impl Fn(&ObjectId) -> bool, ids: &Ids) {
        if self.full {
            return;
        }
        let mut owners = None;
        for object in changed {
            if exists(object) {
                self.objects.insert(object.clone());
                continue;
            }
            let owners = owners.get_or_insert_with(|| ids.slide_owners());
            match owners.get(object).cloned().or_else(|| ids.slides.local(object).map(|_| object.clone())) {
                Some(slide) => {
                    self.slides.insert(slide);
                }
                None => {
                    *self = Self::unknown();
                    return;
                }
            }
        }
    }
}

/// [`validate`] with the view the state had before the change (`ids`, the session's) and what
/// local writes left (`local`). When every object the change deleted belonged to a slide (a slide
/// itself, its shapes, tables, text bodies, paragraphs or animations), the repair checks only what
/// can name them: the changed objects, the objects local writes touched, the root, the remaining
/// objects of those slides and of the slides local writes deleted from, every animation, and the
/// slides listing an animation the repair deletes. Anything else deleted (a layout, a master,
/// media, comments, an object the view does not know) takes the full check. Answers whether it
/// wrote, and whether it repaired what `local` noted (after a deletion, it always does).
///
/// It writes what [`validate_all`] writes, given that no slide's objects are named from outside
/// that slide except by the root and by animations, as Kition writes them (a shape, connector,
/// comment anchor or list of one slide never names another slide's objects), and that the state
/// was repaired before the local writes `local` noted.
/// `tests::targeted_repair_after_deletions_matches_the_full_check` holds the two to the same
/// result on concurrent merges.
pub(crate) fn validate_with_view(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>, ids: &Ids, local: &LocalRepair) -> KdmResult<(bool, bool)> {
    let deleted: Vec<&ObjectId> = changed.iter().filter(|object| !tx.exists(object)).collect();
    if changed.is_empty() || deleted.is_empty() {
        return Ok((validate(tx, changed)?, false));
    }
    if local.full {
        return Ok((validate_all(tx)?, true));
    }
    let owners = ids.slide_owners();
    let mut slides = local.slides.clone();
    for object in deleted {
        let slide = owners.get(object).cloned().or_else(|| ids.slides.local(object).map(|_| object.clone()));
        match slide {
            Some(slide) => slides.insert(slide),
            None => return Ok((validate_all(tx)?, true)),
        };
    }
    let root = root_id();
    if !tx.exists(&root) {
        return Ok((false, true));
    }
    // What can name a deleted object, besides the root and the animations (checked below).
    let mut candidates: BTreeSet<ObjectId> = changed.iter().chain(&local.objects).filter(|object| tx.exists(object)).cloned().collect();
    for (object, slide) in &owners {
        if slides.contains(slide) && tx.exists(object) {
            candidates.insert(object.clone());
        }
    }
    for slide in &slides {
        if tx.exists(slide) {
            candidates.insert(slide.clone());
        }
    }
    #[cfg(test)]
    tests::TARGETED.with(|count| count.set(count.get() + 1));
    Ok((targeted(tx, &root, candidates, ids)?, true))
}

/// [`validate_all`]'s checks, in its order, on `candidates` (and every animation).
fn targeted(tx: &mut Tx<'_>, root: &ObjectId, candidates: BTreeSet<ObjectId>, ids: &Ids) -> KdmResult<bool> {
    let mut by_type: std::collections::BTreeMap<String, Vec<ObjectId>> = std::collections::BTreeMap::new();
    for object in candidates {
        let ty = tx.object_type(&object)?;
        by_type.entry(ty).or_default().push(object);
    }
    let of = |by_type: &std::collections::BTreeMap<String, Vec<ObjectId>>, ty: &str| by_type.get(ty).cloned().unwrap_or_default();
    let mut wrote = false;

    let masters = live_masters(tx)?;
    for slide in of(&by_type, "Slide") {
        wrote |= slide_layout(tx, &slide, &masters)?;
    }

    // Every animation, listed or not; a deleted one leaves an entry in the slide listing it.
    for object in tx.objects_of_type("Animation") {
        if animation(tx, &object)? {
            wrote = true;
            if !tx.exists(&object) {
                for (slide, animations) in &ids.animations {
                    if animations.contains(&object) && tx.exists(slide) && !of(&by_type, "Slide").contains(slide) {
                        by_type.entry("Slide".into()).or_default().push(slide.clone());
                    }
                }
            }
        }
    }
    for slide in of(&by_type, "Slide") {
        wrote |= slide_comments(tx, &slide)?;
    }

    for object in of(&by_type, "Shape") {
        wrote |= shape(tx, &object)?;
    }

    for slide in of(&by_type, "Slide") {
        wrote |= slide_notes(tx, &slide)?;
    }

    wrote |= root_fields(tx, root)?;
    for (ty, fields) in LISTS {
        for object in of(&by_type, ty) {
            wrote |= lists(tx, &object, fields)?;
        }
    }
    Ok(wrote)
}

/// The lists each type holds objects in (besides the root's).
const LISTS: [(&str, &[&str]); 6] = [
    ("Master", &["layouts", "shapes"]),
    ("Layout", &["shapes"]),
    ("Slide", &["shapes", "animations"]),
    ("Shape", &["children"]),
    ("Table", &["rows", "cols"]),
    ("TextBody", &["paragraphs"]),
];

/// Whether a changed object needs a repair that deletes an object (an animation of a shape
/// deleted before): only the full check deletes, so it cleans the list entries that leaves too.
fn needs_full(tx: &Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    for object in changed {
        if tx.object_type(object)? == "Animation" && gone(tx, tx.get(object, "props")?.get("shape")) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn live_masters(tx: &Tx<'_>) -> KdmResult<Vec<ObjectId>> {
    Ok(tx.list_items(&root_id(), "masters")?.into_iter().filter(|m| tx.exists(m)).collect())
}

/// A slide whose layout is gone gets the first layout of the master that listed it (or, if that
/// master is gone too, of the first master).
fn slide_layout(tx: &mut Tx<'_>, slide: &ObjectId, masters: &[ObjectId]) -> KdmResult<bool> {
    let Value::String(layout) = tx.get(slide, "layout")? else { return Ok(false) };
    let layout = ObjectId::from_string(layout);
    if tx.exists(&layout) {
        return Ok(false);
    }
    let first_layout =
        |tx: &Tx<'_>, master: &ObjectId| -> Option<ObjectId> { tx.list_items(master, "layouts").ok()?.into_iter().find(|l| tx.exists(l)) };
    let owner = masters.iter().find(|m| tx.list_items(m, "layouts").is_ok_and(|ls| ls.contains(&layout)));
    let replacement = owner.and_then(|m| first_layout(tx, m)).or_else(|| masters.iter().find_map(|m| first_layout(tx, m)));
    tx.set(slide, "layout", replacement.map_or(Value::Null, |o| Value::String(o.as_str().to_owned())))?;
    Ok(true)
}

/// Clears a trigger at a deleted shape; deletes the animation of a deleted shape.
fn animation(tx: &mut Tx<'_>, animation: &ObjectId) -> KdmResult<bool> {
    let mut props = tx.get(animation, "props")?;
    if gone(tx, props.get("shape")) {
        tx.delete(animation)?;
        return Ok(true);
    }
    if gone(tx, props.get("trigger")) {
        if let Some(map) = props.as_object_mut() {
            map.insert("trigger".into(), Value::Null);
        }
        tx.set(animation, "props", props)?;
        return Ok(true);
    }
    Ok(false)
}

/// Clears a 0.4.x comment's anchor at a deleted shape.
fn slide_comments(tx: &mut Tx<'_>, slide: &ObjectId) -> KdmResult<bool> {
    let mut comments = tx.get(slide, "comments")?;
    let mut cleared = false;
    each_comment_shape(
        &mut comments,
        &mut |shape| {
            if gone(tx, Some(shape)) {
                *shape = Value::Null;
                cleared = true;
            }
        },
        0,
    );
    if cleared {
        tx.set(slide, "comments", comments)?;
    }
    Ok(cleared)
}

/// Clears connector ends at deleted shapes, and a shape's table, media and body refs to deleted
/// objects.
fn shape(tx: &mut Tx<'_>, shape: &ObjectId) -> KdmResult<bool> {
    let mut wrote = false;
    let mut kind_value = crate::look::member(tx, shape, "kind")?;
    let mut cleared = false;
    if let Some(kind) = kind_value.as_object_mut()
        && kind.get("type").and_then(Value::as_str) == Some("connector")
    {
        for side in ["start", "end"] {
            if let Some(end) = kind.get_mut(side)
                && gone(tx, end.as_array().and_then(|pair| pair.first()))
            {
                *end = Value::Null;
                cleared = true;
            }
        }
    }
    if cleared {
        crate::look::set_member(tx, shape, "kind", kind_value)?;
        wrote = true;
    }
    for field in ["table", "media", "body"] {
        let value = tx.get(shape, field)?;
        if gone(tx, Some(&value)) {
            tx.set(shape, field, Value::Null)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

fn slide_notes(tx: &mut Tx<'_>, slide: &ObjectId) -> KdmResult<bool> {
    let value = tx.get(slide, "notesBody")?;
    if gone(tx, Some(&value)) {
        tx.set(slide, "notesBody", Value::Null)?;
        return Ok(true);
    }
    Ok(false)
}

/// Sections, custom shows and embedded fonts naming deleted slides or media, and the root's
/// dangling list entries.
fn root_fields(tx: &mut Tx<'_>, root: &ObjectId) -> KdmResult<bool> {
    let mut wrote = false;
    for field in ["sections", "customShows"] {
        let mut value = tx.get(root, field)?;
        let mut dropped = false;
        each_slide_list(&mut value, &mut |list| {
            if let Some(items) = list.as_array_mut() {
                let before = items.len();
                items.retain(|item| !gone(tx, Some(item)));
                dropped |= items.len() != before;
            }
        });
        if dropped {
            tx.set(root, field, value)?;
            wrote = true;
        }
    }
    let mut fonts = tx.get(root, "embeddedFonts")?;
    if let Some(items) = fonts.as_array_mut() {
        let before = items.len();
        items.retain(|font| !gone(tx, font.get(1)));
        if items.len() != before {
            tx.set(root, "embeddedFonts", fonts)?;
            wrote = true;
        }
    }
    Ok(lists(tx, root, &["masters", "slides", "media"])? | wrote)
}

fn lists(tx: &mut Tx<'_>, owner: &ObjectId, fields: &[&str]) -> KdmResult<bool> {
    let mut wrote = false;
    for field in fields {
        wrote |= clean_dangling(tx, owner, field)?;
    }
    Ok(wrote)
}

/// Every check on every object.
pub(crate) fn validate_all(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = false;

    // Slides whose layout is gone (before the masters' dangling layout entries are cleaned: they
    // say which master the layout belonged to).
    let masters = live_masters(tx)?;
    for slide in tx.objects_of_type("Slide") {
        wrote |= slide_layout(tx, &slide, &masters)?;
    }

    // Animations (listed or not: one added to a slide another peer deleted is listed nowhere),
    // triggers and comment anchors. Their list entries go with the dangling ones below.
    for object in tx.objects_of_type("Animation") {
        wrote |= animation(tx, &object)?;
    }
    for slide in tx.objects_of_type("Slide") {
        wrote |= slide_comments(tx, &slide)?;
    }

    // Connector ends, and a shape's table and media refs.
    for object in tx.objects_of_type("Shape") {
        wrote |= shape(tx, &object)?;
    }

    for slide in tx.objects_of_type("Slide") {
        wrote |= slide_notes(tx, &slide)?;
    }

    // Sections, custom shows and embedded fonts; dangling list entries.
    wrote |= root_fields(tx, &root)?;
    for (ty, fields) in LISTS {
        for object in tx.objects_of_type(ty) {
            wrote |= lists(tx, &object, fields)?;
        }
    }
    Ok(wrote)
}

/// Whether `value` is an object id (a string) naming an object that no longer exists. Numbers
/// (a file's own dangling ids) and `null` are not.
fn gone(tx: &Tx<'_>, value: Option<&Value>) -> bool {
    matches!(value, Some(Value::String(id)) if !tx.exists(&ObjectId::from_string(id.as_str())))
}

/// Removes list entries naming an object that no longer exists, and the later entries of an
/// object listed twice.
fn clean_dangling(tx: &mut Tx<'_>, owner: &ObjectId, field: &str) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let mut wrote = false;
    for (index, object) in items.iter().enumerate().rev() {
        if !tx.exists(object) || items.iter().take(index).any(|earlier| earlier == object) {
            tx.list_remove(owner, field, index)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

#[cfg(test)]
mod tests {
    //! The targeted repair after deletions against the full check, on concurrent merges of random
    //! edits: both must leave the same state.

    use std::cell::Cell;
    use std::sync::Arc;

    use kition_kit::kdm::{ActorId, Origin, Replica};
    use kition_kit::session::MemoryBlobs;
    use kition_slide_model::text::{Action, Hyperlink, TextBody};
    use kition_slide_model::{Animation, Comment, CustomShow, Section, Shape, ShapeId, ShapeKind, SlideFile, Table, Xfrm, defaults};

    use super::*;
    use crate::SlideSession;

    thread_local! {
        /// How many repairs took the targeted path.
        pub(super) static TARGETED: Cell<usize> = const { Cell::new(0) };
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
        fn chance(&mut self, percent: u64) -> bool {
            self.next() % 100 < percent
        }
    }

    fn shape(p: &mut SlideFile, name: &str, x: f64) -> Shape {
        Shape {
            id: ShapeId(p.alloc_id()),
            name: name.into(),
            xfrm: Some(Xfrm::new(x, 50.0, 100.0, 60.0)),
            text: Some(TextBody::from_text(&format!("{name} first\n{name} second"))),
            ..Default::default()
        }
    }

    /// A slide file of `n` slides with what a deletion can leave dangling: connectors,
    /// animations with triggers, notes, tables, groups, sections and a custom show.
    fn slide_file(n: usize) -> SlideFile {
        let mut p = defaults::new_slide_file(None);
        let layout = p.masters[0].layouts[0].id;
        for i in 0..n {
            let mut slide = defaults::new_slide(&mut p, layout);
            let a = shape(&mut p, &format!("A{i}"), 10.0);
            let b = shape(&mut p, &format!("B{i}"), 200.0);
            let mut connector = shape(&mut p, "Connector", 0.0);
            connector.text = None;
            connector.kind = ShapeKind::Connector { start: Some((a.id, 1)), end: Some((b.id, 3)) };
            slide.animations.push(Animation { shape: a.id, trigger: Some(b.id), ..Default::default() });
            slide.notes = TextBody::from_text(&format!("Notes {i}"));
            if i % 5 == 0 {
                slide.comments.push(Comment { author: "Ann".into(), text: format!("On {i}"), shape: Some(a.id), ..Default::default() });
            }
            slide.shapes.extend([a, b, connector]);
            if i % 3 == 1 {
                let mut table = shape(&mut p, "Table", 300.0);
                table.text = None;
                table.kind = ShapeKind::Table(Table::new(2, 2, 200.0, 30.0));
                slide.shapes.push(table);
            }
            if i % 4 == 2 {
                let inner = vec![shape(&mut p, "G1", 0.0), shape(&mut p, "G2", 50.0)];
                let mut group = shape(&mut p, "Group", 400.0);
                group.text = None;
                group.kind = ShapeKind::Group { children: inner, child: Xfrm::new(0.0, 0.0, 100.0, 60.0) };
                slide.shapes.push(group);
            }
            p.slides.push(Arc::new(slide));
        }
        let ids: Vec<_> = p.slides.iter().map(|s| s.id).collect();
        p.sections = vec![
            Section { identity: String::new(), name: "One".into(), slides: ids[..n / 2].to_vec() },
            Section { identity: String::new(), name: "Two".into(), slides: ids[n / 2..].to_vec() },
        ];
        p.custom_shows = vec![CustomShow { name: "Short".into(), slides: ids.iter().step_by(3).copied().collect() }];
        p
    }

    fn slide_mut(p: &mut SlideFile, index: usize) -> &mut kition_slide_model::Slide {
        Arc::make_mut(&mut p.slides[index])
    }

    /// The deleting side: a few slides, and sometimes a shape or a paragraph elsewhere.
    fn deletions(p: &mut SlideFile, rng: &mut Rng) {
        for _ in 0..1 + rng.below(3) {
            if p.slides.len() > 2 {
                let index = rng.below(p.slides.len());
                p.slides.remove(index);
            }
        }
        if rng.chance(40) {
            let index = rng.below(p.slides.len());
            let slide = slide_mut(p, index);
            if !slide.shapes.is_empty() {
                let at = rng.below(slide.shapes.len());
                slide.shapes.remove(at);
            }
        }
        if rng.chance(40) {
            let index = rng.below(p.slides.len());
            if let Some(body) = slide_mut(p, index).shapes.first_mut().and_then(|s| s.text.as_mut())
                && body.paragraphs.len() > 1
            {
                body.paragraphs.pop();
            }
        }
    }

    /// The concurrent side: edits that touch, link to and comment on any slide.
    fn edits(p: &mut SlideFile, rng: &mut Rng) {
        for _ in 0..1 + rng.below(4) {
            let index = rng.below(p.slides.len());
            match rng.below(9) {
                0 => {
                    if let Some(body) = slide_mut(p, index).shapes.first_mut().and_then(|s| s.text.as_mut()) {
                        *body = TextBody::from_text("Edited\nagain");
                    }
                }
                1 => {
                    let s = shape(p, "New", 500.0);
                    slide_mut(p, index).shapes.push(s);
                }
                2 => {
                    let slide = slide_mut(p, index);
                    let ids: Vec<_> = slide.shapes.iter().map(|s| s.id).collect();
                    if ids.len() >= 2 {
                        slide.animations.push(Animation { shape: ids[1], trigger: Some(ids[0]), ..Default::default() });
                    }
                }
                3 => {
                    let mut connector = shape(p, "Link", 0.0);
                    connector.text = None;
                    let slide = slide_mut(p, index);
                    let ids: Vec<_> = slide.shapes.iter().map(|s| s.id).collect();
                    if ids.len() >= 2 {
                        connector.kind = ShapeKind::Connector { start: Some((ids[0], 0)), end: Some((ids[ids.len() - 1], 2)) };
                        slide.shapes.push(connector);
                    }
                }
                4 => {
                    let target = p.slides[rng.below(p.slides.len())].id;
                    if let Some(first) = slide_mut(p, index).shapes.first_mut() {
                        first.click = Some(Hyperlink { action: Action::Slide { slide: target }, tooltip: String::new(), highlight_click: false });
                    }
                }
                5 => {
                    let slide = slide_mut(p, index);
                    let anchor = slide.shapes.first().map(|s| s.id);
                    slide.comments.push(Comment { author: "Bob".into(), text: "Look".into(), shape: anchor, ..Default::default() });
                }
                6 => {
                    let id = p.slides[index].id;
                    p.sections.push(Section { identity: String::new(), name: "Mine".into(), slides: vec![id] });
                    if let Some(show) = p.custom_shows.first_mut() {
                        show.slides.push(id);
                    }
                }
                7 => {
                    if p.slides.len() > 1 {
                        let slide = p.slides.remove(index);
                        let to = rng.below(p.slides.len() + 1);
                        p.slides.insert(to, slide);
                    }
                }
                _ => {
                    let slide = slide_mut(p, index);
                    if slide.shapes.len() > 1 {
                        let at = rng.below(slide.shapes.len());
                        slide.shapes.remove(at);
                    }
                }
            }
        }
    }

    /// Imports `bytes` into two forks of `receiver`'s replica, repairs one with the view-guided
    /// repair and the other with the full check, and answers both states; `None` when the
    /// change deletes nothing (those repairs check only what changed, as before).
    fn both_repairs(receiver: &SlideSession, bytes: &[u8], seed: u64) -> Option<(String, String)> {
        let mut deletes = false;
        let mut repaired = |targeted: bool| {
            let mut replica: Replica = receiver.replica().fork(ActorId::from_u64(seed * 2 + u64::from(targeted) + 100).unwrap()).unwrap();
            replica.import_changes(bytes).unwrap();
            let changed = replica.take_changed();
            deletes |= changed.iter().any(|object| !replica.exists(object));
            replica
                .transact(&Origin::system("repair"), |tx| {
                    if targeted {
                        validate_with_view(tx, &changed, receiver.model().ids(), receiver.local_repair()).map(|(wrote, _)| wrote)
                    } else {
                        validate_all(tx)
                    }
                })
                .unwrap();
            replica.state_hash().unwrap()
        };
        let both = (repaired(true), repaired(false));
        deletes.then_some(both)
    }

    /// Merges per run.
    const SEEDS: u64 = 60;

    #[test]
    fn targeted_repair_after_deletions_matches_the_full_check() {
        let base = slide_file(12);
        let origin = SlideSession::create(&base).unwrap();
        let snapshot = origin.snapshot().unwrap();
        let before = TARGETED.with(Cell::get);
        let mut compared = 0;
        for seed in 1..=SEEDS {
            let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            let mut deleter = SlideSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
            let mut editor = SlideSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
            deleter.repair_all();
            editor.repair_all();
            let mut gone = deleter.slide_file().clone();
            deletions(&mut gone, &mut rng);
            deleter.record(&Origin::person("user:alice"), &gone).unwrap();
            let mut edited = editor.slide_file().clone();
            edits(&mut edited, &mut rng);
            editor.record(&Origin::person("user:bob"), &edited).unwrap();
            for (receiver, sender) in [(&editor, &deleter), (&deleter, &editor)] {
                let bytes = sender.export_changes(&receiver.version()).unwrap();
                if let Some((targeted, full)) = both_repairs(receiver, &bytes, seed) {
                    compared += 1;
                    assert_eq!(targeted, full, "seed {seed}: the targeted repair differs from the full check");
                }
            }
        }
        let used = TARGETED.with(Cell::get) - before;
        println!("{compared} merges that delete, {used} repaired by the targeted path");
        // Both paths ran: the targeted one, and the full check (a slide with comments, say).
        assert!(compared >= 60 && used >= compared / 2 && used < compared, "the targeted path ran {used} times in {compared} merges that delete");
    }

    /// `cargo test --release -p kdm-model --lib deletion_apply_times -- --ignored --nocapture`:
    /// a peer's slide deletion applied to a 120-slide file, with the targeted repair and with the
    /// full check (median of 20).
    #[test]
    #[ignore = "a measurement; run in release builds"]
    fn deletion_apply_times() {
        let base = slide_file(120);
        let snapshot = SlideSession::create(&base).unwrap().snapshot().unwrap();
        for full in [false, true] {
            let mut deleter = SlideSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
            let mut receiver = SlideSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
            receiver.repair_all();
            let mut times = Vec::new();
            for round in 0..20 {
                let mut gone = deleter.slide_file().clone();
                gone.slides.remove(30 + round);
                deleter.record(&Origin::person("user:alice"), &gone).unwrap();
                let bytes = deleter.export_changes(&receiver.version()).unwrap();
                if full {
                    receiver.forget_local_repair();
                }
                let start = std::time::Instant::now();
                receiver.import_changes(&bytes).unwrap();
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            let how = if full { "full check" } else { "targeted repair" };
            println!("kit.slide 120 slides: a slide deletion applies in p50 {:.2} ms, max {:.2} ms ({how})", times[10], times[19]);
        }
    }
}
