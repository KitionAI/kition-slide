//! C8-slide-8 item 7: a document created and never saved is the document its first save opens as.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::kdm::reread;
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn state(doc: &mut SlideDocument) -> Value {
    Exchange::new().replica(doc).unwrap().state_json().unwrap()
}

/// One line per object and field that differs.
fn differences(doc: &mut SlideDocument, name: &str) -> Vec<String> {
    let memory = state(doc);
    let saved = state(&mut SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap());
    let diff = reread::diff_paired(&doc.model_schema(), &memory, &saved).to_json();
    let mut out = Vec::new();
    for list in ["created", "changed", "moved", "deleted"] {
        for entry in diff[list].as_array().into_iter().flatten() {
            match entry["edits"].as_array().filter(|edits| !edits.is_empty()) {
                Some(edits) => {
                    for edit in edits {
                        let text = |value: &Value| value.to_string().chars().take(160).collect::<String>();
                        out.push(format!(
                            "{list} {} {} {}: {} -> {}",
                            entry["id"],
                            entry["type"],
                            edit["field"],
                            text(&edit["before"]),
                            text(&edit["after"])
                        ));
                    }
                }
                None => out.push(format!("{list} {} {}", entry["id"], entry["type"])),
            }
        }
    }
    out
}

#[test]
fn a_fresh_document_is_what_its_first_save_opens_as() {
    for name in ["Fresh.pptx", "Fresh.kslide"] {
        let mut doc = SlideDocument::create(name, 960, 540).unwrap();
        let found = differences(&mut doc, name);
        assert!(found.is_empty(), "{name}: {} differences:\n{}", found.len(), found.join("\n"));
        doc.execute("textBox.insert", json!({"text": "Fresh", "rect": [10, 10, 200, 50]})).unwrap();
        doc.execute("slide.new", json!({"layout": "titleAndContent", "title": "Second", "body": "Body"})).unwrap();
        let found = differences(&mut doc, name);
        assert!(found.is_empty(), "{name} after edits: {} differences:\n{}", found.len(), found.join("\n"));
    }
}
