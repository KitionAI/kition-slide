//! `kition-slide` served headless keeps the editor's state (the current slide, the selection, Slide
//! Master view, text being edited) out of what an agent reads of the deck, and sets it for one call
//! and puts it back through the Kit Process Protocol's `context.get`, `context.set` and
//! `context.restore` (section 7a), so nothing an agent does depends on or leaves behind the
//! editor's current slide or selection (task card C4-slide).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_slide_kit::SlideDocument;
use serde_json::{Map, Value, json};

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The headless kit as the `kition-slide` executable makes it.
fn kit() -> Headless<SlideDocument> {
    Headless::<SlideDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_slide_kit::EDITOR_STATE).unwrap(),
    )
}

/// A folder holding `Deck.kslide`, three slides; the kit opens it.
fn open(name: &str) -> (Headless<SlideDocument>, String, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kition-slide-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut deck = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    for _ in 0..2 {
        deck.execute("slide.new", json!({ "layout": "titleAndContent", "title": "Title", "body": "Body" })).unwrap();
    }
    deck.execute("slide.go", json!({ "index": 0 })).unwrap();
    std::fs::write(root.join("Deck.kslide"), deck.save(None).unwrap()).unwrap();
    let mut kit = kit();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Deck.kslide").unwrap(), read_only: false, create: None }).unwrap().id;
    (kit, id, root)
}

fn run(kit: &mut Headless<SlideDocument>, id: &str, command: &str, value: Value, origin: Origin) -> Value {
    kit.execute(id, command, params(value), origin).unwrap_or_else(|error| panic!("{command}: {error:?}")).result.unwrap_or(Value::Null)
}

#[test]
fn what_an_agent_reads_of_the_deck_leaves_out_the_editors_state() {
    let (mut kit, id, root) = open("reads");
    let before = kit.describe(&id).unwrap().state;
    // A person moves to slide 2, selects a shape there, and opens Slide Master view.
    run(&mut kit, &id, "slide.go", json!({ "index": 1 }), Origin::User);
    run(&mut kit, &id, "edit.selectAll", json!({}), Origin::User);
    let selected = kit.describe(&id).unwrap().state;
    assert_eq!(selected, before, "the current slide and the selection changed what the agent reads of the deck");
    run(&mut kit, &id, "view.slideMaster", json!({}), Origin::User);
    assert_eq!(kit.describe(&id).unwrap().state, before, "Slide Master view changed what the agent reads of the deck");
    let text = serde_json::to_string(&selected).unwrap();
    for key in kition_slide_kit::EDITOR_STATE {
        let name = key.trim_start_matches('/');
        assert!(!text.contains(&format!("\"{name}\"")), "{name} is the editor's, not the deck's: {text}");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// The shapes of slide `number` (from 1) as the agent lists them, without moving the editor.
fn shapes(kit: &mut Headless<SlideDocument>, id: &str, number: u64) -> Vec<Value> {
    run(kit, id, "slide.inspect", json!({ "index": number - 1 }), Origin::Agent)["shapes"].as_array().cloned().unwrap_or_default()
}

/// A run as `kition-kits` makes it over the Kit Process Protocol (section 7a): `context.get` before
/// the first step, `context.set` with each step's `on` and `at.part`, `context.restore` after the
/// last. Steps land on the slides they name; the person's slide and selection are as they were.
#[test]
fn a_run_sets_each_steps_slide_and_selection_and_puts_the_editor_back() {
    let (mut kit, id, root) = open("run");
    // The person is on slide 1 with its title selected.
    let title = shapes(&mut kit, &id, 1)[0]["id"].clone();
    run(&mut kit, &id, "edit.select", json!({ "ids": [title] }), Origin::User);
    let saved = kit.context_get(&id).unwrap();
    assert_eq!(saved["selection"]["shapes"], json!([title]), "{saved}");
    let body = shapes(&mut kit, &id, 2)[1]["id"].clone();
    // Step 1: `format.bold` on slide 2's body through `on` (it reads the selection).
    kit.context_set(&id, &params(json!({ "part": { "slide": 2 }, "on": [body] }))).unwrap();
    run(&mut kit, &id, "format.bold", json!({ "on": true }), Origin::Agent);
    // Step 2: a text box on slide 3 (`at.part`).
    kit.context_set(&id, &params(json!({ "part": { "slide": 3 } }))).unwrap();
    let boxed = run(&mut kit, &id, "insert.textBox", json!({ "rect": [40, 40, 200, 50], "text": "Added" }), Origin::Agent)["id"].clone();
    kit.context_restore(&id, &saved).unwrap();
    assert_eq!(kit.context_get(&id).unwrap(), saved, "the editor is where the person left it");
    assert!(shapes(&mut kit, &id, 3).iter().any(|shape| shape["id"] == boxed && shape["text"] == "Added"), "the text box is on slide 3");
    let bold = run(&mut kit, &id, "shape.inspect", json!({ "id": body, "onSlide": 2 }), Origin::Agent);
    assert!(bold.to_string().contains("\"b\":true") || bold.to_string().contains("\"bold\":true"), "slide 2's body is bold: {bold}");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "reading by onSlide moved nothing");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_context_the_deck_lacks_is_invalid_params_and_says_where_ids_come_from() {
    let (mut kit, id, root) = open("refused");
    let saved = kit.context_get(&id).unwrap();
    for (given, says) in [
        (json!({ "on": [2] }), "give the slide as well"),
        (json!({ "part": { "slide": 7 } }), "there is no slide 7"),
        (json!({ "part": { "slide": 1 }, "on": [999_999] }), "slide 1 has no shape `999999`"),
    ] {
        let error = kit.context_set(&id, &params(given.clone())).unwrap_err();
        assert_eq!(error.code, kition_kit_server::ErrorReason::InvalidParams.code(), "{given}: {}", error.message);
        assert!(error.message.contains(says) || error.data.as_ref().is_some_and(|data| data.to_string().contains(says)), "{given}: {error:?}");
    }
    let error = kit.context_set(&id, &params(json!({ "time": 1.5 }))).unwrap_err();
    assert_eq!(error.code, kition_kit_server::ErrorReason::CommandDisabled.code(), "a member the kit cannot set: {error:?}");
    assert_eq!(kit.context_get(&id).unwrap(), saved);
    let _ = std::fs::remove_dir_all(root);
}

/// What the kit publishes for the agent: the editor state each command reads (`x-khp-reads`), the
/// parameter that names what it acts on where kition-sdk takes it (`x-khp-role`), and the view the
/// render command draws (`target.slide`, `target.slideId`).
#[test]
fn the_agents_commands_say_what_they_read_and_the_view_the_render_takes() {
    let (mut kit, id, root) = open("published");
    let commands = kit.agent_commands(Some(&id)).unwrap();
    let params = |command: &str| commands.iter().find(|listed| listed.id == command).map(|listed| listed.params.clone()).unwrap();
    assert_eq!(params("shape.fill")["x-khp-reads"], json!(["selection", "part"]));
    assert_eq!(params("insert.textBox")["x-khp-reads"], json!(["part"]));
    assert!(params("file.render").get("x-khp-reads").is_none());
    assert_eq!(params("edit.select")["properties"]["ids"]["x-khp-role"], "targets");
    let view = params("file.render")["properties"]["target"]["properties"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(view, ["slide", "slideId"]);
    let _ = std::fs::remove_dir_all(root);
}
