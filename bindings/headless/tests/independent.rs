//! File-writer probes derived from the C8-slide-read handler reading.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{LayoutType, SlideFile};
use serde_json::{Value, json};

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create("Independent.pptx", 960, 540).unwrap();
        for n in 1..=3 {
            if n > 1 {
                doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
            }
            doc.execute("insert.textBox", json!({ "text": format!("Body {n}"), "rect": [60, 60, 240, 80] })).unwrap();
        }
        doc.execute("transition.set", json!({ "index": 0, "kind": "fade", "duration": 600 })).unwrap();
        doc.execute("transition.set", json!({ "index": 1, "kind": "fade", "duration": 600 })).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Independent.pptx"), doc.save(Some("Independent.pptx")).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join("Independent.pptx")
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file("Independent.pptx", &std::fs::read(self.file()).unwrap()).unwrap()
    }

    fn rewrite(&self, change: impl FnOnce(&mut SlideFile)) {
        rewrite(&self.file(), change);
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!("Independent.pptx");
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "independent-agent").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("slide_{action} is missing"),
        }
    }

    async fn basis(&self) -> String {
        let seen = self.call("inspect", json!({ "target": { "slide": 1 } })).await.unwrap();
        seen["revision"].as_str().unwrap().to_owned()
    }

    async fn run(&self, basis: &str, step: Value, must_refuse: bool) -> Value {
        let bytes = std::fs::read(self.file()).unwrap();
        let answer = self.call("run", json!({ "basis": basis, "steps": [step] })).await;
        if must_refuse {
            let error = answer.unwrap_err();
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.file()).unwrap(), bytes, "refusal wrote the file");
            error
        } else {
            answer.unwrap_or_else(|error| panic!("unrelated change refused: {error}"))
        }
    }
}

fn rewrite(path: &Path, change: impl FnOnce(&mut SlideFile)) {
    let mut deck = kition_slide_engine::cmd::file::open_slide_file("Independent.pptx", &std::fs::read(path).unwrap()).unwrap();
    change(&mut deck);
    let modified = std::fs::metadata(path).unwrap().modified().unwrap() - Duration::from_secs(3600);
    std::fs::write(path, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
    std::fs::File::options().write(true).open(path).unwrap().set_modified(modified).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn copied_current_transition_holds_source_but_allows_unrelated_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                Arc::make_mut(&mut deck.slides[0]).transition.as_mut().unwrap().duration_ms = 1500;
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Colleague's unrelated title".into();
            }
        });
        f.run(&basis, json!({ "command": "transition.applyAll" }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides[2].name, "Colleague's unrelated title");
            assert!(deck.slides.iter().all(|slide| slide.transition.as_ref().is_some_and(|t| t.duration_ms == 600)));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_holds_named_layout_placeholders_but_allows_other_slide_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let layout = f.deck().masters[0].layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap().name.clone();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let master = Arc::make_mut(&mut deck.masters[0]);
                let layout = master.layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
                let title = layout.shapes.iter_mut().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap();
                title.name = "Colleague's placeholder".into();
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Unrelated".into();
            }
        });
        f.run(&basis, json!({ "command": "slide.new", "params": { "layout": layout, "at": 1, "title": "Mine" } }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides.len(), 4);
            assert_eq!(deck.slides[3].name, "Unrelated");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn notes_hold_replaced_notes_but_allow_the_slides_name_to_change() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let slide = Arc::make_mut(&mut deck.slides[0]);
            if related {
                slide.notes = kition_slide_model::text::TextBody::from_text("Colleague's notes");
            } else {
                slide.name = "Colleague's title".into();
            }
        });
        f.run(&basis, json!({"command":"slide.notes", "params":{"index":0, "text":"Mine"}}), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn show_settings_hold_replaced_settings_but_allow_unrelated_title() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.show.loop_until_esc = true;
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Colleague's title".into();
            }
        });
        f.run(&basis, json!({"command":"show.setup", "params":{"loop":false}}), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn custom_show_holds_referenced_slide_indexes_but_allows_unrelated_title() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.slides.swap(0, 1);
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Colleague's title".into();
            }
        });
        f.run(&basis, json!({"command":"show.customShow", "params":{"name":"Mine", "slides":[0]}}), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn slide_move_holds_membership_but_allows_unrelated_section_names() {
    for related in [true, false] {
        let f = Fixture::new();
        f.rewrite(|deck| {
            deck.sections = vec![
                kition_slide_model::Section { identity: String::new(), name: "A".into(), slides: vec![deck.slides[0].id, deck.slides[1].id] },
                kition_slide_model::Section { identity: String::new(), name: "B".into(), slides: vec![deck.slides[2].id] },
            ];
        });
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let moved = deck.sections[0].slides.remove(1);
                deck.sections[1].slides.insert(0, moved);
            } else {
                deck.sections[1].name = "Colleague's name".into();
            }
        });
        f.run(&basis, json!({"command":"slide.move", "params":{"from":0,"to":1}}), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn transition_timing_index_holds_slide_place_but_allows_an_unrelated_rename() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if related {
            let path = f.file();
            let mut writer = SlideDocument::open("Independent.pptx", &std::fs::read(&path).unwrap()).unwrap();
            writer.execute("slide.new", json!({ "layout": "blank", "at": 0 })).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap() - Duration::from_secs(3600);
            std::fs::write(&path, writer.save(Some("Independent.pptx")).unwrap()).unwrap();
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
        } else {
            f.rewrite(|deck| Arc::make_mut(&mut deck.slides[2]).name = "Unrelated".into());
        }
        f.run(&basis, json!({ "command": "transition.timing", "params": { "index": 1, "duration": 1700 } }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides[1].transition.as_ref().unwrap().duration_ms, 1700);
            assert_eq!(deck.slides[2].name, "Unrelated");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn shape_move_unpublished_single_id_alias_is_refused_before_writing() {
    for related in [true, false] {
        let f = Fixture::new();
        let deck = f.deck();
        let shape = deck.slides[0].shapes.iter().find(|shape| shape.text.as_ref().is_some_and(|text| text.text() == "Body 1")).unwrap();
        let target = kition_slide_engine::cmd::inspect::shape_tool_id(shape);
        let number = shape.id;
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let slide = Arc::make_mut(&mut deck.slides[0]);
                let shape = slide.shapes.iter_mut().find(|shape| shape.id == number).unwrap();
                shape.text = Some(kition_slide_model::text::TextBody::from_text("Colleague's text"));
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Unrelated".into();
            }
        });
        let bytes = std::fs::read(f.file()).unwrap();
        let error =
            f.call("run", json!({ "basis": basis, "steps": [{ "command": "shape.move", "params": { "id": target, "dx": 10 } }] })).await.unwrap_err();
        assert_eq!(error["error"]["code"], "invalid-params", "{error}");
        assert_eq!(std::fs::read(f.file()).unwrap(), bytes, "invalid unpublished parameter wrote the file");
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_allows_ordinary_layout_shape_text_to_change() {
    let f = Fixture::new();
    f.rewrite(|deck| {
        let id = kition_slide_model::ShapeId(deck.alloc_id());
        let master = Arc::make_mut(&mut deck.masters[0]);
        let layout = master.layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        layout.shapes.push(kition_slide_model::Shape {
            id,
            name: "Ordinary layout decoration".into(),
            text: Some(kition_slide_model::text::TextBody::from_text("Before")),
            ..Default::default()
        });
    });
    let layout = f.deck().masters[0].layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap().name.clone();
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let master = Arc::make_mut(&mut deck.masters[0]);
        let layout = master.layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        let decoration = layout.shapes.iter_mut().find(|shape| shape.name == "Ordinary layout decoration").unwrap();
        decoration.text = Some(kition_slide_model::text::TextBody::from_text("Colleague's decoration"));
    });
    f.run(&basis, json!({ "command": "slide.new", "params": { "layout": layout, "at": 1, "title": "Mine" } }), false).await;
    let deck = f.deck();
    assert_eq!(deck.slides.len(), 4);
    let layout = deck.masters[0].layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
    assert!(layout.shapes.iter().any(|shape| shape.text.as_ref().is_some_and(|text| text.text() == "Colleague's decoration")));
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_holds_its_layout_placeholder_text_style() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let master = Arc::make_mut(&mut deck.masters[0]);
        let layout = master.layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        let placeholder =
            layout.shapes.iter_mut().find(|shape| shape.ph.as_ref().is_some_and(|ph| !ph.kind.is_title() && !ph.kind.is_footer_kind())).unwrap();
        placeholder.text.as_mut().unwrap().list_style.set(
            0,
            kition_slide_model::text::LevelStyle {
                run: kition_slide_model::text::RunProps { size: Some(37.0), ..Default::default() },
                ..Default::default()
            },
        );
    });
    f.run(&basis, json!({"command":"slide.new", "params":{"layout":"titleAndContent", "at":1, "body":"Mine"}}), true).await;
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_holds_a_placeholder_losing_its_role() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let layout = Arc::make_mut(&mut deck.masters[0]).layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        layout.shapes.iter_mut().find(|shape| shape.ph.as_ref().is_some_and(|ph| !ph.kind.is_title() && !ph.kind.is_footer_kind())).unwrap().ph =
            None;
    });
    f.run(&basis, json!({"command":"slide.new", "params":{"layout":"titleAndContent", "at":1, "body":"Mine"}}), true).await;
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_holds_a_deleted_last_placeholder() {
    let f = Fixture::new();
    f.rewrite(|deck| {
        let id = kition_slide_model::ShapeId(deck.alloc_id());
        let layout = Arc::make_mut(&mut deck.masters[0]).layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        let mut tail =
            layout.shapes.iter().find(|shape| shape.ph.as_ref().is_some_and(|ph| !ph.kind.is_title() && !ph.kind.is_footer_kind())).unwrap().clone();
        tail.id = id;
        tail.identity.clear();
        tail.name = "Tail placeholder".into();
        tail.ph.as_mut().unwrap().idx = 99;
        layout.shapes.push(tail);
    });
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let layout = Arc::make_mut(&mut deck.masters[0]).layouts.iter_mut().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap();
        assert_eq!(layout.shapes.pop().unwrap().name, "Tail placeholder");
    });
    f.run(&basis, json!({"command":"slide.new", "params":{"layout":"titleAndContent", "at":1, "body":"Mine"}}), true).await;
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_slide_holds_a_deleted_default_layout() {
    let f = Fixture::new();
    f.rewrite(|deck| {
        let id = kition_slide_model::LayoutId(deck.alloc_id());
        let master = Arc::make_mut(&mut deck.masters[0]);
        let mut fallback = master.layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap().clone();
        fallback.id = id;
        fallback.name = "Fallback layout".into();
        master.layouts.push(fallback);
    });
    let original = f.deck().masters[0].layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap().name.clone();
    let basis = f.basis().await;
    f.rewrite(|deck| Arc::make_mut(&mut deck.masters[0]).layouts.retain(|layout| layout.name != original));
    f.run(&basis, json!({"command":"slide.new", "params":{"at":1, "body":"Mine"}}), true).await;
    f.kits.shutdown().await;
}
