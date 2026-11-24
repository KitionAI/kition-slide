//! Local id bookkeeping. [`Ids`] maps the engine ids of what a view holds to their objects;
//! [`Known`] remembers every object a view ever gave a local id (deleted ones included), so a
//! reload after an undo, redo or merge gives a surviving or restored object the id it had.
//!
//! Shape ids are scoped to the slide, layout or master holding the shape (a PPTX file numbers each
//! part's shapes on its own), so shapes are keyed by `(owner object, ShapeId)`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::ObjectId;
use kition_kit::session::IdMap;
use kition_slide_model::{LayoutId, MasterId, MediaId, Shape, ShapeId, ShapeKind, SlideFile, SlideId};

use crate::pieces::{Piece, Truth};
use crate::{KdmResult, kdm_err};

/// A table's objects: the `Table`, and its rows and columns in the order the view holds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableIds {
    pub table: ObjectId,
    pub rows: Vec<ObjectId>,
    pub cols: Vec<ObjectId>,
}

/// What holds a text body: a shape (its object), a slide's notes (the slide's object), or a table
/// cell (the table's, row's and column's objects).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BodyOwner {
    Shape(ObjectId),
    Notes(ObjectId),
    Cell { table: ObjectId, row: ObjectId, col: ObjectId },
}

/// A text body's objects: the `TextBody`, its `Paragraph`s in the order the view holds them, and
/// the listed paragraphs a merge absorbed into one of those (`crate::pieces`), with the view
/// paragraph showing each, in the order they are placed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyIds {
    pub body: ObjectId,
    pub paragraphs: Vec<ObjectId>,
    pub absorbed: Vec<(ObjectId, ObjectId)>,
}

/// The objects behind a view's masters, layouts, slides, media, shapes, tables and animations.
#[derive(Clone, Debug, Default)]
pub struct Ids {
    pub masters: IdMap<MasterId>,
    pub layouts: IdMap<LayoutId>,
    pub slides: IdMap<SlideId>,
    pub media: IdMap<MediaId>,
    shapes: HashMap<(ObjectId, ShapeId), ObjectId>,
    /// Keyed by the shape's object.
    pub(crate) tables: HashMap<ObjectId, TableIds>,
    /// Keyed by the slide's object: its `Animation` objects in the view's order.
    pub(crate) animations: HashMap<ObjectId, Vec<ObjectId>>,
    /// Every text body the view holds, by what holds it.
    pub(crate) bodies: HashMap<BodyOwner, BodyIds>,
    /// Each paragraph's `text` field as last read or written (the replica's truth, split anchors
    /// and their tails included, `crate::pieces`): what the next write diffs from, so it writes
    /// exactly the view's runs whatever marks a merge left.
    pub(crate) texts: HashMap<ObjectId, Truth>,
    /// The pieces of each view paragraph that shows more than its own field's visible runs.
    pub(crate) pieces: HashMap<ObjectId, Vec<Piece>>,
    /// The objects of the view's comment threads and comments, by their key in the view (an id
    /// the view holds, or one derived from the comment, `crate::comments`).
    pub(crate) comments: HashMap<String, ObjectId>,
    /// Threads whose comment list holds an entry twice or a deleted one, as the read listed them
    /// (the read repair lists them so in the replica).
    pub(crate) relist: Vec<crate::comments::Relist>,
    /// The `Section` objects behind the view's sections, in its order.
    pub(crate) sections: Vec<ObjectId>,
    /// The `CustomShow` objects behind the view's custom shows, in its order.
    pub(crate) shows: Vec<ObjectId>,
}

impl Ids {
    /// The object of shape `id` on the slide, layout or master whose object is `owner`.
    pub fn shape(&self, owner: &ObjectId, id: ShapeId) -> Option<&ObjectId> {
        self.shapes.get(&(owner.clone(), id))
    }

    /// The table objects of the shape whose object is `shape`.
    pub fn table(&self, shape: &ObjectId) -> Option<&TableIds> {
        self.tables.get(shape)
    }

    /// The animation objects of the slide whose object is `slide`, in the view's order.
    pub fn animations(&self, slide: &ObjectId) -> &[ObjectId] {
        self.animations.get(slide).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The text body objects of `owner`, if the view holds its text in the paragraph form.
    pub fn body(&self, owner: &BodyOwner) -> Option<&BodyIds> {
        self.bodies.get(owner)
    }

    /// How many text bodies the view holds.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// The object of the view's comment or thread `key` (its id in the view).
    pub fn comments_object(&self, key: &str) -> Option<&ObjectId> {
        self.comments.get(key)
    }

    /// The `Section` objects behind the view's sections, in its order.
    pub fn sections(&self) -> &[ObjectId] {
        &self.sections
    }

    /// The `CustomShow` objects behind the view's custom shows, in its order.
    pub fn custom_shows(&self) -> &[ObjectId] {
        &self.shows
    }

    /// The slide (its object) each object of the view belongs to: a slide itself, its shapes
    /// (group members included), their tables, rows and columns, every text body a shape, cell or
    /// the slide's notes hold with its paragraphs (absorbed ones included), and the slide's
    /// animations. Objects of layouts and masters, media and comments are not in it.
    pub(crate) fn slide_owners(&self) -> HashMap<ObjectId, ObjectId> {
        let mut owners = HashMap::new();
        for ((owner, _), shape) in &self.shapes {
            if self.slides.local(owner).is_some() {
                owners.insert(owner.clone(), owner.clone());
                owners.insert(shape.clone(), owner.clone());
            }
        }
        for (slide, animations) in &self.animations {
            if self.slides.local(slide).is_some() {
                owners.insert(slide.clone(), slide.clone());
                for animation in animations {
                    owners.insert(animation.clone(), slide.clone());
                }
            }
        }
        let mut table_slides = HashMap::new();
        for (shape, table) in &self.tables {
            if let Some(slide) = owners.get(shape).cloned() {
                table_slides.insert(table.table.clone(), slide.clone());
                for object in std::iter::once(&table.table).chain(&table.rows).chain(&table.cols) {
                    owners.insert(object.clone(), slide.clone());
                }
            }
        }
        for (holder, body) in &self.bodies {
            let slide = match holder {
                BodyOwner::Shape(shape) => owners.get(shape).cloned(),
                BodyOwner::Notes(slide) => self.slides.local(slide).is_some().then(|| slide.clone()),
                BodyOwner::Cell { table, .. } => table_slides.get(table).cloned(),
            };
            if let Some(slide) = slide {
                owners.insert(slide.clone(), slide.clone());
                let absorbed = body.absorbed.iter().map(|(listed, _)| listed);
                for object in std::iter::once(&body.body).chain(&body.paragraphs).chain(absorbed) {
                    owners.insert(object.clone(), slide.clone());
                }
            }
        }
        owners
    }

    /// Every shape as `((owner, id), object)`.
    pub(crate) fn shape_entries(&self) -> impl Iterator<Item = (&(ObjectId, ShapeId), &ObjectId)> {
        self.shapes.iter()
    }

    pub(crate) fn insert_shape(&mut self, owner: &ObjectId, id: ShapeId, object: ObjectId) {
        self.shapes.insert((owner.clone(), id), object);
    }

    pub(crate) fn remove_shape(&mut self, owner: &ObjectId, id: ShapeId) {
        if let Some(object) = self.shapes.remove(&(owner.clone(), id)) {
            if let Some(table) = self.tables.remove(&object) {
                self.bodies.retain(|key, _| !matches!(key, BodyOwner::Cell { table: t, .. } if *t == table.table));
            }
            self.bodies.remove(&BodyOwner::Shape(object));
        }
    }

    pub(crate) fn forget_shape_address(&mut self, owner: &ObjectId, id: ShapeId) {
        self.shapes.remove(&(owner.clone(), id));
    }

    /// Replaces what the slides `slides` (their objects) hold, their shapes, tables, animations,
    /// text bodies and paragraphs, with `read`, a read of just those slides (`crate::patch`), and,
    /// when it read the slide file's own fields too (`root`), its sections and custom shows.
    pub(crate) fn replace_slides(&mut self, slides: &HashSet<ObjectId>, read: Ids, root: bool) {
        let mut shapes = HashSet::new();
        self.shapes.retain(|(owner, _), object| {
            let held = slides.contains(owner);
            if held {
                shapes.insert(object.clone());
            }
            !held
        });
        let mut tables = HashSet::new();
        self.tables.retain(|shape, table| {
            let held = shapes.contains(shape);
            if held {
                tables.insert(table.table.clone());
            }
            !held
        });
        self.animations.retain(|slide, _| !slides.contains(slide));
        let mut paragraphs = Vec::new();
        self.bodies.retain(|owner, body| {
            let held = match owner {
                BodyOwner::Shape(shape) => shapes.contains(shape),
                BodyOwner::Notes(slide) => slides.contains(slide),
                BodyOwner::Cell { table, .. } => tables.contains(table),
            };
            if held {
                paragraphs.extend(body.paragraphs.iter().cloned());
                paragraphs.extend(body.absorbed.iter().map(|(paragraph, _)| paragraph.clone()));
            }
            !held
        });
        for paragraph in &paragraphs {
            self.texts.remove(paragraph);
            self.pieces.remove(paragraph);
        }
        self.shapes.extend(read.shapes);
        self.tables.extend(read.tables);
        self.animations.extend(read.animations);
        self.bodies.extend(read.bodies);
        self.texts.extend(read.texts);
        self.pieces.extend(read.pieces);
        if root {
            self.sections = read.sections;
            self.shows = read.shows;
        }
    }

    /// Forgets shapes, tables and animations whose objects `exists` no longer finds.
    pub(crate) fn retain(&mut self, exists: impl Fn(&ObjectId) -> bool) {
        self.shapes.retain(|(owner, _), object| exists(owner) && exists(object));
        self.tables.retain(|shape, _| exists(shape));
        self.animations.retain(|slide, _| exists(slide));
        self.bodies.retain(|owner, ids| {
            let holder = match owner {
                BodyOwner::Shape(o) | BodyOwner::Notes(o) => exists(o),
                BodyOwner::Cell { table, row, col } => exists(table) && exists(row) && exists(col),
            };
            holder && exists(&ids.body)
        });
    }
}

/// Object ids to the local ids a view has ever given them. See [`Known::remember`].
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub(crate) masters: HashMap<ObjectId, MasterId>,
    pub(crate) layouts: HashMap<ObjectId, LayoutId>,
    pub(crate) slides: HashMap<ObjectId, SlideId>,
    pub(crate) media: HashMap<ObjectId, MediaId>,
    pub(crate) shapes: HashMap<ObjectId, ShapeId>,
    /// The highest local id ever handed out (or `next_id` seen), so fresh ids never reuse one.
    pub(crate) max: u32,
}

impl Known {
    /// Remembers the object each of `p`'s masters, layouts, slides, media and shapes maps to.
    pub fn remember(&mut self, p: &SlideFile, ids: &Ids) {
        self.max = self.max.max(p.next_id);
        for master in &p.masters {
            self.max = self.max.max(master.id.0);
            if let Some(object) = ids.masters.object(master.id) {
                self.masters.insert(object.clone(), master.id);
            }
            for layout in &master.layouts {
                self.max = self.max.max(layout.id.0);
                if let Some(object) = ids.layouts.object(layout.id) {
                    self.layouts.insert(object.clone(), layout.id);
                }
            }
        }
        for slide in &p.slides {
            self.max = self.max.max(slide.id.0);
            if let Some(object) = ids.slides.object(slide.id) {
                self.slides.insert(object.clone(), slide.id);
            }
        }
        for item in &p.media {
            self.max = self.max.max(item.id.0);
            if let Some(object) = ids.media.object(item.id) {
                self.media.insert(object.clone(), item.id);
            }
        }
        for ((_, id), object) in &ids.shapes {
            self.max = self.max.max(id.0);
            self.shapes.insert(object.clone(), *id);
        }
    }
}

impl Known {
    /// What each object of `view` (read with `ids`) is called in `file`, when the two hold the
    /// same content in the same order (a file opened with the replica stored with it): position
    /// by position, masters, layouts, slides and the shapes of each (groups included), and media
    /// by name, type and link. `None` when their shapes do not line up.
    pub(crate) fn as_in(view: &SlideFile, ids: &Ids, file: &SlideFile) -> Option<Known> {
        fn shapes(known: &mut Known, ids: &Ids, owner: &ObjectId, mine: &[Shape], theirs: &[Shape]) -> Option<()> {
            if mine.len() != theirs.len() {
                return None;
            }
            for (a, b) in mine.iter().zip(theirs) {
                known.shapes.insert(ids.shape(owner, a.id)?.clone(), b.id);
                match (&a.kind, &b.kind) {
                    (ShapeKind::Group { children: x, .. }, ShapeKind::Group { children: y, .. }) => shapes(known, ids, owner, x, y)?,
                    (ShapeKind::Group { .. }, _) | (_, ShapeKind::Group { .. }) => return None,
                    _ => {}
                }
            }
            Some(())
        }
        if view.masters.len() != file.masters.len() || view.slides.len() != file.slides.len() || view.media.len() != file.media.len() {
            return None;
        }
        let mut known = Known { max: file.next_id, ..Known::default() };
        for (a, b) in view.masters.iter().zip(&file.masters) {
            let object = ids.masters.object(a.id)?.clone();
            known.masters.insert(object.clone(), b.id);
            shapes(&mut known, ids, &object, &a.shapes, &b.shapes)?;
            if a.layouts.len() != b.layouts.len() {
                return None;
            }
            for (x, y) in a.layouts.iter().zip(&b.layouts) {
                let object = ids.layouts.object(x.id)?.clone();
                known.layouts.insert(object.clone(), y.id);
                shapes(&mut known, ids, &object, &x.shapes, &y.shapes)?;
            }
        }
        for (a, b) in view.slides.iter().zip(&file.slides) {
            let object = ids.slides.object(a.id)?.clone();
            known.slides.insert(object.clone(), b.id);
            shapes(&mut known, ids, &object, &a.shapes, &b.shapes)?;
        }
        let order = |p: &SlideFile| {
            let mut media: Vec<_> = p.media.iter().collect();
            media.sort_by(|a, b| (&a.name, &a.content_type, &a.link).cmp(&(&b.name, &b.content_type, &b.link)));
            media
        };
        for (a, b) in order(view).into_iter().zip(order(file)) {
            known.media.insert(ids.media.object(a.id)?.clone(), b.id);
        }
        Some(known)
    }
}

/// Each place shape ids are unique in: a master's, a layout's or a slide's shape tree.
fn shape_scopes(p: &SlideFile) -> Vec<&[Shape]> {
    let mut scopes: Vec<&[Shape]> = Vec::new();
    for master in &p.masters {
        scopes.push(&master.shapes);
        for layout in &master.layouts {
            scopes.push(&layout.shapes);
        }
    }
    for slide in &p.slides {
        scopes.push(&slide.shapes);
    }
    scopes
}

/// Errs if `p` holds an id twice where a replica needs it unique (see [`crate::decode`]): writing
/// it would merge the two into one object and lose one of them.
pub(crate) fn check_unique(p: &SlideFile) -> KdmResult<()> {
    fn unique<T: std::hash::Hash + Eq>(what: &str, ids: impl Iterator<Item = T>) -> KdmResult<()> {
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                return Err(kdm_err(format!("the slide holds {what} more than once; refusing to write it")));
            }
        }
        Ok(())
    }
    // Displayed through the id itself, so the message names the duplicate.
    let mut seen = HashSet::new();
    for slide in &p.slides {
        if !seen.insert(slide.id) {
            return Err(kdm_err(format!("the slide file holds slide {} more than once; refusing to write it", slide.id)));
        }
    }
    unique("a master id", p.masters.iter().map(|m| m.id))?;
    unique("a layout id", p.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.id)))?;
    unique("a media id", p.media.iter().map(|m| m.id))?;
    for shapes in shape_scopes(p) {
        let mut all = Vec::new();
        kition_slide_model::walk(shapes, &mut |s, _| all.push(s.id));
        unique("a shape id on one slide, layout or master", all.into_iter())?;
    }
    Ok(())
}

/// Gives every duplicate id (after its first use) a fresh one, so [`check_unique`] passes.
/// References to a duplicated id keep naming its first holder.
pub(crate) fn dedupe(p: &mut SlideFile) {
    p.fix_next_id();
    let mut slides = HashSet::new();
    for i in 0..p.slides.len() {
        let dup = p.slides.get(i).is_some_and(|s| !slides.insert(s.id));
        if dup {
            let id = SlideId(p.alloc_id());
            if let Some(slide) = p.slides.get_mut(i) {
                Arc::make_mut(slide).id = id;
            }
        }
    }
    let mut masters = HashSet::new();
    let mut layouts = HashSet::new();
    for m in 0..p.masters.len() {
        let dup = p.masters.get(m).is_some_and(|x| !masters.insert(x.id));
        if dup {
            let id = MasterId(p.alloc_id());
            if let Some(master) = p.masters.get_mut(m) {
                Arc::make_mut(master).id = id;
            }
        }
        let count = p.masters.get(m).map_or(0, |x| x.layouts.len());
        for l in 0..count {
            let dup = p.masters.get(m).and_then(|x| x.layouts.get(l)).is_some_and(|x| !layouts.insert(x.id));
            if dup {
                let id = LayoutId(p.alloc_id());
                if let Some(layout) = p.masters.get_mut(m).and_then(|x| Arc::make_mut(x).layouts.get_mut(l)) {
                    layout.id = id;
                }
            }
        }
    }
    let mut media = HashSet::new();
    for i in 0..p.media.len() {
        let dup = p.media.get(i).is_some_and(|x| !media.insert(x.id));
        if dup {
            let id = MediaId(p.alloc_id());
            if let Some(item) = p.media.get_mut(i) {
                item.id = id;
            }
        }
    }
    let mut next = p.next_id;
    let mut fresh = || {
        next = next.max(1).saturating_add(1);
        ShapeId(next)
    };
    fn renumber(shapes: &mut [Shape], seen: &mut HashSet<ShapeId>, fresh: &mut dyn FnMut() -> ShapeId, depth: usize) {
        if depth > 64 {
            return;
        }
        for shape in shapes {
            if !seen.insert(shape.id) {
                shape.id = fresh();
                seen.insert(shape.id);
            }
            if let ShapeKind::Group { children, .. } = &mut shape.kind {
                renumber(children, seen, fresh, depth + 1);
            }
        }
    }
    let needs = |shapes: &[Shape]| {
        let mut seen = HashSet::new();
        let mut dup = false;
        kition_slide_model::walk(shapes, &mut |s, _| dup |= !seen.insert(s.id));
        dup
    };
    for master in &mut p.masters {
        if needs(&master.shapes) || master.layouts.iter().any(|l| needs(&l.shapes)) {
            let master = Arc::make_mut(master);
            renumber(&mut master.shapes, &mut HashSet::new(), &mut fresh, 0);
            for layout in &mut master.layouts {
                renumber(&mut layout.shapes, &mut HashSet::new(), &mut fresh, 0);
            }
        }
    }
    for slide in &mut p.slides {
        if needs(&slide.shapes) {
            renumber(&mut Arc::make_mut(slide).shapes, &mut HashSet::new(), &mut fresh, 0);
        }
    }
    p.next_id = next;
}
