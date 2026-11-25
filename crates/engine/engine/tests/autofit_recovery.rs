//! A shrink-on-overflow fit stays current through duplication, so a recovered replica has nothing
//! to repair.

use kition_slide_engine::{Session, Version};
use serde_json::json;

#[test]
fn duplicated_placeholder_has_a_current_fit_before_crash_recovery() {
    let mut original = Session::with_new();
    let title = original.doc().unwrap().doc.slides[0].shapes[0].id;
    original.execute("text.set", &json!({"id": title.0, "text": "Inherited title metrics"})).unwrap();
    original.doc_mut().unwrap().selection.shapes = vec![title];
    original.execute("format.autofit", &json!({"mode": "shrink"})).unwrap();
    let source_input = original.doc().unwrap().shape(title).unwrap().text.as_ref().unwrap().body.autofit_input.clone();
    assert!(source_input.is_some());
    original.execute("edit.duplicate", &json!({"ids": [title.0]})).unwrap();
    let copy = original.doc().unwrap().selection.shapes[0];
    assert!(original.doc().unwrap().shape(copy).unwrap().ph.is_none(), "duplication detaches a placeholder from its inherited styles");
    let before = original.state_hash().unwrap();
    let version = original.version().unwrap();
    let mut recovered = Session::new();
    recovered.open_snapshot(&original.snapshot().unwrap(), original.blobs_since(&Version::empty()).unwrap()).unwrap();
    assert_eq!(recovered.state_hash().unwrap(), before, "recovery must not discover an invalid fit left by duplication");
    assert_eq!(recovered.version().unwrap(), version);
}
