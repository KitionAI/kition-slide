//! What the picture, drawing and arrange writes rest on (C8-slide-read-3), with a second file
//! writer: for each kind of read, a colleague change the write must refuse and one it must not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{Shape, ShapeKind, SlideFile, TextBody};
use serde_json::{Value, json};

const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";
const FILE: &str = "Drawing.pptx";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

/// Slide 1: rectangle "A" [100,300,120,80], ellipse "B" [400,300,100,100], a connector glued from
/// A to B, a group of "G1" and "G2", a text box "Note", a picture (red.png); A has an animation.
/// Slide 2 is blank.
impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(FILE, 960, 540).unwrap();
        let id = |answer: Value| answer["id"].clone();
        let a = id(doc.execute("shape.insert", json!({"preset": "rect", "rect": [100, 300, 120, 80], "text": "A"})).unwrap());
        let b = id(doc.execute("shape.insert", json!({"preset": "ellipse", "rect": [400, 300, 100, 100], "text": "B"})).unwrap());
        doc.execute("shape.connect", json!({"from": a, "to": b})).unwrap();
        let g1 = id(doc.execute("shape.insert", json!({"preset": "rect", "rect": [600, 100, 80, 60], "text": "G1"})).unwrap());
        let g2 = id(doc.execute("shape.insert", json!({"preset": "rect", "rect": [700, 100, 80, 60], "text": "G2"})).unwrap());
        doc.execute("arrange.group", json!({"ids": [g1, g2]})).unwrap();
        doc.execute("textBox.insert", json!({"rect": [100, 450, 200, 40], "text": "Note"})).unwrap();
        doc.execute("image.insert", json!({"name": "red.png", "data": RED, "rect": [800, 300, 50, 50]})).unwrap();
        doc.execute("animation.add", json!({"effect": "fade", "ids": [a]})).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
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

    /// The tool id of the shape on slide 1 that `is` picks.
    fn id(&self, is: impl Fn(&Shape) -> bool) -> String {
        let deck = self.deck();
        let mut found = None;
        kition_slide_model::walk(&deck.slides[0].shapes, &mut |shape, _| {
            if found.is_none() && is(shape) {
                found = Some(kition_slide_engine::cmd::inspect::shape_tool_id(shape));
            }
        });
        found.unwrap()
    }

    fn text(&self, text: &str) -> String {
        self.id(|shape| has_text(shape, text))
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "drawing-reader").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing tool: slide_{action}"),
        }
    }

    async fn basis(&self) -> String {
        self.call("inspect", json!({"target": {"slide": 1}})).await.unwrap()["revision"].as_str().unwrap().to_owned()
    }

    async fn run(&self, basis: &str, command: &str, params: Value, refusal: bool) -> Value {
        let saved = std::fs::read(self.path()).unwrap();
        let answer = self.call("run", json!({"basis": basis, "steps": [{"command": command, "params": params}]})).await;
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

fn has_text(shape: &Shape, text: &str) -> bool {
    shape.text.as_ref().is_some_and(|body| body.text() == text)
}

fn shapes(deck: &mut SlideFile) -> &mut Vec<Shape> {
    &mut Arc::make_mut(&mut deck.slides[0]).shapes
}

fn find<'a>(list: &'a mut [Shape], is: &dyn Fn(&Shape) -> bool) -> &'a mut Shape {
    for shape in list {
        if is(shape) {
            return shape;
        }
        if let ShapeKind::Group { children, .. } = &mut shape.kind {
            let mut inside = false;
            kition_slide_model::walk(children, &mut |child, _| inside |= is(child));
            if inside {
                return find(children, is);
            }
        }
    }
    panic!("no such shape");
}

fn named<'a>(deck: &'a mut SlideFile, text: &str) -> &'a mut Shape {
    find(shapes(deck), &|shape| has_text(shape, text))
}

fn top_level(deck: &SlideFile, text: &str) -> Option<usize> {
    deck.slides[0].shapes.iter().position(|shape| has_text(shape, text))
}

// ---- LESS: another shape the write reads ----

#[tokio::test(flavor = "multi_thread")]
async fn delete_holds_the_glued_connectors_other_end_but_not_its_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let a = f.text("A");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let b = named(deck, "B");
            if related {
                b.xfrm.as_mut().unwrap().y += 60.0;
            } else {
                b.name = "Colleague ellipse".into();
            }
        });
        f.run(&basis, "drawing.delete", json!({"ids": [a]}), related).await;
        if !related {
            let mut deck = f.deck();
            assert!(top_level(&deck, "A").is_none());
            assert_eq!(named(&mut deck, "B").name, "Colleague ellipse");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn nudging_a_group_member_holds_its_sibling_but_not_the_siblings_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let g2 = f.text("G2");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let g1 = named(deck, "G1");
            if related {
                g1.xfrm.as_mut().unwrap().x -= 40.0;
            } else {
                g1.text = Some(TextBody::from_text("G1 by colleague"));
            }
        });
        f.run(&basis, "arrange.nudge", json!({"ids": [g2], "dx": 6, "dy": 0}), related).await;
        if !related {
            assert!(f.deck().slides[0].shapes.iter().any(|shape| {
                let mut seen = false;
                kition_slide_model::walk(std::slice::from_ref(shape), &mut |child, _| seen |= has_text(child, "G1 by colleague"));
                seen
            }));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bring_forward_holds_the_shape_above_but_not_its_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let a = f.text("A");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                // B (above A) goes above the connector; A stays where it was.
                let list = shapes(deck);
                let b = list.iter().position(|shape| has_text(shape, "B")).unwrap();
                list.swap(b, b + 1);
            } else {
                named(deck, "B").text = Some(TextBody::from_text("B by colleague"));
            }
        });
        f.run(&basis, "arrange.bringForward", json!({"ids": [a]}), related).await;
        if !related {
            let deck = f.deck();
            assert!(top_level(&deck, "A").unwrap() > top_level(&deck, "B by colleague").unwrap());
        }
        f.kits.shutdown().await;
    }
}

// ---- LESS: the slide's own fields ----

#[tokio::test(flavor = "multi_thread")]
async fn delete_holds_the_slides_animations_but_not_its_transition() {
    for related in [true, false] {
        let f = Fixture::new();
        let note = f.text("Note");
        let number = f.deck().slides[0].shapes.iter().find(|shape| has_text(shape, "Note")).unwrap().id;
        let basis = f.basis().await;
        f.rewrite(|deck| {
            let slide = Arc::make_mut(&mut deck.slides[0]);
            if related {
                let mut extra = slide.animations[0].clone();
                extra.shape = number;
                slide.animations.push(extra);
            } else {
                slide.transition = Some(kition_slide_model::anim::Transition { kind: "fade".into(), duration_ms: 900, ..Default::default() });
            }
        });
        f.run(&basis, "drawing.delete", json!({"ids": [note]}), related).await;
        if !related {
            let deck = f.deck();
            assert!(top_level(&deck, "Note").is_none());
            assert_eq!(deck.slides[0].transition.as_ref().map(|t| t.duration_ms), Some(900));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn group_holds_the_slides_shape_count_but_not_another_shapes_text() {
    for related in [true, false] {
        let f = Fixture::new();
        let (a, b) = (f.text("A"), f.text("B"));
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                let mut extra = named(deck, "Note").clone();
                extra.id = kition_slide_model::ShapeId(deck.alloc_id());
                extra.identity = String::new();
                extra.text = Some(TextBody::from_text("Colleague box"));
                shapes(deck).push(extra);
            } else {
                named(deck, "Note").text = Some(TextBody::from_text("Note by colleague"));
            }
        });
        f.run(&basis, "drawing.group", json!({"ids": [a, b]}), related).await;
        if !related {
            let deck = f.deck();
            assert!(top_level(&deck, "A").is_none());
            assert!(top_level(&deck, "Note by colleague").is_some());
        }
        f.kits.shutdown().await;
    }
}

// ---- LESS: the slide size ----

#[tokio::test(flavor = "multi_thread")]
async fn align_to_the_slide_holds_its_size_but_not_a_shapes_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let a = f.text("A");
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                deck.slide_size.width = 720.0;
            } else {
                named(deck, "A").name = "Colleague rectangle".into();
            }
        });
        f.run(&basis, "arrange.align", json!({"ids": [a], "edge": "right"}), related).await;
        if !related {
            let mut deck = f.deck();
            let shape = named(&mut deck, "A");
            assert_eq!(shape.name, "Colleague rectangle");
            assert!((shape.xfrm.unwrap().x - 840.0).abs() < 0.01, "{:?}", shape.xfrm);
        }
        f.kits.shutdown().await;
    }
}

// ---- LESS: media bytes ----

/// A media item's bytes (`blob`) the picture would reuse are held, as `independent_other.rs`'s
/// `poster_holds_video_bytes_*`; the picture's name is not.
#[tokio::test(flavor = "multi_thread")]
async fn picture_change_holds_the_media_it_would_reuse_but_not_the_pictures_name() {
    // The new bytes are the picture's own plus a tail: no media holds them at `basis`.
    let tail = |data: &[u8]| [data, b"tail"].concat();
    let new = kition_slide_engine::cmd::base64_encode(&tail(&kition_slide_engine::cmd::base64_decode(RED).unwrap()));
    for related in [true, false] {
        let f = Fixture::new();
        let picture = f.id(|shape| matches!(shape.kind, ShapeKind::Picture { .. }));
        let basis = f.basis().await;
        f.rewrite(|deck| {
            if related {
                // The colleague's picture item now holds the very bytes the write stores.
                let item = deck.media.iter_mut().find(|item| item.content_type == "image/png").unwrap();
                item.data = Arc::new(tail(&item.data));
            } else {
                find(shapes(deck), &|shape| matches!(shape.kind, ShapeKind::Picture { .. })).name = "Colleague picture".into();
            }
        });
        f.run(&basis, "image.change", json!({"ids": [picture], "data": new, "name": "new.png"}), related).await;
        if !related {
            let mut deck = f.deck();
            let shape = find(shapes(&mut deck), &|shape| matches!(shape.kind, ShapeKind::Picture { .. }));
            assert_eq!(shape.name, "Colleague picture");
            let ShapeKind::Picture { fill } = &shape.kind else { unreachable!() };
            let media = fill.media;
            assert!(deck.media.iter().any(|item| item.id == media && item.data.ends_with(b"tail")));
        }
        f.kits.shutdown().await;
    }
}

// ---- MORE: held positions the call does not depend on ----

#[tokio::test(flavor = "multi_thread")]
async fn transform_allows_the_target_to_move_in_the_stack() {
    let f = Fixture::new();
    let note = f.text("Note");
    let basis = f.basis().await;
    f.rewrite(|deck| {
        let list = shapes(deck);
        let at = list.iter().position(|shape| has_text(shape, "Note")).unwrap();
        let shape = list.remove(at);
        list.insert(0, shape);
    });
    f.run(&basis, "drawing.transform", json!({"ids": [note], "dy": 10}), false).await;
    let mut deck = f.deck();
    assert_eq!(top_level(&deck, "Note"), Some(0));
    assert!((named(&mut deck, "Note").xfrm.unwrap().y - 460.0).abs() < 0.01);
    f.kits.shutdown().await;
}

// ---- MORE: held fields the call does not read ----

#[tokio::test(flavor = "multi_thread")]
async fn format_fill_allows_the_targets_text_to_change() {
    let f = Fixture::new();
    let a = f.text("A");
    let basis = f.basis().await;
    f.rewrite(|deck| named(deck, "A").text = Some(TextBody::from_text("A by colleague")));
    f.run(&basis, "drawing.format", json!({"ids": [a], "fill": "#FF0000"}), false).await;
    let mut deck = f.deck();
    assert!(named(&mut deck, "A by colleague").fill.is_some());
    f.kits.shutdown().await;
}

// ---- MORE: held an object the call does not read ----

#[tokio::test(flavor = "multi_thread")]
async fn reconnecting_an_end_allows_the_shape_it_leaves_to_move() {
    let f = Fixture::new();
    let line = f.id(|shape| matches!(shape.kind, ShapeKind::Connector { .. }));
    let note = f.deck().slides[0].shapes.iter().find(|shape| has_text(shape, "Note")).unwrap().id.0;
    let basis = f.basis().await;
    f.rewrite(|deck| named(deck, "B").xfrm.as_mut().unwrap().y += 60.0);
    f.run(&basis, "drawing.reconnect", json!({"id": line, "end": "end", "to": note, "site": 0}), false).await;
    let mut deck = f.deck();
    let note_number = named(&mut deck, "Note").id;
    assert!((named(&mut deck, "B").xfrm.unwrap().y - 360.0).abs() < 0.01);
    let connector = deck.slides[0].shapes.iter().find(|shape| matches!(shape.kind, ShapeKind::Connector { .. })).unwrap();
    assert!(matches!(connector.kind, ShapeKind::Connector { end: Some((end, _)), .. } if end == note_number), "{:?}", connector.kind);
    f.kits.shutdown().await;
}
