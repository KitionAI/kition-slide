//! Waiting for the SDK at 34dff06: replay a slide creation with its owned notes body.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use kition_env::{EnvKit, ops_for, read_snapshot};
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn waiting_for_sdk_slide_creation_replays_its_owned_notes_body() {
    let name = "Replay.pptx";
    let doc = SlideDocument::create(name, 960, 540).unwrap();
    let bytes = doc.save(Some(name)).unwrap();
    let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let env = EnvKit::new(Box::leak(Box::new(definition)), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))));
    let snapshot = read_snapshot(&env, name, &bytes).await.unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join(name), &bytes).unwrap();
    let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(env.definition, env.executable.clone())]);
    let made = tools::call_as(
        &kits,
        "slide_batch",
        &json!({"path": name, "ops": [
            {"op": "create", "type": "Slide", "in": "root.slides", "at": 1}
        ]}),
        CallOptions::default(),
        "replay-agent",
    )
    .await
    .unwrap()
    .unwrap()
    .output;
    let changes = &made["changes"];
    println!("slide creation receipt: {changes}");
    let ops = ops_for(changes, &snapshot).unwrap();
    kits.shutdown().await;
    let replay_workspace = tempfile::tempdir().unwrap();
    std::fs::write(replay_workspace.path().join(name), &bytes).unwrap();
    let fresh = KitManager::new(Some(replay_workspace.path().to_path_buf()), vec![(env.definition, env.executable.clone())]);
    let replayed = tools::call_as(&fresh, "slide_batch", &json!({"path": name, "ops": ops}), CallOptions::default(), "replay-agent").await.unwrap();
    fresh.shutdown().await;
    assert!(replayed.is_ok(), "waiting for the SDK: ops_for must recreate a held notesBody and replay held paths; ops={ops:?}, result={replayed:?}");
}
