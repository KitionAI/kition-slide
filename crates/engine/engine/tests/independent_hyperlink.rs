#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_slide_engine::Session;
use kition_slide_model::{ShapeId, text::Action};
use serde_json::json;

#[test]
fn explicit_hyperlink_target_works_with_an_empty_selection() {
    let mut session = Session::with_new();
    let id = session.execute("shape.insert", &json!({"preset": "rect", "text": "Named shape"})).unwrap()["id"].as_u64().unwrap();
    session.execute("edit.deselect", &json!({})).unwrap();
    assert!(session.doc().unwrap().selection.shapes.is_empty());
    session.execute("insert.hyperlink", &json!({"ids": [id], "url": "https://example.org/target"})).unwrap();
    let state = session.doc().unwrap();
    let shape = state.shape(ShapeId(id as u32)).unwrap();
    assert_eq!(shape.click.as_ref().unwrap().action, Action::Url { url: "https://example.org/target".into() });
    assert!(state.selection.shapes.is_empty());
}

#[test]
fn explicit_hyperlink_target_takes_priority_over_another_shapes_text() {
    for single in [false, true] {
        let mut session = Session::with_new();
        let named = session.execute("shape.insert", &json!({"preset": "rect", "text": "Named shape"})).unwrap()["id"].as_u64().unwrap();
        let edited = session.execute("shape.insert", &json!({"preset": "rect", "text": "Edited text"})).unwrap()["id"].as_u64().unwrap();
        session.execute("text.edit", &json!({"id": edited, "at": [0, 0]})).unwrap();
        let before = session.doc().unwrap().shape(ShapeId(edited as u32)).unwrap().clone();
        let mut params = json!({"url": "https://example.org/named"});
        params[if single { "id" } else { "ids" }] = if single { json!(named) } else { json!([named]) };
        session.execute("insert.hyperlink", &params).unwrap();
        let state = session.doc().unwrap();
        assert_eq!(state.shape(ShapeId(edited as u32)).unwrap(), &before, "the text being edited belongs to another shape");
        assert_eq!(
            state.shape(ShapeId(named as u32)).unwrap().click.as_ref().unwrap().action,
            Action::Url { url: "https://example.org/named".into() }
        );
        assert_eq!(state.selection.text.as_ref().unwrap().shape, ShapeId(edited as u32));
    }
}
