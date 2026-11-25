//! C8-slide-8: positional input never silently follows a relocated shape.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};
use std::path::PathBuf;

async fn call(kits: &KitManager, action: &str, mut input: Value) -> Result<Value, Value> {
    input["path"] = json!("Identity.pptx");
    tools::call_as(kits, &format!("slide_{action}"), &input, CallOptions::default(), "held-agent")
        .await
        .unwrap()
        .map(|v| v.output)
        .map_err(|e| e.structured())
}

#[tokio::test(flavor = "multi_thread")]
async fn moved_shapes_keep_handles_but_old_addresses_refuse_relocated() {
    for spelling in ["identity", "scoped", "number"] {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("Identity.pptx");
        let mut doc = SlideDocument::create("Identity.pptx", 960, 540).unwrap();
        doc.execute("textBox.insert", json!({"text": "Held", "rect": [10, 10, 100, 50]})).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        let second = doc.execute("slide.inspect", json!({"index": 1})).unwrap()["toolId"].clone();
        std::fs::write(&path, doc.save(Some("Identity.pptx")).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        let seen = call(&kits, "inspect", json!({"target": {"slide": 1}})).await.unwrap();
        let shape = seen["part"]["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "Held").unwrap();
        let scoped = format!("shape:{}/{}", seen["part"]["id"], shape["id"]);
        let id = match spelling {
            "scoped" => json!(scoped),
            "number" => shape["id"].clone(),
            _ => shape["toolId"].clone(),
        };
        let mut other = SlideDocument::open("Identity.pptx", &std::fs::read(&path).unwrap()).unwrap();
        other.execute("shape.move", json!({"ids": [shape["toolId"]], "toSlide": second})).unwrap();
        std::fs::write(&path, other.save(Some("Identity.pptx")).unwrap()).unwrap();
        let mut params = json!({"id": id, "text": "After move"});
        if spelling == "number" {
            params["onSlide"] = json!(1);
        }
        let result = call(&kits, "run", json!({"basis": seen["revision"], "steps": [{"command": "text.set", "params": params}]})).await;
        kits.shutdown().await;
        if spelling == "identity" {
            assert!(result.is_ok(), "persistent identity follows the shape: {result:?}");
        } else if spelling == "scoped" {
            // The pinned SDK guards a place only through a place parameter (`onSlide`): a scoped
            // address alone that names no shape now is refused by the command, not by the guard.
            let error = result.expect_err("the old address must not write a relocated shape");
            assert_eq!(error["error"]["code"], "command-failed", "{spelling}: {error}");
            assert!(error["error"]["message"].as_str().unwrap().contains("persistent tool id"), "{spelling}: {error}");
        } else {
            let error = result.expect_err("the old address must not write a relocated shape");
            assert_eq!(error["error"]["code"], "conflict", "{spelling}: {error}");
            assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == "relocated"), "{spelling}: {error}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn renumbered_positional_addresses_refuse_relocated() {
    for spelling in ["number", "scoped"] {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("Identity.pptx");
        let mut doc = SlideDocument::create("Identity.pptx", 960, 540).unwrap();
        for text in ["Held", "Other"] {
            doc.execute("textBox.insert", json!({"text": text, "rect": [10, 10, 100, 50]})).unwrap();
        }
        std::fs::write(&path, doc.save(Some("Identity.pptx")).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        let seen = call(&kits, "inspect", json!({"target": {"slide": 1}})).await.unwrap();
        let shapes = seen["part"]["shapes"].as_array().unwrap();
        let a = shapes.iter().find(|s| s["text"] == "Held").unwrap();
        let b = shapes.iter().find(|s| s["text"] == "Other").unwrap();
        let mut file = kition_slide_engine::cmd::file::open_slide_file("Identity.pptx", &std::fs::read(&path).unwrap()).unwrap();
        for shape in &mut std::sync::Arc::make_mut(&mut file.slides[0]).shapes {
            if shape.id.0 == a["id"].as_u64().unwrap() as u32 {
                shape.id.0 = b["id"].as_u64().unwrap() as u32;
            } else if shape.id.0 == b["id"].as_u64().unwrap() as u32 {
                shape.id.0 = a["id"].as_u64().unwrap() as u32;
            }
        }
        std::fs::write(&path, kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()).unwrap();
        let params = match spelling {
            "scoped" => json!({"id": format!("shape:{}/{}", seen["part"]["id"], a["id"]), "text": "Wrong target"}),
            _ => json!({"id": a["id"], "onSlide": 1, "text": "Wrong target"}),
        };
        let result = call(&kits, "run", json!({"basis": seen["revision"], "steps": [{"command": "text.set", "params": params}]})).await;
        kits.shutdown().await;
        let error = result.expect_err("renumbering must invalidate the positional address");
        assert_eq!(error["error"]["code"], "conflict", "{spelling}: {error}");
        assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == "relocated"), "{spelling}: {error}");
    }
}
