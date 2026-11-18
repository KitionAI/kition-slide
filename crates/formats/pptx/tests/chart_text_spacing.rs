//! Chart tracking survives regenerated text containers and spacing-only edits.

use kition_slide_color::Rgba;
use kition_slide_model::chart::{Axis, AxisKind, Chart, ChartText, ChartType, DataLabels};
use kition_slide_model::style::ColorRef;
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};
use std::sync::Arc;

fn chart(presentation: &SlideFile) -> &Chart {
    let ShapeKind::Chart(chart) = &presentation.slides[0].shapes[0].kind else { panic!("missing chart") };
    chart
}

fn chart_mut(presentation: &mut SlideFile) -> &mut Chart {
    let ShapeKind::Chart(chart) = &mut Arc::make_mut(&mut presentation.slides[0]).shapes[0].kind else { panic!("missing chart") };
    chart
}

fn style(spacing: f64) -> ChartText {
    ChartText {
        spacing: Some(spacing),
        font: Some("Inter".into()),
        size: Some(14.0),
        bold: Some(true),
        color: Some(ColorRef::rgb(Rgba::rgb(86, 69, 212))),
        ..Default::default()
    }
}

fn fixture() -> SlideFile {
    let mut chart = Chart::sample(ChartType::Column);
    chart.title = Some("Synthetic title".into());
    chart.legend = Some("b".into());
    chart.axis_titles = (Some("Categories".into()), Some("Values".into()));
    chart.fmt.text = style(0.25);
    chart.fmt.title_text = style(1.25);
    chart.fmt.legend_text = style(-0.25);
    chart.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), cross_ax: 2, text: style(0.5), title_text: style(0.75), ..Default::default() },
        Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), cross_ax: 1, text: style(-0.5), title_text: style(0.0), ..Default::default() },
    ];
    chart.series[0].fmt.labels = Some(DataLabels {
        value: true,
        text: style(1.5),
        points: vec![(0, DataLabels { value: true, text: style(0.0), ..Default::default() })],
        ..Default::default()
    });
    let mut presentation = SlideFile::default();
    Arc::make_mut(&mut presentation.slides[0]).shapes =
        vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(chart)), ..Default::default() }];
    presentation
}

fn roundtrip(presentation: &SlideFile) -> SlideFile {
    kition_slide_pptx::import(&kition_slide_pptx::export(presentation).unwrap()).unwrap()
}

#[test]
fn regenerated_primary_chart_text_keeps_tracking_and_other_typography() {
    let mut presentation = roundtrip(&fixture());
    assert!(chart(&presentation).raw.is_some());
    chart_mut(&mut presentation).raw = None;
    chart_mut(&mut presentation).series[0].values[0] = Some(19.0);
    let restored = roundtrip(&presentation);
    let chart = chart(&restored);
    assert_eq!(chart.fmt.text, style(0.25));
    assert_eq!(chart.fmt.title_text, style(1.25));
    assert_eq!(chart.fmt.legend_text, style(-0.25));
    let category = chart.fmt.axes.iter().find(|axis| axis.kind == AxisKind::Category).unwrap();
    let value = chart.fmt.axes.iter().find(|axis| axis.kind == AxisKind::Value).unwrap();
    assert_eq!(category.text, style(0.5));
    assert_eq!(category.title_text, style(0.75));
    assert_eq!(value.text, style(-0.5));
    assert_eq!(value.title_text, style(0.0));
    let labels = chart.series[0].fmt.labels.as_ref().unwrap();
    assert_eq!(labels.text, style(1.5));
    assert_eq!(labels.point(0).unwrap().text, style(0.0));
    assert_eq!(chart.series[0].values[0], Some(19.0));
}

#[test]
fn spacing_only_edits_do_not_reuse_the_old_chart_part() {
    let mut presentation = roundtrip(&fixture());
    chart_mut(&mut presentation).fmt.title_text.spacing = Some(-1.25);
    chart_mut(&mut presentation).series[0].fmt.labels.as_mut().unwrap().text.spacing = Some(0.0);
    let restored = roundtrip(&presentation);
    assert_eq!(chart(&restored).fmt.title_text.spacing, Some(-1.25));
    assert_eq!(chart(&restored).series[0].fmt.labels.as_ref().unwrap().text.spacing, Some(0.0));
}

#[test]
fn regenerated_axis_tracking_follows_axis_identity_for_bars_and_scatter() {
    for kind in [ChartType::Bar, ChartType::Scatter] {
        let mut presentation = fixture();
        let authored = chart_mut(&mut presentation);
        authored.kind = kind;
        for series in &mut authored.series {
            series.fmt.axes.clear();
        }
        let horizontal = kind == ChartType::Bar;
        authored.fmt.axes[0].kind = if horizontal { AxisKind::Category } else { AxisKind::Value };
        authored.fmt.axes[0].pos = if horizontal { "l" } else { "b" }.into();
        authored.fmt.axes[1].pos = if horizontal { "b" } else { "l" }.into();
        // XML/model order must not make the value-axis typography become category/x text.
        authored.fmt.axes.swap(0, 1);
        let restored = roundtrip(&presentation);
        let restored = chart(&restored);
        let category = restored.fmt.axes.iter().find(|axis| axis.pos == if horizontal { "l" } else { "b" }).unwrap();
        let value = restored.fmt.axes.iter().find(|axis| axis.id == category.cross_ax).unwrap();
        assert_eq!(category.text.spacing, Some(0.5), "{kind:?}");
        assert_eq!(category.title_text.spacing, Some(0.75), "{kind:?}");
        assert_eq!(value.text.spacing, Some(-0.5), "{kind:?}");
        assert_eq!(value.title_text.spacing, Some(0.0), "{kind:?}");
    }
}

#[test]
fn regenerated_axis_typography_follows_first_series_binding_before_secondary_axis_order() {
    for kind in [ChartType::Column, ChartType::Bar, ChartType::Scatter] {
        let mut presentation = fixture();
        let authored = chart_mut(&mut presentation);
        authored.kind = kind;
        let horizontal = kind == ChartType::Bar;
        authored.fmt.axes[0].kind = if kind == ChartType::Scatter { AxisKind::Value } else { AxisKind::Category };
        authored.fmt.axes[0].pos = if horizontal { "l" } else { "b" }.into();
        authored.fmt.axes[1].pos = if horizontal { "b" } else { "l" }.into();
        let mut secondary_category = authored.fmt.axes[0].clone();
        secondary_category.id = 3;
        secondary_category.cross_ax = 4;
        secondary_category.pos = if horizontal { "r" } else { "t" }.into();
        secondary_category.text = style(3.0);
        secondary_category.title_text = style(3.5);
        secondary_category.text.size = Some(28.0);
        secondary_category.title_text.bold = Some(false);
        let mut secondary_value = authored.fmt.axes[1].clone();
        secondary_value.id = 4;
        secondary_value.cross_ax = 3;
        secondary_value.pos = if horizontal { "t" } else { "r" }.into();
        secondary_value.text = style(4.0);
        secondary_value.title_text = style(4.5);
        secondary_value.text.color = Some(ColorRef::rgb(Rgba::rgb(20, 140, 60)));
        secondary_value.title_text.font = Some("DejaVu Sans".into());
        authored.fmt.axes.splice(0..0, [secondary_category, secondary_value]);
        // Bind the first series to the primary pair in reverse ID order; the secondary pair is serialized first.
        authored.series[0].fmt.axes = vec![2, 1];
        for series in authored.series.iter_mut().skip(1) {
            series.fmt.axes = vec![3, 4];
        }
        assert_eq!(authored.main_axes(), Some((1, 2)), "{kind:?}");
        assert!(authored.raw.is_none());
        let restored = roundtrip(&presentation);
        let restored = chart(&restored);
        let (category_id, value_id) = restored.main_axes().unwrap();
        let category = restored.fmt.axis(category_id).unwrap();
        let value = restored.fmt.axis(value_id).unwrap();
        assert_eq!(category.text, style(0.5), "{kind:?}");
        assert_eq!(category.title_text, style(0.75), "{kind:?}");
        assert_eq!(value.text, style(-0.5), "{kind:?}");
        assert_eq!(value.title_text, style(0.0), "{kind:?}");
    }
}
