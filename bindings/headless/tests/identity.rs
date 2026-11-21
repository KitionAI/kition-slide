//! `document.identity` and `document.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-slide` process. A `.kslide` holds `slide.json` and the media, and a
//! `.pptx` is PowerPoint's own package (`kition_slide_engine::cmd::file::save_bytes`): neither
//! embeds a replica, and the kit makes a new one, under a new id, each time it opens a file. So a
//! file carries no identity (`docId` is `null`) and its fork is the same bytes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kit_host::{HeadlessKit, LaunchOptions};
use kition_slide_kit::SlideDocument;

const LIMIT: Duration = Duration::from_secs(30);

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-slide-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-slide"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A new deck saved as `name` (`.kslide` or `.pptx`).
fn deck(name: &str) -> Vec<u8> {
    SlideDocument::create(name, 960, 540).unwrap().save(Some(name)).unwrap()
}

#[tokio::test]
async fn a_slide_file_carries_no_identity_and_forks_into_the_same_bytes() {
    let dir = workspace("plain");
    let (kslide, pptx) = (deck("Plan.kslide"), deck("Plan.pptx"));
    std::fs::write(dir.join("Plan.kslide"), &kslide).unwrap();
    std::fs::write(dir.join("Plan.pptx"), &pptx).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("document.identity"), "{}", kit.capabilities());
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap();
    assert_eq!(kit.capabilities()["features"], manifest["headless"]["features"], "kit.json lists exactly the features kit.initialize answers");

    // A saved deck holds no replica: no identity, for either format, and for an empty file.
    assert_eq!(kit.identity("Plan.kslide", LIMIT).await.unwrap().doc_id, None);
    assert_eq!(kit.identity("Plan.pptx", LIMIT).await.unwrap().doc_id, None);
    std::fs::write(dir.join("Empty.kslide"), b"").unwrap();
    assert_eq!(kit.identity("Empty.kslide", LIMIT).await.unwrap().doc_id, None);

    // Its fork is the same bytes under the new name, with no docId; the source is unchanged.
    for (source, target, bytes) in [("Plan.kslide", "Fork.kslide", &kslide), ("Plan.pptx", "Fork.pptx", &pptx)] {
        let fork = kit.fork(source, target, LIMIT).await.unwrap();
        assert_eq!(fork.doc_id, None, "{target}");
        assert_eq!(fork.name, target);
        assert!(!fork.version.is_empty(), "{target}");
        assert_eq!(&std::fs::read(dir.join(target)).unwrap(), bytes, "a file without a replica forks unchanged: {target}");
        assert_eq!(&std::fs::read(dir.join(source)).unwrap(), bytes, "the source is unchanged: {source}");
        // The fork opens as a deck.
        let state = kit.open(target, true, LIMIT).await.unwrap();
        assert!(state["documentId"].is_string(), "the fork opens as a deck: {state}");
        kit.request("document.close", serde_json::json!({ "documentId": state["documentId"] }), LIMIT).await.unwrap();
    }

    // A fork of the fork is the same bytes again.
    let again = kit.fork("Fork.kslide", "Again.kslide", LIMIT).await.unwrap();
    assert_eq!(again.doc_id, None);
    assert_eq!(std::fs::read(dir.join("Again.kslide")).unwrap(), kslide);

    // A fork never replaces a file.
    std::fs::write(dir.join("Taken.kslide"), b"mine").unwrap();
    let taken = kit.fork("Plan.kslide", "Taken.kslide", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Taken.kslide")).unwrap(), b"mine", "the existing file is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
