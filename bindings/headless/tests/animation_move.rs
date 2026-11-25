//! slide-animation-move-narrow: `animation.move` holds only the item it takes (the window
//! `[index, index + 1)` of the slide's animations) and the gap it puts it into, never any
//! animation's content (cmd/animation.rs move_anim reads none). A separate writer edits the same
//! slide's animation list between the agent's observation and its write: what leaves both places
//! alone applies and the result is the intended order; what touches either place is refused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

const NAME: &str = "Moves.pptx";

/// One slide with six text boxes S0..S5, each with one `fade` animation A0..A5 in that order.
fn deck() -> Vec<u8> {
    let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
    doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
    for i in 0..6 {
        let made = doc.execute("textBox.insert", json!({"rect": [10, 10 + 60 * i, 200, 50], "text": format!("S{i}")})).unwrap();
        doc.execute("animation.add", json!({"ids": [made["id"]], "effect": "fade"})).unwrap();
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

    fn file(&self) -> PathBuf {
        self.workspace.path().join(NAME)
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &input, CallOptions::default(), "move-agent").await.unwrap() {
            Ok(answer) => Ok(answer.output),
            Err(error) => Err(error.structured()),
        }
    }

    /// Another writer saves `steps` on the slide (a new animation is added on S0 as `fly`).
    fn colleague(&self, steps: &[(&str, Value)]) {
        let mut doc = SlideDocument::open(NAME, &std::fs::read(self.file()).unwrap()).unwrap();
        doc.execute("slide.go", json!({"index": 0})).unwrap();
        for (id, params) in steps {
            let mut params = params.clone();
            if *id == "animation.add" {
                let part = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
                params["ids"] = json!([part["shapes"][0]["toolId"]]);
            }
            doc.execute(id, params).unwrap();
        }
        std::fs::write(self.file(), doc.save(Some(NAME)).unwrap()).unwrap();
    }

    /// The slide's animations as saved: (text of the animated shape, effect, duration).
    fn order(&self) -> Vec<(String, String, u32)> {
        let deck = kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(self.file()).unwrap()).unwrap();
        let slide = &deck.slides[0];
        slide
            .animations
            .iter()
            .map(|animation| {
                let shape = slide.shape(animation.shape).unwrap();
                (shape.text.as_ref().unwrap().text(), animation.effect.clone(), animation.duration_ms)
            })
            .collect()
    }
}

/// The agent moves A1 to 3: [A0, A2, A3, A1, A4, A5]. It holds the window [1, 2) (A1) and the gap
/// [4, 4] (between A3 and A4).
fn the_move() -> Value {
    json!({"command": "animation.move", "at": {"part": {"slide": 1}}, "params": {"index": 1, "to": 3}})
}

fn add_at(at: usize) -> [(&'static str, Value); 2] {
    [("animation.add", json!({"effect": "fly"})), ("animation.move", json!({"index": 6, "to": at}))]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_applies_beside_unrelated_edits_of_the_animation_list() {
    let bytes = deck();
    let timing = |index: usize| vec![("animation.timing", json!({"index": index, "duration": 1234}))];
    let moved = ["S0 fade", "S2 fade", "S3 fade", "S1 fade", "S4 fade", "S5 fade"];
    // (the other writer's change, the animation whose timing it set, the saved order)
    let cases: Vec<(&str, Vec<(&str, Value)>, Option<&str>, Vec<&str>)> = vec![
        // The moved animation's own content: carried whole, the other writer's timing kept.
        ("timing of the moved animation", timing(1), Some("S1"), moved.to_vec()),
        ("timing of an animation it passes", timing(2), Some("S2"), moved.to_vec()),
        ("timing of an animation before both places", timing(0), Some("S0"), moved.to_vec()),
        ("timing of a later animation", timing(5), Some("S5"), moved.to_vec()),
        ("removal after the gap", vec![("animation.remove", json!({"index": 5}))], None, moved[..5].to_vec()),
        ("insertion after the gap", add_at(5).to_vec(), None, vec!["S0 fade", "S2 fade", "S3 fade", "S1 fade", "S4 fade", "S0 fly", "S5 fade"]),
        ("insertion at the end", add_at(6).to_vec(), None, vec!["S0 fade", "S2 fade", "S3 fade", "S1 fade", "S4 fade", "S5 fade", "S0 fly"]),
    ];
    let mut failures = Vec::new();
    for (change, steps, edited, names) in cases {
        let f = Fixture::new(&bytes);
        let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
        f.colleague(&steps);
        let theirs = f.order();
        let timed = |order: &[(String, String, u32)]| edited.is_none_or(|shape| order.iter().any(|(text, _, ms)| text == shape && *ms == 1234));
        assert!(timed(&theirs), "{change}: the other writer's timing was not saved: {theirs:?}");
        // Their list with the item at 1 moved to 3: what they did stays, at the same places.
        let mut expected = theirs.clone();
        let item = expected.remove(1);
        expected.insert(3, item);
        let answer = f.call("run", json!({"basis": seen["revision"], "steps": [the_move()]})).await;
        println!("unrelated: {change}: applied={}", answer.is_ok());
        match answer {
            Ok(_) => {
                let saved = f.order();
                let labels: Vec<String> = saved.iter().map(|(text, effect, _)| format!("{text} {effect}")).collect();
                if saved != expected || labels != names || !timed(&saved) {
                    failures.push(format!("{change}: saved {saved:?}, expected {expected:?} ({names:?})"));
                }
            }
            Err(error) => failures.push(format!("{change}: refused {error}")),
        }
        f.kits.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_is_refused_when_its_item_or_its_gap_changed() {
    let bytes = deck();
    let cases: Vec<(&str, Vec<(&str, Value)>)> = vec![
        ("the moved animation deleted", vec![("animation.remove", json!({"index": 1}))]),
        ("the moved animation moved later", vec![("animation.move", json!({"index": 1, "to": 5}))]),
        ("the moved animation moved earlier", vec![("animation.move", json!({"index": 1, "to": 0}))]),
        ("an insertion into the gap", add_at(4).to_vec()),
        ("an insertion between the item and the gap", add_at(2).to_vec()),
        ("a removal between the item and the gap", vec![("animation.remove", json!({"index": 2}))]),
        ("a removal before the item", vec![("animation.remove", json!({"index": 0}))]),
        ("an insertion before the item", add_at(0).to_vec()),
    ];
    let mut failures = Vec::new();
    for (change, steps) in cases {
        let f = Fixture::new(&bytes);
        let seen = f.call("inspect", json!({"target": {"slide": 1}})).await.unwrap();
        f.colleague(&steps);
        let saved = std::fs::read(f.file()).unwrap();
        let answer = f.call("run", json!({"basis": seen["revision"], "steps": [the_move()]})).await;
        let reasons: Vec<String> = answer
            .as_ref()
            .err()
            .into_iter()
            .flat_map(|e| e["conflicts"].as_array().into_iter().flatten())
            .filter_map(|c| c["reason"].as_str().map(str::to_owned))
            .collect();
        println!("related: {change}: applied={} reasons={reasons:?}", answer.is_ok());
        let code = answer.as_ref().err().map(|error| error["error"]["code"].clone());
        // Refused by the windows (an item put in, taken out or shifted across a place), not by
        // any held content: `changed` would mean a content hold is back.
        let by_place = !reasons.is_empty() && reasons.iter().all(|reason| reason == "relocated" || reason == "deleted");
        if code != Some(json!("conflict")) || !by_place {
            failures.push(format!("{change}: applied={} code={code:?} reasons={reasons:?}", answer.is_ok()));
        } else if std::fs::read(f.file()).unwrap() != saved {
            failures.push(format!("{change}: refused but wrote the file"));
        }
        f.kits.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
