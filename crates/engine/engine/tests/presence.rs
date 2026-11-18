//! Presence (Kit UI protocol section 7e): the selection travels as KDM object ids, and a peer
//! places the shapes another person selected on its own view of the same slide.

use kition_slide_engine::{Session, Version};
use serde_json::{Value, json};

/// A session with a blank slide holding two shapes, the second slide current.
fn new_session() -> Session {
    let mut s = Session::with_new();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 80], "text": "A"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [300, 100, 100, 80], "text": "B"})).unwrap();
    s
}

/// A second session holding a replica of `a`'s active slide.
fn peer(a: &mut Session) -> Session {
    let mut b = Session::new();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    b
}

fn objects(value: &Value) -> Vec<String> {
    value["objects"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

#[test]
fn the_selection_travels_as_object_ids_that_a_peer_places_on_its_slide() {
    let mut a = new_session();
    a.execute("edit.selectAll", &json!({})).unwrap();
    let shared = a.execute("presence.selection", &json!({})).unwrap();
    let selected = objects(&shared);
    assert_eq!(selected.len(), 2, "{shared}");
    assert!(shared["slide"].is_string());

    let mut b = peer(&mut a);
    b.execute("slide.go", &json!({"index": 1})).unwrap();
    assert_eq!(b.execute("presence.selection", &json!({})).unwrap()["slide"], shared["slide"]);
    let placed = b.execute("presence.locate", &json!({"ids": [selected[0], "not-an-object"]})).unwrap();
    let placed = placed.as_array().unwrap();
    assert_eq!(placed.len(), 1, "{placed:?}");
    assert_eq!(placed[0]["id"], selected[0]);
    assert_eq!((placed[0]["x"].as_f64(), placed[0]["y"].as_f64(), placed[0]["width"].as_f64()), (Some(100.0), Some(100.0), Some(100.0)));

    // On another slide, nothing of it is placed.
    b.execute("slide.go", &json!({"index": 0})).unwrap();
    assert_eq!(b.execute("presence.locate", &json!({"ids": selected})).unwrap(), json!([]));
}

#[test]
fn nothing_is_shared_before_the_document_has_a_replica() {
    let mut s = Session::with_new();
    assert_eq!(s.execute("presence.selection", &json!({})).unwrap(), json!({ "slide": null, "objects": [] }));
    assert_eq!(s.execute("presence.locate", &json!({"ids": ["x"]})).unwrap(), json!([]));
}
