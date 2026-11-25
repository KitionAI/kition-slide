//! A draft and the document each add a slide (an agent's draft made while the person adds one):
//! the merge keeps both slides, each with a file id of its own, settles at once, and undoes and
//! redoes exactly.
//!
//! Both new slides take the same next number in their own replica, so after the merge both carry
//! the same file id (`Slide::file_id`, the id the file writes the slide with). The engine's repair
//! after the merge (`stored::merged_file_ids`) gives the repeat the next id above every slide's,
//! as part of the merge's undo step.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_engine::Origin;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Deck.kslide";

/// The replicated state, for comparing states.
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
    KitDocument::execute(doc, "file.inspect", json!({})).unwrap()["slides"].as_array().unwrap().len()
}

/// A new deck's document and a draft of it: both open the file with the same stored replica, so
/// the draft is at the document's version.
fn pair() -> (SlideDocument, SlideDocument) {
    let file = SlideDocument::create(NAME, 960, 540).unwrap().save(None).unwrap();
    let mut seed = SlideDocument::open(NAME, &file).unwrap();
    let (snapshot, blobs) = (seed.snapshot().unwrap(), seed.blobs().unwrap());
    let doc = SlideDocument::open_with_replica(NAME, &file, &snapshot, &blobs).unwrap();
    let draft = SlideDocument::open_with_replica(NAME, &file, &snapshot, &blobs).unwrap();
    (doc, draft)
}

#[test]
fn a_draft_and_the_document_adding_a_slide_at_once_keep_a_file_id_each() {
    for reverse in [false, true] {
        let (mut a, mut b) = pair();
        let (doc, draft) = if reverse { (&mut b, &mut a) } else { (&mut a, &mut b) };
        let base = doc.version().unwrap();
        assert_eq!(draft.version().unwrap(), base, "the draft is made at the document's version");
        draft.execute("slide.new", json!({})).unwrap();
        doc.execute("slide.new", json!({})).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let mine = file_ids(doc);
        assert_eq!(mine.len(), 2);
        let before = state(doc);
        doc.merge_changes(&changes, &Origin::agent("agent:drafter")).unwrap();
        assert_eq!(slides(doc), 3, "both new slides are kept");
        // Every slide keeps a file id of its own: the document's two as they were, the draft's a new one.
        let ids = file_ids(doc);
        let mut distinct = ids.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), 3, "{ids:?}");
        assert!(ids.iter().all(Value::is_u64), "{ids:?}");
        assert!(mine.iter().all(|id| ids.contains(id)), "the document's slides keep theirs: {mine:?} in {ids:?}");
        // Settled: a read writes nothing, and the deck reopens with its replica as it is.
        let merged = state(doc);
        let version = doc.version().unwrap();
        KitDocument::execute(doc, "file.inspect", json!({})).unwrap();
        assert_eq!(doc.version().unwrap(), version, "a read after the merge wrote a repair");
        let (file, snapshot, blobs) = (doc.save(None).unwrap(), doc.snapshot().unwrap(), doc.blobs().unwrap());
        let mut reopened = SlideDocument::open_with_replica(NAME, &file, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.version().unwrap(), version, "reopening wrote a repair");
        assert_eq!(state(&mut reopened), merged);
        // One undo takes the merge and its repair back, exactly; the redo brings both back.
        assert!(doc.undo().unwrap().0);
        assert_eq!(state(doc), before, "the undo gives back the exact state before the merge");
        assert_eq!(slides(doc), 2);
        assert!(doc.redo().unwrap().0);
        assert_eq!(state(doc), merged, "the redo gives back the merged state");
        assert_eq!(file_ids(doc), ids);
    }
}
