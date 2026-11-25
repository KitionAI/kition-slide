//! Receipts and the premise guard for the Slide kit, end to end (task card C6/C8-slide): the
//! headless `kition-slide` executable under the kits' MCP server's tools (`kition-kits`, the
//! library `kition-kits-mcp` serves, at the pinned kition-sdk revision), on a `.pptx` in a
//! workspace.
//!
//! The deck has one id space: `slide_inspect` lists each shape's tool id (`shape:<slideId>/<n>`)
//! and each slide's (`slide:<id>`), and receipts (`changes`), what others changed (`others`) and
//! refusals (`conflicts`) name objects by those ids, so the guard connects a step's `on` and
//! `at.part` to what changed. A change made outside is a second copy of the kit (the library)
//! editing the file and writing it back.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const ME: &str = "agent-me";
/// Another agent on the same server: its accepted writes are in the document's journal with what
/// they changed, as a change found in the file would be (see `a_change_made_in_the_file_...`).
const OTHER: &str = "agent-other";

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

/// Six slides, each with a text box `Slide <n>` and a second one `Note <n>`; slide 5's text box
/// has the number of slide 1's (as decks from other programs do: numbers are per slide).
fn deck() -> Vec<u8> {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let mut boxes = Vec::new();
    for n in 1..=6 {
        if n > 1 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        }
        let made = doc.execute("textBox.insert", json!({ "rect": [60, 60, 400, 80], "text": format!("Slide {n}") })).unwrap();
        boxes.push(made["id"].as_u64().unwrap());
        doc.execute("textBox.insert", json!({ "rect": [60, 300, 400, 80], "text": format!("Note {n}") })).unwrap();
    }
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    let fifth = std::sync::Arc::make_mut(&mut file.slides[4]);
    for shape in &mut fifth.shapes {
        if u64::from(shape.id.0) == boxes[4] {
            shape.id = kition_slide_model::ShapeId(u32::try_from(boxes[0]).unwrap());
        }
    }
    kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()
}

struct Fixture {
    kits: KitManager,
    options: CallOptions,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Deck.pptx"), deck()).unwrap();
        let kits = Self::server(workspace.path());
        Self { kits, options: CallOptions::default(), workspace }
    }

    /// A new server on `workspace` (a process of `kition-kits-mcp` is one of these).
    fn server(workspace: &Path) -> KitManager {
        KitManager::new(Some(workspace.to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))])
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join("Deck.pptx")
    }

    async fn call(&self, action: &str, input: Value) -> Result<Value, Value> {
        self.call_as(ME, action, input).await
    }

    async fn call_as(&self, agent: &str, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Deck.pptx");
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, self.options, agent).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("slide_{action} is no tool"),
        }
    }

    async fn ok(&self, action: &str, input: Value) -> Value {
        self.call(action, input).await.unwrap_or_else(|error| panic!("slide_{action}: {error}"))
    }

    /// `agent`'s call, which must succeed.
    async fn by(&self, agent: &str, action: &str, input: Value) -> Value {
        self.call_as(agent, action, input).await.unwrap_or_else(|error| panic!("{agent} slide_{action}: {error}"))
    }

    async fn refused(&self, action: &str, input: Value) -> Value {
        match self.call(action, input).await {
            Err(error) => error,
            Ok(answer) => panic!("slide_{action} was expected to be refused: {answer}"),
        }
    }

    /// Slide `n` (from 1) as the agent inspects it: its `part` and the answer's `revision`.
    async fn inspect(&self, n: u64) -> (Value, String) {
        let seen = self.ok("inspect", json!({ "target": { "slide": n } })).await;
        (seen["part"].clone(), seen["revision"].as_str().unwrap_or_else(|| panic!("a revision: {seen}")).to_owned())
    }

    /// Someone else edits the file: a second copy of the kit opens it, runs `steps` (commands and
    /// parameters, as a person runs them), and writes it back, an hour before the file's own time.
    fn edit_outside(&self, steps: &[(&str, Value)]) {
        let file = self.file();
        let mut doc = SlideDocument::open("Deck.pptx", &std::fs::read(&file).unwrap()).unwrap();
        for (command, params) in steps {
            doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("outside {command}: {error}"));
        }
        let at = std::fs::metadata(&file).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(&file, doc.save(Some("Deck.pptx")).unwrap()).unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(at).unwrap();
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

/// The shape of a listed slide whose text is `text`.
fn shape(part: &Value, text: &str) -> Value {
    part["shapes"].as_array().unwrap().iter().find(|shape| shape["text"] == text).cloned().unwrap_or_else(|| panic!("no `{text}` in {part}"))
}

fn tool_id(part: &Value, text: &str) -> String {
    shape(part, text)["toolId"].as_str().unwrap().to_owned()
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn mentions(set: &Value, id: &str) -> bool {
    ["created", "changed", "moved", "deleted"].iter().any(|list| ids(&set[*list]).iter().any(|named| named == id))
}

fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

/// C8-land-9s: equal shifts of shapes on different slides use the ids footprint form.
#[tokio::test(flavor = "multi_thread")]
async fn scattered_shape_shifts_have_an_ids_footprint() {
    for all in [false, true] {
        let f = Fixture::new();
        let mut targets = Vec::new();
        for n in 1..=6 {
            let (part, _) = f.inspect(n).await;
            targets.push(tool_id(&part, &format!("Note {n}")));
        }
        let steps: Vec<Value> = targets
            .iter()
            .map(|id| {
                json!({
                    "command": "shape.move", "params": { "ids": [id], "dx": 12, "dy": 7 }
                })
            })
            .collect();
        let ran = f.ok("run", json!({ "steps": steps, "changes": if all { json!("all") } else { json!(true) } })).await;
        let changes = &ran["changes"];
        if all {
            assert!(changes.get("footprint").is_none(), "{ran}");
            assert_eq!(ids(&changes["changed"]).len(), targets.len(), "{ran}");
        } else {
            let prints = changes["footprint"].as_array().unwrap_or_else(|| panic!("{ran}"));
            assert_eq!(prints.len(), 1, "{ran}");
            targets.sort();
            assert_eq!(prints[0]["ids"], json!(targets), "{ran}");
            let edits = prints[0]["edits"].as_array().unwrap();
            assert_eq!(edits.len(), 2, "{ran}");
            for (field, by) in [("xfrm.x", 12.0), ("xfrm.y", 7.0)] {
                assert!(edits.iter().any(|edit| edit["field"] == field && edit["by"].as_f64() == Some(by)), "{ran}");
            }
            assert!(ids(&changes["changed"]).is_empty(), "the footprint carries every change: {ran}");
        }
        println!("scattered shifts: changes all={all}, {} bytes", changes.to_string().len());
        f.shutdown().await;
    }
}

/// A refusal of a write whose premises others changed, naming `id` with `reason`.
fn assert_conflict<'a>(refused: &'a Value, id: &str, reason: &str) -> &'a Value {
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    assert_eq!(refused["error"]["retryable"], true, "{refused}");
    assert!(refused["revision"].is_string(), "{refused}");
    let found = refused["conflicts"].as_array().and_then(|conflicts| conflicts.iter().find(|conflict| conflict["id"] == id));
    let found = found.unwrap_or_else(|| panic!("no conflict on {id}: {refused}"));
    assert_eq!(found["reason"], reason, "{refused}");
    found
}

fn fill(id: &str, color: &str) -> Value {
    json!({ "command": "shape.fill", "on": [id], "params": { "color": color } })
}

// ---- a. Receipts name what a call changed by the ids inspect lists ----

#[tokio::test(flavor = "multi_thread")]
async fn a_receipt_names_the_shapes_and_slides_a_run_changed_by_the_ids_inspect_listed() {
    let f = Fixture::new();
    let (first, _) = f.inspect(1).await;
    let (fifth, _) = f.inspect(5).await;
    let target = tool_id(&fifth, "Slide 5");
    let neighbour = tool_id(&first, "Slide 1");
    assert_eq!(shape(&fifth, "Slide 5")["id"], shape(&first, "Slide 1")["id"], "the same number on slides 1 and 5");
    assert_ne!(target, neighbour);
    // A shape's fill: `changed` names exactly that shape, with before and after.
    let ran = f.ok("run", json!({ "steps": [fill(&target, "#C00000")] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), std::slice::from_ref(&target), "{ran}");
    let edit = &ran["changes"]["changed"][0]["edits"][0];
    assert_eq!((edit["field"].clone(), edit["before"].clone()), (json!("look.fill"), Value::Null), "{ran}");
    assert_eq!(edit["after"]["color"]["base"]["rgb"], json!({ "a": 255, "r": 192, "g": 0, "b": 0 }), "{ran}");
    assert!(!mentions(&ran["changes"], &neighbour), "slide 1's shape with the same number is not named: {ran}");
    // Its text: the shape, at the path to its text, before → after; no shape of slide 1.
    let ran = f.ok("run", json!({ "steps": [{ "command": "text.set", "params": { "id": target, "text": "Results" } }] })).await;
    assert_eq!(
        ran["changes"]["changed"],
        json!([{ "id": target, "type": "Shape", "fields": ["body.paragraphs.0.text"], "edits": [{ "field": "body.paragraphs.0.text", "before": "Slide 5", "after": "Results" }] }]),
        "{ran}"
    );
    assert!(!mentions(&ran["changes"], &neighbour), "{ran}");
    // A shape inserted: `created` names it by the id slide_inspect lists for it.
    let ran = f
        .ok(
            "run",
            json!({ "target": { "slide": 5 }, "steps": [{ "command": "textBox.insert", "params": { "rect": [500, 60, 200, 60], "text": "New" } }] }),
        )
        .await;
    let (fifth, _) = f.inspect(5).await;
    let made = tool_id(&fifth, "New");
    assert!(ids(&ran["changes"]["created"]).contains(&made), "{made}: {ran}");
    let created = ran["changes"]["created"].as_array().unwrap().iter().find(|object| object["id"] == made).unwrap();
    assert_eq!(created["at"]["parent"], fifth["toolId"], "created on slide 5: {ran}");
    // Deleted: `deleted` names it and where it was.
    let ran = f.ok("run", json!({ "steps": [{ "command": "shape.delete", "params": { "ids": [made] } }] })).await;
    let deleted: Vec<&Value> = ran["changes"]["deleted"].as_array().unwrap().iter().filter(|object| object["type"] == "Shape").collect();
    assert_eq!(deleted.iter().map(|object| object["id"].clone()).collect::<Vec<_>>(), [json!(made)], "{ran}");
    assert_eq!(deleted[0]["at"]["parent"], fifth["toolId"], "{ran}");
    // Slides reordered: `moved` names the slide that moved.
    let ran = f.ok("run", json!({ "steps": [{ "command": "slide.move", "params": { "from": 4, "to": 0 } }] })).await;
    assert_eq!(ids(&ran["changes"]["moved"]), [fifth["toolId"].as_str().unwrap().to_owned()], "{ran}");
    assert_eq!(
        (ran["changes"]["moved"][0]["from"]["index"].clone(), ran["changes"]["moved"][0]["to"]["index"].clone()),
        (json!(4), json!(0)),
        "{ran}"
    );
    f.shutdown().await;
}

// ---- b. Others' change to another object: reported, never refused ----

#[tokio::test(flavor = "multi_thread")]
async fn b_another_authors_edit_of_another_slide_is_reported_and_the_write_applies() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let (second, _) = f.inspect(2).await;
    let theirs = tool_id(&second, "Note 2");
    f.by(OTHER, "run", json!({ "steps": [fill(&theirs, "#0000C0")] })).await;
    let mine = tool_id(&fifth, "Note 5");
    let ran = f.ok("run", json!({ "basis": basis, "steps": [fill(&mine, "#00C000")] })).await;
    assert_eq!(ids(&ran["others"]["changed"]), [theirs], "{ran}");
    assert_eq!(ran["others"]["by"], json!([OTHER]), "{ran}");
    assert_eq!(ids(&ran["changes"]["changed"]), [mine], "{ran}");
    f.shutdown().await;
}

// ---- c. Others moved what a step addresses: refused as relocated or deleted ----

#[tokio::test(flavor = "multi_thread")]
async fn c_a_slide_inserted_or_deleted_before_the_target_by_another_author_refuses_the_write() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Note 5");
    let before = std::fs::read(f.file()).unwrap();
    // Another author inserts a slide before slide 5: the slide number no longer names it.
    f.by(OTHER, "run", json!({ "target": { "slide": 1 }, "steps": [{ "command": "slide.new", "params": { "layout": "blank" } }] })).await;
    let moved = std::fs::read(f.file()).unwrap();
    assert_ne!(moved, before);
    let refused = f.refused("run", json!({ "basis": basis, "steps": [{ "command": "shape.fill", "on": [target], "at": { "part": { "slide": 5 } }, "params": { "color": "#C00000" } }] })).await;
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    let conflict = &refused["conflicts"][0];
    assert_eq!((conflict["reason"].clone(), conflict["now"].clone()), (json!("relocated"), json!({ "slide": 6 })), "where it is now: {refused}");
    assert_eq!(conflict["id"], fifth["toolId"], "the slide the number named, by its tool id: {refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), moved, "nothing changed in the file");
    // By the shape's own id (it names its slide), the same write is right and applies.
    let ran = f.ok("run", json!({ "basis": basis, "steps": [fill(&target, "#C00000")] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), std::slice::from_ref(&target), "{ran}");
    // Another author deletes the slide: refused as deleted, by the slide and by the shape.
    let basis = revision(&ran);
    f.by(OTHER, "run", json!({ "steps": [{ "command": "slide.delete", "params": { "ids": [fifth["toolId"]] } }] })).await;
    let gone = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [{ "command": "shape.fill", "on": [target], "at": { "part": { "slideId": fifth["id"] } }, "params": { "color": "#00C000" } }] })).await;
    assert_conflict(&refused, &target, "deleted");
    assert_conflict(&refused, fifth["toolId"].as_str().unwrap(), "deleted");
    assert_eq!(std::fs::read(f.file()).unwrap(), gone, "nothing changed in the file");
    f.shutdown().await;
}

// ---- d. Others changed the step's target ----

#[tokio::test(flavor = "multi_thread")]
async fn d_another_authors_change_of_the_same_shape_refuses_a_command_step() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Slide 5");
    f.by(OTHER, "run", json!({ "steps": [fill(&target, "#0000C0")] })).await;
    let applied = f.ok("run", json!({ "basis": basis, "steps": [{ "command": "format.bold", "on": [target], "params": { "on": true } }] })).await;
    assert_eq!(ids(&applied["others"]["changed"]), std::slice::from_ref(&target), "the fill is independent of typography: {applied}");
    assert_eq!(ids(&applied["changes"]["changed"]), std::slice::from_ref(&target), "{applied}");
    let basis = revision(&applied);
    f.by(OTHER, "run", json!({ "steps": [{ "command": "text.set", "params": { "id": target, "text": "Their words" } }] })).await;
    let before = std::fs::read(f.file()).unwrap();
    let refused =
        f.refused("run", json!({ "basis": basis, "steps": [{ "command": "format.bold", "on": [target], "params": { "on": true } }] })).await;
    assert_conflict(&refused, &target, "changed");
    assert_eq!(ids(&refused["others"]["changed"]), [target], "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing changed in the file");
    f.shutdown().await;
}

/// Target text is independent of a fill write; a replacement fill is held. Text on another
/// slide remains independent and its receipt names that shape's owned body.
#[tokio::test(flavor = "multi_thread")]
async fn d_another_authors_text_edit_inside_the_target_refuses_and_inside_another_shape_is_reported() {
    let f = Fixture::new();
    let (first, _) = f.inspect(1).await;
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Slide 5");
    let theirs = tool_id(&first, "Slide 1");
    f.by(OTHER, "run", json!({ "steps": [{ "command": "text.set", "params": { "id": target, "text": "Their words" } }] })).await;
    let applied = f.ok("run", json!({ "basis": basis, "steps": [fill(&target, "#C00000")] })).await;
    assert_eq!(ids(&applied["others"]["changed"]), std::slice::from_ref(&target), "target text is independent of fill: {applied}");
    assert_eq!(ids(&applied["changes"]["changed"]), std::slice::from_ref(&target), "{applied}");
    let basis = revision(&applied);
    f.by(OTHER, "run", json!({ "steps": [fill(&target, "#0000C0")] })).await;
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [fill(&target, "#C00000")] })).await;
    assert_conflict(&refused, &target, "changed");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing changed in the file");
    let basis = revision(&refused);
    f.by(OTHER, "run", json!({ "steps": [{ "command": "text.set", "params": { "id": theirs, "text": "Elsewhere" } }] })).await;
    let ran = f.ok("run", json!({ "basis": basis, "steps": [fill(&target, "#C00000")] })).await;
    assert_eq!(
        ran["others"]["changed"],
        json!([{ "id": theirs, "type": "Shape", "fields": ["body.paragraphs.0.text"], "edits": [{ "field": "body.paragraphs.0.text", "before": "Slide 1", "after": "Elsewhere" }] }]),
        "{ran}"
    );
    assert_eq!(ids(&ran["changes"]["changed"]), [target], "{ran}");
    f.shutdown().await;
}

// ---- e. The agent's own edits never refuse it ----

#[tokio::test(flavor = "multi_thread")]
async fn e_the_agents_own_edits_and_its_own_slide_insert_never_refuse_it() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Note 5");
    f.ok("run", json!({ "steps": [fill(&target, "#0000C0")] })).await;
    f.ok("run", json!({ "target": { "slide": 1 }, "steps": [{ "command": "slide.new", "params": { "layout": "blank" } }] })).await;
    // The old basis, its own insert before slide 5, its own edit of the target: it applies, with
    // the slide where its own insert put it.
    let ran = f.ok("run", json!({ "basis": basis, "steps": [{ "command": "shape.fill", "on": [target], "at": { "part": { "slide": 6 } }, "params": { "color": "#C00000" } }] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), [target], "{ran}");
    for list in ["created", "changed", "moved", "deleted"] {
        assert!(ids(&ran["others"][list]).is_empty(), "nothing by others: {ran}");
    }
    f.shutdown().await;
}

// ---- f. --guard epoch and --guard off ----

#[tokio::test(flavor = "multi_thread")]
async fn f_the_epoch_guard_refuses_any_change_by_others_and_the_guard_off_refuses_none() {
    let mut f = Fixture::new();
    f.options = CallOptions { guard: Guard::Epoch, ..CallOptions::default() };
    let (fifth, basis) = f.inspect(5).await;
    let (second, _) = f.inspect(2).await;
    let theirs = tool_id(&second, "Note 2");
    f.by(OTHER, "run", json!({ "steps": [fill(&theirs, "#0000C0")] })).await;
    let mine = tool_id(&fifth, "Note 5");
    let refused = f.refused("run", json!({ "basis": basis, "steps": [fill(&mine, "#00C000")] })).await;
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    assert_eq!(ids(&refused["others"]["changed"]), std::slice::from_ref(&theirs), "{refused}");
    f.options = CallOptions { guard: Guard::Off, ..CallOptions::default() };
    f.by(OTHER, "run", json!({ "steps": [{ "command": "shape.delete", "params": { "ids": [mine] } }] })).await;
    let ran = f.ok("run", json!({ "basis": basis, "steps": [fill(&tool_id(&fifth, "Slide 5"), "#00C000")] })).await;
    assert!(ids(&ran["others"]["deleted"]).contains(&mine), "off still says what others changed: {ran}");
    assert!(ids(&ran["others"]["changed"]).contains(&theirs), "{ran}");
    f.shutdown().await;
}

// ---- g. Ids survive saving and a new server ----

#[tokio::test(flavor = "multi_thread")]
async fn g_ids_from_an_earlier_inspect_name_the_same_shapes_after_a_new_server_opens_the_saved_file() {
    let f = Fixture::new();
    let (fifth, _) = f.inspect(5).await;
    let target = tool_id(&fifth, "Slide 5");
    f.ok("run", json!({ "steps": [{ "command": "text.set", "params": { "id": target, "text": "Saved" } }, { "command": "slide.new", "at": { "part": { "slide": 6 } }, "params": { "layout": "blank" } }] })).await;
    let (made, _) = f.inspect(7).await;
    let Fixture { kits, workspace, .. } = f;
    kits.shutdown().await;
    let f = Fixture { kits: Fixture::server(workspace.path()), options: CallOptions::default(), workspace };
    let (again, _) = f.inspect(5).await;
    assert_eq!(again["toolId"], fifth["toolId"]);
    assert_eq!(shape(&again, "Saved")["toolId"], target.as_str(), "{again}");
    assert_eq!(f.inspect(7).await.0["toolId"], made["toolId"], "a slide the agent made keeps its id too");
    // The object model and the commands take the earlier ids.
    let read = f.ok("batch", json!({ "ops": [], "load": [{ "target": target, "fields": ["name"] }] })).await;
    assert_eq!(read["loaded"][0]["id"], target.as_str(), "{read}");
    let ran = f.ok("run", json!({ "steps": [fill(&target, "#C00000")] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), [target], "{ran}");
    f.shutdown().await;
}

// ---- A change made in the file itself ----

/// A change made in the file to another shape: a write with `basis` applies, and `others` names
/// the shape the other program changed, by the id slide_inspect lists (the scenarios are in
/// `outside.rs`), and nothing else: one object, under 1 KB.
#[tokio::test(flavor = "multi_thread")]
async fn a_change_made_in_the_file_to_another_shape_lets_a_write_with_basis_apply_and_names_that_shape_alone() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let (second, _) = f.inspect(2).await;
    let theirs = tool_id(&second, "Note 2");
    f.edit_outside(&[("shape.fill", json!({ "ids": [shape(&second, "Note 2")["id"]], "onSlide": 2, "color": "#0000C0" }))]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [fill(&tool_id(&fifth, "Note 5"), "#00C000")] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), [tool_id(&fifth, "Note 5")], "{ran}");
    assert_eq!(ran["others"]["by"], json!(["outside"]), "{ran}");
    let told = ran["others"]["changed"].as_array().unwrap().iter().find(|object| object["id"] == theirs.as_str()).unwrap_or_else(|| panic!("{ran}"));
    assert_eq!(told["edits"][0]["field"], "look.fill", "{ran}");
    assert_eq!(told["edits"][0]["after"]["color"]["base"]["rgb"], json!({ "a": 255, "r": 0, "g": 0, "b": 192 }), "{ran}");
    assert_eq!(ids(&ran["others"]["changed"]), [theirs], "{ran}");
    for list in ["created", "moved", "deleted"] {
        assert!(ids(&ran["others"][list]).is_empty(), "{ran}");
    }
    assert!(ran["others"].get("truncated").is_none() && ran["others"].get("unnamed").is_none(), "{ran}");
    assert!(ran["others"].to_string().len() < 1024, "{ran}");
    assert!(!mentions(&ran["others"], &tool_id(&fifth, "Note 5")), "{ran}");
    f.shutdown().await;
}
