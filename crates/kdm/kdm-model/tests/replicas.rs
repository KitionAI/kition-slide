//! Replicas of one line of changes through `KitSession<SlideModel>`: undo restores a deleted
//! shape with its ids, a chart is its own object whose members each replica takes in turn, and the
//! repair after a merged draft (data model 6.6) clears what names an object the document deleted
//! meanwhile: an animation of a deleted shape goes, a trigger or a connector end at one is
//! cleared, and a slide whose layout is gone gets its master's first layout.

mod common;

use std::sync::Arc;

use common::{
    SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, draft, edit, first, merge, peer, shape_named, shape_object, slide_file, sync,
};
use kition_slide_model::{Animation, ShapeKind};

fn pair() -> (SharedBlobs, common::Session, common::Session) {
    let blobs = SharedBlobs::default();
    let a = first(&slide_file(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &common::Session, b: &common::Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(canonical(&a.model().slide_file), canonical(&b.model().slide_file), "same view");
    assert_no_dangling(&a.model().slide_file);
    assert_replica_clean(a.replica());
}

#[test]
fn undo_brings_a_deleted_shape_back_with_its_local_id() {
    let (blobs, mut a, _b) = pair();
    let id = shape_named(&a.model().slide_file, 0, "A").id;
    let object = shape_object(&a, 0, "A");
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes.retain(|s| s.name != "A"));
    let (undone, _) = a.undo().unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    let p = &a.model().slide_file;
    assert_eq!(shape_named(p, 0, "A").id, id, "the restored shape keeps its id");
    assert_eq!(shape_object(&a, 0, "A"), object);
    assert_no_dangling(p);
}

fn chart_of<'a>(p: &'a kition_slide_model::SlideFile, name: &str) -> &'a kition_slide_model::chart::Chart {
    match &shape_named(p, 0, name).kind {
        ShapeKind::Chart(c) => c,
        other => panic!("not a chart: {other:?}"),
    }
}

/// A chart is its own object (DR14): one person retitles a chart, then another edits its data,
/// and both stay; the chart's members are entries of that object, not the shape's `look`.
#[test]
fn a_title_and_a_data_edit_of_one_chart_are_both_kept() {
    use kition_slide_model::chart::{Chart, ChartType};
    let mut p = slide_file();
    let slide = Arc::make_mut(&mut p.slides[0]);
    let mut chart = Chart::sample(ChartType::Column);
    chart.raw = Some("<c:chartSpace>kept</c:chartSpace>".into());
    slide.shapes.push(kition_slide_model::Shape {
        id: kition_slide_model::ShapeId(900),
        name: "Chart".into(),
        xfrm: Some(kition_slide_model::Xfrm::new(10.0, 10.0, 300.0, 200.0)),
        kind: ShapeKind::Chart(Box::new(chart)),
        ..Default::default()
    });
    let blobs = SharedBlobs::default();
    let mut a = first(&p, &blobs);
    let mut b = peer(&a, 2, &blobs);
    // Stored apart: the shape's members name the kind only; the chart object holds the members.
    let object = shape_object(&a, 0, "Chart");
    let kind = a.replica().map_get(&object, "look", "kind").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(kind, serde_json::json!({ "type": "chart" }));
    assert!(a.replica().get(&object, "chart").is_ok_and(|v| v.is_string()));
    edit(&mut a, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        if let Some(ShapeKind::Chart(c)) = s.shapes.iter_mut().find(|s| s.name == "Chart").map(|s| &mut s.kind) {
            c.title = Some("From A".into());
        }
    });
    sync(&mut a, &mut b);
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        if let Some(ShapeKind::Chart(c)) = s.shapes.iter_mut().find(|s| s.name == "Chart").map(|s| &mut s.kind) {
            c.categories = vec!["North".into(), "South".into()];
            c.legend = Some("t".into());
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let c = chart_of(&a.model().slide_file, "Chart");
    assert_eq!(c.title.as_deref(), Some("From A"));
    assert_eq!((c.categories.clone(), c.legend.as_deref()), (vec!["North".to_string(), "South".to_string()], Some("t")));
    assert_eq!(c.raw.as_deref(), Some("<c:chartSpace>kept</c:chartSpace>"), "the kept part is kept");
    // Deleting the shape deletes its chart object.
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes.retain(|s| s.name != "Chart"));
    assert_replica_clean(a.replica());
}

/// The document's view is what its replica reads, and nothing names a deleted object.
fn assert_clean(s: &common::Session, blobs: &SharedBlobs) {
    let (fresh, _) = kdm_model::read_document(s.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(&s.model().slide_file), "the view is the replica's");
    assert_no_dangling(&s.model().slide_file);
    assert_replica_clean(s.replica());
}

#[test]
fn a_drafted_animation_of_a_shape_deleted_meanwhile_goes_and_its_trigger_is_cleared() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&slide_file(), &blobs);
    let changes = draft(&doc, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let target = s.shapes.iter().find(|s| s.name == "B").map(|s| s.id).unwrap_or_else(|| panic!("B"));
        s.animations.push(Animation { shape: target, effect: "zoom".into(), ..Default::default() });
    });
    edit(&mut doc, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes.retain(|s| s.name != "B"));
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    assert!(p.slides[0].shapes.iter().all(|s| s.name != "B"));
    assert!(p.slides[0].animations.iter().all(|x| x.effect != "zoom"), "the animation of the deleted shape is gone");
    assert_eq!(p.slides[0].animations.len(), 1, "the animation of A is kept");
    assert_eq!(p.slides[0].animations[0].trigger, None, "its trigger (B) is cleared");
}

#[test]
fn a_drafted_edit_of_a_connector_to_a_shape_deleted_meanwhile_loses_that_end() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&slide_file(), &blobs);
    let changes = draft(&doc, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let connector = s.shapes.iter_mut().find(|s| s.name == "Connector").unwrap_or_else(|| panic!("connector"));
        if let Some(x) = &mut connector.xfrm {
            x.w += 5.0;
        }
    });
    // The document deletes A as its delete command does: with A's animation (local writes are
    // not repaired; the merge's repair checks what the draft changed).
    edit(&mut doc, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let a = s.shapes.iter().find(|s| s.name == "A").map(|s| s.id).unwrap_or_else(|| panic!("A"));
        s.shapes.retain(|s| s.id != a);
        s.animations.retain(|x| x.shape != a);
    });
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    let b_id = shape_named(p, 0, "B").id;
    match &shape_named(p, 0, "Connector").kind {
        ShapeKind::Connector { start, end } => {
            assert_eq!(*start, None, "the end at the deleted shape is cleared");
            assert_eq!(*end, Some((b_id, 3)), "the other end is kept");
        }
        other => panic!("not a connector: {other:?}"),
    }
    assert!(shape_named(p, 0, "Connector").xfrm.is_some_and(|x| x.w == 105.0), "the draft's edit is kept");
}

#[test]
fn a_slide_a_draft_moved_to_a_layout_deleted_meanwhile_gets_its_masters_first_layout() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&slide_file(), &blobs);
    let changes = draft(&doc, &blobs, |p| {
        let second = p.masters[0].layouts[1].id;
        Arc::make_mut(&mut p.slides[0]).layout = second;
    });
    let second = doc.model().slide_file.masters[0].layouts[1].id;
    edit(&mut doc, &blobs, |p| Arc::make_mut(&mut p.masters[0]).layouts.retain(|l| l.id != second));
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    assert_eq!(p.slides[0].layout, p.masters[0].layouts[0].id);
}
