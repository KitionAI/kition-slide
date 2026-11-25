//! Comment anchors (Kit UI protocol 2.6 section 7k): what the editor sends a host that keeps
//! comment threads (`comment.create`) and where it marks them again (`comment.place`), through
//! slide reorders, shape moves, deletions, undo, and another person's replica.

use kition_slide_engine::{Session, Version};
use serde_json::{Value, json};

/// Three blank slides; the second holds a rectangle with text and an ellipse, and is current.
fn session() -> Session {
    let mut s = Session::with_new();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("slide.go", &json!({"index": 1})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 80], "text": "Northern lights"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [400, 100, 100, 80]})).unwrap();
    s
}

/// The engine id of the shape whose text is `text` on the current slide.
fn shape_with(s: &mut Session, text: Option<&str>) -> u64 {
    let slide = s.execute("slide.inspect", &json!({})).unwrap();
    let shapes = slide["shapes"].as_array().unwrap();
    let found = shapes.iter().find(|shape| shape["text"].as_str().filter(|t| !t.is_empty()) == text).unwrap();
    found["id"].as_u64().unwrap()
}

fn place(s: &mut Session, anchor: &Value) -> Value {
    let placed = s.execute("comment.place", &json!({ "anchors": [anchor] })).unwrap();
    placed[0].clone()
}

fn ids(anchor: &Value) -> Vec<String> {
    anchor["ids"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

#[test]
fn a_shape_anchor_follows_its_shape_through_reorder_and_move() {
    let mut s = session();
    let rect = shape_with(&mut s, Some("Northern lights"));
    s.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();
    let list = ids(&anchor);
    assert_eq!(list.len(), 2, "the shape, then its slide: {anchor}");
    assert_eq!(anchor["page"], 1);
    assert!(anchor.get("range").is_none());
    assert!(list.iter().all(|id| id.parse::<u64>().is_err()), "object ids, not engine numbers: {list:?}");

    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].as_u64()), (Some(true), Some(1)), "{at}");
    assert_eq!((at["rect"]["x"].as_f64(), at["rect"]["y"].as_f64(), at["rect"]["width"].as_f64()), (Some(100.0), Some(100.0), Some(200.0)));

    // The slide moves to the end: the anchor goes with it.
    s.execute("slide.move", &json!({"from": 1, "to": 2})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].as_u64()), (Some(true), Some(2)), "{at}");

    // The shape moves: its marker follows.
    s.execute("slide.go", &json!({"index": 2})).unwrap();
    let rect = shape_with(&mut s, Some("Northern lights"));
    s.execute("shape.move", &json!({"ids": [rect], "dx": 50, "dy": 30})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["rect"]["x"].as_f64(), at["rect"]["y"].as_f64()), (Some(150.0), Some(130.0)), "{at}");

    // Placed from any slide, not only the current one.
    s.execute("slide.go", &json!({"index": 0})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["slide"].as_u64(), at["rect"]["x"].as_f64()), (Some(2), Some(150.0)), "{at}");
}

#[test]
fn a_deleted_shape_leaves_its_thread_on_the_slide_and_undo_brings_it_back() {
    let mut s = session();
    let rect = shape_with(&mut s, Some("Northern lights"));
    s.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();

    s.execute("edit.delete", &json!({"ids": [rect]})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!(at["found"], false, "the commented shape is gone: {at}");
    assert_eq!((at["slide"].as_u64(), at["rect"].is_null()), (Some(1), true), "the thread stays on its slide: {at}");

    s.execute("edit.undo", &json!({})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].as_u64(), at["rect"]["x"].as_f64()), (Some(true), Some(1), Some(100.0)), "{at}");

    // The slide goes: nothing is left to mark.
    s.execute("slide.delete", &json!({"index": 1})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].is_null()), (Some(false), true), "{at}");
}

#[test]
fn several_shapes_mark_at_the_first_that_still_exists() {
    let mut s = session();
    s.execute("edit.selectAll", &json!({})).unwrap();
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();
    assert_eq!(ids(&anchor).len(), 3, "two shapes and their slide: {anchor}");
    let first = shape_with(&mut s, Some("Northern lights"));
    s.execute("edit.delete", &json!({"ids": [first]})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["slide"].as_u64(), at["rect"]["x"].as_f64()), (Some(1), Some(400.0)), "the ellipse: {at}");
}

#[test]
fn selected_text_is_a_range_and_a_quote_in_its_shape() {
    let mut s = session();
    let rect = shape_with(&mut s, Some("Northern lights"));
    s.execute("text.edit", &json!({"id": rect})).unwrap();
    s.execute("text.select", &json!({"anchor": [0, 15], "caret": [0, 9]})).unwrap();
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();
    assert_eq!(anchor["range"], json!({"start": 9, "end": 15}), "{anchor}");
    assert_eq!(anchor["quote"], "lights");
    assert_eq!(ids(&anchor).len(), 2, "{anchor}");
    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["rect"]["x"].as_f64()), (Some(true), Some(100.0)), "{at}");
}

#[test]
fn a_point_on_the_slide_is_kept_with_the_slide() {
    let mut s = session();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let anchor = s.execute("comment.anchor", &json!({"x": 320.4, "y": 210})).unwrap();
    let list = ids(&anchor);
    assert_eq!(list.get(1).map(String::as_str), Some("pt:320,210"), "{anchor}");
    s.execute("slide.move", &json!({"from": 1, "to": 0})).unwrap();
    let at = place(&mut s, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].as_u64()), (Some(true), Some(0)), "{at}");
    assert_eq!(at["point"], json!({"x": 320.0, "y": 210.0}));

    // Nothing selected and no point: the slide.
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();
    assert_eq!(ids(&anchor).len(), 1, "{anchor}");
}

#[test]
fn another_persons_replica_places_the_same_anchor() {
    let mut a = session();
    let rect = shape_with(&mut a, Some("Northern lights"));
    a.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    let anchor = a.execute("comment.anchor", &json!({})).unwrap();

    let mut b = Session::new();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    b.execute("slide.move", &json!({"from": 1, "to": 0})).unwrap();
    let at = place(&mut b, &anchor);
    assert_eq!((at["found"].as_bool(), at["slide"].as_u64(), at["rect"]["x"].as_f64()), (Some(true), Some(0), Some(100.0)), "{at}");
}

#[test]
fn malformed_and_unknown_anchors_are_not_found() {
    let mut s = session();
    let placed = s.execute("comment.place", &json!({"anchors": [{"ids": ["no-such-object"]}, {"ids": []}, {}, "x"]})).unwrap();
    for at in placed.as_array().unwrap() {
        assert_eq!((at["found"].as_bool(), at["slide"].is_null()), (Some(false), true), "{at}");
    }
    assert_eq!(s.execute("comment.place", &json!({})).unwrap(), json!([]));
}

#[test]
fn placing_never_makes_the_replica_and_a_new_comment_does() {
    let mut s = Session::with_new();
    let placed = s.execute("comment.place", &json!({"anchors": [{"ids": ["x"]}]})).unwrap();
    assert_eq!(placed[0]["found"], false);
    assert!(s.kdm().is_none(), "placing anchors made a replica");
    let anchor = s.execute("comment.anchor", &json!({})).unwrap();
    assert!(s.kdm().is_some());
    assert_eq!(place(&mut s, &anchor)["found"], true, "{anchor}");
}

#[test]
fn outside_the_slides_there_is_no_anchor() {
    let mut s = session();
    s.execute("view.slideMaster", &json!({})).unwrap();
    assert_eq!(s.execute("comment.anchor", &json!({})).unwrap(), Value::Null);
}
