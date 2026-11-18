//! C8-slide-7: diagnostics name stored objects before and after a write alike.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::diagnostics::Severity;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn errors(doc: &SlideDocument) -> Vec<Value> {
    doc.check().into_iter().filter(|d| d.severity == Severity::Error).map(|d| json!({"code": d.code, "target": d.target})).collect()
}

#[test]
fn existing_media_diagnostics_keep_their_identity_after_replica_initialization() {
    for (name, bytes) in [
        ("check-missing-media.kslide", include_bytes!("../../../conformance/check-missing-media.kslide").as_slice()),
        ("check-unreadable-picture.kslide", include_bytes!("../../../conformance/check-unreadable-picture.kslide").as_slice()),
    ] {
        let mut doc = SlideDocument::open(name, bytes).unwrap();
        let before = errors(&doc);
        assert!(!before.is_empty());
        let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap()["toolId"].clone();
        Exchange::new().batch(&mut doc, &json!({"ops": [{"op": "set", "target": first, "field": "props.name", "value": "Renamed"}]}), false).unwrap();
        assert_eq!(before, errors(&doc), "{name}: rename introduced a diagnostic");
        let again = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
        assert_eq!(before, errors(&again), "{name}: reopening renamed the diagnostic target");
    }
}
