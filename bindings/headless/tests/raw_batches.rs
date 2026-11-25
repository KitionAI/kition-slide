//! C8-slide-pin: raw object-model batches and their premise contracts (`batchOps`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Raw.pptx";

struct Fixture {
    workspace: tempfile::TempDir,
    kits: KitManager,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
        doc.execute("textBox.insert", json!({"rect": [10, 10, 300, 200], "text": "First\nSecond\nThird\nFourth"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(NAME), doc.save(Some(NAME)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { workspace, kits }
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "raw-agent").await.unwrap() {
            Ok(answer) => Ok(answer.output),
            Err(error) => Err(error.structured()),
        }
    }

    /// Another program inserts a paragraph at the top of the text box.
    fn colleague_inserts_a_paragraph(&self) {
        let file = self.workspace.path().join(NAME);
        let mut deck = kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(&file).unwrap()).unwrap();
        let slide = Arc::make_mut(&mut deck.slides[0]);
        let shape = slide.shapes.iter_mut().find(|shape| shape.text.as_ref().is_some_and(|body| body.text().starts_with("First"))).unwrap();
        shape.text.as_mut().unwrap().paragraphs.insert(0, kition_slide_model::text::Paragraph::new("Colleague"));
        let at = std::fs::metadata(&file).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(&file, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(at).unwrap();
    }
}

fn text_box(seen: &Value) -> String {
    seen["part"]["shapes"].as_array().unwrap().iter().find(|s| s["text"].as_str().is_some_and(|t| t.starts_with("First"))).unwrap()["toolId"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Every model type declares a contract, so ordinary raw batches run, with or without `basis`.
#[tokio::test(flavor = "multi_thread")]
async fn raw_batches_run_under_their_declared_contract() {
    let f = Fixture::new();
    let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
    let shape = text_box(&seen);
    f.call("batch", json!({"ops": [{"op": "set", "target": shape, "field": "name", "value": "Renamed"}]})).await.unwrap();
    f.call("batch", json!({"ops": [{"op": "create", "type": "Slide", "in": "slide-file.slides", "at": 1, "fields": {}}]})).await.unwrap();
    f.kits.shutdown().await;
}

/// D2: a positional operation without a basis has no stale premise to refuse.
#[tokio::test(flavor = "multi_thread")]
async fn positional_raw_edits_without_a_basis_apply() {
    for operation in ["create", "list.insert", "list.move", "list.remove"] {
        let f = Fixture::new();
        let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
        let shape = text_box(&seen);
        let shapes = f.call("query", json!({"follow": "slides.0.shapes", "fields": []})).await.unwrap();
        let at = shapes["objects"].as_array().unwrap().iter().position(|object| object["id"] == shape).unwrap();
        let loaded = f.call("query", json!({"follow": format!("slides.0.shapes.{at}.body"), "fields": []})).await.unwrap();
        let body = loaded["objects"][0]["id"].as_str().unwrap().to_owned();
        let ops = match operation {
            "create" => {
                json!([{"op": "create", "type": "Paragraph", "in": format!("{body}.paragraphs"), "at": 2, "fields": {"props": {"level": 1}}}])
            }
            "list.insert" => json!([
                {"op": "create", "type": "Paragraph", "ref": "$new", "fields": {"props": {"level": 1}}},
                {"op": "list.insert", "target": body, "field": "paragraphs", "index": 2, "item": "$new"}
            ]),
            "list.move" => json!([{"op": operation, "target": body, "field": "paragraphs", "from": 0, "to": 2}]),
            _ => json!([{"op": operation, "target": body, "field": "paragraphs", "index": 2}]),
        };
        let answer = f.call("batch", json!({"ops": ops})).await;
        f.kits.shutdown().await;
        assert!(answer.is_ok(), "{operation}: {answer:?}");
        let deck = kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(f.workspace.path().join(NAME)).unwrap()).unwrap();
        let body = deck.slides[0].shapes.iter().find_map(|shape| shape.text.as_ref().filter(|body| body.text().contains("First"))).unwrap();
        let text: Vec<_> = body.paragraphs.iter().map(|p| p.text()).collect();
        match operation {
            "create" | "list.insert" => assert_eq!(text, ["First", "Second", "", "Third", "Fourth"]),
            "list.move" => assert_eq!(text, ["Second", "First", "Third", "Fourth"]),
            _ => assert_eq!(text, ["First", "Second", "Fourth"]),
        }
    }
}

/// After another program inserted a paragraph before it, a paragraph's id still names that
/// paragraph (the reread pairs paragraphs by their content): the write lands on it, now one
/// place further down, and leaves the other program's paragraph alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_paragraph_named_by_id_after_an_outside_insertion_is_the_same_paragraph() {
    let f = Fixture::new();
    let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
    let shape = text_box(&seen);
    let shapes = f.call("query", json!({"follow": "slides.0.shapes", "fields": []})).await.unwrap();
    let at = shapes["objects"].as_array().unwrap().iter().position(|object| object["id"] == json!(shape)).unwrap();
    let paragraphs = f.call("query", json!({"follow": format!("slides.0.shapes.{at}.body.paragraphs"), "fields": []})).await.unwrap();
    let paragraph = paragraphs["objects"][1]["id"].as_str().unwrap_or_else(|| panic!("the second paragraph: {paragraphs}")).to_owned();
    f.colleague_inserts_a_paragraph();
    f.call("batch", json!({"basis": seen["revision"], "ops": [{"op": "set", "target": paragraph, "field": "props", "value": {"level": 1}}]}))
        .await
        .unwrap();
    let deck = kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(f.workspace.path().join(NAME)).unwrap()).unwrap();
    let body = deck.slides[0].shapes.iter().find_map(|shape| shape.text.as_ref().filter(|body| body.text().contains("First"))).unwrap();
    let levels: Vec<(String, u8)> = body.paragraphs.iter().map(|p| (p.text(), p.level)).collect();
    assert_eq!(
        levels,
        [("Colleague".to_owned(), 0), ("First".to_owned(), 0), ("Second".to_owned(), 1), ("Third".to_owned(), 0), ("Fourth".to_owned(), 0)]
    );
    f.kits.shutdown().await;
}
