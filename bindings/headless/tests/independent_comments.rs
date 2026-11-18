//! Comment ordinals are not slide ordinals (review.rs::find_thread).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::SlideFile;
use serde_json::{Value, json};

const NAME: &str = "Comments.pptx";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
        for slide in 0..3 {
            if slide > 0 {
                doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
            }
            for index in 0..3 {
                doc.execute(
                    "comment.add",
                    json!({"slide": slide, "text": format!("Thread {slide}:{index}"), "author": "reader", "name": "Reader",
                        "created": format!("2026-11-13T10:00:0{index}Z")}),
                )
                .unwrap();
            }
        }
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(NAME), doc.save(Some(NAME)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(NAME)
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(self.file()).unwrap()).unwrap()
    }

    fn rewrite(&self, change: impl FnOnce(&mut SlideFile)) {
        let mut deck = self.deck();
        change(&mut deck);
        let modified = std::fs::metadata(self.file()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.file(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.file()).unwrap().set_modified(modified).unwrap();
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "comment-reader").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing slide_{action}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_ordinals_hold_thread_identity_but_allow_unrelated_slide_names() {
    for command in ["comment.reply", "comment.delete", "comment.resolve"] {
        for positional in [json!({"index": 1}), json!({"thread": "@2:1"})] {
            for related in [true, false] {
                let f = Fixture::new();
                let seen = f.call("inspect", json!({"target": {"slide": 3}})).await.unwrap();
                let basis = seen["revision"].as_str().unwrap();
                let target = f.deck().slides[2].comments[1].id.clone();
                f.rewrite(|deck| {
                    if related {
                        Arc::make_mut(&mut deck.slides[2]).comments.remove(0);
                    } else {
                        Arc::make_mut(&mut deck.slides[0]).name = "Colleague's unrelated name".into();
                    }
                });
                let before = std::fs::read(f.file()).unwrap();
                let mut params = positional.clone();
                if command == "comment.reply" {
                    params["text"] = json!("My reply");
                    params["name"] = json!("Reader");
                    params["created"] = json!("2026-11-13T11:00:00Z");
                }
                let result =
                    f.call("run", json!({"basis": basis, "steps": [{"command": command, "params": params, "at": {"part": {"slide": 3}}}]})).await;
                f.kits.shutdown().await;
                if related {
                    let error = result.expect_err(&format!("{command} {positional} wrote to a different thread after the ordinal changed"));
                    assert_eq!(error["error"]["code"], "conflict", "{command} {positional}: {error}");
                    assert_eq!(std::fs::read(f.file()).unwrap(), before, "refusal wrote bytes");
                } else {
                    result.unwrap_or_else(|error| panic!("{command} {positional}: unrelated slide name refused: {error}"));
                    let deck = f.deck();
                    assert_eq!(deck.slides[0].name, "Colleague's unrelated name");
                    let thread = deck.slides[2].comments.iter().find(|thread| thread.id == target);
                    match command {
                        "comment.reply" => assert_eq!(thread.unwrap().replies.last().unwrap().text, "My reply"),
                        "comment.resolve" => assert!(thread.unwrap().resolved),
                        "comment.delete" => assert!(thread.is_none()),
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
}
