//! Comment threads (Kition Kit Standard 2.0 rule M23): threads and comments are their own
//! objects beside the slides; concurrent comments all survive; a commenter's change touches only
//! comment objects; a 0.4.x replica's comments convert once, the same way on every replica.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{SharedBlobs, actor, canonical, new_replica, person, slide_file};
use kdm_model::{COMMENTS_SCHEMA_VERSION, SlideModel, SlideSession, read_document};
use kition_kit::kdm::{DocId, ObjectId, Origin, Replica, Schema};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_slide_model::{Comment, CommentKept, SlideFile};
use serde_json::{Value, json};

const COMMENT_TYPES: [&str; 2] = ["CommentThread", "Comment"];

fn said(author: &str, text: &str, date: &str) -> Comment {
    Comment { author: author.into(), text: text.into(), date: date.into(), ..Default::default() }
}

fn new_id() -> String {
    ObjectId::new().as_str().to_owned()
}

/// A thread as the engine opens one: ids minted.
fn thread(author: &str, text: &str, date: &str) -> Comment {
    Comment { id: new_id(), head: new_id(), ..said(author, text, date) }
}

fn reply(author: &str, text: &str, date: &str) -> Comment {
    Comment { id: new_id(), ..said(author, text, date) }
}

fn memory(blobs: &SharedBlobs) -> MemoryBlobs {
    MemoryBlobs(blobs.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().into_iter().collect())
}

fn session_of(p: &SlideFile) -> (SlideSession, SharedBlobs) {
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(p, &blobs);
    let snapshot = replica.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    (SlideSession::open_snapshot(&snapshot, memory(&blobs)).unwrap_or_else(|error| panic!("open: {error}")), blobs)
}

fn peer_of(s: &SlideSession) -> SlideSession {
    SlideSession::open_snapshot(&s.snapshot().unwrap_or_else(|error| panic!("{error}")), s.blobs().clone()).unwrap_or_else(|error| panic!("{error}"))
}

fn record_with(s: &mut SlideSession, who: &str, f: impl FnOnce(&mut SlideFile)) {
    let mut after = s.slide_file().clone();
    f(&mut after);
    s.record(&person(who), &after).unwrap_or_else(|error| panic!("record: {error}"));
}

fn exchange(a: &mut SlideSession, b: &mut SlideSession) {
    for _ in 0..4 {
        let since = b.version();
        b.import_changes(&a.export_changes(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
        let since = a.version();
        a.import_changes(&b.export_changes(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    }
    assert_eq!(a.version(), b.version(), "the replicas converged");
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    assert_eq!(canonical(a.slide_file()), canonical(b.slide_file()));
}

fn view_is_replica(s: &SlideSession) {
    let (fresh, _) = read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(s.slide_file()), "the view is the replica's");
}

/// The objects a change touches, as a replica importing it reports them (the containers its
/// operations write), with their types there.
fn touched(from: &SlideSession, since: &kition_kit::kdm::Version, base: &[u8]) -> BTreeSet<(String, String)> {
    let mut other = Replica::load(SlideModel::schema(), base, actor(77)).unwrap_or_else(|error| panic!("load: {error}"));
    let before = other.state_json().unwrap_or_else(|error| panic!("{error}"));
    let changed =
        other.import_changes(&from.export_changes(since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    let after = other.state_json().unwrap_or_else(|error| panic!("{error}"));
    let base_replica = Replica::load(SlideModel::schema(), base, actor(78)).unwrap_or_else(|error| panic!("load: {error}"));
    // Outside the objects (the document's meta, its schema version) nothing moves.
    let outside = |state: &Value| {
        state.as_object().map(|map| map.iter().filter(|(key, _)| key.as_str() != "objects").map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>())
    };
    assert_eq!(outside(&before), outside(&after), "the change writes outside the objects");
    assert_eq!(other.min_schema_version(), base_replica.min_schema_version());
    changed
        .into_iter()
        .map(|object| {
            let ty = other.object_type(&object).or_else(|_| base_replica.object_type(&object)).unwrap_or_else(|_| "?".into());
            (ty, object.as_str().to_owned())
        })
        .collect()
}

/// No comment is left, and every thread object left lists none (a deleted thread keeps its
/// object, so a concurrent reply to it is not lost).
fn assert_no_threads(replica: &Replica) {
    assert!(replica.objects_of_type("Comment").is_empty());
    assert_eq!(listing(replica), 0);
}

/// How many threads list a comment.
fn listing(replica: &Replica) -> usize {
    replica.objects_of_type("CommentThread").iter().filter(|t| !replica.list(t, "comments").unwrap_or_default().is_empty()).count()
}

fn only_comments(touched: &BTreeSet<(String, String)>) {
    assert!(!touched.is_empty(), "the change touched something");
    for (ty, object) in touched {
        assert!(COMMENT_TYPES.contains(&ty.as_str()), "the change touched {ty} {object}");
    }
}

#[test]
fn threads_are_objects_beside_the_slides_and_read_back() {
    let mut p = slide_file();
    let shape = p.slides[1].shapes[0].id;
    let mut first = thread("Ann", "Is this right?", "2026-10-19T10:00:00Z");
    first.author_id = "user:ann".into();
    first.shape = Some(shape);
    first.x = 120.0;
    first.y = 64.0;
    first.initials = "AN".into();
    first.kept = Some(Box::new(CommentKept { modern: true, guid: "{AAAAAAAA-0000-0000-0000-000000000001}".into(), ..Default::default() }));
    let mut answer = reply("Bob", "Yes.", "2026-10-19T10:05:00Z");
    answer.x = 3.0;
    answer.y = 4.0;
    first.replies = vec![answer];
    first.resolved = true;
    Arc::make_mut(&mut p.slides[1]).comments = vec![first.clone(), thread("Cy", "Second", "2026-10-19T11:00:00Z")];
    let (s, _) = session_of(&p);
    assert_eq!(canonical(s.slide_file()), canonical(&p));
    let read = &s.slide_file().slides[1].comments;
    assert_eq!(read[0].id, first.id, "a thread's id is its object's");
    assert_eq!((read[0].head.as_str(), read[0].replies[0].id.as_str()), (first.head.as_str(), first.replies[0].id.as_str()));

    let replica = s.replica();
    assert_eq!(replica.objects_of_type("CommentThread").len(), 2);
    assert_eq!(replica.objects_of_type("Comment").len(), 3);
    for slide in replica.objects_of_type("Slide") {
        assert!(replica.get(&slide, "comments").unwrap_or_default().is_null(), "no comments on a slide");
    }
    let object = ObjectId::from_string(first.id.as_str());
    let anchor = replica.get(&object, "anchor").unwrap_or_default();
    let view = s.slide_file();
    let slide = s.model().ids().slides.object(view.slides[1].id).unwrap_or_else(|| panic!("slide")).clone();
    let shape_object = s
        .model()
        .ids()
        .shape(&slide, view.slides[1].comments[0].shape.unwrap_or_else(|| panic!("shape id")))
        .unwrap_or_else(|| panic!("shape"))
        .clone();
    assert_eq!(view.slides[1].shapes[0].name, p.slides[1].shapes[0].name);
    let _ = shape;
    let point = |key: &str| anchor.get(key).and_then(Value::as_f64);
    assert_eq!((anchor.get("object"), anchor.get("slide")), (Some(&json!(shape_object.as_str())), Some(&json!(slide.as_str()))));
    assert_eq!((point("x"), point("y")), (Some(120.0), Some(64.0)));
    // The convention's own reader sees the same threads.
    let threads = kition_kit::kdm::comment::threads(replica).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(threads.len(), 2);
    assert!(threads[0].resolved);
    let people: Vec<(&str, Option<&str>, &str)> =
        threads[0].comments.iter().map(|c| (c.author.as_str(), c.name.as_deref(), c.body.as_str())).collect();
    assert_eq!(people, vec![("user:ann", Some("Ann"), "Is this right?"), ("Bob", Some("Bob"), "Yes.")]);
}

#[test]
fn comment_edits_touch_only_comment_objects_and_undo() {
    let p = slide_file();
    let (mut s, _) = session_of(&p);
    let base = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = s.version();
    let opened = thread("Ann", "Look here", "2026-10-19T10:00:00Z");
    let id = opened.id.clone();
    record_with(&mut s, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments.push(opened.clone()));
    only_comments(&touched(&s, &since, &base));
    view_is_replica(&s);

    let base = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = s.version();
    record_with(&mut s, "bob", |p| {
        let c = &mut Arc::make_mut(&mut p.slides[0]).comments[0];
        c.replies.push(reply("Bob", "Agreed", "2026-10-19T10:01:00Z"));
        c.resolved = true;
    });
    let touched_now = touched(&s, &since, &base);
    only_comments(&touched_now);
    assert!(touched_now.iter().any(|(ty, object)| ty == "CommentThread" && *object == id));
    view_is_replica(&s);

    // Deleting a reply, then the thread.
    let base = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = s.version();
    record_with(&mut s, "bob", |p| {
        Arc::make_mut(&mut p.slides[0]).comments[0].replies.clear();
    });
    record_with(&mut s, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments.clear());
    only_comments(&touched(&s, &since, &base));
    assert_no_threads(s.replica());

    // Undo brings them back, one step at a time.
    assert!(s.undo().unwrap_or_else(|error| panic!("{error}")).0);
    assert_eq!(s.slide_file().slides[0].comments.len(), 1);
    assert!(s.undo().unwrap_or_else(|error| panic!("{error}")).0);
    assert_eq!(s.slide_file().slides[0].comments[0].replies.len(), 1);
    view_is_replica(&s);
}

#[test]
fn a_commenter_shaped_change_touches_only_comment_objects() {
    let mut p = slide_file();
    Arc::make_mut(&mut p.slides[0]).comments.push(thread("Ann", "Open question", "2026-10-19T10:00:00Z"));
    let (mut s, _) = session_of(&p);
    s.set_comment_only(true);
    let base = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = s.version();
    // A commenter's view changes only comments; even if something else slipped into it, a
    // comment-only record writes only the threads.
    let mut after = s.slide_file().clone();
    {
        let slide = Arc::make_mut(&mut after.slides[0]);
        slide.comments[0].replies.push(Comment { author_id: "user:bob".into(), ..reply("Bob", "I can answer", "2026-10-19T10:02:00Z") });
        slide.comments[0].resolved = true;
        slide.comments.push(Comment { author_id: "user:bob".into(), ..thread("Bob", "New thread", "2026-10-19T10:03:00Z") });
        slide.name = "Renamed by a commenter".into();
        slide.shapes[0].name = "Moved".into();
    }
    s.record(&person("bob"), &after).unwrap_or_else(|error| panic!("record: {error}"));
    let touched = touched(&s, &since, &base);
    only_comments(&touched);
    assert_eq!(touched.iter().filter(|(ty, _)| ty == "Comment").count(), 2, "two comments written: {touched:?}");
    // The commenter's own comments name them as their author (what the server checks).
    let threads = kition_kit::kdm::comment::threads(s.replica()).unwrap_or_else(|error| panic!("{error}"));
    let bob: Vec<&str> = threads.iter().flat_map(|t| &t.comments).filter(|c| c.body != "Open question").map(|c| c.author.as_str()).collect();
    assert_eq!(bob, vec!["user:bob", "user:bob"]);
    assert!(s.comment_only());
}

#[test]
fn concurrent_replies_to_one_thread_and_new_threads_are_all_kept() {
    let mut p = slide_file();
    let opened = thread("Ann", "One", "2026-10-19T10:00:00Z");
    Arc::make_mut(&mut p.slides[0]).comments.push(opened);
    let (mut a, _) = session_of(&p);
    let mut b = peer_of(&a);
    record_with(&mut a, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments[0].replies.push(reply("Ann", "Two", "2026-10-19T10:02:00Z")));
    record_with(&mut b, "bob", |p| {
        let slide = Arc::make_mut(&mut p.slides[0]);
        slide.comments[0].replies.push(reply("Bob", "Three", "2026-10-19T10:02:00Z"));
        slide.comments[0].resolved = true;
        slide.comments.push(thread("Bob", "Elsewhere", "2026-10-19T10:03:00Z"));
    });
    record_with(&mut a, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments.push(thread("Ann", "Mine too", "2026-10-19T10:04:00Z")));
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        view_is_replica(s);
        let comments = &s.slide_file().slides[0].comments;
        assert_eq!(comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["One", "Elsewhere", "Mine too"]);
        let mut replies: Vec<&str> = comments[0].replies.iter().map(|r| r.text.as_str()).collect();
        replies.sort_unstable();
        assert_eq!(replies, vec!["Three", "Two"], "both replies are kept");
        assert!(comments[0].resolved);
    }
}

#[test]
fn a_reply_made_while_its_thread_is_deleted_is_kept() {
    let mut p = slide_file();
    Arc::make_mut(&mut p.slides[0]).comments.push(thread("Ann", "Delete me?", "2026-10-19T10:00:00Z"));
    let (mut a, _) = session_of(&p);
    let mut b = peer_of(&a);
    record_with(&mut a, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments.clear());
    record_with(&mut b, "bob", |p| {
        Arc::make_mut(&mut p.slides[0]).comments[0].replies.push(reply("Bob", "Wait, I answered", "2026-10-19T10:01:00Z"))
    });
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        let comments = &s.slide_file().slides[0].comments;
        assert_eq!(comments.len(), 1, "the thread reads again with the reply");
        assert_eq!(comments[0].text, "Wait, I answered");
        view_is_replica(s);
    }
}

#[test]
fn deleting_a_slide_deletes_its_threads_and_undo_restores_them() {
    let mut p = slide_file();
    Arc::make_mut(&mut p.slides[2])
        .comments
        .push(Comment { replies: vec![reply("Bob", "Reply", "2026-10-19T10:01:00Z")], ..thread("Ann", "On slide 3", "2026-10-19T10:00:00Z") });
    let (mut s, _) = session_of(&p);
    record_with(&mut s, "ann", |p| {
        p.slides.remove(2);
    });
    assert_no_threads(s.replica());
    assert!(s.undo().unwrap_or_else(|error| panic!("{error}")).0);
    assert_eq!(s.slide_file().slides[2].comments[0].replies[0].text, "Reply");
    view_is_replica(&s);
}

#[test]
fn comments_copied_with_a_slide_get_their_own_objects() {
    let mut p = slide_file();
    Arc::make_mut(&mut p.slides[0]).comments.push(thread("Ann", "Copied", "2026-10-19T10:00:00Z"));
    let (mut s, _) = session_of(&p);
    // A duplicated slide carries the same comment ids (the engine renames them before it
    // writes; the write must not merge the two copies into one object even if it did not).
    record_with(&mut s, "ann", |p| {
        let mut copy = (*p.slides[0]).clone();
        copy.id = kition_slide_model::SlideId(p.alloc_id());
        copy.shapes.clear();
        p.slides.push(Arc::new(copy));
    });
    assert_eq!(s.replica().objects_of_type("CommentThread").len(), 2);
    assert_eq!(s.replica().objects_of_type("Comment").len(), 2);
    let (fresh, _) = read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(fresh.slides.iter().map(|s| s.comments.len()).sum::<usize>(), 2);
    assert_eq!(canonical(&fresh), canonical(s.slide_file()));
}

// --- 0.4.x replicas ------------------------------------------------------------------------------

fn schema_version(version: u32) -> Arc<Schema> {
    let current = format!("\"schemaVersion\": {}", SlideModel::schema().schema_version);
    let json = include_str!("../model/schema.json").replacen(&current, &format!("\"schemaVersion\": {version}"), 1);
    Arc::new(Schema::parse(&json).unwrap_or_else(|error| panic!("schema: {error}")))
}

/// A 0.4.x replica (schema version 2): slide 0's comments in its `comments` register, the way a
/// 0.4.x build wrote them (the shape as its object id).
fn legacy_snapshot() -> (Vec<u8>, MemoryBlobs, SlideFile) {
    let p = slide_file();
    let blobs = SharedBlobs::default();
    let mut ids = kdm_model::Ids::default();
    let (mut replica, ()) = Replica::create_with(schema_version(2), DocId::new(), actor(5), &Origin::system("create"), |tx| {
        kdm_model::write_document(tx, &p, &mut ids, &mut blobs.clone()).map(|_| ())
    })
    .unwrap_or_else(|error| panic!("create: {error}"));
    let slide = ids.slides.object(p.slides[0].id).unwrap_or_else(|| panic!("slide")).clone();
    let shape = ids.shape(&slide, p.slides[0].shapes[0].id).unwrap_or_else(|| panic!("shape")).clone();
    let comments = json!([
        { "author": "Ann", "initials": "A", "text": "Old one", "date": "2026-01-01T10:00:00.000", "x": 10.0, "y": 20.0, "resolved": false,
          "replies": [{ "author": "Bob", "initials": "B", "text": "Old reply", "date": "2026-01-01T11:00:00.000", "x": 0.0, "y": 0.0, "resolved": false, "replies": [], "shape": null }],
          "shape": shape.as_str() },
        { "author": "Cy", "initials": "C", "text": "Old two", "date": "2026-01-02T10:00:00.000", "x": 5.0, "y": 5.0, "resolved": true, "replies": [], "shape": null }
    ]);
    replica.transact(&Origin::system("legacy"), |tx| tx.set(&slide, "comments", comments)).unwrap_or_else(|error| panic!("{error}"));
    let snapshot = replica.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let mut expected = p.clone();
    let mut first = said("Ann", "Old one", "2026-01-01T10:00:00.000");
    first.initials = "A".into();
    first.x = 10.0;
    first.y = 20.0;
    first.shape = Some(p.slides[0].shapes[0].id);
    first.replies = vec![Comment { initials: "B".into(), ..said("Bob", "Old reply", "2026-01-01T11:00:00.000") }];
    let mut second = said("Cy", "Old two", "2026-01-02T10:00:00.000");
    second.initials = "C".into();
    second.x = 5.0;
    second.y = 5.0;
    second.resolved = true;
    Arc::make_mut(&mut expected.slides[0]).comments = vec![first, second];
    (snapshot, memory(&blobs), expected)
}

#[test]
fn a_0_4_replica_reads_its_comments_and_converts_them_on_the_first_write() {
    let (snapshot, blobs, expected) = legacy_snapshot();
    let mut s = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("open: {error}"));
    assert_eq!(canonical(s.slide_file()), canonical(&expected), "the comments read as threads");
    assert!(s.model().ids().needs_comment_migration());
    assert_eq!(s.replica().min_schema_version(), 2, "reading converts nothing");

    record_with(&mut s, "ann", |p| Arc::make_mut(&mut p.slides[1]).name = "Renamed".into());
    assert_eq!(s.replica().min_schema_version(), COMMENTS_SCHEMA_VERSION);
    assert!(!s.model().ids().needs_comment_migration());
    assert_eq!(s.replica().objects_of_type("CommentThread").len(), 2);
    assert_eq!(s.replica().objects_of_type("Comment").len(), 3);
    for slide in s.replica().objects_of_type("Slide") {
        assert!(s.replica().get(&slide, "comments").unwrap_or_default().is_null());
    }
    view_is_replica(&s);
    // The conversion is no undo step: undo takes back the rename only.
    assert!(s.undo().unwrap_or_else(|error| panic!("{error}")).0);
    assert!(!s.can_undo());
    assert_eq!(s.slide_file().slides[0].comments.len(), 2);

    // A 0.4.x build refuses the converted replica rather than show it without its comments.
    let converted = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    assert!(Replica::load(schema_version(2), &converted, actor(9)).is_err());
    assert!(Replica::load(schema_version(2), &snapshot, actor(9)).is_ok(), "it read the replica before");
}

#[test]
fn two_replicas_converting_one_document_offline_converge_without_duplicates() {
    let (snapshot, blobs, _) = legacy_snapshot();
    let mut a = SlideSession::open_snapshot(&snapshot, blobs.clone()).unwrap_or_else(|error| panic!("{error}"));
    let mut b = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("{error}"));
    record_with(&mut a, "ann", |p| Arc::make_mut(&mut p.slides[0]).comments[0].replies.push(reply("Ann", "New reply by A", "2026-10-19T10:00:00Z")));
    record_with(&mut b, "bob", |p| {
        let slide = Arc::make_mut(&mut p.slides[0]);
        slide.comments[1].resolved = false;
        slide.comments[0].replies.push(reply("Bob", "New reply by B", "2026-10-19T10:00:01Z"));
    });
    exchange(&mut a, &mut b);
    assert_eq!(listing(a.replica()), 2, "one thread per comment, whoever converted it");
    let comments = &a.slide_file().slides[0].comments;
    assert_eq!(comments.len(), 2);
    let mut replies: Vec<&str> = comments[0].replies.iter().map(|r| r.text.as_str()).collect();
    replies.sort_unstable();
    assert_eq!(replies, vec!["New reply by A", "New reply by B", "Old reply"]);
    assert_eq!((comments[0].text.as_str(), comments[1].text.as_str()), ("Old one", "Old two"));
    assert!(!comments[1].resolved);
    assert_eq!(a.replica().objects_of_type("Comment").len(), 5, "no copy of an old comment is left");
    view_is_replica(&a);
    view_is_replica(&b);
}

#[test]
fn a_commenter_never_converts_a_0_4_replica() {
    let (snapshot, blobs, _) = legacy_snapshot();
    let mut s = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("{error}"));
    s.set_comment_only(true);
    let base = s.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = s.version();
    record_with(&mut s, "bob", |p| {
        Arc::make_mut(&mut p.slides[1])
            .comments
            .push(Comment { author_id: "user:bob".into(), ..thread("Bob", "A new thread", "2026-10-19T10:00:00Z") });
    });
    assert_eq!(s.replica().min_schema_version(), 2);
    only_comments(&touched(&s, &since, &base));
    assert_eq!(s.slide_file().slides[0].comments.len(), 2, "the old comments still read");
    view_is_replica(&s);
}

#[test]
fn a_reply_to_a_conversion_another_replica_merged_away_rejoins_the_thread() {
    let (snapshot, blobs, _) = legacy_snapshot();
    let mut a = SlideSession::open_snapshot(&snapshot, blobs.clone()).unwrap_or_else(|error| panic!("{error}"));
    let mut b = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("{error}"));
    // Both convert offline (any edit does).
    record_with(&mut a, "ann", |p| Arc::make_mut(&mut p.slides[1]).name = "A".into());
    record_with(&mut b, "bob", |p| Arc::make_mut(&mut p.slides[2]).name = "B".into());
    let own = |s: &SlideSession| {
        let c = &s.slide_file().slides[0].comments[0];
        s.model().ids().comments_object(&c.id).unwrap_or_else(|| panic!("converted")).clone()
    };
    let (thread_a, thread_b) = (own(&a), own(&b));
    // The replica whose thread the merge keeps sees the other's first and merges it away; the
    // other, not knowing yet, replies in its own thread.
    let (keeper, other) = if thread_a.as_str() < thread_b.as_str() { (&mut a, &mut b) } else { (&mut b, &mut a) };
    let since = keeper.version();
    keeper.import_changes(&other.export_changes(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    record_with(other, "cy", |p| Arc::make_mut(&mut p.slides[0]).comments[0].replies.push(reply("Cy", "Late reply", "2026-10-19T12:00:00Z")));
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        let replies: Vec<&str> = s.slide_file().slides[0].comments[0].replies.iter().map(|r| r.text.as_str()).collect();
        assert!(replies.contains(&"Late reply") && replies.contains(&"Old reply"), "{replies:?}");
        assert_eq!(s.slide_file().slides[0].comments.len(), 2);
        view_is_replica(s);
    }
}
