//! The premise guard and receipts when the file itself changes (task cards C8-slide-2 to C8-slide-4): a
//! colleague's program, an engine of this kit that shares nothing with the server under test,
//! opens the deck, edits it and writes it back, so the server sees only a changed file. The agent
//! works through the kits' MCP server's tools (`kition_kits::tools::call_as`, the library
//! `kition-kits-mcp` serves) on the `kition-slide` executable, holding the ids and the `revision`
//! of an inspect made before the colleague's edit.
//!
//! The deck has ten slides, each with a `Title <n>` and a `Body <n>` text box; slide 9's title has
//! the number of slide 2's (numbers are per slide, as in decks from other programs), and slide 10
//! holds a picture. Scenarios `j` change the file as a program other than this kit would, through
//! the kit's file reader and writer or the package's zip alone.
//!
//! Since kition-sdk 2b7231e the two reads of the file are paired object by object (this kit's root
//! keeps its id on every read and no longer stops the rest from pairing), so `others` names what
//! the colleague changed and nothing else. At a0758b3 every object a step's parameters name is a
//! premise (`n`), and a slide named by its index is a place (`o`, C8-slide-5). Tests whose names say
//! `wrongly` pin an outcome that is wrong today, so that fixing it fails them (none now).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const ME: &str = "agent-me";
const SLIDES: u64 = 10;
/// One red pixel and one blue pixel, as PNG.
const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";
const BLUE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYDgBAADMAMkncsFaAAAAAElFTkSuQmCC";

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

fn deck() -> Vec<u8> {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let mut titles = Vec::new();
    for n in 1..=SLIDES {
        if n > 1 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        }
        let made = doc.execute("insert.textBox", json!({ "rect": [60, 60, 400, 80], "text": format!("Title {n}") })).unwrap();
        titles.push(made["id"].as_u64().unwrap());
        doc.execute("insert.textBox", json!({ "rect": [60, 300, 400, 80], "text": format!("Body {n}") })).unwrap();
    }
    doc.execute("insert.picture", json!({ "name": "red.png", "data": RED })).unwrap();
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    let ninth = std::sync::Arc::make_mut(&mut file.slides[8]);
    for shape in &mut ninth.shapes {
        if u64::from(shape.id.0) == titles[8] {
            shape.id = kition_slide_model::ShapeId(u32::try_from(titles[1]).unwrap());
        }
    }
    kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()
}

/// A colleague's program: this kit's engine, its own instance, opening the file from disk, running
/// `steps` (commands and parameters) and saving the file back, with a modification time an hour
/// earlier so that only the content tells the server the file changed.
fn colleague(file: &Path, steps: &[(&str, Value)]) {
    let mut doc = SlideDocument::open("Deck.pptx", &std::fs::read(file).unwrap()).unwrap();
    for (command, params) in steps {
        doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("colleague's {command}: {error}"));
    }
    let at = std::fs::metadata(file).unwrap().modified().unwrap() - Duration::from_secs(3600);
    std::fs::write(file, doc.save(Some("Deck.pptx")).unwrap()).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(at).unwrap();
}

/// The texts of slide `n` (from 1) of the file on disk, as a new engine reads them.
fn texts_on_disk(file: &Path, n: u64) -> Vec<String> {
    let mut doc = SlideDocument::open("Deck.pptx", &std::fs::read(file).unwrap()).unwrap();
    let seen = doc.execute("slide.inspect", json!({ "index": n - 1 })).unwrap();
    seen["shapes"].as_array().unwrap().iter().filter_map(|shape| shape["text"].as_str().map(str::to_owned)).collect()
}

struct Fixture {
    kits: KitManager,
    options: CallOptions,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self::with(CallOptions::default())
    }

    fn with(options: CallOptions) -> Self {
        Self::with_bytes(options, &deck())
    }

    fn with_bytes(options: CallOptions, bytes: &[u8]) -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Deck.pptx"), bytes).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, options, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join("Deck.pptx")
    }

    fn colleague(&self, steps: &[(&str, Value)]) {
        colleague(&self.file(), steps);
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Deck.pptx");
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, self.options, ME).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("slide_{action} is no tool"),
        }
    }

    async fn ok(&self, action: &str, input: Value) -> Value {
        self.call(action, input).await.unwrap_or_else(|error| panic!("slide_{action}: {error}"))
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
        (seen["part"].clone(), revision(&seen))
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

fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// The entry of `list` naming `id`.
fn entry<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array().and_then(|items| items.iter().find(|item| item["id"] == id)).unwrap_or_else(|| panic!("no {id} in {list}"))
}

/// A text edit as receipts and `others` name it: under the shape, at the path to its text.
fn text_edit(id: &str, before: &str, after: &str) -> Value {
    json!({
        "id": id,
        "type": "Shape",
        "fields": ["body.paragraphs.0.text"],
        "edits": [{ "field": "body.paragraphs.0.text", "before": before, "after": after }],
    })
}

fn set_text(id: &str, text: &str) -> Value {
    json!({ "command": "text.set", "params": { "id": id, "text": text } })
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

/// `others` after a colleague's edit of shape texts alone: exactly those edits, by the shapes' tool
/// ids, by `outside`, nothing created, moved or deleted, nothing counted in `unnamed`, nothing cut,
/// under 1 KB. Answers its size in bytes.
fn assert_only_texts(others: &Value, edits: &[Value]) -> usize {
    assert_eq!(
        others,
        &json!({ "by": ["outside"], "created": [], "changed": edits, "moved": [], "deleted": [] }),
        "others names the colleague's edits alone: {others}"
    );
    let size = others.to_string().len();
    assert!(size < 1024, "{size} bytes: {others}");
    size
}

/// What `others` says of the shape `id`.
fn told<'a>(others: &'a Value, id: &str) -> &'a Value {
    entry(&others["changed"], id)
}

fn by_number(slide: u64, number: &Value, text: &str) -> Value {
    json!({ "command": "text.set", "at": { "part": { "slide": slide } }, "params": { "id": number, "text": text } })
}

/// The picture on slide 10, by its tool id.
fn picture(tenth: &Value) -> String {
    tenth["shapes"].as_array().unwrap().iter().find(|shape| shape["text"].is_null()).unwrap()["toolId"].as_str().unwrap().to_owned()
}

/// Rewrites the file through this kit's file reader and writer alone (no session, no command), as
/// another program saving the deck would, an hour before the file's own time.
fn rewrite(file: &Path, change: impl FnOnce(&mut kition_slide_model::SlideFile)) {
    let mut deck = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &std::fs::read(file).unwrap()).unwrap();
    change(&mut deck);
    let at = std::fs::metadata(file).unwrap().modified().unwrap() - Duration::from_secs(3600);
    std::fs::write(file, kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(at).unwrap();
}

// ---- a. A colleague's text edit on another slide ----

/// The write applies, and `others` names the colleague's shape alone by the id slide_inspect lists,
/// at the path to its text, before and after, in under 1 KB.
#[tokio::test(flavor = "multi_thread")]
async fn a_a_colleagues_text_edit_on_another_slide_applies_and_others_names_that_shape_alone() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let (ninth, _) = f.inspect(9).await;
    let theirs = tool_id(&ninth, "Body 9");
    let mine = tool_id(&second, "Body 2");
    f.colleague(&[("text.set", json!({ "id": theirs, "text": "Edited by a colleague" }))]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Body 2", "Mine")]), "{ran}");
    let size = assert_only_texts(&ran["others"], &[text_edit(&theirs, "Body 9", "Edited by a colleague")]);
    println!("others for one unrelated text edit: {size} bytes");
    assert!(texts_on_disk(&f.file(), 2).contains(&"Mine".to_owned()));
    assert!(texts_on_disk(&f.file(), 9).contains(&"Edited by a colleague".to_owned()));
    f.shutdown().await;
}

// ---- b. A colleague's text edit of the agent's target ----

/// Refused as `changed` on the target, nothing written, `others` telling the colleague's text; with
/// the refusal's revision the agent's edit applies, its receipt saying it replaced that text.
#[tokio::test(flavor = "multi_thread")]
async fn b_a_colleagues_text_edit_of_the_target_is_refused_as_changed_and_the_retry_applies() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let mine = tool_id(&second, "Body 2");
    f.colleague(&[("text.set", json!({ "id": mine, "text": "Colleague's" }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine")] })).await;
    assert_conflict(&refused, &mine, "changed");
    assert_eq!(refused["conflicts"].as_array().unwrap().len(), 1, "{refused}");
    assert_only_texts(&refused["others"], &[text_edit(&mine, "Body 2", "Colleague's")]);
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    let ran = f.ok("run", json!({ "basis": revision(&refused), "steps": [set_text(&mine, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Colleague's", "Mine")]), "{ran}");
    assert!(ran.get("others").is_none(), "{ran}");
    f.shutdown().await;
}

// ---- c. A slide inserted or moved before the target's slide ----

/// (i) By the shape's composite id the write applies to that shape, now on slide 6. (ii) By slide
/// number and the shape's per-slide number it is refused as `relocated`, naming the slide by its
/// tool id with its new number, and nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn c_a_slide_inserted_before_the_target_lets_its_composite_id_apply_and_refuses_its_slide_number_as_relocated() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Body 5");
    let number = shape(&fifth, "Body 5")["id"].clone();
    f.colleague(&[("slide.new", json!({ "layout": "blank", "at": 0 }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [by_number(5, &number, "Mine")] })).await;
    let conflict = assert_conflict(&refused, fifth["toolId"].as_str().unwrap(), "relocated");
    assert_eq!(conflict["now"], json!({ "slide": 6 }), "{refused}");
    let made = ids(&refused["others"]["created"]);
    assert!(made.len() == 1 && made[0].starts_with("slide:"), "the new slide alone is named: {refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&target, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&target, "Body 5", "Mine")]), "{ran}");
    assert!(texts_on_disk(&f.file(), 6).contains(&"Mine".to_owned()), "the shape, now on slide 6");
    assert!(!texts_on_disk(&f.file(), 5).contains(&"Mine".to_owned()));
    f.shutdown().await;
}

/// The colleague moves slide 9 to second place; slide 9's title has the number of slide 2's. A step
/// on `{slide: 2}` with that number is refused as `relocated` (slide 2 is now slide 3), so it does
/// not edit slide 9's title; nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn c_a_slide_moved_before_the_target_refuses_a_slide_number_as_relocated() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let number = shape(&second, "Title 2")["id"].clone();
    f.colleague(&[("slide.move", json!({ "from": 8, "to": 1 }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [by_number(2, &number, "Mine")] })).await;
    let conflict = assert_conflict(&refused, second["toolId"].as_str().unwrap(), "relocated");
    assert_eq!(conflict["now"], json!({ "slide": 3 }), "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    assert!(texts_on_disk(&f.file(), 2).contains(&"Title 9".to_owned()));
    f.shutdown().await;
}

// ---- d. The target's slide deleted ----

/// Refused as `deleted`, by the shape's id and by the slide's number; nothing written.
#[tokio::test(flavor = "multi_thread")]
async fn d_a_colleague_deleting_the_targets_slide_is_refused_as_deleted_by_shape_id_and_by_slide_number() {
    let f = Fixture::new();
    let (fifth, basis) = f.inspect(5).await;
    let target = tool_id(&fifth, "Body 5");
    f.colleague(&[("slide.delete", json!({ "ids": [fifth["toolId"]] }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [set_text(&target, "Mine")] })).await;
    assert_conflict(&refused, &target, "deleted");
    let refused = f.refused("run", json!({ "basis": basis, "steps": [by_number(5, &shape(&fifth, "Body 5")["id"], "Mine")] })).await;
    assert_conflict(&refused, fifth["toolId"].as_str().unwrap(), "deleted");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    f.shutdown().await;
}

// ---- e. The same per-slide number on another slide ----

/// The colleague edits slide 9's title, which has the number of slide 2's. The agent's write on
/// slide 2's title applies, by its tool id and by slide number and shape number, and `others`
/// names slide 9's title, not slide 2's.
#[tokio::test(flavor = "multi_thread")]
async fn e_a_colleagues_edit_of_the_same_number_on_another_slide_lets_the_write_apply() {
    for by_tool_id in [true, false] {
        let f = Fixture::new();
        let (second, basis) = f.inspect(2).await;
        let (ninth, _) = f.inspect(9).await;
        let mine = tool_id(&second, "Title 2");
        let theirs = tool_id(&ninth, "Title 9");
        assert_eq!(shape(&second, "Title 2")["id"], shape(&ninth, "Title 9")["id"], "one number on slides 2 and 9");
        f.colleague(&[("text.set", json!({ "id": theirs, "text": "Their title" }))]);
        let step = if by_tool_id { set_text(&mine, "My title") } else { by_number(2, &shape(&second, "Title 2")["id"], "My title") };
        let ran = f.ok("run", json!({ "basis": basis, "steps": [step] })).await;
        assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Title 2", "My title")]), "{ran}");
        assert_eq!(told(&ran["others"], &theirs), &text_edit(&theirs, "Title 9", "Their title"), "{ran}");
        assert!(!ids(&ran["others"]["changed"]).contains(&mine), "{ran}");
        assert!(texts_on_disk(&f.file(), 9).contains(&"Their title".to_owned()));
        f.shutdown().await;
    }
}

// ---- f. --guard epoch and --guard off ----

/// Epoch: a's write is refused (`changed`, no id). Off: a's write applies and `others` tells the
/// colleague's edit, and b's write applies over the colleague's text, its receipt giving the
/// colleague's text as `before`; under off `others` names the colleague's edit alone, as in a.
#[tokio::test(flavor = "multi_thread")]
async fn f_epoch_refuses_after_a_file_change_and_off_applies_and_overwrites_telling_the_colleagues_edit_alone() {
    let f = Fixture::with(CallOptions { guard: Guard::Epoch, ..CallOptions::default() });
    let (second, basis) = f.inspect(2).await;
    let (ninth, _) = f.inspect(9).await;
    f.colleague(&[("text.set", json!({ "id": tool_id(&ninth, "Body 9"), "text": "Edited by a colleague" }))]);
    let refused = f.refused("run", json!({ "basis": basis, "steps": [set_text(&tool_id(&second, "Body 2"), "Mine")] })).await;
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    assert_eq!(refused["conflicts"], json!([{ "reason": "changed" }]), "{refused}");
    f.shutdown().await;

    let f = Fixture::with(CallOptions { guard: Guard::Off, ..CallOptions::default() });
    let (second, basis) = f.inspect(2).await;
    let (ninth, _) = f.inspect(9).await;
    let mine = tool_id(&second, "Body 2");
    let theirs = tool_id(&ninth, "Body 9");
    f.colleague(&[("text.set", json!({ "id": theirs, "text": "Edited by a colleague" }))]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine")] })).await;
    assert_only_texts(&ran["others"], &[text_edit(&theirs, "Body 9", "Edited by a colleague")]);
    let basis = revision(&ran);
    f.colleague(&[("text.set", json!({ "id": mine, "text": "Colleague's" }))]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine again")] })).await;
    assert_only_texts(&ran["others"], &[text_edit(&mine, "Mine", "Colleague's")]);
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Colleague's", "Mine again")]), "overwritten: {ran}");
    f.shutdown().await;
}

// ---- g. No basis ----

/// Without `basis`, a's and b's writes are refused once (`failed`, `changedOutside`, retryable),
/// and the same write then applies; b's overwrites the colleague's text.
#[tokio::test(flavor = "multi_thread")]
async fn g_without_basis_a_file_change_refuses_one_write_then_lets_it_through() {
    let f = Fixture::new();
    let (second, _) = f.inspect(2).await;
    let (ninth, _) = f.inspect(9).await;
    let mine = tool_id(&second, "Body 2");
    let once = |refused: &Value| {
        assert_eq!(refused["error"]["code"], "failed", "{refused}");
        assert_eq!(refused["error"]["retryable"], true, "{refused}");
        assert_eq!(refused["changedOutside"]["path"], "Deck.pptx", "{refused}");
        assert!(refused.get("conflicts").is_none(), "{refused}");
    };
    f.colleague(&[("text.set", json!({ "id": tool_id(&ninth, "Body 9"), "text": "Edited by a colleague" }))]);
    once(&f.refused("run", json!({ "steps": [set_text(&mine, "Mine")] })).await);
    let ran = f.ok("run", json!({ "steps": [set_text(&mine, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Body 2", "Mine")]), "{ran}");
    assert!(ran.get("others").is_none(), "{ran}");
    f.colleague(&[("text.set", json!({ "id": mine, "text": "Colleague's" }))]);
    once(&f.refused("run", json!({ "steps": [set_text(&mine, "Mine again")] })).await);
    let ran = f.ok("run", json!({ "steps": [set_text(&mine, "Mine again")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Colleague's", "Mine again")]), "{ran}");
    f.shutdown().await;
}

// ---- h. A receipt of a text edit ----

/// A text edit is named by the shape slide_inspect lists, at the path to its text, with before and
/// after; a slide's first notes by the slide, as the paragraph created at `notesBody.paragraphs.0`.
#[tokio::test(flavor = "multi_thread")]
async fn h_a_receipt_names_a_text_edit_by_its_shape_and_a_notes_edit_by_its_slide() {
    let f = Fixture::new();
    let (fifth, _) = f.inspect(5).await;
    let target = tool_id(&fifth, "Title 5");
    let ran = f.ok("run", json!({ "steps": [set_text(&target, "Results")] })).await;
    assert_eq!(ran["changes"], json!({ "created": [], "changed": [text_edit(&target, "Title 5", "Results")], "moved": [], "deleted": [] }), "{ran}");
    let ran = f.ok("run", json!({ "target": { "slide": 5 }, "steps": [{ "command": "slide.notes", "params": { "text": "Speak slowly" } }] })).await;
    let notes = entry(&ran["changes"]["changed"], fifth["toolId"].as_str().unwrap());
    assert_eq!(notes["type"], "Slide", "{ran}");
    let made = notes["edits"].as_array().unwrap().iter().find(|edit| edit["field"] == "notesBody.paragraphs.0").unwrap_or_else(|| panic!("{ran}"));
    assert_eq!((made["before"].clone(), made["after"]["text"].clone()), (Value::Null, json!("Speak slowly")), "{ran}");
    f.shutdown().await;
}

// ---- i. A slide's layout and a picture's media ----

/// The colleague changes slide 3's layout, the picture on slide 10's image, or (for comparison) a
/// text on slide 9. After each, with the basis from before it, a write on slide 4 (which the
/// colleague did not touch, by `at.part`: `slide.hide` takes no `on`), a write on the picture and
/// a text write on slide 10's body apply. `others` names
/// slide 3 with its `layout` changed after the layout change, and counts the image in `unnamed`
/// (`{Media: {changed: 1}}`) after the media change: no tool id names a media object.
#[tokio::test(flavor = "multi_thread")]
async fn i_a_layout_or_media_change_lets_writes_on_untouched_slides_and_the_picture_apply() {
    for change in ["layout", "media", "text"] {
        let f = Fixture::new();
        let (third, basis) = f.inspect(3).await;
        let (ninth, _) = f.inspect(9).await;
        let (tenth, _) = f.inspect(10).await;
        let picture = picture(&tenth);
        match change {
            "layout" => f.colleague(&[("slide.layout", json!({ "layout": "titleOnly", "index": 2 }))]),
            "media" => f.colleague(&[("picture.change", json!({ "ids": [picture], "name": "blue.png", "data": BLUE }))]),
            _ => f.colleague(&[("text.set", json!({ "id": tool_id(&ninth, "Body 9"), "text": "Elsewhere" }))]),
        }
        let ran = f
            .ok(
                "run",
                json!({ "basis": basis, "steps": [{ "command": "slide.hide", "at": { "part": { "slide": 4 } }, "params": { "hidden": true } }] }),
            )
            .await;
        let others = &ran["others"];
        println!("{change}: others {} bytes", others.to_string().len());
        match change {
            "layout" => {
                let slide = told(others, third["toolId"].as_str().unwrap());
                assert!(slide["fields"].as_array().unwrap().contains(&json!("layout")), "{ran}");
                assert!(others.get("truncated").is_none(), "{ran}");
            }
            "media" => {
                assert_eq!(others["unnamed"], json!({ "Media": { "changed": 1 } }), "{ran}");
                assert!(ids(&others["changed"]).is_empty() && ids(&others["created"]).is_empty() && ids(&others["deleted"]).is_empty(), "{ran}");
            }
            _ => {
                assert_only_texts(others, &[text_edit(&tool_id(&ninth, "Body 9"), "Body 9", "Elsewhere")]);
            }
        }
        // The picture whose image the colleague replaced is not refused either: the media object is
        // another object, shared, and the picture's own fields did not change.
        let ran = f.ok("run", json!({ "basis": basis, "steps": [{ "command": "picture.reset", "on": [picture], "params": {} }] })).await;
        assert!(ran["changes"].is_object(), "{change}: {ran}");
        if change == "layout" {
            // The part's own fields are premises too (SDK C8-fix-6): the colleague changed
            // slide 3's layout, so a step on that slide is refused, by its tool id.
            let before = std::fs::read(f.file()).unwrap();
            let refused = f
                .refused(
                    "run",
                    json!({ "basis": basis, "steps": [{ "command": "slide.hide", "at": { "part": { "slide": 3 } }, "params": { "hidden": true } }] }),
                )
                .await;
            assert_conflict(&refused, third["toolId"].as_str().unwrap(), "changed");
            assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
        }
        let body = tool_id(&tenth, "Body 10");
        let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&body, "Applies")] })).await;
        assert_eq!(ran["changes"]["changed"], json!([text_edit(&body, "Body 10", "Applies")]), "{change}: {ran}");
        f.shutdown().await;
    }
}

// ---- j. Another program changes the file ----

/// The package written again with the same parts, in another order and without compression (as
/// another program saves a deck it did not change) is no change: with `basis` the write applies
/// without `others`, and without `basis` it is not refused and carries no notice.
#[tokio::test(flavor = "multi_thread")]
async fn j_a_package_saved_again_unchanged_is_no_change() {
    let repack = |file: &Path| {
        let bytes = std::fs::read(file).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut parts = Vec::new();
        for at in 0..archive.len() {
            let mut part = archive.by_index(at).unwrap();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut part, &mut data).unwrap();
            parts.push((part.name().to_owned(), data));
        }
        // `[Content_Types].xml` first, as packages put it; the rest reversed.
        parts[1..].reverse();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in &parts {
            out.start_file(name.as_str(), stored).unwrap();
            std::io::Write::write_all(&mut out, data).unwrap();
        }
        let repacked = out.finish().unwrap().into_inner();
        assert_ne!(repacked, std::fs::read(file).unwrap());
        assert_eq!(parts[0].0, "[Content_Types].xml");
        let at = std::fs::metadata(file).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(file, repacked).unwrap();
        std::fs::File::options().write(true).open(file).unwrap().set_modified(at).unwrap();
    };
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let mine = tool_id(&second, "Body 2");
    repack(&f.file());
    assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2", "Body 2"]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Body 2", "Mine")]), "{ran}");
    assert!(ran.get("others").is_none(), "{ran}");
    repack(&f.file());
    let ran = f.ok("run", json!({ "steps": [set_text(&mine, "Mine again")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&mine, "Mine", "Mine again")]), "{ran}");
    assert!(ran.get("changedOutside").is_none() && ran.get("others").is_none(), "{ran}");
    f.shutdown().await;
}

/// Another program numbers slide 2's shapes again: the title and the body swap numbers, content
/// unchanged. A shape's tool id is its persistent identity (C8-slide-8), not its number: `others`
/// tells the two new numbers (`address`, a position) and no text as changed, and a write on the
/// body by the id the agent holds writes into the body. Its number, the old positional address,
/// is refused as `relocated` (bindings/headless/tests/persistent_ids.rs).
/// Case: `slide.agent.ids/outside-renumbered-shapes`.
#[tokio::test(flavor = "multi_thread")]
async fn j_shapes_renumbered_by_another_program_keep_their_ids_and_a_write_lands_on_the_same_shape() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let (title, body) = (tool_id(&second, "Title 2"), tool_id(&second, "Body 2"));
    rewrite(&f.file(), |deck| {
        let slide = std::sync::Arc::make_mut(&mut deck.slides[1]);
        let (a, b) = (slide.shapes[0].id, slide.shapes[1].id);
        slide.shapes[0].id = b;
        slide.shapes[1].id = a;
    });
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&body, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&body, "Body 2", "Mine")]), "{ran}");
    let others = &ran["others"];
    let mut renumbered = ids(&others["changed"]);
    renumbered.sort();
    let mut both = vec![title.clone(), body.clone()];
    both.sort();
    assert_eq!(renumbered, both, "{ran}");
    for change in others["changed"].as_array().unwrap() {
        assert_eq!(change["fields"], json!(["address.number"]), "only the numbers, no text told as changed: {ran}");
    }
    for list in ["created", "moved", "deleted"] {
        assert!(ids(&others[list]).is_empty(), "{ran}");
    }
    assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2", "Mine"]);
    let (now, _) = f.inspect(2).await;
    assert_eq!(tool_id(&now, "Mine"), body, "the body keeps its id");
    assert_eq!(tool_id(&now, "Title 2"), title, "the title keeps its id");
    f.shutdown().await;
}

/// Another program duplicates slide 2 (a new slide id, the same shape numbers) after it. A write
/// on slide 2's body by its tool id applies to slide 2 alone, and `others` names the new slide; one
/// by slide number on what was slide 3 is refused as `relocated`.
#[tokio::test(flavor = "multi_thread")]
async fn j_a_slide_duplicated_by_another_program_relocates_slide_numbers_and_is_named_in_others() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    let (third, _) = f.inspect(3).await;
    let body = tool_id(&second, "Body 2");
    rewrite(&f.file(), |deck| {
        let mut copy = (*deck.slides[1]).clone();
        copy.id = kition_slide_model::SlideId(deck.slides.iter().map(|slide| slide.id.0).max().unwrap() + 1);
        deck.slides.insert(2, std::sync::Arc::new(copy));
    });
    let refused = f.refused("run", json!({ "basis": basis, "steps": [by_number(3, &shape(&third, "Body 3")["id"], "Mine")] })).await;
    let conflict = assert_conflict(&refused, third["toolId"].as_str().unwrap(), "relocated");
    assert_eq!(conflict["now"], json!({ "slide": 4 }), "{refused}");
    // The copy, as slide_inspect lists it now (its tool id is the deck's, not the number given).
    let made = f.inspect(3).await.0["toolId"].as_str().unwrap().to_owned();
    assert!(made != second["toolId"] && made != third["toolId"], "{made}");
    assert!(ids(&refused["others"]["created"]).contains(&made), "{refused}");
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&body, "Mine")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&body, "Body 2", "Mine")]), "{ran}");
    assert!(ids(&ran["others"]["created"]).contains(&made), "{ran}");
    assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2", "Mine"]);
    assert_eq!(texts_on_disk(&f.file(), 3), ["Title 2", "Body 2"]);
    f.shutdown().await;
}

// ---- k. A table cell's text ----

/// The colleague edits one cell's text. A step on the table's shape is refused as `changed`, and a
/// step on another shape applies; `others` names one edit under the table's shape, through its
/// owning `cells` map, at `table.cells.<row>:<col>.body.paragraphs.0.text`, `b` → `B!`.
#[tokio::test(flavor = "multi_thread")]
async fn k_a_colleagues_cell_edit_is_one_edit_under_the_table_and_refuses_a_step_on_the_table() {
    let f = Fixture::new();
    let table = json!({ "rows": 2, "cols": 2, "rect": [500, 60, 300, 120], "data": [["a", "b"], ["c", "d"]] });
    f.ok("run", json!({ "target": { "slide": 4 }, "steps": [{ "command": "insert.table", "params": table }] })).await;
    let (fourth, basis) = f.inspect(4).await;
    let table = fourth["shapes"].as_array().unwrap().iter().find(|shape| shape["kind"] == "table").unwrap()["toolId"].as_str().unwrap().to_owned();
    f.colleague(&[("text.set", json!({ "id": table, "cell": [0, 1], "text": "B!" }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f
        .refused("run", json!({ "basis": basis, "steps": [{ "command": "table.cellFill", "on": [table], "params": { "color": "#C00000" } }] }))
        .await;
    assert_conflict(&refused, &table, "changed");
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    let others = &refused["others"];
    assert_eq!(ids(&others["changed"]), std::slice::from_ref(&table), "the table alone: {refused}");
    let told = told(others, &table);
    let field = told["fields"][0].as_str().unwrap_or_else(|| panic!("{refused}"));
    let key = field.strip_prefix("table.cells.").and_then(|rest| rest.strip_suffix(".body.paragraphs.0.text"));
    assert!(key.is_some_and(|key| key.split(':').count() == 2), "{field}");
    assert_eq!(told["fields"], json!([field]), "{refused}");
    assert_eq!(told["edits"], json!([{ "field": field, "before": "b", "after": "B!" }]), "{refused}");
    for list in ["created", "moved", "deleted"] {
        assert!(ids(&others[list]).is_empty(), "{refused}");
    }
    println!("others for one cell edit: {} bytes", others.to_string().len());
    let other = tool_id(&fourth, "Body 4");
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&other, "Other")] })).await;
    assert_eq!(ran["changes"]["changed"], json!([text_edit(&other, "Body 4", "Other")]), "{ran}");
    f.shutdown().await;
}

// ---- l. How often an unrelated file change refuses a right write ----

/// Fifty rounds on the ten-slide deck: the colleague edits the text of a shape picked at random
/// (a fixed seed) among the nineteen other than the agent's target, then the agent writes its
/// target (slide 2's body) with the revision of its last answer. Under the premise guard no write
/// is refused; under `--guard epoch` every one is (it refuses any change by others). Prints both
/// counts: the false-refusal rate an experiment reports.
#[tokio::test(flavor = "multi_thread")]
async fn l_fifty_unrelated_file_changes_refuse_no_write_under_premise_and_every_write_under_epoch() {
    const ROUNDS: u64 = 50;
    let mut counts = Vec::new();
    for (guard, name) in [(Guard::Premise, "premise"), (Guard::Epoch, "epoch")] {
        let f = Fixture::with(CallOptions { guard, ..CallOptions::default() });
        let mut shapes = Vec::new();
        let mut basis = String::new();
        for n in 1..=SLIDES {
            let (part, seen) = f.inspect(n).await;
            shapes.push(tool_id(&part, &format!("Title {n}")));
            shapes.push(tool_id(&part, &format!("Body {n}")));
            basis = seen;
        }
        // Titles and bodies in slide order: slide 2's body is the fourth.
        let mine = shapes.remove(3);
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut refused = 0;
        for round in 0..ROUNDS {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let theirs = &shapes[usize::try_from(seed % 19).unwrap()];
            f.colleague(&[("text.set", json!({ "id": theirs, "text": format!("Colleague {round}") }))]);
            match f.call("run", json!({ "basis": basis, "steps": [set_text(&mine, &format!("Mine {round}"))] })).await {
                Ok(ran) => {
                    assert_eq!(ids(&ran["others"]["changed"]), std::slice::from_ref(theirs), "{ran}");
                    basis = revision(&ran);
                }
                Err(error) => {
                    assert_eq!(error["error"]["code"], "conflict", "{error}");
                    refused += 1;
                    basis = revision(&error);
                }
            }
        }
        println!("--guard {name}: {refused} of {ROUNDS} writes refused after an unrelated file change");
        counts.push(refused);
        f.shutdown().await;
    }
    assert_eq!(counts, [0, ROUNDS], "premise refuses none, epoch every one");
}

// ---- m. The agent's own write between two file changes ----

/// The colleague edits X; the agent writes Y with its basis (applies, told about X); the colleague
/// then edits Z, the agent's next target; the agent writes Z with the revision of its last answer:
/// refused as `changed` on Z, so the chain of revisions keeps the second change.
#[tokio::test(flavor = "multi_thread")]
async fn m_the_agents_own_write_between_two_file_changes_keeps_the_second_change() {
    let f = Fixture::new();
    let (second, _) = f.inspect(2).await;
    let (third, _) = f.inspect(3).await;
    let (ninth, basis) = f.inspect(9).await;
    let (x, y, z) = (tool_id(&ninth, "Body 9"), tool_id(&second, "Body 2"), tool_id(&third, "Body 3"));
    f.colleague(&[("text.set", json!({ "id": x, "text": "X by a colleague" }))]);
    let ran = f.ok("run", json!({ "basis": basis, "steps": [set_text(&y, "Y by me")] })).await;
    assert_only_texts(&ran["others"], &[text_edit(&x, "Body 9", "X by a colleague")]);
    f.colleague(&[("text.set", json!({ "id": z, "text": "Z by a colleague" }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f.refused("run", json!({ "basis": revision(&ran), "steps": [set_text(&z, "Z by me")] })).await;
    assert_conflict(&refused, &z, "changed");
    assert_only_texts(&refused["others"], &[text_edit(&z, "Body 3", "Z by a colleague")]);
    assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
    f.shutdown().await;
}

// ---- n. A target named by the parameter the command does not declare ----

/// The colleague edits slide 2's body. `drawing.delete` declares its targets parameter as `ids`
/// (as do `edit.cut`, `edit.delete`, `shape.delete`, `drawing.transform`, `drawing.format` and
/// `drawing.chart`) and also takes one shape as `id`. Named either way, the step is refused as
/// `changed` on the body and nothing is written: since kition-sdk 579a322 every object a step's
/// parameters name is a premise. The other way round, `text.set` declares `id`, and a step giving
/// it `ids` names the changed body too, but is no conflict: since a0758b3 a parameter the command's
/// schema does not take (this kit's schemas close their properties) is `invalid-params`, naming
/// `ids` and the step, before the guard, so a misspelt parameter is never read as a premise and
/// the agent is told what to fix rather than to inspect again; nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn n_a_target_named_by_id_on_a_command_that_declares_ids_is_refused_as_changed() {
    let doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let declared = |id: &str| doc.commands().into_iter().find(|command| command.id == id).and_then(|command| command.targets);
    assert_eq!((declared("drawing.delete"), declared("text.set")), (Some("ids"), Some("id")));
    for by in ["ids", "id"] {
        let f = Fixture::new();
        let (second, basis) = f.inspect(2).await;
        let body = tool_id(&second, "Body 2");
        f.colleague(&[("text.set", json!({ "id": body, "text": "Colleague's" }))]);
        let given = if by == "ids" { json!([body]) } else { json!(body) };
        let step = json!({ "command": "drawing.delete", "params": { by: given } });
        let refused = f.refused("run", json!({ "basis": basis, "steps": [step] })).await;
        assert_conflict(&refused, &body, "changed");
        assert_only_texts(&refused["others"], &[text_edit(&body, "Body 2", "Colleague's")]);
        assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2", "Colleague's"], "{by}: nothing written");
        if by == "ids" {
            let wrong = json!({ "command": "text.set", "params": { "ids": [body], "text": "Mine" } });
            let refused = f.refused("run", json!({ "basis": basis, "steps": [wrong] })).await;
            assert_eq!(refused["error"]["code"], "invalid-params", "{refused}");
            assert_eq!(refused["error"]["index"], 0, "{refused}");
            assert!(refused["error"]["message"].as_str().unwrap().contains("`ids`"), "{refused}");
            assert!(refused.get("conflicts").is_none(), "{refused}");
            assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2", "Colleague's"], "nothing written");
        }
        f.shutdown().await;
    }
}

// ---- o. A slide named by its place in the deck (C8-slide-5) ----

/// Slide commands that name their slide by its index (`slide.notes`, `slide.hide`, `slide.rename`,
/// `slide.layout`…) or by where a move takes it declare those parameters places, and the kit says
/// what slide a place names now (`context.under`). The colleague inserts a slide before slide 5;
/// a step on `{index: 4}` (slide 5 at the agent's basis) is refused as `relocated`, naming the slide
/// now at index 4 (the new one) with the place as the step named it and `under`, and nothing is
/// written. After an insert after slide 5, or a text edit elsewhere, the same step applies to
/// slide 5. A step by slide number and shape number (`at.part`) after the insert is refused once,
/// by the part premise, naming slide 5's slide alone: the place premise does not double it.
#[tokio::test(flavor = "multi_thread")]
async fn o_a_slide_named_by_its_index_is_refused_as_relocated_after_an_insert_before_it() {
    for change in ["insert before", "insert after", "text elsewhere"] {
        let f = Fixture::new();
        let (fifth, basis) = f.inspect(5).await;
        let (ninth, _) = f.inspect(9).await;
        match change {
            "insert before" => f.colleague(&[("slide.new", json!({ "layout": "blank", "at": 1 }))]),
            "insert after" => f.colleague(&[("slide.new", json!({ "layout": "blank", "at": 7 }))]),
            _ => f.colleague(&[("text.set", json!({ "id": tool_id(&ninth, "Body 9"), "text": "Elsewhere" }))]),
        }
        let before = std::fs::read(f.file()).unwrap();
        let step = json!({ "command": "slide.notes", "params": { "index": 4, "text": "Mine" } });
        let answer = f.call("run", json!({ "basis": basis, "steps": [step] })).await;
        if change == "insert before" {
            let refused = answer.expect_err("refused");
            assert_eq!(refused["error"]["code"], "conflict", "{refused}");
            let conflicts = refused["conflicts"].as_array().unwrap();
            assert!(conflicts.iter().all(|conflict| conflict["reason"] == "relocated"), "{refused}");
            let placed = conflicts.iter().find(|conflict| conflict["place"].is_object()).unwrap_or_else(|| panic!("{refused}"));
            assert_eq!(placed["place"]["index"], 4, "{refused}");
            assert_ne!(placed["place"]["under"], json!([fifth["toolId"]]), "the place names another slide now: {refused}");
            assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
            println!("o: {change}: {}", refused["conflicts"]);
            // By slide number and shape number: one refusal, by the part premise.
            let number = shape(&fifth, "Body 5")["id"].clone();
            let refused = f.refused("run", json!({ "basis": basis, "steps": [by_number(5, &number, "Mine")] })).await;
            assert_eq!(refused["conflicts"].as_array().unwrap().len(), 1, "{refused}");
            assert_conflict(&refused, fifth["toolId"].as_str().unwrap(), "relocated");
        } else {
            let ran = answer.unwrap_or_else(|error| panic!("{change}: {error}"));
            // Slide 5's notes (and the deck's notes master, which a deck's first notes bring).
            let notes = ids(&ran["changes"]["changed"]);
            assert!(notes.contains(&fifth["toolId"].as_str().unwrap().to_owned()), "{change}: slide 5's notes: {ran}");
            assert!(notes.iter().all(|id| id == fifth["toolId"].as_str().unwrap() || id == "slide-file"), "{change}: {ran}");
        }
        f.shutdown().await;
    }
}

/// C8-land-9s: the integer single form must be guarded as well as the list form,
/// and its slide is a place premise even when the step names a shape explicitly.
#[tokio::test(flavor = "multi_thread")]
async fn integer_single_ids_hold_the_shape_and_the_numbered_slide() {
    for (change, part) in [
        ("target changed", true),
        ("slide inserted", true),
        ("unrelated", true),
        ("target changed", false),
        ("slide inserted", false),
        ("unrelated", false),
    ] {
        let f = Fixture::new();
        let (second, basis) = f.inspect(2).await;
        let body = shape(&second, "Body 2");
        match change {
            "target changed" => f.colleague(&[("text.set", json!({ "id": body["toolId"], "text": "Colleague's" }))]),
            "slide inserted" => f.colleague(&[("slide.new", json!({ "layout": "blank", "at": 0 }))]),
            _ => f.colleague(&[("text.set", json!({ "onSlide": 9, "id": shape(&second, "Title 2")["id"], "text": "Unrelated" }))]),
        }
        let before = std::fs::read(f.file()).unwrap();
        let mut step = json!({ "command": "drawing.delete", "params": { "id": body["id"] } });
        if part {
            step["at"] = json!({ "part": { "slide": 2 } });
        } else {
            step["params"]["onSlide"] = json!(2);
        }
        let answer = f.call("run", json!({ "basis": basis, "steps": [step] })).await;
        if change == "unrelated" {
            answer.unwrap_or_else(|error| panic!("unrelated: {error}"));
            assert_eq!(texts_on_disk(&f.file(), 2), ["Title 2"]);
        } else {
            let refused = answer.expect_err(change);
            let (id, reason) = if change == "target changed" {
                (body["toolId"].as_str().unwrap(), "changed")
            } else {
                (second["toolId"].as_str().unwrap(), "relocated")
            };
            if change == "slide inserted" && !part {
                // A declared place reports what is there now; at.part reports the old part.
                let mut saved = SlideDocument::open("Deck.pptx", &before).unwrap();
                let now = saved.execute("slide.inspect", json!({ "index": 1 })).unwrap();
                let conflict = assert_conflict(&refused, now["toolId"].as_str().unwrap(), reason);
                assert_eq!(conflict["place"], json!({ "onSlide": 2, "under": [now["toolId"]] }));
            } else {
                assert_conflict(&refused, id, reason);
            }
            assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
        }
        f.shutdown().await;
    }
}

/// slide.delete's ids take precedence over index; an unused index is no premise.
#[tokio::test(flavor = "multi_thread")]
async fn slide_ids_take_precedence_over_unused_index() {
    let f = Fixture::new();
    let (second, basis) = f.inspect(2).await;
    f.colleague(&[("slide.notes", json!({ "index": 4, "text": "Colleague's notes" }))]);
    let ran = f
        .ok(
            "run",
            json!({ "basis": basis, "steps": [{
        "command": "slide.delete", "params": { "ids": [second["toolId"]], "index": 4 }
    }] }),
        )
        .await;
    assert!(ids(&ran["changes"]["deleted"]).contains(&second["toolId"].as_str().unwrap().to_owned()), "{ran}");
    let mut saved = SlideDocument::open("Deck.pptx", &std::fs::read(f.file()).unwrap()).unwrap();
    assert_eq!(saved.execute("slide.inspect", json!({ "index": 3 })).unwrap()["notes"], "Colleague's notes");
    f.shutdown().await;
}

/// A comment's own zero-based slide index scopes its numeric anchor and is a place premise.
/// C8-slide-read-4: cmd/review.rs:155-175 reads anchor identity/existence, not its text.
/// Deletion refuses without writing.
#[tokio::test(flavor = "multi_thread")]
async fn comment_anchor_uses_its_explicit_slide_index() {
    for change in ["target changed", "target deleted", "slide inserted", "unrelated"] {
        let f = Fixture::new();
        let (second, basis) = f.inspect(2).await;
        let title = shape(&second, "Title 2");
        match change {
            "target changed" => f.colleague(&[("text.set", json!({"id":title["toolId"], "text":"Colleague's"}))]),
            "target deleted" => f.colleague(&[("shape.delete", json!({"ids":[title["toolId"]]}))]),
            "slide inserted" => f.colleague(&[("slide.new", json!({"layout":"blank", "at":0}))]),
            _ => f.colleague(&[("text.set", json!({"id":title["id"], "onSlide":9, "text":"Other slide"}))]),
        }
        let before = std::fs::read(f.file()).unwrap();
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[{
                    "command":"comment.add","params":{"slide":1,"id":title["id"],"text":"Review this title"}
                }]}),
            )
            .await;
        if matches!(change, "unrelated" | "target changed") {
            answer.unwrap_or_else(|error| panic!("unrelated: {error}"));
            let saved = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &std::fs::read(f.file()).unwrap()).unwrap();
            assert!(
                saved.slides[1]
                    .comments
                    .iter()
                    .any(|comment| comment.text == "Review this title" && comment.shape.map(|id| u64::from(id.0)) == title["id"].as_u64()),
                "saved comments: {:?}",
                saved.slides[1].comments
            );
            if change == "target changed" {
                assert_eq!(
                    saved.slides[1].shape(kition_slide_model::ShapeId(title["id"].as_u64().unwrap() as u32)).unwrap().text.as_ref().unwrap().text(),
                    "Colleague's"
                );
            }
        } else {
            let refused = answer.expect_err(change);
            if change == "target deleted" {
                assert_conflict(&refused, title["toolId"].as_str().unwrap(), "deleted");
            } else {
                let mut saved = SlideDocument::open("Deck.pptx", &before).unwrap();
                let now = saved.execute("slide.inspect", json!({"index":1})).unwrap();
                let conflict = assert_conflict(&refused, now["toolId"].as_str().unwrap(), "relocated");
                assert_eq!(conflict["place"], json!({"slide":1,"under":[now["toolId"]]}));
            }
            assert_eq!(std::fs::read(f.file()).unwrap(), before, "nothing written");
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_endpoint_on_a_tool_ids_slide_is_guarded() {
    for changed in [true, false] {
        let f = Fixture::new();
        let (second, basis) = f.inspect(2).await;
        let title = shape(&second, "Title 2");
        let body = shape(&second, "Body 2");
        let target = if changed { body["toolId"].clone() } else { title["id"].clone() };
        let (command, params) = if changed {
            ("shape.change", json!({ "ids": [target], "preset": "ellipse" }))
        } else {
            ("text.set", json!({ "id": target, "onSlide": 9, "text": "Other slide" }))
        };
        f.colleague(&[(command, params)]);
        let before = std::fs::read(f.file()).unwrap();
        let answer = f
            .call(
                "run",
                json!({ "basis": basis, "steps": [{
            "command": "shape.connect", "params": { "from": title["toolId"], "to": body["id"] }
        }] }),
            )
            .await;
        if changed {
            let refused = answer.expect_err("the numeric endpoint changed");
            assert_conflict(&refused, body["toolId"].as_str().unwrap(), "changed");
            assert_eq!(std::fs::read(f.file()).unwrap(), before);
        } else {
            answer.unwrap_or_else(|error| panic!("unrelated edit: {error}"));
            let saved = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &std::fs::read(f.file()).unwrap()).unwrap();
            assert_eq!(saved.slides[1].shapes.len(), 3, "the connector was saved on slide 2");
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_animation_index_is_held_beside_its_trigger_id() {
    let mut doc = SlideDocument::open("Deck.pptx", &deck()).unwrap();
    let second = doc.execute("slide.inspect", json!({ "index": 1 })).unwrap();
    let title = shape(&second, "Title 2");
    let body = shape(&second, "Body 2");
    doc.execute("animation.add", json!({ "ids": [body["toolId"], title["toolId"]], "effect": "fade" })).unwrap();
    let bytes = doc.save(Some("Deck.pptx")).unwrap();
    let initial = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &bytes).unwrap();
    let target_shape = initial.slides[1].animations[0].shape;
    let other_shape = initial.slides[1].animations[1].shape;
    for change in ["target", "other", "reorder"] {
        let f = Fixture::with_bytes(CallOptions::default(), &bytes);
        let (_, basis) = f.inspect(2).await;
        let edit = match change {
            "reorder" => ("animation.move", json!({ "index": 1, "to": 0 })),
            _ => ("animation.timing", json!({ "index": if change == "target" { 0 } else { 1 }, "duration": 1234 })),
        };
        f.colleague(&[("slide.go", json!({ "index": 1 })), edit]);
        let before = std::fs::read(f.file()).unwrap();
        let answer = f
            .call(
                "run",
                json!({ "basis": basis, "steps": [{
            "command": "animation.timing", "at": { "part": { "slide": 2 } },
            "params": { "index": 0, "trigger": title["id"], "duration": 800 }
        }] }),
            )
            .await;
        if change != "other" {
            let refused = answer.expect_err("the indexed animation changed");
            assert_conflict(&refused, second["toolId"].as_str().unwrap(), "changed");
            assert_eq!(std::fs::read(f.file()).unwrap(), before);
        } else {
            answer.unwrap_or_else(|error| panic!("another animation changed: {error}"));
            let saved = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &std::fs::read(f.file()).unwrap()).unwrap();
            // PPTX groups triggered animations separately, so saving can reorder the sequence.
            let target = saved.slides[1].animations.iter().find(|animation| animation.shape == target_shape).unwrap();
            let other = saved.slides[1].animations.iter().find(|animation| animation.shape == other_shape).unwrap();
            assert_eq!(target.duration_ms, 800);
            assert_eq!(target.trigger.map(|id| u64::from(id.0)), title["id"].as_u64());
            assert_eq!(other.duration_ms, 1234);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn animation_shape_ids_hold_the_animations_stored_on_the_slide() {
    let mut doc = SlideDocument::open("Deck.pptx", &deck()).unwrap();
    let second = doc.execute("slide.inspect", json!({ "index": 1 })).unwrap();
    let body = shape(&second, "Body 2");
    doc.execute("animation.add", json!({ "ids": [body["toolId"]], "effect": "fade" })).unwrap();
    let bytes = doc.save(Some("Deck.pptx")).unwrap();
    let f = Fixture::with_bytes(CallOptions::default(), &bytes);
    let (_, basis) = f.inspect(2).await;
    f.colleague(&[("slide.go", json!({ "index": 1 })), ("animation.timing", json!({ "index": 0, "duration": 1234 }))]);
    let before = std::fs::read(f.file()).unwrap();
    let refused = f
        .refused(
            "run",
            json!({ "basis": basis, "steps": [{
        "command": "animation.remove", "params": { "ids": [body["toolId"]] }
    }] }),
        )
        .await;
    assert_conflict(&refused, second["toolId"].as_str().unwrap(), "changed");
    assert_eq!(std::fs::read(f.file()).unwrap(), before);
    f.shutdown().await;
}
