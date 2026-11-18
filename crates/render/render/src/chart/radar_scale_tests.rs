use super::*;
use crate::{Image, RenderOpts, render_slide};
use kition_slide_model::{ShapeId, ShapeKind, SlideFile, Xfrm};

fn text_style() -> Txt {
    Txt { font: None, font_ea: None, font_cs: None, size: 9.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None }
}

#[test]
fn radial_scaling_preserves_authored_bounds_units_reversal_and_unobserved_ranges() {
    let text = text_style();
    let authored = Axis { min: Some(1.0), max: Some(9.0), major_unit: Some(2.0), reversed: true, ..Default::default() };
    let scale = radar_scale(&authored, 2.0, 5.0, &text);
    assert_eq!((scale.min, scale.max, scale.major, scale.reversed), (1.0, 9.0, 2.0, true));
    for (axis, lo, hi) in [
        (Axis { log_base: Some(10.0), ..Default::default() }, 1.0, 32.0),
        (Axis::default(), -2.0, 5.0),
        (Axis { min: Some(-1.0), ..Default::default() }, 1.0, 5.0),
    ] {
        let old = scale_for(&axis, lo, hi, text.size * 2.2 * 10.0, &text, false, 0.0);
        let radial = radar_scale(&axis, lo, hi, &text);
        assert_eq!((radial.min, radial.max, radial.major, radial.log), (old.min, old.max, old.major, old.log));
    }
}

fn render(kind: ChartType, highest: f64, maximum: Option<f64>) -> Image {
    let mut chart = Chart::sample(kind);
    chart.title = None;
    chart.legend = None;
    chart.categories = vec!["North".into(), "East".into(), "South".into(), "West".into()];
    chart.series.truncate(1);
    chart.series[0].values = vec![Some(highest), Some(highest * 0.8), Some(highest * 0.6), Some(highest * 0.4)];
    chart.series[0].fmt.axes = vec![1, 2];
    chart.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, cross_ax: 2, deleted: true, ..Default::default() },
        Axis { id: 2, kind: AxisKind::Value, cross_ax: 1, max: maximum, deleted: true, ..Default::default() },
    ];
    let mut presentation = SlideFile::default();
    std::sync::Arc::make_mut(&mut presentation.slides[0]).shapes = vec![Shape {
        id: ShapeId(903),
        xfrm: Some(Xfrm::new(0.0, 0.0, 240.0, 180.0)),
        kind: ShapeKind::Chart(Box::new(chart)),
        ..Default::default()
    }];
    render_slide(&presentation, 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

#[test]
fn automatic_radar_geometry_matches_nice_bounds_without_cartesian_headroom() {
    for kind in [ChartType::Radar, ChartType::FilledRadar] {
        for (highest, maximum, padded_maximum) in [(5.0, 5.0, 6.0), (34.0, 35.0, 40.0)] {
            let automatic = render(kind, highest, None);
            assert_eq!(automatic.pixels, render(kind, highest, Some(maximum)).pixels);
            assert_ne!(automatic.pixels, render(kind, highest, Some(padded_maximum)).pixels);
        }
        assert_eq!(render(kind, 32.0, None).pixels, render(kind, 32.0, Some(35.0)).pixels);
    }
}
