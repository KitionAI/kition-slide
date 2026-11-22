//! C8-land-13-slide: independent saved writers shifting an observed insertion index.
//! Waiting for the SDK at 34dff06: batch index guards, held-path resolution, and structural
//! windows for table/animation lists flattened into holder fields. `animation.move` holds only its
//! two windows; the table insertions still hold the whole table, so a change after their gap is
//! refused `changed` (command_indexes_hold_the_gap_and_allow_later_insertions).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Indexes.pptx";

fn deck() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    for n in 0..6 {
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        for i in 0..5 {
            let made = doc
                .execute("insert.textBox", json!({"rect": [10, 10, 200, 80], "text": format!("Paragraph {n}-{i}\nSecond\nThird\nFourth\nFifth")}))
                .unwrap();
            doc.execute("animation.add", json!({"ids": [made["id"]], "effect": "appear"})).unwrap();
        }
        doc.execute("insert.table", json!({"rows": 5, "cols": 5})).unwrap();
    }
    doc.execute("slide.delete", json!({"index": 0})).unwrap();
    doc.save(Some(NAME)).unwrap()
}

struct Fixture {
    workspace: tempfile::TempDir,
    kits: KitManager,
}

impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(NAME), bytes).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { workspace, kits }
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "index-agent").await.unwrap() {
            Ok(answer) => Ok(answer.output),
            Err(error) => Err(error.structured()),
        }
    }

    fn colleague(&self, steps: &[(&str, Value)]) {
        let file = self.workspace.path().join(NAME);
        let mut doc = SlideDocument::open(NAME, &std::fs::read(&file).unwrap()).unwrap();
        doc.execute("slide.go", json!({"index": 0})).unwrap();
        for (id, params) in steps {
            doc.execute(id, params.clone()).unwrap();
        }
        std::fs::write(file, doc.save(Some(NAME)).unwrap()).unwrap();
    }

    fn colleague_insert_at(&self, animation: bool, at: usize) {
        let file = self.workspace.path().join(NAME);
        let mut doc = SlideDocument::open(NAME, &std::fs::read(&file).unwrap()).unwrap();
        doc.execute("slide.go", json!({"index": 0})).unwrap();
        if animation {
            let part = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
            doc.execute("animation.add", json!({"ids": [part["shapes"][0]["toolId"]], "effect": "fade"})).unwrap();
            doc.execute("animation.move", json!({"index": 5, "to": at})).unwrap();
        } else {
            let made = doc.execute("insert.textBox", json!({"text": "Colleague", "rect": [10, 10, 100, 50]})).unwrap();
            doc.execute("arrange.reorder", json!({"id": made["id"], "index": at})).unwrap();
        }
        std::fs::write(file, doc.save(Some(NAME)).unwrap()).unwrap();
    }

    fn colleague_model(&self, family: &str, remove: bool, before: bool) {
        let file = self.workspace.path().join(NAME);
        let mut doc = kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(&file).unwrap()).unwrap();
        let slide = std::sync::Arc::make_mut(&mut doc.slides[0]);
        let at = if before { 1 } else { 4 };
        if matches!(family, "paragraph" | "run") {
            let body = slide.shapes[0].text.as_mut().unwrap();
            if family == "paragraph" {
                if remove {
                    body.paragraphs.remove(at);
                } else {
                    body.paragraphs.insert(at, kition_slide_model::text::Paragraph::new("Other"));
                }
            } else {
                let text = &mut body.paragraphs[0].runs[0].text;
                if remove {
                    text.remove(at);
                } else {
                    text.insert(at, 'x');
                }
            }
        } else {
            let shape = slide.shapes.iter_mut().find(|s| matches!(s.kind, kition_slide_model::ShapeKind::Table { .. })).unwrap();
            let kition_slide_model::ShapeKind::Table(table) = &mut shape.kind else { panic!("table") };
            match (family, remove) {
                ("row", false) => table.insert_row(at),
                ("row", true) => {
                    assert!(table.delete_row(at));
                }
                (_, false) => table.insert_col(at),
                (_, true) => {
                    assert!(table.delete_col(at));
                }
            }
        }
        std::fs::write(file, kition_slide_engine::cmd::file::save_bytes(&doc, "pptx").unwrap()).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn model_indexes_hold_the_gap_and_allow_later_insertions() {
    let bytes = deck();
    let mut failures = Vec::new();
    for family in ["paragraph", "run", "row", "column"] {
        for remove in [false, true] {
            for before in [true, false] {
                let f = Fixture::new(&bytes);
                let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
                let shapes = seen["part"]["shapes"].as_array().unwrap();
                let shape = shapes[0]["toolId"].as_str().unwrap();
                let table = shapes.iter().find(|s| s["kind"] == "table").unwrap()["toolId"].as_str().unwrap();
                let (target, field, ty, fields) = match family {
                    "paragraph" => (format!("{shape}.body"), "paragraphs", "Paragraph", json!({"props": {"level": 0}})),
                    "run" => (shape.to_owned(), "body.paragraphs.0.text", "", json!({})),
                    "row" => (format!("{table}.table"), "rows", "Row", json!({"height": 30})),
                    _ => (format!("{table}.table"), "cols", "Col", json!({"width": 50})),
                };
                let insertion = |at, text: &str| {
                    if family == "run" {
                        json!({"op": "text.insert", "target": target, "field": field, "index": at, "text": text})
                    } else {
                        json!({"op": "create", "type": ty, "in": format!("{target}.{field}"), "at": at, "fields": fields})
                    }
                };
                f.colleague_model(family, remove, before);
                let answer = f.call("batch", json!({"basis": seen["revision"], "ops": [insertion(3, "z")]})).await;
                let reasons: Vec<String> = answer
                    .as_ref()
                    .err()
                    .into_iter()
                    .flat_map(|e| e["conflicts"].as_array().into_iter().flatten())
                    .filter_map(|c| c["reason"].as_str().map(str::to_owned))
                    .collect();
                println!(
                    "model index: {family} remove={remove} before={before}: applied={} code={} reasons={reasons:?}",
                    answer.is_ok(),
                    answer.as_ref().err().map_or(Value::Null, |e| e["error"]["code"].clone())
                );
                let correct = if before { reasons.iter().any(|r| r == "relocated") } else { answer.is_ok() };
                if !correct {
                    failures.push(format!("{family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok()));
                }
                f.kits.shutdown().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn addressable_model_indexes_hold_the_gap_and_allow_later_insertions() {
    let bytes = deck();
    let mut failures = Vec::new();
    for family in ["slide.create", "slide.insert", "slide.move", "shape.create", "shape.insert", "shape.move"] {
        for remove in [false, true] {
            for before in [true, false] {
                let f = Fixture::new(&bytes);
                let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
                let part = &seen["part"];
                let shapes = part["shapes"].as_array().unwrap();
                let slide = family.starts_with("slide.");
                let at = if before { 1 } else { 4 };
                let other = if slide {
                    if remove { ("slide.delete", json!({"index": at})) } else { ("slide.new", json!({"at": at, "layout": "blank"})) }
                } else if remove {
                    ("drawing.delete", json!({"ids": [shapes[at]["toolId"]]}))
                } else {
                    ("arrange.reorder", json!({"id": shapes[4]["toolId"], "index": at}))
                };
                let parent = if slide { json!("slide-file") } else { part["toolId"].clone() };
                let field = if slide { "slides" } else { "shapes" };
                let ty = if slide { "Slide" } else { "Shape" };
                let fields = if slide { json!({}) } else { json!({"kind": "shape", "name": "New"}) };
                let ops = if family.ends_with("create") {
                    json!([{"op": "create", "type": ty, "in": format!("{}.{field}", parent.as_str().unwrap()), "at": 3, "fields": fields}])
                } else if family.ends_with("insert") {
                    json!([
                        {"op": "create", "type": ty, "ref": "$new", "fields": fields},
                        {"op": "list.insert", "target": parent, "field": field, "index": 3, "item": "$new"}
                    ])
                } else {
                    json!([{"op": "list.move", "target": parent, "field": field, "from": 0, "to": 2}])
                };
                if !slide && !remove {
                    f.colleague_insert_at(false, at);
                } else {
                    f.colleague(&[other]);
                }
                let answer = f.call("batch", json!({"basis": seen["revision"], "ops": ops})).await;
                let reasons: Vec<String> = answer
                    .as_ref()
                    .err()
                    .into_iter()
                    .flat_map(|e| e["conflicts"].as_array().into_iter().flatten())
                    .filter_map(|c| c["reason"].as_str().map(str::to_owned))
                    .collect();
                println!("addressable index: {family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok());
                if !before
                    && family.ends_with("insert")
                    && let Err(error) = &answer
                {
                    println!("addressable refusal detail: {}", error["error"]);
                }
                if !(if before { reasons.iter().any(|r| r == "relocated") } else { answer.is_ok() }) {
                    failures.push(format!("{family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok()));
                }
                f.kits.shutdown().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn section_indexes_hold_the_gap_and_allow_later_insertions() {
    let mut doc = SlideDocument::open(NAME, &deck()).unwrap();
    for at in 0..6 {
        doc.execute("section.add", json!({"at": at, "name": format!("Section {at}")})).unwrap();
    }
    let bytes = doc.save(Some(NAME)).unwrap();
    let mut failures = Vec::new();
    for family in ["section.add", "section.move"] {
        for remove in [false, true] {
            for before in [true, false] {
                let f = Fixture::new(&bytes);
                let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
                let at = if before { 1 } else { 4 };
                if family == "section.add" {
                    let other = if remove { ("slide.delete", json!({"index": at})) } else { ("slide.new", json!({"at": at, "layout": "blank"})) };
                    f.colleague(&[other]);
                } else {
                    let other =
                        if remove { ("section.remove", json!({"index": at})) } else { ("section.add", json!({"at": at, "name": "Colleague"})) };
                    f.colleague(&[other]);
                }
                let params = if family == "section.add" { json!({"at": 3, "name": "New"}) } else { json!({"index": 0, "to": 2}) };
                let answer = f.call("run", json!({"basis": seen["revision"], "steps": [{"command": family, "params": params}]})).await;
                let reasons: Vec<String> = answer
                    .as_ref()
                    .err()
                    .into_iter()
                    .flat_map(|e| e["conflicts"].as_array().into_iter().flatten())
                    .filter_map(|c| c["reason"].as_str().map(str::to_owned))
                    .collect();
                println!("section index: {family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok());
                if !before && answer.is_err() {
                    println!("section false refusal: {}", answer.as_ref().unwrap_err());
                }
                if !(if before { reasons.iter().any(|r| r == "relocated") } else { answer.is_ok() }) {
                    failures.push(format!("{family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok()));
                }
                f.kits.shutdown().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn section_creation_holds_its_boundary_and_anchor_but_not_a_later_slide() {
    let bytes = deck();
    for removed in [1, 3, 4] {
        let f = Fixture::new(&bytes);
        let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
        f.colleague(&[("slide.delete", json!({"index": removed}))]);
        let answer = f
            .call(
                "run",
                json!({"basis": seen["revision"], "steps": [
                    {"command": "section.add", "params": {"at": 3, "name": "New"}}
                ]}),
            )
            .await;
        f.kits.shutdown().await;
        if removed == 4 {
            assert!(answer.is_ok(), "removal after the section boundary is unrelated: {answer:?}");
        } else {
            let error = answer.unwrap_err();
            let reason = "relocated";
            assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == reason), "removed {removed}: {error}");
        }
    }
}

/// Includes both directions of moves and both spellings of each table insertion.
#[tokio::test(flavor = "multi_thread")]
async fn command_indexes_hold_the_gap_and_allow_later_insertions() {
    let bytes = deck();
    let mut failures = Vec::new();
    for family in [
        "slide.new",
        "slide.duplicate",
        "slide.move",
        "arrange.reorder",
        "animation.move",
        "table.insertRowAbove",
        "table.insertRowBelow",
        "table.insertColumnLeft",
        "table.insertColumnRight",
    ] {
        for remove in [false, true] {
            for before in [true, false] {
                let f = Fixture::new(&bytes);
                let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
                let part = &seen["part"];
                let shapes = part["shapes"].as_array().unwrap();
                let shape = shapes[0]["toolId"].clone();
                let table = shapes.iter().find(|s| s["kind"] == "table").unwrap()["toolId"].clone();
                let index = if before { 1 } else { 4 };
                let (write, theirs) = match family {
                    "slide.new" => (
                        json!({"command": family, "params": {"at": 3, "layout": "blank"}}),
                        (if remove { "slide.delete" } else { "slide.new" }, json!({"index": index, "at": index, "layout": "blank"})),
                    ),
                    "slide.duplicate" => (
                        json!({"command": family, "params": {"index": 2}}),
                        (if remove { "slide.delete" } else { "slide.new" }, json!({"index": index, "at": index, "layout": "blank"})),
                    ),
                    "slide.move" => (
                        json!({"command": family, "params": {"from": 0, "to": 2}}),
                        (if remove { "slide.delete" } else { "slide.new" }, json!({"index": index, "at": index, "layout": "blank"})),
                    ),
                    "arrange.reorder" => (
                        json!({"command": family, "params": {"id": shape, "index": 2}}),
                        if remove {
                            ("drawing.delete", json!({"ids": [shapes[index]["toolId"]]}))
                        } else {
                            ("arrange.reorder", json!({"id": shapes[4]["toolId"], "index": index}))
                        },
                    ),
                    "animation.move" => (
                        json!({"command": family, "at": {"part": {"slide": 1}}, "params": {"index": 0, "to": 2}}),
                        if remove { ("animation.remove", json!({"index": index})) } else { ("animation.move", json!({"index": 4, "to": index})) },
                    ),
                    _ => {
                        let row = family.contains("Row");
                        let after = family.ends_with("Below") || family.ends_with("Right");
                        let field = if row { "row" } else { "col" };
                        let mut params = json!({"id": table});
                        params[field] = json!(if after { 1 } else { 2 });
                        let mut other = json!({"id": table});
                        other[field] = json!(index);
                        let edit = match (row, remove) {
                            (true, true) => "table.deleteRow",
                            (true, false) => "table.insertRowAbove",
                            (false, true) => "table.deleteColumn",
                            (false, false) => "table.insertColumnLeft",
                        };
                        (json!({"command": family, "params": params}), (edit, other))
                    }
                };
                let mut theirs = theirs;
                if family.starts_with("slide.") {
                    theirs.1.as_object_mut().unwrap().retain(|key, _| if remove { key == "index" } else { key != "index" });
                }
                if !remove && matches!(family, "arrange.reorder" | "animation.move") {
                    f.colleague_insert_at(family == "animation.move", index);
                } else {
                    f.colleague(&[theirs]);
                }
                let answer = f.call("run", json!({"basis": seen["revision"], "steps": [write]})).await;
                let reasons: Vec<String> = answer
                    .as_ref()
                    .err()
                    .into_iter()
                    .flat_map(|e| e["conflicts"].as_array().into_iter().flatten())
                    .filter_map(|c| c["reason"].as_str().map(str::to_owned))
                    .collect();
                // The four table insertions hold the whole table (table.cells, table.cols,
                // table.rows, xfrm; rests_tables.rs): cmd/table.rs's handler rewrites every cell's
                // merge flags and recomputes the frame size, so a change anywhere in the table,
                // here a row or column after the gap, is refused `changed` and nothing else for
                // today's handler. Narrowing them (the neighbour rows or columns by identity, the
                // merges crossing the gap, the frame size recomputed) is post-launch work
                // (features.yaml slide.agent.premises gap).
                let whole_table = family.starts_with("table.insert");
                let correct = if before {
                    reasons.iter().any(|r| r == "relocated")
                } else if whole_table {
                    answer.as_ref().err().is_some_and(|error| error["error"]["code"] == "conflict") && reasons == ["changed"]
                } else {
                    answer.is_ok()
                };
                println!("index: {family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok());
                if !correct {
                    failures.push(format!("{family} remove={remove} before={before}: applied={} reasons={reasons:?}", answer.is_ok()));
                }
                f.kits.shutdown().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
