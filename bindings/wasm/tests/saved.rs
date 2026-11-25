//! In memory = saved (task card C8-slide-5): the object model a deck holds after a command is the
//! object model the saved file opens as. The premise guard compares the document the kit holds
//! with the document the kit reads from the file another program saved (kition-kits
//! `outside_change`), so every difference between the two reads as a change by others, and a
//! write on an object that differs only so is refused as `changed`.
//!
//! For each deck and each representative command applied to it (in turn, on one open document),
//! the replica after the command and the replica of the saved file opened again are compared as
//! the guard compares two reads of a file: paired by tool id and by place
//! (`kition_kdm::reread::diff_paired`, which lists every change and hides none in a count). The
//! first check of each deck is the no-op case: open, save, open again equals open.
//!
//! The decks: the ten-slide deck of `bindings/headless/tests/outside.rs` and a deck with a table,
//! a chart, a group, a connector, a picture, notes and an animation, both built here; with
//! `KITION_SLIDE_CORPUS=<golden_ppt dir>` also the corpus decks in [`CORPUS`] (slow: the
//! 1,044-slide deck alone takes minutes).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use kition_kit::KitDocument;
use kition_kit::kdm::{Schema, reread};
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_slide_kit::SlideDocument;
use kition_slide_model::text::AutoFit;
use kition_slide_model::{Shape, ShapeKind, SlideFile};
use serde_json::{Value, json};

const NAME: &str = "Deck.pptx";

/// The corpus decks, relative to `KITION_SLIDE_CORPUS`: the shapes deck, the 200-slide and the
/// 1,044-slide decks, and decks with tables, charts, groups and connectors, media, notes,
/// animations, transitions, smart graphics and embedded objects.
const CORPUS: &[&str] = &[
    "shape/形状.pptx",
    "表格/表格.pptx",
    "chart/图表.pptx",
    "animation/元素动画.pptx",
    "transition/页面切换动画.pptx",
    "音频/音频.pptx",
    "备注/备注.pptx",
    "SmartArt/SmartArt.pptx",
    "对象和附件/对象和附件.pptx",
    "shape/流程3项/流程3项.pptx",
    "shape/并列5项2/并列5项2.pptx",
    "shape/列表1-2项/列表1-2项.pptx",
];

/// The replica the document holds now, as `{ objects }`.
fn state(doc: &mut SlideDocument) -> Result<Value, String> {
    Exchange::new().replica(doc).map_err(|error| error.message)?.state_json().map_err(|error| error.to_string())
}

/// What differs between the model in memory and the model of the saved file, one line per object
/// and field: `<list> <id> <type> <field>: <before> -> <after>`.
fn mismatches(schema: &Schema, memory: &Value, saved: &Value) -> Vec<String> {
    let diff = reread::diff_paired(schema, memory, saved).to_json();
    let mut out = Vec::new();
    for list in ["created", "changed", "moved", "deleted"] {
        for entry in diff[list].as_array().into_iter().flatten() {
            let (id, ty) = (&entry["id"], entry["type"].as_str().unwrap_or_default());
            match entry["edits"].as_array() {
                Some(edits) if !edits.is_empty() => {
                    for edit in edits {
                        out.push(format!("{list} {id} {ty} {}: {} -> {}", edit["field"], cut(&edit["before"]), cut(&edit["after"])));
                    }
                }
                _ => out.push(format!("{list} {id} {ty} {}", cut(entry))),
            }
        }
    }
    if let Some(renumbered) = diff.get("renumbered").filter(|value| !value.is_null()) {
        out.push(format!("renumbered {renumbered}"));
    }
    out
}

fn cut(value: &Value) -> String {
    let text = value.to_string();
    if std::env::var_os("KITION_SLIDE_SAVED_FULL").is_none() && text.len() > 400 {
        format!("{}…", text.chars().take(400).collect::<String>())
    } else {
        text
    }
}

/// One deck's checks: each step's mismatches, and how long the deck took.
struct Run {
    deck: String,
    schema: std::sync::Arc<Schema>,
    found: Vec<String>,
    /// Every mismatch found so far, without the step: a later step that leaves a mismatch as it
    /// was does not report it again.
    seen: std::collections::BTreeSet<String>,
    steps: usize,
}

impl Run {
    fn new(deck: &str, doc: &SlideDocument) -> Self {
        Self { deck: deck.to_owned(), schema: doc.model_schema(), found: Vec::new(), seen: Default::default(), steps: 0 }
    }

    /// Model now vs model of the saved file opened again.
    fn check(&mut self, step: &str, doc: &mut SlideDocument) {
        let bytes = doc.save(Some(NAME)).unwrap();
        let mut again = SlideDocument::open(NAME, &bytes).unwrap();
        let (memory, saved) = match (state(doc), state(&mut again)) {
            (Ok(memory), Ok(saved)) => (memory, saved),
            (memory, saved) => {
                let error = memory.err().or(saved.err()).unwrap_or_default();
                self.found.push(format!("{} / {step}: no object model: {error}", self.deck));
                eprintln!("MISMATCH {}", self.found.last().unwrap());
                return;
            }
        };
        for line in mismatches(&self.schema, &memory, &saved) {
            if !self.seen.insert(line.clone()) {
                continue;
            }
            self.found.push(format!("{} / {step}: {line}", self.deck));
            eprintln!("MISMATCH {}", self.found.last().unwrap());
        }
        self.steps += 1;
    }
}

/// A step: a command and its parameters, or an object-model batch.
enum Step {
    Command(&'static str, Value),
    Batch(Value),
}

fn run_step(doc: &mut SlideDocument, step: &Step) -> Result<(), String> {
    match step {
        Step::Command(command, params) => doc.execute(command, params.clone()).map(|_| ()),
        Step::Batch(batch) => Exchange::new().batch(doc, batch, false).map(|_| ()).map_err(|error| error.message),
    }
}

fn shape_tool_id(_slide: &kition_slide_model::Slide, shape: &Shape) -> String {
    format!("shape:{}", shape.identity)
}

fn own_autofit(shape: &Shape) -> Option<AutoFit> {
    shape.text.as_ref().and_then(|body| body.body.autofit)
}

fn has_text(shape: &Shape) -> bool {
    shape.text.as_ref().is_some_and(|body| !body.text().trim().is_empty())
}

/// The first top-level shape (slide index, slide, shape) that `pick` takes, from slide `from` on.
fn find(file: &SlideFile, from: usize, pick: impl Fn(&Shape) -> bool) -> Option<(usize, String, String)> {
    file.slides.iter().enumerate().skip(from).find_map(|(index, slide)| {
        slide.shapes.iter().find(|shape| pick(shape)).map(|shape| (index, format!("slide:{}", slide.id.0), shape_tool_id(slide, shape)))
    })
}

/// The representative agent commands for a deck, picked from what it holds. Steps that change
/// the slides' order or count come last, so the ids the others name stay valid.
fn steps(file: &SlideFile) -> Vec<(String, Step)> {
    let mut out: Vec<(String, Step)> = Vec::new();
    let mut add = |name: &str, step: Step| out.push((name.to_owned(), step));
    if let Some((_, _, id)) = find(file, 0, |shape| has_text(shape) && matches!(own_autofit(shape), Some(AutoFit::Shrink { .. }))) {
        add(
            "text.set on a shrink-to-fit box",
            Step::Command(
                "text.set",
                json!({ "id": id, "text": "A longer text an agent wrote into a box that shrinks its text on overflow, so the box refits it" }),
            ),
        );
    }
    if let Some((_, _, id)) = find(file, 0, |shape| has_text(shape) && own_autofit(shape) == Some(AutoFit::Shape)) {
        add("text.set on a box that grows to fit", Step::Command("text.set", json!({ "id": id, "text": "Grown\nby two\nlines of text" })));
    }
    let plain = find(file, 0, |shape| has_text(shape) && matches!(shape.kind, ShapeKind::Shape));
    if let Some((_, _, id)) = &plain {
        add("text.set", Step::Command("text.set", json!({ "id": id, "text": "Mine" })));
        add("format.fontSize", Step::Command("format.fontSize", json!({ "ids": [id], "size": 13.3 })));
        add("format.bold", Step::Command("format.bold", json!({ "ids": [id] })));
        add("shape.move", Step::Command("shape.move", json!({ "ids": [id], "dx": 7.31, "dy": -2.17 })));
        add("shape.resize", Step::Command("shape.resize", json!({ "ids": [id], "w": 201.337, "h": 61.111 })));
        add("shape.rotate", Step::Command("shape.rotate", json!({ "ids": [id], "deg": 12.345 })));
        add(
            "batch set xfrm",
            Step::Batch(json!({ "origin": { "kind": "agent", "principal": "agent" }, "ops": [
                { "op": "set", "target": id, "field": "xfrm", "value": { "x": 10.123456, "y": 20.654321, "w": 100.0001, "h": 50.4999, "rot": 0.0, "flipH": false, "flipV": false } }
            ] })),
        );
    }
    for (name, kind) in [("table", "table"), ("chart", "chart"), ("group", "group"), ("picture", "picture"), ("connector", "connector")] {
        let found = find(file, 0, |shape| {
            matches!(
                (&shape.kind, kind),
                (ShapeKind::Table(_), "table")
                    | (ShapeKind::Chart(_), "chart")
                    | (ShapeKind::Group { .. }, "group")
                    | (ShapeKind::Picture { .. }, "picture")
                    | (ShapeKind::Connector { .. }, "connector")
            )
        });
        if let Some((_, _, id)) = found {
            add(&format!("shape.move of a {name}"), Step::Command("shape.move", json!({ "ids": [id], "dx": 3.3, "dy": 1.7 })));
            if name == "table" {
                add("text.set in a table cell", Step::Command("text.set", json!({ "id": id, "cell": [0, 0], "text": "Cell" })));
            }
        }
    }
    add("slide.notes", Step::Command("slide.notes", json!({ "text": "Speak slowly", "index": 0 })));
    add("textBox.insert", Step::Command("textBox.insert", json!({ "rect": [33.3, 44.4, 155.55, 66.66], "text": "New box" })));
    add("shape.insert", Step::Command("shape.insert", json!({ "preset": "ellipse", "rect": [300.1, 200.2, 120.3, 80.4], "text": "Oval" })));
    add(
        "table.insert",
        Step::Command(
            "table.insert",
            json!({ "rows": 2, "cols": 3, "rect": [40.5, 300.25, 333.3, 90.9], "data": [["a", "b", "c"], ["d", "e", "f"]] }),
        ),
    );
    // A shape moved to another slide through the object model (as kit-env's moves do).
    if file.slides.len() > 1
        && let Some(first) = file.slides.first()
        && let Some(shape) = first.shapes.last()
    {
        let to = format!("slide:{}", file.slides[1].id.0);
        add(
            "batch move a shape to another slide",
            Step::Batch(json!({ "origin": { "kind": "agent", "principal": "agent" }, "ops": [
                { "op": "list.remove", "target": format!("slide:{}", first.id.0), "field": "shapes", "index": first.shapes.len() - 1 },
                { "op": "list.insert", "target": to, "field": "shapes", "item": shape_tool_id(first, shape) }
            ] })),
        );
    }
    if let Some((_, _, id)) = find(file, 1, has_text) {
        add("drawing.delete", Step::Command("drawing.delete", json!({ "ids": [id] })));
    }
    add("slide.new", Step::Command("slide.new", json!({ "at": 0 })));
    add("slide.duplicate", Step::Command("slide.duplicate", json!({ "index": 1 })));
    if file.slides.len() > 2 {
        add("slide.move", Step::Command("slide.move", json!({ "from": 2, "to": 0 })));
    }
    add("slide.delete", Step::Command("slide.delete", json!({ "index": 1 })));
    out
}

/// Opens `bytes`, checks the no-op case, then each step after running it. Answers the mismatches.
fn sweep(deck: &str, bytes: &[u8]) -> Vec<String> {
    let started = Instant::now();
    let file = kition_slide_engine::cmd::file::open_slide_file(NAME, bytes).unwrap();
    let steps = steps(&file);
    let mut doc = SlideDocument::open(NAME, bytes).unwrap();
    let mut run = Run::new(deck, &doc);
    run.check("open, save, open", &mut doc);
    let mut skipped = Vec::new();
    for (name, step) in &steps {
        match run_step(&mut doc, step) {
            Ok(()) => run.check(name, &mut doc),
            Err(error) => skipped.push(format!("{name}: {error}")),
        }
    }
    eprintln!(
        "{deck}: {} slides, {} checks, {} mismatches, {:.1} s{}",
        file.slides.len(),
        run.steps,
        run.found.len(),
        started.elapsed().as_secs_f64(),
        if skipped.is_empty() { String::new() } else { format!("; not run: {}", skipped.join("; ")) }
    );
    run.found
}

/// The ten-slide deck of `bindings/headless/tests/outside.rs`: a title and a body per slide,
/// a picture on the last.
fn ten_slides() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    for n in 1..=10 {
        if n > 1 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        }
        doc.execute("textBox.insert", json!({ "rect": [60, 60, 400, 80], "text": format!("Title {n}") })).unwrap();
        doc.execute("textBox.insert", json!({ "rect": [60, 300, 400, 80], "text": format!("Body {n}") })).unwrap();
    }
    doc.execute("image.insert", json!({ "name": "red.png", "data": RED })).unwrap();
    doc.save(Some(NAME)).unwrap()
}

const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";

/// A deck with what the corpus decks hold, made by this kit: a placeholder that shrinks its text,
/// a table, a chart, a group, two shapes joined by a connector, a picture, notes and an animation.
fn mixed() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    doc.execute("slide.new", json!({ "layout": "titleAndContent", "title": "Mixed", "body": "One\nTwo\nThree" })).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2, "rect": [500, 60, 300, 120], "data": [["a", "b"], ["c", "d"]] })).unwrap();
    doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
    let a = doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 120, 80], "text": "A" })).unwrap()["id"].clone();
    let b = doc.execute("shape.insert", json!({ "preset": "rect", "rect": [400, 100, 120, 80], "text": "B" })).unwrap()["id"].clone();
    doc.execute("arrange.group", json!({ "ids": [a, b] })).unwrap();
    let c = doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [100, 300, 120, 80], "text": "C" })).unwrap()["id"].clone();
    let d = doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [400, 300, 120, 80], "text": "D" })).unwrap()["id"].clone();
    let _ = doc.execute("shape.connect", json!({ "from": c, "to": d }));
    let _ = doc.execute("animation.add", json!({ "ids": [c], "effect": "fade" }));
    let _ = doc.execute("chart.insert", json!({ "kind": "column" }));
    doc.execute("image.insert", json!({ "name": "red.png", "data": RED })).unwrap();
    doc.execute("slide.notes", json!({ "text": "Notes of slide 3", "index": 2 })).unwrap();
    doc.save(Some(NAME)).unwrap()
}

fn report(found: &[String]) {
    assert!(found.is_empty(), "{} mismatches between the model in memory and the saved file's (above)", found.len());
}

#[test]
fn the_model_in_memory_is_the_saved_files_on_the_ten_slide_deck() {
    report(&sweep("ten slides", &ten_slides()));
}

#[test]
fn the_model_in_memory_is_the_saved_files_on_a_deck_with_tables_charts_groups_connectors_media_notes() {
    report(&sweep("mixed", &mixed()));
}

#[test]
fn creations_use_only_containers_the_file_can_store() {
    let bytes = mixed();
    let mut original = SlideDocument::open(NAME, &bytes).unwrap();
    let file = kition_slide_engine::cmd::file::open_slide_file(NAME, &bytes).unwrap();
    let mut parents = Vec::new();
    for index in 0..file.slides.len() {
        let part = original.execute("slide.inspect", json!({"index": index})).unwrap();
        for shape in part["shapes"].as_array().unwrap() {
            parents.push((shape["toolId"].as_str().unwrap().to_owned(), shape["kind"].as_str().unwrap().to_owned()));
        }
    }
    let mut failures = Vec::new();
    for (parent, kind) in parents {
        for member in ["children", "body"] {
            let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
            let before = state(&mut doc).unwrap();
            let body = before["objects"]
                .as_object()
                .unwrap()
                .values()
                .find(|object| object["key"] == parent.trim_start_matches("shape:"))
                .and_then(|object| object["body"].as_str());
            let ops = if member == "children" {
                json!([{"op": "create", "type": "Shape", "in": format!("{parent}.children"), "fields": {"name": "Child"}}])
            } else if let Some(body) = body {
                json!([
                    {"op": "create", "type": "Paragraph", "in": format!("{body}.paragraphs"), "ref": "$paragraph"},
                    {"op": "text.insert", "target": "$paragraph", "field": "text", "index": 0, "text": "Body"}
                ])
            } else {
                json!([
                    {"op": "create", "type": "TextBody", "ref": "$body"},
                    {"op": "create", "type": "Paragraph", "in": "$body.paragraphs", "ref": "$paragraph"},
                    {"op": "text.insert", "target": "$paragraph", "field": "text", "index": 0, "text": "Body"},
                    {"op": "set", "target": parent, "field": "body", "value": "$body"}
                ])
            };
            let result = Exchange::new().batch(&mut doc, &json!({"ops": ops}), false);
            let allowed = if member == "children" { kind == "group" } else { matches!(kind.as_str(), "shape" | "placeholder") };
            if result.is_ok() != allowed {
                failures.push(format!("{kind}.{member}: allowed={allowed}, result={result:?}"));
            }
            if result.is_err() {
                assert_eq!(before, state(&mut doc).unwrap(), "refusal is atomic: {kind}.{member}");
            } else {
                let mut run = Run::new(&format!("{kind}.{member}"), &doc);
                run.check("create", &mut doc);
                failures.extend(run.found);
            }
        }
    }
    report(&failures);
}

#[test]
fn creations_in_notes_and_table_cells_survive_and_missing_anchors_are_refused() {
    let bytes = mixed();
    let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
    let original = state(&mut doc).unwrap();
    let objects = original["objects"].as_object().unwrap();
    let slide = objects.iter().find(|(_, v)| v["$type"] == "Slide").unwrap().0;
    let table = objects.iter().find(|(_, v)| v["$type"] == "Table").unwrap().0;
    let notes = objects[slide]["notesBody"].as_str().unwrap();
    let cell = objects[table]["cells"].as_object().unwrap().values().next().unwrap()["body"].as_str().unwrap();
    for body in [notes, cell] {
        let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
        // Local object ids change on each open, so find the same body by its stored holder.
        let current = state(&mut doc).unwrap();
        let body = if body == notes {
            current["objects"].as_object().unwrap().values().find(|v| v["$type"] == "Slide").unwrap()["notesBody"].clone()
        } else {
            current["objects"].as_object().unwrap().values().find(|v| v["$type"] == "Table").unwrap()["cells"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap()["body"]
                .clone()
        };
        Exchange::new()
            .batch(
                &mut doc,
                &json!({"ops": [
                    {"op": "create", "type": "Paragraph", "in": format!("{}.paragraphs", body.as_str().unwrap()), "ref": "$p"},
                    {"op": "text.insert", "target": "$p", "field": "text", "index": 0, "text": "Stored"}
                ]}),
                false,
            )
            .unwrap();
        let mut run = Run::new("notes or cell", &doc);
        run.check("create paragraph and run", &mut doc);
        report(&run.found);
    }
    for ops in [
        json!([{"op": "create", "type": "Animation", "in": format!("{slide}.animations"), "fields": {"props": {"shape": "missing"}}}]),
        json!([{"op": "create", "type": "Media", "in": "slide-file.media", "fields": {"props": {"name": "missing"}}}]),
        json!([{"op": "create", "type": "CommentThread", "fields": {"anchor": {"object": "missing", "slide": slide}}}]),
        json!([{"op": "map.set", "target": table, "field": "cells", "key": "missing:missing", "value": {"body": "missing"}}]),
        json!([{"op": "create", "type": "TextBody"}]),
        json!([{"op": "create", "type": "Paragraph"}]),
    ] {
        let result = Exchange::new().batch(&mut doc, &json!({"ops": ops}), false);
        assert!(result.is_err(), "unrepresentable creation accepted: {ops}");
        assert_eq!(original, state(&mut doc).unwrap(), "refusal is atomic: {ops}");
    }
}

#[test]
fn creations_reject_unstored_members_of_every_shape_kind() {
    let bytes = mixed();
    let mut file = kition_slide_engine::cmd::file::open_slide_file(NAME, &bytes).unwrap();
    let mut kinds = vec![
        ShapeKind::Shape,
        ShapeKind::Picture { fill: Default::default() },
        ShapeKind::Group { children: Vec::new(), child: Default::default() },
        ShapeKind::Connector { start: None, end: None },
        ShapeKind::Table(Default::default()),
        ShapeKind::Chart(Default::default()),
        ShapeKind::Media(Default::default()),
        ShapeKind::Ink { strokes: Vec::new() },
        ShapeKind::Opaque { xml: String::new(), preview: None, preview_adjust: None, label: "Preserved".into(), rels: Vec::new() },
    ];
    let shapes = kinds.drain(..).map(|kind| Shape { id: kition_slide_model::ShapeId(file.alloc_id()), kind, ..Default::default() }).collect();
    std::sync::Arc::make_mut(&mut file.slides[0]).shapes = shapes;
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "kslide").unwrap();
    let mut original = SlideDocument::open(NAME, &bytes).unwrap();
    let original = state(&mut original).unwrap();
    let parents: Vec<_> = original["objects"]
        .as_object()
        .unwrap()
        .values()
        .filter(|v| v["$type"] == "Shape")
        .filter_map(|v| Some((format!("shape:{}", v["key"].as_str()?), v["kind"].as_str()?.to_owned())))
        .collect();
    for kind in ["shape", "picture", "group", "connector", "table", "chart", "media", "ink", "opaque"] {
        assert!(parents.iter().any(|(_, found)| found == kind), "missing parent kind {kind}");
    }
    for (parent, kind) in parents {
        for (member, ty, allowed) in
            [("children", "Shape", "group"), ("body", "TextBody", "shape"), ("table", "Table", "table"), ("chart", "Chart", "chart")]
        {
            if kind == allowed {
                continue;
            }
            let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
            let before = state(&mut doc).unwrap();
            let ops = if member == "children" {
                json!([{"op": "create", "type": ty, "in": format!("{parent}.{member}"), "fields": {"name": "Child"}}])
            } else {
                json!([
                    {"op": "create", "type": ty, "ref": "$member"},
                    {"op": "set", "target": parent, "field": member, "value": "$member"}
                ])
            };
            let result = Exchange::new().batch(&mut doc, &json!({"ops": ops}), false);
            assert!(result.is_err(), "{kind}.{member} accepted an unstored member");
            if kind != "opaque" {
                assert!(result.unwrap_err().message.contains("invalid placement"), "{kind}.{member} must be refused by placement validation");
            }
            assert_eq!(before, state(&mut doc).unwrap(), "refusal is atomic: {kind}.{member}");
        }
    }
}

#[test]
fn creations_on_master_and_layout_shape_lists_survive() {
    let bytes = mixed();
    for ty in ["Master", "Layout"] {
        let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
        let before = state(&mut doc).unwrap();
        let owner = before["objects"].as_object().unwrap().iter().find(|(_, v)| v["$type"] == ty).unwrap().0;
        Exchange::new()
            .batch(
                &mut doc,
                &json!({"ops": [{"op": "create", "type": "Shape", "in": format!("{owner}.shapes"), "fields": {"name": "Stored"}}]}),
                false,
            )
            .unwrap();
        let mut run = Run::new(ty, &doc);
        run.check("create shape", &mut doc);
        report(&run.found);
    }
}

#[test]
fn comment_creations_require_an_anchor_on_the_named_slide() {
    let bytes = mixed();
    for anchor_kind in ["shape", "slide", "missing", "other-slide", "notes"] {
        let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
        let before = state(&mut doc).unwrap();
        let objects = before["objects"].as_object().unwrap();
        let (slide, shape) =
            objects.iter().filter(|(_, v)| v["$type"] == "Slide").find_map(|(id, v)| Some((id, v["shapes"].as_array()?.first()?.as_str()?))).unwrap();
        let other = objects.iter().find(|(id, v)| *id != slide && v["$type"] == "Slide").unwrap().0;
        let object = match anchor_kind {
            "slide" => slide.as_str(),
            "missing" => "missing",
            "notes" => objects[slide]["notesBody"].as_str().unwrap(),
            _ => shape,
        };
        let on = if anchor_kind == "other-slide" { other } else { slide };
        let result = Exchange::new().batch(
            &mut doc,
            &json!({"ops": [
                {"op": "create", "type": "CommentThread", "ref": "$thread", "fields": {
                    "anchor": {"object": object, "slide": on, "x": 0, "y": 0}, "resolved": false
                }},
                {"op": "create", "type": "Comment", "ref": "$comment", "in": "$thread.comments", "fields": {
                    "author": "Agent", "name": "Agent", "created": "2026-11-12T00:00:00Z"
                }},
                {"op": "text.insert", "target": "$comment", "field": "body", "index": 0, "text": "Stored comment"}
            ]}),
            false,
        );
        let allowed = matches!(anchor_kind, "shape" | "slide");
        assert_eq!(result.is_ok(), allowed, "{anchor_kind}: {result:?}");
        if allowed {
            let mut run = Run::new(anchor_kind, &doc);
            run.check("create comment", &mut doc);
            report(&run.found);
        } else {
            assert_eq!(before, state(&mut doc).unwrap(), "refusal is atomic: {anchor_kind}");
        }
    }
}

#[test]
fn media_creations_require_available_part_bytes() {
    let mut doc = SlideDocument::open(NAME, &mixed()).unwrap();
    let before = state(&mut doc).unwrap();
    let mut blob = before["objects"].as_object().unwrap().values().find(|v| v["$type"] == "Media" && !v["blob"].is_null()).unwrap()["blob"].clone();
    blob["hash"] = json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    let result = Exchange::new().batch(
        &mut doc,
        &json!({"ops": [{"op": "create", "type": "Media", "in": "slide-file.media", "fields": {
            "blob": blob, "props": {"name": "Absent.png", "contentType": "image/png"}
        }}]}),
        false,
    );
    assert!(result.is_err(), "media without bytes was accepted: {result:?}");
    assert_eq!(before, state(&mut doc).unwrap(), "refusal is atomic");
}

#[test]
fn media_creations_validation_keeps_unedited_imported_missing_images() {
    let mut file = kition_slide_engine::cmd::file::open_slide_file(NAME, &mixed()).unwrap();
    let media = file.media.first_mut().unwrap();
    media.name = "missing.emf".to_owned();
    media.content_type = "image/x-emf".to_owned();
    media.data = Default::default();
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap();
    for initialized in [false, true] {
        let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
        if initialized {
            state(&mut doc).unwrap();
        }
        Exchange::new()
            .batch(&mut doc, &json!({"ops": [{"op": "set", "target": "slide-file", "field": "props.firstSlideNumber", "value": 2}]}), false)
            .unwrap();
        let mut run = Run::new("imported missing image", &doc);
        run.check("unrelated model edit", &mut doc);
        report(&run.found);
        let before = state(&mut doc).unwrap();
        let (id, media) = before["objects"].as_object().unwrap().iter().find(|(_, v)| v["$type"] == "Media" && v["blob"].is_null()).unwrap();
        let mut props = media["props"].clone();
        props["name"] = json!("changed.emf");
        let result = Exchange::new().batch(&mut doc, &json!({"ops": [{"op": "set", "target": id, "field": "props", "value": props}]}), false);
        assert!(result.is_err(), "a changed missing part needs bytes: {result:?}");
        assert_eq!(before, state(&mut doc).unwrap(), "refusal is atomic");
    }
}

#[test]
fn media_creations_validation_keeps_the_corpus_missing_images() {
    let Some(root) = std::env::var_os("KITION_SLIDE_CORPUS") else { return };
    let path = CORPUS.last().unwrap();
    let bytes = std::fs::read(std::path::Path::new(&root).join(path)).unwrap();
    let file = kition_slide_engine::cmd::file::open_slide_file(NAME, &bytes).unwrap();
    let steps = steps(&file);
    let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
    let mut run = Run::new(path, &doc);
    for name in ["batch set xfrm", "batch move a shape to another slide"] {
        let (_, step) = steps.iter().find(|(candidate, _)| candidate == name).unwrap();
        run_step(&mut doc, step).unwrap();
        run.check(name, &mut doc);
    }
    report(&run.found);
}

#[test]
fn media_creations_and_commands_share_the_current_blob_store() {
    let bytes = SlideDocument::create(NAME, 960, 540).unwrap().save(Some(NAME)).unwrap();
    let mut doc = SlideDocument::open(NAME, &bytes).unwrap();
    let slide = doc.execute("slide.inspect", json!({"index": 0})).unwrap()["toolId"].clone();
    Exchange::new()
        .batch(
            &mut doc,
            &json!({"ops": [
                {"op": "command", "id": "image.insert", "params": {"name": "red.png", "data": RED}},
                {"op": "set", "target": slide, "field": "props.name", "value": "With picture"}
            ]}),
            false,
        )
        .unwrap();
    let mut run = Run::new("command then model write", &doc);
    run.check("insert picture and rename", &mut doc);
    report(&run.found);
}

/// The corpus decks ([`CORPUS`]), when `KITION_SLIDE_CORPUS` names the corpus.
#[test]
fn the_model_in_memory_is_the_saved_files_on_the_corpus_decks() {
    let Ok(dir) = std::env::var("KITION_SLIDE_CORPUS") else { return };
    let only = std::env::var("KITION_SLIDE_CORPUS_ONLY").ok();
    let mut found = Vec::new();
    for deck in CORPUS.iter().filter(|deck| only.as_deref().is_none_or(|only| deck.contains(only))) {
        let path = std::path::Path::new(&dir).join(deck);
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("{deck}: not in the corpus");
            continue;
        };
        found.extend(sweep(deck, &bytes));
    }
    report(&found);
}
