//! Which blobs a [`crate::SlideSession`] still needs, so the rest can be dropped (copied from
//! Kition Photo's `kdm-model`, same owner and licence; only [`object_blobs`] is Slide's own).
//!
//! A blob is kept while the replica's current state references it, while an undo or redo step
//! this session can still take might bring it back, or while it waits to be imported (blobs a
//! peer sent before its changes). Everything else (the bytes of a picture deleted and its undo
//! step trimmed from history) is garbage: without this a long session keeps every picture, video
//! and sound it ever held.
//!
//! The current state's references are counted per object, updated from the objects each
//! transaction, undo, redo or import changed, so nothing rescans the whole document. Each undo
//! step keeps the blobs its transaction displaced (to undo it) and added (to redo it after an
//! undo, even if a collaborator overwrote them meanwhile); a redo step keeps the same for the undo
//! that made it.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use kition_kit::kdm::{ObjectId, Replica, UndoScope};
use kition_kit::session::MemoryBlobs;
use serde_json::Value;

/// The blob hashes one replica operation stopped and started referencing.
#[derive(Default)]
pub(crate) struct Moved {
    pub displaced: HashSet<String>,
    pub added: HashSet<String>,
}

impl Moved {
    fn all(self) -> HashSet<String> {
        let mut all = self.displaced;
        all.extend(self.added);
        all
    }
}

#[derive(Default)]
pub(crate) struct BlobIndex {
    by_object: HashMap<ObjectId, Vec<String>>,
    refs: HashMap<String, u32>,
    /// What each undo step can bring back, with its undo scope (KitModel M12a), oldest first.
    undo: VecDeque<(UndoScope, HashSet<String>)>,
    /// What each redo step can bring back, with its scope, the next redo last.
    redo: Vec<(UndoScope, HashSet<String>)>,
    /// Blobs added for changes not imported yet.
    pending: HashSet<String>,
}

/// Every `{"hash": …, "mime": …}` (a blob reference) inside `value`.
fn collect(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let (Some(Value::String(hash)), Some(Value::String(_))) = (map.get("hash"), map.get("mime")) {
                out.push(hash.clone());
            }
            for item in map.values() {
                collect(item, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, out)),
        _ => {}
    }
}

/// The blobs object `id` references (none if it no longer exists): a `Media` object's `blob`,
/// the only blob field of `kit.slide`'s schema.
fn object_blobs(replica: &Replica, id: &ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    if replica.exists(id)
        && matches!(replica.object_type(id).as_deref(), Ok("Media"))
        && let Ok(value) = replica.get(id, "blob")
    {
        collect(&value, &mut out);
    }
    out
}

impl BlobIndex {
    /// Indexes every object of a freshly loaded or created replica.
    pub fn build(replica: &Replica) -> Self {
        let mut index = Self::default();
        let all: BTreeSet<ObjectId> = replica.objects_of_type("Media").into_iter().collect();
        index.update(replica, &all);
        index
    }

    /// Re-reads the references of the `changed` objects.
    ///
    /// An object's field can list the same hash more than once (two tiles sharing one default
    /// pixel): `before` and `now` are counted as multisets, and only a hash's *net* change in
    /// how many times this one object lists it touches `refs`. Without that, a hash an object
    /// keeps listing (one tile painted while another of the same object is untouched) would
    /// drop to a transient zero and look "gained" again from nothing, even though nothing about
    /// it changed; that falsely marked it `Moved::added` for every later change to the object,
    /// not just the one that actually first added it.
    pub fn update(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>) -> Moved {
        let mut dropped = Vec::new();
        let mut gained = Vec::new();
        for id in changed {
            let now = object_blobs(replica, id);
            let before = if now.is_empty() { self.by_object.remove(id) } else { self.by_object.insert(id.clone(), now.clone()) }.unwrap_or_default();
            let mut before_counts: HashMap<&str, i64> = HashMap::new();
            for hash in &before {
                *before_counts.entry(hash.as_str()).or_insert(0) += 1;
            }
            let mut now_counts: HashMap<&str, i64> = HashMap::new();
            for hash in &now {
                *now_counts.entry(hash.as_str()).or_insert(0) += 1;
            }
            let mut hashes: BTreeSet<&str> = before_counts.keys().copied().collect();
            hashes.extend(now_counts.keys().copied());
            for hash in hashes {
                let delta = now_counts.get(hash).copied().unwrap_or(0) - before_counts.get(hash).copied().unwrap_or(0);
                if delta > 0 {
                    let count = self.refs.entry(hash.to_owned()).or_insert(0);
                    if *count == 0 {
                        gained.push(hash.to_owned());
                    }
                    *count += u32::try_from(delta).unwrap_or(u32::MAX);
                } else if delta < 0
                    && let Some(count) = self.refs.get_mut(hash)
                {
                    *count = count.saturating_sub(u32::try_from(-delta).unwrap_or(u32::MAX));
                    if *count == 0 {
                        self.refs.remove(hash);
                        dropped.push(hash.to_owned());
                    }
                }
            }
        }
        let mut moved = Moved::default();
        for hash in dropped {
            if !self.refs.contains_key(&hash) {
                moved.displaced.insert(hash);
            }
        }
        for hash in gained {
            if !moved.displaced.remove(&hash) {
                moved.added.insert(hash);
            }
        }
        moved
    }

    /// A transaction of `scope` that became a new undo step: it clears that scope's redo steps,
    /// as the replica does; the other scope's stay (M12a).
    pub fn push_step(&mut self, scope: UndoScope, moved: Moved) {
        self.undo.push_back((scope, moved.all()));
        self.redo.retain(|(of, _)| *of != scope);
    }

    /// An undo (`undo`) or a redo of `scope` took `taken` steps off the replica's stack: the steps
    /// it passed over (their objects gone since) and, when `done`, the one it took back. They leave
    /// this stack too; with one taken back, they become that scope's one next step on the other
    /// stack, keeping what any of them can bring back and what the call moved.
    pub fn stepped(&mut self, scope: UndoScope, undo: bool, taken: usize, done: bool, moved: Moved) {
        let mut kept = moved.all();
        for _ in 0..taken {
            let from_undo = if undo { self.undo.iter().rposition(|(of, _)| *of == scope) } else { None };
            let from_redo = if undo { None } else { self.redo.iter().rposition(|(of, _)| *of == scope) };
            let step = match (from_undo, from_redo) {
                (Some(i), _) => self.undo.remove(i).map(|(_, blobs)| blobs),
                (_, Some(i)) => Some(self.redo.remove(i).1),
                (None, None) => None,
            };
            match step {
                Some(blobs) => kept.extend(blobs),
                None => break,
            }
        }
        if !done {
            return;
        }
        if undo {
            self.redo.push((scope, kept));
        } else {
            self.undo.push_back((scope, kept));
        }
    }

    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_steps(&self) -> usize {
        self.redo.len()
    }

    pub fn undo_steps_in(&self, scope: UndoScope) -> usize {
        self.undo.iter().filter(|(of, _)| *of == scope).count()
    }

    pub fn redo_steps_in(&self, scope: UndoScope) -> usize {
        self.redo.iter().filter(|(of, _)| *of == scope).count()
    }

    /// Every hash the current state references (not the extra copies undo, redo or a pending
    /// import keep around): what a peer starting from nothing needs.
    pub fn referenced(&self) -> Vec<String> {
        self.refs.keys().cloned().collect()
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps: the older ones
    /// can no longer be taken, so their blobs can go.
    pub fn retain(&mut self, undo: usize, redo: usize) {
        for scope in [UndoScope::Person, UndoScope::Agent] {
            self.retain_in(scope, undo, redo);
        }
    }

    /// [`BlobIndex::retain`] for one scope's steps, the other scope's untouched.
    pub fn retain_in(&mut self, scope: UndoScope, undo: usize, redo: usize) {
        let mut extra = self.undo_steps_in(scope).saturating_sub(undo);
        self.undo.retain(|(of, _)| {
            let drop = *of == scope && extra > 0;
            extra -= usize::from(drop);
            !drop
        });
        let mut extra = self.redo_steps_in(scope).saturating_sub(redo);
        self.redo.retain(|(of, _)| {
            let drop = *of == scope && extra > 0;
            extra -= usize::from(drop);
            !drop
        });
    }

    pub fn protect(&mut self, hashes: impl IntoIterator<Item = String>) {
        self.pending.extend(hashes);
    }

    pub fn imported(&mut self) {
        self.pending.clear();
    }

    /// Drops every blob nothing above needs.
    pub fn collect_garbage(&self, blobs: &mut MemoryBlobs) {
        blobs.0.retain(|hash, _| {
            self.refs.contains_key(hash)
                || self.pending.contains(hash)
                || self.undo.iter().any(|(_, s)| s.contains(hash))
                || self.redo.iter().any(|(_, s)| s.contains(hash))
        });
    }
}
