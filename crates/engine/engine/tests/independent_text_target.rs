//! C8-slide-read: explicit targets and selection fallbacks name the same edited object.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_slide_engine::Session;
use kition_slide_model::{ShapeId, ShapeKind};
use serde_json::json;

#[test]
fn table_merge_does_not_borrow_another_tables_selected_coordinates() {
    let mut session = Session::with_new();
    let a = session.execute("table.insert", &json!({"rows": 2, "cols": 2})).unwrap()["id"].clone();
    let b = session.execute("table.insert", &json!({"rows": 2, "cols": 2})).unwrap()["id"].clone();
    session.execute("table.selectCells", &json!({"id": a, "from": [0, 0], "to": [1, 1]})).unwrap();
    let result = session.execute("table.merge", &json!({"id": b}));
    assert!(result.is_err(), "the explicit target has no selected cell range: {result:?}");
    let shape = session.doc().unwrap().shape(ShapeId(b.as_u64().unwrap() as u32)).unwrap();
    let ShapeKind::Table(table) = &shape.kind else { panic!("not a table") };
    assert_eq!(table.cell(0, 0).unwrap().grid_span, 1);
    assert_eq!(table.cell(0, 0).unwrap().row_span, 1);
}

#[test]
fn painter_explicit_id_overrides_another_shapes_text_range() {
    let mut session = Session::with_new();
    let source = session.execute("textBox.insert", &json!({"text": "Source"})).unwrap()["id"].clone();
    let a = session.execute("textBox.insert", &json!({"text": "Other"})).unwrap()["id"].clone();
    let b = session.execute("textBox.insert", &json!({"text": "Target"})).unwrap()["id"].clone();
    session.execute("format.bold", &json!({"ids": [source], "on": true})).unwrap();
    session.execute("edit.select", &json!({"ids": [source]})).unwrap();
    session.execute("format.painter", &json!({})).unwrap();
    session.execute("text.edit", &json!({"id": a})).unwrap();
    session.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 5]})).unwrap();
    session.execute("format.painterApply", &json!({"id": b})).unwrap();
    let state = session.doc().unwrap();
    let bold =
        |id: &serde_json::Value| state.shape(ShapeId(id.as_u64().unwrap() as u32)).unwrap().text.as_ref().unwrap().paragraphs[0].runs[0].props.bold;
    assert_ne!(bold(&a), Some(true), "the selected text belongs to another shape");
    assert_eq!(bold(&b), Some(true), "the explicit shape receives the copied formatting");
}
