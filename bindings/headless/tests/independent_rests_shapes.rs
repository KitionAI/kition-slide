//! C8-slide-read-3: the shape family's premises, as the independent reading of their handlers
//! says, with an actual second file writer: for each class of read, a colleague change of what the
//! write reads refuses it, and one of what it does not read applies (its result checked).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{Fill, Shape, ShapeKind, SlideFile};
use serde_json::{Value, json};

const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";
const FILE: &str = "Rests.pptx";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

fn run(doc: &mut SlideDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

/// Slide 1: the title slide. Slide 2 (Title and Content): placeholders "Results" / "One";
/// `Rect` (text "Box") glued by `Line` to `Oval`; `Group` of `GA` and `GB`; `Chart`; `Text`; `Pic`
/// (RED); `Video`; an animation on `Rect`; a comment on `Rect`. Slide 3 (Blank): `Star`.
impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(FILE, 960, 540).unwrap();
        run(&mut doc, "slide.new", json!({"layout": "titleAndContent", "title": "Results", "body": "One"}));
        run(&mut doc, "slide.go", json!({"index": 1}));
        let named = |doc: &mut SlideDocument, answer: Value, name: &str| {
            let id = answer["id"].clone();
            run(doc, "shape.rename", json!({"id": id, "name": name}));
            id
        };
        let a = run(&mut doc, "shape.insert", json!({"preset": "rect", "rect": [100, 300, 120, 80], "text": "Box"}));
        let rect = named(&mut doc, a, "Rect");
        let a = run(&mut doc, "shape.insert", json!({"preset": "ellipse", "rect": [400, 300, 100, 100]}));
        let oval = named(&mut doc, a, "Oval");
        let a = run(&mut doc, "shape.connect", json!({"from": rect, "to": oval}));
        named(&mut doc, a, "Line");
        let a = run(&mut doc, "shape.insert", json!({"preset": "rect", "rect": [600, 100, 80, 60]}));
        let ga = named(&mut doc, a, "GA");
        let a = run(&mut doc, "shape.insert", json!({"preset": "rect", "rect": [700, 100, 80, 60]}));
        let gb = named(&mut doc, a, "GB");
        let a = run(&mut doc, "arrange.group", json!({"ids": [ga, gb]}));
        named(&mut doc, a, "Group");
        let a = run(
            &mut doc,
            "insert.chart",
            json!({"type": "column", "rect": [500, 200, 200, 150], "categories": ["Q1", "Q2"], "series": [{"name": "Sales", "values": [1, 2]}]}),
        );
        named(&mut doc, a, "Chart");
        let a = run(&mut doc, "insert.textBox", json!({"rect": [100, 450, 200, 40], "text": "Note"}));
        named(&mut doc, a, "Text");
        let a = run(&mut doc, "insert.picture", json!({"name": "red.png", "data": RED, "rect": [800, 300, 50, 50]}));
        named(&mut doc, a, "Pic");
        let video = include_bytes!("../../../crates/media/media/tests/fixtures/clip-h264.mp4");
        let a = run(
            &mut doc,
            "insert.video",
            json!({"name": "clip.mp4", "data": kition_slide_engine::cmd::base64_encode(video), "rect": [800, 400, 120, 80]}),
        );
        named(&mut doc, a, "Video");
        run(&mut doc, "animation.add", json!({"effect": "fade", "ids": [rect]}));
        run(&mut doc, "comment.add", json!({"slide": 1, "id": rect, "text": "Check", "author": "colleague"}));
        run(&mut doc, "slide.new", json!({"layout": "blank", "at": 2}));
        run(&mut doc, "slide.go", json!({"index": 2}));
        let a = run(&mut doc, "shape.insert", json!({"preset": "star5", "rect": [300, 200, 100, 100]}));
        named(&mut doc, a, "Star");
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), doc.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn path(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file(FILE, &std::fs::read(self.path()).unwrap()).unwrap()
    }

    fn rewrite(&self, edit: impl FnOnce(&mut SlideFile)) {
        let mut deck = self.deck();
        edit(&mut deck);
        let old_time = std::fs::metadata(self.path()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.path(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.path()).unwrap().set_modified(old_time).unwrap();
    }

    /// The tool id of the shape named `name` on slide `slide` (from 0).
    fn id(&self, slide: usize, name: &str) -> String {
        let deck = self.deck();
        let mut found = None;
        kition_slide_model::walk(&deck.slides[slide].shapes, &mut |shape, _| {
            if shape.name == name {
                found = Some(kition_slide_engine::cmd::inspect::shape_tool_id(shape));
            }
        });
        found.unwrap_or_else(|| panic!("no shape {name} on slide {slide}"))
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "shape-reader").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing tool: slide_{action}"),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target": {"slide": 2}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    async fn run(&self, basis: &str, steps: Value, refusal: bool) -> Value {
        let saved = std::fs::read(self.path()).unwrap();
        let answer = self.call("run", json!({"basis": basis, "steps": steps})).await;
        if refusal {
            let error = answer.expect_err("a related rewrite applied");
            eprintln!("refused as it must: {}", error["conflicts"]);
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(self.path()).unwrap(), saved, "refusal changed saved bytes");
            error
        } else {
            answer.unwrap_or_else(|error| panic!("unrelated rewrite refused: {error}"))
        }
    }
}

fn named_mut<'a>(shapes: &'a mut [Shape], name: &str) -> &'a mut Shape {
    let mut path = None;
    for (at, shape) in shapes.iter().enumerate() {
        if shape.name == name {
            path = Some((at, None));
        } else if let Some(inner) = shape.children().iter().position(|child| child.name == name) {
            path = Some((at, Some(inner)));
        }
    }
    match path {
        Some((at, None)) => &mut shapes[at],
        Some((at, Some(inner))) => match &mut shapes[at].kind {
            ShapeKind::Group { children, .. } => &mut children[inner],
            _ => unreachable!(),
        },
        None => panic!("no shape {name}"),
    }
}

fn named<'a>(shapes: &'a [Shape], name: &str) -> Option<&'a Shape> {
    shapes.iter().find(|shape| shape.name == name).or_else(|| shapes.iter().flat_map(|shape| shape.children()).find(|shape| shape.name == name))
}

fn slide2(deck: &mut SlideFile) -> &mut kition_slide_model::Slide {
    Arc::make_mut(&mut deck.slides[1])
}

// LESS: another shape the write reads (a connector glued to the target and its other end).
#[tokio::test(flavor = "multi_thread")]
async fn delete_holds_the_glued_connector_and_its_other_end() {
    for related in [true, false] {
        let f = Fixture::new();
        let rect = f.id(1, "Rect");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let oval = named_mut(&mut slide2(deck).shapes, "Oval");
            if related {
                oval.xfrm.as_mut().unwrap().x += 40.0;
            } else {
                oval.fill = Some(Fill::None);
            }
        });
        f.run(&basis, json!([{"command": "shape.delete", "params": {"ids": [rect]}}]), related).await;
        if !related {
            let deck = f.deck();
            assert!(named(&deck.slides[1].shapes, "Rect").is_none());
            assert_eq!(named(&deck.slides[1].shapes, "Oval").unwrap().fill, Some(Fill::None));
        }
        f.kits.shutdown().await;
    }
}

// LESS: another shape the write reads (the glued connector itself, rerouted from its other end).
#[tokio::test(flavor = "multi_thread")]
async fn delete_holds_the_glued_connectors_box() {
    for related in [true, false] {
        let f = Fixture::new();
        let rect = f.id(1, "Rect");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let line = named_mut(&mut slide2(deck).shapes, "Line");
            if related {
                line.xfrm.as_mut().unwrap().y += 30.0;
            } else {
                line.name = "Colleague line".into();
            }
        });
        f.run(&basis, json!([{"command": "edit.delete", "params": {"ids": [rect]}}]), related).await;
        if !related {
            let deck = f.deck();
            assert!(named(&deck.slides[1].shapes, "Rect").is_none());
            assert!(named(&deck.slides[1].shapes, "Colleague line").is_some());
        }
        f.kits.shutdown().await;
    }
}

// LESS: another shape the write reads (the group refit to its members' union).
#[tokio::test(flavor = "multi_thread")]
async fn resize_of_a_group_member_holds_its_sibling() {
    for related in [true, false] {
        let f = Fixture::new();
        let ga = f.id(1, "GA");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                named_mut(&mut slide2(deck).shapes, "GB").xfrm.as_mut().unwrap().y += 40.0;
            } else {
                named_mut(&mut slide2(deck).shapes, "Group").name = "Colleague group".into();
            }
        });
        f.run(&basis, json!([{"command": "shape.resize", "params": {"ids": [ga], "w": 100}}]), related).await;
        if !related {
            let deck = f.deck();
            assert!(named(&deck.slides[1].shapes, "Colleague group").is_some());
            assert!((named(&deck.slides[1].shapes, "GA").unwrap().xfrm.unwrap().w - 100.0).abs() < 0.5);
        }
        f.kits.shutdown().await;
    }
}

// LESS: the slide's own field (its animations, filtered for the deleted shape).
#[tokio::test(flavor = "multi_thread")]
async fn edit_delete_holds_the_slides_animations() {
    for related in [true, false] {
        let f = Fixture::new();
        let rect = f.id(1, "Rect");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let slide = slide2(deck);
            if related {
                let oval = named(&slide.shapes, "Oval").unwrap().id;
                let mut animation = slide.animations[0].clone();
                animation.shape = oval;
                slide.animations.push(animation);
            } else {
                slide.notes = kition_slide_model::TextBody::from_text("Colleague notes");
            }
        });
        f.run(&basis, json!([{"command": "edit.delete", "params": {"id": rect}}]), related).await;
        if !related {
            let deck = f.deck();
            assert!(named(&deck.slides[1].shapes, "Rect").is_none());
            assert_eq!(deck.slides[1].notes.text(), "Colleague notes");
        }
        f.kits.shutdown().await;
    }
}

// An inserted text box holds the theme fonts used to fit it, on its explicit or current slide.
#[tokio::test(flavor = "multi_thread")]
async fn text_box_insert_holds_the_theme_fonts() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let theme = &mut Arc::make_mut(&mut deck.masters[0]).theme;
            if related {
                theme.fonts.minor.latin = "Courier New".into();
            } else {
                theme.colors.colors[4] = theme.colors.colors[5];
            }
        });
        let mut step = json!({"command": "insert.textBox", "params": {"rect": [10, 10, 200, 40], "text": "Hi"}});
        if !related {
            step["at"] = json!({"part": {"slide": 2}});
        }
        f.run(&basis, json!([step]), related).await;
        if !related {
            let deck = f.deck();
            assert!(deck.slides[1].shapes.iter().any(|shape| shape.text.as_ref().is_some_and(|body| body.text() == "Hi")));
        }
        f.kits.shutdown().await;
    }
}

// LESS: an inherited read (the deck's default text style a text box is fitted with).
#[tokio::test(flavor = "multi_thread")]
async fn text_box_insert_holds_the_default_text_style() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let mut level = kition_slide_model::text::LevelStyle::default();
                level.run.size = Some(40.0);
                deck.default_text_style.levels = vec![Some(level)];
            } else {
                deck.first_slide_number = 7;
            }
        });
        f.run(&basis, json!([{"command": "insert.textBox", "params": {"rect": [10, 10, 200, 40], "text": "Hi"}}]), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.first_slide_number, 7);
            assert!(
                deck.slides.iter().flat_map(|slide| slide.shapes.iter()).any(|shape| shape.text.as_ref().is_some_and(|body| body.text() == "Hi"))
            );
        }
        f.kits.shutdown().await;
    }
}

// Media reuse holds the basis-time match, including a media item whose bytes stopped matching.
#[tokio::test(flavor = "multi_thread")]
async fn picture_insert_holds_the_media_it_reuses() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let pic = named(&deck.slides[1].shapes, "Pic").unwrap();
                let ShapeKind::Picture { fill } = &pic.kind else { panic!("not a picture") };
                let media = fill.media;
                let item = deck.media.iter_mut().find(|item| item.id == media).unwrap();
                let other = include_bytes!("../../../crates/media/media/tests/fixtures/clip-h264.mp4");
                item.data = Arc::new(other.to_vec());
            } else {
                named_mut(&mut slide2(deck).shapes, "Pic").descr = "Colleague alt text".into();
            }
        });
        f.run(&basis, json!([{"command": "insert.picture", "params": {"name": "red.png", "data": RED, "rect": [10, 10, 50, 50]}}]), related).await;
        if !related {
            let deck = f.deck();
            let pictures: Vec<_> = deck
                .slides
                .iter()
                .flat_map(|slide| slide.shapes.iter())
                .filter_map(|shape| if let ShapeKind::Picture { fill } = &shape.kind { Some(fill.media) } else { None })
                .collect();
            assert_eq!(pictures.len(), 2);
            assert_eq!(pictures[0], pictures[1], "the new picture reuses the media");
            assert_eq!(named(&deck.slides[1].shapes, "Pic").unwrap().descr, "Colleague alt text");
        }
        f.kits.shutdown().await;
    }
}

// LESS: the named target's field the write replaces whole (a video's clip, `look.kind`).
#[tokio::test(flavor = "multi_thread")]
async fn poster_frame_holds_the_whole_clip() {
    for related in [true, false] {
        let f = Fixture::new();
        let video = f.id(1, "Video");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let shape = named_mut(&mut slide2(deck).shapes, "Video");
            if related {
                let ShapeKind::Media(clip) = &mut shape.kind else { panic!("not media") };
                clip.fade_in_ms = 500;
            } else {
                shape.descr = "Colleague alt text".into();
            }
        });
        f.run(&basis, json!([{"command": "media.posterFrame", "params": {"id": video, "ms": 500}}]), related).await;
        if !related {
            assert_eq!(named(&f.deck().slides[1].shapes, "Video").unwrap().descr, "Colleague alt text");
        }
        f.kits.shutdown().await;
    }
}

// MORE: held positions the call does not depend on (the deleted shape's index).
#[tokio::test(flavor = "multi_thread")]
async fn edit_delete_does_not_hold_its_targets_place() {
    for related in [true, false] {
        let f = Fixture::new();
        let rect = f.id(1, "Rect");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let slide = slide2(deck);
            if related {
                named_mut(&mut slide.shapes, "Rect").fill = Some(Fill::None);
            } else {
                let at = slide.shapes.iter().position(|shape| shape.name == "Rect").unwrap();
                let earlier = slide.shapes.remove(at - 1);
                slide.shapes.push(earlier);
            }
        });
        f.run(&basis, json!([{"command": "edit.delete", "params": {"id": rect}}]), related).await;
        if !related {
            assert!(named(&f.deck().slides[1].shapes, "Rect").is_none());
        }
        f.kits.shutdown().await;
    }
}

// MORE: held fields the call does not read (a chart's other members).
#[tokio::test(flavor = "multi_thread")]
async fn chart_data_does_not_hold_the_chart_title() {
    for related in [true, false] {
        let f = Fixture::new();
        let chart = f.id(1, "Chart");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let shape = named_mut(&mut slide2(deck).shapes, "Chart");
            let ShapeKind::Chart(spec) = &mut shape.kind else { panic!("not a chart") };
            if related {
                spec.categories = vec!["X".into(), "Y".into()];
            } else {
                spec.title = Some("Colleague title".into());
            }
            spec.raw = None;
        });
        f.run(&basis, json!([{"command": "chart.data", "params": {"id": chart, "categories": ["A", "B"]}}]), related).await;
        if !related {
            let deck = f.deck();
            let ShapeKind::Chart(spec) = &named(&deck.slides[1].shapes, "Chart").unwrap().kind else { panic!("not a chart") };
            assert_eq!(spec.title.as_deref(), Some("Colleague title"));
            assert_eq!(spec.categories, ["A", "B"]);
        }
        f.kits.shutdown().await;
    }
}

// MORE: held fields the call does not read (a moved shape's fill).
#[tokio::test(flavor = "multi_thread")]
async fn move_does_not_hold_the_targets_fill() {
    for related in [true, false] {
        let f = Fixture::new();
        let star = f.id(2, "Star");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let star = named_mut(&mut Arc::make_mut(&mut deck.slides[2]).shapes, "Star");
            if related {
                star.xfrm.as_mut().unwrap().x += 40.0;
            } else {
                star.fill = Some(Fill::None);
            }
        });
        f.run(&basis, json!([{"command": "shape.move", "params": {"ids": [star], "dx": 5}}]), related).await;
        if !related {
            let deck = f.deck();
            let star = named(&deck.slides[2].shapes, "Star").unwrap();
            assert_eq!(star.fill, Some(Fill::None));
            assert!((star.xfrm.unwrap().x - 305.0).abs() < 0.5);
        }
        f.kits.shutdown().await;
    }
}

// MORE: held an object the call does not read (the master's title placeholder, for a move of a
// slide title whose layout placeholder has its own box).
#[tokio::test(flavor = "multi_thread")]
async fn placeholder_move_holds_the_layout_box_not_the_master_placeholder() {
    for related in [true, false] {
        let f = Fixture::new();
        let deck = f.deck();
        let title = kition_slide_engine::cmd::inspect::shape_tool_id(&deck.slides[1].shapes[0]);
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let layout = deck.slides[1].layout;
            let master = Arc::make_mut(&mut deck.masters[0]);
            if related {
                let layout = master.layouts.iter_mut().find(|candidate| candidate.id == layout).unwrap();
                let placeholder = layout.shapes.iter_mut().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap();
                placeholder.xfrm.as_mut().unwrap().y += 20.0;
            } else {
                let placeholder = master.shapes.iter_mut().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap();
                placeholder.fill = Some(Fill::None);
            }
        });
        f.run(&basis, json!([{"command": "shape.move", "params": {"ids": [title], "dy": 5}}]), related).await;
        if !related {
            assert!(f.deck().slides[1].shapes[0].xfrm.is_some());
        }
        f.kits.shutdown().await;
    }
}
