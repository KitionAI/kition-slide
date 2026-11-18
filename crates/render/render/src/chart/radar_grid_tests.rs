#![allow(clippy::expect_used)]

use super::*;
use kition_slide_model::SlideFile;
use kition_slide_model::style::LineEnd;

fn render_grid(kind: ChartType, category_line: Option<Line>, category_grid: Option<Line>, value_axis: Axis) -> Vec<u8> {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).expect("default slide has a master");
    let mut chart = Chart::sample(kind);
    chart.categories = vec!["North".into(), "East".into(), "South".into(), "West".into()];
    chart.series.truncate(1);
    chart.series[0].values = vec![None; 4];
    chart.series[0].fmt.axes = vec![1, 2];
    chart.fmt.axes = vec![
        Axis {
            id: 1,
            kind: AxisKind::Category,
            cross_ax: 2,
            label_pos: "none".into(),
            line: category_line,
            major_grid: category_grid,
            ..Default::default()
        },
        Axis {
            id: 2,
            kind: AxisKind::Value,
            cross_ax: 1,
            label_pos: "none".into(),
            min: Some(0.0),
            max: Some(4.0),
            major_unit: Some(1.0),
            ..value_axis
        },
    ];
    let env = Env {
        rctx: &rctx,
        c: &chart,
        m: Affine::IDENTITY,
        pal: Pal { rctx: &rctx, scheme: None },
        base: Txt { font: None, font_ea: None, font_cs: None, size: 12.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None },
        size: (64.0, 64.0),
    };
    let mut context = RenderContext::new_with(64, 64, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_radar(&mut context, &env, None, Some(Rect::new(8.0, 8.0, 56.0, 56.0)));
    context.flush();
    let mut pixels = vec![0; 64 * 64 * 4];
    let pixmap = vello_cpu::PixmapMut::new(64, 64, &mut pixels).expect("pixel buffer has the requested size");
    context.render(pixmap, &mut vello_cpu::Resources::new());
    pixels
}

fn decorated_axis_line() -> Line {
    let end = LineEnd { kind: "diamond".into(), w: "lg".into(), len: "lg".into() };
    Line { head: Some(end.clone()), tail: Some(end), ..Line::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)), 0.25) }
}

#[test]
fn category_strokes_and_grids_do_not_create_radial_value_axes() {
    for kind in [ChartType::Radar, ChartType::FilledRadar] {
        let absent = render_grid(kind, None, None, Axis::default());
        assert!(absent.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0));
        for value_axis in [
            Axis::default(),
            Axis { line: Some(Line { fill: Some(Fill::None), ..Default::default() }), ..Default::default() },
            Axis { line: Some(decorated_axis_line()), deleted: true, ..Default::default() },
        ] {
            let actual = render_grid(kind, Some(decorated_axis_line()), Some(decorated_axis_line()), value_axis);
            assert_eq!(actual, absent, "category styling cannot introduce hidden or absent value axes: {kind:?}");
        }
    }
}

#[test]
fn radial_value_axes_own_the_spoke_stroke_and_endpoint_decorations() {
    for kind in [ChartType::Radar, ChartType::FilledRadar] {
        let line = Line::solid(ColorRef::rgb(Rgba::rgb(0, 160, 0)), 1.0);
        let axis = Axis { line: Some(line.clone()), ..Default::default() };
        let plain = render_grid(kind, None, None, axis.clone());
        for (x, y) in [(32, 16), (48, 32), (32, 48), (16, 32)] {
            assert!(plain[(y * 64 + x) * 4 + 1] > 0, "value-axis spoke at {x},{y}");
        }
        assert_eq!(render_grid(kind, Some(decorated_axis_line()), Some(decorated_axis_line()), axis), plain);
        let decorated = Line { tail: Some(LineEnd { kind: "triangle".into(), w: "lg".into(), len: "lg".into() }), ..line };
        let axis = Axis { line: Some(decorated), ..Default::default() };
        let with_ends = render_grid(kind, None, None, axis.clone());
        assert_ne!(with_ends, plain, "authored value-axis decorations remain supported");
        let reversed = render_grid(kind, None, None, Axis { reversed: true, ..axis });
        assert_ne!(reversed, with_ends, "reversal moves the tail to the central maximum");
        assert!(reversed.as_chunks::<4>().0.iter().any(|pixel| pixel[1] > 0 && pixel[3] > 0), "reversed spokes retain full radius");
    }
}

#[test]
fn radial_polygon_grids_remain_independent_of_value_axis_visibility() {
    let axis = Axis {
        deleted: true,
        line: Some(decorated_axis_line()),
        major_grid: Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 160, 0)), 1.0)),
        ..Default::default()
    };
    for kind in [ChartType::Radar, ChartType::FilledRadar] {
        let rings = render_grid(kind, None, None, axis.clone());
        assert!(rings.as_chunks::<4>().0.iter().any(|pixel| pixel[1] > 0 && pixel[3] > 0));
        assert_eq!(rings, render_grid(kind, None, Some(decorated_axis_line()), axis.clone()));
        assert!(rings.as_chunks::<4>().0.iter().all(|pixel| pixel[0] == 0), "hidden red axis cannot contaminate green rings");
    }
}
