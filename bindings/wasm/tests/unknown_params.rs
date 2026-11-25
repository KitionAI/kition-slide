//! A command with a parameter schema refuses a key the schema does not name, and says which
//! (platform rule E7a, kit requirement AX15): an agent that misspells a parameter is told instead
//! of the command running without it.
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;

fn doc() -> SlideDocument {
    SlideDocument::create("Deck.kslide", 960, 540).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn every_command_refuses_an_unknown_parameter() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(doc, &[]) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// With `KITION_COMMANDS_DUMP` set to a file, writes the catalogue there: what callers are checked
/// against when auditing the keys commands take.
#[test]
fn dump_commands() {
    if let Ok(path) = std::env::var("KITION_COMMANDS_DUMP") {
        std::fs::write(path, serde_json::to_string(&doc().commands()).unwrap()).unwrap();
    }
}

#[test]
fn keys_commands_read_are_in_their_schemas() {
    // Keys the commands read (crates/engine/engine/src/cmd), so a call with them is not refused.
    let read: &[(&str, &[&str])] = &[
        ("edit.cut", &["ids", "id"]),
        ("edit.delete", &["id"]),
        ("edit.select", &["id"]),
        ("shape.delete", &["id"]),
        ("drawing.describe", &["id"]),
        ("media.gifPlay", &["ids"]),
        ("format.paragraph", &["lineSpacingPt"]),
        ("slide.background", &["pattern"]),
        ("slide.size", &["w", "h"]),
        ("show.setup", &["all"]),
        ("shape.insert", &["adj", "flip"]),
        ("table.insert", &["data"]),
        ("chart.insert", &["title"]),
        ("animation.set", &["start", "duration", "delay"]),
        ("file.saveTemplate", &["format"]),
        // drawing.* hand their parameters to kition-office-core's DrawingOp::parse.
        ("drawing.insert", &["placement", "body", "warp"]),
        ("drawing.delete", &["id"]),
        ("drawing.transform", &["id"]),
        ("drawing.format", &["id", "effects"]),
        ("drawing.chart", &["ids", "seriesFormat", "xAxis", "yAxis", "dataLabelPosition"]),
    ];
    let commands = doc().commands();
    let mut missing = Vec::new();
    for (id, keys) in read {
        let schema = commands.iter().find(|command| command.id == *id).and_then(|command| command.schema.clone()).unwrap();
        missing.extend(keys.iter().filter(|key| schema.get(**key).is_none()).map(|key| format!("{id} {key}")));
    }
    assert!(missing.is_empty(), "keys the commands read that their schemas do not name: {missing:?}");
}
