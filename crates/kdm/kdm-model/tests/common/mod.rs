//! Shared test helpers: a small slide to edit, a blob store shared between replicas, and a
//! multi-replica harness over `KitSession<SlideModel>`. Each `tests/*.rs` binary uses only some.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kdm_model::{Ids, SlideModel, write_changes, write_document};
use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, Replica};
use kition_kit::session::{BlobRef, BlobSink, BlobSource, KitError, KitModel, KitSession, sha256_hex};
use kition_slide_model::style::PictureFill;
use kition_slide_model::text::TextBody;
use kition_slide_model::{Animation, Shape, ShapeId, ShapeKind, SlideFile, Table, Xfrm, defaults};

pub type Session = KitSession<SlideModel>;

/// One blob store every replica of a test shares (blobs live outside the replica).
#[derive(Clone, Default)]
pub struct SharedBlobs(pub Arc<Mutex<HashMap<String, Arc<[u8]>>>>);

impl BlobSource for SharedBlobs {
    fn get(&self, hash: &str) -> Option<Arc<[u8]>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(hash).cloned()
    }
}

impl BlobSink for SharedBlobs {
    fn put(&mut self, bytes: &[u8], mime: &str) -> BlobRef {
        let hash = sha256_hex(bytes);
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).entry(hash.clone()).or_insert_with(|| Arc::from(bytes));
        BlobRef { hash, mime: mime.to_owned(), size: bytes.len() as u64 }
    }
}

pub fn actor(value: u64) -> ActorId {
    ActorId::from_u64(value).unwrap_or_else(|_| panic!("actor {value}"))
}

pub fn person(name: &str) -> Origin {
    Origin::person(format!("user:{name}"))
}

pub fn shape(p: &mut SlideFile, name: &str, x: f64) -> Shape {
    Shape {
        id: ShapeId(p.alloc_id()),
        name: name.into(),
        xfrm: Some(Xfrm::new(x, 50.0, 100.0, 60.0)),
        text: Some(TextBody::from_text(name)),
        ..Default::default()
    }
}

/// A slide file with three slides: slide 0 has two shapes, a connector between them and an animation;
/// slide 1 has a 3x3 table; slide 2 has a picture of embedded media.
pub fn slide_file() -> SlideFile {
    let mut p = defaults::new_slide_file(None);
    let layout = p.masters[0].layouts[0].id;
    for _ in 0..3 {
        let slide = defaults::new_slide(&mut p, layout);
        p.slides.push(Arc::new(slide));
    }
    let a = shape(&mut p, "A", 50.0);
    let b = shape(&mut p, "B", 300.0);
    let mut connector = shape(&mut p, "Connector", 0.0);
    connector.text = None;
    connector.kind = ShapeKind::Connector { start: Some((a.id, 1)), end: Some((b.id, 3)) };
    let animation = Animation { shape: a.id, trigger: Some(b.id), ..Default::default() };
    {
        let s0 = Arc::make_mut(&mut p.slides[0]);
        s0.shapes.extend([a, b, connector]);
        s0.animations.push(animation);
    }
    let mut table = shape(&mut p, "Table", 100.0);
    table.text = None;
    let mut grid = Table::new(3, 3, 300.0, 30.0);
    for (r, row) in grid.rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            cell.text = TextBody::from_text(&format!("r{r}c{c}"));
        }
    }
    table.kind = ShapeKind::Table(grid);
    Arc::make_mut(&mut p.slides[1]).shapes.push(table);
    let media = p.add_media("image1.png", "image/png", vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4]);
    let mut picture = shape(&mut p, "Picture", 10.0);
    picture.text = None;
    picture.kind = ShapeKind::Picture { fill: PictureFill { media, ..Default::default() } };
    Arc::make_mut(&mut p.slides[2]).shapes.push(picture);
    p
}

/// A fresh replica of `p` (its creating transaction, not an undo step) and the ids it minted.
pub fn new_replica(p: &SlideFile, blobs: &SharedBlobs) -> (Replica, Ids) {
    let mut ids = Ids::default();
    let mut sink = blobs.clone();
    let (replica, ()) = Replica::create_with(SlideModel::schema(), DocId::new(), actor(1), &Origin::system("create"), |tx| {
        write_document(tx, p, &mut ids, &mut sink).map(|_| ())
    })
    .unwrap_or_else(|error| panic!("create: {error}"));
    (replica, ids)
}

pub fn first(p: &SlideFile, blobs: &SharedBlobs) -> Session {
    let (replica, _) = new_replica(p, blobs);
    KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap_or_else(|error| panic!("attach: {error}"))
}

pub fn peer(of: &Session, value: u64, blobs: &SharedBlobs) -> Session {
    let snapshot = of.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    KitSession::open_snapshot(&snapshot, Box::new(blobs.clone()), actor(value)).unwrap_or_else(|error| panic!("open_snapshot: {error}"))
}

/// Edits `session`'s view with `f` and records what changed, the way the engine bridge will.
pub fn edit(session: &mut Session, blobs: &SharedBlobs, f: impl FnOnce(&mut SlideFile)) {
    let before = session.model().slide_file.clone();
    let mut after = before.clone();
    f(&mut after);
    let mut ids = session.model().ids().clone();
    let mut sink = blobs.clone();
    session
        .record(&person("test"), |tx| write_changes(tx, &before, &after, &mut ids, &mut sink).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

/// Sends `from`'s changes that `to` lacks.
pub fn send(from: &Session, to: &mut Session) {
    let bytes = from.export_changes(&to.version()).unwrap_or_else(|error| panic!("export: {error}"));
    to.import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
}

/// Exchanges until every session holds the same version (repairs are changes too).
pub fn sync_all(sessions: &mut [Session]) {
    for _ in 0..8 {
        for i in 0..sessions.len() {
            for j in 0..sessions.len() {
                if i != j {
                    let bytes = sessions[i].export_changes(&sessions[j].version()).unwrap_or_else(|error| panic!("export: {error}"));
                    sessions[j].import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
                }
            }
        }
        if sessions.windows(2).all(|w| w[0].version() == w[1].version()) {
            return;
        }
    }
    panic!("the replicas did not converge");
}

pub fn sync(a: &mut Session, b: &mut Session) {
    send(a, b);
    send(b, a);
    send(a, b);
    assert_eq!(a.version(), b.version(), "the replicas converged");
}

/// The view as its id-blind JSON, for comparing views of different replicas.
pub fn canonical(p: &SlideFile) -> serde_json::Value {
    serde_json::to_value(kdm_model::normalize(p)).unwrap_or_else(|error| panic!("serialize: {error}"))
}

/// The slide file's table (slide 1).
pub fn table_mut(p: &mut SlideFile) -> Option<&mut Table> {
    Arc::make_mut(&mut p.slides[1]).shapes.iter_mut().find_map(|s| if let ShapeKind::Table(t) = &mut s.kind { Some(t) } else { None })
}

pub fn shape_named<'a>(p: &'a SlideFile, slide: usize, name: &str) -> &'a Shape {
    p.slides[slide].shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no shape {name} on slide {slide}"))
}

pub fn shape_object(session: &Session, slide: usize, name: &str) -> ObjectId {
    let p = &session.model().slide_file;
    let slide_object = session.model().ids().slides.object(p.slides[slide].id).unwrap_or_else(|| panic!("slide object")).clone();
    let id = shape_named(p, slide, name).id;
    session.model().ids().shape(&slide_object, id).unwrap_or_else(|| panic!("shape object")).clone()
}

/// The replica itself holds no reference to a deleted object (what the repair after a merge
/// clears): list entries, slide layouts, animation shapes and triggers, connector ends, sections.
pub fn assert_replica_clean(replica: &Replica) {
    let gone =
        |v: Option<&serde_json::Value>| matches!(v, Some(serde_json::Value::String(id)) if !replica.exists(&ObjectId::from_string(id.as_str())));
    let lists: &[(&str, &[&str])] = &[
        ("SlideFile", &["masters", "slides", "media"]),
        ("Master", &["layouts", "shapes"]),
        ("Layout", &["shapes"]),
        ("Slide", &["shapes", "animations"]),
        ("Shape", &["children"]),
        ("Table", &["rows", "cols"]),
        ("TextBody", &["paragraphs"]),
    ];
    for (ty, fields) in lists {
        for object in replica.objects_of_type(ty) {
            for field in *fields {
                for item in replica.list(&object, field).unwrap_or_default() {
                    assert!(replica.exists(&item), "{ty} {object}.{field} lists deleted {item}");
                }
            }
        }
    }
    for slide in replica.objects_of_type("Slide") {
        assert!(!gone(Some(&replica.get(&slide, "layout").unwrap_or_default())), "slide {slide} names a deleted layout");
        assert!(!gone(Some(&replica.get(&slide, "notesBody").unwrap_or_default())), "slide {slide} names deleted notes");
    }
    for animation in replica.objects_of_type("Animation") {
        let props = replica.get(&animation, "props").unwrap_or_default();
        assert!(!gone(props.get("shape")), "animation {animation} names a deleted shape");
        assert!(!gone(props.get("trigger")), "animation {animation}'s trigger names a deleted shape");
    }
    for shape in replica.objects_of_type("Shape") {
        assert!(!gone(Some(&replica.get(&shape, "body").unwrap_or_default())), "shape {shape} names a deleted text body");
        let props = replica.get(&shape, "props").unwrap_or_default();
        for side in ["start", "end"] {
            let end = props.get("kind").and_then(|k| k.get(side)).and_then(|e| e.as_array()).and_then(|pair| pair.first());
            assert!(!gone(end), "connector {shape}'s {side} names a deleted shape");
        }
    }
    let root = ObjectId::from_string("slide-file");
    for field in ["sections", "customShows"] {
        for entry in replica.get(&root, field).unwrap_or_default().as_array().into_iter().flatten() {
            for slide in entry.get("slides").and_then(|s| s.as_array()).into_iter().flatten() {
                assert!(!gone(Some(slide)), "{field} names a deleted slide");
            }
        }
    }
}

/// Every reference in `p` resolves: slide layouts, connector ends, animation shapes and
/// triggers, comment shapes, sections and custom shows, picture and media-clip media.
pub fn assert_no_dangling(p: &SlideFile) {
    let media: std::collections::HashSet<_> = p.media.iter().map(|m| m.id).collect();
    let slides: std::collections::HashSet<_> = p.slides.iter().map(|s| s.id).collect();
    for slide in &p.slides {
        assert!(p.layout(slide.layout).is_some(), "slide {} has layout {}, which does not exist", slide.id, slide.layout);
        let mut ids = std::collections::HashSet::new();
        kition_slide_model::walk(&slide.shapes, &mut |s, _| {
            assert!(ids.insert(s.id), "duplicate shape id {}", s.id);
        });
        kition_slide_model::walk(&slide.shapes, &mut |s, _| match &s.kind {
            ShapeKind::Connector { start, end } => {
                for (id, _) in start.iter().chain(end.iter()) {
                    assert!(ids.contains(id), "connector {} names shape {id}, which is gone", s.id);
                }
            }
            ShapeKind::Picture { fill } => assert!(media.contains(&fill.media), "picture {} names media {}, which is gone", s.id, fill.media),
            _ => {}
        });
        for animation in &slide.animations {
            assert!(ids.contains(&animation.shape), "an animation names shape {}, which is gone", animation.shape);
            if let Some(trigger) = animation.trigger {
                assert!(ids.contains(&trigger), "an animation's trigger names shape {trigger}, which is gone");
            }
        }
        for comment in &slide.comments {
            if let Some(shape) = comment.shape {
                assert!(ids.contains(&shape), "a comment names shape {shape}, which is gone");
            }
        }
    }
    for section in &p.sections {
        for id in &section.slides {
            assert!(slides.contains(id), "section {} names slide {id}, which is gone", section.name);
        }
    }
    for show in &p.custom_shows {
        for id in &show.slides {
            assert!(slides.contains(id), "custom show {} names slide {id}, which is gone", show.name);
        }
    }
}
