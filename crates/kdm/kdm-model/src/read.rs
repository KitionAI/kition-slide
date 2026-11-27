//! Replica -> `SlideFile`, in two passes: the first walks every list (skipping entries whose
//! object is gone, and every object after its first listing) and gives each object its local id
//! — the one [`Known`] remembers for it, else a fresh one above every id the view ever used — so
//! the second can translate every reference stored as data while it reads the fields.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{KdmError, ObjectId, Replica};
use kition_kit::session::{BlobRef, BlobSource};
use kition_slide_model::table::{Cell, Row};
use kition_slide_model::text::{ListStyle, Paragraph, TextBody};
use kition_slide_model::{
    Animation, Background, FontFace, LayoutId, LayoutType, Master, MasterId, MediaId, MediaItem, Properties, Shape, ShapeId, ShapeKind, Size, Slide,
    SlideFile, SlideId, Table, TagList,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ids::{BodyIds, BodyOwner, Ids, Known, TableIds};
use crate::refs::{Locals, shape_in, to_locals};
use crate::{KdmResult, MediaCache, kdm_err, root_id};

const MAX_DEPTH: usize = 64;

/// The id a slide's object holds (`slideId`, its tool id's value): what a read gives it when no
/// view gave it one before, so every replica reads a slide with the id its tool id names.
fn stored_slide_id(replica: &Replica, slide: &ObjectId) -> Option<u32> {
    replica.get(slide, "slideId").ok()?.as_u64().and_then(|id| u32::try_from(id).ok())
}

/// The part-local number is an address, separate from the persistent key.
fn stored_shape_number(replica: &Replica, shape: &ObjectId) -> Option<u32> {
    replica.get(shape, "address").ok()?.get("number")?.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Reads the whole slide with fresh local ids.
pub fn read_document(replica: &Replica, blobs: &dyn BlobSource) -> KdmResult<(SlideFile, Ids)> {
    let (p, ids, _, _) = read_with(replica, blobs, &Known::default(), &MediaCache::new())?;
    Ok((p, ids))
}

/// Reads the whole slide, giving every object `known` remembers the local id it had.
pub fn read_document_known(replica: &Replica, blobs: &dyn BlobSource, known: &Known) -> KdmResult<(SlideFile, Ids)> {
    let (p, ids, _, _) = read_with(replica, blobs, known, &MediaCache::new())?;
    Ok((p, ids))
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RootProps {
    doc: Properties,
    slide_size: Size,
    notes_size: Size,
    first_slide_number: u32,
    tags: Vec<TagList>,
    font_faces: Vec<FontFace>,
    pres_props: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct MasterProps {
    name: String,
    background: Option<Background>,
    preserve: bool,
    tags: Vec<TagList>,
    hf: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ListStyles {
    title: ListStyle,
    body: ListStyle,
    other: ListStyle,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct LayoutProps {
    name: String,
    kind: LayoutType,
    background: Option<Background>,
    show_master_shapes: bool,
    preserve: bool,
    user_drawn: bool,
    raw_ext: Option<String>,
    tags: Vec<TagList>,
    hf: Option<String>,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SlideProps {
    name: String,
    hidden: bool,
    show_master_shapes: bool,
    tags: Vec<TagList>,
    timing: Option<Value>,
    notes_page: Option<String>,
    file_id: Option<u32>,
}

impl Default for SlideProps {
    fn default() -> Self {
        Self { name: String::new(), hidden: false, show_master_shapes: true, tags: vec![], timing: None, notes_page: None, file_id: None }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct MediaProps {
    name: String,
    content_type: String,
    link: Option<String>,
}

/// `null` (a field never written) reads as `T`'s default.
fn from<T: DeserializeOwned + Default>(value: Value) -> KdmResult<T> {
    if value.is_null() { Ok(T::default()) } else { serde_json::from_value(value).map_err(kdm_err) }
}

struct ShapeNode {
    object: ObjectId,
    id: ShapeId,
    props: Value,
    children: Vec<ShapeNode>,
}

struct LayoutNode {
    object: ObjectId,
    id: LayoutId,
    shapes: Vec<ShapeNode>,
}

struct MasterNode {
    object: ObjectId,
    id: MasterId,
    shapes: Vec<ShapeNode>,
    layouts: Vec<LayoutNode>,
}

struct SlideNode {
    object: ObjectId,
    id: SlideId,
    shapes: Vec<ShapeNode>,
}

struct Reader<'r> {
    replica: &'r Replica,
    known: &'r Known,
    used: HashSet<u32>,
    /// The ids of what is not a shape (masters, layouts, slides, media): a shape's number may
    /// repeat across slides, layouts and masters, never one of these.
    non_shapes: HashSet<u32>,
    /// The shape numbers each slide, layout or master (its object) holds: unique within it.
    scopes: HashMap<ObjectId, HashSet<u32>>,
    next: u32,
    seen: HashSet<ObjectId>,
    ids: Ids,
    locals: Locals,
    layouts: HashMap<ObjectId, LayoutId>,
    /// For a partial read ([`read_slides`]): the objects the rest of the view holds. Listing one
    /// of them again sets `conflict` (a full read decides which place keeps it).
    guard: Option<&'r HashSet<ObjectId>>,
    conflict: bool,
}

/// Also answers the media bytes it read, by hash, and the hashes of media blobs `blobs` does not
/// hold yet: a peer's changes can arrive before their blobs, and such media reads as empty until
/// they do (the replica is unaffected; [`crate::SlideSession::add_blobs`] reloads the view).
pub(crate) fn read_with(
    replica: &Replica,
    blobs: &dyn BlobSource,
    known: &Known,
    cache: &MediaCache,
) -> KdmResult<(SlideFile, Ids, MediaCache, BTreeSet<String>)> {
    let mut r = Reader {
        replica,
        known,
        used: HashSet::new(),
        non_shapes: HashSet::new(),
        scopes: HashMap::new(),
        next: known.max,
        seen: HashSet::new(),
        ids: Ids::default(),
        locals: Locals::default(),
        layouts: HashMap::new(),
        guard: None,
        conflict: false,
    };
    let root = root_id();
    if !replica.exists(&root) {
        return Err(KdmError::NotFound(root.to_string()));
    }

    // Pass 1: structure and local ids.
    let mut masters = Vec::new();
    for object in r.live(&root, "masters")? {
        let id = MasterId(r.mint(known.masters.get(&object).map(|m| m.0)));
        r.ids.masters.insert(object.clone(), id);
        let mut layouts = Vec::new();
        for layout in r.live(&object, "layouts")? {
            let id = LayoutId(r.mint(known.layouts.get(&layout).map(|l| l.0)));
            r.ids.layouts.insert(layout.clone(), id);
            r.layouts.insert(layout.clone(), id);
            let shapes = r.shape_nodes(&layout, "shapes", &layout, 0)?;
            layouts.push(LayoutNode { object: layout, id, shapes });
        }
        let shapes = r.shape_nodes(&object, "shapes", &object, 0)?;
        masters.push(MasterNode { object, id, shapes, layouts });
    }
    let mut slides = Vec::new();
    for object in r.live(&root, "slides")? {
        let id = SlideId(r.mint(known.slides.get(&object).map(|s| s.0).or_else(|| stored_slide_id(replica, &object))));
        r.ids.slides.insert(object.clone(), id);
        r.locals.slides.insert(object.clone(), id);
        let shapes = r.shape_nodes(&object, "shapes", &object, 0)?;
        slides.push(SlideNode { object, id, shapes });
    }
    let mut media_objects = Vec::new();
    for object in r.live(&root, "media")? {
        let id = MediaId(r.mint(known.media.get(&object).map(|m| m.0)));
        r.ids.media.insert(object.clone(), id);
        r.locals.media.insert(object.clone(), id);
        media_objects.push((object, id));
    }

    // Pass 2: fields.
    let mut p = SlideFile { slides: Vec::new(), masters: Vec::new(), media: Vec::new(), ..SlideFile::default() };
    r.root_fields(&root, &mut p)?;

    let mut media_cache = MediaCache::new();
    let mut missing = BTreeSet::new();
    for (object, id) in media_objects {
        p.media.push(read_media(replica, &object, id, blobs, cache, &mut media_cache, &mut missing)?);
    }

    for node in &masters {
        let props: MasterProps = from(r.translated(&node.object, "props", false)?)?;
        let styles: ListStyles = from(r.translated(&node.object, "listStyles", false)?)?;
        let mut layouts = Vec::new();
        for layout in &node.layouts {
            let props: LayoutProps = from(r.translated(&layout.object, "props", false)?)?;
            layouts.push(kition_slide_model::Layout {
                id: layout.id,
                identity: replica.get(&layout.object, "identity")?.as_str().unwrap_or("").to_owned(),
                name: props.name,
                kind: props.kind,
                shapes: r.shapes(&layout.shapes, 0)?,
                background: props.background,
                show_master_shapes: props.show_master_shapes,
                preserve: props.preserve,
                user_drawn: props.user_drawn,
                tags: props.tags,
                raw_ext: props.raw_ext,
                hf: props.hf,
            });
        }
        p.masters.push(Arc::new(Master {
            id: node.id,
            name: props.name,
            theme: from(r.translated(&node.object, "theme", false)?)?,
            shapes: r.shapes(&node.shapes, 0)?,
            background: props.background,
            title_style: styles.title,
            body_style: styles.body,
            other_style: styles.other,
            layouts,
            color_map: from(replica.get(&node.object, "colorMap")?)?,
            preserve: props.preserve,
            tags: props.tags,
            raw_ext: serde_json::from_value(replica.get(&node.object, "rawExt")?).map_err(kdm_err)?,
            hf: props.hf,
        }));
    }
    let fallback_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();
    let mut read_slides = Vec::with_capacity(slides.len());
    for node in &slides {
        read_slides.push(r.slide(node, fallback_layout)?);
    }
    // Comment threads live beside the slides (M23): each goes to the slide its anchor names.
    let mut holders: Vec<(ObjectId, &mut Vec<kition_slide_model::Comment>)> =
        slides.iter().zip(read_slides.iter_mut()).map(|(node, slide)| (node.object.clone(), &mut slide.comments)).collect();
    crate::comments::read(replica, &mut holders, &mut r.ids)?;
    p.slides = read_slides.into_iter().map(Arc::new).collect();
    p.next_id = r.used.iter().copied().max().unwrap_or(0).max(known.max);
    Ok((p, r.ids, media_cache, missing))
}

/// One media item, its bytes from `cache` or `blobs` (kept in `out`); a blob `blobs` does not hold
/// yet reads as empty and is listed in `missing`, so the next read takes it.
pub(crate) fn read_media(
    replica: &Replica,
    object: &ObjectId,
    id: MediaId,
    blobs: &dyn BlobSource,
    cache: &MediaCache,
    out: &mut MediaCache,
    missing: &mut BTreeSet<String>,
) -> KdmResult<MediaItem> {
    let props: MediaProps = from(replica.get(object, "props")?)?;
    let data = match replica.get(object, "blob")? {
        Value::Null => Arc::new(Vec::new()),
        blob => {
            let blob: BlobRef = serde_json::from_value(blob).map_err(kdm_err)?;
            match cache.get(&blob.hash).cloned().or_else(|| blobs.get(&blob.hash).map(|bytes| Arc::new(bytes.to_vec()))) {
                Some(data) => {
                    out.insert(blob.hash, data.clone());
                    data
                }
                None => {
                    missing.insert(blob.hash);
                    Arc::new(Vec::new())
                }
            }
        }
    };
    Ok(MediaItem { id, name: props.name, content_type: props.content_type, data, link: props.link })
}

/// What a partial read ([`read_slides`]) starts from: the rest of the view, as it stays.
pub(crate) struct Context {
    /// Every local id the rest of the view holds.
    pub used: HashSet<u32>,
    /// Those of them that are no shape's (masters, layouts, slides, media).
    pub non_shapes: HashSet<u32>,
    /// The references the rest of the view resolves: every slide and media item, and the shapes
    /// outside the slides read.
    pub locals: Locals,
    pub layouts: HashMap<ObjectId, LayoutId>,
    pub fallback_layout: LayoutId,
}

/// Slides read again by [`read_slides`].
pub(crate) struct ReadSlides {
    /// Each slide's object and what it reads as.
    pub slides: Vec<(ObjectId, Slide)>,
    /// Their ids: shapes, tables, animations, text bodies and paragraphs.
    pub ids: Ids,
    /// Every object they hold.
    pub objects: HashSet<ObjectId>,
    /// The highest local id the read gave.
    pub max: u32,
}

/// Reads `slides` again (each its object and the local id it keeps, `None` for a slide new to the
/// view), as a full read would read them where the rest of the view (`context`, holding the
/// objects `guard`) stays as it is; with `root`, the slide's own fields too, into it.
/// `None` when they list an object the rest of the view holds (only a full read can say which
/// place keeps it). Each slide's `comments` are empty: comment threads are read with the whole
/// view.
pub(crate) fn read_slides(
    replica: &Replica,
    known: &Known,
    guard: &HashSet<ObjectId>,
    context: Context,
    slides: &[(ObjectId, Option<SlideId>)],
    root: Option<&mut SlideFile>,
) -> KdmResult<Option<ReadSlides>> {
    let mut r = Reader {
        replica,
        known,
        used: context.used,
        non_shapes: context.non_shapes,
        scopes: HashMap::new(),
        next: known.max,
        seen: HashSet::new(),
        ids: Ids::default(),
        locals: context.locals,
        layouts: context.layouts,
        guard: Some(guard),
        conflict: false,
    };
    let mut nodes = Vec::with_capacity(slides.len());
    for (object, id) in slides {
        let id = match id {
            Some(id) => {
                r.used.insert(id.0);
                r.non_shapes.insert(id.0);
                *id
            }
            None => SlideId(r.mint(known.slides.get(object).map(|s| s.0).or_else(|| stored_slide_id(replica, object)))),
        };
        r.seen.insert(object.clone());
        r.ids.slides.insert(object.clone(), id);
        r.locals.slides.insert(object.clone(), id);
        let shapes = r.shape_nodes(object, "shapes", object, 0)?;
        nodes.push(SlideNode { object: object.clone(), id, shapes });
    }
    let mut read = Vec::with_capacity(nodes.len());
    for node in &nodes {
        read.push((node.object.clone(), r.slide(node, context.fallback_layout)?));
    }
    if let Some(p) = root {
        r.root_fields(&root_id(), p)?;
    }
    if r.conflict {
        return Ok(None);
    }
    let mut objects = std::mem::take(&mut r.seen);
    objects.extend(r.ids.bodies.values().map(|body| body.body.clone()));
    objects.extend(r.ids.tables.values().map(|table| table.table.clone()));
    let max = r.used.iter().copied().max().unwrap_or(0);
    Ok(Some(ReadSlides { slides: read, ids: r.ids, objects, max }))
}

impl Reader<'_> {
    /// The slide file's own fields (not its masters, slides or media), read into `p`, with its
    /// sections and custom shows (`crate::members`). The view properties are not replicated: `p`
    /// keeps its own.
    fn root_fields(&mut self, root: &ObjectId, p: &mut SlideFile) -> KdmResult<()> {
        let props: RootProps = from(self.replica.get(root, "props")?)?;
        p.props = props.doc;
        p.slide_size = props.slide_size;
        p.notes_size = props.notes_size;
        p.first_slide_number = props.first_slide_number;
        p.tags = props.tags;
        p.font_faces = props.font_faces;
        p.pres_props = props.pres_props;
        p.show = from(self.replica.get(root, "show")?)?;
        p.header_footer = from(self.replica.get(root, "headerFooter")?)?;
        p.default_text_style = from(self.translated(root, "defaultTextStyle", false)?)?;
        // A slide belongs to one section at most: the first that lists it (see `crate::members`).
        p.sections = Vec::new();
        self.ids.sections = Vec::new();
        let mut placed = HashSet::new();
        for object in self.listed(root, "sections")? {
            let mut slides = crate::members::read(self.replica.map_entries(&object, crate::members::MEMBERS)?, &self.locals);
            slides.retain(|slide| placed.insert(*slide));
            p.sections.push(kition_slide_model::Section {
                identity: self.replica.get(&object, "identity")?.as_str().unwrap_or("").to_owned(),
                name: self.replica.get(&object, "name")?.as_str().unwrap_or("").to_owned(),
                slides,
            });
            self.ids.sections.push(object);
        }
        p.custom_shows = Vec::new();
        self.ids.shows = Vec::new();
        for object in self.listed(root, "customShows")? {
            p.custom_shows.push(kition_slide_model::CustomShow {
                name: self.replica.get(&object, "name")?.as_str().unwrap_or("").to_owned(),
                slides: crate::members::read(self.replica.map_entries(&object, crate::members::MEMBERS)?, &self.locals),
            });
            self.ids.shows.push(object);
        }
        p.embedded_fonts = Vec::new();
        if let Value::Array(fonts) = self.replica.get(root, "embeddedFonts")? {
            for font in fonts {
                let (Some(name), Some(media)) = (font.get(0).and_then(Value::as_str), font.get(1)) else { continue };
                let id = match media {
                    Value::String(s) => self.locals.media.get(&ObjectId::from_string(s.as_str())).copied(),
                    other => other.as_u64().and_then(|n| u32::try_from(n).ok()).map(MediaId),
                };
                if let Some(id) = id {
                    p.embedded_fonts.push((name.to_owned(), id));
                }
            }
        }
        p.embedded_font_list = serde_json::from_value(self.replica.get(root, "embeddedFontList")?).map_err(kdm_err)?;
        p.embedded_font_rels = serde_json::from_value(self.translated(root, "embeddedFontRels", false)?).unwrap_or_default();
        p.notes_master = serde_json::from_value(self.translated(root, "notesMaster", false)?).map_err(kdm_err)?;
        p.handout_master = serde_json::from_value(self.translated(root, "handoutMaster", false)?).map_err(kdm_err)?;
        p.raw_ext = serde_json::from_value(self.replica.get(root, "rawExt")?).map_err(kdm_err)?;
        Ok(())
    }

    /// A fresh local id, or `known` if no object of this read has it yet (for a master, layout,
    /// slide or media item).
    fn mint(&mut self, known: Option<u32>) -> u32 {
        let id = match known {
            Some(id) if id > 1 && self.used.insert(id) => id,
            _ => self.fresh(),
        };
        self.non_shapes.insert(id);
        id
    }

    /// A shape's number in `scope` (a slide, layout or master): `known` unless that scope holds it
    /// already or it is a master's, layout's, slide's or media item's id, else a fresh one.
    /// Numbers repeat across scopes, as in a file (`cNvPr` is unique per slide).
    /// [`Self::mint_shape`] with `known`, else `stored` when `known` is taken on its slide.
    fn mint_shape_of(&mut self, scope: &ObjectId, known: Option<u32>, stored: Option<u32>) -> u32 {
        let free =
            |reader: &Self, id: u32| id > 1 && !reader.non_shapes.contains(&id) && !reader.scopes.get(scope).is_some_and(|taken| taken.contains(&id));
        let pick = known.filter(|id| free(self, *id)).or_else(|| stored.filter(|id| free(self, *id))).or(known);
        self.mint_shape(scope, pick)
    }

    fn mint_shape(&mut self, scope: &ObjectId, known: Option<u32>) -> u32 {
        let taken = self.scopes.entry(scope.clone()).or_default();
        if let Some(id) = known
            && id > 1
            && !self.non_shapes.contains(&id)
            && taken.insert(id)
        {
            self.used.insert(id);
            return id;
        }
        let id = self.fresh();
        self.scopes.entry(scope.clone()).or_default().insert(id);
        id
    }

    /// An id no object of this read has.
    fn fresh(&mut self) -> u32 {
        loop {
            self.next = self.next.max(1).saturating_add(1);
            if self.used.insert(self.next) || self.next == u32::MAX {
                return self.next;
            }
        }
    }

    /// The list's objects that exist, each once (sections and custom shows: no other list holds
    /// them).
    fn listed(&self, owner: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        let mut seen = HashSet::new();
        Ok(self.replica.list(owner, field)?.into_iter().filter(|o| self.replica.exists(o) && seen.insert(o.clone())).collect())
    }

    /// The list's objects that exist and were not listed before (anywhere in this read).
    fn live(&mut self, owner: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        let items = self.replica.list(owner, field)?;
        let mut out = Vec::with_capacity(items.len());
        for o in items {
            if !self.replica.exists(&o) {
                continue;
            }
            if self.guard.is_some_and(|guard| guard.contains(&o)) {
                self.conflict = true;
                continue;
            }
            if self.seen.insert(o.clone()) {
                out.push(o);
            }
        }
        Ok(out)
    }

    fn translated(&self, object: &ObjectId, field: &str, connectors: bool) -> KdmResult<Value> {
        let mut v = self.replica.get(object, field)?;
        to_locals(&mut v, &self.locals, connectors);
        Ok(v)
    }

    fn shape_nodes(&mut self, owner: &ObjectId, field: &str, scope: &ObjectId, depth: usize) -> KdmResult<Vec<ShapeNode>> {
        if depth > MAX_DEPTH {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for object in self.live(owner, field)? {
            // The stored positional number wins when moving into a part required renumbering.
            let known = self.known.shapes.get(&object).map(|s| s.0);
            let stored = stored_shape_number(self.replica, &object);
            let id = ShapeId(self.mint_shape_of(scope, stored, known));
            self.ids.insert_shape(scope, id, object.clone());
            self.locals.shapes.insert(object.clone(), id);
            let props = crate::look::of(self.replica, &object)?;
            let group = props.get("kind").and_then(|k| k.get("type")).and_then(Value::as_str) == Some("group");
            let children = if group { self.shape_nodes(&object, "children", scope, depth + 1)? } else { Vec::new() };
            out.push(ShapeNode { object, id, props, children });
        }
        Ok(out)
    }

    fn shapes(&mut self, nodes: &[ShapeNode], depth: usize) -> KdmResult<Vec<Shape>> {
        nodes.iter().map(|node| self.shape(node, depth)).collect()
    }

    fn shape(&mut self, node: &ShapeNode, depth: usize) -> KdmResult<Shape> {
        let replica = self.replica;
        let mut v = match &node.props {
            Value::Object(map) => Value::Object(map.clone()),
            _ => Value::Object(Default::default()),
        };
        if let Some(fingerprint) = v.pointer_mut("/diagram/fingerprint")
            && let Some(number) = fingerprint.as_str().and_then(|text| u64::from_str_radix(text, 16).ok())
        {
            *fingerprint = Value::from(number);
        }
        if let Some(map) = v.as_object_mut() {
            map.insert("id".into(), Value::from(node.id.0));
            // A slide shape's identity is its key (its tool id); a layout's or master's shape has
            // no key and keeps it in `identity`.
            let key = replica.get(&node.object, "key")?;
            let identity = match key {
                Value::String(key) => Value::String(key),
                _ => match replica.get(&node.object, "identity")? {
                    Value::String(identity) => Value::String(identity),
                    _ => Value::String(String::new()),
                },
            };
            map.insert("identity".into(), identity);
            map.insert("name".into(), replica.get(&node.object, "name")?);
            map.insert("xfrm".into(), replica.get(&node.object, "xfrm")?);
            map.insert("text".into(), Value::Null);
            let kind = map.entry("kind").or_insert_with(|| serde_json::json!({ "type": "shape" }));
            if let Some(kind) = kind.as_object_mut()
                && kind.get("type").and_then(Value::as_str) == Some("group")
            {
                kind.insert("children".into(), Value::Array(Vec::new()));
            }
        }
        to_locals(&mut v, &self.locals, true);
        let mut shape: Shape = serde_json::from_value(v).map_err(kdm_err)?;
        shape.text = self.text_of(BodyOwner::Shape(node.object.clone()), &node.object, "body")?;
        if let Some(children) = shape.children_mut() {
            *children = self.shapes(&node.children, depth + 1)?;
        }
        if let ShapeKind::Table(table) = &mut shape.kind {
            *table = self.table(&node.object)?;
        }
        // A chart's members are its `Chart` object's.
        if matches!(shape.kind, ShapeKind::Chart(_))
            && let Some(parts) = crate::charts::read(replica, &node.object)?
        {
            let mut kind = Value::Object(parts);
            if let Some(map) = kind.as_object_mut() {
                map.insert("type".into(), Value::String("chart".into()));
            }
            let mut wrapped = serde_json::json!({ "kind": kind });
            to_locals(&mut wrapped, &self.locals, true);
            if let Some(kind) = wrapped.get_mut("kind").map(Value::take) {
                shape.kind = serde_json::from_value(kind).map_err(kdm_err)?;
            }
        }
        Ok(shape)
    }

    /// The table of the shape `shape`; an empty one if its object is gone.
    fn table(&mut self, shape: &ObjectId) -> KdmResult<Table> {
        let Value::String(object) = self.replica.get(shape, "table")? else { return Ok(Table::default()) };
        let object = ObjectId::from_string(object);
        if !self.replica.exists(&object) {
            return Ok(Table::default());
        }
        let mut table: Table = from(self.replica.get(&object, "props")?)?;
        let cols = self.live(&object, "cols")?;
        let rows = self.live(&object, "rows")?;
        let cells: HashMap<String, Value> = self.replica.map_entries(&object, "cells")?.into_iter().collect();
        table.cols = cols.iter().map(|c| Ok(self.replica.get(c, "width")?.as_f64().unwrap_or(0.0))).collect::<KdmResult<_>>()?;
        table.col_ids = cols.iter().map(|col| Ok(self.replica.get(col, "identity")?.as_str().unwrap_or("").to_owned())).collect::<KdmResult<_>>()?;
        table.rows = Vec::with_capacity(rows.len());
        for row in &rows {
            let height = self.replica.get(row, "height")?.as_f64().unwrap_or(0.0);
            let mut out =
                Row { identity: self.replica.get(row, "identity")?.as_str().unwrap_or("").to_owned(), height, cells: Vec::with_capacity(cols.len()) };
            for col in &cols {
                let cell = match cells.get(&format!("{}:{}", row.as_str(), col.as_str())) {
                    Some(v) if !v.is_null() => {
                        let mut v = v.clone();
                        let body = v.as_object_mut().and_then(|map| map.remove("body"));
                        to_locals(&mut v, &self.locals, false);
                        let mut cell: Cell = serde_json::from_value(v).map_err(kdm_err)?;
                        if let Some(Value::String(body)) = body {
                            let owner = BodyOwner::Cell { table: object.clone(), row: row.clone(), col: col.clone() };
                            cell.text = self.body(owner, &ObjectId::from_string(body))?.unwrap_or_else(|| TextBody::from_text(""));
                        }
                        cell
                    }
                    _ => Cell::new(""),
                };
                out.cells.push(cell);
            }
            table.rows.push(out);
        }
        self.ids.tables.insert(shape.clone(), TableIds { table: object, rows, cols });
        Ok(table)
    }

    /// The text body `owner` holds: the `TextBody` its `field` names, else none.
    fn text_of(&mut self, owner: BodyOwner, object: &ObjectId, field: &str) -> KdmResult<Option<TextBody>> {
        match self.replica.get(object, field)? {
            Value::String(body) => self.body(owner, &ObjectId::from_string(body)),
            _ => Ok(None),
        }
    }

    /// A `TextBody` object and its paragraphs, recorded in the id maps under `owner`; `None` if
    /// it is gone.
    fn body(&mut self, owner: BodyOwner, object: &ObjectId) -> KdmResult<Option<TextBody>> {
        if !self.replica.exists(object) || self.replica.object_type(object)? != "TextBody" {
            return Ok(None);
        }
        let mut body: TextBody = from(self.translated(object, "props", false)?)?;
        body.paragraphs.clear();
        let listed = self.live(object, "paragraphs")?;
        for item in &listed {
            let delta = self.replica.text_delta(item, "text")?;
            let mut paragraph: Paragraph = from(self.translated(item, "props", false)?)?;
            let locals = &self.locals;
            paragraph.runs = crate::text::from_runs(&delta, &|v: &mut Value| to_locals(v, locals, false))?;
            body.paragraphs.push(paragraph);
            self.ids.texts.insert(item.clone(), Arc::new(delta));
        }
        self.ids.bodies.insert(owner, BodyIds { body: object.clone(), paragraphs: listed });
        Ok(Some(body))
    }

    /// A slide's kept timing tree, its shapes as the reader's ids (a shape it names that the
    /// reader does not hold is dropped, so the writer builds the timing from the animations).
    fn kept_timing(&self, timing: Option<Value>) -> KdmResult<Option<kition_slide_model::anim::KeptTiming>> {
        let Some(mut v) = timing else { return Ok(None) };
        if let Some(targets) = v.get_mut("targets").and_then(Value::as_array_mut) {
            targets.retain_mut(|target| match target.get("shape").and_then(|s| shape_in(s, &self.locals)) {
                Some(local) => {
                    if let Some(map) = target.as_object_mut() {
                        map.insert("shape".into(), local);
                    }
                    true
                }
                None => false,
            });
        }
        Ok(Some(from(v)?))
    }

    fn slide(&mut self, node: &SlideNode, fallback_layout: LayoutId) -> KdmResult<Slide> {
        let replica = self.replica;
        let object = &node.object;
        let props: SlideProps = from(replica.get(object, "props")?)?;
        let layout = match replica.get(object, "layout")? {
            Value::String(s) => self.layouts.get(&ObjectId::from_string(s)).copied().unwrap_or(fallback_layout),
            _ => fallback_layout,
        };
        let mut animations = Vec::new();
        let mut animation_objects = Vec::new();
        for item in self.live(object, "animations")? {
            let mut v = replica.get(&item, "props")?;
            let Some(shape) = v.get("shape").and_then(|s| shape_in(s, &self.locals)) else { continue };
            if let Some(map) = v.as_object_mut() {
                map.insert("shape".into(), shape);
                if let Some(trigger) = map.get("trigger").cloned() {
                    map.insert("trigger".into(), shape_in(&trigger, &self.locals).unwrap_or(Value::Null));
                }
            }
            to_locals(&mut v, &self.locals, false);
            let animation: Animation = serde_json::from_value(v).map_err(kdm_err)?;
            animations.push(animation);
            animation_objects.push(item);
        }
        self.ids.animations.insert(object.clone(), animation_objects);
        Ok(Slide {
            id: node.id,
            layout,
            name: props.name,
            shapes: self.shapes(&node.shapes, 0)?,
            background: serde_json::from_value(self.translated(object, "background", false)?).map_err(kdm_err)?,
            hidden: props.hidden,
            show_master_shapes: props.show_master_shapes,
            transition: serde_json::from_value(self.translated(object, "transition", false)?).map_err(kdm_err)?,
            animations,
            timing: self.kept_timing(props.timing)?,
            notes: self.text_of(BodyOwner::Notes(object.clone()), object, "notesBody")?.unwrap_or_default(),
            notes_page: props.notes_page,
            comments: Vec::new(),
            tags: props.tags,
            raw_ext: serde_json::from_value(replica.get(object, "rawExt")?).map_err(kdm_err)?,
            file_id: props.file_id,
        })
    }
}
