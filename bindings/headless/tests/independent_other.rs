//! Handler-derived media reads with a second writer of the saved file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{ShapeKind, SlideFile};
use serde_json::{Value, json};

const NAME: &str = "Poster.pptx";
const ORIGINAL: &[u8] = include_bytes!("../../../crates/media/media/tests/fixtures/clip-h264.mp4");
const REPLACEMENT: &[u8] = include_bytes!("../../../crates/media/media/tests/fixtures/clip-hevc.mp4");

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    target: String,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        doc.execute("insert.video", json!({"name": "clip.mp4", "data": kition_slide_engine::cmd::base64_encode(ORIGINAL)})).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let bytes = doc.save(Some(NAME)).unwrap();
        let saved = kition_slide_engine::cmd::file::open_slide_file(NAME, &bytes).unwrap();
        let target = saved.slides[1]
            .shapes
            .iter()
            .find(|shape| matches!(shape.kind, ShapeKind::Media(_)))
            .map(kition_slide_engine::cmd::inspect::shape_tool_id)
            .unwrap();
        std::fs::write(workspace.path().join(NAME), bytes).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace, target }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(NAME)
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(self.file()).unwrap()).unwrap()
    }

    fn rewrite(&self, related: bool) {
        let mut deck = self.deck();
        if related {
            let media = deck.slides[1]
                .shapes
                .iter()
                .find_map(|shape| match &shape.kind {
                    ShapeKind::Media(clip) => Some(clip.media),
                    _ => None,
                })
                .unwrap();
            deck.media.iter_mut().find(|item| item.id == media).unwrap().data = Arc::new(REPLACEMENT.to_vec());
        } else {
            Arc::make_mut(&mut deck.slides[2]).name = "Colleague's unrelated title".into();
        }
        let changed_time = std::fs::metadata(self.file()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.file(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.file()).unwrap().set_modified(changed_time).unwrap();
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "media-reader").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing slide_{action}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn poster_holds_video_bytes_but_allows_unrelated_slide_title() {
    for related in [true, false] {
        let fixture = Fixture::new();
        let seen = fixture.call("inspect", json!({"target": {"slide": 2}})).await.unwrap();
        let basis = seen["revision"].as_str().unwrap();
        fixture.rewrite(related);
        let before = std::fs::read(fixture.file()).unwrap();
        let result = fixture
            .call("run", json!({"basis": basis, "steps": [{"command": "media.posterFrame", "params": {"id": fixture.target, "ms": 0}}]}))
            .await;
        if related {
            let error = result.unwrap_err();
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(fixture.file()).unwrap(), before, "refusal wrote saved bytes");
        } else {
            result.unwrap_or_else(|error| panic!("unrelated slide title refused: {error}"));
            assert_eq!(fixture.deck().slides[2].name, "Colleague's unrelated title");
        }
        fixture.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn poster_allows_unrelated_slide_title() {
    let fixture = Fixture::new();
    let seen = fixture.call("inspect", json!({"target": {"slide": 2}})).await.unwrap();
    let basis = seen["revision"].as_str().unwrap();
    fixture.rewrite(false);
    let result =
        fixture.call("run", json!({"basis": basis, "steps": [{"command": "media.posterFrame", "params": {"id": fixture.target, "ms": 0}}]})).await;
    result.unwrap_or_else(|error| panic!("unrelated slide title refused: {error}"));
    assert_eq!(fixture.deck().slides[2].name, "Colleague's unrelated title");
    fixture.kits.shutdown().await;
}
