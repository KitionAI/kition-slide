//! One id space (KitObjectModel OM9, task card C6/C8-slide): the object model names slides and
//! shapes by the ids the kit's tools use, so receipts, the premise guard, `slide_query` and
//! `slide_batch` speak the ids `slide_inspect` lists.
//!
//! - A slide's tool id is `slide:<slideId>`, its id in the file (`p:sldId`).
//! - A shape's tool id is `shape:<identity>` (C8-slide-8): its persistent creation GUID, unique in
//!   the deck although numbers (`cNvPr`) repeat across slides; it survives saving, reopening,
//!   slides inserted, deleted or reordered, and moves to another slide.
//! - Commands, a step's `on` and the object model take those ids; a shape's tool id names its
//!   slide, so it needs no `onSlide`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::session::model::Exchange;
use kition_kit::{CallContext, KitDocument};
use kition_slide_engine::Origin;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

/// Three slides; slides 1 and 2 hold rectangles with the same numbers (as decks from other
/// programs do), slide 3 a title and body. Saved as `.pptx` and opened again, as a file is.
fn deck() -> SlideDocument {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let mut rects = Vec::new();
    for slide in 0..2 {
        if slide > 0 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        }
        for (x, text) in [(100, "A"), (300, "B")] {
            rects.push(
                doc.execute("shape.insert", json!({ "preset": "rect", "rect": [x, 100, 120, 80], "text": text })).unwrap()["id"].as_u64().unwrap(),
            );
        }
    }
    doc.execute("slide.new", json!({ "layout": "titleAndContent", "title": "Third", "body": "Body" })).unwrap();
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    let second = std::sync::Arc::make_mut(&mut file.slides[1]);
    for shape in &mut second.shapes {
        if let Some(at) = rects[2..].iter().position(|id| u64::from(shape.id.0) == *id) {
            shape.id = kition_slide_model::ShapeId(u32::try_from(rects[at]).unwrap());
        }
    }
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap();
    SlideDocument::open("Deck.pptx", &bytes).unwrap()
}

fn agent() -> Origin {
    Origin::agent("agent")
}

/// Slide `index` (from 0) as `slide_inspect` lists it.
fn slide(doc: &mut SlideDocument, index: usize) -> Value {
    doc.execute_as("slide.inspect", json!({ "index": index }), &agent()).unwrap()
}

fn shapes(doc: &mut SlideDocument, index: usize) -> Vec<Value> {
    slide(doc, index)["shapes"].as_array().unwrap().clone()
}

/// The shape of a listed slide whose text is `text`.
fn shape_with(slide: &Value, text: &str) -> Value {
    slide["shapes"].as_array().unwrap().iter().find(|shape| shape["text"] == text).cloned().unwrap_or_else(|| panic!("no {text}: {slide}"))
}

/// The shape of slide `index` named as it was listed before (`listed`).
fn now(doc: &mut SlideDocument, index: usize, listed: &Value) -> Value {
    shapes(doc, index).into_iter().find(|shape| shape["toolId"] == listed["toolId"]).unwrap_or_else(|| panic!("no {listed}"))
}

fn load(exchange: &mut Exchange, doc: &mut SlideDocument, target: &str, field: &str) -> Value {
    let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), true).unwrap();
    answer["loaded"][0]["fields"][field].clone()
}

/// The tool ids the object model lists for the slides, and for slide `index`'s shapes.
fn model_ids(doc: &mut SlideDocument, index: usize) -> (Vec<Value>, Vec<Value>) {
    let mut exchange = Exchange::new();
    let slides = load(&mut exchange, doc, "root", "slides").as_array().unwrap().clone();
    let shapes = load(&mut exchange, doc, slides[index].as_str().unwrap(), "shapes").as_array().unwrap().clone();
    (slides, shapes)
}

#[test]
fn inspect_and_the_object_model_name_slides_and_shapes_by_one_id() {
    let mut doc = deck();
    let (first, second) = (slide(&mut doc, 0), slide(&mut doc, 1));
    let second_id = second["id"].as_u64().unwrap();
    assert_eq!(second["toolId"], format!("slide:{second_id}"), "{second}");
    let number = shape_with(&first, "A")["id"].as_u64().unwrap();
    assert_eq!(shape_with(&second, "A")["id"], number, "slides 1 and 2 share shape numbers");
    let file = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    let identity = |index: usize| format!("shape:{}", file.slides[index].shape(kition_slide_model::ShapeId(number as u32)).unwrap().identity);
    assert_eq!(shape_with(&first, "A")["toolId"], identity(0));
    assert_eq!(shape_with(&second, "A")["toolId"], identity(1), "the same number, another id");
    for index in 0..3 {
        let listed = slide(&mut doc, index);
        let (slides, model) = model_ids(&mut doc, index);
        assert_eq!(slides[index], listed["toolId"], "the object model lists slide {} by its tool id", index + 1);
        let inspected: Vec<Value> = listed["shapes"].as_array().unwrap().iter().map(|shape| shape["toolId"].clone()).collect();
        assert_eq!(model, inspected, "slide {}: the object model lists the ids inspect lists", index + 1);
    }
    // A query by that id reaches that shape, and only it.
    let mut exchange = Exchange::new();
    let listed = shape_with(&second, "A");
    let target = listed["toolId"].as_str().unwrap();
    let answer = exchange.batch(&mut doc, &json!({ "ops": [], "load": [{ "target": target, "fields": ["name"] }] }), true).unwrap();
    assert_eq!(answer["loaded"][0]["id"], target);
    assert_eq!(answer["loaded"][0]["fields"]["name"], listed["name"]);
}

#[test]
fn commands_and_on_take_tool_ids_which_name_their_slide() {
    let mut doc = deck();
    let (first, second) = (slide(&mut doc, 0), slide(&mut doc, 1));
    let (a1, a2, b2) = (shape_with(&first, "A"), shape_with(&second, "A"), shape_with(&second, "B"));
    let (a1, a2, b2) = (a1["toolId"].clone(), a2["toolId"].clone(), b2["toolId"].clone());
    let at = |id: &Value| json!({ "toolId": id });
    // No `onSlide`: the id names slide 2.
    doc.execute_as("text.set", json!({ "id": a2, "text": "Edited" }), &agent()).unwrap();
    assert_eq!(now(&mut doc, 1, &at(&a2))["text"], "Edited");
    assert_eq!(now(&mut doc, 0, &at(&a1))["text"], "A", "slide 1's shape with the same number is another");
    doc.execute_as("shape.fill", json!({ "ids": [a2, b2], "color": "#00A000" }), &agent()).unwrap();
    assert_eq!((now(&mut doc, 1, &at(&a2))["fill"].clone(), now(&mut doc, 1, &at(&b2))["fill"].clone()), (json!("#00A000"), json!("#00A000")));
    assert_ne!(now(&mut doc, 0, &at(&a1))["fill"], "#00A000");
    // Ids of two slides in one call, or an `onSlide` that names another slide, are refused.
    let error = doc.execute_as("shape.fill", json!({ "ids": [a1, b2], "color": "#000000" }), &agent()).unwrap_err();
    assert!(error.contains("two slides") || error.contains("more than one slide"), "{error}");
    let error = doc.execute_as("shape.fill", json!({ "ids": [a2], "onSlide": 1, "color": "#000000" }), &agent()).unwrap_err();
    assert!(error.contains("slide 2") && error.contains("onSlide"), "{error}");
    let error = doc.execute_as("shape.fill", json!({ "ids": ["shape:999999/2"], "color": "#000000" }), &agent()).unwrap_err();
    assert!(error.contains("no slide has id 999999"), "{error}");
    // `on` for a command that reads the selection: the id names the slide, and `at.part` must agree.
    let context = |value: Value| -> CallContext { serde_json::from_value(value).unwrap() };
    doc.set_context(&context(json!({ "on": [a2] }))).unwrap();
    assert_eq!(doc.editor_state()["selection"]["slide"], 1, "{}", doc.editor_state());
    doc.set_context(&context(json!({ "part": { "slide": 2 }, "on": [a2] }))).unwrap();
    let error = doc.set_context(&context(json!({ "part": { "slide": 1 }, "on": [a2] }))).unwrap_err();
    assert!(error.contains("slide 2") && error.contains("slide 1"), "{error}");
    // Slides by their tool ids.
    let third = slide(&mut doc, 2)["toolId"].clone();
    doc.execute_as("slide.delete", json!({ "ids": [third] }), &agent()).unwrap();
    assert_eq!(doc.inspect().unwrap()["slides"].as_array().unwrap().len(), 2);
}

#[test]
fn tool_ids_survive_reopening_and_reordering_and_deleting_slides() {
    let mut doc = deck();
    let before: Vec<Value> = (0..3).map(|index| slide(&mut doc, index)).collect();
    let ids = |slides: &[Value]| -> Vec<(Value, Vec<Value>)> {
        slides
            .iter()
            .map(|slide| (slide["toolId"].clone(), slide["shapes"].as_array().unwrap().iter().map(|shape| shape["toolId"].clone()).collect()))
            .collect()
    };
    // Saved and opened again.
    let mut reopened = SlideDocument::open("Deck.pptx", &doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    let again: Vec<Value> = (0..3).map(|index| slide(&mut reopened, index)).collect();
    assert_eq!(ids(&again), ids(&before));
    for index in 0..3 {
        let (slides, model) = model_ids(&mut reopened, index);
        assert_eq!((slides[index].clone(), model), ids(&before)[index].clone(), "the object model after reopening");
    }
    // The last slide moved first, then a slide inserted before it: every id stays.
    reopened.execute_as("slide.move", json!({ "from": 2, "to": 0 }), &agent()).unwrap();
    reopened.execute("slide.go", json!({ "index": 0 })).unwrap();
    reopened.execute_as("slide.new", json!({ "layout": "blank" }), &agent()).unwrap();
    reopened.execute_as("slide.move", json!({ "from": 1, "to": 0 }), &agent()).unwrap();
    let moved: Vec<Value> = (0..4).map(|index| slide(&mut reopened, index)).collect();
    let moved_ids = ids(&moved);
    for (id, shapes) in ids(&before) {
        assert!(moved_ids.contains(&(id.clone(), shapes.clone())), "{id} keeps its shapes' ids: {moved_ids:?}");
    }
    // Deleting a slide leaves the others' ids.
    let gone = before[0]["toolId"].clone();
    reopened.execute_as("slide.delete", json!({ "ids": [gone] }), &agent()).unwrap();
    let after: Vec<Value> = (0..3).map(|index| slide(&mut reopened, index)).collect();
    for kept in &ids(&before)[1..] {
        assert!(ids(&after).contains(kept), "{kept:?}");
    }
    // An undo reads the slides from the object model again: the ids are the same.
    reopened.execute_as("edit.undo", json!({}), &agent()).unwrap();
    let undone: Vec<Value> = (0..4).map(|index| slide(&mut reopened, index)).collect();
    for kept in ids(&before) {
        assert!(ids(&undone).contains(&kept), "after an undo {kept:?}: {:?}", ids(&undone));
    }
}

#[test]
fn numeric_secondary_ids_use_the_slide_implied_by_a_tool_id() {
    let mut doc = deck();
    let second = slide(&mut doc, 1);
    let a = shape_with(&second, "A");
    let b = shape_with(&second, "B");
    doc.execute("slide.go", json!({ "index": 0 })).unwrap();
    for (command, params) in [
        ("shape.connect", json!({ "from": a["toolId"], "to": b["id"] })),
        ("drawing.connect", json!({ "from": a["toolId"], "to": b["id"] })),
        ("drawing.reconnect", json!({ "id": a["toolId"], "to": b["id"] })),
    ] {
        let under = serde_json::to_value(doc.under(command, &params).unwrap().unwrap()).unwrap();
        assert!(under["premises"].as_array().unwrap().iter().any(|p| p["id"] == b["toolId"]), "{command}: {under}");
    }
}

#[test]
fn animation_triggers_take_tool_ids_and_numeric_strings() {
    let mut doc = deck();
    let second = slide(&mut doc, 1);
    let a = shape_with(&second, "A");
    let b = shape_with(&second, "B");
    doc.execute("animation.add", json!({ "ids": [a["toolId"]], "effect": "fade" })).unwrap();
    doc.set_context(&serde_json::from_value(json!({ "on": [a["toolId"]] })).unwrap()).unwrap();
    for trigger in [b["toolId"].clone(), json!(b["id"].as_u64().unwrap().to_string())] {
        doc.execute("animation.timing", json!({ "trigger": trigger })).unwrap();
        let animations = doc.execute("animation.get", json!({ "slide": 1 })).unwrap();
        assert_eq!(animations[0]["trigger"], b["id"], "trigger {trigger}: {animations}");
    }
}
