//! C8-slide-read-4: necessity checks with a separate writer between observe and write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{Shape, ShapeKind, SlideFile};
use serde_json::{Value, json};

const FILE: &str = "Necessity.pptx";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(FILE, 960, 540).unwrap();
        let a = doc.execute("shape.insert", json!({"preset":"rect", "rect":[60,300,120,80], "text":"A"})).unwrap()["id"].clone();
        let b = doc.execute("shape.insert", json!({"preset":"rect", "rect":[400,300,120,80], "text":"B"})).unwrap()["id"].clone();
        doc.execute("shape.connect", json!({"from":a,"to":b})).unwrap();
        doc.execute("table.insert", json!({"rows":2,"cols":2,"rect":[60,420,240,80]})).unwrap();
        doc.execute("slide.new", json!({"layout":"blank"})).unwrap();
        doc.execute("slide.go", json!({"index":0})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), doc.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file(FILE, &std::fs::read(self.file()).unwrap()).unwrap()
    }

    fn rewrite(&self, change: impl FnOnce(&mut SlideFile)) {
        let mut deck = self.deck();
        change(&mut deck);
        let modified = std::fs::metadata(self.file()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.file(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.file()).unwrap().set_modified(modified).unwrap();
    }

    fn id(&self, pick: impl Fn(&Shape) -> bool) -> String {
        kition_slide_engine::cmd::inspect::shape_tool_id(self.deck().slides[0].shapes.iter().find(|shape| pick(shape)).unwrap())
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "necessity-reader").await.unwrap() {
            Ok(output) => Ok(output.output),
            Err(error) => Err(error.structured()),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target":{"slide":1}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    async fn run(&self, basis: &str, steps: Value, refuse: bool) {
        let bytes = std::fs::read(self.file()).unwrap();
        let result = self.call("run", json!({"basis":basis,"steps":steps})).await;
        if refuse {
            let error = result.expect_err("a necessary change applied");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.file()).unwrap(), bytes, "refusal wrote the file");
        } else {
            result.unwrap_or_else(|error| panic!("unrelated change refused: {error}"));
        }
    }
}

fn text(shape: &Shape, value: &str) -> bool {
    shape.text.as_ref().is_some_and(|body| body.text() == value)
}

#[tokio::test(flavor = "multi_thread")]
async fn anchor_content_is_unrelated_but_anchor_existence_and_slide_place_are_necessary() {
    for change in ["content", "stack", "deleted", "slide moved"] {
        let f = Fixture::new();
        let id = f.id(|shape| text(shape, "A"));
        let number = f.deck().slides[0].shapes.iter().find(|shape| text(shape, "A")).unwrap().id;
        let basis = f.basis().await;
        f.rewrite(|deck| match change {
            "content" => Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| shape.id == number).unwrap().name = "Colleague".into(),
            "stack" => Arc::make_mut(&mut deck.slides[0]).shapes.reverse(),
            "deleted" => Arc::make_mut(&mut deck.slides[0]).shapes.retain(|shape| shape.id != number),
            _ => deck.slides.swap(0, 1),
        });
        let necessary = matches!(change, "deleted" | "slide moved");
        f.run(&basis, json!([{"command":"comment.add","params":{"id":id,"slide":0,"text":"Mine"}}]), necessary).await;
        if !necessary {
            let deck = f.deck();
            assert!(deck.slides[0].comments.iter().any(|comment| comment.text == "Mine" && comment.shape == Some(number)));
            if change == "content" {
                assert_eq!(deck.slides[0].shape(number).unwrap().name, "Colleague");
            }
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn show_members_hold_only_the_settings_supplied_by_the_call() {
    for setting in ["type", "range", "loop", "noNarration", "noAnimation", "useTimings", "all"] {
        for related in [false, true] {
            let f = Fixture::new();
            let basis = f.basis().await;
            f.rewrite(|deck| {
                if related {
                    match setting {
                        "type" => deck.show.show_type = "browsed".into(),
                        "range" | "all" => deck.show.range = Some((2, 2)),
                        "loop" => deck.show.loop_until_esc = !deck.show.loop_until_esc,
                        "noNarration" => deck.show.without_narration = !deck.show.without_narration,
                        "noAnimation" => deck.show.without_animation = !deck.show.without_animation,
                        _ => deck.show.use_timings = !deck.show.use_timings,
                    }
                } else if matches!(setting, "range" | "all") {
                    deck.show.loop_until_esc = !deck.show.loop_until_esc;
                } else {
                    deck.show.range = Some((1, 1));
                }
            });
            let params = match setting {
                "type" => json!({"type":"kiosk"}),
                "range" => json!({"from":1,"to":1}),
                _ => json!({setting:true}),
            };
            f.run(&basis, json!([{"command":"show.setup","params":params}]), related).await;
            if !related {
                let show = f.deck().show.clone();
                if matches!(setting, "range" | "all") {
                    assert!(show.loop_until_esc);
                    assert_eq!(show.range, if setting == "all" { None } else { Some((1, 1)) });
                } else {
                    assert_eq!(show.range, Some((1, 1)));
                    if setting == "type" {
                        assert_eq!(show.show_type, "kiosk");
                        assert!(show.loop_until_esc);
                    }
                }
            }
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_drawing_targets_hold_target_text_but_allow_another_shapes_text() {
    for related in [false, true] {
        let f = Fixture::new();
        let a = f.id(|shape| text(shape, "A"));
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let shape = Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| text(shape, if related { "A" } else { "B" })).unwrap();
            shape.text = Some(kition_slide_model::TextBody::from_text("Colleague"));
        });
        f.run(&basis, json!([{ "command":"drawing.text","params":{"id":a,"text":"Mine"}}]), related).await;
        if !related {
            let deck = f.deck();
            assert!(deck.slides[0].shapes.iter().any(|shape| text(shape, "Mine")));
            assert!(deck.slides[0].shapes.iter().any(|shape| text(shape, "Colleague")));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn geometry_and_table_insertions_ignore_stack_moves_but_hold_result_inputs() {
    for command in [
        "drawing.transform",
        "drawing.text",
        "drawing.reconnect",
        "shape.connect",
        "table.insertRowAbove",
        "table.insertRowBelow",
        "table.insertColumnLeft",
        "table.insertColumnRight",
    ] {
        for related in [false, true] {
            let f = Fixture::new();
            let a = f.id(|shape| text(shape, "A"));
            let connector = f.id(|shape| matches!(shape.kind, ShapeKind::Connector { .. }));
            let table = f.id(|shape| matches!(shape.kind, ShapeKind::Table(_)));
            let a_number = f.deck().slides[0].shapes.iter().find(|shape| text(shape, "A")).unwrap().id.0;
            let b_number = f.deck().slides[0].shapes.iter().find(|shape| text(shape, "B")).unwrap().id.0;
            let basis = f.basis().await;
            f.rewrite(|deck| {
                let shapes = &mut Arc::make_mut(&mut deck.slides[0]).shapes;
                if related {
                    if command.starts_with("table.") {
                        let shape = shapes.iter_mut().find(|shape| matches!(shape.kind, ShapeKind::Table(_))).unwrap();
                        let ShapeKind::Table(grid) = &mut shape.kind else { panic!("table") };
                        grid.rows[0].height += 10.0;
                    } else {
                        let shape = shapes.iter_mut().find(|shape| text(shape, "A")).unwrap();
                        if command == "drawing.text" {
                            shape.text = Some(kition_slide_model::TextBody::from_text("Colleague"));
                        } else {
                            shape.xfrm.as_mut().unwrap().w += 20.0;
                        }
                    }
                } else {
                    shapes.reverse();
                }
            });
            let params = match command {
                "drawing.transform" => json!({"ids":[a],"dx":10}),
                "drawing.text" => json!({"id":a,"text":"Mine"}),
                "drawing.reconnect" => json!({"id":connector,"end":"start","to":a_number,"site":0}),
                "shape.connect" => json!({"id":connector,"from":a_number,"to":b_number}),
                _ if command.contains("Row") => json!({"id":table,"row":0}),
                _ => json!({"id":table,"col":0}),
            };
            f.run(&basis, json!([{"command":command,"params":params}]), related).await;
            if !related && command.starts_with("table.") {
                let deck = f.deck();
                let shape = deck.slides[0].shapes.iter().find(|shape| matches!(shape.kind, ShapeKind::Table(_))).unwrap();
                let ShapeKind::Table(grid) = &shape.kind else { panic!("table") };
                assert_eq!((grid.n_rows(), grid.n_cols()), if command.contains("Row") { (3, 2) } else { (2, 3) });
            }
            f.kits.shutdown().await;
        }
    }
}

/// SDK 34dff06 checks the whole plan before text.edit establishes the edited-text path.
/// Keep both assertions: ancestor paragraph text is unrelated; the edited own text is necessary.
#[tokio::test(flavor = "multi_thread")]
async fn paste_uses_own_paragraphs_and_allows_ancestor_paragraph_text() {
    for related in [false, true] {
        let f = Fixture::new();
        let title = f.deck().slides[0].shapes.iter().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap().clone();
        let target = kition_slide_engine::cmd::inspect::shape_tool_id(&title);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| shape.id == title.id).unwrap().text =
                    Some(kition_slide_model::TextBody::from_text("Colleague"));
            } else {
                let layout_id = deck.slides[0].layout;
                let master = Arc::make_mut(&mut deck.masters[0]);
                let layout = master.layouts.iter_mut().find(|layout| layout.id == layout_id).unwrap();
                let ancestor = layout.shapes.iter_mut().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap();
                let body = ancestor.text.as_mut().unwrap();
                body.paragraphs = kition_slide_model::TextBody::from_text("Ancestor text").paragraphs;
            }
        });
        f.run(
            &basis,
            json!([
                {"command":"text.edit","params":{"id":target}},
                {"command":"edit.paste","params":{"text":"Hi"}}
            ]),
            related,
        )
        .await;
        if !related {
            assert!(f.deck().slides[0].shape(title.id).unwrap().text.as_ref().unwrap().text().contains("Hi"));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn generated_ids_do_not_need_an_old_selection_but_generated_names_hold_shape_count() {
    for command in ["shape.freeform", "smartGraphic.insert"] {
        for related in [false, true] {
            let f = Fixture::new();
            let basis = f.basis().await;
            f.rewrite(|deck| {
                if related {
                    let id = kition_slide_model::ShapeId(deck.alloc_id());
                    Arc::make_mut(&mut deck.slides[0]).shapes.push(Shape {
                        id,
                        name: "Colleague".into(),
                        xfrm: Some(kition_slide_model::Xfrm::new(700.0, 100.0, 60.0, 60.0)),
                        ..Default::default()
                    });
                } else {
                    Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| text(shape, "B")).unwrap().name = "Colleague".into();
                }
            });
            let count = f.deck().slides[0].shapes.len();
            let params = if command == "shape.freeform" {
                json!({"points":[[10,10],[40,10],[40,40]],"closed":true})
            } else {
                json!({"kind":"list","items":["One","Two"],"rect":[600,200,200,120]})
            };
            f.run(&basis, json!([{"command":command,"at":{"part":{"slide":1}},"params":params}]), related).await;
            if !related {
                let deck = f.deck();
                assert_eq!(deck.slides[0].shapes.len(), count + 1);
                assert!(deck.slides[0].shapes.iter().any(|shape| shape.name == "Colleague"));
            }
            f.kits.shutdown().await;
        }
    }
}
