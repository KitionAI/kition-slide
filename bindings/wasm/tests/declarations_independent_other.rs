//! The media read set is asserted independently of the file-change journal's SDK limits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::json;

#[test]
fn poster_holds_referenced_blob_for_explicit_and_discovered_targets() {
    let mut doc = SlideDocument::create("Poster.pptx", 960, 540).unwrap();
    doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
    let bytes = include_bytes!("../../../crates/media/media/tests/fixtures/clip-h264.mp4");
    doc.execute("insert.video", json!({"name": "clip.mp4", "data": kition_slide_engine::cmd::base64_encode(bytes)})).unwrap();
    doc.execute("slide.go", json!({"index": 1})).unwrap();
    let state = Exchange::new().replica(&mut doc).unwrap().state_json().unwrap();
    let shape = state["objects"].as_object().unwrap().values().find(|object| object["$type"] == "Shape" && object["kind"] == "media").unwrap();
    let media = shape["media"].as_str().unwrap();
    let target = format!("shape:{}", shape["key"].as_str().unwrap());
    for params in [json!({"id": target, "ms": 0}), json!({"ms": 0})] {
        let under = doc.under("media.posterFrame", &params).unwrap().unwrap();
        assert!(
            under.premises.iter().any(|premise| premise.id == json!(media)
                && premise.fields.as_deref() == Some(&["blob".to_owned()])
                && premise.positions.as_ref().is_some_and(Vec::is_empty)),
            "missing referenced video bytes: {under:?}"
        );
        assert!(under.premises.iter().any(|premise| premise.id == json!(target)), "missing resolved media shape: {under:?}");
    }
}
