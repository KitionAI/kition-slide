//! `file.recovery.discard` removes exactly the AutoRecover entry whose `uid` `file.recovery.list`
//! answered, refuses a `uid` that is not one of them (of the wrong type, or of no entry) and then
//! discards nothing, and discards every entry only when `uid` is left out (case
//! regressions/issue-recovery-discard-uid).
//!
//! A uid is the number `DocState::uid` gives each open slide file (`<uid>.kslide` and `<uid>.json` in
//! the recovery folder, `crates/engine/engine/src/recovery.rs`). The calls go the way an agent's call
//! goes: the host checks the parameters against the schema this kit declares for the command
//! (`kition_kit::schema::refuse_unknown`, then `check_params`, as kition-headless does) before the
//! engine runs it. The kit's document sets no recovery folder, so the engine session the kit runs
//! on is given one here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_engine::Session;
use kition_slide_kit::SlideDocument;
use serde_json::{Map, Value, json};

/// The parameter schema and output schema the kit declares for each command.
fn declared() -> Vec<(String, Option<Value>, Option<Value>)> {
    let doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap_or_else(|error| panic!("{error}"));
    doc.commands().into_iter().map(|command| (command.id, command.schema, command.output_schema)).collect()
}

/// Runs `id` as an agent's call: the declared schema first, then the engine.
fn call(session: &mut Session, commands: &[(String, Option<Value>, Option<Value>)], id: &str, params: Value) -> Result<Value, String> {
    let (_, schema, _) = commands.iter().find(|(name, ..)| name == id).unwrap_or_else(|| panic!("no command {id}"));
    if let Some(schema) = schema {
        kition_kit::schema::refuse_unknown(id, schema, &params)?;
        kition_kit::schema::check_params(schema, params.as_object().unwrap_or(&Map::new()))?;
    }
    session.execute(id, &params).map_err(|error| error.to_string())
}

/// A session with three unsaved slide files whose AutoRecover entries are in a new folder.
fn three_entries(name: &str, commands: &[(String, Option<Value>, Option<Value>)]) -> (Session, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("kition-slide-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut session = Session::new();
    session.recovery_dir = Some(dir.clone());
    for _ in 0..3 {
        call(&mut session, commands, "file.new", json!({})).unwrap();
        call(&mut session, commands, "slide.new", json!({ "layout": "blank" })).unwrap();
    }
    let written = call(&mut session, commands, "file.recovery.save", json!({})).unwrap();
    assert_eq!(written["written"], json!(3), "{written}");
    (session, dir)
}

fn listed(session: &mut Session, commands: &[(String, Option<Value>, Option<Value>)]) -> Vec<Value> {
    let list = call(session, commands, "file.recovery.list", json!({})).unwrap();
    list.as_array().cloned().unwrap_or_else(|| panic!("file.recovery.list answered {list}"))
}

fn uids(entries: &[Value]) -> Vec<Value> {
    entries.iter().map(|entry| entry["uid"].clone()).collect()
}

#[test]
fn discarding_the_listed_uid_removes_that_entry_only() {
    let commands = declared();
    let (mut session, dir) = three_entries("discard-one", &commands);
    let entries = listed(&mut session, &commands);
    assert_eq!(entries.len(), 3, "{entries:?}");
    let all = uids(&entries);
    let chosen = all[1].clone();
    call(&mut session, &commands, "file.recovery.discard", json!({ "uid": chosen }))
        .unwrap_or_else(|error| panic!("discarding the uid {chosen} as file.recovery.list answered it was refused: {error}"));
    let left = uids(&listed(&mut session, &commands));
    assert_eq!(left, vec![all[0].clone(), all[2].clone()], "discarding {chosen} of {all:?} left {left:?}");
    // What the list answers has the types its declared output schema gives.
    let (_, _, output) = commands.iter().find(|(id, ..)| id == "file.recovery.list").unwrap();
    let item =
        output.as_ref().and_then(|schema| schema["items"]["properties"].as_object()).expect("file.recovery.list declares its items' properties");
    for entry in &entries {
        let errors = kition_kit::schema::param_errors(&Value::Object(item.clone()), entry.as_object().unwrap());
        assert!(errors.is_empty(), "file.recovery.list answered {entry}, which its output schema does not describe: {errors:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_uid_of_the_wrong_type_or_of_no_entry_discards_nothing() {
    let commands = declared();
    let (mut session, dir) = three_entries("discard-refused", &commands);
    let all = uids(&listed(&mut session, &commands));
    let first = all[0].as_u64().map(|uid| uid.to_string()).unwrap_or_else(|| all[0].to_string());
    let missing = all.iter().filter_map(Value::as_u64).max().unwrap_or(0) + 1000;
    let wrong = [json!(first), json!("all"), json!(""), json!(-1), json!(1.5), json!(true), json!([all[0]]), Value::Null, json!(missing)];
    for uid in wrong {
        let answer = call(&mut session, &commands, "file.recovery.discard", json!({ "uid": uid }));
        assert!(answer.is_err(), "file.recovery.discard with uid {uid} was accepted: {answer:?}");
        let left = uids(&listed(&mut session, &commands));
        assert_eq!(left, all, "file.recovery.discard with uid {uid} discarded entries");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_uid_discards_every_entry() {
    let commands = declared();
    let (mut session, dir) = three_entries("discard-all", &commands);
    assert_eq!(listed(&mut session, &commands).len(), 3);
    call(&mut session, &commands, "file.recovery.discard", json!({})).unwrap();
    assert!(listed(&mut session, &commands).is_empty(), "entries left after discarding all");
    let _ = std::fs::remove_dir_all(&dir);
}
