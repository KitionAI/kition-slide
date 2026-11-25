//! Bridges Kition Slide's [`SlideFile`] ([`kition_slide_model`]) to a `kit.slide` KDM replica.
//!
//! [`SlideModel`] implements [`KitModel`]: the engine's materialised view of a replica, the same
//! [`SlideFile`] the rest of the engine already knows. Masters, layouts, slides, shapes (with
//! nested group children), tables (with their rows and columns), animations and media become KDM
//! objects with their own identity, so an edit, an undo and a merged draft (data model 6.6) touch
//! only the objects they change;
//! [`write_document`] and [`write_changes`] are the engine command bridge's entry points: writing
//! a whole slide in one creating transaction, or only what a command changed.
//!
//! What lives where (`model/schema.json`):
//! - a shape's text body is a `TextBody` object (its `body` ref) listing `Paragraph` objects, each
//!   a `props` register and a `text` field whose marks are the run properties (K4, `src/text.rs`);
//!   table cells' and slide notes' text bodies are the same (a cell's map value names its body, a
//!   slide's `notesBody`). A paragraph split leaves a split anchor in the first half's field, and a
//!   merge keeps the second paragraph listed behind a merge anchor, so both keep their paragraph
//!   objects (`src/pieces.rs`); each paragraph's field is written from what it held when last read
//!   or written, so every replica reads the same runs. A shape's `xfrm` and `name` are their own registers;
//!   every other member of the shape and its kind is an entry of its `look` map
//!   (`src/look.rs`), except group children
//!   (`children` list), a table's grid (a `Table` object: `cols`/`rows` lists, and a `cells` map
//!   keyed `"<rowObjectId>:<colObjectId>"`, so a row or column insert adds one object and never
//!   shifts other cells), a chart (a `Chart` object: one `parts` map entry per member,
//!   `src/charts.rs`) and media bytes (a `Media` object's `blob`, content-addressed through
//!   [`BlobSink`]/[`BlobSource`]);
//! - sections and custom shows are `Section` and `CustomShow` objects listed by the slide file,
//!   each a `name` and a `slides` map with one entry per member slide (`src/members.rs`), so a
//!   draft's membership change merges per slide;
//! - comment threads are `CommentThread` objects beside the slides (`src/comments.rs`);
//! - everything not modelled as its own field is kept in a `json` register next to it (a slide's
//!   `props`/`background`/`transition`/`rawExt`, a master's `theme`/`listStyles`/`colorMap`, the
//!   slide file's `show`/`notesMaster`…), so nothing is lost on a round trip. The slide file's
//!   view properties (`SlideFile::view_props`: last view, zoom, guides) are private to the view
//!   that opened the file: never replicated, kept by [`SlideModel`] across reads.
//!
//! **Raw batches (`batchOps`).** Every type declares the raw operations an agent's batch may
//! run on it, guarded by the ordinary object, field, reference and insertion-window holds
//! (`direct`). Table rows and columns retain per-table immutable member identities across saves;
//! paragraphs are paired by content on reread. Positional creates and list operations are direct
//! contracts, and nested list gaps use the SDK's private ordinal history. Field writes on a
//! paragraph, row or column named by id are independent of unrelated ancestor positions.
//!
//! **Ids (S3).** Engine ids (`SlideId`, `ShapeId`, `LayoutId`, `MasterId`, `MediaId`) are never
//! replicated: [`read_document`] mints them (keeping, through [`Known`], the ones a session's view
//! already gave an object), and [`Ids`] maps them to object ids. Every id stored as *data* — a
//! slide's layout, connector ends, an animation's shape and trigger, a comment's shape, a section's
//! or custom show's member slides (map keys, `src/members.rs`), embedded fonts, a hyperlink to a slide, every media reference (pictures,
//! media clips, picture bullets and fills, sounds, previews) — is written as the object id it names
//! and translated back on read (`refs`). One that names nothing (a file's dangling reference) is
//! kept as the number it was.
//!
//! **Read repair.** What only a read can see (a transaction cannot read text fields): a paragraph
//! or a comment listed twice, is written back by
//! [`SlideSession`] in a `system` transaction after each import, undo or redo and when it opens a
//! replica (`src/siblings.rs`).
//!
//! **Repair (S4).** [`KitModel::validate`] clears what a merged draft, an undo or a redo left
//! dangling (a draft is checked against the document's changes since it was made, not against
//! what they deleted): animations of deleted
//! shapes, triggers and connector ends at deleted shapes, section and custom show members that are
//! deleted slides (and a slide two sections hold, from all but the first), embedded fonts of deleted media, dangling list entries, and a slide
//! whose layout is gone gets its master's first layout. An object listed twice in one list (a
//! replica stored that way) is read once, at its first place, and the repair removes its later
//! entries.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod blob_gc;
mod charts;
mod comments;
mod compare;
mod ids;
mod list;
mod look;
mod members;
mod patch;
mod pieces;
mod read;
mod refs;
mod session;
mod siblings;
mod stored;
mod text;
mod validate;
mod write;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};
use kition_slide_model::SlideFile;

pub use compare::normalize;
pub use ids::{BodyIds, BodyOwner, Ids, Known, TableIds};
pub use read::{read_document, read_document_known};
pub use refs::renumber_media;
pub use session::{SlideSession, merge_scope};
pub use stored::{settle_batch, settle_premise_batch, stored_objects};
pub use write::{write_changes, write_comment_changes, write_document};

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

const SCHEMA: &str = include_str!("../model/schema.json");

/// The `SlideFile` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub(crate) fn root_id() -> ObjectId {
    ObjectId::from_string("slide-file")
}

fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

/// Media bytes the view already holds, by blob hash, so a reload does not copy them again.
pub(crate) type MediaCache = HashMap<String, Arc<Vec<u8>>>;

/// A file's slide, as [`KitModel::import`] reads it: `.kslide` or PowerPoint `.pptx`
/// (by content, else by `name`'s extension), with duplicate ids renumbered (a replica needs every
/// slide, layout, master and media id unique in the slide file, and every shape id unique on its slide,
/// layout or master).
pub fn decode(name: &str, bytes: &[u8]) -> Result<SlideFile> {
    let lower = name.to_ascii_lowercase();
    let mut p = if kition_slide_format::sniff(bytes) {
        kition_slide_format::load(bytes).map_err(kit_err)?
    } else if kition_slide_pptx::sniff(bytes) || lower.ends_with(".pptx") {
        kition_slide_pptx::import(bytes).map_err(kit_err)?
    } else {
        kition_slide_format::load(bytes).map_err(kit_err)?
    };
    ids::dedupe(&mut p);
    Ok(p)
}

/// The engine's view of a `kit.slide` replica: the same [`SlideFile`] the rest of the engine
/// knows, plus the id maps [`write_changes`] needs to find each object again.
pub struct SlideModel {
    pub slide_file: SlideFile,
    ids: Ids,
    known: Known,
    media: MediaCache,
    /// Media blobs the view read as empty because the store lacked them (see `read::read_with`).
    missing: BTreeSet<String>,
}

impl SlideModel {
    /// A view built by the caller itself: `slide_file` as written by [`write_document`] or
    /// [`write_changes`], with the ids they filled.
    pub fn from_parts(slide_file: SlideFile, ids: Ids) -> Self {
        let mut known = Known::default();
        known.remember(&slide_file, &ids);
        Self { slide_file, ids, known, media: MediaCache::new(), missing: BTreeSet::new() }
    }

    /// Replaces the view after the caller wrote `slide_file` to the replica itself (a diff
    /// bridge's [`write_changes`]), with the ids that write left.
    pub fn adopt(&mut self, slide_file: SlideFile, ids: Ids) {
        self.known.remember(&slide_file, &ids);
        self.slide_file = slide_file;
        self.ids = ids;
    }

    /// The id maps, current as of the last `load`/`apply`/`adopt`. A diff bridge clones them into
    /// its own working copy for a [`write_changes`] transaction, then hands them back through
    /// [`SlideModel::adopt`].
    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    fn reload(&mut self, replica: &Replica, blobs: &dyn BlobSource) -> Result<()> {
        let (mut slide_file, ids, media, missing) = read::read_with(replica, blobs, &self.known, &self.media).map_err(kit_err)?;
        // The view properties are this view's own, never replicated.
        slide_file.view_props = self.slide_file.view_props.take();
        self.known.remember(&slide_file, &ids);
        self.slide_file = slide_file;
        self.ids = ids;
        self.media = media;
        self.missing = missing;
        Ok(())
    }

    /// Whether `blobs` now holds a media blob the view read as empty: the view needs a reload.
    pub fn awaits_any_of(&self, blobs: &dyn BlobSource) -> bool {
        self.missing.iter().any(|hash| blobs.get(hash).is_some())
    }
}

impl std::fmt::Debug for SlideModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlideModel").field("slides", &self.slide_file.slides.len()).finish_non_exhaustive()
    }
}

impl KitModel for SlideModel {
    fn schema() -> Arc<Schema> {
        #[allow(clippy::expect_used)]
        Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
    }

    fn load(replica: &Replica, blobs: &dyn BlobSource) -> Result<Self> {
        let (slide_file, ids, media, missing) = read::read_with(replica, blobs, &Known::default(), &MediaCache::new()).map_err(kit_err)?;
        let mut model = Self::from_parts(slide_file, ids);
        model.media = media;
        model.missing = missing;
        Ok(model)
    }

    /// Patches the view: reads again only the slides `changed` touched when it can
    /// (`src/patch.rs`), else the whole view (keeping every surviving object's local id, and media
    /// bytes it already holds). An empty `changed` reloads the whole view.
    fn apply(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> Result<Damage> {
        if !patch::patch(self, replica, changed, blobs) {
            self.reload(replica, blobs)?;
        }
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        validate::validate(tx, changed).map_err(kit_err)
    }

    fn import(name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let slide_file = decode(name, bytes)?;
        let mut ids = Ids::default();
        let (replica, ()) = Replica::create_with(Self::schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &slide_file, &mut ids, blobs)?;
            Ok(())
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    /// `.pptx` when `name` says so, otherwise `.kslide`.
    fn export(&self, _replica: &Replica, name: &str, _blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        if name.to_ascii_lowercase().ends_with(".pptx") {
            let mut exported = self.slide_file.clone();
            exported.keep_own_slide_ids();
            kition_slide_pptx::export(&exported).map_err(kit_err)
        } else {
            kition_slide_format::save(&self.slide_file).map_err(kit_err)
        }
    }
}
