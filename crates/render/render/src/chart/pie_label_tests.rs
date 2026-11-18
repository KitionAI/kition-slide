use crate::{RenderOpts, render_slide};
use kition_slide_color::Rgba;
use kition_slide_model::chart::{Chart, ChartText, ChartType, DataLabels, ManualLayout};
use kition_slide_model::style::{ColorRef, Fill, Line};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile, Xfrm};

#[test]
fn inside_end_label_frames_stay_inside_the_pie_when_text_rotates() {
    for rotation in [0.0, 90.0] {
        let mut chart = Chart::sample(ChartType::Pie);
        chart.title = None;
        chart.legend = None;
        chart.vary_colors = false;
        chart.categories = vec!["Long category label".into(); 2];
        chart.fmt.first_slice_angle = 0.0;
        // A square manual plot fixes the circle at (480, 270), radius 200.
        // Equal slices put the labels on the horizontal right and left radii.
        chart.fmt.plot_layout = Some(ManualLayout {
            inner: true,
            x: Some(280.0 / 960.0),
            y: Some(70.0 / 540.0),
            w: Some(400.0 / 960.0),
            h: Some(400.0 / 540.0),
            x_edge: true,
            y_edge: true,
        });
        let series = &mut chart.series[0];
        series.values = vec![Some(1.0); 2];
        series.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 200, 220))));
        series.line = Some(Line::none());
        series.fmt.labels = Some(DataLabels {
            category: true,
            pos: Some("inEnd".into()),
            text: ChartText {
                font: Some("Inter".into()),
                size: Some(18.0),
                color: Some(ColorRef::rgb(Rgba::BLACK)),
                rot: Some(rotation),
                ..Default::default()
            },
            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)))),
            line: Some(Line::none()),
            ..Default::default()
        });
        let mut slide_file = SlideFile::default();
        std::sync::Arc::make_mut(&mut slide_file.slides[0]).shapes = vec![Shape {
            id: ShapeId(900),
            xfrm: Some(Xfrm::new(0.0, 0.0, 960.0, 540.0)),
            kind: ShapeKind::Chart(Box::new(chart)),
            ..Default::default()
        }];
        let image = render_slide(&slide_file, 0, &RenderOpts { scale: 1.0, ..Default::default() });
        let mut frames = [0usize; 2];
        for y in 0..image.height {
            for x in 0..image.width {
                let [r, g, b, a] = image.pixel(x, y);
                if r > 200 && g < 60 && b < 60 && a > 200 {
                    frames[usize::from(x >= 480)] += 1;
                    let distance = (f64::from(x) + 0.5 - 480.0).hypot(f64::from(y) + 0.5 - 270.0);
                    // Allow one raster pixel at the rim. The unrotated frame must
                    // fit too: measuring only rotated text loses its long width.
                    assert!(distance <= 201.0, "rotation {rotation}: frame pixel ({x}, {y}) lies outside the pie: {distance}");
                }
            }
        }
        assert!(frames.iter().all(|&count| count > 500), "rotation {rotation}: both label frames must be visible: {frames:?}");
    }
}
