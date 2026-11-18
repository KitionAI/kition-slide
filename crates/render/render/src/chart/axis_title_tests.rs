use super::*;
use crate::{Image, RenderOpts, render_slide};
use kition_slide_model::{ShapeId, ShapeKind, SlideFile, Xfrm};

fn chart(kind: ChartType) -> Chart {
    let mut chart = Chart::sample(kind);
    chart.title = None;
    chart.legend = None;
    chart.gridlines = false;
    chart.series.truncate(1);
    chart.series[0].values = vec![None; chart.categories.len()];
    chart.series[0].fmt.axes = vec![1, 2];
    let horizontal = kind.horizontal();
    chart.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, pos: if horizontal { "l" } else { "b" }.into(), deleted: true, cross_ax: 2, ..Default::default() },
        Axis {
            id: 2,
            kind: AxisKind::Value,
            pos: if horizontal { "b" } else { "l" }.into(),
            deleted: true,
            cross_ax: 1,
            min: Some(0.0),
            max: Some(4.0),
            major_unit: Some(1.0),
            major_tick: "cross".into(),
            line: Some(Line { width: Some(2.0), fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 200, 0)))), ..Default::default() }),
            text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(200, 0, 0))), ..Default::default() },
            title: Some("Authored axis title".into()),
            title_text: ChartText { size: Some(16.0), color: Some(ColorRef::rgb(Rgba::rgb(0, 0, 200))), ..Default::default() },
            ..Default::default()
        },
    ];
    chart
}

fn render(chart: Chart) -> Image {
    let mut presentation = SlideFile::default();
    std::sync::Arc::make_mut(&mut presentation.slides[0]).shapes = vec![Shape {
        id: ShapeId(901),
        xfrm: Some(Xfrm::new(0.0, 0.0, 960.0, 540.0)),
        kind: ShapeKind::Chart(Box::new(chart)),
        ..Default::default()
    }];
    render_slide(&presentation, 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

fn colored_pixels(image: &Image) -> [usize; 3] {
    let mut counts = [0; 3];
    for [r, g, b, _] in image.pixels.as_chunks::<4>().0 {
        counts[0] += usize::from(*r > 100 && *g < 80 && *b < 80);
        counts[1] += usize::from(*g > 100 && *r < 80 && *b < 80);
        counts[2] += usize::from(*b > 100 && *r < 80 && *g < 80);
    }
    counts
}

#[test]
fn hidden_value_axes_keep_authored_titles_without_line_ticks_or_labels() {
    for kind in [ChartType::Column, ChartType::Bar] {
        let mut chart = chart(kind);
        let hidden = render(chart.clone());
        let [labels, lines, title] = colored_pixels(&hidden);
        assert_eq!((labels, lines), (0, 0), "hidden axis must not regain its labels or stroke");
        assert!(title > 20, "authored title must remain visible for {kind:?}");
        assert!(!shows_labels(&chart.fmt.axes[1]));
        // The importer supplies this default for an authored title node without a text source.
        chart.fmt.axes[1].title = Some("Axis Title".into());
        let [labels, lines, title] = colored_pixels(&render(chart.clone()));
        assert_eq!((labels, lines), (0, 0));
        assert!(title > 20, "the imported default title is also an authored title element");
        chart.fmt.axes[1].deleted = false;
        let [labels, lines, title] = colored_pixels(&render(chart));
        assert!(labels > 0 && lines > 0 && title > 20, "visible control must paint all three elements");
    }
}

#[test]
fn hidden_axes_without_title_text_do_not_invent_a_title() {
    for kind in [ChartType::Column, ChartType::Bar] {
        let mut chart = chart(kind);
        chart.fmt.axes[1].title = None;
        let untitled = render(chart.clone());
        assert_eq!(colored_pixels(&untitled), [0, 0, 0]);
        chart.fmt.axes[1].title = Some(" \t ".into());
        assert_eq!(render(chart).pixels, untitled.pixels);
    }
}
