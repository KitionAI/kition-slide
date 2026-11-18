#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_slide_engine::Session;
use serde_json::json;

#[test]
fn failed_animation_paint_keeps_the_nonsticky_buffer_for_retry() {
    let mut session = Session::with_new();
    let source = session.execute("shape.insert", &json!({"preset": "rect", "text": "Source"})).unwrap()["id"].clone();
    let target = session.execute("shape.insert", &json!({"preset": "rect", "text": "Target"})).unwrap()["id"].clone();
    session.execute("animation.add", &json!({"ids": [source], "effect": "fade"})).unwrap();
    session.execute("animation.painter", &json!({"id": source})).unwrap();
    let before = session.doc().unwrap().doc.clone();
    assert!(session.execute("animation.painterApply", &json!({"ids": [0]})).is_err());
    assert_eq!(session.doc().unwrap().doc, before);
    let applied = session.execute("animation.painterApply", &json!({"ids": [target]})).expect("failed validation must leave the painter available");
    assert_eq!(applied["applied"], 1);
    assert!(session.execute("animation.painterApply", &json!({"ids": [target]})).is_err(), "a successful nonsticky paint consumes the buffer");
}
