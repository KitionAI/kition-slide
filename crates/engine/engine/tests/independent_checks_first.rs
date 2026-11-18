//! C8-slide-read-3: engine defects the independent reading found, a command that checks what it
//! acts on only after it already wrote (an orphan media item or a stray shape the failed call
//! leaves in the deck), and a picture command that changed shapes that are not pictures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_slide_engine::Session;
use kition_slide_model::ShapeKind;
use serde_json::json;

const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";

fn deck() -> Session {
    let mut session = Session::with_new();
    session.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    session
}

fn media_count(session: &Session) -> usize {
    session.active().unwrap().doc.media.len()
}

#[test]
fn picture_change_with_a_missing_target_leaves_no_media() {
    let mut session = deck();
    let before = media_count(&session);
    assert!(session.execute("picture.change", &json!({"data": RED, "name": "red.png", "ids": ["999"]})).is_err());
    assert_eq!(media_count(&session), before, "the refused change added a media item");
}

#[test]
fn shape_fill_picture_with_a_missing_target_leaves_no_media() {
    let mut session = deck();
    let before = media_count(&session);
    assert!(session.execute("shape.fill", &json!({"picture": RED, "id": 999})).is_err());
    assert_eq!(media_count(&session), before, "the refused fill added a media item");
}

#[test]
fn a_one_item_diagram_is_refused_without_a_stray_shape() {
    let mut session = deck();
    let before = session.active().unwrap().doc.slides[1].shapes.len();
    let answer = session.execute("insert.smartGraphic", &json!({"kind": "process", "items": ["A"]}));
    let after = session.active().unwrap().doc.slides[1].shapes.len();
    assert!(answer.is_ok() || after == before, "a failed diagram left {} shape(s)", after - before);
}

#[test]
fn picture_reset_leaves_a_selected_rectangle_alone() {
    let mut session = deck();
    let rect = session.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    session.execute("shape.line", &json!({"ids": [rect], "color": "#FF0000", "width": 3})).unwrap();
    let pic = session.execute("insert.picture", &json!({"data": RED, "name": "red.png", "rect": [200, 10, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    let line = |session: &Session| session.active().unwrap().doc.slides[1].shapes.iter().find(|s| u64::from(s.id.0) == rect).unwrap().line.clone();
    let before = line(&session);
    session.execute("picture.reset", &json!({"ids": [rect, pic]})).unwrap();
    assert_eq!(line(&session), before, "picture.reset removed a rectangle's outline");
    let picture = session.active().unwrap().doc.slides[1].shapes.iter().find(|s| u64::from(s.id.0) == pic).unwrap();
    assert!(matches!(picture.kind, ShapeKind::Picture { .. }));
}

#[test]
fn a_comment_anchored_on_a_shape_its_slide_lacks_is_refused() {
    let mut session = deck();
    let rect = session.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let before: usize = session.active().unwrap().doc.slides.iter().map(|slide| slide.comments.len()).sum();
    // The rectangle is on slide index 1; slide index 0 has no shape with its number.
    assert!(session.execute("comment.add", &json!({"text": "x", "slide": 0, "id": rect})).is_err(), "the anchor was silently dropped");
    let after: usize = session.active().unwrap().doc.slides.iter().map(|slide| slide.comments.len()).sum();
    assert_eq!(after, before, "a refused comment was added");
    session.execute("comment.add", &json!({"text": "y", "slide": 1, "id": rect})).unwrap();
    assert_eq!(session.active().unwrap().doc.slides[1].comments.last().unwrap().shape.map(|id| u64::from(id.0)), Some(rect));
}
