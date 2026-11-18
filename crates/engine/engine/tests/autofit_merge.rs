use kition_slide_engine::{Session, Version};
use kition_slide_model::{
    Shape,
    resolve::{self, Ctx},
};
use kition_slide_text::{NoFields, Opts, TextLayout};
use serde_json::json;

fn layout(s: &Session, legacy: bool) -> TextLayout {
    let doc = &s.doc().unwrap().doc;
    let slide = &doc.slides[0];
    let mut shape: Shape = slide.shapes.last().unwrap().clone();
    if legacy {
        shape.text.as_mut().unwrap().body.autofit_saved = None;
    }
    let ctx = Ctx::for_slide(doc, slide).unwrap();
    let x = resolve::xfrm(&ctx, &shape);
    let geometry = kition_slide_render::shape_geometry(&shape, x.w, x.h);
    kition_slide_text::layout(
        &ctx,
        &shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: geometry.text_rect, fields: &NoFields, prompt_color: None, no_shrink: false },
    )
}

fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap();
    to.add_blobs(&from.blobs_since(&since).unwrap()).unwrap();
    to.import_changes(&from.export_changes(&since).unwrap()).unwrap();
}

#[test]
fn concurrent_autofit_typing_does_not_keep_a_stale_saved_scale() {
    for reverse in [false, true] {
        concurrent_merge(reverse);
    }
}

fn concurrent_merge(reverse: bool) {
    let mut a = Session::with_new();
    let id = a.execute("shape.insert", &json!({"preset":"rect", "rect":[10,10,150,40], "text":"Base"})).unwrap()["id"].as_u64().unwrap();
    a.execute("format.font", &json!({"family":"Inter"})).unwrap();
    a.execute("format.size", &json!({"size":20})).unwrap();
    a.execute("format.autofit", &json!({"mode":"shrink"})).unwrap();
    let mut b = Session::new();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    let bid = b.doc().unwrap().doc.slides[0].shapes.last().unwrap().id.0;
    a.execute("text.edit", &json!({"id":id,"end":true})).unwrap();
    b.execute("text.edit", &json!({"id":bid,"at":[0,0]})).unwrap();
    a.execute("text.insert", &json!({"text":"\nFrom A"})).unwrap();
    b.execute("text.insert", &json!({"text":"From B\n"})).unwrap();
    assert!(!layout(&a, false).overflow);
    assert!(!layout(&b, false).overflow);
    let a_history = a.active().unwrap().history.undo.len();
    let b_history = b.active().unwrap().history.undo.len();
    let a_steps = a.kdm().unwrap().steps().0;
    let b_steps = b.kdm().unwrap().steps().0;
    // Capture both packets before either repair: both peers independently repair the merge.
    let ap = a.export_changes(&b.version().unwrap()).unwrap();
    let bp = b.export_changes(&a.version().unwrap()).unwrap();
    if reverse {
        b.import_changes(&ap).unwrap();
        a.import_changes(&bp).unwrap();
    } else {
        a.import_changes(&bp).unwrap();
        b.import_changes(&ap).unwrap();
    }
    for _ in 0..2 {
        send(&mut a, &mut b);
        send(&mut b, &mut a);
    }
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert_eq!(a.active().unwrap().history.undo.len(), a_history);
    assert_eq!(b.active().unwrap().history.undo.len(), b_history);
    assert_eq!(a.kdm().unwrap().steps().0, a_steps);
    assert_eq!(b.kdm().unwrap().steps().0, b_steps);
    let version = a.version().unwrap();
    for _ in 0..3 {
        send(&mut a, &mut b);
        send(&mut b, &mut a);
    }
    assert_eq!(a.version().unwrap(), version, "repairs must be idempotent");
    assert_eq!(b.version().unwrap(), version);
    for s in [&a, &b] {
        assert!(!layout(s, false).overflow, "merged text must be reflowed");
        let doc = &s.doc().unwrap().doc;
        let kdm = s.kdm().unwrap();
        let replica = kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0;
        assert_eq!(kdm_model::normalize(doc), kdm_model::normalize(&replica));
        let reopened = kition_slide_pptx::import(&kition_slide_pptx::export(doc).unwrap()).unwrap();
        let before = doc.slides[0].shapes.last().unwrap().text.as_ref().unwrap();
        let after = reopened.slides[0].shapes.last().unwrap().text.as_ref().unwrap();
        assert_eq!(after.text(), before.text());
        let (
            Some(kition_slide_model::text::AutoFit::Shrink { font_scale: x, .. }),
            Some(kition_slide_model::text::AutoFit::Shrink { font_scale: y, .. }),
        ) = (before.body.autofit, after.body.autofit)
        else {
            panic!("missing fit")
        };
        assert!((x - y).abs() < 0.00001);
        assert_eq!(after.body.autofit_saved, Some(true));
        assert!(after.body.autofit_input.is_none(), "PPTX stores an authoritative result");
    }
    a.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(a.doc().unwrap().doc.slides[0].shapes.last().unwrap().text.as_ref().unwrap().text(), "From B\nBase");
    assert!(!layout(&a, false).overflow);
    a.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(a.doc().unwrap().doc.slides[0].shapes.last().unwrap().text.as_ref().unwrap().text(), "From B\nBase\nFrom A");
    assert!(!layout(&a, false).overflow);
}

#[test]
fn comment_only_merge_renders_without_writing_repair_transactions() {
    let mut a = Session::with_new();
    let id = a.execute("shape.insert", &json!({"preset":"rect", "rect":[10,10,150,40], "text":"Base"})).unwrap()["id"].as_u64().unwrap();
    a.execute("format.font", &json!({"family":"Inter"})).unwrap();
    a.execute("format.size", &json!({"size":20})).unwrap();
    a.execute("format.autofit", &json!({"mode":"shrink"})).unwrap();
    let mut b = Session::new();
    b.open_snapshot(&a.snapshot().unwrap(), a.blobs_since(&Version::empty()).unwrap()).unwrap();
    let mut viewer = Session::new();
    viewer.open_snapshot(&a.snapshot().unwrap(), a.blobs_since(&Version::empty()).unwrap()).unwrap();
    viewer.set_comment_only(Some("person:viewer".into()));
    let bid = b.doc().unwrap().doc.slides[0].shapes.last().unwrap().id.0;
    a.execute("text.edit", &json!({"id":id,"end":true})).unwrap();
    b.execute("text.edit", &json!({"id":bid,"at":[0,0]})).unwrap();
    a.execute("text.insert", &json!({"text":"\nFrom A"})).unwrap();
    b.execute("text.insert", &json!({"text":"From B\n"})).unwrap();
    send(&mut a, &mut viewer);
    send(&mut b, &mut viewer);
    let version = viewer.version().unwrap();
    assert!(!layout(&viewer, false).overflow);
    assert!(viewer.active().unwrap().history.undo.is_empty());
    let kdm = viewer.kdm().unwrap();
    let replica = kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0;
    assert_eq!(kdm_model::normalize(&viewer.doc().unwrap().doc), kdm_model::normalize(&replica));
    send(&mut a, &mut viewer);
    send(&mut b, &mut viewer);
    assert_eq!(viewer.version().unwrap(), version);
    let hash = viewer.state_hash().unwrap();
    let bytes = kition_slide_engine::cmd::file::save_bytes(&viewer.doc().unwrap().doc, "pptx").unwrap();
    let reopened = kition_slide_pptx::import(&bytes).unwrap();
    let fit = reopened.slides[0].shapes.last().unwrap().text.as_ref().unwrap().body.autofit;
    let Some(kition_slide_model::text::AutoFit::Shrink { font_scale, .. }) = fit else { panic!("missing exported fit") };
    assert!((font_scale - layout(&viewer, false).font_scale).abs() < 0.00001);
    assert_eq!(viewer.state_hash().unwrap(), hash, "export must not mutate a comment-only replica");
    assert_eq!(viewer.version().unwrap(), version);
    let snapshot = viewer.snapshot().unwrap();
    let blobs = viewer.blobs_since(&Version::empty()).unwrap();
    let mut reopened_viewer = Session::new();
    reopened_viewer.set_comment_only(Some("person:other-viewer".into()));
    reopened_viewer.open_snapshot(&snapshot, blobs).unwrap();
    assert_eq!(reopened_viewer.version().unwrap(), version, "opening read-only must not repair the replica");
    assert_eq!(reopened_viewer.state_hash().unwrap(), hash);
    assert!(!layout(&reopened_viewer, false).overflow);
    assert_eq!(viewer.kdm_resets(), 0);
}

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
