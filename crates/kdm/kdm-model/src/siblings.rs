//! The read repair: what a read shows that the replica should hold, written by the session in a
//! `system` transaction (not an undo step) after an import, undo or redo and when it opens a
//! replica. A transaction cannot read text fields, so these repairs cannot be part of
//! [`kition_kit::session::KitModel::validate`]; each is deterministic (the same state plans the
//! same writes) and idempotent (two replicas writing it at once converge to the same view).
//!
//! - **Sibling bodies.** Two replicas that migrate one 0.3.x text body at once (both changed it
//!   offline, [`crate::migrate`]) each write a body of their own; the holder's reference keeps one
//!   (the *kept* body), and the other (the *sibling*) still holds the other person's paragraphs.
//!   Their paragraphs are merged into the kept body by comparing both with the text the migration
//!   started from (`migratedFrom`, recorded in each body's `props`; paragraph `<body>-p<index>`
//!   started as its `index`-th paragraph): a paragraph only the sibling changed replaces the kept
//!   one, one only the kept body changed stays, one both changed is kept twice (both versions), a
//!   paragraph the sibling added is listed after its neighbour, one the sibling deleted and the
//!   kept body left unchanged is deleted. The sibling body is then deleted. Paragraphs move by
//!   reference, so typing in them carries on wherever they are listed.
//! - **Paragraphs listed twice** in one body (two replicas merged the same sibling at once) are
//!   listed once, at their first place.
//!
//! An edit made to a sibling's paragraph list in the moment it is merged (a paragraph added there)
//! can be lost, as an edit of a deleted object is.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Replica, TextRun, Tx};
use serde_json::Value;

use crate::KdmResult;
use crate::ids::Ids;
use crate::list::sync_list;
use crate::pieces::canon;
use crate::write::MIGRATED_FROM;

/// One write of the repair.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fix {
    /// The body lists exactly these paragraphs.
    List {
        body: ObjectId,
        paragraphs: Vec<ObjectId>,
    },
    Delete(ObjectId),
}

/// A paragraph's content: its `props` register and its field's runs.
type Content = (Value, Vec<TextRun>);

/// Whether `id` names a body a migration made (`<holder>-text`, `-notes` or `-cell`, then
/// optionally `:<uuid>`), and the holder part.
fn migration_key(id: &str) -> Option<&str> {
    let key = id.rsplit_once(':').map_or(id, |(key, _)| key);
    (key.ends_with("-text") || key.ends_with("-notes") || key.ends_with("-cell")).then_some(key)
}

/// The base paragraph index of paragraph `paragraph` of body `body` (`<body>-p<index>`).
fn base_index(body: &ObjectId, paragraph: &ObjectId) -> Option<usize> {
    paragraph.as_str().strip_prefix(body.as_str())?.strip_prefix("-p")?.parse().ok()
}

fn content(replica: &Replica, paragraph: &ObjectId) -> Content {
    (replica.get(paragraph, "props").unwrap_or(Value::Null), canon(replica.text_delta(paragraph, "text").unwrap_or_default()))
}

/// The base a migration recorded in body `body`'s `props`.
fn base_of(replica: &Replica, body: &ObjectId) -> Option<Vec<Content>> {
    let props = replica.get(body, "props").ok()?;
    let list = props.get(MIGRATED_FROM)?.as_array()?;
    list.iter()
        .map(|p| {
            let runs: Vec<TextRun> = serde_json::from_value(p.get("runs")?.clone()).ok()?;
            Some((p.get("props").cloned().unwrap_or(Value::Null), canon(runs)))
        })
        .collect()
}

/// A body's live paragraphs, each once.
fn live(replica: &Replica, body: &ObjectId) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    replica.list(body, "paragraphs").unwrap_or_default().into_iter().filter(|p| replica.exists(p) && seen.insert(p.clone())).collect()
}

/// The repair `replica` needs, given the view's `ids` (read from it). `changed`: what the last
/// import, undo or redo touched; `None` (opening a replica) checks everything.
pub(crate) fn plan(replica: &Replica, ids: &Ids, changed: Option<&BTreeSet<ObjectId>>) -> Vec<Fix> {
    let mut fixes = Vec::new();
    // Bodies whose list holds an entry twice (or a dead one) are listed anew.
    let mut listed: HashMap<&ObjectId, &[ObjectId]> = HashMap::new();
    for body in ids.bodies.values() {
        listed.insert(&body.body, &body.paragraphs);
        let all = replica.list(&body.body, "paragraphs").unwrap_or_default();
        if all.len() != body.paragraphs.len() {
            let mut seen = HashSet::new();
            let dupes = all.iter().any(|p| replica.exists(p) && !seen.insert(p));
            if dupes {
                fixes.push(Fix::List { body: body.body.clone(), paragraphs: live(replica, &body.body) });
            }
        }
    }
    let worth = changed.is_none_or(|changed| changed.iter().any(|o| migration_key(o.as_str()).is_some() || o.as_str().contains("-text:")));
    if !worth {
        return fixes;
    }
    // Migrated bodies by holder.
    let mut groups: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    for body in replica.objects_of_type("TextBody") {
        if let Some(key) = migration_key(body.as_str()) {
            groups.entry(key.to_owned()).or_default().push(body);
        }
    }
    for (_, mut bodies) in groups {
        if bodies.len() < 2 {
            continue;
        }
        bodies.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let Some(kept) = bodies.iter().find(|b| listed.contains_key(b)).cloned() else { continue };
        let siblings: Vec<ObjectId> = bodies.into_iter().filter(|b| *b != kept).collect();
        fixes.extend(merge(replica, &kept, &siblings));
    }
    fixes
}

/// Merges `siblings`' paragraphs into `kept` (see the module docs).
fn merge(replica: &Replica, kept: &ObjectId, siblings: &[ObjectId]) -> Vec<Fix> {
    let mut contents: HashMap<ObjectId, Content> = HashMap::new();
    let mut content_of = |p: &ObjectId| contents.entry(p.clone()).or_insert_with(|| content(replica, p)).clone();
    let base = std::iter::once(kept).chain(siblings).find_map(|b| base_of(replica, b));
    let mut list = live(replica, kept);
    let before: Vec<ObjectId> = list.clone();
    let mut dropped: Vec<ObjectId> = Vec::new();
    for sibling in siblings {
        let theirs = live(replica, sibling);
        // Where the next paragraph the sibling added goes: after the last one placed.
        let mut after: Option<usize> = None;
        for x in &theirs {
            if let Some(at) = list.iter().position(|p| p == x) {
                after = Some(at);
                continue;
            }
            let Some(j) = base_index(sibling, x) else {
                let at = after.map_or(0, |a| a + 1);
                list.insert(at, x.clone());
                after = Some(at);
                continue;
            };
            let x_content = content_of(x);
            let base_j = base.as_ref().and_then(|b| b.get(j));
            let ours = list.iter().position(|p| base_index(kept, p) == Some(j));
            if base_j == Some(&x_content) {
                // They left it as it was.
                if ours.is_some() {
                    after = ours;
                }
                dropped.push(x.clone());
                continue;
            }
            match ours {
                Some(at) => {
                    let ours_content = content_of(&list[at]);
                    if ours_content == x_content {
                        after = Some(at);
                        dropped.push(x.clone());
                    } else if base_j == Some(&ours_content) {
                        // Only they changed it.
                        dropped.push(std::mem::replace(&mut list[at], x.clone()));
                        after = Some(at);
                    } else {
                        // Both changed it: both versions stay.
                        list.insert(at + 1, x.clone());
                        after = Some(at + 1);
                    }
                }
                None => {
                    // We deleted it, they changed it: theirs stays.
                    let at = after.map_or(0, |a| a + 1);
                    list.insert(at, x.clone());
                    after = Some(at);
                }
            }
        }
        // They deleted a paragraph we left as it was.
        if let Some(base) = &base {
            let theirs_has: HashSet<usize> = theirs.iter().filter_map(|x| base_index(sibling, x)).collect();
            for (j, base_j) in base.iter().enumerate() {
                if theirs_has.contains(&j) {
                    continue;
                }
                if let Some(at) = list.iter().position(|p| base_index(kept, p) == Some(j))
                    && content_of(&list[at]) == *base_j
                {
                    dropped.push(list.remove(at));
                }
            }
        }
    }
    let mut fixes = Vec::new();
    if list != before {
        fixes.push(Fix::List { body: kept.clone(), paragraphs: list.clone() });
    }
    let listed: HashSet<&ObjectId> = list.iter().collect();
    for p in dropped {
        if !listed.contains(&p) {
            fixes.push(Fix::Delete(p));
        }
    }
    fixes.extend(siblings.iter().cloned().map(Fix::Delete));
    fixes
}

/// Writes `fixes`. Answers whether it wrote anything.
pub(crate) fn apply(tx: &mut Tx<'_>, fixes: &[Fix]) -> KdmResult<bool> {
    let mut wrote = false;
    for fix in fixes {
        match fix {
            Fix::List { body, paragraphs } => {
                if tx.exists(body) {
                    let alive: Vec<ObjectId> = paragraphs.iter().filter(|p| tx.exists(p)).cloned().collect();
                    wrote |= sync_list(tx, body, "paragraphs", &alive)? > 0;
                }
            }
            Fix::Delete(object) => {
                if tx.exists(object) {
                    tx.delete(object)?;
                    wrote = true;
                }
            }
        }
    }
    Ok(wrote)
}
