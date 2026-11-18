//! Every command says what it does, and read commands declare what they answer
//! (`kition_kit::testing::check_commands`, kition-sdk W1-46 P2-1).
use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn sample() -> SlideDocument {
    SlideDocument::open("basic.kslide", include_bytes!("../../../conformance/basic.kslide")).unwrap()
}

#[test]
fn every_command_says_what_it_does() {
    if let Err(problems) = kition_kit::testing::check_commands(&sample().commands(), kition_slide_kit::described::NO_OUTPUT) {
        panic!("{}", problems.join("\n"));
    }
}

/// Parameters a read command needs on `basic.kslide`.
fn sample_params(id: &str) -> Value {
    match id {
        "slide.go" => json!({ "index": 0 }),
        "shape.sites" | "text.get" | "shape.inspect" => json!({ "id": 260 }),
        "text.edit" => json!({ "id": 260 }),
        "media.seek" => json!({ "ms": 0 }),
        _ => json!({}),
    }
}

#[test]
fn every_schema_is_valid() {
    for command in sample().commands() {
        if let Some(schema) = &command.schema {
            kition_kit::schema::validate(schema).unwrap_or_else(|error| panic!("{}: {error}", command.id));
        }
        if let Some(schema) = &command.output_schema {
            kition_kit::schema::validate_output(schema).unwrap_or_else(|error| panic!("{}: {error}", command.id));
        }
    }
}

/// Read commands answer what they declare. A command whose precondition `basic.kslide` lacks (no
/// media, no recovery folder) is left out.
#[test]
fn read_commands_answer_what_they_declare() {
    for command in sample().commands() {
        let Some(output) = command.output_schema.clone() else { continue };
        if command.id.starts_with("media.") || command.id == "file.recovery.list" {
            continue;
        }
        let mut doc = sample();
        // Commands that act on the selection find the sample's text box selected.
        doc.execute("edit.select", json!({ "ids": [260] })).unwrap();
        let answer = doc.execute(&command.id, sample_params(&command.id)).unwrap_or_else(|error| panic!("{}: {error}", command.id));
        let schema = json!({ "answer": output });
        let answer = serde_json::Map::from_iter([("answer".to_owned(), answer)]);
        if let Err(error) = kition_kit::schema::check_params(&schema, &answer) {
            panic!("{}: {error}", command.id);
        }
    }
}
