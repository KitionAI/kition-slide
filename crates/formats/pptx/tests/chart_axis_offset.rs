//! Nondefault category label offsets survive chart regeneration and edits.
use kition_slide_model::chart::{Axis, AxisKind, Chart, ChartType};
use kition_slide_model::{Shape, ShapeKind, SlideFile};
use std::sync::Arc;

fn chart(p: &SlideFile) -> &Chart {
    let ShapeKind::Chart(chart) = &p.slides[0].shapes[0].kind else { panic!("missing chart") };
    chart
}
fn chart_mut(p: &mut SlideFile) -> &mut Chart {
    let ShapeKind::Chart(chart) = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { panic!("missing chart") };
    chart
}
fn roundtrip(p: &SlideFile) -> SlideFile {
    kition_slide_pptx::import(&kition_slide_pptx::export(p).unwrap()).unwrap()
}
fn fixture(kind: ChartType, offset: Option<f64>) -> SlideFile {
    let mut c = Chart::sample(kind);
    c.series[0].fmt.axes = vec![2, 1];
    c.fmt.axes = vec![
        Axis {
            id: 2,
            kind: AxisKind::Value,
            cross_ax: 1,
            pos: if kind.horizontal() { "b" } else { "l" }.into(),
            label_offset: Some(999.0),
            ..Default::default()
        },
        Axis {
            id: 1,
            kind: AxisKind::Category,
            cross_ax: 2,
            pos: if kind.horizontal() { "l" } else { "b" }.into(),
            label_offset: offset,
            ..Default::default()
        },
    ];
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![Shape { kind: ShapeKind::Chart(Box::new(c)), ..Default::default() }];
    p
}
fn offset(p: &SlideFile) -> Option<f64> {
    chart(p).fmt.axes.iter().find(|a| a.kind == AxisKind::Category).unwrap().label_offset
}

#[test]
fn authored_offsets_survive_import_regeneration_and_value_edits() {
    for kind in [ChartType::Column, ChartType::Bar] {
        for value in [0.0, 1.0, 10.0, 100.0, 1000.0] {
            let mut p = roundtrip(&fixture(kind, Some(value)));
            assert_eq!(offset(&p), Some(value));
            assert!(chart(&p).raw.is_some());
            chart_mut(&mut p).raw = None;
            chart_mut(&mut p).series[0].values[0] = Some(41.0);
            let restored = roundtrip(&p);
            assert_eq!(offset(&restored), Some(value));
            assert_eq!(chart(&restored).series[0].values[0], Some(41.0));
            assert!(chart(&restored).fmt.axes.iter().filter(|a| a.kind == AxisKind::Value).all(|a| a.label_offset.is_none()));
        }
    }
}

#[test]
fn offset_only_edit_invalidates_raw_chart_reuse() {
    let mut p = roundtrip(&fixture(ChartType::Column, Some(100.0)));
    chart_mut(&mut p).fmt.axes.iter_mut().find(|a| a.kind == AxisKind::Category).unwrap().label_offset = Some(0.0);
    assert_eq!(offset(&roundtrip(&p)), Some(0.0));
}

#[test]
fn invalid_offsets_export_the_valid_default() {
    for value in [None, Some(-1.0), Some(1001.0), Some(0.5), Some(f64::NAN), Some(f64::INFINITY)] {
        assert_eq!(offset(&roundtrip(&fixture(ChartType::Column, value))), Some(100.0));
    }
}
