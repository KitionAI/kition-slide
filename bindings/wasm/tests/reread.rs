//! C8-slide-pin: a clean session reads its saved file again without losing its undo history
//! (kition-sdk `KitDocument::reconcile_reread`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Reread.pptx";

fn texts(doc: &mut SlideDocument) -> Vec<String> {
    let seen = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    seen["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|shape| shape["text"].as_str().map(str::to_owned))
        .filter(|text| text.starts_with("Mine") || text.starts_with("Theirs"))
        .collect()
}

fn id_of(doc: &mut SlideDocument, text: &str) -> Value {
    let seen = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    seen["shapes"].as_array().unwrap().iter().find(|shape| shape["text"] == text).unwrap()["toolId"].clone()
}

fn deck() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    doc.execute("insert.textBox", json!({"text": "Mine", "rect": [10, 10, 200, 50]})).unwrap();
    doc.execute("insert.textBox", json!({"text": "Theirs", "rect": [10, 100, 200, 50]})).unwrap();
    doc.save(Some(NAME)).unwrap()
}

/// The session's own saved edit stays undoable after another program's edit is read in, and its
/// undo keeps that edit.
#[test]
fn a_reread_keeps_the_sessions_undo_and_the_files_edit() {
    let mut live = SlideDocument::open(NAME, &deck()).unwrap();
    let mine = id_of(&mut live, "Mine");
    live.execute("text.set", json!({"id": mine, "text": "Mine edited"})).unwrap();
    let saved = live.save(Some(NAME)).unwrap();
    live.mark_saved();

    let mut other = SlideDocument::open(NAME, &saved).unwrap();
    let theirs = id_of(&mut other, "Theirs");
    other.execute("text.set", json!({"id": theirs, "text": "Theirs edited"})).unwrap();
    let mut fresh = SlideDocument::open(NAME, &other.save(Some(NAME)).unwrap()).unwrap();
    fresh.restore_editor_state(&live.editor_state()).unwrap();

    assert_eq!(live.reconcile_reread(&fresh), Ok(true));
    assert!(!live.dirty(), "the reread session holds what the file holds");
    assert_eq!(texts(&mut live), ["Mine edited", "Theirs edited"]);
    live.execute("edit.undo", json!({})).unwrap();
    assert_eq!(texts(&mut live), ["Mine", "Theirs edited"], "undo takes back the session's edit only");
}

/// A file saved again unchanged changes nothing and keeps the history.
#[test]
fn an_unchanged_reread_keeps_everything() {
    let mut live = SlideDocument::open(NAME, &deck()).unwrap();
    let mine = id_of(&mut live, "Mine");
    live.execute("text.set", json!({"id": mine, "text": "Mine edited"})).unwrap();
    let saved = live.save(Some(NAME)).unwrap();
    live.mark_saved();
    let fresh = SlideDocument::open(NAME, &saved).unwrap();
    assert_eq!(live.reconcile_reread(&fresh), Ok(true));
    assert_eq!(id_of(&mut live, "Mine edited"), mine, "identities survive the reread");
    live.execute("edit.undo", json!({})).unwrap();
    assert_eq!(texts(&mut live), ["Mine", "Theirs"]);
}
