//! `kition-slide` refuses an agent's call whose params do not match the command's schema (KHP A5,
//! A18) before the engine runs it, and leaves the file as it was: `shape.insert` once took any
//! `preset` and `rect` and drew a default rectangle (khp check phase 5).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{ErrorReason, KitBackend, OpenRequest};
use kition_slide_kit::SlideDocument;
use serde_json::{Map, Value, json};

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn ill_typed_params_are_refused_and_change_nothing() {
    let root = std::env::temp_dir().join(format!("kition-slide-params-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("Deck.kslide"), SlideDocument::create("Deck.kslide", 960, 540).unwrap().save(None).unwrap()).unwrap();

    let mut kit = Headless::<SlideDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_slide_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Deck.kslide").unwrap(), read_only: false, create: None }).unwrap().id;
    let before = kit.describe(&id).unwrap().state;
    for bad in [json!({ "preset": { "khp-check": "not a valid value" }, "rect": [10, 10, 50, 50] }), json!({ "preset": "rect", "rect": { "x": 10 } })]
    {
        let error = kit.execute(&id, "shape.insert", params(bad.clone()), Origin::Agent).unwrap_err();
        assert_eq!(error.code, ErrorReason::InvalidParams.code(), "{bad}: {}", error.message);
    }
    assert_eq!(kit.describe(&id).unwrap().state, before, "nothing ran");
    kit.execute(&id, "shape.insert", params(json!({ "preset": "ellipse", "rect": [360, 200, 120, 120] })), Origin::Agent).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A command that cannot run says why by a typed reason (`kition_kit::failure`, task card
/// C8-slide-4), not by wording the host would have to read: a picture command with no picture
/// selected is `command_disabled`, and the message the agent reads carries no reason marker.
#[test]
fn a_command_that_cannot_run_says_why_by_a_typed_reason() {
    let mut doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    let message = doc.execute("picture.reset", json!({})).unwrap_err();
    assert_eq!(
        kition_kit::failure::parse_failure(&message).map(|(_, reason)| reason),
        Some(kition_kit::failure::CommandFailure::Disabled),
        "{message}"
    );

    let root = std::env::temp_dir().join(format!("kition-slide-failure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("Deck.kslide"), doc.save(None).unwrap()).unwrap();
    let mut kit = Headless::<SlideDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_slide_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Deck.kslide").unwrap(), read_only: false, create: None }).unwrap().id;
    let error = kit.execute(&id, "picture.reset", params(json!({})), Origin::Agent).unwrap_err();
    assert_eq!(error.code, ErrorReason::CommandDisabled.code(), "{}", error.message);
    assert!(!error.message.contains(kition_kit::failure::FAILURE_SEPARATOR), "{:?}", error.message);
    let _ = std::fs::remove_dir_all(root);
}
