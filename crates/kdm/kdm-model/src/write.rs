//! `SlideFile` -> replica: a full write (a new replica, an import) and an incremental diff (the
//! engine command bridge) that writes only what changed.
//!
//! Both run the same passes, a full write being a diff from nothing:
//! 1. every new master, layout, slide, media item and shape gets its object, so every reference
//!    written after can name it;
//! 2. what `after` no longer holds is deleted, with everything under it (a slide's shapes and
//!    animations, a group's children, a table's rows and columns) except what `after` still holds
//!    elsewhere (an ungrouped shape, a layout moved to another master);
//! 3. every field that differs is set, and every list made to hold `after`'s order (lists are
//!    aligned by object identity, so a move is a move, not a rewrite).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ObjectId, TextRun, Tx};
use kition_kit::session::{BlobSink, write_rich_text};
use kition_slide_model::table::Cell;
use kition_slide_model::text::{Paragraph, TextBody};
use kition_slide_model::{Animation, Layout, Master, MediaItem, Shape, ShapeId, ShapeKind, Slide, SlideFile, Table};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::ids::{BodyIds, BodyOwner, Ids, TableIds, check_unique};
use crate::list::{align, sync_list};
use crate::pieces::{self, Anchor, Composition, Piece, assign, canon, char_len, compose, text_of};
use crate::refs::{shape_out, to_objects};
use crate::text::{INLINE, INLINE_MARK, align_paragraphs, body_props, paragraph_props, to_runs};
use crate::{KdmResult, kdm_err, root_id};

const MAX_DEPTH: usize = 64;

fn value(v: &impl Serialize) -> KdmResult<Value> {
    serde_json::to_value(v).map_err(kdm_err)
}

fn fields(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

/// Writes a whole slide into a fresh replica's creating transaction. Answers the
/// `SlideFile` object's id.
pub fn write_document(tx: &mut Tx<'_>, p: &SlideFile, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<ObjectId> {
    let root = root_id();
    tx.create_with_id("SlideFile", &root, Map::new())?;
    *ids = Ids::default();
    Writer { tx, ids, blobs, fresh: HashSet::new() }.write(None, p)?;
    Ok(root)
}

/// Writes only what differs between `before` (the slide `ids` describe) and `after`. Answers
/// whether it wrote anything. Errs, writing nothing usable, if `after` holds an id twice where it
/// must be unique (see [`crate::decode`]).
pub fn write_changes(tx: &mut Tx<'_>, before: &SlideFile, after: &SlideFile, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<bool> {
    Writer { tx, ids, blobs, fresh: HashSet::new() }.write(Some(before), after)
}

/// Writes only what differs in the comment threads between `before` (the slide `ids`
/// describe) and `after` (rule M23, `crate::comments`): every object it writes is a
/// `CommentThread` or a `Comment`, so the change is one a commenter may push. Answers whether it
/// wrote anything.
pub fn write_comment_changes(tx: &mut Tx<'_>, before: &SlideFile, after: &SlideFile, ids: &mut Ids) -> KdmResult<bool> {
    let threads = crate::comments::before(Some(before), ids);
    crate::comments::write(tx, &threads, after, ids)
}

struct Writer<'w, 'tx> {
    tx: &'w mut Tx<'tx>,
    ids: &'w mut Ids,
    blobs: &'w mut dyn BlobSink,
    /// Objects this write created: written in full, whatever `before` held.
    fresh: HashSet<ObjectId>,
}

/// The register fields of one object, as written (ids already object ids).
type Fields = Vec<(&'static str, Value)>;

// --- what each object's registers hold -------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RootProps<'a> {
    doc: &'a kition_slide_model::Properties,
    slide_size: kition_slide_model::Size,
    notes_size: kition_slide_model::Size,
    first_slide_number: u32,
    #[serde(skip_serializing_if = "<[kition_slide_model::TagList]>::is_empty")]
    tags: &'a [kition_slide_model::TagList],
    #[serde(skip_serializing_if = "<[kition_slide_model::FontFace]>::is_empty")]
    font_faces: &'a [kition_slide_model::FontFace],
    /// The file's presentation properties (`SlideFile::pres_props`). The view properties
    /// (`SlideFile::view_props`: the last view, zoom, guides) are private to whoever opened the
    /// file and never replicated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pres_props: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MasterProps<'a> {
    name: &'a str,
    background: &'a Option<kition_slide_model::Background>,
    preserve: bool,
    #[serde(skip_serializing_if = "<[kition_slide_model::TagList]>::is_empty")]
    tags: &'a [kition_slide_model::TagList],
    /// Additive (`Master::hf`): a 0.4.x reader ignores it.
    #[serde(skip_serializing_if = "Option::is_none")]
    hf: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListStyles<'a> {
    title: &'a kition_slide_model::text::ListStyle,
    body: &'a kition_slide_model::text::ListStyle,
    other: &'a kition_slide_model::text::ListStyle,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LayoutProps<'a> {
    name: &'a str,
    kind: kition_slide_model::LayoutType,
    background: &'a Option<kition_slide_model::Background>,
    show_master_shapes: bool,
    preserve: bool,
    /// Additive (`Layout::user_drawn`): a 0.4.x reader ignores it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    user_drawn: bool,
    raw_ext: &'a Option<String>,
    #[serde(skip_serializing_if = "<[kition_slide_model::TagList]>::is_empty")]
    tags: &'a [kition_slide_model::TagList],
    /// Additive (`Layout::hf`): a 0.4.x reader ignores it.
    #[serde(skip_serializing_if = "Option::is_none")]
    hf: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SlideProps<'a> {
    name: &'a str,
    hidden: bool,
    show_master_shapes: bool,
    #[serde(skip_serializing_if = "<[kition_slide_model::TagList]>::is_empty")]
    tags: &'a [kition_slide_model::TagList],
    /// The kept timing tree (`Slide::timing`), its shapes as object ids. Additive: 0.4.x builds
    /// ignore it and write the timing from the animations.
    #[serde(skip_serializing_if = "Option::is_none")]
    timing: Option<Value>,
    /// The notes page kept while the notes are empty (`Slide::notes_page`). Additive: 0.4.x
    /// builds ignore it and write a notes page only for notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes_page: &'a Option<String>,
    /// The slide's id in its file (`Slide::file_id`). Additive: older builds ignore it and number
    /// the slides from 256.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_id: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaProps<'a> {
    name: &'a str,
    content_type: &'a str,
    link: &'a Option<String>,
}

fn refs_out(mut v: Value, ids: &Ids, scope: Option<&ObjectId>) -> Value {
    to_objects(&mut v, ids, scope);
    v
}

fn root_fields(p: &SlideFile, ids: &Ids) -> KdmResult<Fields> {
    let fonts: Vec<Value> =
        p.embedded_fonts.iter().map(|(name, media)| json!([name, ids.media.object(*media).map_or(json!(media.0), |o| json!(o.as_str()))])).collect();
    Ok(vec![
        (
            "props",
            value(&RootProps {
                doc: &p.props,
                slide_size: p.slide_size,
                notes_size: p.notes_size,
                first_slide_number: p.first_slide_number,
                tags: &p.tags,
                font_faces: &p.font_faces,
                pres_props: &p.pres_props,
            })?,
        ),
        ("show", value(&p.show)?),
        ("headerFooter", value(&p.header_footer)?),
        ("defaultTextStyle", refs_out(value(&p.default_text_style)?, ids, None)),
        ("embeddedFonts", Value::Array(fonts)),
        ("embeddedFontList", value(&p.embedded_font_list)?),
        ("embeddedFontRels", refs_out(value(&p.embedded_font_rels)?, ids, None)),
        ("notesMaster", refs_out(value(&p.notes_master)?, ids, None)),
        ("handoutMaster", refs_out(value(&p.handout_master)?, ids, None)),
        ("rawExt", value(&p.raw_ext)?),
    ])
}

/// Whether every field [`root_fields`] writes is the same (so none needs serializing).
fn root_unchanged(b: &SlideFile, a: &SlideFile) -> bool {
    b.props == a.props
        && b.slide_size == a.slide_size
        && b.notes_size == a.notes_size
        && b.first_slide_number == a.first_slide_number
        && b.tags == a.tags
        && b.font_faces == a.font_faces
        && b.pres_props == a.pres_props
        && b.show == a.show
        && b.header_footer == a.header_footer
        && b.default_text_style == a.default_text_style
        && b.embedded_fonts == a.embedded_fonts
        && b.embedded_font_list == a.embedded_font_list
        && b.embedded_font_rels == a.embedded_font_rels
        && b.notes_master == a.notes_master
        && b.handout_master == a.handout_master
        && b.raw_ext == a.raw_ext
}

fn master_fields(m: &Master, ids: &Ids) -> KdmResult<Fields> {
    Ok(vec![
        (
            "props",
            refs_out(value(&MasterProps { name: &m.name, background: &m.background, preserve: m.preserve, tags: &m.tags, hf: &m.hf })?, ids, None),
        ),
        ("theme", refs_out(value(&m.theme)?, ids, None)),
        ("listStyles", refs_out(value(&ListStyles { title: &m.title_style, body: &m.body_style, other: &m.other_style })?, ids, None)),
        ("colorMap", value(&m.color_map)?),
        ("rawExt", value(&m.raw_ext)?),
    ])
}

fn layout_fields(l: &Layout, ids: &Ids) -> KdmResult<Fields> {
    let props = LayoutProps {
        name: &l.name,
        kind: l.kind,
        background: &l.background,
        show_master_shapes: l.show_master_shapes,
        preserve: l.preserve,
        user_drawn: l.user_drawn,
        raw_ext: &l.raw_ext,
        tags: &l.tags,
        hf: &l.hf,
    };
    Ok(vec![("identity", json!(l.identity)), ("props", refs_out(value(&props)?, ids, None))])
}

fn slide_fields(s: &Slide, scope: &ObjectId, ids: &Ids) -> KdmResult<Fields> {
    let timing = match &s.timing {
        Some(t) => {
            let mut v = value(t)?;
            if let Some(targets) = v.get_mut("targets").and_then(Value::as_array_mut) {
                for target in targets {
                    if let Some(shape) = target.get_mut("shape") {
                        shape_out(shape, ids, scope);
                    }
                }
            }
            Some(v)
        }
        None => None,
    };
    Ok(vec![
        ("layout", ids.layouts.object(s.layout).map_or(Value::Null, |o| json!(o.as_str()))),
        (
            "props",
            value(&SlideProps {
                name: &s.name,
                hidden: s.hidden,
                show_master_shapes: s.show_master_shapes,
                tags: &s.tags,
                timing,
                notes_page: &s.notes_page,
                file_id: s.file_id,
            })?,
        ),
        ("background", refs_out(value(&s.background)?, ids, None)),
        ("transition", refs_out(value(&s.transition)?, ids, None)),
        ("rawExt", value(&s.raw_ext)?),
        // The slide's id, as slide_inspect lists it: its tool id `slide:<id>` (OM9).
        ("slideId", json!(s.id.0)),
    ])
}

fn animation_props(a: &Animation, scope: &ObjectId, ids: &Ids) -> KdmResult<Value> {
    let mut v = refs_out(value(a)?, ids, None);
    for key in ["shape", "trigger"] {
        if let Some(shape) = v.get_mut(key) {
            shape_out(shape, ids, scope);
        }
    }
    Ok(v)
}

fn media_fields(m: &MediaItem, blobs: &mut dyn BlobSink) -> KdmResult<Fields> {
    let blob = if m.data.is_empty() { Value::Null } else { value(&blobs.put(&m.data, &m.content_type))? };
    Ok(vec![("blob", blob), ("props", value(&MediaProps { name: &m.name, content_type: &m.content_type, link: &m.link })?)])
}

pub(crate) fn kind_name(kind: &ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Shape => "shape",
        ShapeKind::Picture { .. } => "picture",
        ShapeKind::Group { .. } => "group",
        ShapeKind::Connector { .. } => "connector",
        ShapeKind::Table(_) => "table",
        ShapeKind::Chart(_) => "chart",
        ShapeKind::Media(_) => "media",
        ShapeKind::Ink { .. } => "ink",
        ShapeKind::Opaque { .. } => "opaque",
    }
}

/// A shape's registers but `children` and `table`: `props` is the whole shape without its id,
/// name, transform and text, and without a group's children or a table's grid.
fn shape_fields(s: &Shape, scope: &ObjectId, ids: &Ids) -> KdmResult<Fields> {
    let mut props = value(s)?;
    if let Some(map) = props.as_object_mut() {
        for key in ["id", "identity", "name", "xfrm", "text"] {
            map.remove(key);
        }
        if let Some(kind) = map.get_mut("kind").and_then(Value::as_object_mut) {
            match &s.kind {
                ShapeKind::Group { .. } => {
                    kind.remove("children");
                }
                ShapeKind::Table(_) => kind.retain(|key, _| key == "type"),
                _ => {}
            }
        }
    }
    if let Some(fingerprint) = props.pointer_mut("/diagram/fingerprint")
        && let Some(number) = fingerprint.as_u64()
    {
        *fingerprint = Value::String(format!("{number:016x}"));
    }
    to_objects(&mut props, ids, Some(scope));
    // A chart's members are its `Chart` object's (`chart_parts`).
    if let (ShapeKind::Chart(_), Some(kind)) = (&s.kind, props.get_mut("kind")) {
        crate::charts::split(kind);
    }
    let media = match &s.kind {
        ShapeKind::Picture { fill } => Some(fill.media),
        ShapeKind::Media(clip) => Some(clip.media),
        _ => None,
    };
    Ok(vec![
        ("kind", json!(kind_name(&s.kind))),
        ("name", json!(s.name)),
        ("xfrm", value(&s.xfrm)?),
        ("props", props),
        ("media", media.and_then(|m| ids.media.object(m)).map_or(Value::Null, |o| json!(o.as_str()))),
        ("key", shape_key(s, scope, ids).map_or(Value::Null, Value::String)),
        // A layout's or master's shape has no tool id; its identity is kept here.
        (
            "identity",
            match ids.slides.local(scope) {
                Some(_) => Value::Null,
                None => kition_slide_model::identity::canonical(&s.identity).map_or(Value::Null, Value::String),
            },
        ),
        ("address", ids.slides.local(scope).map_or(Value::Null, |slide| json!({"slide": slide.0, "number": s.id.0}))),
    ])
}

/// The persistent GUID shared by a slide shape's tool id and its DrawingML creation extension.
/// Shapes of layouts and masters have none (their `identity` field holds it).
pub(crate) fn shape_key(s: &Shape, scope: &ObjectId, ids: &Ids) -> Option<String> {
    ids.slides.local(scope)?;
    kition_slide_model::identity::canonical(&s.identity)
}

/// A chart shape's chart members (its ids as object ids), as its `Chart` object holds them.
fn chart_parts(s: &Shape, scope: &ObjectId, ids: &Ids) -> KdmResult<Map<String, Value>> {
    let mut v = value(&s.kind)?;
    let mut wrapped = json!({ "kind": v.take() });
    to_objects(&mut wrapped, ids, Some(scope));
    Ok(wrapped.get_mut("kind").map(crate::charts::split).unwrap_or_default())
}

/// A table's `props`: everything but its grid.
fn table_props(t: &Table) -> KdmResult<Value> {
    let mut v = value(t)?;
    if let Some(map) = v.as_object_mut() {
        map.remove("rows");
        map.remove("cols");
        map.remove("colIds");
    }
    Ok(v)
}

fn cell_key(row: &ObjectId, col: &ObjectId) -> String {
    format!("{}:{}", row.as_str(), col.as_str())
}

/// Every shape of a shape tree, by id (first holder wins).
fn index_shapes(shapes: &[Shape]) -> HashMap<ShapeId, &Shape> {
    let mut out = HashMap::new();
    kition_slide_model::walk(shapes, &mut |s, _| {
        out.entry(s.id).or_insert(s);
    });
    out
}

/// One shape tree's holder: its object and its shapes, before and after.
struct Scope<'p> {
    object: ObjectId,
    before: Option<&'p [Shape]>,
    after: &'p [Shape],
}

impl Writer<'_, '_> {
    fn write(&mut self, before: Option<&SlideFile>, after: &SlideFile) -> KdmResult<bool> {
        check_unique(after)?;
        let root = root_id();
        let mut wrote = false;
        let before_threads = crate::comments::before(before, self.ids);

        let before_masters: HashMap<_, &Arc<Master>> = before.map(|b| b.masters.iter().map(|m| (m.id, m)).collect()).unwrap_or_default();
        let before_layouts: HashMap<_, &Layout> =
            before.map(|b| b.masters.iter().flat_map(|m| m.layouts.iter().map(|l| (l.id, l))).collect()).unwrap_or_default();
        let before_slides: HashMap<_, &Arc<Slide>> = before.map(|b| b.slides.iter().map(|s| (s.id, s)).collect()).unwrap_or_default();
        let before_media: HashMap<_, &MediaItem> = before.map(|b| b.media.iter().map(|m| (m.id, m)).collect()).unwrap_or_default();

        // 1. Objects for everything new.
        for item in &after.media {
            if !before_media.contains_key(&item.id) || self.ids.media.object(item.id).is_none() {
                let object = self.tx.create("Media", fields(media_fields(item, self.blobs)?))?;
                self.ids.media.insert(object, item.id);
                wrote = true;
            }
        }
        for master in &after.masters {
            if !before_masters.contains_key(&master.id) || self.ids.masters.object(master.id).is_none() {
                let object = self.tx.create("Master", Map::new())?;
                self.fresh.insert(object.clone());
                self.ids.masters.insert(object, master.id);
            }
            for layout in &master.layouts {
                if !before_layouts.contains_key(&layout.id) || self.ids.layouts.object(layout.id).is_none() {
                    let object = self.tx.create("Layout", Map::new())?;
                    self.fresh.insert(object.clone());
                    self.ids.layouts.insert(object, layout.id);
                }
            }
        }
        for slide in &after.slides {
            if !before_slides.contains_key(&slide.id) || self.ids.slides.object(slide.id).is_none() {
                let object = self.tx.create("Slide", Map::new())?;
                self.fresh.insert(object.clone());
                self.ids.slides.insert(object, slide.id);
            }
        }
        let scopes = self.scopes(after, &before_masters, &before_layouts, &before_slides)?;
        let mut identities = HashMap::new();
        for scope in &scopes {
            if let Some(shapes) = scope.before {
                kition_slide_model::walk(shapes, &mut |shape, _| {
                    if !shape.identity.is_empty()
                        && let Some(object) = self.ids.shape(&scope.object, shape.id)
                    {
                        identities.insert(shape.identity.clone(), object.clone());
                    }
                });
            }
        }
        let mut keep: HashSet<ObjectId> = HashSet::new();
        for scope in &scopes {
            keep.insert(scope.object.clone());
            let before_ids: HashSet<ShapeId> = scope.before.map(|b| index_shapes(b).into_keys().collect()).unwrap_or_default();
            let mut new = Vec::new();
            kition_slide_model::walk(scope.after, &mut |s, _| {
                new.push((s.id, before_ids.contains(&s.id), kind_name(&s.kind), s.name.clone(), s.identity.clone()))
            });
            for (id, existed, kind, name, identity) in new {
                let object = match self.ids.shape(&scope.object, id) {
                    Some(object) if existed => object.clone(),
                    _ => {
                        if let Some(object) = identities.get(&identity) {
                            self.ids.insert_shape(&scope.object, id, object.clone());
                            keep.insert(object.clone());
                            continue;
                        }
                        let values = fields(vec![("kind", json!(kind)), ("name", json!(name))]);
                        // The identity's id when it is free; a shape replacing one deleted with
                        // its identity (a file newer than its replica) takes a fresh one.
                        let derived = kition_slide_model::identity::guid(&identity)
                            .map(|guid| ObjectId::from_string(guid.trim_matches(['{', '}']).to_ascii_lowercase()))
                            .and_then(|id| self.tx.create_with_id("Shape", &id, values.clone()).ok());
                        let object = match derived {
                            Some(object) => object,
                            None => self.tx.create("Shape", values)?,
                        };
                        self.fresh.insert(object.clone());
                        self.ids.insert_shape(&scope.object, id, object.clone());
                        object
                    }
                };
                keep.insert(object);
            }
        }
        for slide in &after.slides {
            if let Some(object) = self.ids.slides.object(slide.id) {
                keep.insert(object.clone());
            }
        }
        for master in &after.masters {
            for layout in &master.layouts {
                if let Some(object) = self.ids.layouts.object(layout.id) {
                    keep.insert(object.clone());
                }
            }
        }

        // 2. Deletes.
        let mut deleted = false;
        if let Some(before) = before {
            let after_slides: HashSet<_> = after.slides.iter().map(|s| s.id).collect();
            let after_masters: HashSet<_> = after.masters.iter().map(|m| m.id).collect();
            let after_layouts: HashSet<_> = after.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.id)).collect();
            let after_media: HashSet<_> = after.media.iter().map(|m| m.id).collect();
            for slide in &before.slides {
                if !after_slides.contains(&slide.id)
                    && let Some(object) = self.ids.slides.remove_local(slide.id)
                {
                    self.delete_tree(&object, &keep, 0)?;
                    deleted = true;
                }
            }
            for master in &before.masters {
                for layout in &master.layouts {
                    if !after_layouts.contains(&layout.id)
                        && let Some(object) = self.ids.layouts.remove_local(layout.id)
                    {
                        self.delete_tree(&object, &keep, 0)?;
                        deleted = true;
                    }
                }
                if !after_masters.contains(&master.id)
                    && let Some(object) = self.ids.masters.remove_local(master.id)
                {
                    self.delete_tree(&object, &keep, 0)?;
                    deleted = true;
                }
            }
            for scope in &scopes {
                let Some(before_shapes) = scope.before else { continue };
                let after_ids: HashSet<ShapeId> = index_shapes(scope.after).into_keys().collect();
                let mut gone = Vec::new();
                kition_slide_model::walk(before_shapes, &mut |s, _| {
                    if !after_ids.contains(&s.id) {
                        gone.push(s.id);
                    }
                });
                for id in gone {
                    if let Some(object) = self.ids.shape(&scope.object, id).cloned() {
                        self.delete_tree(&object, &keep, 0)?;
                        if keep.contains(&object) {
                            self.ids.forget_shape_address(&scope.object, id);
                            continue;
                        }
                    }
                    self.ids.remove_shape(&scope.object, id);
                    deleted = true;
                }
            }
            for item in &before.media {
                if !after_media.contains(&item.id)
                    && let Some(object) = self.ids.media.remove_local(item.id)
                {
                    if self.tx.exists(&object) {
                        self.tx.delete(&object)?;
                    }
                    deleted = true;
                }
            }
        }
        if deleted {
            let tx = &*self.tx;
            self.ids.retain(|object| tx.exists(object));
            wrote = true;
        }

        // 3. Fields and lists.
        if !before.is_some_and(|b| root_unchanged(b, after)) {
            wrote |= self.set_fields(&root, before.map(|b| root_fields(b, self.ids)).transpose()?, root_fields(after, self.ids)?)?;
        }
        for item in &after.media {
            let Some(old) = before_media.get(&item.id) else { continue };
            let Some(object) = self.ids.media.object(item.id).cloned() else { continue };
            if !Arc::ptr_eq(&old.data, &item.data) && old.data != item.data {
                let fields = media_fields(item, self.blobs)?;
                wrote |= self.set_fields(&object, None, fields)?;
            } else if old.name != item.name || old.content_type != item.content_type || old.link != item.link {
                let props = value(&MediaProps { name: &item.name, content_type: &item.content_type, link: &item.link })?;
                self.tx.set(&object, "props", props)?;
                wrote = true;
            }
        }
        for master in &after.masters {
            let object = self.object_of(self.ids.masters.object(master.id))?;
            let old = before_masters.get(&master.id).copied().filter(|_| !self.fresh.contains(&object));
            if old.is_some_and(|o| Arc::ptr_eq(o, master)) {
                continue;
            }
            wrote |= self.set_fields(&object, old.map(|o| master_fields(o, self.ids)).transpose()?, master_fields(master, self.ids)?)?;
            for layout in &master.layouts {
                let layout_object = self.object_of(self.ids.layouts.object(layout.id))?;
                let old = before_layouts.get(&layout.id).copied().filter(|_| !self.fresh.contains(&layout_object));
                if old == Some(layout) {
                    continue;
                }
                wrote |= self.set_fields(&layout_object, old.map(|o| layout_fields(o, self.ids)).transpose()?, layout_fields(layout, self.ids)?)?;
            }
            let order: Vec<ObjectId> = master.layouts.iter().filter_map(|l| self.ids.layouts.object(l.id).cloned()).collect();
            wrote |= sync_list(self.tx, &object, "layouts", &order)? > 0;
        }
        let mut previous = HashMap::new();
        for scope in &scopes {
            if let Some(shapes) = scope.before {
                kition_slide_model::walk(shapes, &mut |shape, _| {
                    if !shape.identity.is_empty() {
                        previous.insert(shape.identity.clone(), shape);
                    }
                });
            }
        }
        for scope in &scopes {
            wrote |= self.write_scope(scope, &previous)?;
        }
        for slide in &after.slides {
            let object = self.object_of(self.ids.slides.object(slide.id))?;
            let old = before_slides.get(&slide.id).copied().filter(|_| !self.fresh.contains(&object));
            if old.is_some_and(|o| Arc::ptr_eq(o, slide) || **o == **slide) {
                continue;
            }
            wrote |=
                self.set_fields(&object, old.map(|o| slide_fields(o, &object, self.ids)).transpose()?, slide_fields(slide, &object, self.ids)?)?;
            wrote |= self.write_notes(&object, old.map(|o| &o.notes), &slide.notes)?;
            wrote |= self.write_animations(&object, old.map(|o| o.animations.as_slice()), &slide.animations)?;
        }
        // 4. Sections and custom shows: their members name slides, every one of which has its
        // object now.
        let fresh_slide = after.slides.iter().any(|s| self.ids.slides.object(s.id).is_some_and(|o| self.fresh.contains(o)));
        if fresh_slide || before.is_none_or(|b| b.sections != after.sections) {
            let old =
                before.map(|b| b.sections.iter().map(|s| Group { identity: Some(s.identity.as_str()), name: &s.name, slides: &s.slides }).collect());
            let new = after.sections.iter().map(|s| Group { identity: Some(s.identity.as_str()), name: &s.name, slides: &s.slides }).collect();
            wrote |= self.write_groups(crate::members::SECTION, "sections", old, new)?;
        }
        if fresh_slide || before.is_none_or(|b| b.custom_shows != after.custom_shows) {
            let old = before.map(|b| b.custom_shows.iter().map(|s| Group { identity: None, name: &s.name, slides: &s.slides }).collect());
            let new = after.custom_shows.iter().map(|s| Group { identity: None, name: &s.name, slides: &s.slides }).collect();
            wrote |= self.write_groups(crate::members::SHOW, "customShows", old, new)?;
        }
        // 5. Comment threads (beside the slides, M23), the deleted slides' ones with them.
        wrote |= crate::comments::write(self.tx, &before_threads, after, self.ids)?;
        let masters: Vec<ObjectId> = after.masters.iter().filter_map(|m| self.ids.masters.object(m.id).cloned()).collect();
        let slides: Vec<ObjectId> = after.slides.iter().filter_map(|s| self.ids.slides.object(s.id).cloned()).collect();
        // Media is a shared pool, not a stacking order. Import discovers it in use order.
        let mut pool: Vec<_> = after.media.iter().collect();
        pool.sort_by(|a, b| (&a.name, &a.content_type, &a.link).cmp(&(&b.name, &b.content_type, &b.link)));
        let media: Vec<ObjectId> = pool.iter().filter_map(|m| self.ids.media.object(m.id).cloned()).collect();
        wrote |= sync_list(self.tx, &root, "masters", &masters)? > 0;
        wrote |= sync_list(self.tx, &root, "slides", &slides)? > 0;
        wrote |= sync_list(self.tx, &root, "media", &media)? > 0;
        Ok(wrote)
    }

    /// Writes the sections or custom shows (`ty`, listed in the root's `field`): `before` the view's
    /// (whose objects `ids` holds in order), `after` the new ones. Each is paired with the one it
    /// was by its content, else its identity (a section's) or its name, so a rename, a move or a
    /// change of members writes only that; members are written by entry (`crate::members`).
    fn write_groups(&mut self, ty: &str, field: &str, before: Option<Vec<Group<'_>>>, after: Vec<Group<'_>>) -> KdmResult<bool> {
        let known = if ty == crate::members::SECTION { self.ids.sections.clone() } else { self.ids.shows.clone() };
        // A view whose objects do not match it has nothing to pair with: its objects go.
        let before = before.filter(|b| b.len() == known.len()).unwrap_or_default();
        let key = |g: &Group<'_>| g.identity.filter(|i| !i.is_empty()).unwrap_or(g.name).to_owned();
        let old: Vec<_> = before.iter().map(|g| (g.clone(), key(g))).collect();
        let new: Vec<_> = after.iter().map(|g| (g.clone(), key(g))).collect();
        let partner = align(&old, &new);
        let mut wrote = false;
        let mut objects = Vec::with_capacity(after.len());
        for (j, group) in after.iter().enumerate() {
            let paired =
                partner.get(j).copied().flatten().and_then(|i| known.get(i).cloned().map(|object| (i, object))).filter(|(_, o)| self.tx.exists(o));
            let object = match paired {
                Some((i, object)) => {
                    let was = before.get(i);
                    if was.map(|g| g.name) != Some(group.name) {
                        self.tx.set(&object, "name", json!(group.name))?;
                        wrote = true;
                    }
                    if let Some(identity) = group.identity
                        && was.and_then(|g| g.identity) != Some(identity)
                    {
                        self.tx.set(&object, "identity", if identity.is_empty() { Value::Null } else { json!(identity) })?;
                        wrote = true;
                    }
                    object
                }
                None => {
                    let mut values = fields(vec![("name", json!(group.name))]);
                    if let Some(identity) = group.identity.filter(|i| !i.is_empty()) {
                        values.insert("identity".into(), json!(identity));
                    }
                    wrote = true;
                    self.tx.create(ty, values)?
                }
            };
            wrote |= crate::members::write(self.tx, &object, &crate::members::entries(group.slides, self.ids))?;
            objects.push(object);
        }
        let used: HashSet<&ObjectId> = objects.iter().collect();
        for object in &known {
            if !used.contains(object) && self.tx.exists(object) {
                self.tx.delete(object)?;
                wrote = true;
            }
        }
        wrote |= sync_list(self.tx, &root_id(), field, &objects)? > 0;
        if ty == crate::members::SECTION {
            self.ids.sections = objects;
        } else {
            self.ids.shows = objects;
        }
        Ok(wrote)
    }

    fn object_of(&self, object: Option<&ObjectId>) -> KdmResult<ObjectId> {
        object.cloned().ok_or_else(|| kdm_err("an object the write just made is missing from the id maps"))
    }

    /// Every shape tree of `after` with its holder's object and, if the holder existed, its shapes
    /// before. An unchanged slide (the same `Arc`) is left out: nothing in it needs writing.
    fn scopes<'p>(
        &self,
        after: &'p SlideFile,
        before_masters: &HashMap<kition_slide_model::MasterId, &'p Arc<Master>>,
        before_layouts: &HashMap<kition_slide_model::LayoutId, &'p Layout>,
        before_slides: &HashMap<kition_slide_model::SlideId, &'p Arc<Slide>>,
    ) -> KdmResult<Vec<Scope<'p>>> {
        let mut out = Vec::new();
        for master in &after.masters {
            let object = self.object_of(self.ids.masters.object(master.id))?;
            let old = before_masters.get(&master.id).copied().filter(|_| !self.fresh.contains(&object));
            if old.is_some_and(|o| Arc::ptr_eq(o, master)) {
                continue;
            }
            out.push(Scope { before: old.map(|o| o.shapes.as_slice()), object, after: &master.shapes });
            for layout in &master.layouts {
                let object = self.object_of(self.ids.layouts.object(layout.id))?;
                let old = before_layouts.get(&layout.id).copied().filter(|_| !self.fresh.contains(&object));
                out.push(Scope { before: old.map(|o| o.shapes.as_slice()), object, after: &layout.shapes });
            }
        }
        for slide in &after.slides {
            let object = self.object_of(self.ids.slides.object(slide.id))?;
            let old = before_slides.get(&slide.id).copied().filter(|_| !self.fresh.contains(&object));
            if old.is_some_and(|o| Arc::ptr_eq(o, slide)) {
                continue;
            }
            out.push(Scope { before: old.map(|o| o.shapes.as_slice()), object, after: &slide.shapes });
        }
        Ok(out)
    }

    /// Sets each field of `object` whose value differs from `before` (all of them, `null`s aside,
    /// when there is no `before`). Answers whether it set any.
    fn set_fields(&mut self, object: &ObjectId, before: Option<Fields>, after: Fields) -> KdmResult<bool> {
        let mut wrote = false;
        for (index, (field, v)) in after.into_iter().enumerate() {
            let old = before.as_ref().and_then(|b| b.get(index)).map(|(_, v)| v);
            let differs = match old {
                Some(old) => *old != v,
                None => !v.is_null(),
            };
            if differs {
                self.tx.set(object, field, v)?;
                wrote = true;
            }
        }
        Ok(wrote)
    }

    /// Deletes `object` and everything under it, except objects in `keep` (and what is under them).
    fn delete_tree(&mut self, object: &ObjectId, keep: &HashSet<ObjectId>, depth: usize) -> KdmResult<()> {
        if depth > MAX_DEPTH || keep.contains(object) || !self.tx.exists(object) {
            return Ok(());
        }
        let ty = self.tx.object_type(object)?;
        let lists: &[&str] = match ty.as_str() {
            "Master" => &["shapes", "layouts"],
            "Layout" => &["shapes"],
            "Slide" => &["shapes", "animations"],
            "Shape" => &["children"],
            "Table" => &["rows", "cols"],
            "TextBody" => &["paragraphs"],
            _ => &[],
        };
        for field in lists {
            for item in self.tx.list_items(object, field)? {
                self.delete_tree(&item, keep, depth + 1)?;
            }
        }
        let refs: &[&str] = match ty.as_str() {
            "Shape" => &["table", "body", "chart"],
            "Slide" => &["notesBody"],
            _ => &[],
        };
        for field in refs {
            if let Value::String(target) = self.tx.get(object, field)? {
                self.delete_tree(&ObjectId::from_string(target), keep, depth + 1)?;
            }
        }
        if ty == "Table" {
            for (_, cell) in self.tx.map_entries(object, "cells")? {
                if let Some(Value::String(body)) = cell.get("body") {
                    self.delete_tree(&ObjectId::from_string(body.as_str()), keep, depth + 1)?;
                }
            }
        }
        self.tx.delete(object)
    }

    fn write_scope(&mut self, scope: &Scope<'_>, previous: &HashMap<String, &Shape>) -> KdmResult<bool> {
        // A shape is the one of the same identity before, wherever it was (a move to another part);
        // one without an identity (a view not settled) is the one of its number in this part.
        let here = scope.before.map(index_shapes).unwrap_or_default();
        let mut index = HashMap::new();
        kition_slide_model::walk(scope.after, &mut |shape, _| {
            let before = if shape.identity.is_empty() { here.get(&shape.id).copied() } else { previous.get(&shape.identity).copied() };
            if let Some(before) = before {
                index.insert(shape.id, before);
            }
        });
        let field = "shapes";
        self.write_shapes(&scope.object, field, &scope.object, scope.before, scope.after, &index, 0)
    }

    /// Writes `after` (one shape list: a holder's, or a group's children) and makes `owner.field`
    /// list it.
    fn write_shapes(
        &mut self,
        owner: &ObjectId,
        field: &str,
        scope: &ObjectId,
        before: Option<&[Shape]>,
        after: &[Shape],
        index: &HashMap<ShapeId, &Shape>,
        depth: usize,
    ) -> KdmResult<bool> {
        if depth > MAX_DEPTH {
            return Err(kdm_err("groups nest too deep to write"));
        }
        let mut wrote = false;
        for shape in after {
            wrote |= self.write_shape(scope, index.get(&shape.id).copied(), shape, index, depth)?;
        }
        let same_order =
            !self.fresh.contains(owner) && before.is_some_and(|b| b.len() == after.len() && b.iter().zip(after).all(|(x, y)| x.id == y.id));
        if !same_order {
            let order: Vec<ObjectId> = after.iter().filter_map(|s| self.ids.shape(scope, s.id).cloned()).collect();
            wrote |= sync_list(self.tx, owner, field, &order)? > 0;
        }
        Ok(wrote)
    }

    fn write_shape(
        &mut self,
        scope: &ObjectId,
        before: Option<&Shape>,
        after: &Shape,
        index: &HashMap<ShapeId, &Shape>,
        depth: usize,
    ) -> KdmResult<bool> {
        let object = self.object_of(self.ids.shape(scope, after.id))?;
        let before = before.filter(|_| !self.fresh.contains(&object));
        // `before` is the shape as it was, wherever it was: one moved to another part (or
        // renumbered there) compares equal yet has a new address.
        let mut readdressed = false;
        if before.is_some() {
            let address = self.ids.slides.local(scope).map_or(Value::Null, |slide| json!({"slide": slide.0, "number": after.id.0}));
            if self.tx.get(&object, "address")? != address {
                self.tx.set(&object, "address", address)?;
                readdressed = true;
            }
        }
        if before == Some(after) {
            // What a group holds moved with it.
            for child in after.children() {
                readdressed |= self.write_shape(scope, Some(child), child, index, depth + 1)?;
            }
            return Ok(readdressed);
        }
        // The shape's members are entries of its `look` map (schema 5); its other fields are
        // registers.
        let split = |f: Fields| -> (Fields, Value) {
            let mut look = Value::Null;
            let rest = f
                .into_iter()
                .filter_map(|(k, v)| {
                    if k == "props" {
                        look = v;
                        None
                    } else {
                        Some((k, v))
                    }
                })
                .collect();
            (rest, look)
        };
        let (rest, look) = split(shape_fields(after, scope, self.ids)?);
        let (before_rest, before_look) = match before.map(|b| shape_fields(b, scope, self.ids)).transpose()? {
            Some(f) => {
                let (r, l) = split(f);
                (Some(r), Some(l))
            }
            None => (None, None),
        };
        let mut wrote = self.set_fields(&object, before_rest, rest)?;
        wrote |= crate::look::write(self.tx, &object, before_look.as_ref(), &look)?;
        wrote |= self.write_shape_text(&object, before, after)?;
        match &after.kind {
            ShapeKind::Group { children, .. } => {
                let old = before.map(|b| b.children());
                wrote |= self.write_shapes(&object, "children", scope, old, children, index, depth + 1)?;
            }
            _ if before.is_some_and(Shape::is_group) => {
                wrote |= sync_list(self.tx, &object, "children", &[])? > 0;
            }
            _ => {}
        }
        match &after.kind {
            ShapeKind::Chart(_) => {
                wrote |= crate::charts::write(self.tx, &object, &chart_parts(after, scope, self.ids)?)?;
            }
            _ => {
                if let Value::String(chart) = self.tx.get(&object, "chart")? {
                    self.delete_tree(&ObjectId::from_string(chart), &HashSet::new(), 0)?;
                    self.tx.set(&object, "chart", Value::Null)?;
                    wrote = true;
                }
            }
        }
        let old_table = match before.map(|b| &b.kind) {
            Some(ShapeKind::Table(t)) => self.ids.tables.get(&object).cloned().map(|ids| (t, ids)),
            _ => None,
        };
        match &after.kind {
            ShapeKind::Table(t) => wrote |= self.write_table(&object, scope, old_table, t)?,
            _ => {
                if let Some(table) = self.ids.tables.remove(&object) {
                    self.forget_cells(&table.table);
                    self.delete_tree(&table.table, &HashSet::new(), 0)?;
                    self.tx.set(&object, "table", Value::Null)?;
                    wrote = true;
                }
            }
        }
        Ok(wrote)
    }

    fn write_table(&mut self, shape: &ObjectId, scope: &ObjectId, before: Option<(&Table, TableIds)>, after: &Table) -> KdmResult<bool> {
        // The view's grid must match the objects it was read or written with; anything else (a
        // table read without its object) is rewritten as a new table.
        let before = before.filter(|(t, ids)| t.rows.len() == ids.rows.len() && t.cols.len() == ids.cols.len());
        let Some((old, old_ids)) = before else {
            if let Some(stale) = self.ids.tables.remove(shape) {
                self.forget_cells(&stale.table);
                self.delete_tree(&stale.table, &HashSet::new(), 0)?;
            }
            let table = self.tx.create("Table", fields(vec![("props", table_props(after)?)]))?;
            self.tx.set(shape, "table", json!(table.as_str()))?;
            let mut ids = TableIds { table: table.clone(), rows: Vec::new(), cols: Vec::new() };
            for (c, width) in after.cols.iter().enumerate() {
                let col = self.tx.create("Col", fields(vec![("identity", json!(after.col_ids.get(c))), ("width", json!(width))]))?;
                self.tx.list_insert(&table, "cols", c, &col)?;
                ids.cols.push(col);
            }
            for (r, row) in after.rows.iter().enumerate() {
                let row_object = self.tx.create("Row", fields(vec![("identity", json!(row.identity)), ("height", json!(row.height))]))?;
                self.tx.list_insert(&table, "rows", r, &row_object)?;
                for (c, col) in ids.cols.iter().enumerate() {
                    let (_, v) = self.cell_entry(&table, &row_object, col, scope, None, row.cells.get(c))?;
                    self.tx.map_set(&table, "cells", &cell_key(&row_object, col), v)?;
                }
                ids.rows.push(row_object);
            }
            self.ids.tables.insert(shape.clone(), ids);
            return Ok(true);
        };
        let table = old_ids.table.clone();
        let mut wrote = false;
        let props = table_props(after)?;
        if table_props(old)? != props {
            self.tx.set(&table, "props", props)?;
            wrote = true;
        }

        // Pair only immutable member identities. Equal content never proves continuity.
        let before_cols: HashMap<_, _> = old.col_ids.iter().enumerate().map(|(at, id)| (id, at)).collect();
        let col_partner: Vec<_> = after.col_ids.iter().map(|id| before_cols.get(id).copied()).collect();
        let before_rows: HashMap<_, _> = old.rows.iter().enumerate().map(|(at, row)| (&row.identity, at)).collect();
        let row_partner: Vec<_> = after.rows.iter().map(|row| before_rows.get(&row.identity).copied()).collect();

        let kept_cols: HashSet<usize> = col_partner.iter().flatten().copied().collect();
        let kept_rows: HashSet<usize> = row_partner.iter().flatten().copied().collect();
        // Removed columns and rows: their objects, cells and the cells' text bodies.
        for (i, col) in old_ids.cols.iter().enumerate() {
            if !kept_cols.contains(&i) {
                for row in &old_ids.rows {
                    self.tx.map_delete(&table, "cells", &cell_key(row, col))?;
                    self.drop_cell_body(&table, row, col)?;
                }
                if self.tx.exists(col) {
                    self.tx.delete(col)?;
                }
                wrote = true;
            }
        }
        for (i, row) in old_ids.rows.iter().enumerate() {
            if !kept_rows.contains(&i) {
                for (c, col) in old_ids.cols.iter().enumerate() {
                    if kept_cols.contains(&c) {
                        self.tx.map_delete(&table, "cells", &cell_key(row, col))?;
                        self.drop_cell_body(&table, row, col)?;
                    }
                }
                if self.tx.exists(row) {
                    self.tx.delete(row)?;
                }
                wrote = true;
            }
        }
        let mut cols = Vec::with_capacity(after.cols.len());
        for (j, width) in after.cols.iter().enumerate() {
            match col_partner.get(j).copied().flatten() {
                Some(i) => {
                    let col = old_ids.cols.get(i).cloned().ok_or_else(|| kdm_err("a table column has no object"))?;
                    if old.cols.get(i) != Some(width) {
                        self.tx.set(&col, "width", json!(width))?;
                        wrote = true;
                    }
                    cols.push((col, Some(i)));
                }
                None => {
                    cols.push((self.tx.create("Col", fields(vec![("identity", json!(after.col_ids.get(j))), ("width", json!(width))]))?, None));
                    wrote = true;
                }
            }
        }
        let mut rows = Vec::with_capacity(after.rows.len());
        for (j, row) in after.rows.iter().enumerate() {
            match row_partner.get(j).copied().flatten() {
                Some(i) => {
                    let object = old_ids.rows.get(i).cloned().ok_or_else(|| kdm_err("a table row has no object"))?;
                    if old.rows.get(i).map(|r| r.height) != Some(row.height) {
                        self.tx.set(&object, "height", json!(row.height))?;
                        wrote = true;
                    }
                    rows.push((object, Some(i)));
                }
                None => {
                    rows.push((self.tx.create("Row", fields(vec![("identity", json!(row.identity)), ("height", json!(row.height))]))?, None));
                    wrote = true;
                }
            }
        }
        for (r, (row_object, old_r)) in rows.iter().enumerate() {
            let Some(row) = after.rows.get(r) else { continue };
            for (c, (col_object, old_c)) in cols.iter().enumerate() {
                let new_cell = row.cells.get(c);
                // `None`: the cell's row or column is new; `Some(None)`: the old row had no cell there.
                let old_cell = match (old_r, old_c) {
                    (Some(i), Some(k)) => old.rows.get(*i).map(|old_row| old_row.cells.get(*k)),
                    _ => None,
                };
                if old_cell.is_some_and(|o| o == new_cell) {
                    continue;
                }
                let owner = BodyOwner::Cell { table: table.clone(), row: row_object.clone(), col: col_object.clone() };
                let old_body = self.ids.bodies.get(&owner).map(|b| b.body.clone());
                let old_v = match old_cell {
                    Some(Some(cell)) => Some(self.cell_value(cell, old_body.as_ref(), scope)?),
                    Some(None) => Some(Value::Null),
                    None => None,
                };
                let (w, v) = self.cell_entry(&table, row_object, col_object, scope, old_cell.flatten(), new_cell)?;
                wrote |= w;
                if old_v.as_ref() != Some(&v) {
                    self.tx.map_set(&table, "cells", &cell_key(row_object, col_object), v)?;
                    wrote = true;
                }
            }
        }
        let col_objects: Vec<ObjectId> = cols.into_iter().map(|(o, _)| o).collect();
        let row_objects: Vec<ObjectId> = rows.into_iter().map(|(o, _)| o).collect();
        wrote |= sync_list(self.tx, &table, "cols", &col_objects)? > 0;
        wrote |= sync_list(self.tx, &table, "rows", &row_objects)? > 0;
        self.ids.tables.insert(shape.clone(), TableIds { table, rows: row_objects, cols: col_objects });
        Ok(wrote)
    }

    /// A cell's map value: the cell without its text, and the `TextBody` holding the text.
    fn cell_value(&self, cell: &Cell, body: Option<&ObjectId>, scope: &ObjectId) -> KdmResult<Value> {
        let mut v = value(cell)?;
        if let Some(map) = v.as_object_mut() {
            map.remove("text");
            if let Some(body) = body {
                map.insert("body".into(), json!(body.as_str()));
            }
        }
        Ok(refs_out(v, self.ids, Some(scope)))
    }

    /// Writes one cell's text body (`after`; `None`: no cell there) and answers whether it wrote
    /// and the cell's map value.
    fn cell_entry(
        &mut self,
        table: &ObjectId,
        row: &ObjectId,
        col: &ObjectId,
        scope: &ObjectId,
        before: Option<&Cell>,
        after: Option<&Cell>,
    ) -> KdmResult<(bool, Value)> {
        let owner = BodyOwner::Cell { table: table.clone(), row: row.clone(), col: col.clone() };
        let Some(cell) = after else {
            let wrote = self.drop_cell_body(table, row, col)?;
            return Ok((wrote, Value::Null));
        };
        let known = self.ids.bodies.get(&owner).map(|b| b.body.clone());
        let (wrote, body) = match known {
            Some(body) if before.map(|b| &b.text) == Some(&cell.text) => (false, body),
            _ => self.write_body(owner, before.map(|b| &b.text), &cell.text)?,
        };
        Ok((wrote, self.cell_value(cell, Some(&body), scope)?))
    }

    /// Deletes the text body of a cell that is gone. Answers whether there was one.
    fn drop_cell_body(&mut self, table: &ObjectId, row: &ObjectId, col: &ObjectId) -> KdmResult<bool> {
        let owner = BodyOwner::Cell { table: table.clone(), row: row.clone(), col: col.clone() };
        match self.ids.bodies.remove(&owner) {
            Some(ids) => {
                self.delete_tree(&ids.body, &HashSet::new(), 0)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Forgets the cell bodies of `table` (deleted with it).
    fn forget_cells(&mut self, table: &ObjectId) {
        self.ids.bodies.retain(|owner, _| !matches!(owner, BodyOwner::Cell { table: t, .. } if t == table));
    }

    /// A shape's text: its `TextBody` written, or deleted when the shape has no text.
    fn write_shape_text(&mut self, object: &ObjectId, before: Option<&Shape>, after: &Shape) -> KdmResult<bool> {
        let owner = BodyOwner::Shape(object.clone());
        let old_body = self.ids.bodies.get(&owner).map(|b| b.body.clone());
        let old_text = before.and_then(|b| b.text.as_ref());
        let mut wrote = false;
        match &after.text {
            Some(text) => {
                if old_body.is_none() || old_text != Some(text) {
                    let (w, body) = self.write_body(owner, old_text, text)?;
                    wrote |= w;
                    if old_body.as_ref() != Some(&body) {
                        self.tx.set(object, "body", json!(body.as_str()))?;
                        wrote = true;
                    }
                }
            }
            None => {
                if let Some(ids) = self.ids.bodies.remove(&owner) {
                    self.delete_tree(&ids.body, &HashSet::new(), 0)?;
                    self.tx.set(object, "body", Value::Null)?;
                    wrote = true;
                }
            }
        }
        Ok(wrote)
    }

    /// A slide's notes, as [`Writer::write_shape_text`] does a shape's text. Every slide gets a
    /// body, even with empty notes: two people typing a slide's first notes at once then edit one
    /// body and both keep their text, where each creating a body of their own would keep only one
    /// (the `notesBody` register keeps the last write).
    fn write_notes(&mut self, slide: &ObjectId, before: Option<&TextBody>, after: &TextBody) -> KdmResult<bool> {
        let owner = BodyOwner::Notes(slide.clone());
        let old_body = self.ids.bodies.get(&owner).map(|b| b.body.clone());
        if before == Some(after) {
            return Ok(false);
        }
        let (mut wrote, body) = self.write_body(owner, before, after)?;
        if old_body.as_ref() != Some(&body) {
            self.tx.set(slide, "notesBody", json!(body.as_str()))?;
            wrote = true;
        }
        Ok(wrote)
    }

    /// A new `Paragraph` holding `p`.
    fn new_paragraph(&mut self, p: &Paragraph) -> KdmResult<ObjectId> {
        let ids = &*self.ids;
        let fix = |v: &mut Value| to_objects(v, ids, None);
        let props = paragraph_props(p, &fix)?;
        let runs = to_runs(p, &fix)?;
        let object = self.tx.create("Paragraph", fields(vec![("props", props)]))?;
        write_rich_text(self.tx, &object, "text", &[], &runs).map_err(kdm_err)?;
        self.ids.texts.insert(object.clone(), Arc::new(canon(runs)));
        Ok(object)
    }

    /// Writes `after` as `owner`'s text body: the body the view holds for it diffed against
    /// `before` (paragraphs aligned by [`align_paragraphs`], text and run properties written by
    /// character), or, when it holds none matching `before`, a new `TextBody`. Answers whether it
    /// wrote, and the body's object (the caller points the owner at it when it changed).
    fn write_body(&mut self, owner: BodyOwner, before: Option<&TextBody>, after: &TextBody) -> KdmResult<(bool, ObjectId)> {
        let known = self
            .ids
            .bodies
            .get(&owner)
            .cloned()
            .filter(|ids| before.is_some_and(|b| b.paragraphs.len() == ids.paragraphs.len()) && self.tx.exists(&ids.body));
        let (Some(known), Some(old)) = (known, before) else {
            if let Some(stale) = self.ids.bodies.remove(&owner) {
                self.delete_tree(&stale.body, &HashSet::new(), 0)?;
            }
            let props = {
                let ids = &*self.ids;
                body_props(after, &|v: &mut Value| to_objects(v, ids, None))?
            };
            let body = self.tx.create("TextBody", fields(vec![("props", props)]))?;
            let mut paragraphs = Vec::with_capacity(after.paragraphs.len());
            for (j, p) in after.paragraphs.iter().enumerate() {
                let object = self.new_paragraph(p)?;
                self.tx.list_insert(&body, "paragraphs", j, &object)?;
                paragraphs.push(object);
            }
            self.ids.bodies.insert(owner, BodyIds { body: body.clone(), paragraphs, absorbed: Vec::new() });
            return Ok((true, body));
        };
        let body = known.body.clone();
        let mut wrote = false;
        let (old_props, new_props) = {
            let ids = &*self.ids;
            let fix = |v: &mut Value| to_objects(v, ids, None);
            (body_props(old, &fix)?, body_props(after, &fix)?)
        };
        if old_props != new_props {
            self.tx.set(&body, "props", new_props)?;
            wrote = true;
        }
        let absorbed: Vec<ObjectId> = known.absorbed.iter().map(|(object, _)| object.clone()).collect();
        // Every paragraph's field as last read or written, split at its anchors (`pieces`).
        let mut old_fields: HashMap<ObjectId, Composition> = HashMap::with_capacity(known.paragraphs.len() + absorbed.len());
        let mut truths: HashMap<ObjectId, Vec<TextRun>> = HashMap::with_capacity(known.paragraphs.len() + absorbed.len());
        for (i, object) in known.paragraphs.iter().enumerate() {
            let truth = match (self.ids.texts.get(object), old.paragraphs.get(i)) {
                (Some(truth), _) => canon(truth.as_ref().clone()),
                (None, Some(q)) => {
                    let ids = &*self.ids;
                    canon(to_runs(q, &|v: &mut Value| to_objects(v, ids, None))?)
                }
                (None, None) => Vec::new(),
            };
            old_fields.insert(object.clone(), compose(&truth));
            truths.insert(object.clone(), truth);
        }
        for object in &absorbed {
            let truth = self.ids.texts.get(object).map(|truth| canon(truth.as_ref().clone())).unwrap_or_default();
            old_fields.insert(object.clone(), compose(&truth));
            truths.insert(object.clone(), truth);
        }
        let pieces_of = |ids: &Ids, object: &ObjectId| ids.pieces.get(object).cloned().unwrap_or_else(|| vec![Piece::own(object)]);
        let aligned = align_paragraphs(&old.paragraphs, &after.paragraphs);
        let mut partner = aligned.clone();
        let mut merges = self.find_merges(old, after, &known, &mut partner)?;
        let mut gone = gone_paragraphs(&known, &partner, &merges);
        if !merges.is_empty() && !merges_show(&merges, &known, &gone, &old_fields, self.ids, after)? {
            // The anchors would not show the merged text as the view holds it: copy it instead.
            merges.clear();
            partner = aligned;
            gone = gone_paragraphs(&known, &partner, &merges);
        }
        let mut fields = old_fields.clone();
        // The paragraphs `after` adds get their objects first: a split's anchor names one.
        let mut objects = Vec::with_capacity(after.paragraphs.len());
        for (j, p) in after.paragraphs.iter().enumerate() {
            match partner.get(j).copied().flatten().and_then(|i| known.paragraphs.get(i)) {
                Some(object) => objects.push(object.clone()),
                None => {
                    let object = self.new_paragraph(p)?;
                    if let Some(truth) = self.ids.texts.get(&object) {
                        fields.insert(object.clone(), compose(truth));
                    }
                    objects.push(object);
                    wrote = true;
                }
            }
        }
        // A deleted paragraph's view took its tails with it.
        for object in &gone {
            for piece in pieces_of(self.ids, object) {
                if piece.object != *object
                    && !gone.contains(&piece.object)
                    && let Some(slot) = fields.get_mut(&piece.object).and_then(|c| c.segment_mut(piece.seg))
                {
                    slot.clear();
                }
            }
        }
        // Paired paragraphs: properties, then their new runs onto their pieces.
        // The anchors each field gets: (segment, offset in it, anchor).
        let mut inserts: HashMap<ObjectId, Vec<(usize, usize, Anchor)>> = HashMap::new();
        for (j, p) in after.paragraphs.iter().enumerate() {
            let Some(i) = partner.get(j).copied().flatten() else { continue };
            let (Some(object), Some(q)) = (known.paragraphs.get(i), old.paragraphs.get(i)) else { continue };
            let parts = pieces_of(self.ids, object);
            let lost = parts.iter().any(|piece| gone.contains(&piece.object));
            if q == p && !lost {
                continue;
            }
            let (old_props, new_props, new_runs, next_runs) = {
                let ids = &*self.ids;
                let fix = |v: &mut Value| to_objects(v, ids, None);
                let next = match (after.paragraphs.get(j + 1), partner.get(j + 1)) {
                    (Some(next), Some(None)) => Some(canon(to_runs(next, &fix)?)),
                    _ => None,
                };
                (paragraph_props(q, &fix)?, paragraph_props(p, &fix)?, canon(to_runs(p, &fix)?), next)
            };
            if old_props != new_props {
                self.tx.set(object, "props", new_props)?;
                wrote = true;
            }
            // A merge's text is both paragraphs' fields as they are, joined by its anchor.
            if merges.iter().any(|m| m.after == j) {
                continue;
            }
            let olds: Vec<&[TextRun]> = parts.iter().map(|piece| old_fields.get(&piece.object).map_or(&[][..], |c| c.segment(piece.seg))).collect();
            let old_runs = canon(olds.iter().flat_map(|runs| runs.iter().cloned()).collect());
            if old_runs == new_runs && !lost {
                continue;
            }
            let lens: Vec<usize> = olds.iter().map(|runs| char_len(runs)).collect();
            let losing: Vec<bool> = parts.iter().map(|piece| gone.contains(&piece.object)).collect();
            let old_text = text_of(&old_runs);
            for (piece, runs) in parts.iter().zip(assign(&old_text, &lens, &losing, &new_runs)) {
                if gone.contains(&piece.object) {
                    continue;
                }
                if let Some(slot) = fields.get_mut(&piece.object).and_then(|c| c.segment_mut(piece.seg)) {
                    *slot = runs;
                }
            }
            // Enter inside this paragraph, its object kept for the first half: a split anchor at
            // the split point, and at the start of every other field's text after it, so text
            // typed concurrently after it follows the second half (`pieces`).
            if let (Some(next), Some(target)) = (next_runs, objects.get(j + 1)) {
                let (first, second) = (text_of(&new_runs), text_of(&next));
                let at = first.chars().count();
                if at > 0 && !second.is_empty() && old_text == format!("{first}{second}") {
                    let mut start = 0;
                    let mut seen = HashSet::new();
                    for (piece, len) in parts.iter().zip(&lens) {
                        let end = start + len;
                        if (end > at || start >= at)
                            && !gone.contains(&piece.object)
                            && let Some(c) = old_fields.get(&piece.object)
                            && seen.insert((piece.object.clone(), c.region(piece.seg)))
                        {
                            let offset = at.saturating_sub(start).min(*len);
                            inserts.entry(piece.object.clone()).or_default().push((piece.seg, offset, Anchor::Split(target.clone())));
                        }
                        start = end;
                    }
                }
            }
        }
        for object in &gone {
            if self.tx.exists(object) {
                self.tx.delete(object)?;
                wrote = true;
            }
        }
        // A merge anchor where the first paragraph's view ends.
        for m in &merges {
            let len = old_fields.get(&m.at.object).map_or(0, |c| char_len(c.segment(m.at.seg)));
            inserts.entry(m.at.object.clone()).or_default().push((m.at.seg, len, Anchor::Merge(m.second.clone())));
        }
        // Last first, so earlier segments keep their index.
        for list in inserts.values_mut() {
            list.sort_by_key(|a| std::cmp::Reverse((a.0, a.1)));
        }
        let seconds: Vec<ObjectId> = merges.iter().map(|m| m.second.clone()).collect();
        // Each kept paragraph's field: written where it differs from what it held.
        let kept_fields: Vec<ObjectId> = known.paragraphs.iter().chain(&absorbed).filter(|o| !gone.contains(*o)).cloned().collect();
        for object in &kept_fields {
            let Some(mut c) = fields.remove(object) else { continue };
            let mut old_truth = truths.remove(object).unwrap_or_default();
            if let Some(list) = inserts.get(object) {
                // Each anchor on its own, between the characters it goes between.
                let mut with = compose(&old_truth);
                for (seg, offset, anchor) in list {
                    insert_anchor_char(self.tx, object, with.start_of(*seg) + offset, anchor, &old_truth)?;
                    with.insert_anchor_at(*seg, *offset, anchor.clone());
                    c.insert_anchor(*seg, anchor.clone());
                }
                old_truth = with.runs();
                wrote = true;
            }
            // Anchors naming deleted paragraphs go (a split anchor once its tail is empty).
            for k in (0..c.anchors.len()).rev() {
                let target = c.anchors[k].target();
                let dead = gone.contains(target) || !self.tx.exists(target);
                let drop = match &c.anchors[k] {
                    Anchor::Split(_) => dead && c.segment(k + 1).is_empty(),
                    Anchor::Merge(_) => dead,
                };
                if drop {
                    c.remove_anchor(k);
                }
            }
            let new_truth = c.runs();
            if new_truth != old_truth {
                write_rich_text(self.tx, object, "text", &old_truth, &new_truth).map_err(kdm_err)?;
                wrote = true;
            }
            fields.insert(object.clone(), compose(&new_truth));
            self.ids.texts.insert(object.clone(), Arc::new(new_truth));
        }
        for object in &gone {
            self.ids.texts.remove(object);
            self.ids.pieces.remove(object);
        }
        // The body lists every view paragraph, each followed by the paragraphs absorbed into it.
        let mut listed = objects.clone();
        listed.extend(absorbed.iter().chain(&seconds).filter(|o| !gone.contains(*o) && !objects.contains(*o)).cloned());
        let layout = pieces::layout(&listed, &fields);
        debug_assert_eq!(layout.roots, objects, "the view's paragraphs are the body's");
        let mut order = Vec::with_capacity(listed.len());
        for root in &objects {
            order.push(root.clone());
            order.extend(layout.absorbed.iter().filter(|(_, host)| host == root).map(|(object, _)| object.clone()));
        }
        for object in &listed {
            self.ids.pieces.remove(object);
        }
        self.ids.pieces.extend(layout.pieces);
        wrote |= sync_list(self.tx, &body, "paragraphs", &order)? > 0;
        self.ids.bodies.insert(owner, BodyIds { body: body.clone(), paragraphs: objects, absorbed: layout.absorbed });
        Ok((wrote, body))
    }

    /// The paragraph merges `after` makes of `old` that a merge anchor can keep (`pieces`): a view
    /// paragraph whose runs are two adjacent old paragraphs' runs joined, both with text, the
    /// second not paired with any. The view paragraph is paired with the first (`partner`).
    fn find_merges(&self, old: &TextBody, after: &TextBody, known: &BodyIds, partner: &mut [Option<usize>]) -> KdmResult<Vec<Merge>> {
        let ids = &*self.ids;
        let fix = |v: &mut Value| to_objects(v, ids, None);
        let mut paired: HashSet<usize> = partner.iter().flatten().copied().collect();
        let mut used: HashSet<usize> = HashSet::new();
        let mut out = Vec::new();
        for (j, slot) in partner.iter_mut().enumerate() {
            let (Some(i), Some(p)) = (*slot, after.paragraphs.get(j)) else { continue };
            let candidates = [Some((i, i + 1)), i.checked_sub(1).map(|first| (first, i))];
            for (first, second) in candidates.into_iter().flatten() {
                let other = if first == i { second } else { first };
                let (Some(q1), Some(q2)) = (old.paragraphs.get(first), old.paragraphs.get(second)) else { continue };
                let (Some(x), Some(y)) = (known.paragraphs.get(first), known.paragraphs.get(second)) else { continue };
                if paired.contains(&other) || used.contains(&first) || used.contains(&second) || q1.text().is_empty() || q2.text().is_empty() {
                    continue;
                }
                let mut joined = to_runs(q1, &fix)?;
                joined.extend(to_runs(q2, &fix)?);
                if canon(joined) != canon(to_runs(p, &fix)?) {
                    continue;
                }
                let at = ids.pieces.get(x).and_then(|parts| parts.last().cloned()).unwrap_or_else(|| Piece::own(x));
                *slot = Some(first);
                paired.insert(first);
                used.extend([first, second]);
                out.push(Merge { after: j, first, second: y.clone(), at });
                break;
            }
        }
        Ok(out)
    }

    /// A slide's animations: aligned by content (then by the shape they animate), so an edit,
    /// insert, delete or reorder keeps every other animation's object.
    fn write_animations(&mut self, slide: &ObjectId, before: Option<&[Animation]>, after: &[Animation]) -> KdmResult<bool> {
        let mut existing = self.ids.animations.get(slide).cloned().unwrap_or_default();
        let before = before.unwrap_or(&[]);
        if existing.len() != before.len() {
            // The view does not match the objects it was read with: rewrite.
            for object in &existing {
                if self.tx.exists(object) {
                    self.tx.delete(object)?;
                }
            }
            existing.clear();
            let mut objects = Vec::new();
            for a in after {
                objects.push(self.tx.create("Animation", fields(vec![("props", animation_props(a, slide, self.ids)?)]))?);
            }
            sync_list(self.tx, slide, "animations", &objects)?;
            let wrote = !objects.is_empty() || !before.is_empty();
            self.ids.animations.insert(slide.clone(), objects);
            return Ok(wrote);
        }
        let before_values: Vec<(Value, u32)> =
            before.iter().map(|a| Ok((animation_props(a, slide, self.ids)?, a.shape.0))).collect::<KdmResult<_>>()?;
        let after_values: Vec<(Value, u32)> =
            after.iter().map(|a| Ok((animation_props(a, slide, self.ids)?, a.shape.0))).collect::<KdmResult<_>>()?;
        if before_values == after_values {
            return Ok(false);
        }
        let partner = align(&before_values, &after_values);
        let kept: HashSet<usize> = partner.iter().flatten().copied().collect();
        let mut wrote = false;
        for (i, object) in existing.iter().enumerate() {
            if !kept.contains(&i) && self.tx.exists(object) {
                self.tx.delete(object)?;
                wrote = true;
            }
        }
        let mut objects = Vec::with_capacity(after.len());
        for (j, (v, _)) in after_values.into_iter().enumerate() {
            match partner.get(j).copied().flatten() {
                Some(i) => {
                    let object = existing.get(i).cloned().ok_or_else(|| kdm_err("an animation has no object"))?;
                    if before_values.get(i).map(|(old, _)| old) != Some(&v) {
                        self.tx.set(&object, "props", v)?;
                        wrote = true;
                    }
                    objects.push(object);
                }
                None => {
                    objects.push(self.tx.create("Animation", fields(vec![("props", v)]))?);
                    wrote = true;
                }
            }
        }
        wrote |= sync_list(self.tx, slide, "animations", &objects)? > 0;
        self.ids.animations.insert(slide.clone(), objects);
        Ok(wrote)
    }
}

/// A section or custom show as [`Writer::write_groups`] pairs and writes it (a custom show has no
/// identity).
#[derive(Clone, PartialEq)]
struct Group<'a> {
    identity: Option<&'a str>,
    name: &'a str,
    slides: &'a [kition_slide_model::SlideId],
}

/// A paragraph merge kept with a merge anchor (`crate::pieces`): view paragraph `after` is old
/// paragraphs `first` and `first + 1` joined; `second` is the latter's object, absorbed at the end
/// of piece `at` (the last of the first's view).
struct Merge {
    after: usize,
    first: usize,
    second: ObjectId,
    at: Piece,
}

/// The paragraphs a body write deletes: the old view paragraphs neither paired nor absorbed by a
/// merge, and the paragraphs absorbed into those.
fn gone_paragraphs(known: &BodyIds, partner: &[Option<usize>], merges: &[Merge]) -> HashSet<ObjectId> {
    let kept: HashSet<usize> = partner.iter().flatten().copied().collect();
    let seconds: HashSet<&ObjectId> = merges.iter().map(|m| &m.second).collect();
    let mut gone: HashSet<ObjectId> =
        known.paragraphs.iter().enumerate().filter(|(i, o)| !kept.contains(i) && !seconds.contains(o)).map(|(_, o)| o.clone()).collect();
    for (object, host) in &known.absorbed {
        if gone.contains(host) {
            gone.insert(object.clone());
        }
    }
    gone
}

/// Whether `merges`' anchors in the fields as they are show each merged view paragraph as `after`
/// holds it.
fn merges_show(
    merges: &[Merge],
    known: &BodyIds,
    gone: &HashSet<ObjectId>,
    old_fields: &HashMap<ObjectId, Composition>,
    ids: &Ids,
    after: &TextBody,
) -> KdmResult<bool> {
    let mut sim = old_fields.clone();
    let mut inserts: Vec<&Merge> = merges.iter().collect();
    inserts.sort_by_key(|a| std::cmp::Reverse(a.at.seg));
    for m in inserts {
        if gone.contains(&m.at.object) {
            return Ok(false);
        }
        let Some(c) = sim.get_mut(&m.at.object) else { return Ok(false) };
        c.insert_anchor(m.at.seg, Anchor::Merge(m.second.clone()));
    }
    let listed: Vec<ObjectId> =
        known.paragraphs.iter().chain(known.absorbed.iter().map(|(o, _)| o)).filter(|o| !gone.contains(*o)).cloned().collect();
    let layout = pieces::layout(&listed, &sim);
    let fix = |v: &mut Value| to_objects(v, ids, None);
    for m in merges {
        let (Some(x), Some(p)) = (known.paragraphs.get(m.first), after.paragraphs.get(m.after)) else { return Ok(false) };
        if layout.roots.contains(&m.second) {
            return Ok(false);
        }
        let shown = match layout.pieces.get(x) {
            Some(parts) => pieces::shown(parts, &sim),
            None => sim.get(x).map(|c| canon(c.segment(0).to_vec())).unwrap_or_default(),
        };
        if shown != canon(to_runs(p, &fix)?) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Inserts `anchor`'s character at `at` of `object`'s `text` field (whose runs are `runs`), with
/// its `inline` mark and none of the marks it may have taken from its neighbours. Written on its
/// own, not diffed, so it lands exactly between the characters it joins.
fn insert_anchor_char(tx: &mut Tx<'_>, object: &ObjectId, at: usize, anchor: &Anchor, runs: &[TextRun]) -> KdmResult<()> {
    tx.text_insert(object, "text", at, &INLINE.to_string())?;
    let run = anchor.run();
    if let Some(value) = run.attributes.get(INLINE_MARK) {
        tx.text_mark(object, "text", at, at + 1, INLINE_MARK, value.clone())?;
    }
    let keys: BTreeSet<&str> = runs.iter().flat_map(|r| r.attributes.keys().map(String::as_str)).filter(|k| *k != INLINE_MARK).collect();
    for key in keys {
        tx.text_unmark(object, "text", at, at + 1, key)?;
    }
    Ok(())
}
