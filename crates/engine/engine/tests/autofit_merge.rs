//! Shrink-on-overflow text whose fit the document does not hold any more: a merged draft that
//! turns shrink on for text the document resized meanwhile is reflowed in the merge's own undo
//! step (so one undo gives back the exact state before, and the redo the reflowed one), and a
//! commenter shows and exports text whose fit is stale without writing to the replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use kdm_model::kition_kit::kdm::UndoScope;
use kition_slide_engine::{Origin, Session, Version};
use kition_slide_model::{
    Shape,
    resolve::{self, Ctx},
    text::AutoFit,
};
use kition_slide_text::{NoFields, Opts, TextLayout};
use serde_json::json;

fn layout(s: &Session) -> TextLayout {
    let doc = &s.doc().unwrap().doc;
    let slide = &doc.slides[0];
    let shape: &Shape = slide.shapes.last().unwrap();
    let ctx = Ctx::for_slide(doc, slide).unwrap();
    let x = resolve::xfrm(&ctx, shape);
    let geometry = kition_slide_render::shape_geometry(shape, x.w, x.h);
    kition_slide_text::layout(
        &ctx,
        shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: geometry.text_rect, fields: &NoFields, prompt_color: None, no_shrink: false, empty_bullets: false },
    )
}

fn last_shape(s: &Session) -> Shape {
    s.doc().unwrap().doc.slides[0].shapes.last().unwrap().clone()
}

fn text(s: &Session) -> String {
    last_shape(s).text.as_ref().unwrap().text()
}

fn scale(shape: &Shape) -> Option<f64> {
    match shape.text.as_ref().unwrap().body.autofit {
        Some(AutoFit::Shrink { font_scale, .. }) => Some(font_scale),
        _ => None,
    }
}

/// The engine's slide is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let kdm = s.kdm().unwrap();
    let replica = kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0;
    assert_eq!(kdm_model::normalize(&s.doc().unwrap().doc), kdm_model::normalize(&replica), "{what}");
}

/// A slide with one text box of three lines in Inter 20 pt (rect `[10, 10, 150, 100]`, fits
/// unshrunk), its id.
fn three_lines() -> (Session, u64) {
    let mut a = Session::with_new();
    let id = a.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 150, 100], "text": "Base"})).unwrap()["id"].as_u64().unwrap();
    a.execute("format.fontName", &json!({"family": "Inter"})).unwrap();
    a.execute("format.fontSize", &json!({"size": 20})).unwrap();
    a.execute("text.edit", &json!({"id": id, "end": true})).unwrap();
    a.execute("text.insert", &json!({"text": "\nFrom A\nFrom B"})).unwrap();
    a.execute("edit.deselectAll", &json!({})).unwrap();
    (a, id)
}

#[test]
fn a_merged_draft_turning_shrink_on_for_text_resized_meanwhile_is_reflowed_in_its_step() {
    let (mut a, id) = three_lines();
    assert!(!layout(&a).overflow, "three lines fit the box unshrunk");
    // The draft (an agent's, at this version) turns shrink on: it fits there at full size.
    let base = a.version().unwrap();
    let mut draft = Session::new();
    draft.open_snapshot(&a.snapshot().unwrap(), a.blobs_since(&Version::empty()).unwrap()).unwrap();
    let draft_id = last_shape(&draft).id.0;
    draft.execute("format.autofit", &json!({"mode": "shrink", "ids": [draft_id]})).unwrap();
    assert_eq!(scale(&last_shape(&draft)), Some(1.0), "the draft's fit, for the box it saw");
    let changes = draft.export_changes(&base).unwrap();
    // Meanwhile the person makes the box lower: the text no longer fits it, and nothing shrinks it.
    a.execute("shape.resize", &json!({"h": 40, "ids": [id]})).unwrap();
    assert_eq!(scale(&last_shape(&a)), None);
    let (before_hash, before_text) = (a.state_hash().unwrap(), text(&a));
    let entries = a.active().unwrap().history.undo.len();
    let depth = a.kdm().unwrap().replica().undo_depth(UndoScope::Person);
    a.merge_changes(&changes, &Origin::agent("agent:drafter")).unwrap();
    // The merged text is reflowed for the box it is in now.
    assert!(!layout(&a).overflow, "the merged fit is reflowed");
    let fitted = scale(&last_shape(&a)).unwrap();
    assert!(fitted < 1.0, "shrunk to the lower box: {fitted}");
    assert_eq!(last_shape(&a).xfrm.unwrap().h, 40.0, "the person's resize stays");
    assert_eq!(text(&a), before_text);
    assert_holds(&a, "after the merge");
    // One step, the merge and its reflow: the reflow joined the merge's replica undo step
    // (`Replica::transact_in_step`), so it is one step of the replica's too.
    let history = &a.active().unwrap().history;
    assert_eq!(history.undo.len(), entries + 1);
    assert_eq!(history.undo.last().map(|e| (e.agent, e.steps)), Some((false, 1)));
    assert_eq!(a.kdm().unwrap().replica().undo_depth(UndoScope::Person), depth + 1, "the reflow is in the merge's replica step");
    let merged_hash = a.state_hash().unwrap();
    // Settled: reopening the replica writes nothing.
    let version = a.version().unwrap();
    let mut reopened = Session::new();
    reopened.open_snapshot(&a.snapshot().unwrap(), a.blobs_since(&Version::empty()).unwrap()).unwrap();
    assert_eq!(reopened.version().unwrap(), version, "reopening wrote a repair");
    assert_eq!(reopened.state_hash().unwrap(), merged_hash);
    // The fit is the one the file stores.
    let doc = &a.doc().unwrap().doc;
    let reread = kition_slide_pptx::import(&kition_slide_pptx::export(doc).unwrap()).unwrap();
    let after = reread.slides[0].shapes.last().unwrap();
    assert_eq!(after.text.as_ref().unwrap().text(), before_text);
    assert!((scale(after).unwrap() - fitted).abs() < 0.00001);
    assert_eq!(after.text.as_ref().unwrap().body.autofit_saved, Some(true));
    assert!(after.text.as_ref().unwrap().body.autofit_input.is_none(), "PPTX stores an authoritative result");
    // One undo gives back the exact state before the merge; the redo the reflowed state.
    a.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(a.state_hash().unwrap(), before_hash, "the undo gives back the exact state before the merge");
    assert_eq!(scale(&last_shape(&a)), None);
    assert_holds(&a, "after the undo");
    a.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(a.state_hash().unwrap(), merged_hash, "the redo gives back the reflowed state");
    assert!(!layout(&a).overflow);
    assert_holds(&a, "after the redo");
}

#[test]
fn a_commenter_shows_and_exports_text_whose_fit_is_stale_without_writing() {
    let mut a = Session::with_new();
    a.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 150, 40], "text": "Base"})).unwrap();
    a.execute("format.fontName", &json!({"family": "Inter"})).unwrap();
    a.execute("format.fontSize", &json!({"size": 20})).unwrap();
    a.execute("format.autofit", &json!({"mode": "shrink"})).unwrap();
    assert!(!layout(&a).overflow);
    // A replica whose text has grown past its fit (the fit and its receipt are the one-line
    // text's): what a commenter, who may not write the fit, can be given to open.
    let mut stale = (*a.doc().unwrap().doc).clone();
    {
        let slide = Arc::make_mut(&mut stale.slides[0]);
        let body = slide.shapes.last_mut().unwrap().text.as_mut().unwrap();
        assert!(body.body.autofit_input.is_some(), "the fit has a receipt");
        let first = body.paragraphs[0].clone();
        body.paragraphs = ["Base", "From A", "From B"]
            .into_iter()
            .map(|line| {
                let mut paragraph = first.clone();
                paragraph.runs.truncate(1);
                if let Some(run) = paragraph.runs.first_mut() {
                    run.text = line.into();
                }
                paragraph
            })
            .collect();
    }
    let held = kdm_model::SlideSession::create(&stale).unwrap();
    let (snapshot, blobs) = (held.snapshot().unwrap(), held.blobs().clone());
    let (version, hash) = (held.version(), held.state_hash().unwrap());

    let mut viewer = Session::new();
    viewer.set_comment_only(Some("person:viewer".into()));
    viewer.open_snapshot(&snapshot, blobs.clone()).unwrap();
    assert_eq!(viewer.version().unwrap(), version, "opening read-only must not repair the replica");
    assert_eq!(viewer.state_hash().unwrap(), hash);
    assert_eq!(text(&viewer), "Base\nFrom A\nFrom B");
    assert!(!layout(&viewer).overflow, "the stale fit is laid out afresh");
    assert!(viewer.active().unwrap().history.undo.is_empty());
    let bytes = kition_slide_engine::cmd::file::save_bytes(&viewer.doc().unwrap().doc, "pptx").unwrap();
    let reopened = kition_slide_pptx::import(&bytes).unwrap();
    let exported = scale(reopened.slides[0].shapes.last().unwrap()).unwrap();
    assert!((exported - layout(&viewer).font_scale).abs() < 0.00001, "the export holds the fit the layout shows");
    assert!(exported < 1.0);
    assert_eq!(viewer.state_hash().unwrap(), hash, "export must not mutate a comment-only replica");
    assert_eq!(viewer.version().unwrap(), version);
    assert_eq!(viewer.kdm_resets(), 0);

    // An editor opening the same replica writes the reflow (not an undo step).
    let mut editor = Session::new();
    editor.open_snapshot(&snapshot, blobs).unwrap();
    assert_ne!(editor.version().unwrap(), version, "the editor repairs the fit");
    assert!(!layout(&editor).overflow);
    assert!((scale(&last_shape(&editor)).unwrap() - exported).abs() < 0.00001, "the fit the commenter's export holds");
    assert!(editor.active().unwrap().history.undo.is_empty());
}
