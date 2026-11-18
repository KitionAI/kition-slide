//! C8-slide-8: a shape's identity survives its position and its file container.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::kdm::reread;
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn state(doc: &mut SlideDocument) -> Value {
    Exchange::new().replica(doc).unwrap().state_json().unwrap()
}

fn fixture(name: &str) -> SlideDocument {
    let mut doc = SlideDocument::create(name, 960, 540).unwrap();
    doc.execute("insert.textBox", json!({"text": "Held", "rect": [10, 10, 100, 50]})).unwrap();
    doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
    SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap()
}

fn held(doc: &mut SlideDocument, index: usize) -> Value {
    doc.execute("slide.inspect", json!({"index": index})).unwrap()["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "Held").unwrap().clone()
}

#[test]
fn moves_keep_the_held_identity_and_the_saved_receipt() {
    for name in ["Identity.pptx", "Identity.kslide"] {
        for command in [false, true] {
            let mut doc = fixture(name);
            let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
            let second = doc.execute("slide.inspect", json!({"index": 1})).unwrap();
            let shape = held(&mut doc, 0);
            let id = shape["toolId"].as_str().unwrap();
            assert!(!id.contains('/'), "shape identity must be independent of its slide: {id}");
            let before = state(&mut doc);
            let mut exchange = Exchange::new();
            let version = exchange.replica(&mut doc).unwrap().version();
            if command {
                doc.execute("shape.move", json!({"ids": [id], "toSlide": second["toolId"]})).unwrap();
            } else {
                let index = first["shapes"].as_array().unwrap().iter().position(|s| s["toolId"] == id).unwrap();
                exchange
                    .batch(
                        &mut doc,
                        &json!({"ops": [
                            {"op": "list.remove", "target": first["toolId"], "field": "shapes", "index": index},
                            {"op": "list.insert", "target": second["toolId"], "field": "shapes", "item": id, "index": 0}
                        ]}),
                        false,
                    )
                    .unwrap();
            }
            assert_eq!(held(&mut doc, 1)["toolId"], id);
            let memory = state(&mut doc);
            let receipt = exchange.replica(&mut doc).unwrap().changes_since(&version).unwrap().to_json();
            let mut again = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
            assert_eq!(held(&mut again, 1)["toolId"], id);
            let disk = state(&mut again);
            let actual = reread::diff_paired(&doc.model_schema(), &before, &disk).to_json();
            let extra = reread::diff_paired(&doc.model_schema(), &memory, &disk).to_json();
            for list in ["created", "changed", "moved", "deleted"] {
                assert_eq!(extra[list], json!([]), "{name} command={command}: {extra}");
                assert_eq!(receipt[list], actual[list], "{name} command={command}: {list}");
            }
            assert_eq!(actual["created"], json!([]));
            assert_eq!(actual["deleted"], json!([]));
            assert!(!actual["moved"].as_array().unwrap().is_empty());
            again.execute("text.set", json!({"id": id, "text": "Still held"})).unwrap();
            assert!(
                again.execute("slide.inspect", json!({"index": 1})).unwrap()["shapes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["toolId"] == id && s["text"] == "Still held")
            );
        }
    }
}

#[test]
fn duplicate_shapes_and_slides_get_distinct_identities() {
    let mut doc = fixture("Identity.pptx");
    let id = held(&mut doc, 0)["toolId"].clone();
    doc.execute("edit.duplicate", json!({"ids": [id]})).unwrap();
    doc.execute("slide.duplicate", json!({"index": 0})).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for index in 0..3 {
        let part = doc.execute("slide.inspect", json!({"index": index})).unwrap();
        for shape in part["shapes"].as_array().unwrap() {
            let id = shape["toolId"].as_str().unwrap();
            assert!(!id.contains('/'));
            assert!(ids.insert(id.to_owned()), "duplicate identity: {id}");
        }
    }
}

#[test]
fn identityless_input_opens_deterministically_and_first_save_persists_ids() {
    let name = "Legacy.kslide";
    let bytes = include_bytes!("../../../conformance/check-empty-placeholder.kslide");
    let mut a = SlideDocument::open(name, bytes).unwrap();
    let mut b = SlideDocument::open(name, bytes).unwrap();
    let part = a.execute("slide.inspect", json!({"index": 0})).unwrap();
    assert_eq!(part, b.execute("slide.inspect", json!({"index": 0})).unwrap());
    assert!(part["shapes"].as_array().unwrap().iter().all(|s| !s["toolId"].as_str().unwrap().contains('/')));
    let mut c = SlideDocument::open("Persisted.pptx", &a.save(Some("Persisted.pptx")).unwrap()).unwrap();
    let saved = c.execute("slide.inspect", json!({"index": 0})).unwrap();
    let ids = |part: &Value| part["shapes"].as_array().unwrap().iter().map(|s| s["toolId"].clone()).collect::<Vec<_>>();
    assert_eq!(ids(&part), ids(&saved));
}

#[test]
fn grouping_and_ungrouping_keep_child_identities() {
    let mut doc = fixture("Identity.pptx");
    let first = held(&mut doc, 0)["toolId"].clone();
    doc.execute("slide.go", json!({"index": 0})).unwrap();
    doc.execute("insert.textBox", json!({"text": "Second", "rect": [150, 10, 100, 50]})).unwrap();
    let part = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    let second = part["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "Second").unwrap()["toolId"].clone();
    let group = doc.execute("arrange.group", json!({"ids": [first, second]})).unwrap();
    doc.execute("arrange.ungroup", json!({"ids": [group["id"]], "onSlide": 1})).unwrap();
    let part = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    assert!(part["shapes"].as_array().unwrap().iter().any(|s| s["toolId"] == first));
    assert!(part["shapes"].as_array().unwrap().iter().any(|s| s["toolId"] == second));
}

#[test]
fn group_moves_keep_endpoint_animation_and_comment_targets() {
    for command in [false, true] {
        let name = "References.pptx";
        let mut doc = fixture(name);
        doc.execute("slide.go", json!({"index": 0})).unwrap();
        let a = held(&mut doc, 0);
        let b = doc.execute("insert.textBox", json!({"text": "Second", "rect": [150, 10, 100, 50]})).unwrap()["id"].clone();
        let line = doc.execute("shape.connect", json!({"from": a["id"], "to": b, "onSlide": 1})).unwrap()["id"].clone();
        let group = doc.execute("arrange.group", json!({"ids": [a["id"], b, line], "onSlide": 1})).unwrap()["id"].clone();
        let mut file = kition_slide_engine::cmd::file::open_slide_file(name, &doc.save(Some(name)).unwrap()).unwrap();
        let first = std::sync::Arc::make_mut(&mut file.slides[0]);
        first.animations.push(kition_slide_model::Animation {
            shape: kition_slide_model::ShapeId(a["id"].as_u64().unwrap() as u32),
            trigger: Some(kition_slide_model::ShapeId(b.as_u64().unwrap() as u32)),
            ..Default::default()
        });
        first.comments.push(kition_slide_model::Comment {
            author: "Agent".into(),
            text: "On held shape".into(),
            shape: Some(kition_slide_model::ShapeId(a["id"].as_u64().unwrap() as u32)),
            ..Default::default()
        });
        doc = SlideDocument::open(name, &kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()).unwrap();
        let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
        let second = doc.execute("slide.inspect", json!({"index": 1})).unwrap();
        let index = first["shapes"].as_array().unwrap().iter().position(|s| s["id"] == group).unwrap();
        let id = first["shapes"][index]["toolId"].clone();
        let before = state(&mut doc);
        let mut exchange = Exchange::new();
        let version = exchange.replica(&mut doc).unwrap().version();
        if command {
            doc.execute("shape.move", json!({"ids": [id], "toSlide": second["toolId"]})).unwrap();
        } else {
            exchange
                .batch(
                    &mut doc,
                    &json!({"ops": [
                        {"op": "list.remove", "target": first["toolId"], "field": "shapes", "index": index},
                        {"op": "list.insert", "target": second["toolId"], "field": "shapes", "item": id, "index": 0}
                    ]}),
                    false,
                )
                .unwrap();
        }
        let receipt = exchange.replica(&mut doc).unwrap().changes_since(&version).unwrap().to_json();
        let bytes = doc.save(Some(name)).unwrap();
        let disk = state(&mut SlideDocument::open(name, &bytes).unwrap());
        let actual = reread::diff_paired(&doc.model_schema(), &before, &disk).to_json();
        // A thread's anchor names its slide by the slide's object id, which a reread draws
        // afresh: compare that edit's field, not its values (the saved slide is asserted below).
        let plain = |changes: &Value| {
            let mut changes = changes.clone();
            for item in changes["changed"].as_array_mut().into_iter().flatten() {
                if item["type"] == "CommentThread" {
                    for edit in item["edits"].as_array_mut().into_iter().flatten() {
                        edit["before"] = Value::Null;
                        edit["after"] = Value::Null;
                    }
                }
            }
            changes
        };
        let (receipt, actual) = (plain(&receipt), plain(&actual));
        for list in ["created", "changed", "moved", "deleted"] {
            assert_eq!(receipt[list], actual[list], "command={command}: {list}");
        }
        let file = kition_slide_engine::cmd::file::open_slide_file(name, &bytes).unwrap();
        let moved = &file.slides[1];
        assert_eq!(moved.animations.len(), 1, "command={command}");
        let target = moved.shape(moved.animations[0].shape).unwrap();
        assert_eq!(format!("shape:{}", target.identity), a["toolId"]);
        assert_eq!(moved.shape(moved.animations[0].trigger.unwrap()).unwrap().text.as_ref().unwrap().text(), "Second");
        assert_eq!(moved.comments.len(), 1);
        assert_eq!(moved.comments[0].shape, Some(target.id));
        let connector = moved.shapes[0].children().iter().find(|s| matches!(s.kind, kition_slide_model::ShapeKind::Connector { .. })).unwrap();
        let kition_slide_model::ShapeKind::Connector { start, end } = &connector.kind else { unreachable!() };
        assert_eq!(moved.shape(start.unwrap().0).unwrap().identity, target.identity);
        assert_eq!(moved.shape(end.unwrap().0).unwrap().text.as_ref().unwrap().text(), "Second");
    }
}

#[test]
fn repeated_imported_creation_guids_are_repaired_deterministically() {
    let name = "Copies.pptx";
    let mut doc = fixture(name);
    let mut file = kition_slide_engine::cmd::file::open_slide_file(name, &doc.save(Some(name)).unwrap()).unwrap();
    let original = file.slides[0].shapes.last().unwrap().clone();
    let mut copy = original.clone();
    copy.id = kition_slide_model::ShapeId(file.alloc_id());
    std::sync::Arc::make_mut(&mut file.slides[1]).shapes.push(copy);
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap();
    let mut ids = Vec::new();
    for _ in 0..2 {
        doc = SlideDocument::open(name, &bytes).unwrap();
        ids.push([held(&mut doc, 0)["toolId"].clone(), held(&mut doc, 1)["toolId"].clone()]);
    }
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0][0], ids[0][1]);
    assert_eq!(ids[0][0], format!("shape:{}", original.identity));
}
