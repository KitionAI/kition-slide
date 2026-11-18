use super::category_count;
use crate::{Image, RenderOpts, render_slide};
use kition_slide_color::Rgba;
use kition_slide_model::chart::{Axis, AxisKind, Chart, ChartType, DataTable, ManualLayout, Series};
use kition_slide_model::style::{ColorRef, Fill, Line};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile, Xfrm};

fn chart() -> Chart {
    let mut chart = Chart::sample(ChartType::Column);
    chart.title = None;
    chart.legend = None;
    chart.vary_colors = false;
    chart.gap_width = 0.5;
    chart.categories = (1..=6).map(|i| format!("Category {i}")).collect();
    chart.series.truncate(1);
    chart.series[0].values = vec![Some(2.0); 5];
    chart.series[0].fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0))));
    chart.series[0].line = Some(Line::none());
    chart.series[0].fmt.axes = vec![1, 2];
    chart.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), deleted: true, cross_ax: 2, ..Default::default() },
        Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), deleted: true, cross_ax: 1, min: Some(0.0), max: Some(4.0), ..Default::default() },
    ];
    chart.fmt.plot_layout = Some(ManualLayout {
        inner: true,
        x: Some(100.0 / 960.0),
        y: Some(100.0 / 540.0),
        w: Some(600.0 / 960.0),
        h: Some(300.0 / 540.0),
        x_edge: true,
        y_edge: true,
    });
    chart
}

fn render(chart: Chart) -> Image {
    let mut presentation = SlideFile::default();
    std::sync::Arc::make_mut(&mut presentation.slides[0]).shapes = vec![Shape {
        id: ShapeId(900),
        xfrm: Some(Xfrm::new(0.0, 0.0, 960.0, 540.0)),
        kind: ShapeKind::Chart(Box::new(chart)),
        ..Default::default()
    }];
    render_slide(&presentation, 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

#[test]
fn extra_bar_category_cache_entries_do_not_add_plot_legend_or_table_slots() {
    for mode in 0..3 {
        let mut chart = chart();
        if mode == 1 {
            chart.vary_colors = true;
            chart.legend = Some("b".into());
        } else if mode == 2 {
            chart.fmt.data_table = Some(DataTable::default());
        }
        let actual = render(chart.clone());
        chart.categories.truncate(5);
        assert_eq!(actual.pixels, render(chart).pixels, "extra category must not change rendering, mode={mode}");
        // Five 120-point slots fill the fixed 600-point plot. Each bar is 80 points wide.
        for x in [160, 280, 400, 520, 640] {
            let [r, g, b, a] = actual.pixel(x, 300);
            assert!(r > 240 && g < 20 && b < 20 && a > 240, "bar at category centre {x}, mode={mode}");
        }
    }
}

#[test]
fn sparse_and_trailing_blank_values_keep_the_longest_series_extent() {
    let mut chart = chart();
    chart.kind = ChartType::StackedColumn;
    chart.categories.push("Unused category".into());
    chart.series[0].values = vec![Some(2.0), None, Some(2.0)];
    let mut longer = chart.series[0].clone();
    longer.values = vec![None, None, None, None, Some(2.0), None];
    longer.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255))));
    chart.series.push(longer);
    let actual = render(chart.clone());
    chart.categories.truncate(6);
    assert_eq!(actual.pixels, render(chart).pixels);
    for x in [150, 350] {
        assert_eq!(actual.pixel(x, 300), [255, 0, 0, 255]);
    }
    assert_eq!(actual.pixel(550, 300), [0, 0, 255, 255]);
    for x in [250, 450, 650] {
        assert_eq!(actual.pixel(x, 300), [255, 255, 255, 255], "blank slot at {x}");
    }
}

#[test]
fn bar_category_extent_preserves_empty_data_other_kinds_and_mixed_series() {
    let mut chart = chart();
    for kind in [ChartType::Column, ChartType::StackedColumn, ChartType::PercentColumn, ChartType::Bar, ChartType::StackedBar, ChartType::PercentBar]
    {
        chart.kind = kind;
        assert_eq!(category_count(&chart), 5);
    }
    for kind in [ChartType::Line, ChartType::Pie, ChartType::Radar, ChartType::Scatter, ChartType::Combo, ChartType::Histogram, ChartType::Waterfall]
    {
        chart.kind = kind;
        assert_eq!(category_count(&chart), 6);
    }
    chart.kind = ChartType::Column;
    chart.series[0].kind = Some(ChartType::Line);
    assert_eq!(category_count(&chart), 6);
    chart.series[0].kind = None;
    chart.series[0].values.clear();
    assert_eq!(category_count(&chart), 6);
    chart.series.push(Series { values: vec![None; 8], ..Default::default() });
    assert_eq!(category_count(&chart), 8);
    chart.categories.truncate(2);
    assert_eq!(category_count(&chart), 8);
}
