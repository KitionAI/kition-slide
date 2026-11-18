//! C8-slide-pin: raw prefixes may be incomplete only in an isolated premise plan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::json;

#[test]
fn detached_prefix_is_only_allowed_in_an_unsaveable_isolated_plan() {
    let created = SlideDocument::create("Plan.kslide", 960, 540).unwrap();
    let bytes = created.save(None).unwrap();
    let mut source = SlideDocument::open("Plan.kslide", &bytes).unwrap();
    let before = Exchange::new().replica(&mut source).unwrap().state_json().unwrap();
    let detached = json!({"origin":{"kind":"agent", "principal":"planner"}, "ops":[
        {"op":"create", "type":"Shape", "ref":"$new", "fields":{"kind":"shape", "name":"Pending"}}
    ]});
    let error = Exchange::new().batch(&mut source, &detached, false).unwrap_err();
    assert!(error.message.contains("no stored container"), "{error:?}");
    assert_eq!(Exchange::new().replica(&mut source).unwrap().state_json().unwrap(), before);
    let mut plan = source.fork_for_premises().unwrap();
    assert_eq!(Exchange::new().replica(&mut plan).unwrap().state_json().unwrap(), before);
    Exchange::new().batch(&mut plan, &detached, false).unwrap();
    assert!(plan.save(None).is_err(), "a temporary detached object must never be saved");
    let destination = std::env::temp_dir().join(format!("kition-slide-premise-plan-{}.kslide", std::process::id()));
    let result = plan.execute("file.save", json!({"path":destination}));
    let wrote = destination.exists();
    if wrote {
        std::fs::remove_file(&destination).unwrap();
    }
    assert!(result.is_err() && !wrote, "a plan must never write a file through a command: {result:?}");
    assert_eq!(Exchange::new().replica(&mut source).unwrap().state_json().unwrap(), before);
    assert_eq!(source.save(None).unwrap(), bytes);
}
