//! C8-slide-7: opening settles the state agents observe before their first write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn state(doc: &mut SlideDocument) -> Value {
    Exchange::new().replica(doc).unwrap().state_json().unwrap()
}

#[test]
fn an_unrelated_write_does_not_refit_empty_placeholders() {
    let name = "check-empty-placeholder.kslide";
    let mut doc = SlideDocument::open(name, include_bytes!("../../../conformance/check-empty-placeholder.kslide")).unwrap();
    let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap()["toolId"].clone();
    let before = state(&mut doc);
    Exchange::new().batch(&mut doc, &json!({"ops": [{"op": "set", "target": first, "field": "props.name", "value": "Renamed"}]}), false).unwrap();
    let after = state(&mut doc);
    let mut wanted = before;
    let slide =
        wanted["objects"].as_object_mut().unwrap().values_mut().find(|v| format!("slide:{}", v["slideId"]) == first.as_str().unwrap()).unwrap();
    slide["props"]["name"] = json!("Renamed");
    for (id, object) in wanted["objects"].as_object().unwrap() {
        assert_eq!(object, &after["objects"][id], "renaming a slide must only rename that slide: {id}");
    }
}
