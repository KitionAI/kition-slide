#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_slide_kit::SlideDocument;
use serde_json::json;

#[test]
fn indexed_reads_are_published_as_places_and_resolve_the_slide() {
    let folder = tempfile::tempdir().unwrap();
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    doc.execute("slide.new", json!({"layout":"blank"})).unwrap();
    let expected = doc.execute("slide.inspect", json!({"index":1})).unwrap();
    let under = doc.under("transition.timing", &json!({"index":1})).unwrap().unwrap();
    assert_eq!(under.within.iter().map(|w| (w.field.as_str(), w.from, w.to)).collect::<Vec<_>>(), vec![("slides", 1, 2)], "{under:?}; {expected}");
    std::fs::write(folder.path().join("Deck.pptx"), doc.save(None).unwrap()).unwrap();
    let mut kit = Headless::<SlideDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_slide_kit::EDITOR_STATE).unwrap(),
    );
    let host = HostFolder::new(folder.path()).unwrap();
    kit.set_host(host.host());
    let id = kit.open(OpenRequest { file: &host.file("Deck.pptx").unwrap(), read_only: false, create: None }).unwrap().id;
    let commands = kit.agent_commands(Some(&id)).unwrap();
    for (id, parameter) in [("transition.timing", "index"), ("show.customShow", "slides")] {
        let schema = &commands.iter().find(|command| command.id == id).unwrap().params;
        assert_eq!(schema["properties"][parameter]["x-khp-role"], "place", "{id}: {schema}");
    }
}

#[test]
fn entering_text_edit_is_declared_as_a_possible_document_write() {
    let mut doc = SlideDocument::create("Text.kslide", 960, 540).unwrap();
    let id = doc.execute("shape.insert", json!({"preset": "rect"})).unwrap()["id"].clone();
    let local = kition_slide_model::ShapeId(id.as_u64().unwrap() as u32);
    let mut stored = kition_slide_engine::cmd::file::open_slide_file("Text.kslide", &doc.save(None).unwrap()).unwrap();
    let shape = std::sync::Arc::make_mut(&mut stored.slides[0]).shapes.iter_mut().find(|shape| shape.id == local).unwrap();
    shape.text = None;
    let mut doc = SlideDocument::open("Text.kslide", &kition_slide_engine::cmd::file::save_bytes(&stored, "kslide").unwrap()).unwrap();
    let command = doc.commands().into_iter().find(|command| command.id == "text.edit").unwrap();
    assert!(command.mutates, "text.edit materializes a stored TextBody on a shape without one");
    doc.execute("text.edit", json!({"id": id})).unwrap();
    let saved = kition_slide_engine::cmd::file::open_slide_file("Text.kslide", &doc.save(None).unwrap()).unwrap();
    assert!(saved.slides[0].shapes.iter().find(|shape| shape.id == local).unwrap().text.is_some(), "this path creates a stored text body");
}
