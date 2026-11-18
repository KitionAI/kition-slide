//! The command bridge to each slide's KDM replica (Kition phase 3d, Task 2), the same
//! design as Kition Film's and Photo's.
//!
//! A document's replica ([`kdm_model::SlideSession`]) is its shared source of truth;
//! [`DocState::doc`] is the engine's own view of it. Commands, pointer gestures and keys run
//! exactly as before on the in-memory [`SlideFile`]; at the end of every top-level call
//! ([`Session::execute`], [`Session::pointer`], [`Session::key`], [`Session::type_text`]) the
//! bridge writes what changed since the replica last saw it ([`Link::synced`]) as one transaction
//! per new history step, found by [`crate::History::pushed`] (a counter, not the stack's length:
//! at the limit a push drops the oldest step). So nested commands, `fit_text_box`'s in-place
//! refit, `connect::reroute`, the panic guard's rollback and a revert are all replicated, each in
//! the transaction of the step they belong to. A live pointer interaction is written when it
//! commits, as the one step it is (an import in the middle of it writes it so far first; the
//! step then undoes every transaction it took).
//!
//! Undo is per person: each history step remembers how many replica undo steps it took
//! ([`crate::HistoryEntry::steps`]); undo and redo step the replica that many times and take the
//! replica's result (with every collaborator's edit since) as the document, not the engine's own
//! snapshot. A step the replica holds nothing of (`steps == 0`) leaves the document at the
//! replica's view. [`crate::Selection`], the clipboard, painters, tools, zoom and `media_status`
//! are this person's own: never written; after an import, undo or redo the selection follows its
//! slide by id and drops what vanished ([`follow_selection`]).
//!
//! A replica that refuses a write or an undo is left ([`Session::kdm_resets`]): the document keeps
//! working from its own history snapshots, and the next edit makes a fresh replica.

use std::sync::Arc;

use kdm_model::SlideSession;
use kdm_model::kition_kit::kdm::{Origin, OriginKind, UndoScope, Version};
use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
use kition_slide_model::{ShapeKind, SlideFile};

use crate::{DocState, EngineError, Result, Selection, Session, Target};

/// The origin of the local person's commands (used when nothing else was set with
/// [`Session::execute_as`]/[`Session::set_origin`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn kdm_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Other(format!("slide replica: {error}"))
}

fn no_replica() -> EngineError {
    EngineError::Other("the slide has no replica and none could be made".into())
}

/// One document's bridge state.
#[derive(Debug)]
pub struct Link {
    /// The replica, made lazily (the first edit or passthrough); `None` again after a reset.
    pub(crate) kdm: Option<SlideSession>,
    /// The document as last written to (or read from) the replica.
    pub(crate) synced: Arc<SlideFile>,
    pub(crate) resets: u64,
    /// [`crate::History::pushed`] when the bridge last looked.
    pub(crate) mark: u64,
    /// Replica steps written for a live interaction before it committed (an import in the middle
    /// of it): they belong to the step the interaction pushes.
    pub(crate) pending: u32,
    /// The person may only comment ([`Session::set_comment_only`]): writes go through
    /// [`SlideSession::set_comment_only`].
    pub(crate) comment_only: bool,
}

impl Link {
    pub(crate) fn new(doc: &Arc<SlideFile>) -> Self {
        Self { kdm: None, synced: doc.clone(), resets: 0, mark: 0, pending: 0, comment_only: false }
    }
}

impl DocState {
    /// The document's replica, if it has made one.
    pub fn kdm(&self) -> Option<&SlideSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the document left its replica (a write or undo the replica refused).
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// Writes what the document changed since the replica last saw it: one transaction per
    /// history step pushed since the last look; a change that pushed none belongs to the top step
    /// (a `system` one right after an undo, so the redo steps survive). A live interaction is left
    /// for its commit unless `force` (an import must not lose it).
    pub(crate) fn sync_kdm(&mut self, origin: &Origin, force: bool) {
        // Comments copied with a slide (or made without one) get their own ids before they are
        // written, so each is its own object (the write falls back to ids derived from content).
        if !self.doc.comment_ids_complete() {
            Arc::make_mut(&mut self.doc).fill_comment_ids(&mut crate::cmd::review::new_comment_id);
        }
        // What the replica takes is what the file stores (C8-slide-5): commands settle what they
        // change; this catches a change made outside `Session::edit` (a tool's gesture).
        if !Arc::ptr_eq(&self.link.synced, &self.doc) && self.interaction.is_none() {
            let mut doc = (*self.doc).clone();
            if crate::stored::settle(&mut doc, Some(&self.link.synced)) {
                self.doc = Arc::new(doc);
            }
        }
        let pushed = self.history.pushed_since(self.link.mark);
        self.link.mark = self.history.pushed;
        if pushed.is_empty() && self.link.pending == 0 && Arc::ptr_eq(&self.link.synced, &self.doc) {
            return;
        }
        if pushed.is_empty() {
            if self.interaction.is_some() {
                if force && !Arc::ptr_eq(&self.link.synced, &self.doc) {
                    let after = self.doc.clone();
                    let steps = self.record_kdm(origin, &after).unwrap_or(0);
                    self.link.pending = self.link.pending.saturating_add(steps);
                }
                return;
            }
            let mut steps = std::mem::take(&mut self.link.pending);
            if !Arc::ptr_eq(&self.link.synced, &self.doc) {
                let origin = if self.history.redo.is_empty() { origin.clone() } else { Origin::system("view") };
                let after = self.doc.clone();
                steps = steps.saturating_add(self.record_kdm(&origin, &after).unwrap_or(0));
            }
            if steps > 0
                && let Some(top) = self.history.undo.last_mut()
            {
                top.steps = top.steps.saturating_add(steps);
            }
        } else {
            let last = pushed.end.saturating_sub(1);
            // The steps are the scope of who ran them (a document opened during the call missed
            // [`Session::set_origin`]).
            let agent = origin.kind == OriginKind::Agent;
            for e in &mut self.history.undo[pushed.clone()] {
                e.agent = agent;
            }
            for i in pushed {
                let after = self.history.undo.get(i + 1).map(|e| e.doc.clone()).unwrap_or_else(|| self.doc.clone());
                let mut steps = if i == last { std::mem::take(&mut self.link.pending) } else { 0 };
                if !Arc::ptr_eq(&self.link.synced, &after) {
                    match self.record_kdm(origin, &after) {
                        Some(n) => steps = steps.saturating_add(n),
                        None => break,
                    }
                }
                if let Some(e) = self.history.undo.get_mut(i) {
                    e.steps = steps;
                }
            }
        }
        self.trim_kdm();
    }

    /// Writes `after` on top of the replica (made now, baselined on [`Link::synced`], if there is
    /// none) and answers the undo steps that took; `None` when the replica refused and was left.
    fn record_kdm(&mut self, origin: &Origin, after: &Arc<SlideFile>) -> Option<u32> {
        if self.link.kdm.is_none() {
            match SlideSession::create(&self.link.synced) {
                Ok(kdm) => self.link.kdm = Some(kdm),
                Err(error) => {
                    log::warn!("the slide can't be held in a replica: {error}");
                    self.link.synced = after.clone();
                    return Some(0);
                }
            }
        }
        let kdm = self.link.kdm.as_mut()?;
        kdm.set_comment_only(self.link.comment_only);
        match kdm.record(origin, after) {
            Ok(steps) => {
                self.link.synced = after.clone();
                Some(steps)
            }
            Err(error) => {
                log::warn!("the slide replica refused an edit: {error}");
                self.detach_kdm();
                None
            }
        }
    }

    /// The replica, made now from the current document if there is none yet (anything not
    /// written yet is written first; a live interaction stays for its commit).
    pub(crate) fn ensure_kdm(&mut self, origin: &Origin, force: bool) -> Option<&mut SlideSession> {
        self.sync_kdm(origin, force);
        if self.link.kdm.is_none() {
            match SlideSession::create(&self.doc) {
                Ok(kdm) => {
                    self.link.kdm = Some(kdm);
                    self.link.synced = self.doc.clone();
                }
                Err(_) => return None,
            }
        }
        self.link.kdm.as_mut()
    }

    /// Leaves the replica: every step's replica steps were the dropped replica's, and a fresh
    /// one never holds them.
    fn detach_kdm(&mut self) {
        self.link.kdm = None;
        self.link.resets = self.link.resets.saturating_add(1);
        self.link.synced = self.doc.clone();
        self.link.pending = 0;
        for e in self.history.undo.iter_mut().chain(self.history.redo.iter_mut()) {
            e.steps = 0;
        }
    }

    /// Lets the replica forget the steps the history can no longer take, and their blobs.
    fn trim_kdm(&mut self) {
        let sum = |v: &[crate::HistoryEntry], agent: bool| v.iter().filter(|e| e.agent == agent).map(|e| e.steps as usize).sum::<usize>();
        // Changes not written yet belong to the step on top, and so to its scope.
        let pending_agent = self.history.undo.last().is_some_and(|e| e.agent);
        for (agent, scope) in [(false, UndoScope::Person), (true, UndoScope::Agent)] {
            let pending = if agent == pending_agent { self.link.pending as usize } else { 0 };
            let undo = sum(&self.history.undo, agent).saturating_add(pending);
            let redo = sum(&self.history.redo, agent);
            if let Some(kdm) = self.link.kdm.as_mut() {
                kdm.retain_steps_in(scope, undo, redo);
            }
        }
    }

    /// Whether the document is held in a replica now (undo of a step not on top needs one).
    pub(crate) fn link_holds_replica(&self) -> bool {
        self.link.kdm.is_some()
    }

    /// After undo or redo set the document to the step's own snapshot: steps the replica too and
    /// takes its view (with every collaborator's edit since) instead. `floor` is the document's
    /// id counter before, which the view never lowers.
    pub(crate) fn step_kdm(&mut self, steps: u32, undo: bool, floor: u32, agent: bool) {
        let scope = if agent { UndoScope::Agent } else { UndoScope::Person };
        if steps > 0
            && let Some(kdm) = self.link.kdm.as_mut()
            && !(0..steps).all(|_| matches!(if undo { kdm.undo_in(scope) } else { kdm.redo_in(scope) }, Ok((true, _))))
        {
            self.detach_kdm();
            self.link.mark = self.history.pushed;
            return;
        }
        if let Some(kdm) = self.link.kdm.as_ref() {
            self.doc = view_of(kdm, floor);
        }
        self.link.synced = self.doc.clone();
        self.repair_autofit(None);
        self.link.mark = self.history.pushed;
    }

    /// Persist merged text reflow as a non-undoable replica transaction. Read-only replicas
    /// retain their state; the layout layer validates receipts for their derived rendering.
    fn repair_autofit(&mut self, before: Option<&SlideFile>) {
        if self.link.comment_only {
            return;
        }
        let mut repaired = (*self.doc).clone();
        // Values others wrote (an agent's object-model batch) as the file stores them (C8-slide-5),
        // for what changed since `before`.
        let settled = before.is_some_and(|before| crate::stored::settle(&mut repaired, Some(before)));
        if (crate::autofit::repair(&mut repaired) | settled)
            && self.record_kdm(&Origin::system("autofit"), &Arc::new(repaired)).is_some()
            && let Some(kdm) = self.link.kdm.as_ref()
        {
            self.doc = view_of(kdm, self.doc.next_id);
            self.link.synced = self.doc.clone();
        }
    }

    /// After an import: the document follows the replica, and the selection follows it.
    fn follow_kdm(&mut self) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let before = self.doc.clone();
        self.doc = view_of(kdm, before.next_id);
        self.link.synced = self.doc.clone();
        self.repair_autofit(Some(&*before));
        let mut selection = std::mem::take(&mut self.selection);
        follow_selection(&before, &mut selection, &self.doc);
        self.selection = selection;
        crate::clamp_selection(self);
        self.revision += 1;
    }
}

/// Whether every shape number `fresh` shares with `live` (on the same slide, master or layout)
/// names the same shape identity, and every identity both hold has the same number.
fn same_numbers(live: &SlideFile, fresh: &SlideFile) -> bool {
    use std::collections::HashMap;
    fn numbers(shapes: &[kition_slide_model::Shape]) -> (HashMap<u32, String>, HashMap<String, u32>) {
        let (mut by_number, mut by_identity) = (HashMap::new(), HashMap::new());
        kition_slide_model::walk(shapes, &mut |shape, _| {
            by_number.insert(shape.id.0, shape.identity.clone());
            if !shape.identity.is_empty() {
                by_identity.insert(shape.identity.clone(), shape.id.0);
            }
        });
        (by_number, by_identity)
    }
    let agree = |a: &[kition_slide_model::Shape], b: &[kition_slide_model::Shape]| {
        let ((a_numbers, a_identities), (b_numbers, b_identities)) = (numbers(a), numbers(b));
        a_numbers.iter().all(|(number, identity)| b_numbers.get(number).is_none_or(|other| other == identity))
            && a_identities.iter().all(|(identity, number)| b_identities.get(identity).is_none_or(|other| other == number))
    };
    let slides = fresh.slides.iter().all(|slide| live.slides.iter().find(|s| s.id == slide.id).is_none_or(|old| agree(&old.shapes, &slide.shapes)));
    let masters = fresh.masters.iter().all(|master| {
        live.masters.iter().find(|m| m.id == master.id).is_none_or(|old| {
            agree(&old.shapes, &master.shapes)
                && master
                    .layouts
                    .iter()
                    .all(|layout| old.layouts.iter().find(|l| l.id == layout.id).is_none_or(|was| agree(&was.shapes, &layout.shapes)))
        })
    });
    let layout_numbers = fresh.masters.iter().flat_map(|master| &master.layouts).all(|layout| {
        live.masters
            .iter()
            .flat_map(|master| &master.layouts)
            .all(|old| (old.id != layout.id || old.identity == layout.identity) && (old.identity != layout.identity || old.id == layout.id))
    });
    slides && masters && layout_numbers
}

/// Keep session-local part numbers when persisted layout identities prove identical ownership
/// and order. A changed layout set/order remains an explicit history-reread limitation.
fn retained_part_numbers(live: &SlideFile, fresh: &SlideFile) -> Option<SlideFile> {
    if live.masters.len() != fresh.masters.len() {
        return None;
    }
    let mut copy = fresh.clone();
    let mut layouts = std::collections::HashMap::new();
    for (old, new) in live.masters.iter().zip(&mut copy.masters) {
        if old.layouts.len() != new.layouts.len()
            || !old.layouts.iter().zip(&new.layouts).all(|(old, new)| !old.identity.is_empty() && old.identity == new.identity)
        {
            return None;
        }
        let new = Arc::make_mut(new);
        new.id = old.id;
        for (old, new) in old.layouts.iter().zip(&mut new.layouts) {
            layouts.insert(new.id, old.id);
            new.id = old.id;
        }
    }
    for slide in &mut copy.slides {
        let id = *layouts.get(&slide.layout)?;
        if slide.layout != id {
            Arc::make_mut(slide).layout = id;
        }
    }
    copy.next_id = copy.next_id.max(live.next_id);
    Some(retained_media_numbers(live, copy))
}

/// Media has no persisted identity, so a reread numbers it afresh. Keep the session's number of
/// each item whose part name, type, link and bytes are unchanged (paired one to one, in order);
/// recording a renumbered item would recreate its object and report an untouched picture as
/// changed. Changed media keeps the fresh number and reads as the change it is.
fn retained_media_numbers(live: &SlideFile, fresh: SlideFile) -> SlideFile {
    let mut used = vec![false; live.media.len()];
    let mut map = std::collections::HashMap::new();
    for item in &fresh.media {
        let same = |(at, old): &(usize, &kition_slide_model::MediaItem)| {
            !used[*at]
                && old.name == item.name
                && old.content_type == item.content_type
                && old.link == item.link
                && (Arc::ptr_eq(&old.data, &item.data) || old.data == item.data)
        };
        if let Some((at, old)) = live.media.iter().enumerate().find(same) {
            used[at] = true;
            if old.id != item.id {
                map.insert(item.id, old.id);
            }
        }
    }
    let mut numbers: Vec<_> = fresh.media.iter().map(|item| map.get(&item.id).copied().unwrap_or(item.id)).collect();
    numbers.sort_unstable();
    if map.is_empty() || numbers.windows(2).any(|pair| pair[0] == pair[1]) {
        return fresh;
    }
    kdm_model::renumber_media(&fresh, &map).unwrap_or(fresh)
}

/// The replica's view as the document, with an id counter never below `floor`.
fn view_of(kdm: &SlideSession, floor: u32) -> Arc<SlideFile> {
    let mut p = kdm.slide_file().clone();
    p.next_id = p.next_id.max(floor);
    Arc::new(p)
}

/// Moves `sel` (made on `before`) onto `after`: the current slide and the edited master or
/// layout by id (a removed one: the slide now in its place, or back to the slides), the caret and
/// table cells clamped to what the text and table now hold. [`crate::clamp_selection`] then drops
/// the shapes and slides that are gone.
pub(crate) fn follow_selection(before: &SlideFile, sel: &mut Selection, after: &SlideFile) {
    if let Some(id) = before.slides.get(sel.slide).map(|s| s.id) {
        sel.slide = after.slides.iter().position(|s| s.id == id).unwrap_or(sel.slide);
    }
    if after.slides.get(sel.slide).is_none() {
        sel.slide = after.slides.len().saturating_sub(1);
    }
    let master_index = |m: usize| before.masters.get(m).and_then(|old| after.masters.iter().position(|x| x.id == old.id));
    sel.target = match sel.target {
        Target::Slides => Target::Slides,
        Target::Master { master } => master_index(master).map_or(Target::Slides, |master| Target::Master { master }),
        Target::Layout { master, layout } => {
            let id = before.masters.get(master).and_then(|m| m.layouts.get(layout)).map(|l| l.id);
            let found = after.masters.iter().enumerate().find_map(|(mi, m)| m.layouts.iter().position(|l| Some(l.id) == id).map(|li| (mi, li)));
            found.map_or(Target::Slides, |(master, layout)| Target::Layout { master, layout })
        }
    };
    if sel.target != Target::Slides {
        sel.slides.clear();
    }
}

/// Clamps the caret and the selected table cells to what the document now holds (after an
/// import, undo or redo a collaborator's edit may have shortened them).
pub(crate) fn clamp_text_and_cells(st: &mut DocState) {
    if let Some(t) = st.selection.text.clone()
        && let Some((r, c)) = t.cell
        && let Some(ShapeKind::Table(table)) = st.shape(t.shape).map(|sh| &sh.kind)
        && let Some(owner) = table.cell_owner(r, c)
        && let Some(text) = st.selection.text.as_mut()
    {
        text.cell = Some(owner);
    }
    if let Some(t) = st.selection.text.clone() {
        match crate::cmd::text::body_of(st, &t) {
            Some(body) => {
                let clamp = |(p, c): (usize, usize)| {
                    let last = body.paragraphs.len().saturating_sub(1);
                    let p = p.min(last);
                    let len = body.paragraphs.get(p).map_or(0, |x| x.text().chars().count());
                    (p, c.min(len))
                };
                let (anchor, caret) = (clamp(t.anchor), clamp(t.caret));
                if let Some(text) = st.selection.text.as_mut() {
                    text.anchor = anchor;
                    text.caret = caret;
                }
            }
            None if t.cell.is_some() => st.selection.text = None,
            None => {}
        }
    }
    if let Some((id, a, b)) = st.selection.cells {
        let fits = match st.shape(id).map(|s| &s.kind) {
            Some(ShapeKind::Table(t)) => [a, b].iter().all(|(r, c)| t.cell(*r, *c).is_some()),
            _ => false,
        };
        if !fits {
            st.selection.cells = None;
        }
    }
}

impl Session {
    /// Who the running (or next) command acts for: the origin of [`Session::execute_as`], or the
    /// local person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// Sets who the following commands act for (`None`: the local person); answers the previous
    /// setting, to put back.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Option<Origin> {
        let previous = std::mem::replace(&mut self.origin, origin);
        let agent = self.acts_as_agent();
        for st in &mut self.docs {
            st.history.agent = agent;
        }
        previous
    }

    /// Whether the running (or next) command is an agent's: its steps are the agent's undo scope.
    pub fn acts_as_agent(&self) -> bool {
        self.origin.as_ref().is_some_and(|origin| origin.kind == OriginKind::Agent)
    }

    /// [`Session::execute`] as `origin` (a person, an agent): the replica records the edit as
    /// theirs, and it undoes as this replica's own step.
    pub fn execute_as(&mut self, id: &str, params: &serde_json::Value, origin: &Origin) -> Result<serde_json::Value> {
        let previous = self.set_origin(Some(origin.clone()));
        let r = self.execute(id, params);
        self.set_origin(previous);
        r
    }

    /// After a top-level call: replicates every document's changes.
    pub(crate) fn sync_all_kdm(&mut self) {
        if self.depth != 0 {
            return;
        }
        let origin = self.origin();
        for st in &mut self.docs {
            st.sync_kdm(&origin, false);
        }
    }

    /// The principal of a person who may only comment, if this one may ([`Session::set_comment_only`]).
    pub fn comment_only(&self) -> Option<&str> {
        self.comment_only.as_deref()
    }

    /// Lets the person only comment (`Some(principal)`: the Kit UI protocol's `document.comments`
    /// for a commenter), or edit again (`None`): see [`Session::comment_only`].
    pub fn set_comment_only(&mut self, me: Option<String>) {
        let on = me.is_some();
        self.comment_only = me;
        for st in &mut self.docs {
            st.link.comment_only = on;
            if let Some(kdm) = st.link.kdm.as_mut() {
                kdm.set_comment_only(on);
            }
        }
    }

    /// The active document's replica, if it has made one (never makes one).
    pub fn kdm(&self) -> Option<&SlideSession> {
        self.active()?.kdm()
    }

    /// How many times the active document left its replica. Hosts watch this: the next edit's
    /// fresh replica is a new KDM document collaborators must join from its snapshot.
    pub fn kdm_resets(&self) -> u64 {
        self.active().map_or(0, DocState::kdm_resets)
    }

    /// The active document's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        Some(self.kdm()?.doc_id().to_string())
    }

    pub(crate) fn active_kdm(&mut self, force: bool) -> Result<&mut SlideSession> {
        let origin = self.origin();
        self.doc_mut()?.ensure_kdm(&origin, force).ok_or_else(no_replica)
    }

    /// The active document's replica version.
    pub fn version(&mut self) -> Result<Version> {
        Ok(self.active_kdm(false)?.version())
    }

    /// The active document's replica as a snapshot, for a peer ([`Session::open_snapshot`]).
    pub fn snapshot(&mut self) -> Result<Vec<u8>> {
        self.active_kdm(false)?.snapshot().map_err(kdm_error)
    }

    /// SHA-256 of the active document's replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self) -> Result<String> {
        self.active_kdm(false)?.state_hash().map_err(kdm_error)
    }

    /// The active document's changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, since: &Version) -> Result<Vec<u8>> {
        self.active_kdm(false)?.export_changes(since).map_err(kdm_error)
    }

    /// Every blob the active document's replica references or can still bring back.
    pub fn blobs(&mut self) -> Result<MemoryBlobs> {
        Ok(self.active_kdm(false)?.blobs().clone())
    }

    /// Adds a peer's blobs (before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<()> {
        if self.active_kdm(false)?.add_blobs(blobs).map_err(kdm_error)? {
            // Media that arrived after its changes: the document takes the reloaded view.
            self.doc_mut()?.follow_kdm();
        }
        Ok(())
    }

    /// The blobs a peer at `since` needs (`Version::empty()`: every blob the current state
    /// references), so a live change ships only its own media.
    pub fn blobs_since(&mut self, since: &Version) -> Result<MemoryBlobs> {
        Ok(MemoryBlobs(self.active_kdm(false)?.blobs_since(since).into_iter().collect()))
    }

    /// Imports a collaborator's (or stored) changes into the active document's replica (their
    /// blobs added first, [`Session::add_blobs`]). Local edits not written yet, a live drag
    /// included, are written first. The document and its selection follow the replica; one whose
    /// view could not be loaded from it leaves it (a reset) rather than diff against it later.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        let kdm = self.active_kdm(true)?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        let follow = result.is_ok() || (changed && kdm.view_loaded());
        let st = self.doc_mut()?;
        if follow {
            st.follow_kdm();
            clamp_text_and_cells(st);
        } else if changed {
            st.detach_kdm();
        }
        st.trim_kdm();
        result.map_err(kdm_error)
    }

    /// Reads the saved file again into the active document (kition-sdk session continuity on
    /// reread). `fresh` is the file as read now, `selection` the editor state the host restored
    /// on it.
    ///
    /// A document with no history takes the file as read, as opening it does: nothing is lost,
    /// and its objects get the identities the file gives them. One with history keeps it: what
    /// `fresh` holds is recorded onto the replica as one `import` transaction, outside both undo
    /// histories, and the document and its selection follow it, so undo still takes back this
    /// session's own steps and keeps the file's edit. That needs the file's numbers to name the
    /// shapes they named before (the replica pairs a shape by its number on its slide); after
    /// another program renumbered them (a shape's number now on another shape's identity), or in
    /// a comment-only session, `false`: nothing changed, and the host refuses the reread rather
    /// than drop the history. The document is clean afterwards.
    pub fn reread_file(&mut self, fresh: &SlideFile, selection: &Selection) -> Result<bool> {
        if self.comment_only.is_some() {
            return Ok(false);
        }
        let st = self.doc_mut()?;
        if st.interaction.is_some() {
            return Ok(false);
        }
        if st.history.undo.is_empty() && st.history.redo.is_empty() {
            let comment_only = st.link.comment_only;
            st.doc = Arc::new(fresh.clone());
            st.saved_doc = st.doc.clone();
            st.link = Link::new(&st.doc);
            st.link.comment_only = comment_only;
            st.selection = selection.clone();
            crate::clamp_selection(st);
            clamp_text_and_cells(st);
            st.revision += 1;
            return Ok(true);
        }
        let Some(fresh) = retained_part_numbers(&st.doc, fresh) else { return Ok(false) };
        if !same_numbers(&st.doc, &fresh) {
            return Ok(false);
        }
        let kdm = self.active_kdm(true)?;
        let before = kdm.version();
        kdm.record(&Origin::new(OriginKind::Import, "file reread"), &fresh).map_err(kdm_error)?;
        let changed = kdm.version() != before;
        let st = self.doc_mut()?;
        if changed {
            st.follow_kdm();
            clamp_text_and_cells(st);
        }
        st.saved_doc = st.doc.clone();
        st.trim_kdm();
        Ok(true)
    }

    /// Opens another replica's snapshot (with the blobs it references) as a new document, a
    /// replica of the same KDM document under a fresh actor, and makes it active: clean, with an
    /// empty history.
    pub fn open_snapshot(&mut self, bytes: &[u8], blobs: MemoryBlobs) -> Result<usize> {
        let kdm = SlideSession::open_snapshot(bytes, blobs).map_err(kdm_error)?;
        let title = self.next_untitled();
        let mut st = DocState::new(kdm.slide_file().clone(), None, title);
        st.link.kdm = Some(kdm);
        st.link.comment_only = self.comment_only.is_some();
        st.repair_autofit(None);
        st.saved_doc = st.doc.clone();
        Ok(self.add_document(st))
    }

    /// [`Session::open_snapshot`] for a host that keeps a document's replica beside its saved
    /// file. The file is what the person saved: when `file` is not empty and the replica does not
    /// hold its content (id-blind), the file wins, recorded onto the replica as one `import`
    /// transaction (not an undo step) under the replica's own identity, so collaborators get it as
    /// ordinary changes. The document opens clean either way.
    pub fn open_with_replica(&mut self, file: &[u8], snapshot: &[u8], blobs: MemoryBlobs) -> Result<usize> {
        let loaded = if file.is_empty() { None } else { Some(kdm_model::decode("", file).map_err(kdm_error)?) };
        let index = self.open_snapshot(snapshot, blobs)?;
        let st = self.doc_mut()?;
        if let Some(mut slide_file) = loaded {
            let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
            if !kdm.holds(&slide_file).map_err(kdm_error)? {
                kdm.record(&Origin::system("import"), &slide_file).map_err(kdm_error)?;
                slide_file.next_id = slide_file.next_id.max(st.doc.next_id);
                st.doc = Arc::new(slide_file);
                st.saved_doc = st.doc.clone();
                st.link.synced = st.doc.clone();
                st.selection = Selection::default();
            }
        }
        Ok(index)
    }
}
