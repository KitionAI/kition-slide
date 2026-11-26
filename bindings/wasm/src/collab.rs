//! Kition's collaboration members (engine contract 2, Kition phase 3d Task 3): the slide
//! as a KDM replica.
//!
//! The engine keeps its own `Session` (the command bridge to the replica,
//! `kition_slide_engine::kdm_bridge`), not a `kition_kit::session::KitSession` directly, so
//! `kition_kit::export_kit_session!` does not apply here; these members are added by hand with
//! the exact JS names and shapes `kition-sdk`'s `session::web` gives
//! (`crates/kition-kit/src/session/web.rs`), so Kition's host code is the same for every kit. The
//! wasm bindings live in [`crate::web`].
//!
//! Slides embed media (pictures, audio, video) in the replica, so [`SlideDocument::blobs`],
//! [`SlideDocument::add_blobs`] and [`SlideDocument::blobs_since`] move real content, encoded the
//! same way Kition Photo's do: `(64-char ASCII-hex sha256, u64 LE length, bytes) * N`.

use std::collections::HashMap;
use std::sync::Arc;

use kdm_model::SlideModel;
use kition_kit::kdm::Schema;
use kition_kit::session::KitModel;
use kition_kit::session::model::ReplicaPort;
use kition_kit::{KitCommand, KitDocument};
use kition_slide_engine::{Damage, MemoryBlobs, Origin, Session, Version};
use serde_json::{Value, json};

use crate::{SlideDocument, engine_error};

impl SlideDocument {
    pub(crate) fn premise_fork(&self) -> Result<Self, String> {
        use kition_kit::kdm::UndoScope;

        let state = self.session.doc().map_err(engine_error)?;
        let kdm = state.kdm().ok_or("the slide has no replica for premise planning")?;
        // Snapshot loading does not copy undo stacks or an in-progress gesture. Keep the
        // SDK's existing fallback for sessions that need those rather than claiming equivalence.
        if state.is_dirty()
            || state.interaction.is_some()
            || !state.history.undo.is_empty()
            || !state.history.redo.is_empty()
            || self.session.comment_only().is_some()
            || [UndoScope::Person, UndoScope::Agent].iter().any(|scope| kdm.can_undo_in(*scope) || kdm.can_redo_in(*scope))
        {
            return Err("premise snapshot requires a clean session without undo history or a live gesture".into());
        }
        let snapshot = kdm.replica().snapshot().map_err(engine_error)?;
        let mut session = Session::new();
        session.open_snapshot(&snapshot, kdm.blobs().clone()).map_err(engine_error)?;
        if let Some(editor) = self.session.editor_state() {
            session.restore_editor_state(&editor).map_err(engine_error)?;
        }
        let copied = session.doc_mut().map_err(engine_error)?;
        Arc::make_mut(&mut copied.doc).next_id = state.doc.next_id;
        copied.saved_doc = copied.doc.clone();
        copied.path.clone_from(&state.path);
        session.prefs = self.session.prefs.clone();
        session.clipboard = self.session.clipboard.clone();
        session.tool = self.session.tool.clone();
        session.painter = self.session.painter.clone();
        session.anim_painter = self.session.anim_painter.clone();
        session.default_look = self.session.default_look.clone();
        session.media_status = self.session.media_status.clone();
        session.gif_paused = self.session.gif_paused.clone();
        let mut fork = Self::from_session(session);
        // A native caller may not be hosted. Planning still never writes or opens files by path.
        fork.session.set_host_files_only(true);
        fork.premise_plan = true;
        Ok(fork)
    }

    /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
    /// capability: `.kslide`/`.pptx` cannot carry their own replica): the slide is the
    /// replica's, so it keeps its KDM identity across saves. The host vouches that the replica was
    /// saved with these bytes; when they disagree (an older save, a slide edited outside the
    /// replica) the engine records the file on top as one import. A replica that does not open
    /// (damaged, a newer schema) is ignored and the file opens as [`SlideDocument::open`] opens
    /// it.
    ///
    /// Empty `bytes` mean there is no file yet, only the replica (a host opening a shared
    /// slide for the first time from its sync room's snapshot, to save it as a new file):
    /// the slide is the replica's alone.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        match restore(name, bytes, snapshot, blobs) {
            Ok(session) => Ok(Self::from_session(session)),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the slide has made its replica yet: it makes one lazily, on the first
    /// shared edit or when asked for it ([`SlideDocument::version`] and the rest).
    pub fn has_replica(&self) -> bool {
        self.session.active().is_some_and(|state| state.kdm().is_some())
    }

    /// The replica's own document id (a `kition-sdk` `DocId`, stable across saves and across
    /// opening another replica of the same document — never Kition Slide's own `.kslide`
    /// path/name): Kit UI's generic runtime reads this for the Kit UI protocol's `replica/info`
    /// message. `None` (`undefined` in JS, through [`crate::web`]) before the slide has
    /// made a replica yet ([`SlideDocument::has_replica`]).
    pub fn doc_id(&self) -> Option<String> {
        self.session.doc_id()
    }

    /// The slide's replica version (empty before it has made one).
    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.session.version().map_err(engine_error)?.encode())
    }

    /// The slide's changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        self.session.export_changes(&since).map_err(engine_error)
    }

    /// Imports a peer's (or stored) changes; add their blobs first with
    /// [`SlideDocument::add_blobs`]. Answers what went stale on screen.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.session.import_changes(bytes).map_err(engine_error)
    }

    /// The slide's replica as a snapshot, for a peer to open as a new replica of the same
    /// slide ([`SlideDocument::open`]'ing the saved bytes does not: `.kslide` carries
    /// no replica).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.session.snapshot().map_err(engine_error)
    }

    /// Undoes the slide's last local step on its replica. `done` is `false`, not an error,
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.execute("edit.undo", json!({}))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.execute("edit.redo", json!({}))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn can_undo(&self) -> bool {
        self.session.active().is_some_and(|state| !state.history.undo.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        self.session.active().is_some_and(|state| !state.history.redo.is_empty())
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs, and it
    /// undoes as that replica's own step. Contract 1's [`SlideDocument::execute`](kition_kit::KitDocument::execute)
    /// is this with the local person.
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        let result = self.run_named(command, params, |session, params| session.execute_as(command, params, origin));
        self.take_requests();
        result
    }

    /// Every blob the slide's replica references or can still bring back, as one buffer a
    /// peer feeds straight into [`SlideDocument::add_blobs`].
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        let blobs = self.session.blobs().map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// Adds a peer's blobs (from their [`SlideDocument::blobs`]): kept until the next
    /// [`SlideDocument::import_changes`], which a host calls right after.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        let blobs = decode_blobs(bytes)?;
        self.session.add_blobs(&blobs).map_err(engine_error)
    }

    /// The blobs the slide's replica has newly needed since `since` (empty bytes: every
    /// blob the current state references), encoded the same way as [`SlideDocument::blobs`]: for
    /// live sync, so one change ships only the blobs it needs rather than the whole store.
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        let blobs = self.session.blobs_since(&since).map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.session.version().map_err(engine_error)?;
        let kdm = self.session.kdm().ok_or("the slide has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica forked from this one) in as one undo step of
    /// `origin`'s scope (`mergeChanges`, as `kition-sdk`'s `export_kit_session!` answers it): the
    /// engine's slide follows, and Undo takes the whole draft back. Add the draft's blobs first.
    /// Answers `{"damage", "scope", "summary"}`: `scope` the undo scope the step went to
    /// (`"person"`, or `null` when none), `summary` the `StateDiff` of the replicated state before
    /// and after. A refused merge changes nothing and fails with the KDM's message.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let before = self.replicated_state()?;
        let (scope, damage) = self.session.merge_changes(bytes, origin).map_err(engine_error)?;
        let after = self.replicated_state()?;
        self.take_requests();
        Ok(
            json!({ "damage": damage, "scope": scope.map(|scope| scope.as_str()), "summary": kition_kit::kdm::diff::diff_states(&before, &after).to_json() }),
        )
    }

    /// What merging a draft's changes would change in the slide now, as `StateDiff` JSON, the
    /// replica's repair of the merged state included as the merge runs it
    /// (`SlideSession::merged_state`), so the preview is what accepting gives; nothing changes.
    /// Refuses what [`SlideDocument::merge_changes`] refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<Value, String> {
        let kdm = self.session.kdm().ok_or("the slide has no replica, so no draft of it")?;
        let before = kdm.replica().state_json().map_err(engine_error)?;
        let after = kdm.merged_state(bytes).map_err(engine_error)?;
        Ok(kition_kit::kdm::diff::diff_states(&before, &after).to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this slide, the Kit UI protocol's
    /// `replica/restore`) over the slide by object id, as one undo step of `origin`'s scope: the
    /// engine's slide follows. Add the blobs it references first. Nothing changes when the states
    /// are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        let damage = self.session.restore_state(snapshot, origin).map_err(engine_error)?;
        self.take_requests();
        Ok(damage)
    }

    /// An agent's reply (JSON text) with every object id it holds as a value or a key named as
    /// the tool interface names it (its tool id, KitObjectModel OM9); unchanged without a replica,
    /// when it names no object, or when the schema declares no tool ids. The slide keeps its own
    /// ids. The editor sends every `ai/inspect`, `document/check` and `ai/run` reply through this,
    /// so the tool ids are read at the objects the reply names alone (`ToolIds::show_reply`), not
    /// at every object of the deck.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let Some(kdm) = self.session.kdm() else { return Ok(value.to_string()) };
        Ok(kition_kit::kdm::tool_ids::ToolIds::show_reply(kdm.replica(), &value).to_string())
    }

    /// What changed since the version `since` (empty bytes: since the replica began), a receipt as
    /// `kition-sdk`'s `changesSince` answers it: `{"changes": StateDiff}`, or `{"changes": null,
    /// "unavailable": "<why>"}` when the replica's log does not hold `since` (or there is no
    /// replica yet), never an empty change set in its place.
    pub fn changes_since(&self, since: &[u8]) -> Value {
        let Some(kdm) = self.session.kdm() else { return json!({ "changes": null, "unavailable": "the slide has no replica yet" }) };
        let replica = kdm.replica();
        let since = if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since) };
        match since.and_then(|since| replica.changes_since(&since)) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error.to_string() }),
        }
    }

    /// How many times the slide left its replica because it refused a write or an undo (a
    /// slide it cannot hold, or a bug). The slide keeps working, undoing from its
    /// own history; the next shared edit makes a fresh replica, which collaborators must resync
    /// (its full [`SlideDocument::snapshot`]). Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        u32::try_from(self.session.kdm_resets()).unwrap_or(u32::MAX)
    }
}

/// The object model (`KitObjectModel 1.0`) over the slide file's change exchange: `kition-slide`
/// serves `model.describe` and `model.batch` headless through `kition_kit::session::model::Exchange`,
/// and `model.open`, `model.import` and `model.export` with the media the changes reference.
impl ReplicaPort for SlideDocument {
    fn model_schema(&self) -> Arc<Schema> {
        <SlideModel as KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let changes = self.export_changes(since)?;
        self.refresh_batch_media()?;
        Ok(changes)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<KitCommand> {
        self.commands()
    }

    fn model_add_blobs(&mut self, blobs: &[u8]) -> Result<(), String> {
        self.add_blobs(blobs)
    }

    fn model_blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.blobs_since(since)
    }

    /// A deck with no file yet (an agent's headless draft, a server's headless kit opening a sync
    /// room): the replica alone, failing rather than falling back to an empty deck.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        restore(name, &[], snapshot, blobs).map(Self::from_session)
    }
}

/// A session with the deck `snapshot` holds (and `bytes` on top, when given), saved to `name`.
fn restore(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Session, String> {
    crate::served_by_host();
    let blobs = decode_blobs(blobs)?;
    let mut session = Session::new();
    session.open_with_replica(bytes, snapshot, blobs).map_err(engine_error)?;
    // The new slide is the active one, as after `open`, and is saved to `name`.
    session.doc_mut().map_err(engine_error)?.path = Some(name.to_owned());
    Ok(session)
}

/// Encodes blobs for one wasm call's worth of blob transfer
/// ([`SlideDocument::blobs`]/[`SlideDocument::add_blobs`]): `(64-byte ASCII-hex hash, u64 LE
/// length, bytes) * N`. Every hash the KDM model produces is a 64-char hex SHA-256, so the hash
/// field needs no length prefix of its own.
fn encode_blobs(blobs: &MemoryBlobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in &blobs.0 {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`]; truncated or malformed input is an error, never a panic.
fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        let hash = std::str::from_utf8(hash).map_err(|_| "blob bundle: hash is not ASCII".to_owned())?.to_owned();
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        let data = bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        map.insert(hash, Arc::from(data));
        i = data_end;
    }
    Ok(MemoryBlobs(map))
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = SlideDocument::create("Slide.kslide", 960, 540).unwrap();
        doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap();
        doc.save(None).unwrap()
    }

    fn extra_edit(doc: &mut SlideDocument) {
        doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [200, 200, 60, 60] })).unwrap();
        doc.execute("shape.fill", json!({ "color": "#0000ff" })).unwrap();
    }

    #[test]
    fn check_names_shapes_by_their_object_once_there_is_a_replica() {
        let mut doc = SlideDocument::create("Slide.kslide", 960, 540).unwrap();
        let plain: Vec<_> = doc.check().into_iter().filter(|d| d.target.path.as_deref().is_some_and(|p| p.starts_with("shape:"))).collect();
        assert!(!plain.is_empty(), "a new slide's empty placeholders are reported");
        // Diagnostics name stored tool ids before the replica exists, and the same ids after it.
        assert!(plain.iter().all(|d| d.target.object_id.is_some()), "no replica, still stored object ids");
        doc.version().unwrap();
        let named: Vec<_> = doc.check().into_iter().filter(|d| d.target.path.as_deref().is_some_and(|p| p.starts_with("shape:"))).collect();
        assert_eq!(named.len(), plain.len());
        let before: Vec<_> = plain.iter().map(|d| d.target.object_id.clone()).collect();
        let after: Vec<_> = named.iter().map(|d| d.target.object_id.clone()).collect();
        assert_eq!(after, before, "the replica keeps the stored object ids");
        let ids: std::collections::BTreeSet<_> = named.iter().map(|d| d.target.object_id.clone().expect("an object id")).collect();
        assert_eq!(ids.len(), named.len(), "each shape names its own object");
    }

    #[test]
    fn version_and_snapshot_force_a_replica_lazily() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        assert!(!doc.has_replica(), "opening a .kslide plainly makes no replica");
        let version = doc.version().unwrap();
        assert!(doc.has_replica());
        assert_eq!(doc.version().unwrap(), version, "asking twice makes no further replica");
        let snap = doc.snapshot().unwrap();
        assert!(!snap.is_empty());
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        assert!(!doc.can_undo());
        doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [10, 10, 20, 20] })).unwrap();
        assert!(doc.can_undo());
        let (done, damage) = doc.undo().unwrap();
        assert!(done);
        assert_ne!(damage, Damage::None);
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        let (done, _) = doc.redo().unwrap();
        assert!(done);
        // Undoing with nothing to undo is a normal `false`, not an error.
        doc.undo().unwrap();
        let (done, damage) = doc.undo().unwrap();
        assert!(!done);
        assert_eq!(damage, Damage::None);
    }

    #[test]
    fn execute_as_records_the_given_origin() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let origin = Origin::person("user:agent-1");
        doc.execute_as("shape.insert", json!({ "preset": "ellipse", "rect": [10, 10, 20, 20] }), &origin).unwrap();
        assert!(doc.can_undo());
    }

    /// The editor's own reads after a change (`slide.inspect` with `select: false` among them)
    /// and an agent's `file.inspect` leave the person where they are: in Slide Master view, on
    /// the master they picked. The editor's `slide.inspect` used to drop them back to the slides.
    #[test]
    fn the_editors_reads_and_an_agents_keep_the_person_in_slide_master_view() {
        use kition_kit::KitDocument;
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        doc.version().unwrap(); // with its replica, as in the editor
        let agent = Origin::agent("agent");
        KitDocument::execute(&mut doc, "slide.new", json!({})).unwrap();
        KitDocument::execute(&mut doc, "view.slideMaster", json!({ "master": 0, "layout": null })).unwrap();
        let before = KitDocument::execute(&mut doc, "file.inspect", json!({})).unwrap();
        assert_eq!(before["selection"]["target"]["kind"], "master", "{before}");
        for (command, params) in [
            ("format.state", json!({})),
            ("file.inspect", json!({})),
            ("slide.inspect", json!({ "index": 1, "select": false })),
            ("comment.threads", json!({})),
            ("presence.selection", json!({})),
        ] {
            KitDocument::execute(&mut doc, command, params).unwrap();
            let now = KitDocument::execute(&mut doc, "file.inspect", json!({})).unwrap();
            assert_eq!(now["selection"]["target"]["kind"], "master", "after {command}: {now}");
        }
        let seen = doc.execute_as("file.inspect", json!({}), &agent).unwrap();
        assert_eq!(seen["selection"]["target"]["kind"], "master", "the agent reads the person's view: {seen}");
        let after = KitDocument::execute(&mut doc, "file.inspect", json!({})).unwrap();
        assert_eq!(after["selection"]["target"]["kind"], "master", "the agent's read moved the person: {after}");
        // Reading every slide (the slides panel) moves neither the view nor the current slide.
        KitDocument::execute(&mut doc, "view.closeMaster", json!({})).unwrap();
        KitDocument::execute(&mut doc, "slide.go", json!({ "index": 0 })).unwrap();
        for index in 0..2 {
            KitDocument::execute(&mut doc, "slide.inspect", json!({ "index": index, "select": false })).unwrap();
        }
        let read = KitDocument::execute(&mut doc, "file.inspect", json!({})).unwrap();
        assert_eq!(read["selection"]["slide"], 0, "{read}");
        // Without `select: false` an inspected slide still becomes the one being edited.
        KitDocument::execute(&mut doc, "slide.inspect", json!({ "index": 1 })).unwrap();
        let moved = KitDocument::execute(&mut doc, "file.inspect", json!({})).unwrap();
        assert_eq!(moved["selection"]["slide"], 1, "{moved}");
    }

    /// `docId` is `None` before any replica exists, and the replica's own id (not the file's
    /// `name`, and stable across a save/reopen with that same replica) once one does.
    #[test]
    fn doc_id_is_none_until_a_replica_exists_then_the_replicas_own_id() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        assert_eq!(doc.doc_id(), None, "no replica yet");
        doc.version().unwrap(); // makes the replica lazily
        let id = doc.doc_id().unwrap_or_else(|| panic!("doc_id should be set once a replica exists"));
        assert!(!id.is_empty());
        assert_eq!(doc.doc_id().as_deref(), Some(id.as_str()), "stable across further asks");

        let bytes = doc.save(None).unwrap();
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        let reopened = SlideDocument::open_with_replica("Slide.kslide", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.doc_id(), Some(id), "the reopened replica keeps the same document id");
    }

    /// With no file yet (empty bytes), the replica alone opens: a peer's full change export
    /// becomes the same slide, same identity, and it saves as a new `.kslide`.
    #[test]
    fn a_replica_opens_without_a_file() {
        let mut a = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        extra_edit(&mut a);
        let changes = a.export_changes(&[]).unwrap();
        let mut b = SlideDocument::open_with_replica("Shared.kslide", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(b.version().unwrap(), a.version().unwrap());
        assert!(!b.dirty());
        assert!(SlideDocument::open("Shared.kslide", &b.save(None).unwrap()).is_ok());
        assert!(SlideDocument::open_with_replica("Shared.kslide", &[], b"not a replica", &[]).is_err());
    }

    /// A receipt names what changed since a version the replica's log holds, and says so when it
    /// cannot (no replica yet, a version of another line).
    #[test]
    fn changes_since_answers_a_receipt_or_why_not() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        assert!(doc.changes_since(&[])["changes"].is_null(), "no replica yet");
        let since = doc.version().unwrap();
        extra_edit(&mut doc);
        let receipt = doc.changes_since(&since);
        assert!(receipt["changes"].is_object(), "{receipt}");
        let now = doc.version().unwrap();
        assert_ne!(receipt["changes"], doc.changes_since(&now)["changes"], "the edit is in the receipt: {receipt}");
        let other = SlideDocument::open("Slide.kslide", &fixture()).unwrap().version().unwrap();
        let refused = doc.changes_since(&other);
        assert!(refused["changes"].is_null() && refused["unavailable"].is_string(), "{refused}");
    }

    /// The shapes of the first slide, read without moving the person.
    fn shape_count(doc: &mut SlideDocument) -> usize {
        doc.execute("slide.inspect", json!({ "index": 0, "select": false })).unwrap()["shapes"].as_array().unwrap().len()
    }

    /// A draft (a replica forked from the slide's) merges in as one step of the person's Undo: the
    /// comparison beforehand names what the merge does and changes nothing, the engine's slide
    /// follows the merge, an edit made here meanwhile stays, and a refused merge changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_slide_follows() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = SlideDocument::open_with_replica("Slide.kslide", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("shape.insert", json!({ "preset": "star5", "rect": [300, 300, 50, 50] })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        doc.execute("slide.rename", json!({ "name": "Meanwhile", "index": 0 })).unwrap();
        let before = shape_count(&mut doc);
        let exactly_before = exact(&mut doc);
        let version = doc.version().unwrap();
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "comparing changes nothing");
        let origin = Origin::person("person:local");
        let answer = doc.merge_changes(&changes, &origin).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert_ne!(answer["damage"], json!("none"), "{answer}");
        assert_eq!(shape_count(&mut doc), before + 1, "the engine's slide follows the merge");
        assert!(doc.dirty());
        assert!(doc.replicated_state().unwrap().to_string().contains("Meanwhile"), "the edit made here stays");
        let merged = exact(&mut doc);
        assert!(doc.undo().unwrap().0);
        assert_eq!(shape_count(&mut doc), before, "one undo takes the whole draft back");
        assert_eq!(exact(&mut doc), exactly_before, "exactly");
        assert!(doc.redo().unwrap().0);
        assert_eq!(exact(&mut doc), merged, "the redo gives back the merged state exactly");
        assert!(doc.undo().unwrap().0);
        let at = doc.version().unwrap();
        assert!(doc.merge_changes(b"not changes", &origin).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
    }

    /// The Animation objects of a replicated state.
    fn animations(state: &Value) -> usize {
        state["objects"].as_object().map_or(0, |objects| objects.values().filter(|object| object["$type"] == "Animation").count())
    }

    /// A draft's preview runs the replica's repair as the merge does: a draft animates the shape
    /// the document deleted meanwhile (and adds a star), the merge's repair deletes that animation (it names a
    /// deleted shape), and the preview's merged state is the state after actually merging (the
    /// unrepaired merge still holds the animation), its comparison the merge's summary.
    #[test]
    fn a_drafts_preview_is_what_merging_it_gives_its_repair_included() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let shape = doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [300, 300, 60, 60] })).unwrap()["id"].clone();
        let base = doc.version().unwrap();
        let mut draft = SlideDocument::open_with_replica("Slide.kslide", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("animation.add", json!({ "ids": [shape.clone()], "effect": "fade" })).unwrap();
        // Something the repair keeps, so the merge is an undo step of its own.
        draft.execute("shape.insert", json!({ "preset": "star5", "rect": [500, 300, 50, 50] })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        doc.execute("shape.delete", json!({ "ids": [shape] })).unwrap();
        let before = doc.replicated_state().unwrap();
        let exactly_before = exact(&mut doc);
        let version = doc.version().unwrap();
        let kdm = doc.session.kdm().unwrap();
        let unrepaired = kdm.replica().merged_state(&changes).unwrap();
        let preview = kdm.merged_state(&changes).unwrap();
        assert_eq!(animations(&unrepaired), animations(&before) + 1, "the merge alone keeps the drafted animation");
        assert_eq!(animations(&preview), animations(&before), "the preview has the repair: the animation of the deleted shape goes");
        let compared = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "a preview changes nothing");
        let answer = doc.merge_changes(&changes, &Origin::person("person:local")).unwrap();
        assert_eq!(doc.replicated_state().unwrap(), preview, "the preview's merged state is the merge's");
        assert_eq!(answer["summary"], compared, "the comparison named what the merge did: {answer}");
        assert!(doc.undo().unwrap().0);
        assert_eq!(exact(&mut doc), exactly_before, "one undo gives back the exact state before the merge");
    }

    /// The person's own records of the document: the engine's undo and redo labels (with each
    /// step's replica steps and scope), the selection, and the step index of the replica's blobs.
    fn records(doc: &SlideDocument) -> (Vec<(String, u32, bool)>, Vec<(String, u32, bool)>, String, (usize, usize)) {
        let st = doc.session.doc().unwrap();
        let entries = |steps: &[kition_slide_engine::HistoryEntry]| steps.iter().map(|e| (e.label.clone(), e.steps, e.agent)).collect::<Vec<_>>();
        (entries(&st.history.undo), entries(&st.history.redo), format!("{:?}", st.selection), doc.session.kdm().unwrap().steps())
    }

    /// A draft whose merge the repair takes back entirely records nothing (kition-sdk 6a57d68,
    /// data model 6.6): a draft animates a shape, and the person deletes the shape meanwhile (a
    /// named step); the repair deletes the drafted animation (it names a deleted shape), so the
    /// merge leaves the document as it was. The answer has no scope and an empty summary; the
    /// state hash, the version, the replica's undo depth and the engine's history, labels and
    /// selection stay; the next Undo is the person's deletion and takes exactly it back.
    #[test]
    fn a_draft_its_repair_takes_back_entirely_merges_as_no_step() {
        use kition_kit::kdm::UndoScope;
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let shape = doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [300, 300, 60, 60] })).unwrap()["id"].clone();
        let base = doc.version().unwrap();
        let mut draft = SlideDocument::open_with_replica("Slide.kslide", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("animation.add", json!({ "ids": [shape.clone()], "effect": "fade" })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let before_delete = exact(&mut doc);
        let hash_before_delete = doc.session.kdm().unwrap().state_hash().unwrap();
        doc.execute("shape.delete", json!({ "ids": [shape] })).unwrap();
        let deleted = doc.session.doc().unwrap().history.undo.last().unwrap().label.clone();
        let state = doc.replicated_state().unwrap();
        let (hash, version) = (doc.session.kdm().unwrap().state_hash().unwrap(), doc.version().unwrap());
        let depth = doc.session.kdm().unwrap().replica().undo_depth(UndoScope::Person);
        let records_before = records(&doc);
        let answer = doc.merge_changes(&changes, &Origin::agent("agent:drafter")).unwrap();
        assert!(answer["scope"].is_null(), "no step: {answer}");
        assert_eq!(answer["damage"], json!("none"), "{answer}");
        assert_eq!(answer["summary"], kition_kit::kdm::diff::diff_states(&state, &state).to_json(), "an empty summary: {answer}");
        let kdm = doc.session.kdm().unwrap();
        assert_eq!(kdm.state_hash().unwrap(), hash, "the state is as it was");
        assert_eq!(doc.version().unwrap(), version, "the version stays");
        assert_eq!(doc.session.kdm().unwrap().replica().undo_depth(UndoScope::Person), depth, "no undo step");
        assert_eq!(records(&doc), records_before, "the history, its labels and the selection stay");
        assert_eq!(doc.session.doc().unwrap().history.undo.last().unwrap().label, deleted, "the next Undo is named for the deletion");
        assert!(doc.undo().unwrap().0);
        assert_eq!(doc.session.kdm().unwrap().state_hash().unwrap(), hash_before_delete, "the Undo takes back exactly the deletion");
        assert_eq!(exact(&mut doc), before_delete);
    }

    /// The slides' file ids of the replicated state, sorted.
    fn file_ids(doc: &mut SlideDocument) -> Vec<Value> {
        let state = doc.replicated_state().unwrap();
        let mut ids: Vec<Value> = state["objects"]
            .as_object()
            .map(|objects| objects.values().filter(|object| object["$type"] == "Slide").map(|slide| slide["props"]["fileId"].clone()).collect())
            .unwrap_or_default();
        ids.sort_by_key(Value::to_string);
        ids
    }

    /// The engine's repair after a merge is in the merge's undo step (kition-sdk 6a57d68,
    /// `Replica::transact_in_step`): a draft and the document each add a slide, both with the same
    /// next file id; the merge renumbers the draft's (`stored::merged_file_ids`), as one replica
    /// undo step with the merge; one Undo gives back the exact state hash before the merge, the
    /// Redo the merged one.
    #[test]
    fn a_merged_drafts_renumbered_slide_ids_are_in_the_merges_one_undo_step() {
        use kition_kit::kdm::UndoScope;
        let file = SlideDocument::create("Slide.kslide", 960, 540).unwrap().save(None).unwrap();
        let mut seed = SlideDocument::open("Slide.kslide", &file).unwrap();
        let (snapshot, blobs) = (seed.snapshot().unwrap(), seed.blobs().unwrap());
        let mut doc = SlideDocument::open_with_replica("Slide.kslide", &file, &snapshot, &blobs).unwrap();
        let mut draft = SlideDocument::open_with_replica("Slide.kslide", &file, &snapshot, &blobs).unwrap();
        let base = doc.version().unwrap();
        draft.execute("slide.new", json!({})).unwrap();
        doc.execute("slide.new", json!({})).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let mine = file_ids(&mut doc);
        let theirs = file_ids(&mut draft);
        assert_eq!(mine, theirs, "both new slides took the same next file id");
        let hash = doc.session.kdm().unwrap().state_hash().unwrap();
        let depth = doc.session.kdm().unwrap().replica().undo_depth(UndoScope::Person);
        let answer = doc.merge_changes(&changes, &Origin::agent("agent:drafter")).unwrap();
        assert_eq!(answer["scope"], "person", "{answer}");
        let ids = file_ids(&mut doc);
        let mut distinct = ids.clone();
        distinct.dedup();
        assert_eq!((ids.len(), distinct.len()), (3, 3), "the repeat is renumbered: {ids:?}");
        assert_eq!(doc.session.kdm().unwrap().replica().undo_depth(UndoScope::Person), depth + 1, "the renumbering joined the merge's step");
        let top = doc.session.doc().unwrap().history.undo.last().unwrap();
        assert_eq!((top.label.as_str(), top.steps, top.agent), ("Merge Draft", 1, false));
        assert_histories_agree(&doc, "after the merge");
        let merged = doc.session.kdm().unwrap().state_hash().unwrap();
        assert!(doc.undo().unwrap().0);
        assert_eq!(doc.session.kdm().unwrap().state_hash().unwrap(), hash, "one Undo gives back the exact state before the merge");
        assert_eq!(doc.session.kdm().unwrap().replica().undo_depth(UndoScope::Person), depth);
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.session.kdm().unwrap().state_hash().unwrap(), merged, "the Redo gives back the merged state");
        assert_eq!(file_ids(&mut doc), ids);
        assert_histories_agree(&doc, "after the redo");
    }

    /// An earlier state (a checkpoint's snapshot) is written over the slide by object id as one
    /// undo step: the replica holds that state again, with the same ids, and Undo takes it back.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        doc.version().unwrap();
        let checkpoint = doc.snapshot().unwrap();
        let before = shape_count(&mut doc);
        extra_edit(&mut doc);
        assert_eq!(shape_count(&mut doc), before + 1);
        let exactly_before = exact(&mut doc);
        let origin = Origin::person("person:local");
        let damage = doc.restore_state(&checkpoint, &origin).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(shape_count(&mut doc), before, "the engine's slide follows the restore");
        let then = Replica::load(<SlideModel as KitModel>::schema(), &checkpoint, ActorId::random()).unwrap().state_json().unwrap();
        assert_eq!(doc.replicated_state().unwrap()["objects"], then["objects"], "the checkpoint's objects, under their ids");
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &origin).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        assert!(doc.undo().unwrap().0);
        assert_eq!(shape_count(&mut doc), before + 1, "one undo takes the restore back");
        assert_eq!(exact(&mut doc), exactly_before, "exactly");
        let other = SlideDocument::open("Slide.kslide", &fixture()).unwrap().snapshot().unwrap();
        assert!(doc.restore_state(&other, &origin).is_err(), "another document's state is refused");
    }

    /// The replicated state and the engine's own view of the deck (id-blind), for exact
    /// comparisons.
    fn exact(doc: &mut SlideDocument) -> (Value, Value) {
        let state = doc.replicated_state().unwrap();
        let view = serde_json::to_value(kdm_model::normalize(&doc.session.doc().unwrap().doc)).unwrap();
        (state, view)
    }

    /// The replica steps the engine's history holds for one scope: (undo, redo).
    fn engine_steps(doc: &SlideDocument, agent: bool) -> (usize, usize) {
        let history = &doc.session.doc().unwrap().history;
        let sum = |steps: &[kition_slide_engine::HistoryEntry]| steps.iter().filter(|e| e.agent == agent).map(|e| e.steps as usize).sum::<usize>();
        (sum(&history.undo), sum(&history.redo))
    }

    /// The engine's history and the replica's undo agree, scope by scope.
    fn assert_histories_agree(doc: &SlideDocument, what: &str) {
        use kition_kit::kdm::UndoScope;
        let kdm = doc.session.kdm().unwrap();
        assert_eq!(engine_steps(doc, false), kdm.steps_in(UndoScope::Person), "{what}: the person's steps");
        assert_eq!(engine_steps(doc, true), kdm.steps_in(UndoScope::Agent), "{what}: the agent's steps");
    }

    /// The left edges of the first slide's last `n` shapes, in slide order.
    fn last_lefts(doc: &SlideDocument, n: usize) -> Vec<f64> {
        let shapes = &doc.session.doc().unwrap().doc.slides[0].shapes;
        shapes[shapes.len().saturating_sub(n)..].iter().map(|shape| shape.xfrm.map_or(f64::NAN, |x| x.x)).collect()
    }

    /// Rule M12a: a draft an agent made merges in as one undo step of the person who accepts it
    /// (the host passes the agent's origin). The agent's undo leaves it; the person's undo gives
    /// back the exact state before (replica and engine view), and the redo the merged one. The
    /// person's edit made meanwhile (another shape on the same slide) stays, before the draft's.
    #[test]
    fn a_merged_agent_draft_is_one_step_of_the_persons_undo_and_undoes_exactly() {
        use kition_kit::kdm::UndoScope;
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = SlideDocument::open_with_replica("Slide.kslide", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("shape.insert", json!({ "preset": "star5", "rect": [300, 300, 50, 50] })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [20, 20, 40, 40] })).unwrap();
        assert_eq!(last_lefts(&doc, 2), vec![100.0, 20.0]);
        let before = exact(&mut doc);
        let agent = Origin::agent("agent:drafter");
        let answer = doc.merge_changes(&changes, &agent).unwrap();
        assert_eq!(answer["scope"], "person", "the replica recorded the step as the person's: {answer}");
        assert_eq!(last_lefts(&doc, 3), vec![100.0, 20.0, 300.0], "both inserts are kept, the document's first");
        let merged = exact(&mut doc);
        let top = doc.session.doc().unwrap().history.undo.last().unwrap();
        assert!(!top.agent, "the merge is the person's step");
        assert_eq!(top.label, "Merge Draft");
        assert_eq!(doc.session.kdm().unwrap().steps_in(UndoScope::Agent), (0, 0), "the replica holds no agent step");
        assert_histories_agree(&doc, "after the merge");
        assert!(doc.execute_as("edit.undo", json!({}), &agent).is_err(), "the agent has nothing to undo: the merge is the person's");
        assert_eq!(exact(&mut doc), merged, "the agent's undo leaves the merged draft");
        assert!(doc.undo().unwrap().0);
        assert_eq!(exact(&mut doc), before, "the person's undo gives back the exact state before the merge");
        assert_histories_agree(&doc, "after the undo");
        assert!(doc.redo().unwrap().0);
        assert_eq!(exact(&mut doc), merged, "the redo gives back the merged state");
        assert_histories_agree(&doc, "after the redo");
        assert!(doc.undo().unwrap().0);
        assert!(doc.undo().unwrap().0);
        assert_eq!(last_lefts(&doc, 1), vec![100.0], "the person's own insert is the next step down");
    }

    /// A restored version undoes and redoes exactly; restored by an agent it is the agent's step.
    #[test]
    fn a_restored_version_undoes_and_redoes_exactly_in_its_origins_scope() {
        for agent in [false, true] {
            let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
            doc.version().unwrap();
            let checkpoint = doc.snapshot().unwrap();
            extra_edit(&mut doc);
            let before = exact(&mut doc);
            let origin = if agent { Origin::agent("agent:restorer") } else { Origin::person("person:local") };
            doc.restore_state(&checkpoint, &origin).unwrap();
            let restored = exact(&mut doc);
            assert_ne!(restored, before);
            assert_eq!(doc.session.doc().unwrap().history.undo.last().unwrap().agent, agent, "the step is in the origin's scope");
            assert_histories_agree(&doc, "after the restore");
            if !agent {
                assert!(doc.execute_as("edit.undo", json!({}), &Origin::agent("agent:other")).is_err(), "no agent step to undo");
                assert_eq!(exact(&mut doc), restored, "the agent's undo leaves the person's restore");
            }
            doc.execute_as("edit.undo", json!({}), &origin).unwrap();
            assert_eq!(exact(&mut doc), before, "one undo gives back the exact state before the restore");
            assert_histories_agree(&doc, "after the undo");
            doc.execute_as("edit.redo", json!({}), &origin).unwrap();
            assert_eq!(exact(&mut doc), restored, "the redo gives back the restored state");
            assert_histories_agree(&doc, "after the redo");
        }
    }

    /// The editor sends every agent reply through `modelReply`: on a deck of many objects a reply
    /// naming one object (or several) gets the ids the whole deck's tool ids give them, read at
    /// those objects alone, and a reply naming none comes back as it is.
    #[test]
    fn a_reply_names_the_objects_it_holds_as_the_whole_decks_tool_ids_do() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        for i in 0..30 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
            doc.execute("shape.insert", json!({ "preset": "rect", "rect": [10 + i, 10, 50, 50] })).unwrap();
        }
        let state = doc.replicated_state().unwrap();
        let objects: Vec<String> = state["objects"].as_object().unwrap().keys().cloned().collect();
        assert!(objects.len() > 60, "{} objects", objects.len());
        let full = kition_kit::kdm::tool_ids::ToolIds::of_replica(doc.session.kdm().unwrap().replica());
        assert!(full.is_active());
        let typed = |ty: &str| objects.iter().find(|id| state["objects"][id.as_str()]["$type"] == ty).cloned();
        assert!(typed("Slide").is_some() && typed("Shape").is_some(), "{state}");
        let named: Vec<String> = ["Slide", "Shape", "Layout", "Master", "TextBody", "Paragraph"]
            .into_iter()
            .filter_map(typed)
            .chain([objects[objects.len() / 2].clone()])
            .collect();
        for id in named {
            let mut keyed = serde_json::Map::new();
            keyed.insert(id.clone(), json!(1));
            let reply = json!({ "target": id, "list": [id, "text"], "keyed": keyed, "note": format!("about {id}") });
            let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
            assert_eq!(shown, full.show_reply_json(&reply), "{id}");
        }
        let every = json!({ "objects": objects });
        let shown: Value = serde_json::from_str(&doc.model_reply(&every.to_string()).unwrap()).unwrap();
        assert_eq!(shown, full.show_reply_json(&every), "every object named at once");
        let none = json!({ "text": "nothing here", "n": 3, "list": ["a", "slide:1"], "keyed": { "b": [null, true] } });
        assert_eq!(doc.model_reply(&none.to_string()).unwrap(), none.to_string(), "a reply naming no object comes back as it is");
    }

    /// An agent's reply names objects by their tool ids; text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let state = doc.replicated_state().unwrap();
        let slide = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, object)| object["$type"] == "Slide")
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("a slide object: {state}"));
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": slide, "note": format!("about {slide}") }).to_string()).unwrap()).unwrap();
        assert!(shown["target"].as_str().is_some_and(|id| id.starts_with("slide:")), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {slide}")), "text that holds an id stays");
        assert!(doc.model_reply("not json").is_err());
    }

    #[test]
    fn blobs_round_trip_through_encode_and_decode() {
        let mut a = MemoryBlobs::default();
        a.0.insert("a".repeat(64), Arc::from(b"hello".as_slice()));
        a.0.insert("b".repeat(64), Arc::from(b"".as_slice()));
        let bytes = encode_blobs(&a);
        let b = decode_blobs(&bytes).unwrap();
        assert_eq!(b.0.len(), 2);
        assert_eq!(b.0.get(&"a".repeat(64)).map(|v| v.as_ref()), Some(b"hello".as_ref()));
        assert_eq!(b.0.get(&"b".repeat(64)).map(|v| v.as_ref()), Some(b"".as_ref()));
    }

    #[test]
    fn decode_blobs_rejects_truncated_input_without_panicking() {
        assert!(decode_blobs(&[1, 2, 3]).is_err());
        assert!(decode_blobs(&"a".repeat(64).into_bytes()).is_err(), "hash with no length");
        let mut malformed = "a".repeat(64).into_bytes();
        malformed.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_blobs(&malformed).is_err(), "a length claiming more data than exists");
    }

    /// A `.kslide` saved with its replica reopens as that replica: same version, same
    /// rendered pixels, not dirty, and it takes new edits (which undo) at once.
    #[test]
    fn a_slide_file_reopens_with_its_stored_replica() {
        let mut a = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let version = a.version().unwrap();

        let mut b = SlideDocument::open_with_replica("Slide.kslide", &bytes, &snapshot, &blobs).unwrap();
        assert!(b.has_replica());
        assert_eq!(b.version().unwrap(), version);
        assert_eq!(b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(), a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap());
        assert!(!b.dirty());
        assert!(!b.can_undo());
        extra_edit(&mut b);
        assert!(b.dirty());
        assert_ne!(b.version().unwrap(), version);
        assert!(b.undo().unwrap().0);
        assert!(SlideDocument::open("Slide.kslide", &b.save(None).unwrap()).is_ok(), "it still saves as .kslide");
    }

    /// A damaged (or missing) stored replica never stops the file from opening: it opens plainly,
    /// never as a blank slide.
    #[test]
    fn a_damaged_stored_replica_opens_the_file_plainly() {
        let bytes = fixture();
        let a = SlideDocument::open("Slide.kslide", &bytes).unwrap();
        for (snapshot, blobs) in [(&b"not a snapshot"[..], &[][..]), (&[][..], &[1u8, 2, 3][..])] {
            let doc = SlideDocument::open_with_replica("Slide.kslide", &bytes, snapshot, blobs).unwrap();
            assert_eq!((doc.width(), doc.height()), (a.width(), a.height()));
            assert!(!doc.has_replica(), "the file opened plainly");
        }
        assert!(SlideDocument::open_with_replica("Slide.kslide", b"not a slide", b"not a snapshot", &[]).is_err());
    }

    /// `a` saves with its replica; `b` opens with it, so both start from the same replica
    /// identity. `a` edits further;
    /// `b.importChanges(a.exportChanges(b.version()))` brings `b` to the same replicated
    /// state as `a`, through the public SDK surface alone.
    #[test]
    fn a_second_document_catches_up_through_export_and_import_changes() {
        let mut a = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let snapshot = a.snapshot().unwrap();
        let blobs = a.blobs().unwrap();

        let mut b = SlideDocument::open_with_replica("Slide.kslide", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(b.version().unwrap(), a.version().unwrap(), "b starts from a's own replica");

        a.execute("shape.insert", json!({ "preset": "star5", "rect": [300, 300, 50, 50] })).unwrap();

        let since = b.version().unwrap();
        let changes = a.export_changes(&since).unwrap();
        b.add_blobs(&a.blobs().unwrap()).unwrap();
        b.import_changes(&changes).unwrap();

        assert_eq!(a.version().unwrap(), b.version().unwrap());
        assert_eq!(
            a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap(),
            b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(),
            "replicas at one version render the same slide"
        );
    }

    /// A collaborator's edit of one shape patches only its slide (Kit Standard 2.0, rule E11): the
    /// other slides keep their very `Arc`s, so their cached images stay and the import costs one
    /// slide's read, not the whole slide file's.
    #[test]
    fn a_remote_shape_edit_keeps_the_other_slides() {
        let mut a = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        a.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        a.execute("shape.insert", json!({ "preset": "star5", "rect": [50, 50, 60, 60] })).unwrap();
        a.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        let mut b = SlideDocument::open_with_replica("Slide.kslide", &[], &a.snapshot().unwrap(), &a.blobs().unwrap()).unwrap();
        let slides = |doc: &SlideDocument| doc.session.active().unwrap().doc.slides.clone();
        let before = slides(&b);
        assert_eq!(before.len(), 3);

        let since = a.version().unwrap();
        a.execute("slide.go", json!({ "index": 1 })).unwrap();
        let star = a.execute("slide.inspect", json!({ "index": 1, "select": false })).unwrap()["shapes"][0]["id"].clone();
        a.execute("shape.fill", json!({ "color": "#336699", "ids": [star] })).unwrap();
        b.import_changes(&a.export_changes(&since).unwrap()).unwrap();

        let after = slides(&b);
        assert!(!Arc::ptr_eq(&before[1], &after[1]), "the edited slide is new");
        assert!(Arc::ptr_eq(&before[0], &after[0]) && Arc::ptr_eq(&before[2], &after[2]), "the others are kept");
        assert_eq!(a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap(), b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap());
    }

    /// `kition-slide` serves the object model through [`ReplicaPort`]: reads, a set, and a command
    /// in one batch reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        use kition_kit::session::model::{Exchange, ModelErrorCode};

        let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert_eq!(described["root"], "SlideFile");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "slide.new"));

        let load = |exchange: &mut Exchange, doc: &mut SlideDocument, target: &str, field: &str| -> Vec<Value> {
            let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), false).unwrap();
            answer["loaded"][0]["fields"][field].as_array().unwrap().clone()
        };
        let slides = load(&mut exchange, &mut doc, "root", "slides");
        assert_eq!(slides.len(), 1);
        let shapes = load(&mut exchange, &mut doc, slides[0].as_str().unwrap(), "shapes");
        let shape = shapes[0].as_str().unwrap().to_owned();

        let ops = json!({ "ops": [
            { "op": "set", "target": shape, "field": "name", "value": "Scripted" },
            { "op": "command", "id": "slide.new", "params": { "layout": "blank" } },
        ] });
        let refused = exchange.batch(&mut doc, &ops, true).unwrap_err();
        assert_eq!(refused.code, ModelErrorCode::ReadOnly);
        exchange.batch(&mut doc, &ops, false).unwrap();

        // A fresh exchange starts from the engine: the edit and the command are the engine's now.
        let mut fresh = Exchange::new();
        assert_eq!(load(&mut fresh, &mut doc, "root", "slides").len(), 2);
        let named = fresh.batch(&mut doc, &json!({ "ops": [], "load": [{ "target": shape, "fields": ["name"] }] }), false).unwrap();
        assert_eq!(named["loaded"][0]["fields"]["name"], "Scripted");
        assert!(doc.dirty());
    }
}
