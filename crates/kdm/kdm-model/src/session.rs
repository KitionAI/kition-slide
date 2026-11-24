//! [`SlideSession`]: one slide's replica, its [`SlideModel`] view and its blobs, for the
//! engine's command bridge (Kition phase 3d, Task 2).
//!
//! It is `kition_kit::session::KitSession<SlideModel>`'s flow (transactions, per-actor undo, the
//! change exchange, the post-merge repair through [`KitModel::validate`], the view patched through
//! [`KitModel::apply`]) on concrete parts, as Kition Photo's `PhotoSession` does: the engine
//! must enumerate, ship and garbage-collect the media blobs, which `KitSession`'s boxed blob store
//! does not allow. Edits are diff-bridged: the engine changes its own [`SlideFile`], and
//! [`SlideSession::record`] writes what changed as one transaction ([`write_changes`]), then
//! adopts it as the view. Everything here is `Send`.

use std::collections::BTreeSet;
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitModel, MemoryBlobs};
use kition_slide_model::SlideFile;

use crate::blob_gc::{BlobIndex, Moved};
use crate::validate::LocalRepair;
use crate::{Ids, Result, SlideModel, kit_err, normalize, read_document, siblings, write_changes, write_comment_changes, write_document};

/// One open slide as a KDM replica and the engine's view of it. See the module docs.
pub struct SlideSession {
    replica: Replica,
    model: SlideModel,
    blobs: MemoryBlobs,
    /// Which blobs are still needed ([`crate::blob_gc`]); the rest are dropped after each change.
    index: BlobIndex,
    /// The hashes each record, import, undo and redo started referencing, with the version right
    /// after it, for [`SlideSession::blobs_since`] (Kition Photo's blob log: chronological, so
    /// `since` cleanly splits it into "already has" and "needs"; trimmed with the store).
    blob_log: Vec<(Version, Vec<String>)>,
    /// The person may only comment: a record writes only comment threads (M23) (see
    /// [`SlideSession::set_comment_only`]).
    comment_only: bool,
    /// What local writes left for the next repair after a deletion ([`LocalRepair`]).
    local: LocalRepair,
}

impl std::fmt::Debug for SlideSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlideSession").field("doc", &self.replica.doc_id()).field("actor", &self.replica.actor()).finish_non_exhaustive()
    }
}

impl SlideSession {
    /// A new replica holding `slide_file`, written in its creating transaction (not an undo
    /// step). The view is `slide_file` itself, with its own local ids.
    pub fn create(slide_file: &SlideFile) -> Result<Self> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (mut replica, ()) = Replica::create_with(SlideModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, slide_file, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        replica.take_changed();
        let index = BlobIndex::build(&replica);
        let model = SlideModel::from_parts(slide_file.clone(), ids);
        Ok(Self { replica, model, blobs, index, blob_log: Vec::new(), comment_only: false, local: LocalRepair::unknown() })
    }

    /// Opens a snapshot (or a full change export) of another replica as a new replica of a fresh
    /// actor. `blobs` must hold the blobs it references.
    pub fn open_snapshot(bytes: &[u8], blobs: MemoryBlobs) -> Result<Self> {
        let mut replica = Replica::load(SlideModel::schema(), bytes, ActorId::random()).map_err(kit_err)?;
        replica.take_changed();
        let model = SlideModel::load(&replica, &blobs)?;
        let index = BlobIndex::build(&replica);
        let mut session = Self { replica, model, blobs, index, blob_log: Vec::new(), comment_only: false, local: LocalRepair::unknown() };
        // Best effort: a replica merged elsewhere may need the read repair; a failure leaves it.
        let _ = session.repair_read();
        session.index.collect_garbage(&mut session.blobs);
        Ok(session)
    }

    /// The view: the slide as of the last record, undo, redo or import.
    pub fn slide_file(&self) -> &SlideFile {
        &self.model.slide_file
    }

    pub fn model(&self) -> &SlideModel {
        &self.model
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then
    /// makes `after` the view. Answers how many undo steps that made: 1, or 0 when nothing
    /// replicated changed or `origin` is a `system`/`import` one (never an undo step). On failure
    /// nothing is written and the view is unchanged.
    pub fn record(&mut self, origin: &Origin, after: &SlideFile) -> Result<u32> {
        let comment_only = self.comment_only;
        let mut ids = self.model.ids().clone();
        let before = self.replica.version();
        let (model, blobs) = (&self.model, &mut self.blobs);
        let result = self.replica.transact(origin, |tx| {
            if comment_only {
                write_comment_changes(tx, &model.slide_file, after, &mut ids).map(|_| ())
            } else {
                write_changes(tx, &model.slide_file, after, &mut ids, blobs).map(|_| ())
            }
        });
        let changed = self.replica.take_changed();
        if let Err(error) = result {
            self.index.collect_garbage(&mut self.blobs);
            return Err(kit_err(error));
        }
        let moved = self.index.update(&self.replica, &changed);
        let replica = &self.replica;
        self.local.wrote(&changed, |object| replica.exists(object), self.model.ids());
        self.model.adopt(after.clone(), ids);
        let wrote = self.replica.version() != before;
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        self.log_added(moved.added.iter().cloned());
        if undoable {
            self.index.push_step(UndoScope::of(origin.kind).unwrap_or(UndoScope::Person), moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok(u32::from(undoable))
    }

    /// Makes every later [`SlideSession::record`] write only what changed in the comment threads
    /// (the `CommentThread` and `Comment` objects, rule M23): all a
    /// principal with the sync protocol's `commenter` role may push. The engine refuses the
    /// person's other edits before they reach the view.
    pub fn set_comment_only(&mut self, on: bool) {
        self.comment_only = on;
    }

    pub fn comment_only(&self) -> bool {
        self.comment_only
    }

    /// Whether the replica holds `slide_file`'s content, compared id-blind ([`normalize`]) as
    /// [`read_document`] reads both back. Errs if `slide_file` cannot be written to a replica.
    pub fn holds(&self, slide_file: &SlideFile) -> Result<bool> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (scratch, ()) = Replica::create_with(SlideModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, slide_file, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        let (theirs, _) = read_document(&scratch, &blobs).map_err(kit_err)?;
        let (mine, _) = read_document(&self.replica, &self.blobs).map_err(kit_err)?;
        let value = |p: &SlideFile| serde_json::to_value(normalize(p)).map_err(kit_err);
        Ok(value(&theirs)? == value(&mine)?)
    }

    /// Gives the view `file`'s own ids, when the replica holds `file`'s content in the same order
    /// ([`SlideSession::holds`]; a file opened with the replica stored with it): its masters,
    /// layouts, slides, shapes and media keep the numbers the file gave them, which comments,
    /// links, animations and other programs name, and the view is `file` itself. A fresh read
    /// numbers what the replica stores no number for (masters, layouts, their shapes, media) anew.
    /// Answers whether it did; otherwise the view is left as it was.
    pub fn take_ids_of(&mut self, file: &SlideFile) -> Result<bool> {
        let Some(known) = crate::Known::as_in(&self.model.slide_file, self.model.ids(), file) else { return Ok(false) };
        let (read, ids) = crate::read_document_known(&self.replica, &self.blobs, &known).map_err(kit_err)?;
        // Exactly the file but for what a replica does not hold: the id counter and the view properties.
        let comparable = |p: &SlideFile| {
            let mut p = p.clone();
            p.next_id = 0;
            p.view_props = None;
            serde_json::to_value(p).map_err(kit_err)
        };
        if comparable(&read)? != comparable(file)? {
            return Ok(false);
        }
        self.model.adopt(file.clone(), ids);
        Ok(true)
    }

    /// Undoes the person's last undo step on this replica (never imports or repairs; an agent's
    /// steps are the agent's, KitModel M12a), repairs the result and patches the view. Answers
    /// whether there was one, and the damage.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    /// Undoes the last undo step of `scope` (the person's or the agent's, M12a): the other scope's
    /// later edits are kept, as other replicas' are.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if !self.can_undo_in(scope) {
            return Ok((false, Damage::None));
        }
        let done = self.replica.undo_in(scope).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        if done {
            self.log_added(moved.added.iter().cloned());
            self.index.undone(scope, moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok((done, damage?))
    }

    /// Redoes the last undone step of `scope`.
    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if !self.can_redo_in(scope) {
            return Ok((false, Damage::None));
        }
        let done = self.replica.redo_in(scope).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        if done {
            self.log_added(moved.added.iter().cloned());
            self.index.redone(scope, moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok((done, damage?))
    }

    /// Whether the person has an undo step to take: one of this replica's, not trimmed by
    /// [`SlideSession::retain_steps`].
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_undo_in(scope) && self.index.undo_steps_in(scope) > 0
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_redo_in(scope) && self.index.redo_steps_in(scope) > 0
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps reachable (the
    /// engine history's own) and drops the blobs only older steps needed: the replica has no
    /// limit of its own, so this is what keeps a long session's media bounded.
    pub fn retain_steps(&mut self, undo: usize, redo: usize) {
        self.index.retain(undo, redo);
        self.index.collect_garbage(&mut self.blobs);
        let blobs = &self.blobs;
        self.blob_log.retain(|(_, hashes)| hashes.iter().any(|hash| blobs.0.contains_key(hash)));
    }

    /// [`SlideSession::retain_steps`] for one undo scope's steps (the engine keeps one history
    /// with each step's scope).
    pub fn retain_steps_in(&mut self, scope: UndoScope, undo: usize, redo: usize) {
        self.index.retain_in(scope, undo, redo);
        self.index.collect_garbage(&mut self.blobs);
        let blobs = &self.blobs;
        self.blob_log.retain(|(_, hashes)| hashes.iter().any(|hash| blobs.0.contains_key(hash)));
    }

    /// Undo and redo steps reachable, as [`SlideSession::retain_steps`] left them.
    pub fn steps(&self) -> (usize, usize) {
        (self.index.undo_steps(), self.index.redo_steps())
    }

    /// Imports remote or stored changes (their blobs added first, [`SlideSession::add_blobs`]),
    /// repairs the merged state and patches the view. An import whose repair or view load fails
    /// has still imported its changes: [`SlideSession::view_loaded`] tells whether the view
    /// followed.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.replica.import_changes(bytes).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage
    }

    /// Whether the view matches the replica as it is now (its own read succeeds).
    pub fn view_loaded(&self) -> bool {
        read_document(&self.replica, &self.blobs).is_ok()
    }

    /// The blobs a peer at `since` needs to reach this session's current state (Kition Photo's
    /// `blobs_since`): the hashes logged for a version `since` does not include, still in the
    /// store. An empty `since` answers every blob the current state references (a creating
    /// transaction or a snapshot is never logged).
    pub fn blobs_since(&self, since: &Version) -> Vec<(String, Arc<[u8]>)> {
        if *since == Version::empty() {
            return self.index.referenced().into_iter().filter_map(|hash| self.blobs.0.get(&hash).cloned().map(|bytes| (hash, bytes))).collect();
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (version, hashes) in &self.blob_log {
            if since.includes(version) {
                continue;
            }
            for hash in hashes {
                if seen.insert(hash.clone())
                    && let Some(bytes) = self.blobs.0.get(hash)
                {
                    out.push((hash.clone(), bytes.clone()));
                }
            }
        }
        out
    }

    fn log_added(&mut self, added: impl IntoIterator<Item = String>) {
        let added: Vec<String> = added.into_iter().collect();
        if !added.is_empty() {
            self.blob_log.push((self.replica.version(), added));
        }
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.replica.export_changes(since).map_err(kit_err)
    }

    pub fn version(&self) -> Version {
        self.replica.version()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    pub fn state_hash(&self) -> Result<String> {
        self.replica.state_hash().map_err(kit_err)
    }

    pub fn doc_id(&self) -> DocId {
        self.replica.doc_id()
    }

    /// What local writes left for the next repair after a deletion.
    #[cfg(test)]
    pub(crate) fn local_repair(&self) -> &LocalRepair {
        &self.local
    }

    /// Forgets what local writes left, so the next repair after a deletion checks everything.
    #[cfg(test)]
    pub(crate) fn forget_local_repair(&mut self) {
        self.local = LocalRepair::unknown();
    }

    /// Repairs everything now, as after a merge that deleted what no slide holds (a test's
    /// starting point: a replica whose local writes are all repaired).
    #[cfg(test)]
    pub(crate) fn repair_all(&mut self) {
        let result = self.replica.transact(&Origin::system("repair"), |tx| crate::validate::validate_all(tx).map(|_| ()));
        let changed = self.replica.take_changed();
        result.unwrap_or_else(|error| panic!("repair: {error}"));
        self.local = LocalRepair::settled();
        self.refresh(&changed).unwrap_or_else(|error| panic!("refresh: {error}"));
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// The blobs the replica references or an undo, redo or pending import can still need.
    pub fn blobs(&self) -> &MemoryBlobs {
        &self.blobs
    }

    /// Adds blobs (from a peer, before importing its changes): kept until the next import. Blobs
    /// that arrive after the changes naming them (the view showed that media empty) reload the
    /// view; answers whether it did, so the engine takes the new view.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<bool> {
        for (hash, bytes) in &blobs.0 {
            self.blobs.0.entry(hash.clone()).or_insert_with(|| bytes.clone());
        }
        self.index.protect(blobs.0.keys().cloned());
        if !self.model.awaits_any_of(blobs) {
            return Ok(false);
        }
        let store: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, &BTreeSet::new(), store)?;
        Ok(true)
    }

    /// As `KitSession`'s: repairs what changed in a `system` transaction (not an undo step), then
    /// patches the view with both; the view follows the replica even if the repair fails. Also
    /// answers which blobs moved (the index is updated either way).
    fn settle(&mut self) -> (Result<Damage>, Moved) {
        let mut changed = self.replica.take_changed();
        let repaired = if changed.is_empty() {
            Ok(())
        } else {
            let mut kit_error = None;
            // The view still shows the state before the change: it tells where deleted objects
            // lived, so a slide's deletion repairs only what can name its objects.
            let (ids, local) = (self.model.ids(), &self.local);
            let mut settled = false;
            let result = self.replica.transact(&Origin::system("repair"), |tx: &mut Tx<'_>| {
                crate::validate::validate_with_view(tx, &changed, ids, local).map(|(_, done)| settled = done).map_err(kit_err).map_err(|error| {
                    let message = error.to_string();
                    kit_error = Some(error);
                    KdmError::Engine(message)
                })
            });
            if settled && result.is_ok() {
                self.local = LocalRepair::settled();
            }
            match (result, kit_error) {
                (_, Some(error)) => Err(error),
                (result, None) => result.map_err(kit_err),
            }
        };
        changed.extend(self.replica.take_changed());
        let moved = self.index.update(&self.replica, &changed);
        let damage = self.refresh(&changed).and_then(|damage| repaired.map(|()| damage));
        let damage = match damage {
            Ok(damage) => self.repair_read().map(|repaired| if repaired { Damage::Full } else { damage }),
            failed => failed,
        };
        (damage, moved)
    }

    /// The read repair (`crate::siblings`): what the view shows that the replica does not hold yet
    /// (a paragraph or a comment listed twice), written as a `system` transaction (not an undo
    /// step), then the view refreshed. Answers whether it wrote.
    fn repair_read(&mut self) -> Result<bool> {
        let fixes = siblings::plan(&self.replica, self.model.ids());
        // A commenter leaves them to an editor: the view shows them listed once either way.
        let threads = if self.comment_only { Vec::new() } else { self.model.ids().relist.clone() };
        if fixes.is_empty() && threads.is_empty() {
            return Ok(false);
        }
        let result = self.replica.transact(&Origin::system("repair"), |tx| Ok(siblings::apply(tx, &fixes)? | crate::comments::repair(tx, &threads)?));
        let changed = self.replica.take_changed();
        let wrote = result.map_err(kit_err)?;
        let moved = self.index.update(&self.replica, &changed);
        self.log_added(moved.added.iter().cloned());
        self.refresh(&changed)?;
        Ok(wrote)
    }

    fn refresh(&mut self, changed: &BTreeSet<ObjectId>) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, changed, blobs)
    }
}
