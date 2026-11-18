//! C8-slide-pin: saved member identities distinguish equal rows, columns and empty sections.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn model(bytes: &[u8]) -> Value {
    serde_json::to_value(kition_slide_engine::cmd::file::open_slide_file("Ids.pptx", bytes).unwrap()).unwrap()
}

#[test]
fn equal_table_members_keep_identity_across_insert_delete_and_reopen() {
    let mut doc = SlideDocument::create("Ids.pptx", 960, 540).unwrap();
    let table = doc.execute("insert.table", json!({"rows": 5, "cols": 5})).unwrap();
    let before = model(&doc.save(Some("Ids.pptx")).unwrap());
    let pick = |model: &Value| {
        let table = model["slides"][0]["shapes"].as_array().unwrap().iter().find(|shape| shape["kind"]["type"] == "table").unwrap();
        let table = &table["kind"];
        (
            table["rows"].as_array().unwrap().iter().map(|row| row["identity"].as_str().unwrap_or("").to_owned()).collect::<Vec<_>>(),
            table["colIds"].as_array().cloned().unwrap_or_default(),
        )
    };
    let (rows, cols) = pick(&before);
    assert!(rows.iter().all(|id| !id.is_empty()), "saved equal rows need identity: {rows:?}");
    assert_eq!(cols.len(), 5, "saved equal columns need identity");
    doc.execute("table.insertRowAbove", json!({"id": table["id"], "row": 1})).unwrap();
    doc.execute("table.insertColumnLeft", json!({"id": table["id"], "col": 1})).unwrap();
    let (after_rows, after_cols) = pick(&model(&doc.save(Some("Ids.pptx")).unwrap()));
    assert_eq!(&after_rows[2..], &rows[1..]);
    assert_eq!(&after_cols[2..], &cols[1..]);
    assert!(!rows.contains(&after_rows[1]));
    assert!(!cols.contains(&after_cols[1]));
    doc.execute("table.deleteRow", json!({"id": table["id"], "row": 1})).unwrap();
    doc.execute("table.deleteColumn", json!({"id": table["id"], "col": 1})).unwrap();
    assert_eq!(pick(&model(&doc.save(Some("Ids.pptx")).unwrap())), (rows, cols));
}

#[test]
fn empty_sections_keep_distinct_identity_across_rename_move_and_reopen() {
    let mut doc = SlideDocument::create("Ids.pptx", 960, 540).unwrap();
    doc.execute("section.add", json!({"at": 0, "name": "Same"})).unwrap();
    doc.execute("section.add", json!({"at": 0, "name": "Same"})).unwrap();
    let before = model(&doc.save(Some("Ids.pptx")).unwrap());
    let sections = before["sections"].as_array().unwrap();
    let first = sections[0]["identity"].as_str().unwrap_or("");
    let second = sections[1]["identity"].as_str().unwrap_or("");
    assert!(!first.is_empty() && first != second, "empty sections need distinct identity: {sections:?}");
    doc.execute("section.rename", json!({"index": 0, "name": "Renamed"})).unwrap();
    doc.execute("section.move", json!({"index": 0, "to": 1})).unwrap();
    let after = model(&doc.save(Some("Ids.pptx")).unwrap());
    assert_eq!(after["sections"][1]["identity"], first);
    assert_eq!(after["sections"][0]["identity"], second);
}

#[test]
fn missing_and_duplicate_member_ids_repair_deterministically_without_changing_survivors() {
    let mut table = kition_slide_model::Table::new(5, 5, 500.0, 30.0);
    let untouched = table.rows[4].identity.clone();
    table.rows[0].identity.clear();
    table.rows[1].identity = table.rows[2].identity.clone();
    table.col_ids[0].clear();
    table.col_ids[1] = table.col_ids[2].clone();
    let mut other = table.clone();
    assert!(table.settle_ids());
    assert!(other.settle_ids());
    assert_eq!(table, other);
    assert!(!table.settle_ids());
    assert_eq!(table.rows[4].identity, untouched);
    let rows: std::collections::BTreeSet<_> = table.rows.iter().map(|row| &row.identity).collect();
    let cols: std::collections::BTreeSet<_> = table.col_ids.iter().collect();
    assert_eq!(rows.len(), 5);
    assert_eq!(cols.len(), 5);
}

#[test]
fn a_table_copy_owns_its_members_and_raw_creations_persist_their_ids() {
    use kition_kit::session::model::Exchange;
    let mut doc = SlideDocument::create("Ids.pptx", 960, 540).unwrap();
    let source = doc.execute("insert.table", json!({"rows": 3, "cols": 3})).unwrap();
    doc.execute("edit.duplicate", json!({})).unwrap();
    doc.version().unwrap();
    doc.execute("table.rowHeight", json!({"id": source["id"], "row": 0, "height": 47})).unwrap();
    let saved = model(&doc.save(Some("Ids.pptx")).unwrap());
    let tables: Vec<_> = saved["slides"][0]["shapes"].as_array().unwrap().iter().filter(|shape| shape["kind"]["type"] == "table").collect();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0]["kind"]["rows"][0]["height"], json!(47.0));
    assert_eq!(tables[1]["kind"]["rows"][0]["height"], json!(40.0));
    let shapes = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    let table = shapes["shapes"].as_array().unwrap().iter().find(|shape| shape["id"] == source["id"]).unwrap()["toolId"].as_str().unwrap();
    Exchange::default()
        .batch(&mut doc, &json!({"ops": [{"op":"create", "type":"Row", "in":format!("{table}.table.rows"), "at":1, "fields":{"height":29}}]}), false)
        .unwrap();
    let bytes = doc.save(Some("Ids.pptx")).unwrap();
    let saved = model(&bytes);
    let rows = saved["slides"][0]["shapes"].as_array().unwrap().iter().find(|shape| shape["id"] == source["id"]).unwrap()["kind"]["rows"]
        .as_array()
        .unwrap();
    let ids: std::collections::BTreeSet<_> = rows.iter().map(|row| row["identity"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.iter().all(|id| !id.is_empty()));
    let reopened = SlideDocument::open("Ids.pptx", &bytes).unwrap();
    assert_eq!(model(&reopened.save(Some("Ids.pptx")).unwrap())["slides"], saved["slides"]);
}

#[test]
fn identityless_imports_do_not_reuse_positional_ids_after_a_grid_change() {
    let mut old = kition_slide_model::Table::new(5, 5, 500.0, 30.0);
    old.col_ids.clear();
    for row in &mut old.rows {
        row.identity.clear();
    }
    let mut inserted = old.clone();
    inserted.rows.insert(1, inserted.rows[0].clone());
    let mut before = old.clone();
    before.settle_ids();
    inserted.settle_ids();
    let ids: std::collections::BTreeSet<_> = before.rows.iter().map(|row| &row.identity).collect();
    assert!(inserted.rows.iter().all(|row| !ids.contains(&row.identity)), "an identityless changed grid cannot prove any old row position");
    let mut again = old;
    again.settle_ids();
    assert_eq!(before, again, "the same imported value regenerates the same ids");
}

#[test]
fn inconsistent_column_metadata_does_not_claim_prefix_continuity() {
    let before = kition_slide_model::Table::new(5, 5, 500.0, 30.0);
    let mut after = before.clone();
    after.cols.insert(1, after.cols[0]);
    for row in &mut after.rows {
        row.cells.insert(1, row.cells[0].clone());
    }
    // A writer retained old parallel metadata but did not identify the inserted column.
    after.settle_ids();
    assert!(after.col_ids.iter().all(|id| !before.col_ids.contains(id)));
}
