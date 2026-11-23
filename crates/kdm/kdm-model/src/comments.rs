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
//! - A comment listed twice in its thread (two replicas listed it at once), or listed but gone,
//!   reads once, or not at all; the session's read repair ([`repair`]) lists the thread's comments
//!   so in the replica.
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
        // What the view held of it: unchanged, it is not written.
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

/// A thread whose comment list holds an entry twice, or a deleted one, as the read listed it:
/// `order` its live comments, each once (the read repair lists them so).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relist {
    thread: ObjectId,
    order: Vec<ObjectId>,
}

/// One thread as read from its object.
struct ReadThread {
    id: ObjectId,
    anchor: Value,
    resolved: bool,
    /// Its live comments in order, with their objects.
    comments: Vec<(ObjectId, Comment)>,
}

fn read_thread(replica: &Replica, id: ObjectId, relist: &mut Vec<Relist>) -> KdmResult<ReadThread> {
    let mut comments = Vec::new();
    let mut listed = HashSet::new();
    let items = replica.list(&id, "comments")?;
    for item in &items {
        if replica.exists(item) && replica.object_type(item)? == COMMENT && listed.insert(item.clone()) {
            comments.push((item.clone(), read_comment(replica, item)?));
        }
    }
    if items.len() != comments.len() {
        relist.push(Relist { thread: id.clone(), order: comments.iter().map(|(object, _)| object.clone()).collect() });
    }
    Ok(ReadThread { anchor: replica.get(&id, "anchor")?, resolved: replica.get(&id, "resolved")?.as_bool().unwrap_or(false), id, comments })
}

/// Reads the replica's comment threads into the slides holding them (`slides`: each slide's
/// object and its comments, empty), recording every thread and comment in `ids`. A thread whose
/// slide is gone stays in the replica, unread.
pub(crate) fn read(replica: &Replica, slides: &mut [(ObjectId, &mut Vec<Comment>)], ids: &mut Ids) -> KdmResult<()> {
    let index: HashMap<ObjectId, usize> = slides.iter().enumerate().map(|(i, (object, _))| (object.clone(), i)).collect();
    let mut shapes: HashMap<ObjectId, (ObjectId, ShapeId)> = HashMap::new();
    for ((owner, shape), object) in ids.shape_entries() {
        if index.contains_key(owner) {
            shapes.insert(object.clone(), (owner.clone(), *shape));
        }
    }
    ids.relist.clear();
    let mut threads: Vec<ReadThread> = Vec::new();
    for id in replica.objects_of_type(THREAD) {
        threads.push(read_thread(replica, id, &mut ids.relist)?);
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
        let Some((head, mut first)) = comments.next() else { continue };
        let replies: Vec<Comment> = comments
            .map(|(object, mut reply)| {
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

/// The read repair: lists each thread's comments as the read listed them ([`Relist`]). Answers
/// whether it wrote.
pub(crate) fn repair(tx: &mut Tx<'_>, threads: &[Relist]) -> KdmResult<bool> {
    let mut wrote = false;
    for relist in threads {
        if !tx.exists(&relist.thread) {
            continue;
        }
        let order: Vec<ObjectId> = relist.order.iter().filter(|o| tx.exists(o)).cloned().collect();
        wrote |= sync_list(tx, &relist.thread, "comments", &order)? > 0;
    }
    Ok(wrote)
}

fn read_comment(replica: &Replica, object: &ObjectId) -> KdmResult<Comment> {
    let text = |field: &str| replica.get(object, field).map(|v| v.as_str().map(str::to_owned));
    let author = text("author")?.unwrap_or_default();
    let name = text("name")?;
    let props: CommentProps = match replica.get(object, "props")? {
        Value::Null => CommentProps::default(),
        v => serde_json::from_value(v).unwrap_or_default(),
    };
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
    Ok(comment)
}
