//! The premise guard on real decks (task card C8-slide-5): the measurements of
//! `bindings/headless/tests/outside.rs` on the ten-slide deck and on decks of the local corpus,
//! and fifty rounds of mixed editing.
//!
//! As in `outside.rs`, a colleague's program (this kit's engine, its own instance, sharing nothing
//! with the server under test) opens the deck from disk, edits it and writes it back; the agent
//! works through the kits' MCP tools (`kition_kits::tools::call_as`) on the `kition-slide`
//! executable, with the `revision` of its last answer as `basis`. Dev profile.
//!
//! - [`ten_writes_after_unrelated_changes_on_each_deck`], per deck: ten rounds of an unrelated
//!   colleague text edit followed by the agent's text write on a shape of another slide: applied or
//!   refused (reason), the bytes of `others`, and the median write time with no change by others,
//!   after an unrelated change, and when refused (the colleague edited the target).
//! - [`fifty_unrelated_changes_refuse_no_write_under_premise_and_every_write_under_epoch`], the
//!   false-refusal count of `outside.rs`'s test `l` on a deck.
//! - [`fifty_rounds_of_mixed_editing`], where each round the colleague makes one random change and the
//!   agent writes its target by shape id and by slide number and shape number; the table of
//!   outcomes, by the rule its doc states.
//!
//! The ten-slide deck runs always; the corpus decks when `KITION_SLIDE_CORPUS` names the
//! `golden_ppt` directory (slow: the 1,044-slide deck takes many minutes).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::ShapeKind;
use serde_json::{Value, json};

const ME: &str = "agent-me";
const NAME: &str = "Deck.pptx";
const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";

/// The corpus decks the measurements run on, relative to `KITION_SLIDE_CORPUS`.
const DECKS: &[&str] = &["shape/形状.pptx", "shape/并列5项2/并列5项2.pptx", "shape/列表1-2项/列表1-2项.pptx"];

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

/// The ten-slide deck of `outside.rs`: a title and a body per slide (slide 9's title has the
/// number of slide 2's), a picture on slide 10.
fn ten_slides() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    let mut titles = Vec::new();
    for n in 1..=10 {
        if n > 1 {
            doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        }
        let made = doc.execute("insert.textBox", json!({ "rect": [60, 60, 400, 80], "text": format!("Title {n}") })).unwrap();
        titles.push(made["id"].as_u64().unwrap());
        doc.execute("insert.textBox", json!({ "rect": [60, 300, 400, 80], "text": format!("Body {n}") })).unwrap();
    }
    doc.execute("insert.picture", json!({ "name": "red.png", "data": RED })).unwrap();
    let mut file = kition_slide_engine::cmd::file::open_slide_file(NAME, &doc.save(Some(NAME)).unwrap()).unwrap();
    let ninth = std::sync::Arc::make_mut(&mut file.slides[8]);
    for shape in &mut ninth.shapes {
        if u64::from(shape.id.0) == titles[8] {
            shape.id = kition_slide_model::ShapeId(u32::try_from(titles[1]).unwrap());
        }
    }
    kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()
}

/// A colleague's program: this kit's engine, its own instance, opening the file from disk, running
/// `steps` and saving the file back, an hour earlier, so only the content tells that it changed.
fn colleague(file: &Path, steps: &[(&str, Value)]) {
    let mut doc = SlideDocument::open(NAME, &std::fs::read(file).unwrap()).unwrap();
    for (command, params) in steps {
        doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("colleague's {command}: {error}"));
    }
    let at = std::fs::metadata(file).unwrap().modified().unwrap() - Duration::from_secs(3600);
    std::fs::write(file, doc.save(Some(NAME)).unwrap()).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(at).unwrap();
}

struct Fixture {
    kits: KitManager,
    options: CallOptions,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new(bytes: &[u8], guard: Guard) -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(NAME), bytes).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, options: CallOptions { guard, ..CallOptions::default() }, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(NAME)
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, self.options, ME).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("slide_{action} is no tool"),
        }
    }

    /// Slide `n` (from 1) as the agent inspects it, and the answer's revision.
    async fn inspect(&self, n: u64) -> (Value, String) {
        let seen = self.call("inspect", json!({ "target": { "slide": n } })).await.unwrap_or_else(|error| panic!("inspect: {error}"));
        (seen["part"].clone(), revision(&seen))
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

fn set_text(id: &str, text: &str) -> Value {
    json!({ "command": "text.set", "params": { "id": id, "text": text } })
}

/// The ids `others` lists, in all four lists.
fn others_ids(others: &Value) -> Vec<String> {
    ["created", "changed", "moved", "deleted"]
        .iter()
        .flat_map(|list| others[*list].as_array().cloned().unwrap_or_default())
        .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
        .collect()
}

/// The text shapes of a deck an agent can write with `text.set` (autoshapes and text boxes with
/// text, at the top of their slide), by tool id, slide by slide.
fn text_shapes(bytes: &[u8]) -> Vec<Vec<String>> {
    let file = kition_slide_engine::cmd::file::open_slide_file(NAME, bytes).unwrap();
    file.slides
        .iter()
        .map(|slide| {
            slide
                .shapes
                .iter()
                .filter(|shape| matches!(shape.kind, ShapeKind::Shape) && shape.text.as_ref().is_some_and(|body| !body.text().trim().is_empty()))
                .map(|shape| format!("shape:{}", shape.identity))
                .collect()
        })
        .collect()
}

fn median(times: &mut [Duration]) -> f64 {
    if times.is_empty() {
        return f64::NAN;
    }
    times.sort();
    times[times.len() / 2].as_secs_f64() * 1000.0
}

/// A fixed sequence of numbers below a bound (xorshift; the seed is printed with each table).
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(bound.max(1)).unwrap()).unwrap()
    }
}

/// The decks to measure: the ten-slide deck, and the corpus decks when the corpus is named.
fn decks() -> Vec<(String, Vec<u8>)> {
    let mut out = vec![("ten slides".to_owned(), ten_slides())];
    if let Ok(dir) = std::env::var("KITION_SLIDE_CORPUS") {
        let only = std::env::var("KITION_SLIDE_CORPUS_ONLY").ok();
        for deck in DECKS.iter().filter(|deck| only.as_deref().is_none_or(|only| deck.contains(only))) {
            match std::fs::read(Path::new(&dir).join(deck)) {
                Ok(bytes) => out.push(((*deck).to_owned(), bytes)),
                Err(_) => println!("{deck}: not in the corpus"),
            }
        }
    }
    out
}

/// Per deck, ten rounds: the colleague sets the text of a shape on another slide than the agent's
/// target, then the agent sets its target's text with the revision of its last answer. Wanted:
/// ten of ten applied, and `others` naming the colleague's shape alone. Prints per deck the
/// outcome of each round, the bytes of `others`, and the median write time with no change by
/// others (five writes), after an unrelated change (the ten rounds) and when refused (five rounds
/// in which the colleague set the target's own text).
#[tokio::test(flavor = "multi_thread")]
async fn ten_writes_after_unrelated_changes_on_each_deck() {
    let mut failures = Vec::new();
    for (deck, bytes) in decks() {
        let shapes = text_shapes(&bytes);
        let slides: Vec<usize> = (0..shapes.len()).filter(|index| !shapes[*index].is_empty()).collect();
        assert!(slides.len() >= 2, "{deck}: two slides with text");
        let f = Fixture::new(&bytes, Guard::Premise);
        let started = Instant::now();
        let (_, mut basis) = f.inspect(1).await;
        let opened = started.elapsed();
        // Targets: the first text shape of ten slides spread over the deck; the colleague's: the
        // last text shape of the slide after each.
        let pick = |round: usize| slides[(round * slides.len() / 10) % slides.len()];
        let mut plain = Vec::new();
        for round in 0..5 {
            let mine = &shapes[pick(round)][0];
            let at = Instant::now();
            let ran = f.call("run", json!({ "basis": basis, "steps": [set_text(mine, &format!("Plain {round}"))] })).await;
            plain.push(at.elapsed());
            basis = revision(ran.as_ref().unwrap_or_else(|error| panic!("{deck}: plain write: {error}")));
        }
        let mut after = Vec::new();
        let mut outcomes = Vec::new();
        let mut sizes = Vec::new();
        for round in 0..10 {
            let index = pick(round);
            let mine = shapes[index][0].clone();
            let other_slide = slides[(slides.iter().position(|s| *s == index).unwrap() + 1) % slides.len()];
            let theirs = shapes[other_slide].last().unwrap().clone();
            colleague(&f.file(), &[("text.set", json!({ "id": theirs, "text": format!("Colleague {round}") }))]);
            let at = Instant::now();
            let answer = f.call("run", json!({ "basis": basis, "steps": [set_text(&mine, &format!("Mine {round}"))] })).await;
            after.push(at.elapsed());
            match answer {
                Ok(ran) => {
                    let others = &ran["others"];
                    let size = others.to_string().len();
                    sizes.push(size);
                    let named = others_ids(others);
                    let alone = named == [theirs.clone()] && others.get("unnamed").is_none() && others.get("renumbered").is_none();
                    outcomes.push(if alone {
                        "applied".to_owned()
                    } else {
                        format!("applied, others also {named:?} {}", others.get("unnamed").cloned().unwrap_or(Value::Null))
                    });
                    if !alone {
                        failures.push(format!("{deck} round {round}: others names more than the colleague's {theirs}: {}", cut(others)));
                    }
                    basis = revision(&ran);
                }
                Err(error) => {
                    let reasons: Vec<String> = error["conflicts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|c| format!("{} {}", c["reason"].as_str().unwrap_or("?"), c["id"]))
                        .collect();
                    outcomes.push(format!("refused {} ({})", error["error"]["code"], reasons.join(", ")));
                    failures.push(format!("{deck} round {round}: refused: {}", cut(&error)));
                    sizes.push(error["others"].to_string().len());
                    basis = error["revision"].as_str().map_or(basis, str::to_owned);
                }
            }
        }
        let mut refused = Vec::new();
        for round in 0..5 {
            let mine = shapes[pick(round)][0].clone();
            colleague(&f.file(), &[("text.set", json!({ "id": mine, "text": format!("Colleague's own {round}") }))]);
            let at = Instant::now();
            let answer = f.call("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine again")] })).await;
            refused.push(at.elapsed());
            match answer {
                Err(error) if error["error"]["code"] == "conflict" => basis = revision(&error),
                other => {
                    failures.push(format!("{deck}: a write on the colleague's target was not refused: {other:?}"));
                    basis = other.map(|ran| revision(&ran)).unwrap_or(basis);
                }
            }
        }
        // Receipts of a reorder, an insert and a delete of slides: their bytes, and whether a
        // footprint states a shift (slides have no `position` field: their place is their index).
        let last = shapes.len() - 1;
        for (what, step) in [
            ("slide.move last to first", json!({ "command": "slide.move", "params": { "from": last, "to": 0 } })),
            ("slide.new at 0", json!({ "command": "slide.new", "params": { "at": 0 } })),
            ("slide.delete index 0", json!({ "command": "slide.delete", "params": { "index": 0 } })),
        ] {
            let ran = f.call("run", json!({ "basis": basis, "steps": [step] })).await.unwrap_or_else(|error| panic!("{deck}: {what}: {error}"));
            let changes = &ran["changes"];
            println!(
                "{deck}: receipt of {what}: {} bytes, footprint {}, created {} changed {} moved {} deleted {}",
                changes.to_string().len(),
                changes.get("footprint").map_or("none".to_owned(), Value::to_string),
                changes["created"].as_array().map_or(0, Vec::len),
                changes["changed"].as_array().map_or(0, Vec::len),
                changes["moved"].as_array().map_or(0, Vec::len),
                changes["deleted"].as_array().map_or(0, Vec::len)
            );
            basis = revision(&ran);
        }
        let applied = outcomes.iter().filter(|outcome| outcome.starts_with("applied")).count();
        println!("{deck}: {} slides; first inspect {:.0} ms", shapes.len(), opened.as_secs_f64() * 1000.0);
        println!("{deck}: {applied}/10 applied; rounds: {outcomes:?}");
        println!("{deck}: others bytes per round: {sizes:?}");
        println!(
            "{deck}: median write: plain {:.0} ms, after an unrelated change {:.0} ms, refused {:.0} ms",
            median(&mut plain),
            median(&mut after),
            median(&mut refused)
        );
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn cut(value: &Value) -> String {
    let text = value.to_string();
    if text.len() > 600 { format!("{}…", text.chars().take(600).collect::<String>()) } else { text }
}

/// Fifty rounds on the ten-slide deck and (with the corpus) the 200-slide deck, under the premise
/// guard and under `--guard epoch`: the colleague sets the text of a shape picked at random (seed
/// printed) among those of the other slides than the agent's target, then the agent sets its
/// target's text with the revision of its last answer. Wanted: premise refuses none, epoch every
/// one.
#[tokio::test(flavor = "multi_thread")]
async fn fifty_unrelated_changes_refuse_no_write_under_premise_and_every_write_under_epoch() {
    const ROUNDS: usize = 50;
    const SEED: u64 = 0x2545_F491_4F6C_DD1D;
    let mut decks = vec![("ten slides".to_owned(), ten_slides())];
    if let Ok(dir) = std::env::var("KITION_SLIDE_CORPUS") {
        let deck = "shape/并列5项2/并列5项2.pptx";
        if let Ok(bytes) = std::fs::read(Path::new(&dir).join(deck)) {
            decks.push((deck.to_owned(), bytes));
        }
    }
    let mut failures = Vec::new();
    for (deck, bytes) in decks {
        let shapes = text_shapes(&bytes);
        let target_slide = shapes.iter().position(|shapes| !shapes.is_empty()).unwrap();
        let mine = shapes[target_slide][0].clone();
        let theirs: Vec<String> = shapes.iter().enumerate().filter(|(index, _)| *index != target_slide).flat_map(|(_, ids)| ids.clone()).collect();
        for (guard, name) in [(Guard::Premise, "premise"), (Guard::Epoch, "epoch")] {
            let f = Fixture::new(&bytes, guard);
            let (_, mut basis) = f.inspect(1).await;
            let mut rng = Rng(SEED);
            let mut refused = 0;
            let mut noisy = 0;
            let started = Instant::now();
            for round in 0..ROUNDS {
                let other = &theirs[rng.below(theirs.len())];
                colleague(&f.file(), &[("text.set", json!({ "id": other, "text": format!("Colleague {round}") }))]);
                match f.call("run", json!({ "basis": basis, "steps": [set_text(&mine, &format!("Mine {round}"))] })).await {
                    Ok(ran) => {
                        if others_ids(&ran["others"]) != [other.clone()] {
                            noisy += 1;
                        }
                        basis = revision(&ran);
                    }
                    Err(error) => {
                        assert_eq!(error["error"]["code"], "conflict", "{error}");
                        refused += 1;
                        basis = revision(&error);
                    }
                }
            }
            println!(
                "{deck}: --guard {name}: {refused} of {ROUNDS} writes refused after an unrelated file change (seed {SEED:#x}); {noisy} answers whose others named more than the colleague's shape; {:.1} s",
                started.elapsed().as_secs_f64()
            );
            let wanted = if guard == Guard::Premise { 0 } else { ROUNDS };
            if refused != wanted {
                failures.push(format!("{deck}: --guard {name}: {refused} of {ROUNDS} refused, wanted {wanted}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What the colleague does in a round of [`fifty_rounds_of_mixed_editing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Theirs {
    /// Sets the text of a shape on another slide than the target's.
    EditOther,
    /// Inserts a slide before the target's slide.
    InsertBefore,
    /// Inserts a slide after the target's slide.
    InsertAfter,
    /// Deletes the other shape on the target's slide (its title).
    DeleteOther,
    /// Moves the other shape on the target's slide.
    MoveOther,
    /// Duplicates a slide picked at random (before or after the target's).
    Duplicate,
}

/// Fifty rounds on a fresh ten-slide deck each: the agent inspects slide 5, the colleague makes
/// one random change ([`Theirs`], seed printed), and the agent, with the revision of its inspect,
/// (A) sets the text of its target, slide 5's body, by its tool id and (B) sets it again by slide
/// number and shape number (`at.part {slide: 5}`, `id` the body's number). Rule for "necessary":
/// (A) is necessary to refuse when the colleague changed or deleted the target (none of these
/// changes does); (B) when slide 5 is no longer the slide the agent inspected (a slide inserted or
/// duplicated before it), since it would then write another slide's shape. "Applied wrongly": (B)
/// applied when it was necessary to refuse it. Prints the table (their change × write → rounds,
/// applied, refused necessary, refused false, applied wrongly, reasons).
#[tokio::test(flavor = "multi_thread")]
async fn fifty_rounds_of_mixed_editing() {
    const ROUNDS: usize = 50;
    const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
    const TARGET: u64 = 5;
    let kinds = [Theirs::EditOther, Theirs::InsertBefore, Theirs::InsertAfter, Theirs::DeleteOther, Theirs::MoveOther, Theirs::Duplicate];
    type Cell = (usize, usize, usize, usize, usize, BTreeMap<String, usize>);
    let mut table: BTreeMap<(Theirs, char), Cell> = BTreeMap::new();
    let mut rng = Rng(SEED);
    let bytes = ten_slides();
    for round in 0..ROUNDS {
        let kind = kinds[rng.below(kinds.len())];
        let f = Fixture::new(&bytes, Guard::Premise);
        let (part, basis) = f.inspect(TARGET).await;
        let slide_tool = part["toolId"].as_str().unwrap().to_owned();
        let body = part["shapes"].as_array().unwrap().iter().find(|shape| shape["text"] == format!("Body {TARGET}")).unwrap().clone();
        let title = part["shapes"].as_array().unwrap().iter().find(|shape| shape["text"] == format!("Title {TARGET}")).unwrap().clone();
        let mine = body["toolId"].as_str().unwrap().to_owned();
        let number = body["id"].clone();
        let (theirs, slide_moved) = match kind {
            Theirs::EditOther => {
                let mut n = 1 + rng.below(9);
                if n >= TARGET as usize {
                    n += 1;
                }
                let (other, _) = f.inspect(n as u64).await;
                let id = other["shapes"][0]["toolId"].as_str().unwrap().to_owned();
                (("text.set", json!({ "id": id, "text": format!("Colleague {round}") })), false)
            }
            Theirs::InsertBefore => (("slide.new", json!({ "layout": "blank", "at": rng.below(TARGET as usize) })), true),
            Theirs::InsertAfter => (("slide.new", json!({ "layout": "blank", "at": TARGET as usize + rng.below(10 - TARGET as usize + 1) })), false),
            Theirs::DeleteOther => (("drawing.delete", json!({ "ids": [title["toolId"]] })), false),
            Theirs::MoveOther => (("shape.move", json!({ "ids": [title["toolId"]], "dx": 12, "dy": 7 })), false),
            Theirs::Duplicate => {
                let index = rng.below(10);
                // A copy of a slide before the target's goes in before it.
                (("slide.duplicate", json!({ "index": index })), index < TARGET as usize - 1)
            }
        };
        colleague(&f.file(), std::slice::from_ref(&theirs));
        let a = f.call("run", json!({ "basis": basis, "steps": [set_text(&mine, "Mine by id")] })).await;
        let b = f
            .call(
                "run",
                json!({ "basis": basis, "steps": [{ "command": "text.set", "at": { "part": { "slide": TARGET } }, "params": { "id": number, "text": "Mine by number" } }] }),
            )
            .await;
        let mut writes = vec![('A', a, false), ('B', b, slide_moved)];
        if matches!(kind, Theirs::InsertBefore | Theirs::InsertAfter) {
            let c =
                f.call("run", json!({"basis": basis, "steps": [{"command": "slide.new", "params": {"at": TARGET - 1, "layout": "blank"}}]})).await;
            writes.push(('C', c, slide_moved));
        }
        for (write, answer, necessary) in writes {
            let cell = table.entry((kind, write)).or_default();
            cell.0 += 1;
            match answer {
                Ok(_) if necessary => cell.4 += 1,
                Ok(_) => cell.1 += 1,
                Err(error) => {
                    assert_eq!(error["error"]["code"], "conflict", "{theirs:?}: {error}");
                    let reasons: Vec<String> = error["conflicts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| format!("{}{}", c["reason"].as_str().unwrap(), if c["id"] == json!(slide_tool) { " (the slide)" } else { "" }))
                        .collect();
                    *cell.5.entry(reasons.join("+")).or_default() += 1;
                    if necessary {
                        cell.2 += 1;
                    } else {
                        cell.3 += 1;
                        println!("mixed: false refusal of {write} after {theirs:?}: {}", error["conflicts"]);
                    }
                }
            }
        }
        f.shutdown().await;
    }
    println!(
        "mixed (seed {SEED:#x}, {ROUNDS} rounds): their change | write | rounds | applied | refused, necessary | refused, false | applied, wrongly | reasons"
    );
    let mut wrong = 0;
    let mut false_refusals = 0;
    for ((kind, write), (rounds, applied, needed, falsely, wrongly, reasons)) in &table {
        println!("mixed: {kind:?} | {write} | {rounds} | {applied} | {needed} | {falsely} | {wrongly} | {reasons:?}");
        wrong += wrongly;
        false_refusals += falsely;
    }
    assert_eq!(wrong, 0, "no write accepted that wrote another slide's shape");
    assert_eq!(false_refusals, 0, "no right write refused");
}
