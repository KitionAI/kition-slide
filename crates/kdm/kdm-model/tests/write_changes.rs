//! `write_changes` writes only what differs: one shape's text, one table row and its cells, one
//! slide moved; a paragraph split and merge leave only paragraph text in the fields.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{SharedBlobs, new_replica, person, slide_file};
use kdm_model::{BodyOwner, write_changes};
use kition_kit::kdm::ObjectId;
use kition_slide_model::text::TextBody;
use serde_json::Value;

#[test]
fn editing_one_shapes_text_writes_only_that_text() {
    let blobs = SharedBlobs::default();
    let before = slide_file();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let slide = ids.slides.object(before.slides[0].id).cloned().unwrap_or_else(|| panic!("slide"));
    // Shape "A", not `shapes[0]`: the slide's layout placeholders come first, with empty bodies.
    let index = before.slides[0].shapes.iter().position(|s| s.name == "A").unwrap_or_else(|| panic!("shape A"));
    let object = ids.shape(&slide, before.slides[0].shapes[index].id).cloned().unwrap_or_else(|| panic!("shape"));
    let fields = |replica: &kition_kit::kdm::Replica| -> Vec<Value> {
        let mut out: Vec<Value> = ["name", "xfrm", "kind"].iter().map(|f| replica.get(&object, f).unwrap_or(Value::Null)).collect();
        out.extend(
            replica.map_entries(&object, "look").unwrap_or_default().into_iter().map(|(key, member)| Value::Array(vec![Value::String(key), member])),
        );
        out
    };
    let untouched = fields(&replica);
    let paragraph = ids.body(&BodyOwner::Shape(object.clone())).unwrap_or_else(|| panic!("a text body")).paragraphs[0].clone();

    let mut after = before.clone();
    Arc::make_mut(&mut after.slides[0]).shapes[index].text = Some(TextBody::from_text("Edited"));
    let mut sink = blobs.clone();
    let wrote =
        replica.transact(&person("t"), |tx| write_changes(tx, &before, &after, &mut ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"));
    assert!(wrote);
    assert_eq!(replica.take_changed(), BTreeSet::from([paragraph.clone()]), "only the edited paragraph changed");
    assert_eq!(fields(&replica), untouched, "the shape's other fields are as they were");
    assert_eq!(replica.text(&paragraph, "text").unwrap_or_else(|error| panic!("text: {error}")), "Edited");
    assert!(replica.schema().field("Shape", "text").is_err(), "a shape's text lives in its body only");

    // Nothing more to write.
    let wrote =
        replica.transact(&person("t"), |tx| write_changes(tx, &after, &after, &mut ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"));
    assert!(!wrote);
}

#[test]
fn inserting_a_table_row_adds_one_row_and_its_cells() {
    let blobs = SharedBlobs::default();
    let before = slide_file();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let tables = replica.objects_of_type("Table");
    assert_eq!(tables.len(), 1);
    let table = tables[0].clone();
    let rows_before = replica.list(&table, "rows").unwrap_or_default();
    let cells_before = replica.map_entries(&table, "cells").unwrap_or_default();
    assert_eq!(cells_before.len(), 9);

    let mut after = before.clone();
    if let Some(t) = common::table_mut(&mut after) {
        t.insert_row(1);
        t.rows[1].cells[0].text = TextBody::from_text("new");
    }
    let mut sink = blobs.clone();
    replica.transact(&person("t"), |tx| write_changes(tx, &before, &after, &mut ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"));
    let changed = replica.take_changed();
    let rows_after = replica.list(&table, "rows").unwrap_or_default();
    assert_eq!(rows_after.len(), 4);
    let new_row = rows_after[1].clone();
    assert!(!rows_before.contains(&new_row));
    assert_eq!(&rows_after[..1], &rows_before[..1]);
    assert_eq!(&rows_after[2..], &rows_before[1..], "the other rows keep their objects");
    let (text, structure): (BTreeSet<ObjectId>, BTreeSet<ObjectId>) =
        changed.into_iter().partition(|o| matches!(replica.object_type(o).as_deref(), Ok("TextBody" | "Paragraph")));
    assert_eq!(structure, BTreeSet::from([table.clone(), new_row.clone()]), "only the table and the new row changed");
    assert!(text.len() >= 6, "the new cells' text bodies and paragraphs: {}", text.len());
    let cells_after = replica.map_entries(&table, "cells").unwrap_or_default();
    assert_eq!(cells_after.len(), 12);
    for (key, value) in &cells_before {
        assert_eq!(cells_after.iter().find(|(k, _)| k == key).map(|(_, v)| v), Some(value), "cell {key} is untouched");
    }
    let new_cells: Vec<_> = cells_after.iter().filter(|(k, _)| k.starts_with(new_row.as_str())).collect();
    assert_eq!(new_cells.len(), 3);
}

#[test]
fn moving_a_slide_is_one_list_move() {
    let blobs = SharedBlobs::default();
    let before = slide_file();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let root = ObjectId::from_string("slide-file");
    let order = replica.list(&root, "slides").unwrap_or_default();

    let mut after = before.clone();
    let moved = after.slides.remove(0);
    after.slides.push(moved);
    let mut sink = blobs.clone();
    let ops = replica
        .transact(&person("t"), |tx| {
            write_changes(tx, &before, &after, &mut ids, &mut sink)?;
            tx.list_items(&root, "slides")
        })
        .unwrap_or_else(|error| panic!("write: {error}"));
    assert_eq!(replica.take_changed(), BTreeSet::from([root.clone()]), "only the slide list changed");
    let mut expected = order.clone();
    let first = expected.remove(0);
    expected.push(first);
    assert_eq!(ops, expected);
    // That it took exactly one `list_move` is `list::tests` (the list sync counts its operations).
}

/// A split keeps the larger half's object and makes the other half a new paragraph; a merge keeps
/// one object and deletes the other. Every `text` field holds only its paragraph's own text (no
/// anchor characters), and the body lists only the paragraphs the view shows.
#[test]
fn a_paragraph_split_and_merge_store_only_the_paragraphs_text() {
    let blobs = SharedBlobs::default();
    let mut before = slide_file();
    let index = before.slides[0].shapes.iter().position(|s| s.name == "A").unwrap_or_else(|| panic!("shape A"));
    Arc::make_mut(&mut before.slides[0]).shapes[index].text = Some(TextBody::from_text("Hello big world"));
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    let slide = ids.slides.object(before.slides[0].id).cloned().unwrap_or_else(|| panic!("slide"));
    let object = ids.shape(&slide, before.slides[0].shapes[index].id).cloned().unwrap_or_else(|| panic!("shape"));
    let owner = BodyOwner::Shape(object);
    let body = ids.body(&owner).unwrap_or_else(|| panic!("a text body")).body.clone();
    let original = ids.body(&owner).unwrap_or_else(|| panic!("a text body")).paragraphs[0].clone();
    let texts = |replica: &kition_kit::kdm::Replica| -> Vec<(ObjectId, String)> {
        replica
            .list(&body, "paragraphs")
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let text = replica.text(&p, "text").unwrap_or_else(|error| panic!("text: {error}"));
                (p, text)
            })
            .collect()
    };
    let mut sink = blobs.clone();

    let mut split = before.clone();
    Arc::make_mut(&mut split.slides[0]).shapes[index].text = Some(TextBody::from_text("Hello big\n world"));
    replica.transact(&person("t"), |tx| write_changes(tx, &before, &split, &mut ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"));
    let after_split = texts(&replica);
    assert_eq!(after_split.len(), 2, "the body lists the two paragraphs only");
    assert_eq!(after_split[0], (original.clone(), "Hello big".to_owned()), "the larger half keeps the object");
    assert_eq!(after_split[1].1, " world");
    let second = after_split[1].0.clone();
    assert_eq!(ids.body(&owner).map(|b| b.paragraphs.clone()), Some(vec![original.clone(), second.clone()]));

    let mut merged = split.clone();
    Arc::make_mut(&mut merged.slides[0]).shapes[index].text = Some(TextBody::from_text("Hello big world"));
    replica.transact(&person("t"), |tx| write_changes(tx, &split, &merged, &mut ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"));
    assert_eq!(texts(&replica), [(original.clone(), "Hello big world".to_owned())], "one paragraph, its own text only");
    assert!(!replica.objects_of_type("Paragraph").contains(&second), "the merged-away paragraph is deleted");
}
