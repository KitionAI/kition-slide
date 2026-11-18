//! C8-slide-pin: closing discards unsaved work and terminates an agent run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::json;
use std::path::PathBuf;

async fn close_runs(guarded: bool) {
    for outside in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("Close.pptx");
        let doc = SlideDocument::create("Close.pptx", 960, 540).unwrap();
        std::fs::write(&path, doc.save(Some("Close.pptx")).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_owned()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        let seen =
            tools::call_as(&kits, "slide_inspect", &json!({"path":"Close.pptx"}), CallOptions::default(), "closer").await.unwrap().unwrap().output;
        if outside {
            let mut other = SlideDocument::open("Close.pptx", &std::fs::read(&path).unwrap()).unwrap();
            other.execute("slide.rename", json!({"name":"Colleague"})).unwrap();
            std::fs::write(&path, other.save(Some("Close.pptx")).unwrap()).unwrap();
        }
        if outside && !guarded {
            let read =
                tools::call_as(&kits, "slide_inspect", &json!({"path":"Close.pptx"}), CallOptions::default(), "closer").await.unwrap().unwrap();
            assert!(read.output["changedOutside"].is_object(), "the fresh read reports the outside change");
        }
        let before = std::fs::read(&path).unwrap();
        let mut input = json!({"path":"Close.pptx", "steps":[
            {"command":"file.close"},
            {"command":"slide.rename", "params":{"name":"Never run"}}
        ]});
        if guarded {
            input["basis"] = seen["revision"].clone();
        }
        let mut answer = tools::call_as(&kits, "slide_run", &input, CallOptions::default(), "closer").await.unwrap();
        if outside && !guarded {
            // SDK 5c1253a tools/schemas.rs:316: inspect reports the change, and the next
            // no-basis write is still refused once. Check that policy before the close itself.
            let refusal = answer.unwrap_err().structured();
            assert!(refusal["changedOutside"].is_object(), "{refusal}");
            assert_eq!(std::fs::read(&path).unwrap(), before, "the refused attempt runs nothing");
            answer = tools::call_as(&kits, "slide_run", &input, CallOptions::default(), "closer").await.unwrap();
        }
        kits.shutdown().await;
        let answer = answer.unwrap_or_else(|error| panic!("outside={outside}: {error:?}")).output;
        assert_eq!(answer["closed"], true, "{answer}");
        assert_eq!(answer["saved"], false, "{answer}");
        assert_eq!(answer["notRun"], 1, "{answer}");
        assert!(answer.get("document").is_none(), "{answer}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "close never saves over the file");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_is_terminal_and_does_not_save_or_hold_discarded_content() {
    close_runs(true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_without_basis_is_terminal_and_does_not_save() {
    close_runs(false).await;
}

#[test]
fn closing_reports_the_terminal_result() {
    let mut doc = SlideDocument::create("Close.pptx", 960, 540).unwrap();
    assert_eq!(doc.execute("file.close", json!({})).unwrap(), json!({"closed": true}));
    assert!(doc.inspect().is_err(), "the engine is closed before reporting success");
}
