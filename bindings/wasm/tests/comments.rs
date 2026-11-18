//! Comment threads between two replicas of one slide, driven as a host drives the
//! editor's engine (`SlideDocument`'s commands and contract 2 members): concurrent comments on one
//! thread are all kept, and a commenter's changes touch only comment objects (what a sync server
//! takes from the `commenter` role, kition-sync 1.6).

use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::KitModel;
use kition_kit::tools::{KitTools, PointerAction, ToolMods, ToolPointer};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Slide.kslide";

fn run(doc: &mut SlideDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// A slide file with a shape on its first slide, and a second replica of it.
fn two_replicas() -> (SlideDocument, SlideDocument) {
    let mut a = SlideDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("{error}"));
    run(&mut a, "shape.insert", json!({ "preset": "rect", "rect": [100, 300, 120, 80], "text": "box" }));
    let snapshot = a.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let blobs = a.blobs().unwrap_or_else(|error| panic!("{error}"));
    let b = SlideDocument::open_with_replica(NAME, &[], &snapshot, &blobs).unwrap_or_else(|error| panic!("{error}"));
    assert!(b.has_replica());
    (a, b)
}

fn send(from: &mut SlideDocument, to: &mut SlideDocument) {
    let since = to.version().unwrap_or_else(|error| panic!("{error}"));
    let changes = from.export_changes(&since).unwrap_or_else(|error| panic!("{error}"));
    to.add_blobs(&from.blobs_since(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    to.import_changes(&changes).unwrap_or_else(|error| panic!("{error}"));
}

fn sync(a: &mut SlideDocument, b: &mut SlideDocument) {
    for _ in 0..4 {
        send(a, b);
        send(b, a);
    }
    assert_eq!(a.version().ok(), b.version().ok(), "the replicas converged");
}

/// The replicated state's hash, from a snapshot.
fn state_hash(doc: &mut SlideDocument) -> String {
    let snapshot = doc.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let replica = Replica::load(<kdm_model::SlideModel as KitModel>::schema(), &snapshot, ActorId::from_u64(40).unwrap_or_else(|_| panic!("actor")))
        .unwrap_or_else(|error| panic!("{error}"));
    replica.state_hash().unwrap_or_else(|error| panic!("{error}"))
}

fn threads(doc: &mut SlideDocument) -> Vec<Value> {
    run(doc, "comment.threads", json!({})).as_array().cloned().unwrap_or_default()
}

#[test]
fn concurrent_comments_on_one_thread_are_all_kept() {
    let (mut a, mut b) = two_replicas();
    let opened = run(&mut a, "comment.add", json!({ "text": "One", "author": "user:ann", "name": "Ann", "created": "2026-10-19T10:00:00Z" }));
    let thread = opened["thread"].as_str().unwrap_or_default().to_owned();
    sync(&mut a, &mut b);
    // Offline, both reply to the thread, one resolves it, and each opens a thread of their own.
    run(&mut a, "comment.reply", json!({ "thread": thread, "text": "Two", "author": "user:ann", "name": "Ann", "created": "2026-10-19T10:01:00Z" }));
    run(
        &mut b,
        "comment.reply",
        json!({ "thread": thread, "text": "Three", "author": "user:bob", "name": "Bob", "created": "2026-10-19T10:01:00Z" }),
    );
    run(&mut b, "comment.resolve", json!({ "thread": thread }));
    run(&mut a, "comment.add", json!({ "text": "A's own", "author": "user:ann", "name": "Ann", "created": "2026-10-19T10:02:00Z" }));
    run(&mut b, "comment.add", json!({ "text": "B's own", "author": "user:bob", "name": "Bob", "created": "2026-10-19T10:03:00Z" }));
    sync(&mut a, &mut b);
    let (ta, tb) = (threads(&mut a), threads(&mut b));
    // Local ids (a shape's, a slide's) are each engine's own; the rest is the same.
    let shared_part = |list: &[Value]| {
        list.iter()
            .map(|t| (t["id"].clone(), t["resolved"].clone(), t["comments"].clone(), t["anchor"]["box"].clone(), t["anchor"]["x"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(shared_part(&ta), shared_part(&tb), "both replicas show the same threads");
    let bodies = |t: &Value| {
        t["comments"].as_array().map(|c| c.iter().map(|c| c["body"].as_str().unwrap_or_default().to_owned()).collect::<Vec<_>>()).unwrap_or_default()
    };
    let shared = ta.iter().find(|t| t["id"] == json!(thread)).unwrap_or_else(|| panic!("the thread"));
    let mut replies = bodies(shared);
    replies.sort();
    assert_eq!(replies, vec!["One", "Three", "Two"], "both replies are kept");
    assert_eq!(shared["resolved"], json!(true));
    let mut opened: Vec<String> = ta.iter().map(|t| bodies(t)[0].clone()).collect();
    opened.sort();
    assert_eq!(opened, vec!["A's own", "B's own", "One"]);
    assert_eq!(state_hash(&mut a), state_hash(&mut b));
}

#[test]
fn a_commenter_changes_only_comment_objects() {
    let (mut a, mut b) = two_replicas();
    let opened = run(&mut a, "comment.add", json!({ "text": "Please check", "author": "user:ann", "name": "Ann" }));
    let thread = opened["thread"].as_str().unwrap_or_default().to_owned();
    sync(&mut a, &mut b);
    b.set_comment_only("user:bob");
    let base = b.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let since = b.version().unwrap_or_else(|error| panic!("{error}"));

    // Edits of the slides are refused, by command and by the engine tools.
    assert!(b.execute("shape.insert", json!({ "preset": "rect", "rect": [10, 10, 40, 40] })).is_err());
    assert!(b.execute("slide.new", json!({})).is_err());
    let press = ToolPointer { action: PointerAction::Down, x: 120.0, y: 120.0, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 };
    assert!(b.pointer(&press).is_err());
    assert!(KitTools::text(&mut b, "typed", 1.0).is_err());
    // Comments are not.
    run(&mut b, "comment.reply", json!({ "thread": thread, "text": "Checked", "name": "Bob" }));
    run(&mut b, "comment.add", json!({ "text": "One more thing", "name": "Bob" }));
    run(&mut b, "comment.resolve", json!({ "thread": thread }));
    assert!(!b.dirty(), "a commenter has nothing to save");

    // What B sends touches only CommentThread and Comment objects, and every comment B wrote
    // names B as its author.
    let changes = b.export_changes(&since).unwrap_or_else(|error| panic!("{error}"));
    let schema = <kdm_model::SlideModel as KitModel>::schema();
    let mut replica =
        Replica::load(schema.clone(), &base, ActorId::from_u64(41).unwrap_or_else(|_| panic!("actor"))).unwrap_or_else(|error| panic!("{error}"));
    let min = replica.min_schema_version();
    let changed = replica.import_changes(&changes).unwrap_or_else(|error| panic!("{error}"));
    assert!(!changed.is_empty());
    assert_eq!(replica.min_schema_version(), min);
    for object in &changed {
        let ty = replica.object_type(object).unwrap_or_else(|error| panic!("{object}: {error}"));
        assert!(ty == "CommentThread" || ty == "Comment", "the commenter's change touched {ty} {object}");
        if ty == "Comment" && replica.text(object, "body").unwrap_or_default() != "Please check" {
            assert_eq!(replica.get(object, "author").unwrap_or_default(), json!("user:bob"));
        }
    }
    sync(&mut a, &mut b);
    let listed = threads(&mut a);
    assert_eq!(listed.len(), 2);
    assert_eq!(state_hash(&mut a), state_hash(&mut b));
}
