//! C8-slide-read: independent disk writers change inputs to text and table handlers.
//! `waiting_for_sdk_*` keeps the required refusal assertion where the stale write still applies.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::text::{AutoFit, LevelStyle, RunProps, TextBody, TextDir};
use kition_slide_model::{Fill, LayoutType, PhType, Placeholder, Shape, ShapeId, SlideFile};
use serde_json::{Value, json};

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    target: String,
}

impl Fixture {
    fn new(placeholder: bool, grouped: bool) -> Self {
        let mut doc = SlideDocument::create("Text.pptx", 960, 540).unwrap();
        let target = doc.execute("textBox.insert", json!({"text": "Before", "rect": [60, 60, 240, 80]})).unwrap()["id"].as_u64().unwrap();
        if grouped {
            let neighbour = doc.execute("shape.insert", json!({"preset": "rect", "rect": [320, 60, 40, 80]})).unwrap()["id"].clone();
            doc.execute("arrange.group", json!({"ids": [target, neighbour]})).unwrap();
        }
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("Text.pptx");
        std::fs::write(&path, doc.save(Some("Text.pptx")).unwrap()).unwrap();
        let mut deck = open(&path);
        let layout = deck.masters[0].layouts.iter().find(|layout| layout.kind == LayoutType::TitleAndContent).unwrap().id;
        Arc::make_mut(&mut deck.slides[0]).layout = layout;
        let target = ShapeId(target as u32);
        let shape = kition_slide_model::find_shape_mut(&mut Arc::make_mut(&mut deck.slides[0]).shapes, target).unwrap();
        shape.name = "Read target".into();
        shape.ph = placeholder.then_some(Placeholder { kind: PhType::Body, idx: 1, ..Default::default() });
        shape.text_box = !placeholder;
        shape.text = Some(TextBody::from_text("Before"));
        shape.text.as_mut().unwrap().body.autofit =
            Some(if grouped { AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 } } else { AutoFit::Shape });
        if grouped {
            shape.text.as_mut().unwrap().body.vert = Some(TextDir::EaVertical);
        }
        let target = kition_slide_engine::cmd::inspect::shape_tool_id(shape);
        if placeholder {
            let master = Arc::make_mut(&mut deck.masters[0]);
            let layout = master.layouts.iter_mut().find(|item| item.id == layout).unwrap();
            let parent =
                layout.shapes.iter_mut().find(|shape| shape.ph_type() == Some(PhType::Body) || shape.ph_type() == Some(PhType::Obj)).unwrap();
            parent.ph = Some(Placeholder { kind: PhType::Body, idx: 1, ..Default::default() });
            parent.text = Some(TextBody::default());
            parent
                .text
                .as_mut()
                .unwrap()
                .list_style
                .set(0, LevelStyle { run: RunProps { size: Some(18.0), ..Default::default() }, ..Default::default() });
        }
        std::fs::write(&path, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace, target }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join("Text.pptx")
    }

    fn rewrite(&self, change: impl FnOnce(&mut SlideFile)) {
        let path = self.file();
        let mut deck = open(&path);
        change(&mut deck);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(&path, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(path).unwrap().set_modified(modified).unwrap();
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!("Text.pptx");
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "text-reader").await.unwrap() {
            Ok(output) => Ok(output.output),
            Err(error) => Err(error.structured()),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target": {"slide": 1}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    async fn write(&self, basis: &str, command: &str, related: bool) {
        let bytes = std::fs::read(self.file()).unwrap();
        let params = match command {
            "text.set" => json!({"id": self.target, "text": "Mine"}),
            "text.edit" => json!({"id": self.target}),
            _ => json!({"ids": [self.target]}),
        };
        let result = self.call("run", json!({"basis": basis, "steps": [{"command": command, "params": params}]})).await;
        if related {
            let error = result.expect_err("an input read by the handler changed on disk; the stale write must refuse");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.file()).unwrap(), bytes, "refusal wrote the file");
        } else {
            result.unwrap_or_else(|error| panic!("unrelated change refused: {error}"));
            let mut deck = open(&self.file());
            let shape = target_mut(&mut deck);
            if command == "text.set" {
                assert_eq!(shape.text.as_ref().unwrap().text(), "Mine");
            }
        }
    }
}

fn open(path: &std::path::Path) -> SlideFile {
    kition_slide_engine::cmd::file::open_slide_file("Text.pptx", &std::fs::read(path).unwrap()).unwrap()
}

fn target_mut(deck: &mut SlideFile) -> &mut Shape {
    let slide = Arc::make_mut(&mut deck.slides[0]);
    let id = {
        let mut id = None;
        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
            if shape.name == "Read target" {
                id = Some(shape.id)
            }
        });
        id.unwrap()
    };
    kition_slide_model::find_shape_mut(&mut slide.shapes, id).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn layout_placeholder_text_style_is_held_but_another_slides_name_is_not() {
    for related in [false, true] {
        let f = Fixture::new(true, false);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let layout_id = deck.slides[0].layout;
                let layout = Arc::make_mut(&mut deck.masters[0]).layouts.iter_mut().find(|layout| layout.id == layout_id).unwrap();
                let shape = layout.shapes.iter_mut().find(|shape| shape.ph_type() == Some(PhType::Body)).unwrap();
                shape
                    .text
                    .as_mut()
                    .unwrap()
                    .list_style
                    .set(0, LevelStyle { run: RunProps { size: Some(37.0), ..Default::default() }, ..Default::default() });
            } else {
                Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
            }
        });
        f.write(&basis, "format.increaseFontSize", related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn master_text_style_is_held_but_master_name_is_not() {
    for related in [false, true] {
        let f = Fixture::new(true, false);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let master = Arc::make_mut(&mut deck.masters[0]);
            if related {
                master.body_style.set(0, LevelStyle { run: RunProps { bold: Some(true), ..Default::default() }, ..Default::default() });
            } else {
                master.name = "Unrelated master name".into();
            }
        });
        f.write(&basis, "format.bold", related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn theme_fonts_are_held_but_master_name_is_not() {
    for related in [false, true] {
        let f = Fixture::new(false, false);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let master = Arc::make_mut(&mut deck.masters[0]);
            if related {
                master.theme.fonts.minor.latin = "Lora".into();
            } else {
                master.name = "Unrelated master name".into();
            }
        });
        f.write(&basis, "text.set", related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn default_text_style_is_held_but_other_slides_name_is_not() {
    for related in [false, true] {
        let f = Fixture::new(false, false);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.default_text_style.set(0, LevelStyle { run: RunProps { size: Some(37.0), ..Default::default() }, ..Default::default() });
            } else {
                Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
            }
        });
        f.write(&basis, "format.increaseFontSize", related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ancestor_group_frame_is_held_but_another_slides_name_is_not() {
    for related in [false, true] {
        let f = Fixture::new(false, true);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let group = Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| shape.is_group()).unwrap();
                group.xfrm.as_mut().unwrap().rot = 90.0;
            } else {
                Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
            }
        });
        f.write(&basis, "text.set", related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn same_target_fill_change_does_not_refuse_text_set() {
    let f = Fixture::new(false, false);
    let basis = f.basis().await;
    f.rewrite(|deck| {
        target_mut(deck).fill = Some(Fill::None);
    });
    f.write(&basis, "text.set", false).await;
    assert_eq!(target_mut(&mut open(&f.file())).fill, Some(Fill::None));
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn entering_text_edit_holds_the_body_it_may_create_but_allows_another_slide_name() {
    for related in [true, false] {
        let f = Fixture::new(false, false);
        // The handler creates a body only where there is none: a colleague who removed the
        // body makes the stale edit create one, so the write must refuse.
        if !related {
            f.rewrite(|deck| target_mut(deck).text = None);
        }
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                target_mut(deck).text = None;
            } else {
                Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
            }
        });
        f.write(&basis, "text.edit", related).await;
        if !related {
            assert!(target_mut(&mut open(&f.file())).text.is_some(), "the command's stored body must be saved");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn formatting_without_a_body_holds_default_typography_but_allows_another_slide_name() {
    for related in [true, false] {
        let f = Fixture::new(false, false);
        f.rewrite(|deck| {
            let shape = target_mut(deck);
            shape.text = None;
            shape.text_box = false;
        });
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.default_text_style.set(0, LevelStyle { run: RunProps { bold: Some(true), ..Default::default() }, ..Default::default() });
            } else {
                Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
            }
        });
        f.write(&basis, "format.bold", related).await;
        f.kits.shutdown().await;
    }
}
