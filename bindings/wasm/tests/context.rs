//! The explicit call context (kition-sdk `KitDocument::editor_state`, `restore_editor_state`,
//! `set_context`; task card C4-slide): what an agent's call acts on is named by the call, a host
//! sets it for one call and puts the editor back, and no command given what it acts on depends on,
//! or leaves behind, the editor's current slide or selection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::{CallContext, KitDocument};
use kition_slide_engine::Origin;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

/// Three slides; the first two hold two rectangles with the same ids (as in PowerPoint, shape ids
/// are unique only within a slide). Answers the deck and those two ids.
fn deck() -> (SlideDocument, u64, u64) {
    let mut doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
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
    // Slide 2's rectangles take slide 1's ids.
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.kslide", &doc.save(None).unwrap()).unwrap();
    let second = std::sync::Arc::make_mut(&mut file.slides[1]);
    for shape in &mut second.shapes {
        if let Some(at) = rects[2..].iter().position(|id| u64::from(shape.id.0) == *id) {
            shape.id = kition_slide_model::ShapeId(u32::try_from(rects[at]).unwrap());
        }
    }
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "kslide").unwrap();
    let mut doc = SlideDocument::open("Deck.kslide", &bytes).unwrap();
    let (a, b) = (rects[0], rects[1]);
    for slide in [0, 1] {
        let ids = shape_ids(&mut doc, slide);
        assert!(ids.contains(&a) && ids.contains(&b), "slide {} holds the same ids: {ids:?}", slide + 1);
    }
    assert_eq!(doc.editor_state()["selection"]["slide"], 0);
    (doc, a, b)
}

fn shape_ids(doc: &mut SlideDocument, index: usize) -> Vec<u64> {
    slide(doc, index)["shapes"].as_array().unwrap().iter().filter_map(|shape| shape["id"].as_u64()).collect()
}

/// Slide `index` (from 0) as an agent reads it.
fn slide(doc: &mut SlideDocument, index: usize) -> Value {
    doc.execute_as("slide.inspect", json!({ "index": index }), &Origin::agent("agent")).unwrap()
}

fn fill_of(doc: &mut SlideDocument, index: usize, id: u64) -> Value {
    slide(doc, index)["shapes"].as_array().unwrap().iter().find(|shape| shape["id"] == id).map(|shape| shape["fill"].clone()).unwrap()
}

/// What `inspect()` says of the deck, without the editor's state (`EDITOR_STATE`).
fn deck_only(doc: &SlideDocument) -> Value {
    let mut inspected = doc.inspect().unwrap();
    for pointer in kition_slide_kit::EDITOR_STATE {
        inspected.as_object_mut().unwrap().remove(pointer.trim_start_matches('/'));
    }
    inspected
}

fn context(value: Value) -> CallContext {
    serde_json::from_value(value).unwrap()
}

/// The editor as a person left it: on slide 3, with its title placeholder selected.
fn person_on_slide_three(doc: &mut SlideDocument) -> Value {
    doc.execute("slide.go", json!({ "index": 2 })).unwrap();
    let title = shape_ids(doc, 2)[0];
    doc.execute("edit.select", json!({ "ids": [title] })).unwrap();
    let state = doc.editor_state();
    assert_eq!(state["selection"]["slide"], 2, "{state}");
    assert_eq!(state["selection"]["shapes"], json!([title]), "{state}");
    state
}

#[test]
fn the_sdk_contract_holds_for_the_editor_state_hooks() {
    let (mut doc, a, b) = deck();
    person_on_slide_three(&mut doc);
    // kition-sdk's check leaves out the editor state the kit declares (`EDITOR_STATE`), so a
    // context that really moves the editor (another slide, shapes selected there) is checked.
    kition_kit::testing::check_context(&mut doc, &CallContext::default(), kition_slide_kit::EDITOR_STATE).unwrap();
    kition_kit::testing::check_context(&mut doc, &context(json!({ "part": { "slide": 1 } })), kition_slide_kit::EDITOR_STATE).unwrap();
    kition_kit::testing::check_context(&mut doc, &context(json!({ "part": { "slide": 2 }, "on": [a, b] })), kition_slide_kit::EDITOR_STATE).unwrap();
    assert_eq!(doc.editor_state()["selection"]["slide"], 2, "the person's slide is back");
    let first = doc.inspect().unwrap()["slides"].clone();
    assert!(first.is_array());
}

#[test]
fn setting_the_context_is_no_edit_and_restoring_puts_the_editor_back() {
    let (mut doc, a, b) = deck();
    let saved = person_on_slide_three(&mut doc);
    let (revision, dirty, history, deck) = (doc.revision(), doc.dirty(), doc.inspect().unwrap()["canUndo"].clone(), deck_only(&doc));
    doc.set_context(&context(json!({ "part": { "slide": 2 }, "on": [a, b.to_string()] }))).unwrap();
    let state = doc.editor_state();
    assert_eq!(state["selection"]["slide"], 1, "slide 2 is current: {state}");
    assert_eq!(state["selection"]["shapes"], json!([a, b]), "{state}");
    assert_eq!(
        (doc.revision(), doc.dirty(), doc.inspect().unwrap()["canUndo"].clone()),
        (revision, dirty, history),
        "setting the context is no edit"
    );
    assert_eq!(deck_only(&doc), deck);
    let slide_id = slide(&mut doc, 0)["id"].clone();
    doc.set_context(&context(json!({ "part": { "slideId": slide_id } }))).unwrap();
    assert_eq!(doc.editor_state()["selection"]["slide"], 0);
    assert_eq!(doc.editor_state()["selection"]["shapes"], json!([]), "a part alone selects nothing there");
    doc.restore_editor_state(&saved).unwrap();
    assert_eq!(doc.editor_state(), saved);
    assert_eq!((doc.revision(), doc.dirty()), (revision, dirty), "restoring is no edit");
}

#[test]
fn commands_given_their_targets_leave_the_current_slide_and_the_selection_as_they_were() {
    let (mut doc, a, b) = deck();
    let agent = Origin::agent("agent");
    let saved = person_on_slide_three(&mut doc);
    let unchanged = |doc: &mut SlideDocument, after: &str| assert_eq!(doc.editor_state(), saved, "after {after}");
    // Text edit, on another slide.
    doc.execute_as("text.set", json!({ "id": a, "text": "Edited", "onSlide": 2 }), &agent).unwrap();
    unchanged(&mut doc, "text.set {id, onSlide}");
    assert_eq!(slide(&mut doc, 1)["shapes"].as_array().unwrap().iter().find(|s| s["id"] == a).unwrap()["text"], "Edited");
    assert_eq!(slide(&mut doc, 0)["shapes"].as_array().unwrap().iter().find(|s| s["id"] == a).unwrap()["text"], "A", "slide 1's shape is another");
    // Formatting with ids (the engine selects them for the command only), with and without onSlide.
    doc.execute_as("format.bold", json!({ "ids": [a, b], "onSlide": 1, "on": true }), &agent).unwrap();
    unchanged(&mut doc, "format.bold {ids, onSlide}");
    let title = saved["selection"]["shapes"][0].as_u64().unwrap();
    doc.execute_as("format.color", json!({ "ids": [title], "color": "#C00000" }), &agent).unwrap();
    unchanged(&mut doc, "format.color {ids}");
    // A shape command with ids that fails still leaves the editor as it was.
    assert!(doc.execute_as("shape.fill", json!({ "ids": [999_999], "onSlide": 2, "color": "#00FF00" }), &agent).is_err());
    unchanged(&mut doc, "a failed shape.fill {ids, onSlide}");
    // Reading a slide.
    slide(&mut doc, 1);
    unchanged(&mut doc, "slide.inspect {index}");
    // Drawing a slide.
    doc.execute_as("file.render", json!({ "target": { "slide": 2 }, "scale": 0.25 }), &agent).unwrap();
    unchanged(&mut doc, "file.render {target}");
    // Inserting on a slide, as a host runs a step with `at.part`: set the context, run, put back.
    doc.set_context(&context(json!({ "part": { "slide": 2 } }))).unwrap();
    let inserted = doc.execute_as("textBox.insert", json!({ "rect": [500, 300, 200, 60], "text": "New" }), &agent).unwrap()["id"].as_u64().unwrap();
    doc.restore_editor_state(&saved).unwrap();
    unchanged(&mut doc, "textBox.insert at slide 2");
    assert!(shape_ids(&mut doc, 1).contains(&inserted), "the text box is on slide 2");
    // The person's own read still makes the slide current (`select: false` reads only).
    doc.execute("slide.inspect", json!({ "index": 1, "select": false })).unwrap();
    assert_eq!(doc.editor_state(), saved);
}

#[test]
fn on_and_a_part_select_exactly_those_shapes_for_a_command_that_reads_the_selection() {
    let (mut doc, a, b) = deck();
    let saved = person_on_slide_three(&mut doc);
    let before = (fill_of(&mut doc, 0, a), fill_of(&mut doc, 1, b));
    doc.set_context(&context(json!({ "part": { "slide": 2 }, "on": [a] }))).unwrap();
    // `shape.fill` without ids fills the selection.
    doc.execute_as("shape.fill", json!({ "color": "#00A000" }), &Origin::agent("agent")).unwrap();
    doc.restore_editor_state(&saved).unwrap();
    assert_eq!(fill_of(&mut doc, 1, a), "#00A000", "slide 2's shape {a} is filled");
    assert_eq!((fill_of(&mut doc, 0, a), fill_of(&mut doc, 1, b)), before, "slide 1's shape {a} and slide 2's {b} are not");
    assert_eq!(doc.editor_state(), saved);
}

#[test]
fn the_same_id_on_two_slides_is_reached_on_the_slide_each_step_names() {
    let (mut doc, a, _) = deck();
    let agent = Origin::agent("agent");
    let saved = person_on_slide_three(&mut doc);
    // One run, as a host runs it: each step's `at.part` is set before it, the editor put back after.
    for (part, color) in [(json!({ "slide": 1 }), "#FF0000"), (json!({ "slide": 2 }), "#0000FF")] {
        doc.set_context(&context(json!({ "part": part, "on": [a] }))).unwrap();
        doc.execute_as("shape.fill", json!({ "color": color }), &agent).unwrap();
    }
    doc.restore_editor_state(&saved).unwrap();
    assert_eq!(fill_of(&mut doc, 0, a), "#FF0000");
    assert_eq!(fill_of(&mut doc, 1, a), "#0000FF");
    // The same with ids and onSlide, without any context.
    doc.execute_as("shape.fill", json!({ "ids": [a], "onSlide": 1, "color": "#00FF00" }), &agent).unwrap();
    doc.execute_as("shape.fill", json!({ "ids": [a], "onSlide": 2, "color": "#FFFF00" }), &agent).unwrap();
    assert_eq!((fill_of(&mut doc, 0, a), fill_of(&mut doc, 1, a)), (json!("#00FF00"), json!("#FFFF00")));
    assert_eq!(doc.editor_state(), saved);
}

#[test]
fn a_context_the_deck_cannot_take_is_refused_saying_what_to_give_and_changes_nothing() {
    let (mut doc, a, _) = deck();
    let saved = person_on_slide_three(&mut doc);
    let refused = |doc: &mut SlideDocument, given: Value| {
        let error = doc.set_context(&context(given.clone())).unwrap_err();
        assert_eq!(doc.editor_state(), saved, "a refused {given} changed the editor");
        error
    };
    let error = refused(&mut doc, json!({ "on": [a] }));
    assert!(error.contains("give the slide as well") && error.contains("at.part"), "{error}");
    let error = refused(&mut doc, json!({ "part": { "slide": 9 } }));
    assert!(error.contains("there is no slide 9") && error.contains("3 slides") && error.contains("slide_inspect"), "{error}");
    let error = refused(&mut doc, json!({ "part": { "slideId": 999_999 } }));
    assert!(error.contains("no slide has id 999999") && error.contains("slide_inspect"), "{error}");
    let error = refused(&mut doc, json!({ "part": { "slide": 2 }, "on": [a, 999_999] }));
    assert!(error.contains("slide 2 has no shape `999999`") && error.contains("slide_inspect"), "{error}");
    let error = refused(&mut doc, json!({ "part": { "page": 1 } }));
    assert!(error.contains("parts are slides"), "{error}");
    let error = refused(&mut doc, json!({ "part": { "slide": 1 }, "time": 2.0 }));
    assert!(error.contains("not available") && error.contains("timeline"), "{error}");
    let error = doc.restore_editor_state(&json!({ "slide": 1 })).unwrap_err();
    assert!(error.contains("not the editor state"), "{error}");
}

/// The agent's preview (`file.render`, the kit's `agent.render`) draws the slide the call names,
/// the same bytes whatever is current or selected and on every call.
#[test]
fn the_render_is_a_function_of_the_deck_and_the_view_the_call_gives() {
    let (mut doc, a, _) = deck();
    let agent = Origin::agent("agent");
    let render = |doc: &mut SlideDocument, params: Value| doc.execute_as("file.render", params, &agent).unwrap();
    doc.execute("text.edit", json!({ "id": a })).unwrap();
    let first = render(&mut doc, json!({ "target": { "slide": 3 }, "scale": 0.5 }));
    assert_eq!((first["slide"].clone(), first["width"].clone(), first["height"].clone()), (json!(3), json!(480), json!(270)), "{}", first["slide"]);
    assert_eq!(render(&mut doc, json!({ "target": { "slide": 3 }, "scale": 0.5 }))["png"], first["png"], "the same call, the same bytes");
    person_on_slide_three(&mut doc);
    assert_eq!(render(&mut doc, json!({ "target": { "slide": 3 }, "scale": 0.5 }))["png"], first["png"], "the current slide changes nothing");
    let by_id = render(&mut doc, json!({ "target": { "slideId": first["slideId"] }, "scale": 0.5 }));
    assert_eq!(by_id["png"], first["png"]);
    assert_eq!(render(&mut doc, json!({ "slide": 2, "scale": 0.5 }))["png"], first["png"], "`slide` is an index from 0");
    // Without a slide, the first: never the current one.
    let default = render(&mut doc, json!({ "scale": 0.5 }));
    assert_eq!(default["slide"], 1);
    assert_eq!(default["png"], render(&mut doc, json!({ "target": { "slide": 1 }, "scale": 0.5 }))["png"]);
    // maxSize brings it down to fit.
    let small = render(&mut doc, json!({ "target": { "slide": 1 }, "scale": 2, "maxSize": 96 }));
    assert_eq!((small["width"].clone(), small["height"].clone()), (json!(96), json!(54)));
    for (bad, says) in [
        (json!({ "target": { "slide": 4 } }), "counted from 1"),
        (json!({ "target": { "slide": 0 } }), "counted from 1"),
        (json!({ "target": { "slideId": 999_999 } }), "no slide has id"),
        (json!({ "target": { "slide": 1 }, "slide": 0 }), "named twice"),
        (json!({ "target": { "slide": 1, "slideId": 1 } }), "twice"),
        (json!({ "scale": 40 }), "at most 16"),
        (json!({ "maxSize": 4 }), "16 to 8192"),
    ] {
        let error = doc.execute_as("file.render", bad.clone(), &agent).unwrap_err();
        assert!(error.contains(says), "{bad}: {error}");
    }
}

#[test]
fn the_render_command_declares_the_view_it_takes() {
    let doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    let render = doc.commands().into_iter().find(|command| command.id == "file.render").unwrap();
    let schema = render.schema.unwrap();
    let members: Vec<&String> = schema["target"]["properties"].as_object().unwrap().keys().collect();
    assert_eq!(members, ["slide", "slideId"], "{schema}");
    assert_eq!(schema["target"]["additionalProperties"], false);
    for key in ["scale", "maxSize", "slide"] {
        assert!(schema.get(key).is_some(), "{key}: {schema}");
    }
    assert!(render.reads.is_empty(), "it reads no editor state");
}

/// Every command that names what it acts on (`ids`, `id`) declares it as `targets`: kition-sdk's
/// `schema::check_targets` takes this kit's ids (a number, or a string such as the tool id
/// `shape:<slideId>/<n>`, `oneOf`), so none waits on the SDK any more.
#[test]
fn commands_declare_the_parameter_that_names_what_they_act_on() {
    let doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    let mut refused = Vec::new();
    let mut declared = 0;
    for command in doc.commands() {
        let Some(schema) = &command.schema else {
            assert!(command.targets.is_none(), "{}", command.id);
            continue;
        };
        match kition_slide_kit::acts::names_objects(&command.id, schema) {
            None => assert!(command.targets.is_none(), "{} names nothing it acts on", command.id),
            Some(name) => {
                if let Err(error) = kition_kit::schema::check_targets(schema, name) {
                    refused.push(format!("{}: {error}", command.id));
                }
                assert_eq!(command.targets, Some(name), "{}", command.id);
                declared += 1;
            }
        }
    }
    assert!(refused.is_empty(), "{refused:#?}");
    for (id, name) in [("edit.select", "ids"), ("text.set", "id"), ("format.bold", "ids"), ("slide.delete", "ids")] {
        assert!(doc.commands().iter().any(|command| command.id == id && command.targets == Some(name)), "{id} declares `{name}`");
    }
    eprintln!("{declared} commands declare targets");
}

#[test]
fn scoped_shape_coverage_excludes_private_text_objects_but_keeps_unknown_shape_addresses() {
    let mut doc = SlideDocument::create("Coverage.pptx", 960, 540).unwrap();
    let made = doc.execute("textBox.insert", serde_json::json!({"text":"Body", "rect":[10,10,100,50]})).unwrap();
    doc.version().unwrap();
    let asks: Vec<kition_kit::CoveredAsk> = serde_json::from_value(serde_json::json!([
        {"id":"01234567-89ab-7123-8123-0123456789ab", "deleted":true, "unknown":["address"], "before":{"text":"Other"}},
        {"id":"shape:0123456789ab712381230123456789ab", "unknown":["address"]}
    ]))
    .unwrap();
    let answer = doc.covered("text.set", &serde_json::json!({"id":made["id"], "text":"Mine"}), &asks).unwrap().unwrap();
    assert_eq!(answer.covered[0].covered, Some(false));
    assert_eq!(answer.covered[1].covered, None);
}
