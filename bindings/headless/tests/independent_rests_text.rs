//! C8-slide-read-3: text, format and table writes hold what their handlers read and no more.
//! An independent writer changes the file between the basis and the write: a change the handler
//! reads refuses the stale write, an unrelated one lets it apply.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::text::{AutoFit, LevelStyle, RunProps, TextBody};
use kition_slide_model::{Shape, SlideFile};
use serde_json::{Value, json};

const FILE: &str = "Rests.pptx";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

/// Slide 1: "Filler" (a plain rectangle), "Plain" (a rectangle with text, no autofit), "Group"
/// of "Member A" and "Member B" (rectangles with text), "Grid" (a 2x2 table). Slide 2: blank.
impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(FILE, 960, 540).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        doc.execute("slide.go", json!({"index": 0})).unwrap();
        let number = |value: Value| value["id"].as_u64().unwrap() as u32;
        let filler = number(doc.execute("shape.insert", json!({"preset": "rect", "rect": [20, 20, 40, 40]})).unwrap());
        let plain = number(doc.execute("shape.insert", json!({"preset": "rect", "rect": [100, 20, 200, 80]})).unwrap());
        let a = number(doc.execute("shape.insert", json!({"preset": "rect", "rect": [100, 200, 120, 60]})).unwrap());
        let b = number(doc.execute("shape.insert", json!({"preset": "rect", "rect": [260, 200, 120, 60]})).unwrap());
        doc.execute("arrange.group", json!({"ids": [a, b]})).unwrap();
        let grid = number(doc.execute("insert.table", json!({"rows": 2, "cols": 2, "rect": [500, 300, 200, 80]})).unwrap());
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join(FILE);
        std::fs::write(&path, doc.save(Some(FILE)).unwrap()).unwrap();
        let mut deck = open(&path);
        let slide = Arc::make_mut(&mut deck.slides[0]);
        for (id, name, text) in
            [(filler, "Filler", None), (plain, "Plain", Some("Plain text")), (a, "Member A", Some("Alpha")), (b, "Member B", Some("Beta"))]
        {
            let shape = kition_slide_model::find_shape_mut(&mut slide.shapes, kition_slide_model::ShapeId(id)).unwrap();
            shape.name = name.into();
            if let Some(text) = text {
                let mut body = TextBody::from_text(text);
                body.body.autofit = Some(AutoFit::None);
                shape.text = Some(body);
            }
        }
        kition_slide_model::find_shape_mut(&mut slide.shapes, kition_slide_model::ShapeId(grid)).unwrap().name = "Grid".into();
        slide.shapes.iter_mut().find(|shape| shape.is_group()).unwrap().name = "Group".into();
        std::fs::write(&path, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }

    fn deck(&self) -> SlideFile {
        open(&self.file())
    }

    /// The tool id of the shape named `name` on slide 1.
    fn id(&self, name: &str) -> String {
        let mut deck = self.deck();
        kition_slide_engine::cmd::inspect::shape_tool_id(named(&mut deck, name))
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
        params["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "rests-text").await.unwrap() {
            Ok(output) => Ok(output.output),
            Err(error) => Err(error.structured()),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target": {"slide": 1}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    /// Runs `command` with `basis`: refused untouched when `related`, else applied.
    async fn write(&self, basis: &str, command: &str, params: Value, related: bool) {
        let bytes = std::fs::read(self.file()).unwrap();
        let result = self.call("run", json!({"basis": basis, "steps": [{"command": command, "params": params}]})).await;
        if related {
            let error = result.expect_err("an input read by the handler changed on disk; the stale write must refuse");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.file()).unwrap(), bytes, "refusal wrote the file");
        } else {
            result.unwrap_or_else(|error| panic!("unrelated change refused: {error}"));
        }
    }
}

fn open(path: &std::path::Path) -> SlideFile {
    kition_slide_engine::cmd::file::open_slide_file(FILE, &std::fs::read(path).unwrap()).unwrap()
}

fn named<'a>(deck: &'a mut SlideFile, name: &str) -> &'a mut Shape {
    let slide = Arc::make_mut(&mut deck.slides[0]);
    let mut id = None;
    kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
        if shape.name == name {
            id = Some(shape.id)
        }
    });
    kition_slide_model::find_shape_mut(&mut slide.shapes, id.unwrap()).unwrap()
}

fn bold(shape: &Shape) -> bool {
    shape.text.as_ref().unwrap().paragraphs.iter().flat_map(|p| &p.runs).all(|run| run.props.bold == Some(true))
}

fn rename_slide_2(deck: &mut SlideFile) {
    Arc::make_mut(&mut deck.slides[1]).name = "Unrelated".into();
}

/// Another shape the write reads: formatting a group formats its members' text.
#[tokio::test(flavor = "multi_thread")]
async fn group_formatting_holds_member_text_but_allows_another_slides_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                named(deck, "Member B").text = Some(TextBody::from_text("Colleague"));
            } else {
                rename_slide_2(deck);
            }
        });
        f.write(&basis, "format.bold", json!({"ids": [f.id("Group")], "on": true}), related).await;
        if !related {
            let mut deck = f.deck();
            assert!(bold(named(&mut deck, "Member A")) && bold(named(&mut deck, "Member B")));
        }
        f.kits.shutdown().await;
    }
}

/// Held positions the call does not depend on: formatting does not read where the shape is.
#[tokio::test(flavor = "multi_thread")]
async fn formatting_allows_a_z_order_change_but_holds_the_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                named(deck, "Plain").text = Some(TextBody::from_text("Colleague"));
            } else {
                let shapes = &mut Arc::make_mut(&mut deck.slides[0]).shapes;
                let at = shapes.iter().position(|shape| shape.name == "Filler").unwrap();
                let filler = shapes.remove(at);
                shapes.push(filler);
            }
        });
        f.write(&basis, "format.italic", json!({"ids": [f.id("Plain")], "on": true}), related).await;
        if !related {
            let mut deck = f.deck();
            let shape = named(&mut deck, "Plain");
            assert!(shape.text.as_ref().unwrap().paragraphs[0].runs.iter().all(|run| run.props.italic == Some(true)));
            assert_eq!(deck.slides[0].shapes.last().unwrap().name, "Filler");
        }
        f.kits.shutdown().await;
    }
}

/// Held fields the call does not read: a box that does not fit its text keeps its frame.
#[tokio::test(flavor = "multi_thread")]
async fn formatting_a_box_that_does_not_fit_allows_its_frame_to_move() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.rewrite(|deck| named(deck, "Plain").xfrm.as_mut().unwrap().x = 333.0);
    f.write(&basis, "format.bold", json!({"ids": [f.id("Plain")], "on": true}), false).await;
    let mut deck = f.deck();
    let shape = named(&mut deck, "Plain");
    assert!(bold(shape));
    assert_eq!(shape.xfrm.unwrap().x, 333.0);
    f.kits.shutdown().await;
}

/// Held an object the call does not read: an explicit `on` does not resolve the default style.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_bold_allows_the_default_text_style_to_change_but_a_toggle_holds_it() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            deck.default_text_style.set(0, LevelStyle { run: RunProps { bold: Some(true), ..Default::default() }, ..Default::default() })
        });
        let params = if related { json!({"ids": [f.id("Plain")]}) } else { json!({"ids": [f.id("Plain")], "on": true}) };
        f.write(&basis, "format.bold", params, related).await;
        if !related {
            assert!(bold(named(&mut f.deck(), "Plain")));
        }
        f.kits.shutdown().await;
    }
}

/// Held fields the call does not read: a table style write reads the table's style, not its frame
/// or its cells.
#[tokio::test(flavor = "multi_thread")]
async fn table_style_allows_frame_and_cell_changes_but_holds_the_style() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let grid = named(deck, "Grid");
            if related {
                if let kition_slide_model::ShapeKind::Table(table) = &mut grid.kind {
                    table.band_row = !table.band_row;
                }
            } else {
                grid.xfrm.as_mut().unwrap().y = 222.0;
                if let kition_slide_model::ShapeKind::Table(table) = &mut grid.kind {
                    table.rows[1].cells[1].text = TextBody::from_text("Colleague cell");
                }
            }
        });
        f.write(&basis, "table.style", json!({"id": f.id("Grid"), "style": "grid"}), related).await;
        if !related {
            let mut deck = f.deck();
            let grid = named(&mut deck, "Grid");
            assert_eq!(grid.xfrm.unwrap().y, 222.0);
            let kition_slide_model::ShapeKind::Table(table) = &grid.kind else { panic!("not a table") };
            assert_eq!(table.style, "grid");
            assert_eq!(table.rows[1].cells[1].text.text(), "Colleague cell");
        }
        f.kits.shutdown().await;
    }
}

/// A layout placeholder read: without text being edited, insert.slideNumber turns on the slide
/// number on every slide, copying each layout's Slide Number placeholder.
#[tokio::test(flavor = "multi_thread")]
async fn slide_number_footer_holds_the_layout_placeholder_but_allows_a_slide_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                // The placeholder each slide's copy takes its name from.
                let layout = deck.slides[0].layout;
                let master = deck.masters.iter_mut().find(|master| master.layout(layout).is_some()).unwrap();
                let layout = Arc::make_mut(master).layouts.iter_mut().find(|item| item.id == layout).unwrap();
                let placeholder = layout.shapes.iter_mut().find(|shape| shape.ph_type() == Some(kition_slide_model::PhType::SlideNum)).unwrap();
                placeholder.name = "Colleague number".into();
            } else {
                rename_slide_2(deck);
            }
        });
        f.write(&basis, "insert.slideNumber", json!({}), related).await;
        if !related {
            let deck = f.deck();
            assert!(deck.header_footer.slide_number);
            assert_eq!(deck.slides[1].name, "Unrelated");
        }
        f.kits.shutdown().await;
    }
}

/// The slide's own field: the footer path adds a placeholder to each slide from the kinds it has.
#[tokio::test(flavor = "multi_thread")]
async fn slide_number_footer_holds_each_slides_shapes() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let filler = deck.slides[0].shapes.iter().find(|shape| shape.name == "Filler").cloned().unwrap();
        let slide = Arc::make_mut(&mut deck.slides[1]);
        slide.shapes.push(Shape { id: kition_slide_model::ShapeId(900), identity: Default::default(), name: "Colleague".into(), ..filler });
    });
    f.write(&basis, "insert.slideNumber", json!({}), true).await;
    f.kits.shutdown().await;
}

/// A link to the slide at an index holds that slide (where it is), not its content: renaming the
/// linked slide applies. (A colleague's slide inserted before it cannot be told apart by a file
/// rewrite: the outside journal names slides by their place, so the slide now at that index keeps
/// the identity the link holds.)
#[tokio::test(flavor = "multi_thread")]
async fn slide_link_allows_the_linked_slides_name_to_change() {
    let f = Fixture::new();
    let plain = f.id("Plain");
    let basis = f.basis().await;
    f.rewrite(rename_slide_2);
    f.write(&basis, "insert.hyperlink", json!({"ids": [plain], "slide": 1}), false).await;
    let mut deck = f.deck();
    let target = deck.slides[1].id;
    assert_eq!(deck.slides[1].name, "Unrelated");
    let link = named(&mut deck, "Plain").click.clone().unwrap();
    assert_eq!(link.action, kition_slide_model::text::Action::Slide { slide: target });
    f.kits.shutdown().await;
}

/// Held fields the call does not read: linking a shape replaces its click action only.
#[tokio::test(flavor = "multi_thread")]
async fn shape_link_allows_a_fill_change_but_holds_the_click_action() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let plain = named(deck, "Plain");
            if related {
                plain.click = Some(kition_slide_model::text::Hyperlink {
                    action: kition_slide_model::text::Action::Url { url: "https://colleague.example".into() },
                    tooltip: String::new(),
                    highlight_click: false,
                });
            } else {
                plain.fill = Some(kition_slide_model::Fill::None);
            }
        });
        f.write(&basis, "insert.hyperlink", json!({"ids": [f.id("Plain")], "slide": 1}), related).await;
        if !related {
            let mut deck = f.deck();
            let plain = named(&mut deck, "Plain");
            assert_eq!(plain.fill, Some(kition_slide_model::Fill::None));
            assert!(matches!(plain.click.as_ref().unwrap().action, kition_slide_model::text::Action::Slide { .. }));
        }
        f.kits.shutdown().await;
    }
}
