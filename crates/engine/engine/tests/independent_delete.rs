//! C8-slide-read: explicit shape addresses must override an unrelated text selection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_slide_engine::Session;
use kition_slide_model::ShapeId;
use serde_json::{Value, json};

fn editing_first_of_two() -> (Session, ShapeId, ShapeId) {
    let mut session = Session::with_new();
    let mut insert = |text: &str| {
        ShapeId(session.execute("insert.textBox", &json!({"text": text, "rect": [40, 40, 180, 30]})).unwrap()["id"].as_u64().unwrap() as u32)
    };
    let first = insert("First text");
    let second = insert("Second text");
    session.execute("text.edit", &json!({"id": first.0, "at": [0, 0]})).unwrap();
    session.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 5]})).unwrap();
    (session, first, second)
}

fn first_text(session: &Session, id: ShapeId) -> String {
    session.active().unwrap().shape(id).unwrap().text.as_ref().unwrap().text()
}

#[test]
fn delete_explicit_shape_preserves_another_shapes_edited_text() {
    for params in [json!({"id": "SECOND"}), json!({"ids": ["SECOND"]})] {
        let (mut session, first, second) = editing_first_of_two();
        let params = match params.get("ids") {
            Some(_) => json!({"ids": [second.0]}),
            None => json!({"id": second.0}),
        };
        session.execute("edit.delete", &params).unwrap();
        assert_eq!(first_text(&session, first), "First text", "explicit target must preserve other edited text");
        assert!(session.active().unwrap().shape(second).is_none(), "the explicitly addressed shape must be deleted");
    }
}

#[test]
fn cut_explicit_shape_copies_and_deletes_that_shape() {
    for key in ["id", "ids"] {
        let (mut session, first, second) = editing_first_of_two();
        let value = if key == "id" { json!(second.0) } else { json!([second.0]) };
        let result = session.execute("edit.cut", &json!({key: value})).unwrap();
        assert_eq!(result["text"], "Second text", "cut must copy its explicitly addressed shape");
        assert_eq!(first_text(&session, first), "First text");
        assert!(session.active().unwrap().shape(second).is_none());
        assert_eq!(session.clipboard.shapes.len(), 1);
        assert_eq!(session.clipboard.shapes[0].id, second);
        assert_eq!(session.clipboard.plain, "Second text");
    }
}

#[test]
fn unnamed_delete_and_cut_still_edit_the_text_range() {
    for command in ["edit.delete", "edit.cut"] {
        let (mut session, first, second) = editing_first_of_two();
        let result = session.execute(command, &json!({})).unwrap();
        assert_eq!(first_text(&session, first), " text");
        assert_eq!(first_text(&session, second), "Second text");
        if command == "edit.cut" {
            assert_eq!(result.get("text"), Some(&Value::String("First".into())));
        }
    }
}
