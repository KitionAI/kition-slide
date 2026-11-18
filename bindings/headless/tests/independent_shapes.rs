//! Independent handler-derived shape premises, with an actual second file writer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{Fill, Shape, ShapeKind, SlideFile, TextBody};
use serde_json::{Value, json};

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create("Shapes.pptx", 960, 540).unwrap();
        doc.execute("shape.insert", json!({"preset": "rect", "rect": [40, 40, 100, 60], "text": "Fill target"})).unwrap();
        let a = doc.execute("shape.insert", json!({"preset": "rect", "rect": [200, 40, 100, 60], "text": "Group one"})).unwrap()["id"].clone();
        let b = doc.execute("shape.insert", json!({"preset": "rect", "rect": [320, 40, 100, 60], "text": "Group two"})).unwrap()["id"].clone();
        doc.execute("arrange.group", json!({"ids": [a, b]})).unwrap();
        doc.execute("slide.new", json!({"layout": "titleAndContent", "title": "Destination title", "body": "Destination body"})).unwrap();
        doc.execute("slide.new", json!({"layout": "blank", "title": "Unrelated"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Shapes.pptx"), doc.save(Some("Shapes.pptx")).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn path(&self) -> PathBuf {
        self.workspace.path().join("Shapes.pptx")
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file("Shapes.pptx", &std::fs::read(self.path()).unwrap()).unwrap()
    }

    fn rewrite(&self, edit: impl FnOnce(&mut SlideFile)) {
        let mut deck = self.deck();
        edit(&mut deck);
        let old_time = std::fs::metadata(self.path()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.path(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.path()).unwrap().set_modified(old_time).unwrap();
    }

    fn target(&self, text: &str) -> String {
        let deck = self.deck();
        let mut target = None;
        kition_slide_model::walk(&deck.slides[0].shapes, &mut |shape, _| {
            if shape.text.as_ref().is_some_and(|body| body.text() == text) {
                target = Some(kition_slide_engine::cmd::inspect::shape_tool_id(shape));
            }
        });
        target.unwrap()
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Shapes.pptx");
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "shape-reader").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing tool: slide_{action}"),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target": {"slide": 1}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    async fn run(&self, basis: &str, steps: Value, refusal: bool) -> Value {
        let saved = std::fs::read(self.path()).unwrap();
        let answer = self.call("run", json!({"basis": basis, "steps": steps})).await;
        if refusal {
            let error = answer.unwrap_err();
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.path()).unwrap(), saved, "refusal changed saved bytes");
            error
        } else {
            answer.unwrap_or_else(|error| panic!("unrelated rewrite refused: {error}"))
        }
    }
}

fn find_text_mut<'a>(shapes: &'a mut [Shape], text: &str) -> &'a mut Shape {
    for shape in shapes {
        if shape.text.as_ref().is_some_and(|body| body.text() == text) {
            return shape;
        }
        if let ShapeKind::Group { children, .. } = &mut shape.kind
            && children.iter().any(|child| child.text.as_ref().is_some_and(|body| body.text() == text))
        {
            return find_text_mut(children, text);
        }
    }
    panic!("no shape with text {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn inherited_fill_holds_its_theme_color_but_not_other_theme_fonts() {
    for related in [true, false] {
        let f = Fixture::new();
        let target = f.target("Fill target");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let theme = &mut Arc::make_mut(&mut deck.masters[0]).theme;
            if related {
                theme.colors.colors[4] = theme.colors.colors[5];
            } else {
                theme.fonts.name = "Colleague font scheme".into();
            }
        });
        f.run(&basis, json!([{"command": "shape.fill", "params": {"ids": [target], "transparency": 0.5}}]), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn inherited_fill_allows_an_unused_theme_font_change() {
    let f = Fixture::new();
    let target = f.target("Fill target");
    let basis = f.basis().await;
    f.rewrite(|deck| Arc::make_mut(&mut deck.masters[0]).theme.fonts.name = "Colleague font scheme".into());
    f.run(&basis, json!([{"command": "shape.fill", "params": {"ids": [target], "transparency": 0.5}}]), false).await;
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn group_member_move_holds_ancestor_transform_but_not_ancestor_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let target = f.target("Group one");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let slide = Arc::make_mut(&mut deck.slides[0]);
            let group = slide.shapes.iter_mut().find(|shape| matches!(shape.kind, ShapeKind::Group { .. })).unwrap();
            if related {
                group.xfrm.as_mut().unwrap().x += 80.0;
            } else {
                group.name = "Colleague group name".into();
            }
        });
        f.run(&basis, json!([{"command": "shape.move", "params": {"ids": [target], "dx": 10}}]), related).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_fill_holds_replaced_fill_but_allows_same_targets_text_change() {
    for related in [true, false] {
        let f = Fixture::new();
        let target = f.target("Fill target");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let shape = find_text_mut(&mut Arc::make_mut(&mut deck.slides[0]).shapes, "Fill target");
            if related {
                shape.fill = Some(Fill::None);
            } else {
                shape.text = Some(TextBody::from_text("Colleague text"));
            }
        });
        f.run(&basis, json!([{"command": "shape.fill", "params": {"ids": [target], "color": "#126E57"}}]), related).await;
        if !related {
            assert!(f.deck().slides[0].shapes.iter().any(|shape| shape.text.as_ref().is_some_and(|body| body.text() == "Colleague text")));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hyperlink_holds_destination_index_but_not_unused_destination_title() {
    for related in [true, false] {
        let f = Fixture::new();
        let target = f.target("Fill target");
        let destination = f.deck().slides[1].id;
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let mut inserted = (*deck.slides[2]).clone();
                inserted.id = kition_slide_model::SlideId(deck.alloc_id());
                inserted.shapes.clear();
                deck.slides.insert(1, Arc::new(inserted));
            } else {
                find_text_mut(&mut Arc::make_mut(&mut deck.slides[1]).shapes, "Destination title").text =
                    Some(TextBody::from_text("Colleague title"));
            }
        });
        f.run(&basis, json!([{"command": "insert.hyperlink", "params": {"ids": [target], "slide": 1}}]), related).await;
        if !related {
            let deck = f.deck();
            let action = deck.slides[0]
                .shapes
                .iter()
                .find(|shape| shape.text.as_ref().is_some_and(|body| body.text() == "Fill target"))
                .unwrap()
                .click
                .as_ref()
                .unwrap();
            assert_eq!(action.action, kition_slide_model::text::Action::Slide { slide: destination });
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hyperlink_default_display_holds_destination_title_after_text_edit_step() {
    for related in [true, false] {
        let f = Fixture::new();
        let target = f.target("Fill target");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                find_text_mut(&mut Arc::make_mut(&mut deck.slides[1]).shapes, "Destination title").text =
                    Some(TextBody::from_text("Colleague title"));
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Colleague other slide".into();
            }
        });
        f.run(
            &basis,
            json!([
                {"command": "text.edit", "params": {"id": target, "at": [0, 0]}},
                {"command": "insert.hyperlink", "params": {"slide": 1}}
            ]),
            related,
        )
        .await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn centered_insert_holds_slide_size_but_allows_other_document_properties() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.slide_size.width = 720.0;
            } else {
                deck.first_slide_number = 7;
            }
        });
        f.run(&basis, json!([{"command": "shape.insert", "params": {"preset": "ellipse"}}]), related).await;
        if !related {
            assert_eq!(f.deck().first_slide_number, 7)
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn connector_holds_endpoint_geometry_but_allows_endpoint_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let from = f.target("Fill target");
        let to = f.target("Group one");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let shape = find_text_mut(&mut Arc::make_mut(&mut deck.slides[0]).shapes, "Fill target");
            if related {
                shape.xfrm.as_mut().unwrap().x += 30.0;
            } else {
                shape.text = Some(TextBody::from_text("Colleague endpoint text"));
            }
        });
        f.run(&basis, json!([{"command": "drawing.connect", "params": {"from": from, "to": to}}]), related).await;
        f.kits.shutdown().await;
    }
}
