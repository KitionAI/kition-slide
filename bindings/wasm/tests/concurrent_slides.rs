//! Two editors of one room add a slide at the same time: their replicas converge, and stop
//! writing once they have.
//!
//! Both new slides take the same next number in their own editor, so after the merge both carry
//! the same file id (`Slide::file_id`, the id the file writes the slide with). Each replica gave
//! the repeat its own view's number for that slide, and the two replicas number the two slides the
//! other way round: every import made a repair the other replica undid, without end, and a room
//! never settled.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Deck.kslide";

/// The replicated state, for comparing two replicas.
fn state(doc: &mut SlideDocument) -> Value {
    Exchange::new().replica(doc).unwrap().state_json().unwrap()
}

fn file_ids(doc: &mut SlideDocument) -> Vec<Value> {
    let state = state(doc);
    let mut ids: Vec<Value> = state["objects"]
        .as_object()
        .map(|objects| objects.values().filter(|object| object["$type"] == "Slide").map(|slide| slide["props"]["fileId"].clone()).collect())
        .unwrap_or_default();
    ids.sort_by_key(Value::to_string);
    ids
}

fn slides(doc: &mut SlideDocument) -> usize {
    KitDocument::execute(doc, "document.inspect", json!({})).unwrap()["slides"].as_array().unwrap().len()
}

/// What each lacks of the other, both taken before either imports (`b` first): two pushes that
/// cross in a room.
fn cross(a: &mut SlideDocument, b: &mut SlideDocument) {
    let (since_a, since_b) = (a.version().unwrap(), b.version().unwrap());
    let (to_b, to_a) = (a.export_changes(&since_b).unwrap(), b.export_changes(&since_a).unwrap());
    let (blobs_b, blobs_a) = (a.blobs_since(&since_b).unwrap(), b.blobs_since(&since_a).unwrap());
    b.add_blobs(&blobs_b).unwrap();
    b.import_changes(&to_b).unwrap();
    a.add_blobs(&blobs_a).unwrap();
    a.import_changes(&to_a).unwrap();
}

/// Two editors of a new deck's room: both open the file with the room's snapshot.
fn pair() -> (SlideDocument, SlideDocument) {
    let file = SlideDocument::create(NAME, 960, 540).unwrap().save(None).unwrap();
    let mut seed = SlideDocument::open(NAME, &file).unwrap();
    let (snapshot, blobs) = (seed.snapshot().unwrap(), seed.blobs().unwrap());
    let a = SlideDocument::open_with_replica(NAME, &file, &snapshot, &blobs).unwrap();
    let b = SlideDocument::open_with_replica(NAME, &file, &snapshot, &blobs).unwrap();
    (a, b)
}

#[test]
fn two_editors_add_a_slide_at_once_and_their_replicas_settle() {
    for reverse in [false, true] {
        let (mut a, mut b) = pair();
        let (a, b) = if reverse { (&mut b, &mut a) } else { (&mut a, &mut b) };
        a.execute("slide.new", json!({})).unwrap();
        b.execute("slide.new", json!({})).unwrap();
        // The new slides, then each editor's repairs, sent while the other's are on their way.
        for _ in 0..6 {
            cross(a, b);
        }
        assert_eq!(state(a), state(b), "the replicas converged");
        assert_eq!((slides(a), slides(b)), (3, 3));
        // Once converged, an exchange writes nothing more.
        let (va, vb) = (a.version().unwrap(), b.version().unwrap());
        cross(a, b);
        assert!(a.version().unwrap() == va && b.version().unwrap() == vb, "an exchange after converging wrote a repair");
        // Every slide keeps a file id of its own.
        let ids = file_ids(a);
        let mut distinct = ids.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), 3, "{ids:?}");
        assert!(ids.iter().all(Value::is_u64), "{ids:?}");
    }
}
