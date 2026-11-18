use super::*;
use crate::{Image, RenderOpts, render_slide};
use kition_slide_model::{ShapeId, ShapeKind, SlideFile, Xfrm};

fn fixture(kind: ChartType, percent: bool, lower: Vec<Option<f64>>, upper: Vec<Option<f64>>) -> Chart {
    let mut chart = Chart::sample(kind);
    chart.title = None;
    chart.legend = None;
    chart.categories = (0..lower.len()).map(|i| i.to_string()).collect();
    chart.series.truncate(2);
    for (series, values) in chart.series.iter_mut().zip([lower, upper]) {
        series.values = values;
        series.line = Some(Line::none());
        series.fill = Some(Fill::None);
        series.marker = Some("none".into());
        series.fmt.axes = vec![1, 2];
    }
    chart.series[1].fmt.err_bars = vec![ErrorBars {
        kind: "both".into(),
        val_type: "stdErr".into(),
        line: Some(Line::solid(ColorRef::rgb(Rgba::BLACK), 1.0)),
        ..Default::default()
    }];
    if percent {
        chart.raw = Some("\"percentStacked\"".into());
    }
    chart.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, cross_ax: 2, deleted: true, ..Default::default() },
        Axis { id: 2, kind: AxisKind::Value, cross_ax: 1, min: Some(-20.0), max: Some(20.0), deleted: true, ..Default::default() },
    ];
    chart.fmt.plot_layout = Some(ManualLayout { inner: true, x: Some(0.1), y: Some(0.1), w: Some(0.8), h: Some(0.8), x_edge: true, y_edge: true });
    chart
}

fn render(chart: Chart) -> Image {
    let mut presentation = SlideFile::default();
    std::sync::Arc::make_mut(&mut presentation.slides[0]).shapes = vec![Shape {
        id: ShapeId(940),
        xfrm: Some(Xfrm::new(0.0, 0.0, 400.0, 400.0)),
        kind: ShapeKind::Chart(Box::new(chart)),
        ..Default::default()
    }];
    render_slide(&presentation, 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

fn matches_fixed_amount(chart: Chart, amount: f64) {
    let actual = render(chart.clone());
    let mut fixed = chart.clone();
    fixed.series[1].fmt.err_bars[0].val_type = "fixedVal".into();
    fixed.series[1].fmt.err_bars[0].val = amount;
    assert_eq!(actual.pixels, render(fixed).pixels, "standard error differs from expected {amount}");
    let mut absent = chart;
    absent.series[1].fmt.err_bars.clear();
    if amount == 0.0 {
        assert_eq!(actual.pixels, render(absent).pixels, "zero error must not invent end caps");
    } else {
        assert_ne!(actual.pixels, render(absent).pixels, "the expected error bars must be visible");
    }
}

#[test]
fn percentage_stack_has_zero_error_at_its_constant_total() {
    for kind in [ChartType::PercentColumn, ChartType::PercentBar, ChartType::StackedLine] {
        matches_fixed_amount(fixture(kind, true, vec![Some(1.0), Some(2.0), Some(3.0)], vec![Some(3.0), Some(2.0), Some(1.0)]), 0.0);
    }
}

#[test]
fn percentage_standard_error_uses_normalized_cumulative_values() {
    for kind in [ChartType::PercentColumn, ChartType::PercentBar, ChartType::StackedLine] {
        let mut chart = fixture(kind, true, vec![Some(1.0); 3], vec![Some(1.0); 3]);
        let mut third = chart.series[0].clone();
        third.values = vec![Some(1.0), Some(2.0), Some(3.0)];
        chart.series.push(third);
        chart.fmt.axes[1].min = Some(0.0);
        chart.fmt.axes[1].max = Some(1.0);
        // The middle cumulative coordinates are [2/3, 1/2, 2/5], with SE 7/90.
        matches_fixed_amount(chart, 7.0 / 90.0);
    }
}

#[test]
fn percentage_statistics_follow_sign_stacks_and_skip_missing_points() {
    for kind in [ChartType::PercentColumn, ChartType::PercentBar] {
        // Normalized sign-separated endpoints are [1/2, -3/4, 1], with SE sqrt(13/48).
        let mut chart = fixture(kind, true, vec![Some(-1.0), Some(1.0), Some(2.0)], vec![Some(1.0), Some(-3.0), Some(4.0)]);
        chart.fmt.axes[1].min = Some(-1.0);
        chart.fmt.axes[1].max = Some(1.0);
        matches_fixed_amount(chart, (13.0_f64 / 48.0).sqrt());
    }
    for kind in [ChartType::PercentColumn, ChartType::PercentBar, ChartType::StackedLine] {
        // Only actual upper points [1, 1] enter the statistic; a missing point is not zero.
        matches_fixed_amount(fixture(kind, true, vec![Some(1.0), Some(3.0), Some(5.0)], vec![Some(1.0), None, Some(1.0)]), 0.0);
    }
}

#[test]
fn ordinary_stacks_non_stacked_and_other_error_modes_keep_source_statistics() {
    for kind in [ChartType::Column, ChartType::Bar, ChartType::Line] {
        matches_fixed_amount(fixture(kind, false, vec![Some(20.0); 3], vec![Some(1.0), Some(3.0), Some(5.0)]), (4.0_f64 / 3.0).sqrt());
    }
    // Ordinary-stack cumulative statistics remain a separate compatibility gap.
    for kind in [ChartType::StackedColumn, ChartType::StackedBar, ChartType::StackedLine] {
        matches_fixed_amount(fixture(kind, false, vec![Some(1.0), Some(3.0), Some(5.0)], vec![Some(1.0); 3]), 0.0);
    }
    let chart = Chart::sample(ChartType::StackedColumn);
    for mode in ["stdDev", "fixedVal", "percentage", "cust"] {
        let error = ErrorBars { val_type: mode.into(), ..Default::default() };
        assert_eq!(category_err_values(&chart, &chart.series[1], &error, true, &[Some(999.0)]), err_values(&chart, &chart.series[1], &error, false));
    }
}
