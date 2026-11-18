//! Two replicas through `KitSession<SlideModel>`: concurrent edits of different things both
//! survive, and the repair after a merge clears references to what the other side deleted.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, edit, first, peer, shape_named, shape_object, slide_file, sync};
use kition_slide_model::text::TextBody;
use kition_slide_model::{Animation, ShapeKind};

fn text_of(p: &kition_slide_model::SlideFile, slide: usize, name: &str) -> String {
    shape_named(p, slide, name).text.as_ref().map(TextBody::text).unwrap_or_default()
}

fn table_of(p: &kition_slide_model::SlideFile) -> &kition_slide_model::Table {
    match &shape_named(p, 1, "Table").kind {
        ShapeKind::Table(t) => t,
        other => panic!("not a table: {other:?}"),
    }
}

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
fn different_shapes_edited_concurrently_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        s.shapes.iter_mut().find(|s| s.name == "A").unwrap_or_else(|| panic!("A")).text = Some(TextBody::from_text("from a"));
    });
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let shape = s.shapes.iter_mut().find(|s| s.name == "B").unwrap_or_else(|| panic!("B"));
        shape.text = Some(TextBody::from_text("from b"));
        if let Some(x) = &mut shape.xfrm {
            x.x += 10.0;
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    assert_eq!(text_of(p, 0, "A"), "from a");
    assert_eq!(text_of(p, 0, "B"), "from b");
}

#[test]
fn two_members_of_one_shape_edited_concurrently_are_both_kept() {
    use kition_slide_model::style::{ColorRef, Fill, Line};
    let (blobs, mut a, mut b) = pair();
    let red = ColorRef::rgb(kition_slide_model::Rgba::rgb(0xC0, 0, 0));
    let blue = ColorRef::rgb(kition_slide_model::Rgba::rgb(0, 0, 0xC0));
    edit(&mut a, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        s.shapes.iter_mut().find(|s| s.name == "A").unwrap_or_else(|| panic!("A")).fill = Some(Fill::Solid { color: red.clone() });
    });
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        s.shapes.iter_mut().find(|s| s.name == "A").unwrap_or_else(|| panic!("A")).line = Some(Line::solid(blue.clone(), 3.0));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    // Each member is its own entry of the shape's `look` (schema 5): A's fill and B's outline both stay.
    let shape = a.model().slide_file.slides[0].shapes.iter().find(|s| s.name == "A").cloned().unwrap_or_else(|| panic!("A"));
    assert_eq!(shape.fill, Some(Fill::Solid { color: red }));
    assert_eq!(shape.line.as_ref().and_then(|l| l.width), Some(3.0));
}

#[test]
fn a_row_insert_and_a_cell_edit_in_another_row_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| {
        if let Some(t) = common::table_mut(p) {
            t.insert_row(0);
        }
    });
    edit(&mut b, &blobs, |p| {
        if let Some(t) = common::table_mut(p) {
            t.rows[2].cells[1].text = TextBody::from_text("edited");
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let t = table_of(&a.model().slide_file);
    assert_eq!(t.rows.len(), 4);
    assert_eq!(t.rows[0].cells[1].text.text(), "");
    assert_eq!(t.rows[3].cells[1].text.text(), "edited");
    assert_eq!(t.rows[1].cells[1].text.text(), "r0c1");
}

#[test]
fn a_column_insert_and_a_cell_edit_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| {
        if let Some(t) = common::table_mut(p) {
            t.insert_col(1);
        }
    });
    edit(&mut b, &blobs, |p| {
        if let Some(t) = common::table_mut(p) {
            t.rows[0].cells[2].text = TextBody::from_text("edited");
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let t = table_of(&a.model().slide_file);
    assert_eq!(t.cols.len(), 4);
    assert_eq!(t.rows[0].cells[3].text.text(), "edited");
    assert_eq!(t.rows[0].cells[0].text.text(), "r0c0");
}

#[test]
fn deleting_a_shape_removes_an_animation_added_to_it_concurrently() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes.retain(|s| s.name != "B"));
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let target = s.shapes.iter().find(|s| s.name == "B").map(|s| s.id).unwrap_or_else(|| panic!("B"));
        s.animations.push(Animation { shape: target, effect: "zoom".into(), ..Default::default() });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    assert!(p.slides[0].shapes.iter().all(|s| s.name != "B"));
    assert!(p.slides[0].animations.iter().all(|x| x.effect != "zoom"), "the animation of the deleted shape is gone");
    assert_eq!(p.slides[0].animations.len(), 1, "the animation of A is kept");
    assert_eq!(p.slides[0].animations[0].trigger, None, "its trigger (B) is cleared");
}

#[test]
fn a_slide_reorder_and_an_edit_of_the_moved_slide_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| {
        let moved = p.slides.remove(0);
        p.slides.push(moved);
    });
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        s.shapes.iter_mut().find(|s| s.name == "A").unwrap_or_else(|| panic!("A")).text = Some(TextBody::from_text("moved and edited"));
        s.hidden = true;
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    let last = p.slides.len() - 1;
    assert_eq!(text_of(p, last, "A"), "moved and edited");
    assert!(p.slides[last].hidden);
}

#[test]
fn a_connector_to_a_shape_deleted_elsewhere_loses_that_end() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes.retain(|s| s.name != "A"));
    edit(&mut b, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        let connector = s.shapes.iter_mut().find(|s| s.name == "Connector").unwrap_or_else(|| panic!("connector"));
        if let Some(x) = &mut connector.xfrm {
            x.w += 5.0;
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    let b_id = shape_named(p, 0, "B").id;
    match &shape_named(p, 0, "Connector").kind {
        ShapeKind::Connector { start, end } => {
            assert_eq!(*start, None, "the end at the deleted shape is cleared");
            assert_eq!(*end, Some((b_id, 3)), "the other end is kept");
        }
        other => panic!("not a connector: {other:?}"),
    }
    assert!(shape_named(p, 0, "Connector").xfrm.is_some_and(|x| x.w == 105.0));
}

#[test]
fn a_slide_whose_layout_is_deleted_gets_its_masters_first_layout() {
    let (blobs, mut a, mut b) = pair();
    let second_layout = a.model().slide_file.masters[0].layouts[1].id;
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).layout = second_layout);
    let second_layout_b = b.model().slide_file.masters[0].layouts[1].id;
    edit(&mut b, &blobs, |p| Arc::make_mut(&mut p.masters[0]).layouts.retain(|l| l.id != second_layout_b));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    assert_eq!(p.slides[0].layout, p.masters[0].layouts[0].id);
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

#[test]
fn a_slide_listed_twice_by_concurrent_edits_is_listed_once_after_the_merge() {
    let (_blobs, mut a, mut b) = pair();
    let root = kition_kit::kdm::ObjectId::from_string("slide-file");
    let slides = a.replica().list(&root, "slides").unwrap_or_else(|error| panic!("{error}"));
    let slide = slides.first().cloned().unwrap_or_else(|| panic!("a slide"));
    // Both peers list the first slide again at the end (as two concurrent re-inserts do).
    for session in [&mut a, &mut b] {
        session
            .execute(&common::person("test"), |_, tx| {
                let len = tx.list_items(&root, "slides")?.len();
                tx.list_insert(&root, "slides", len, &slide).map_err(Into::into)
            })
            .unwrap_or_else(|error| panic!("{error}"));
    }
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = a.replica().list(&root, "slides").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(after, slides, "each slide is listed once, at its first place");
}

fn chart_of<'a>(p: &'a kition_slide_model::SlideFile, name: &str) -> &'a kition_slide_model::chart::Chart {
    match &shape_named(p, 0, name).kind {
        ShapeKind::Chart(c) => c,
        other => panic!("not a chart: {other:?}"),
    }
}

/// A chart is its own object (schema version 4, DR14): one person retitling a chart while another
/// edits its data keeps both, and the chart's members are entries of that object, not the shape's
/// `props`.
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

/// A schema 3 replica held a chart in its shape's `props`: it reads as it is, and the next write of
/// the chart makes its `Chart` object and leaves the shape's `props` with the kind only.
#[test]
fn a_chart_in_the_shapes_props_reads_and_moves_to_its_object_on_the_next_write() {
    use kition_slide_model::chart::{Chart, ChartType};
    let mut p = slide_file();
    let slide = Arc::make_mut(&mut p.slides[0]);
    slide.shapes.push(kition_slide_model::Shape {
        id: kition_slide_model::ShapeId(900),
        name: "Chart".into(),
        kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Pie))),
        ..Default::default()
    });
    let blobs = SharedBlobs::default();
    let (mut replica, ids) = common::new_replica(&p, &blobs);
    // Rewrite it in the schema 3 form: the members back in `props`, no chart object.
    let slide_object = ids.slides.object(p.slides[0].id).unwrap_or_else(|| panic!("slide")).clone();
    let shape = ids.shape(&slide_object, kition_slide_model::ShapeId(900)).unwrap_or_else(|| panic!("shape")).clone();
    replica
        .transact(&kition_kit::kdm::Origin::system("schema 3"), |tx| {
            let serde_json::Value::String(chart) = tx.get(&shape, "chart")? else { panic!("a chart object") };
            let chart = kition_kit::kdm::ObjectId::from_string(chart);
            // Before schema 5 the shape's members were one `props` register.
            let mut props = serde_json::Value::Object(tx.map_entries(&shape, "look")?.into_iter().collect());
            for (k, _) in tx.map_entries(&shape, "look")? {
                tx.map_delete(&shape, "look", &k)?;
            }
            for (k, v) in tx.map_entries(&chart, "parts")? {
                props["kind"][k] = v;
            }
            tx.set(&shape, "props", props)?;
            tx.set(&shape, "chart", serde_json::Value::Null)?;
            tx.delete(&chart)
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let mut a: common::Session = kition_kit::session::KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(chart_of(&a.model().slide_file, "Chart").kind, ChartType::Pie, "the schema 3 form reads");
    edit(&mut a, &blobs, |p| {
        let s = Arc::make_mut(&mut p.slides[0]);
        if let Some(ShapeKind::Chart(c)) = s.shapes.iter_mut().find(|s| s.name == "Chart").map(|s| &mut s.kind) {
            c.title = Some("Share".into());
        }
    });
    let object = shape_object(&a, 0, "Chart");
    assert!(a.replica().get(&object, "chart").is_ok_and(|v| v.is_string()), "the chart has its object");
    // The members moved into `look` (schema 5), the kind only; the register is gone.
    assert_eq!(a.replica().map_get(&object, "look", "kind").unwrap_or_else(|e| panic!("{e}")), serde_json::json!({ "type": "chart" }));
    assert!(a.replica().get(&object, "props").is_ok_and(|v| v.is_null()));
    let c = chart_of(&a.model().slide_file, "Chart");
    assert_eq!((c.kind, c.title.as_deref()), (ChartType::Pie, Some("Share")));
    assert!(a.replica().min_schema_version() >= kdm_model::CHART_SCHEMA_VERSION);
}

/// A stroke `k` (distinct points per `k`), in the slide's ink layer's terms.
fn stroke(k: f64) -> kition_slide_model::InkStroke {
    kition_slide_model::InkStroke {
        points: vec![(10.0 * k, 20.0, 0.5), (10.0 * k + 30.0, 40.0, 0.6)],
        color: kition_slide_model::Rgba::rgb(0x20, 0x40, 0xC0),
        width: 2.0,
        highlighter: false,
    }
}

/// The strokes of the slide's ink layer.
fn strokes(s: &common::Session) -> Vec<kition_slide_model::InkStroke> {
    s.model().slide_file.slides[0]
        .shapes
        .iter()
        .find_map(|sh| if let ShapeKind::Ink { strokes } = &sh.kind { Some(strokes.clone()) } else { None })
        .unwrap_or_default()
}

/// Edits the slide's ink layer (made when there is none).
fn ink(s: &mut common::Session, blobs: &SharedBlobs, f: impl FnOnce(&mut Vec<kition_slide_model::InkStroke>)) {
    edit(s, blobs, |p| {
        let slide = Arc::make_mut(&mut p.slides[0]);
        if !slide.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Ink { .. })) {
            slide.shapes.push(kition_slide_model::Shape {
                id: kition_slide_model::ShapeId(9000),
                name: "Ink".into(),
                kind: ShapeKind::Ink { strokes: vec![] },
                ..Default::default()
            });
        }
        if let Some(ShapeKind::Ink { strokes }) = slide.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Ink { .. })) {
            f(strokes);
        }
    });
}

#[test]
fn strokes_two_people_draw_and_erase_on_one_slide_at_once_all_count() {
    let (blobs, mut a, mut b) = pair();
    ink(&mut a, &blobs, |s| s.extend([stroke(1.0), stroke(2.0)]));
    sync(&mut a, &mut b);
    assert_eq!(strokes(&b), vec![stroke(1.0), stroke(2.0)]);
    // Each person draws one more at once: both stay (schema 6: one `look` entry per stroke).
    ink(&mut a, &blobs, |s| s.push(stroke(3.0)));
    ink(&mut b, &blobs, |s| s.push(stroke(4.0)));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let both = strokes(&a);
    assert_eq!(both.len(), 4, "{both:?}");
    assert_eq!(&both[..2], &[stroke(1.0), stroke(2.0)], "older strokes stay first");
    assert!(both.contains(&stroke(3.0)) && both.contains(&stroke(4.0)));
    // Each erases a different stroke at once: both are gone, nothing comes back.
    ink(&mut a, &blobs, |s| s.retain(|x| *x != stroke(1.0)));
    ink(&mut b, &blobs, |s| s.retain(|x| *x != stroke(3.0)));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let left = strokes(&a);
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(left.contains(&stroke(2.0)) && left.contains(&stroke(4.0)));
    // A stroke drawn twice over is two strokes; erasing everything leaves none.
    ink(&mut a, &blobs, |s| s.push(stroke(2.0)));
    assert_eq!(strokes(&a).iter().filter(|x| **x == stroke(2.0)).count(), 2);
    ink(&mut a, &blobs, Vec::clear);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(strokes(&b).is_empty());
}

#[test]
fn a_schema_5_ink_layer_reads_and_moves_its_strokes_into_their_own_entries() {
    let mut p = slide_file();
    let slide = Arc::make_mut(&mut p.slides[0]);
    slide.shapes.push(kition_slide_model::Shape {
        id: kition_slide_model::ShapeId(901),
        name: "Ink".into(),
        kind: ShapeKind::Ink { strokes: vec![stroke(1.0), stroke(2.0)] },
        ..Default::default()
    });
    let blobs = SharedBlobs::default();
    let (mut replica, ids) = common::new_replica(&p, &blobs);
    // Rewrite it in the schema 5 form: the strokes inside the `kind` entry, none of their own.
    let slide_object = ids.slides.object(p.slides[0].id).unwrap_or_else(|| panic!("slide")).clone();
    let shape = ids.shape(&slide_object, kition_slide_model::ShapeId(901)).unwrap_or_else(|| panic!("shape")).clone();
    replica
        .transact(&kition_kit::kdm::Origin::system("schema 5"), |tx| {
            for (k, _) in tx.map_entries(&shape, "look")?.into_iter().filter(|(k, _)| k.starts_with("stroke:")) {
                tx.map_delete(&shape, "look", &k)?;
            }
            let strokes = serde_json::to_value(vec![stroke(1.0), stroke(2.0)]).unwrap_or_default();
            tx.map_set(&shape, "look", "kind", serde_json::json!({ "type": "ink", "strokes": strokes }))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let mut a: common::Session = kition_kit::session::KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(strokes(&a), vec![stroke(1.0), stroke(2.0)], "the schema 5 form reads");
    // Its next write moves them; erasing every stroke then leaves none (nothing stale comes back).
    ink(&mut a, &blobs, |s| s.push(stroke(3.0)));
    let object = shape_object(&a, 0, "Ink");
    assert_eq!(a.replica().map_get(&object, "look", "kind").unwrap_or_else(|e| panic!("{e}")), serde_json::json!({ "type": "ink" }));
    assert_eq!(strokes(&a), vec![stroke(1.0), stroke(2.0), stroke(3.0)]);
    ink(&mut a, &blobs, Vec::clear);
    assert!(strokes(&a).is_empty());
}
