//! Replicas of one line of changes through `KitSession<SlideModel>`: undo restores a deleted
//! shape with its ids, and a chart is its own object whose members each replica takes in turn.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, edit, first, peer, shape_named, shape_object, slide_file, sync};
use kition_slide_model::ShapeKind;

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
