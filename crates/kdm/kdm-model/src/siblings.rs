//! The read repair: what a read shows that the replica should hold, planned from what the read
//! found (the view's [`Ids`]: the paragraphs a body lists, the comments a thread lists) and so
//! written by the session after the read, not in [`kition_kit::session::KitModel::validate`],
//! which sees only the transaction (a transaction reads text, `Tx::text` and `Tx::text_delta`,
//! but not the view's ids). After an import, undo or redo and when the session opens a replica it
//! is a `system` transaction (not an undo step); after a merged draft or a restore it joins that
//! undo step (`Replica::transact_in_step`), so one undo takes the merge and the repair back
//! together. Each is deterministic (the same state plans the same writes) and idempotent (writing
//! it again changes nothing).
//!
//! - **Paragraphs listed twice** in one body (a replica stored that way) are listed once, at their
//!   first place.
//! - Comments listed twice in their thread: `crate::comments::repair`.

use std::collections::HashSet;

use kition_kit::kdm::{ObjectId, Replica, Tx};

use crate::KdmResult;
use crate::ids::Ids;
use crate::list::sync_list;

/// One write of the repair.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fix {
    /// The body lists exactly these paragraphs.
    List { body: ObjectId, paragraphs: Vec<ObjectId> },
}

/// A body's live paragraphs, each once.
fn live(replica: &Replica, body: &ObjectId) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    replica.list(body, "paragraphs").unwrap_or_default().into_iter().filter(|p| replica.exists(p) && seen.insert(p.clone())).collect()
}

/// The repair `replica` needs, given the view's `ids` (read from it).
pub(crate) fn plan(replica: &Replica, ids: &Ids) -> Vec<Fix> {
    let mut fixes = Vec::new();
    // Bodies whose list holds an entry twice (or a dead one) are listed anew.
    for body in ids.bodies.values() {
        let all = replica.list(&body.body, "paragraphs").unwrap_or_default();
        if all.len() != body.paragraphs.len() {
            let mut seen = HashSet::new();
            let dupes = all.iter().any(|p| replica.exists(p) && !seen.insert(p));
            if dupes {
                fixes.push(Fix::List { body: body.body.clone(), paragraphs: live(replica, &body.body) });
            }
        }
    }
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
        }
    }
    Ok(wrote)
}
