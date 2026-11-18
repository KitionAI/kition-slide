//! Comment threads (Kition Kit Standard 2.0 rule M23): each slide comment is a `CommentThread`
//! object beside the slides, its comments `Comment` objects listed in the thread, so a change that
//! only comments touches only these two types (all a principal with the sync protocol's
//! `commenter` role may push) and two people replying to one thread at once both keep their reply.
//!
//! - A thread's `anchor` is `{object, slide, x, y}`: the shape's object when the comment is on a
//!   shape (else the slide's), the slide's object, and the point on the slide in points. Nothing
//!   in a slide points back at its threads.
//! - A `Comment` is `author` (the principal; a file's author name for a comment read from a file),
//!   `name` (the display name), `body`, `created`, and, as this kit's own optional fields, its
//!   `initials` and `props` (what PowerPoint kept of it, [`kition_slide_model::CommentKept`], and a
//!   reply's own point).
//! - The view's comment ids are the objects' ids (a top-level comment's `id` its thread's, `head`
//!   its own comment's), minted by the engine ([`kition_slide_model::SlideFile::fill_comment_ids`]).
//!   A view comment without a usable id (empty, or held by another comment) is written under an id
//!   derived from its content, the same in every write of that view.
//! - A 0.4.x replica kept a slide's comments as JSON in its `comments` register. They read as
//!   threads under keys derived from the slide and their place ([`legacy_id`]); the conversion
//!   ([`migrate`]) runs as a `system` transaction before a write, makes their objects under fresh
//!   ids tagged with those keys (their `legacy` field), clears the register and raises the minimum
//!   schema version to [`COMMENTS_SCHEMA_VERSION`], so a 0.4.x build refuses the document instead
//!   of showing it without its comments. Two replicas that convert one document offline both make
//!   a thread for each old comment (fresh ids: never one object two replicas made, whose later
//!   edits one side would lose); the read shows such siblings as one thread (every reply either
//!   side added, a resolve either side made), and the session's read repair ([`repair`]) merges
//!   them into one in the replica.
//! - A thread that goes (deleted, its slide deleted, merged into a sibling) keeps its object,
//!   listing no comment, so a reply someone adds to it at the same time is not lost: the thread
//!   reads again with it.

use std::collections::{BTreeMap, HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_slide_model::{Comment, CommentKept, ShapeId, SlideFile};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::ids::Ids;
use crate::list::sync_list;
use crate::{KdmResult, kdm_err};

pub(crate) const THREAD: &str = "CommentThread";
pub(crate) const COMMENT: &str = "Comment";

/// The schema version that moved comments out of the slides into `CommentThread` objects: the
/// minimum a converted document requires of its readers.
pub const COMMENTS_SCHEMA_VERSION: u32 = 3;

/// Whether `id` can name an object (`[A-Za-z0-9][A-Za-z0-9:_-]{0,127}`).
fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 128
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
}

fn fnv(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0xff)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

/// The id a 0.4.x replica's comment gets: thread `index` of slide `slide`, and its comment `at`
/// (0 the thread's first, then its replies).
pub(crate) fn legacy_id(slide: &ObjectId, index: usize, at: Option<usize>) -> String {
    let base = if slide.as_str().len() <= 96 && valid_id(slide.as_str()) {
        format!("{}:cm{index}", slide.as_str())
    } else {
        format!("cm{:016x}:{index}", fnv(&[slide.as_str()]))
    };
    match at {
        Some(at) => format!("{base}:{at}"),
        None => base,
    }
}

/// A comment's content, for ids derived from it.
fn content_key(c: &Comment) -> u64 {
    fnv(&[&c.author, &c.author_id, &c.date, &c.text, &c.x.to_bits().to_string(), &c.y.to_bits().to_string()])
}

fn unique(key: String, seen: &mut HashSet<String>) -> String {
    if seen.insert(key.clone()) {
        return key;
    }
    let mut n = 1u32;
    loop {
        let next = format!("{key}-{n}");
        if seen.insert(next.clone()) {
            return next;
        }
        n += 1;
    }
}

/// The keys (object ids, or ids derived from content) of a view's threads and comments.
#[derive(Clone, Debug, PartialEq)]
struct Keys {
    thread: String,
    /// The thread's first comment, then its replies.
    comments: Vec<String>,
}

fn keys_of(c: &Comment, slide: &ObjectId, seen: &mut HashSet<String>) -> Keys {
    let own = |id: &str, seen: &mut HashSet<String>| (valid_id(id) && seen.insert(id.to_owned())).then(|| id.to_owned());
    let thread = own(&c.id, seen).unwrap_or_else(|| unique(format!("c{:016x}{:016x}", fnv(&[slide.as_str()]), content_key(c)), seen));
    let head = own(&c.head, seen).unwrap_or_else(|| unique(format!("{thread}:h"), seen));
    let mut comments = vec![head];
    for r in &c.replies {
        comments.push(own(&r.id, seen).unwrap_or_else(|| unique(format!("{thread}:r{:016x}", content_key(r)), seen)));
    }
    Keys { thread, comments }
}

/// Every thread of `p` with its slide's object and keys, in slide order.
fn view_threads<'p>(p: &'p SlideFile, ids: &Ids) -> Vec<(ObjectId, &'p Comment, Keys)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for slide in &p.slides {
        let Some(object) = ids.slides.object(slide.id).cloned() else { continue };
        for c in &slide.comments {
            let keys = keys_of(c, &object, &mut seen);
            out.push((object.clone(), c, keys));
        }
    }
    out
}

/// A comment's own optional `props`: what PowerPoint kept, and a reply's point on the slide.
#[derive(Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct CommentProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    kept: Option<Box<CommentKept>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
}

/// A comment's register fields as written: `author` the principal (the display name when there is
/// none), `name` the display name.
fn comment_fields(c: &Comment, reply: bool) -> KdmResult<Map<String, Value>> {
    let mut fields = Map::new();
    let author = if c.author_id.is_empty() { c.author.clone() } else { c.author_id.clone() };
    fields.insert("author".into(), Value::String(author));
    fields.insert("name".into(), if c.author.is_empty() { Value::Null } else { Value::String(c.author.clone()) });
    fields.insert("created".into(), Value::String(c.date.clone()));
    fields.insert("initials".into(), if c.initials.is_empty() { Value::Null } else { Value::String(c.initials.clone()) });
    let props = CommentProps { kept: c.kept.clone(), x: (reply && c.x != 0.0).then_some(c.x), y: (reply && c.y != 0.0).then_some(c.y) };
    let props =
        if props.kept.is_none() && props.x.is_none() && props.y.is_none() { Value::Null } else { serde_json::to_value(props).map_err(kdm_err)? };
    fields.insert("props".into(), props);
    Ok(fields)
}

/// A thread's anchor: its shape's object (else its slide's), its slide's object, and its point.
fn anchor_of(c: &Comment, slide: &ObjectId, ids: &Ids) -> Value {
    let object = c.shape.and_then(|shape| ids.shape(slide, shape)).unwrap_or(slide);
    json!({ "object": object.as_str(), "slide": slide.as_str(), "x": c.x, "y": c.y })
}

/// Creates an object of `ty` under `key` when it can name one, else under a fresh id.
fn create(tx: &mut Tx<'_>, ty: &str, key: &str, fields: Map<String, Value>) -> KdmResult<ObjectId> {
    if valid_id(key)
        && let Ok(object) = tx.create_with_id(ty, &ObjectId::from_string(key), fields.clone())
    {
        return Ok(object);
    }
    tx.create(ty, fields)
}

fn non_null(fields: Map<String, Value>) -> Map<String, Value> {
    fields.into_iter().filter(|(_, v)| !v.is_null()).collect()
}

/// The object a key names now, if it still exists.
fn live(tx: &Tx<'_>, ids: &Ids, key: &str) -> Option<ObjectId> {
    ids.comments.get(key).filter(|object| tx.exists(object)).cloned()
}

/// Writes one comment (`before` what the view held of it, if its object exists); answers its
/// object and whether it wrote.
fn write_comment(tx: &mut Tx<'_>, ids: &mut Ids, key: &str, before: Option<&Comment>, after: &Comment, reply: bool) -> KdmResult<(ObjectId, bool)> {
    let fields = comment_fields(after, reply)?;
    let object = match live(tx, ids, key) {
        Some(object) => object,
        None => {
            let object = create(tx, COMMENT, key, non_null(fields))?;
            if !after.text.is_empty() {
                tx.text_insert(&object, "body", 0, &after.text)?;
            }
            ids.comments.insert(key.to_owned(), object.clone());
            return Ok((object, true));
        }
    };
    let mut wrote = false;
    let old = before.map(|b| comment_fields(b, reply)).transpose()?;
    for (field, value) in fields {
        if old.as_ref().and_then(|o| o.get(&field)) != Some(&value) {
            tx.set(&object, &field, value)?;
            wrote = true;
        }
    }
    if before.is_none_or(|b| b.text != after.text) {
        let len = tx.text_len(&object, "body")?;
        if len > 0 {
            tx.text_delete(&object, "body", 0, len)?;
        }
        if !after.text.is_empty() {
            tx.text_insert(&object, "body", 0, &after.text)?;
        }
        wrote = true;
    }
    Ok((object, wrote))
}

/// The threads a view held before a write, by key: taken before the write deletes slides (and
/// forgets their objects), so their threads go with them.
pub(crate) struct Before<'p>(BTreeMap<String, (ObjectId, &'p Comment, Keys)>);

pub(crate) fn before<'p>(before: Option<&'p SlideFile>, ids: &Ids) -> Before<'p> {
    Before(match before {
        Some(b) => view_threads(b, ids).into_iter().map(|(slide, c, keys)| (keys.thread.clone(), (slide, c, keys))).collect(),
        None => BTreeMap::new(),
    })
}

/// Writes the view's comment threads that differ from `before` (all of them without one) and
/// deletes those it no longer holds. Every object it writes is a `CommentThread` or `Comment`.
pub(crate) fn write(tx: &mut Tx<'_>, before: &Before<'_>, after: &SlideFile, ids: &mut Ids) -> KdmResult<bool> {
    let old = &before.0;
    let mut wrote = false;
    let mut kept: HashSet<String> = HashSet::new();
    for (slide, c, keys) in view_threads(after, ids) {
        kept.insert(keys.thread.clone());
        // What the view held of it: unchanged, it is not written (a thread a 0.4.x replica still
        // holds on its slide stays there until the conversion).
        let previous = old.get(&keys.thread);
        if previous.is_some_and(|(s, b, k)| *s == slide && *b == c && *k == keys) {
            continue;
        }
        let previous = previous.filter(|_| live(tx, ids, &keys.thread).is_some());
        let anchor = anchor_of(c, &slide, ids);
        let thread = match live(tx, ids, &keys.thread) {
            Some(thread) => {
                if previous.is_none_or(|(s, b, _)| anchor_of(b, s, ids) != anchor) {
                    tx.set(&thread, "anchor", anchor)?;
                    wrote = true;
                }
                if previous.is_none_or(|(_, b, _)| b.resolved != c.resolved) {
                    tx.set(&thread, "resolved", Value::Bool(c.resolved))?;
                    wrote = true;
                }
                thread
            }
            None => {
                let mut fields = Map::new();
                fields.insert("anchor".into(), anchor);
                fields.insert("resolved".into(), Value::Bool(c.resolved));
                let thread = create(tx, THREAD, &keys.thread, fields)?;
                ids.comments.insert(keys.thread.clone(), thread.clone());
                wrote = true;
                thread
            }
        };
        // The thread's comments: its first, then its replies.
        let before_comments: HashMap<&str, (&Comment, bool)> = previous
            .map(|(_, b, k)| {
                k.comments.iter().map(String::as_str).zip(std::iter::once((*b, false)).chain(b.replies.iter().map(|r| (r, true)))).collect()
            })
            .unwrap_or_default();
        let mut order = Vec::with_capacity(keys.comments.len());
        for (key, (comment, reply)) in keys.comments.iter().zip(std::iter::once((c, false)).chain(c.replies.iter().map(|r| (r, true)))) {
            let earlier = before_comments.get(key.as_str()).filter(|(_, was_reply)| *was_reply == reply).map(|(b, _)| *b);
            let earlier = earlier.filter(|_| live(tx, ids, key).is_some());
            let unchanged = earlier.is_some_and(|b| if reply { b == comment } else { same_head(b, comment) });
            let object = match (unchanged, live(tx, ids, key)) {
                (true, Some(object)) => object,
                _ => {
                    let (object, w) = write_comment(tx, ids, key, earlier, comment, reply)?;
                    wrote |= w;
                    object
                }
            };
            order.push(object);
        }
        let listed: HashSet<&ObjectId> = order.iter().collect();
        if let Some((_, _, k)) = previous {
            for key in &k.comments {
                if let Some(object) = live(tx, ids, key)
                    && !listed.contains(&object)
                {
                    tx.delete(&object)?;
                    ids.comments.remove(key);
                    wrote = true;
                }
            }
        }
        wrote |= sync_list(tx, &thread, "comments", &order)? > 0;
    }
    for (key, (_, _, keys)) in old {
        if kept.contains(key) {
            continue;
        }
        for comment in &keys.comments {
            if let Some(object) = live(tx, ids, comment) {
                tx.delete(&object)?;
                wrote = true;
            }
            ids.comments.remove(comment);
        }
        // The thread object stays, listing nothing (it reads as no thread): a reply someone adds
        // to it meanwhile brings it back instead of being lost with it.
        if let Some(thread) = live(tx, ids, key) {
            wrote |= sync_list(tx, &thread, "comments", &[])? > 0;
        }
        ids.comments.remove(key);
    }
    Ok(wrote)
}

/// Whether two first comments of a thread hold the same comment (their thread's fields and
/// replies aside).
fn same_head(a: &Comment, b: &Comment) -> bool {
    a.author == b.author && a.author_id == b.author_id && a.initials == b.initials && a.text == b.text && a.date == b.date && a.kept == b.kept
}

/// What a thread converted from a 0.4.x comment records (its own optional fields, which no
/// later write changes): that comment's key ([`legacy_id`]) and whether it was resolved then, so
/// merging two conversions knows which side changed it.
#[derive(Default, Clone)]
struct ThreadProps {
    legacy: Option<String>,
    was_resolved: bool,
}

/// Converts the 0.4.x comments `view` read from its slides' `comments` registers into threads (the
/// ones no replica converted yet), clears those registers, and raises the minimum schema version
/// to [`COMMENTS_SCHEMA_VERSION`]. Answers whether it wrote.
pub(crate) fn migrate(tx: &mut Tx<'_>, view: &SlideFile, ids: &mut Ids) -> KdmResult<bool> {
    let mut wrote = false;
    if ids.legacy_comments {
        for slide in &view.slides {
            let Some(object) = ids.slides.object(slide.id).cloned() else { continue };
            if !tx.exists(&object) || tx.get(&object, "comments")?.is_null() {
                continue;
            }
            for c in &slide.comments {
                // The view's keys for an old comment are its `legacy_id`s; a thread read from an
                // object (converted already) is left alone.
                if live(tx, ids, &c.id).is_some() || !ids.legacy_keys.contains(&c.id) {
                    continue;
                }
                let mut fields = Map::new();
                fields.insert("anchor".into(), anchor_of(c, &object, ids));
                fields.insert("resolved".into(), Value::Bool(c.resolved));
                fields.insert("legacy".into(), Value::String(c.id.clone()));
                fields.insert("wasResolved".into(), Value::Bool(c.resolved));
                let thread = tx.create(THREAD, fields)?;
                ids.comments.insert(c.id.clone(), thread.clone());
                let mut order = Vec::new();
                for (key, (comment, reply)) in std::iter::once(&c.head)
                    .chain(c.replies.iter().map(|r| &r.id))
                    .zip(std::iter::once((c, false)).chain(c.replies.iter().map(|r| (r, true))))
                {
                    let mut fields = non_null(comment_fields(comment, reply)?);
                    fields.insert("legacy".into(), Value::String(key.clone()));
                    let object = tx.create(COMMENT, fields)?;
                    if !comment.text.is_empty() {
                        tx.text_insert(&object, "body", 0, &comment.text)?;
                    }
                    ids.comments.insert(key.clone(), object.clone());
                    order.push(object);
                }
                sync_list(tx, &thread, "comments", &order)?;
            }
            tx.set(&object, "comments", Value::Null)?;
            wrote = true;
        }
        ids.legacy_comments = false;
    }
    let raised = tx.raise_schema_version(COMMENTS_SCHEMA_VERSION)?;
    Ok(wrote || raised)
}

/// Two or more threads converted from one 0.4.x comment (by replicas converting offline), as the
/// read shows them: one thread, `keeper`, listing `order` and resolved as `resolved`; the others
/// list nothing more, and the copies of old comments they repeat go.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Siblings {
    keeper: ObjectId,
    order: Vec<ObjectId>,
    resolved: bool,
    threads: Vec<ObjectId>,
    copies: Vec<ObjectId>,
}

/// One thread as read from its object.
struct ReadThread {
    id: ObjectId,
    anchor: Value,
    resolved: bool,
    props: ThreadProps,
    /// Its live comments in order, with their objects and 0.4.x tags.
    comments: Vec<(ObjectId, Option<String>, Comment)>,
}

fn read_thread(replica: &Replica, id: ObjectId, relist: &mut Vec<Siblings>) -> KdmResult<ReadThread> {
    let props = ThreadProps {
        legacy: replica.get(&id, "legacy")?.as_str().map(str::to_owned),
        was_resolved: replica.get(&id, "wasResolved")?.as_bool().unwrap_or(false),
    };
    let mut comments = Vec::new();
    let mut listed = HashSet::new();
    let items = replica.list(&id, "comments")?;
    for item in &items {
        if replica.exists(item) && replica.object_type(item)? == COMMENT && listed.insert(item.clone()) {
            let (tag, comment) = read_comment(replica, item)?;
            comments.push((item.clone(), tag, comment));
        }
    }
    // A comment listed twice (two replicas listed it at once) or listed but gone reads once, or
    // not at all; the read repair lists the thread's comments anew.
    if items.len() != comments.len() {
        relist.push(Siblings {
            keeper: id.clone(),
            order: comments.iter().map(|(object, ..)| object.clone()).collect(),
            resolved: replica.get(&id, "resolved")?.as_bool().unwrap_or(false),
            threads: Vec::new(),
            copies: Vec::new(),
        });
    }
    Ok(ReadThread { anchor: replica.get(&id, "anchor")?, resolved: replica.get(&id, "resolved")?.as_bool().unwrap_or(false), id, props, comments })
}

/// Merges threads converted from one 0.4.x comment (`group`, two or more, by id): the first keeps
/// its comments; the others add each old comment it lacks and the comments not converted from an
/// old one (replies added after the conversion), oldest first. Nothing is dropped but the copies
/// of an old comment the merged thread already holds (an old comment one side deleted may come
/// back: a sibling that lacks one may have lacked it because a repair merged it away). The thread
/// is resolved as its last side to change it left it.
fn merge(mut group: Vec<ReadThread>) -> (ReadThread, Option<Siblings>) {
    group.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let mut kept = Vec::new();
    let mut copies = Vec::new();
    let mut added = Vec::new();
    let mut seen = HashSet::new();
    for (i, thread) in group.iter().enumerate() {
        for (object, tag, comment) in &thread.comments {
            match tag {
                Some(tag) if seen.insert(tag.clone()) => kept.push((object.clone(), Some(tag.clone()), comment.clone())),
                Some(_) => copies.push(object.clone()),
                None if i == 0 => kept.push((object.clone(), None, comment.clone())),
                None => added.push((object.clone(), None, comment.clone())),
            }
        }
    }
    added.sort_by(|a, b| a.2.date.cmp(&b.2.date).then_with(|| a.0.as_str().cmp(b.0.as_str())));
    kept.extend(added);
    // Siblings a repair emptied already have no say in whether the thread is resolved.
    let original = group.first().is_some_and(|t| t.props.was_resolved);
    let voting = group.iter().enumerate().filter(|(i, t)| *i == 0 || !t.comments.is_empty());
    let resolved = if voting.into_iter().any(|(_, t)| t.resolved != original) { !original } else { original };
    let mut keeper = group.remove(0);
    let order: Vec<ObjectId> = kept.iter().map(|(object, ..)| object.clone()).collect();
    let copies: Vec<ObjectId> = copies.into_iter().filter(|c| !kept.iter().any(|(k, ..)| k == c)).collect();
    let settled = keeper.comments.iter().map(|(object, ..)| object).eq(order.iter())
        && keeper.resolved == resolved
        && copies.is_empty()
        && group.iter().all(|t| t.comments.is_empty());
    let siblings =
        (!settled).then(|| Siblings { keeper: keeper.id.clone(), order, resolved, threads: group.iter().map(|t| t.id.clone()).collect(), copies });
    keeper.comments = kept;
    keeper.resolved = resolved;
    (keeper, siblings)
}

/// Reads the replica's comment threads into the slides holding them (`slides`: each slide's
/// object and the comments its 0.4.x `comments` register held), recording every thread and
/// comment in `ids`. A thread whose slide is gone stays in the replica, unread.
pub(crate) fn read(replica: &Replica, slides: &mut [(ObjectId, &mut Vec<Comment>)], ids: &mut Ids) -> KdmResult<()> {
    let index: HashMap<ObjectId, usize> = slides.iter().enumerate().map(|(i, (object, _))| (object.clone(), i)).collect();
    let mut shapes: HashMap<ObjectId, (ObjectId, ShapeId)> = HashMap::new();
    for ((owner, shape), object) in ids.shape_entries() {
        if index.contains_key(owner) {
            shapes.insert(object.clone(), (owner.clone(), *shape));
        }
    }
    ids.siblings.clear();
    ids.legacy_keys.clear();
    let types = &replica.schema().types;
    let mut threads: Vec<ReadThread> = Vec::new();
    if types.contains_key(THREAD) && types.contains_key(COMMENT) {
        let mut by_legacy: BTreeMap<String, Vec<ReadThread>> = BTreeMap::new();
        for id in replica.objects_of_type(THREAD) {
            let thread = read_thread(replica, id, &mut ids.siblings)?;
            match thread.props.legacy.clone() {
                Some(tag) => by_legacy.entry(tag).or_default().push(thread),
                None => threads.push(thread),
            }
        }
        for (_, group) in by_legacy {
            if group.len() == 1 {
                threads.extend(group);
                continue;
            }
            let (keeper, siblings) = merge(group);
            ids.siblings.extend(siblings);
            threads.push(keeper);
        }
    }
    let converted: HashSet<String> = threads.iter().filter_map(|t| t.props.legacy.clone()).collect();
    // The 0.4.x comments no replica converted yet, under their keys.
    for (object, comments) in slides.iter_mut() {
        if comments.is_empty() {
            continue;
        }
        ids.legacy_comments = true;
        let legacy = std::mem::take(*comments);
        for (i, mut c) in legacy.into_iter().enumerate() {
            c.id = legacy_id(object, i, None);
            if converted.contains(&c.id) {
                continue;
            }
            c.head = legacy_id(object, i, Some(0));
            for (j, r) in c.replies.iter_mut().enumerate() {
                r.id = legacy_id(object, i, Some(j + 1));
            }
            ids.legacy_keys.insert(c.id.clone());
            comments.push(c);
        }
    }
    let mut found: Vec<(usize, Comment)> = Vec::new();
    for thread in threads {
        let text = |key: &str| thread.anchor.get(key).and_then(Value::as_str).map(ObjectId::from_string);
        let on = |object: &ObjectId| {
            index
                .get(object)
                .copied()
                .map(|i| (i, None))
                .or_else(|| shapes.get(object).and_then(|(slide, shape)| index.get(slide).map(|i| (*i, Some(*shape)))))
        };
        let at = text("object").and_then(|o| on(&o));
        let Some((slide, shape)) = at.or_else(|| text("slide").and_then(|s| index.get(&s).map(|i| (*i, None)))) else { continue };
        let mut comments = thread.comments.into_iter();
        let Some((head, _, mut first)) = comments.next() else { continue };
        let replies: Vec<Comment> = comments
            .map(|(object, _, mut reply)| {
                reply.id = object.as_str().to_owned();
                ids.comments.insert(reply.id.clone(), object);
                reply
            })
            .collect();
        first.id = thread.id.as_str().to_owned();
        first.head = head.as_str().to_owned();
        first.shape = shape;
        first.x = thread.anchor.get("x").and_then(Value::as_f64).unwrap_or(0.0);
        first.y = thread.anchor.get("y").and_then(Value::as_f64).unwrap_or(0.0);
        first.resolved = thread.resolved;
        first.replies = replies;
        ids.comments.insert(first.id.clone(), thread.id);
        ids.comments.insert(first.head.clone(), head);
        found.push((slide, first));
    }
    // Oldest first, as `kition_kdm::comment::threads` orders them.
    found.sort_by(|a, b| a.1.date.cmp(&b.1.date).then_with(|| a.1.id.cmp(&b.1.id)));
    for (slide, c) in found {
        if let Some((_, comments)) = slides.get_mut(slide) {
            comments.push(c);
        }
    }
    Ok(())
}

/// The read repair: merges in the replica each group of threads the read showed as one
/// ([`Siblings`]). Answers whether it wrote.
pub(crate) fn repair(tx: &mut Tx<'_>, groups: &[Siblings]) -> KdmResult<bool> {
    let mut wrote = false;
    for group in groups {
        if !tx.exists(&group.keeper) {
            continue;
        }
        let order: Vec<ObjectId> = group.order.iter().filter(|o| tx.exists(o)).cloned().collect();
        wrote |= sync_list(tx, &group.keeper, "comments", &order)? > 0;
        if tx.get(&group.keeper, "resolved")?.as_bool() != Some(group.resolved) {
            tx.set(&group.keeper, "resolved", Value::Bool(group.resolved))?;
            wrote = true;
        }
        for object in &group.copies {
            if tx.exists(object) {
                tx.delete(object)?;
                wrote = true;
            }
        }
        // The other threads stay, listing nothing: a reply a replica adds to one of them before
        // it sees this merge joins the thread again with the next read.
        for thread in &group.threads {
            if tx.exists(thread) {
                wrote |= sync_list(tx, thread, "comments", &[])? > 0;
            }
        }
    }
    Ok(wrote)
}

fn read_comment(replica: &Replica, object: &ObjectId) -> KdmResult<(Option<String>, Comment)> {
    let text = |field: &str| replica.get(object, field).map(|v| v.as_str().map(str::to_owned));
    let author = text("author")?.unwrap_or_default();
    let name = text("name")?;
    let props: CommentProps = match replica.get(object, "props")? {
        Value::Null => CommentProps::default(),
        v => serde_json::from_value(v).unwrap_or_default(),
    };
    let legacy = replica.get(object, "legacy")?.as_str().map(str::to_owned);
    let author_id = match &name {
        Some(name) if *name != author => author.clone(),
        _ => String::new(),
    };
    let comment = Comment {
        author: name.unwrap_or(author),
        author_id,
        initials: text("initials")?.unwrap_or_default(),
        text: replica.text(object, "body")?,
        date: text("created")?.unwrap_or_default(),
        x: props.x.unwrap_or(0.0),
        y: props.y.unwrap_or(0.0),
        kept: props.kept,
        ..Comment::default()
    };
    Ok((legacy, comment))
}
