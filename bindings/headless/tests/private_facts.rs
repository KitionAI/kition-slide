//! C8-slide-pin: private derived facts guard writes without appearing in public receipts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::json;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread")]
async fn private_shape_count_is_hidden_from_receipts_and_still_guards_writes() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("Facts.pptx");
    let doc = SlideDocument::create("Facts.pptx", 960, 540).unwrap();
    std::fs::write(&path, doc.save(Some("Facts.pptx")).unwrap()).unwrap();
    let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let kits = KitManager::new(Some(workspace.path().to_owned()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
    let seen = tools::call_as(&kits, "slide_inspect", &json!({"path":"Facts.pptx"}), CallOptions::default(), "facts").await.unwrap().unwrap().output;
    let step = json!({"command":"insert.textBox", "params":{"text":"Mine", "rect":[10,10,100,50]}});
    let made =
        tools::call_as(&kits, "slide_run", &json!({"path":"Facts.pptx", "basis":seen["revision"], "steps":[step]}), CallOptions::default(), "facts")
            .await
            .unwrap()
            .unwrap()
            .output;
    for list in ["created", "changed", "deleted"] {
        for object in made["changes"][list].as_array().into_iter().flatten() {
            assert!(
                !object["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|field| matches!(field.as_str(), Some("~shapeCount" | "~commentOrder" | "~slideOrder" | "~layoutCount"))),
                "private fact in receipt: {made}"
            );
        }
    }
    let mut other = SlideDocument::open("Facts.pptx", &std::fs::read(&path).unwrap()).unwrap();
    other.execute("insert.textBox", json!({"text":"Colleague", "rect":[10,80,100,50]})).unwrap();
    let bytes = other.save(Some("Facts.pptx")).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let refused =
        tools::call_as(&kits, "slide_run", &json!({"path":"Facts.pptx", "basis":made["revision"], "steps":[step]}), CallOptions::default(), "facts")
            .await
            .unwrap();
    kits.shutdown().await;
    let error = refused.expect_err("the consumed private count changed").structured();
    println!("private fact refusal: {error}");
    assert!(error["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["reason"] == "changed"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "a refusal writes nothing");
}
