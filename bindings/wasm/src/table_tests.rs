use super::*;

fn table() -> (SlideDocument, Value) {
    let mut doc = SlideDocument::create("Table.pptx", 960, 540).unwrap();
    doc.set_single(true);
    let id = doc.execute("table.insert", json!({"rows": 3, "cols": 3, "rect": [100, 100, 450, 180]})).unwrap()["id"].clone();
    (doc, id)
}

fn pointer(doc: &mut SlideDocument, action: PointerAction, x: f64, y: f64, shift: bool) -> ToolResponse {
    doc.pointer(&ToolPointer { action, x: x + 40.0, y: y + 40.0, mods: ToolMods { shift, ..Default::default() }, pressure: 1.0, zoom: 1.0 }).unwrap()
}

#[test]
fn table_pointer_shows_caret_and_selects_cells_without_moving_the_frame() {
    let (mut doc, id) = table();
    pointer(&mut doc, PointerAction::Down, 125.0, 125.0, false);
    let response = pointer(&mut doc, PointerAction::Up, 125.0, 125.0, false);
    assert!(response.overlays.iter().any(|o| matches!(o, ToolOverlay::Line { color, .. } if *color == CARET)));
    doc.execute("text.insert", json!({"text": "First cell"})).unwrap();
    pointer(&mut doc, PointerAction::Down, 275.0, 185.0, true);
    let response = pointer(&mut doc, PointerAction::Up, 275.0, 185.0, true);
    assert_eq!(response.overlays.iter().filter(|o| matches!(o, ToolOverlay::Highlight { .. })).count(), 4);
    let state = doc.execute("table.state", json!({})).unwrap();
    assert_eq!(state["from"], json!([0, 0]));
    assert_eq!(state["to"], json!([1, 1]));
    doc.execute("table.merge", json!({})).unwrap();
    pointer(&mut doc, PointerAction::Down, 275.0, 185.0, false);
    pointer(&mut doc, PointerAction::Up, 275.0, 185.0, false);
    let inspected = doc.execute("file.inspect", json!({})).unwrap();
    assert_eq!(inspected["selection"]["text"]["cell"], json!([0, 0]));
    let shape = doc.execute("shape.inspect", json!({"id": id})).unwrap();
    assert_eq!(shape["box"]["x"], 100.0);
    assert_eq!(shape["box"]["y"], 100.0);
}

#[test]
fn table_edits_survive_pptx_reopen_and_remain_editable() {
    let (mut doc, id) = table();
    doc.execute("text.set", json!({"id": id, "cell": [0, 0], "text": "Revenue"})).unwrap();
    doc.execute("table.selectCells", json!({"from": [0, 0], "to": [0, 1]})).unwrap();
    doc.execute("table.merge", json!({})).unwrap();
    doc.execute("table.cellFill", json!({"color": "#5645d4"})).unwrap();
    doc.execute("table.cellFormat", json!({"anchor": "center", "margins": [8, 8, 4, 4], "border": "all", "width": 2})).unwrap();
    doc.execute("table.insertColumnRight", json!({"col": 0})).unwrap();
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.execute("table.state", json!({})).unwrap()["cols"], 3);
    doc.execute("edit.redo", json!({})).unwrap();
    assert_eq!(doc.execute("table.state", json!({})).unwrap()["cols"], 4);
    let bytes = doc.save(Some("Table.pptx")).unwrap();
    let mut reopened = SlideDocument::open("Table.pptx", &bytes).unwrap();
    let slide = reopened.execute("slide.inspect", json!({})).unwrap();
    let table = slide["shapes"].as_array().unwrap().iter().find(|s| s["kind"] == "table").unwrap();
    let id = table["id"].clone();
    assert_eq!(table["table"]["cells"][0][0], "Revenue");
    reopened.execute("edit.select", json!({"ids": [id]})).unwrap();
    reopened.execute("text.edit", json!({"id": id, "cell": [0, 2]})).unwrap();
    let state = reopened.execute("table.state", json!({})).unwrap();
    assert_eq!(state["from"], json!([0, 0]));
    assert_eq!(state["anchor"], "center");
    assert_eq!(state["canSplit"], true);
    reopened.execute("text.insert", json!({"text": " updated"})).unwrap();
    reopened.execute("table.split", json!({})).unwrap();
    assert_eq!(
        reopened.execute("slide.inspect", json!({})).unwrap()["shapes"].as_array().unwrap().iter().find(|s| s["kind"] == "table").unwrap()["table"]["cells"]
            [0][0],
        "Revenue updated"
    );
}

#[test]
fn table_navigation_skips_covered_cells_and_shading_targets_only_the_edited_cell() {
    let (mut doc, id) = table();
    doc.execute("table.merge", json!({"from": [0, 0], "to": [0, 1]})).unwrap();
    doc.execute("text.edit", json!({"cell": [0, 1]})).unwrap();
    doc.key("Tab", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.execute("table.state", json!({})).unwrap()["from"], json!([0, 2]));
    doc.execute("table.cellFill", json!({"color": "#5645d4"})).unwrap();
    let sh = doc.session.active().unwrap().shape(kition_slide_model::ShapeId(id.as_u64().unwrap() as u32)).unwrap();
    let kition_slide_model::ShapeKind::Table(table) = &sh.kind else { panic!("table") };
    assert!(table.cell(0, 2).unwrap().fill.is_some());
    assert!(table.cell(1, 2).unwrap().fill.is_none());
    doc.execute("text.edit", json!({"cell": [2, 2]})).unwrap();
    doc.key("Tab", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.execute("table.state", json!({})).unwrap()["rows"], 4);
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.execute("table.state", json!({})).unwrap()["rows"], 3);
    assert!(doc.execute("table.selectCells", json!({"from": [999999, 0]})).is_err());
    assert!(doc.execute("table.insertRowBelow", json!({"row": u64::MAX})).is_err());
}
