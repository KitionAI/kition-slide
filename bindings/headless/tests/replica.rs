//! `kition-slide` opens a document from its replica (`model.open`, which `kit.json` lists): what
//! `model.export` answers for an open deck, its changes and the media they reference, opens again
//! as the same deck, with no file. An agent's headless draft is such a replica (KHP A12); the kit
//! listed `model.open` but refused it, so its drafts fell back to copies of the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_slide_kit::SlideDocument;
use serde_json::{Map, Value, json};

/// A 1x1 PNG.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The shapes of the deck's first slide, as an agent reads them, without their session ids (each
/// open document numbers its shapes; `ref`, the object's own id, stays).
fn shapes(kit: &mut Headless<SlideDocument>, id: &str) -> Value {
    let inspected = kit.execute(id, "slide.inspect", params(json!({ "index": 0, "select": false })), Origin::Agent).unwrap();
    let mut shapes = inspected.result.unwrap_or_default()["shapes"].clone();
    for shape in shapes.as_array_mut().into_iter().flatten() {
        shape.as_object_mut().map(|shape| shape.remove("id"));
    }
    shapes
}

#[test]
fn a_deck_opens_from_its_exported_replica_with_its_media() {
    let root = std::env::temp_dir().join(format!("kition-slide-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut deck = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    deck.execute("insert.picture", json!({ "name": "dot.png", "data": PICTURE })).unwrap();
    std::fs::write(root.join("Deck.kslide"), deck.save(None).unwrap()).unwrap();

    let mut kit = Headless::<SlideDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_slide_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let file = kit.open(OpenRequest { file: &folder.file("Deck.kslide").unwrap(), read_only: false, create: None }).unwrap().id;
    kit.execute(&file, "shape.insert", params(json!({ "preset": "ellipse", "rect": [360, 200, 120, 120] })), Origin::Agent).unwrap();
    let before = shapes(&mut kit, &file);
    assert!(before.as_array().is_some_and(|shapes| shapes.len() >= 2), "{before}");

    let exported = kit.model("model.export", json!({ "documentId": file })).unwrap();
    assert!(exported["blobs"].is_string(), "the picture travels with the changes: {exported}");
    let opened = kit
        .model("model.open", json!({ "name": "Deck.kslide", "snapshot": exported["changes"], "blobs": exported["blobs"] }))
        .unwrap_or_else(|error| panic!("model.open: {}", error.message));
    let replica = opened["documentId"].as_str().unwrap().to_owned();
    assert_ne!(replica, file);
    assert_eq!(shapes(&mut kit, &replica), before, "the replica is the deck");

    // The replica takes edits of its own and leaves the file's document alone.
    kit.execute(&replica, "shape.insert", params(json!({ "preset": "rect", "rect": [40, 40, 80, 80] })), Origin::Agent).unwrap();
    assert_eq!(shapes(&mut kit, &file), before);
    assert_ne!(shapes(&mut kit, &replica), before);
    let saved = kit.model("model.save", json!({ "documentId": replica })).unwrap();
    assert!(saved["data"].as_str().is_some_and(|data| !data.is_empty()), "{saved}");
    let _ = std::fs::remove_dir_all(root);
}
