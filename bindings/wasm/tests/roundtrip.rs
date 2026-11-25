//! Core editing survives save and reopen: each test edits a new deck only through engine commands
//! (as the editor and the agent do), checks the edit took, saves it as .kslide and as .pptx, opens
//! each saved file again and compares the deck before and after.
//!
//! The comparison is `file.inspect` plus, for every slide, `slide.inspect` and `shape.inspect`
//! of each of its shapes (geometry, text, paragraph and run properties, tables, fills). Left out:
//! object ids and refs (a .pptx gives every object a new one), the selection and undo state, and
//! the slide's design advice (derived from the content and naming shapes by id), the deck's title
//! (its file name) and a text body's `autofitInput` (a cache key recomputed on open).
//!
//! A .pptx states some things the model leaves implicit; `equivalent` writes both sides the same
//! way before comparing, and nothing else is forgiven:
//! - a shape's own text with no anchor shows middle (shapes) or top (text boxes, tables) in Kition
//!   (`resolve::body`); the .pptx writes that anchor out and reads it back explicitly
//!   (`write/shapes.rs`, `read/shapes.rs`);
//! - a text body with no paragraph is written with one empty paragraph, which the schema requires;
//! - an animation with no preset subtype is subtype 0 (`read/timing.rs`, `write/timing.rs`);
//! - sizes are compared to 1e-6 pt: a .pptx holds them in whole EMUs.

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn exec(doc: &mut SlideDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn new_deck() -> SlideDocument {
    SlideDocument::create("Deck.kslide", 960, 540).unwrap_or_else(|error| panic!("{error}"))
}

fn id_of(answer: &Value) -> Value {
    answer["id"].clone()
}

/// Drops the keys a reopened file may change for reasons other than lost content.
fn strip(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for volatile in ["id", "ref", "selection", "dirty", "redo", "undo", "path", "design"] {
                object.remove(volatile);
            }
            object.values_mut().for_each(strip);
        }
        Value::Array(items) => items.iter_mut().for_each(strip),
        _ => {}
    }
}

/// Writes what a .pptx makes explicit the way it does (see the module comment).
fn equivalent(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(shape) = object.get_mut("shape").and_then(Value::as_object_mut) {
                let own = shape.get("ph").is_none_or(Value::is_null);
                let top = shape.get("textBox") == Some(&json!(true)) || shape["kind"]["type"] == json!("table");
                if let Some(text) = shape.get_mut("text").and_then(Value::as_object_mut) {
                    if own && text["body"]["anchor"].is_null() {
                        text["body"]["anchor"] = json!(if top { "top" } else { "middle" });
                    }
                    if text.get("paragraphs") == Some(&json!([])) {
                        text.insert("paragraphs".into(), json!([{ "level": 0, "runs": [] }]));
                    }
                    if let Some(body) = text.get_mut("body").and_then(Value::as_object_mut) {
                        body.remove("autofitInput");
                    }
                }
            }
            if let Some(subtype) = object.get_mut("presetSubtype")
                && subtype.is_null()
            {
                *subtype = json!(0);
            }
            object.values_mut().for_each(equivalent);
        }
        Value::Array(items) => items.iter_mut().for_each(equivalent),
        Value::Number(number) => {
            if let Some(float) = number.as_f64().filter(|_| number.is_f64()) {
                *value = json!((float * 1e6).round() / 1e6);
            }
        }
        _ => {}
    }
}

fn state(doc: &mut SlideDocument) -> Value {
    let mut deck = exec(doc, "file.inspect", json!({}));
    let count = deck["slides"].as_array().map_or(0, Vec::len);
    let mut slides = Vec::new();
    for index in 0..count {
        let mut slide = exec(doc, "slide.inspect", json!({ "index": index }));
        let shapes: Vec<Value> = slide["shapes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|shape| exec(doc, "shape.inspect", json!({ "id": shape["id"], "onSlide": index + 1 })))
            .collect();
        strip(&mut slide);
        let mut shapes = Value::Array(shapes);
        strip(&mut shapes);
        slides.push(json!({ "slide": slide, "shapes": shapes }));
    }
    strip(&mut deck);
    // The deck's title is its file name, not content.
    deck.as_object_mut().map(|deck| deck.remove("title"));
    let mut state = json!({ "deck": deck, "slides": slides });
    equivalent(&mut state);
    state
}

/// Saves `doc` as `name`, opens the bytes again and asserts the deck did not change.
fn assert_survives(doc: &mut SlideDocument, name: &str) {
    let before = state(doc);
    let bytes = doc.save(Some(name)).unwrap_or_else(|error| panic!("save {name}: {error}"));
    let mut reopened = SlideDocument::open(name, &bytes).unwrap_or_else(|error| panic!("reopen {name}: {error}"));
    let after = state(&mut reopened);
    let mut changes = Vec::new();
    differences(&before, &after, "", &mut changes);
    assert!(changes.is_empty(), "the deck changed across saving and reopening {name}:\n{}", changes.join("\n"));
}

fn assert_survives_save_and_reopen(doc: &mut SlideDocument) {
    assert_survives(doc, "Round trip.kslide");
    assert_survives(doc, "Round trip.pptx");
}

/// Each JSON pointer where `before` and `after` differ, with both values.
fn differences(before: &Value, after: &Value, at: &str, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
                differences(a.get(key).unwrap_or(&Value::Null), b.get(key).unwrap_or(&Value::Null), &format!("{at}/{key}"), out);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                differences(x, y, &format!("{at}/{index}"), out);
            }
        }
        _ if before != after => out.push(format!("{at}: {before} -> {after}")),
        _ => {}
    }
}

/// The text of shape `id` on slide `slide` (counted from 1, as `onSlide` is).
fn shape_text(doc: &mut SlideDocument, id: &Value, slide: usize) -> Value {
    exec(doc, "shape.inspect", json!({ "id": id, "onSlide": slide }))["shape"]["text"].clone()
}

#[test]
fn slides_and_layouts_survive_save_and_reopen() {
    let mut doc = new_deck();
    exec(&mut doc, "slide.new", json!({ "title": "Agenda", "body": "Goals\nTimeline\nBudget" }));
    exec(&mut doc, "slide.new", json!({ "title": "Timeline", "layout": "Title Only" }));
    exec(&mut doc, "slide.duplicate", json!({ "index": 1 }));
    exec(&mut doc, "slide.move", json!({ "from": 3, "to": 1 }));
    exec(&mut doc, "slide.layout", json!({ "index": 3, "layout": "Section Header" }));
    let deck = exec(&mut doc, "file.inspect", json!({}));
    let layouts: Vec<Value> = deck["slides"].as_array().unwrap().iter().map(|slide| slide["layout"].clone()).collect();
    // Title, Agenda, Timeline; the duplicate of Agenda goes after it; Timeline moves to second.
    assert_eq!(layouts, [json!("Title Slide"), json!("Title Only"), json!("Title and Content"), json!("Section Header")], "{deck}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn text_and_character_formatting_survive_save_and_reopen() {
    let mut doc = new_deck();
    let bold = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [60, 60, 400, 60], "text": "Bold and large" })));
    let italic = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [60, 140, 400, 60], "text": "Italic, underlined, red" })));
    let font = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [60, 220, 400, 60], "text": "A serif font" })));
    exec(&mut doc, "format.bold", json!({ "ids": [bold], "on": true }));
    exec(&mut doc, "format.fontSize", json!({ "ids": [bold], "size": 36 }));
    exec(&mut doc, "format.italic", json!({ "ids": [italic], "on": true }));
    exec(&mut doc, "format.underline", json!({ "ids": [italic], "on": true, "style": "sng" }));
    exec(&mut doc, "format.color", json!({ "ids": [italic], "color": "#C00000" }));
    exec(&mut doc, "format.fontName", json!({ "ids": [font], "family": "Georgia" }));
    exec(&mut doc, "text.set", json!({ "id": font, "text": "A serif font, retyped" }));
    let text = shape_text(&mut doc, &bold, 1).to_string();
    assert!(text.contains("\"bold\":true") && text.contains("36"), "{text}");
    let text = shape_text(&mut doc, &italic, 1).to_string();
    assert!(text.contains("\"italic\":true") && text.contains("\"r\":192"), "{text}");
    let text = shape_text(&mut doc, &font, 1).to_string();
    assert!(text.contains("Georgia") && text.contains("retyped"), "{text}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn paragraph_formatting_survives_save_and_reopen() {
    let mut doc = new_deck();
    let centred = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [60, 60, 600, 60], "text": "A centred line" })));
    let list = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [60, 160, 600, 160], "text": "First\nSecond\nThird" })));
    exec(&mut doc, "format.alignCenter", json!({ "ids": [centred] }));
    exec(&mut doc, "format.bullets", json!({ "ids": [list], "on": true }));
    exec(&mut doc, "format.indent", json!({ "ids": [list] }));
    let text = shape_text(&mut doc, &centred, 1).to_string();
    assert!(text.contains("\"ctr\"") || text.contains("center"), "{text}");
    let text = shape_text(&mut doc, &list, 1).to_string();
    assert!(text.contains("bullet") || text.contains("buChar"), "{text}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn shapes_survive_save_and_reopen() {
    let mut doc = new_deck();
    let card = id_of(&exec(&mut doc, "shape.insert", json!({ "preset": "roundRect", "rect": [80, 80, 240, 120], "text": "Card" })));
    let arrow = id_of(&exec(&mut doc, "shape.insert", json!({ "preset": "rightArrow", "rect": [360, 110, 160, 60] })));
    exec(&mut doc, "shape.setBounds", json!({ "ids": [card], "x": 100, "y": 90, "w": 260, "h": 140 }));
    exec(&mut doc, "shape.rotate", json!({ "ids": [arrow], "deg": 30 }));
    let card_now = exec(&mut doc, "shape.inspect", json!({ "id": card, "onSlide": 1 }));
    assert_eq!(card_now["box"], json!({ "x": 100.0, "y": 90.0, "w": 260.0, "h": 140.0 }), "{card_now}");
    assert_eq!(card_now["shape"]["geom"]["name"], json!("roundRect"), "{card_now}");
    let arrow_now = exec(&mut doc, "shape.inspect", json!({ "id": arrow, "onSlide": 1 }));
    assert!(arrow_now.to_string().contains("30"), "{arrow_now}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn tables_survive_save_and_reopen() {
    let mut doc = new_deck();
    let table = id_of(&exec(&mut doc, "table.insert", json!({ "rows": 2, "cols": 2, "rect": [100, 100, 500, 120] })));
    for (row, col, text) in [(0, 0, "Item"), (0, 1, "Price"), (1, 0, "Tea"), (1, 1, "3.50")] {
        exec(&mut doc, "text.set", json!({ "id": table, "cell": [row, col], "text": text }));
    }
    exec(&mut doc, "table.insertRowBelow", json!({ "id": table, "row": 1 }));
    exec(&mut doc, "text.set", json!({ "id": table, "cell": [2, 0], "text": "Coffee" }));
    let state = exec(&mut doc, "table.state", json!({ "id": table }));
    let shape = exec(&mut doc, "shape.inspect", json!({ "id": table, "onSlide": 1 })).to_string();
    for text in ["Item", "Price", "Tea", "3.50", "Coffee"] {
        assert!(shape.contains(text), "{text} missing: {shape}");
    }
    assert!(state.to_string().contains('3'), "three rows: {state}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn transitions_and_animations_survive_save_and_reopen() {
    let mut doc = new_deck();
    exec(&mut doc, "slide.new", json!({ "title": "Moving parts", "body": "Fades in" }));
    let box_id = id_of(&exec(&mut doc, "textBox.insert", json!({ "rect": [100, 400, 300, 60], "text": "Appears" })));
    exec(&mut doc, "transition.set", json!({ "index": 1, "kind": "fade", "duration": 700 }));
    exec(&mut doc, "animation.add", json!({ "ids": [box_id], "onSlide": 2, "class": "entrance", "effect": "fade" }));
    let slide = exec(&mut doc, "slide.inspect", json!({ "index": 1 }));
    assert!(slide["animations"].as_array().is_some_and(|items| !items.is_empty()), "{slide}");
    let deck = exec(&mut doc, "file.inspect", json!({}));
    assert!(!deck["slides"][1]["transition"].is_null(), "{deck}");
    assert_survives_save_and_reopen(&mut doc);
}
