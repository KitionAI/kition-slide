#![allow(clippy::expect_used)]

use super::{Env, Pal, Txt, draw_marker, draw_scatter, point_marker};
use kition_slide_color::Rgba;
use kition_slide_model::SlideFile;
use kition_slide_model::chart::{Chart, ChartType, PointMarker, Series};
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorRef, Effects, Fill, Line, Shadow};
use kurbo::{Affine, Point, Rect};
use vello_cpu::RenderContext;

const WIDTH: usize = 256;
const HEIGHT: usize = 128;

fn effect(distance: f64) -> Effects {
    Effects {
        outer_shadow: Some(Shadow {
            color: ColorRef::rgb(Rgba::rgba(0, 0, 0, 128)),
            blur: 0.0,
            dist: distance,
            dir: 0.0,
            inner: false,
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            align: "ctr".into(),
            rotate_with_shape: true,
        }),
        ..Default::default()
    }
}

fn render_with(chart: &Chart, draw: impl FnOnce(&mut RenderContext, &Env)) -> Vec<u8> {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).expect("default slide has a master");
    let env = Env {
        rctx: &rctx,
        c: chart,
        m: Affine::IDENTITY,
        pal: Pal { rctx: &rctx, scheme: None },
        base: Txt { font: None, font_ea: None, font_cs: None, size: 12.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None },
        size: (WIDTH as f64, HEIGHT as f64),
    };
    let mut context = RenderContext::new_with(WIDTH as u16, HEIGHT as u16, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw(&mut context, &env);
    context.flush();
    let mut pixels = vec![0; WIDTH * HEIGHT * 4];
    let pixmap = vello_cpu::PixmapMut::new(WIDTH as u16, HEIGHT as u16, &mut pixels).expect("pixel buffer has the requested size");
    context.render(pixmap, &mut vello_cpu::Resources::new());
    pixels
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    pixels[(y * WIDTH + x) * 4..(y * WIDTH + x + 1) * 4].try_into().expect("one RGBA pixel")
}

fn marker_chart() -> Chart {
    let mut chart = Chart::sample(ChartType::Scatter);
    let mut series = Series {
        values: vec![Some(32.0), Some(32.0)],
        x_values: vec![Some(32.0), Some(96.0)],
        marker: Some("circle".into()),
        line: Some(Line::none()),
        ..Default::default()
    };
    series.fmt.marker_size = Some(12.0);
    series.fmt.marker_fill = Some(Fill::Solid { color: ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)) });
    series.fmt.marker_line = Some(Line::none());
    series.fmt.marker_effects = Some(effect(24.0));
    chart.series = vec![series];
    chart
}

fn markers(chart: &Chart) -> Vec<u8> {
    render_with(chart, |context, env| {
        draw_scatter(context, env, Rect::new(0.0, 0.0, 128.0, 128.0), &|v| v, &|v| v, &mut vec![]);
    })
}

#[test]
fn marker_shadow_precedes_the_connecting_line_and_marker_foreground() {
    let mut chart = marker_chart();
    chart.series[0].line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 2.0));
    let pixels = markers(&chart);
    assert_eq!(pixel(&pixels, 56, 32), [0, 0, 255, 255], "connecting line remains above the offset shadow");
    assert!(pixel(&pixels, 56, 36)[3] > 0, "shadow is present away from the foreground line");
    // Keep the single-fill check away from the line itself.
    assert_eq!(pixel(&pixels, 32, 35)[3], 128, "marker foreground is painted once");
}

#[test]
fn point_marker_inherits_only_marker_effects_and_empty_list_clears_them() {
    let mut chart = marker_chart();
    let series = &mut chart.series[0];
    series.values = vec![Some(24.0), Some(64.0), Some(104.0)];
    series.x_values = vec![Some(32.0); 3];
    series.fmt.shadow = effect(100.0).outer_shadow;
    series.fmt.point_markers = vec![
        (0, PointMarker { size: Some(10.0), ..Default::default() }),
        (1, PointMarker { effects: Some(Effects::default()), ..Default::default() }),
        (2, PointMarker { effects: Some(effect(48.0)), ..Default::default() }),
    ];
    assert_eq!(point_marker(series, 0).fmt.marker_effects, series.fmt.marker_effects);
    assert!(point_marker(series, 1).fmt.marker_effects.as_ref().expect("explicit empty effects").is_empty());
    let pixels = markers(&chart);
    assert!(pixel(&pixels, 56, 24)[3] > 0, "missing point list inherits series marker effect");
    assert_eq!(pixel(&pixels, 56, 64)[3], 0, "explicit empty point list clears the shadow");
    assert!(pixel(&pixels, 80, 104)[3] > 0, "point shadow overrides the series marker shadow");
    assert_eq!(pixel(&pixels, 56, 104)[3], 0, "no duplicate series marker shadow");
    assert_eq!(pixel(&pixels, 132, 24)[3], 0, "series body shadow never becomes a marker effect");
}

#[test]
fn series_body_shadow_does_not_fill_missing_or_empty_marker_effects() {
    for marker_effects in [None, Some(Effects::default())] {
        let mut chart = marker_chart();
        chart.series[0].fmt.shadow = effect(24.0).outer_shadow;
        chart.series[0].fmt.marker_effects = marker_effects;
        let pixels = markers(&chart);
        assert_eq!(pixel(&pixels, 56, 32)[3], 0);
        assert_eq!(pixel(&pixels, 32, 32)[3], 128);
    }
}

#[test]
fn marker_none_or_absent_paint_casts_no_shadow() {
    let mut chart = marker_chart();
    chart.series[0].marker = Some("none".into());
    assert!(markers(&chart).as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    chart.series[0].marker = Some("circle".into());
    chart.series[0].fmt.marker_fill = Some(Fill::None);
    assert!(markers(&chart).as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    chart.series[0].fmt.marker_fill = Some(Fill::Solid { color: ColorRef::rgb(Rgba::rgba(255, 0, 0, 0)) });
    assert!(markers(&chart).as_chunks::<4>().0.iter().all(|p| p[3] == 0));
}

#[test]
fn legend_marker_foreground_does_not_implicitly_draw_plotted_effects() {
    let chart = marker_chart();
    let series = &chart.series[0];
    let pixels = render_with(&chart, |context, env| {
        draw_marker(context, env, series, Rgba::BLACK, "circle", 12.0, Point::new(32.0, 32.0));
    });
    assert_eq!(pixel(&pixels, 32, 32)[3], 128);
    assert_eq!(pixel(&pixels, 56, 32)[3], 0);
}

fn marker_glow(color: Rgba, radius: f64) -> Effects {
    Effects { glow: Some(kition_slide_model::style::Glow { color: ColorRef::rgb(color), radius }), ..Default::default() }
}

#[test]
fn authored_marker_glow_stays_below_the_connecting_line_and_foreground() {
    let mut chart = marker_chart();
    let series = &mut chart.series[0];
    series.fmt.marker_effects = Some(marker_glow(Rgba::rgba(0, 0, 255, 128), 8.0));
    series.fmt.marker_fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0))));
    series.line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 255, 0)), 2.0));
    let pixels = markers(&chart);
    assert_eq!(pixel(&pixels, 32, 32), [255, 0, 0, 255], "opaque marker remains foreground");
    assert_eq!(pixel(&pixels, 44, 32), [0, 255, 0, 255], "connecting line remains above marker glow");
    let halo = pixel(&pixels, 32, 44);
    assert!(halo[3] > 0 && halo[3] <= 128 && halo[2] == halo[3] && halo[0] == 0 && halo[1] == 0, "authored blue glow alpha applies once: {halo:?}");
    assert_eq!(pixel(&pixels, 32, 60)[3], 0, "blur remains bounded around the marker");
}

#[test]
fn point_marker_glow_inheritance_empty_clear_and_color_override_are_independent() {
    let mut chart = marker_chart();
    let series = &mut chart.series[0];
    series.values = vec![Some(24.0), Some(64.0), Some(104.0)];
    series.x_values = vec![Some(32.0); 3];
    series.fmt.marker_fill = Some(Fill::solid(ColorRef::rgb(Rgba::WHITE)));
    series.fmt.marker_effects = Some(marker_glow(Rgba::rgb(0, 0, 255), 8.0));
    series.fmt.point_markers = vec![
        (0, PointMarker { size: Some(12.0), ..Default::default() }),
        (1, PointMarker { effects: Some(Effects::default()), ..Default::default() }),
        (2, PointMarker { effects: Some(marker_glow(Rgba::rgb(255, 0, 0), 8.0)), ..Default::default() }),
    ];
    let pixels = markers(&chart);
    let inherited = pixel(&pixels, 44, 24);
    assert!(inherited[2] > 0 && inherited[0] == 0);
    assert_eq!(pixel(&pixels, 44, 64)[3], 0, "explicit empty point list clears glow");
    let overridden = pixel(&pixels, 44, 104);
    assert!(overridden[0] > 0 && overridden[2] == 0, "point glow replaces the series marker glow");
}

#[test]
fn marker_glow_is_suppressed_without_visible_paint_and_does_not_leak_into_legend_keys() {
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_effects = Some(marker_glow(Rgba::rgb(0, 0, 255), 8.0));
    let legend = render_with(&chart, |ctx, env| draw_marker(ctx, env, &chart.series[0], Rgba::BLACK, "circle", 12.0, Point::new(32.0, 32.0)));
    assert_eq!(pixel(&legend, 44, 32)[3], 0, "legend foreground does not acquire plotted glow");
    for fill in [Fill::None, Fill::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 0)))] {
        chart.series[0].fmt.marker_fill = Some(fill);
        assert!(markers(&chart).as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    }
    chart.series[0].fmt.marker_fill = Some(Fill::solid(ColorRef::rgb(Rgba::WHITE)));
    chart.series[0].marker = Some("none".into());
    assert!(markers(&chart).as_chunks::<4>().0.iter().all(|p| p[3] == 0));
}

#[test]
fn marker_glow_keeps_open_markers_unfilled_and_rejects_unsafe_extents() {
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_effects = Some(marker_glow(Rgba::rgba(0, 0, 255, 128), 1.0));
    chart.series[0].fmt.marker_line = Some(Line::solid(ColorRef::rgb(Rgba::WHITE), 1.0));
    let open = render_with(&chart, |ctx, env| {
        super::marker_effects::below(ctx, env, &chart.series[0], Rgba::BLACK, "plus", 32.0, Point::new(40.0, 40.0));
    });
    assert!(pixel(&open, 40, 48)[3] > 0, "open marker casts glow from its stroke");
    assert_eq!(pixel(&open, 48, 48)[3], 0, "open marker does not acquire a filled interior");
    for (radius, x) in [(f64::NAN, 32.0), (f64::INFINITY, 32.0), (1e12, 32.0), (8.0, 1e12)] {
        chart.series[0].fmt.marker_effects = Some(marker_glow(Rgba::WHITE, radius));
        let pixels = render_with(&chart, |ctx, env| {
            super::marker_effects::below(ctx, env, &chart.series[0], Rgba::BLACK, "circle", 12.0, Point::new(x, 32.0));
        });
        assert!(pixels.iter().all(|v| *v == 0), "unsafe glow is skipped before the filter layer");
    }
}

fn shadow_only(chart: &Chart) -> Vec<u8> {
    render_with(chart, |context, env| {
        let series = &chart.series[0];
        super::marker_effects::shadow(context, env, series, Rgba::BLACK, "square", 12.0, Point::new(32.0, 32.0));
    })
}

#[test]
fn filled_marker_shadow_includes_visible_outline_without_double_opacity() {
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0))));
    chart.series[0].fmt.marker_line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 4.0));
    let image = shadow_only(&chart);
    assert_eq!(pixel(&image, 56, 32), [0, 0, 0, 128], "opaque fill casts one shadow");
    assert_eq!(pixel(&image, 56, 25), [0, 0, 0, 128], "outline extends beyond the fill silhouette");
    assert_eq!(pixel(&image, 56, 26), [0, 0, 0, 128], "overlapping fill and outline are not separate shadows");
    assert_eq!(pixel(&image, 56, 23)[3], 0);
    assert_eq!(pixel(&image, 32, 32)[3], 0, "the shadow layer does not leak foreground paint");
    chart.series[0].fmt.marker_line = Some(Line::none());
    assert_eq!(pixel(&shadow_only(&chart), 56, 25)[3], 0, "an invisible outline does not enlarge the shadow");
}

#[test]
fn marker_shadow_respects_partial_fill_alpha_and_outline_only_holes() {
    let mut chart = marker_chart();
    let image = shadow_only(&chart);
    assert!(pixel(&image, 56, 32)[3].abs_diff(64) <= 1, "half-opacity fill times half-opacity shadow");
    chart.series[0].fmt.marker_fill = Some(Fill::None);
    chart.series[0].fmt.marker_line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 4.0));
    let image = shadow_only(&chart);
    assert_eq!(pixel(&image, 56, 32)[3], 0, "unfilled marker center remains absent from the mask");
    assert_eq!(pixel(&image, 56, 25)[3], 128);
}

#[test]
fn marker_shadow_retains_spatial_gradient_alpha() {
    use kition_slide_model::style::{Gradient, GradientShape, GradientStop};
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_fill = Some(Fill::Gradient(Gradient {
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgba(255, 0, 0, 0)) },
            GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0, 0, 255)) },
        ],
        shape: GradientShape::Linear { angle: 0.0, scaled: false },
        tile_rect: None,
        rotate_with_shape: true,
    }));
    let image = shadow_only(&chart);
    assert!(pixel(&image, 51, 32)[3] < pixel(&image, 60, 32)[3]);
    assert!(pixel(&image, 60, 32)[3] < 128);
    assert_eq!(pixel(&image, 56, 32)[0..3], [0, 0, 0], "source gradient colors are replaced by shadow color");
}

#[test]
fn marker_shadow_keeps_compound_outline_gaps_transparent() {
    use kition_slide_model::style::Compound;
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_fill = Some(Fill::None);
    chart.series[0].fmt.marker_line = Some(Line { compound: Some(Compound::ThinThick), ..Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 10.0) });
    // A 24pt marker leaves a valid inner contour at this 10pt stroke width.
    let center = Point::new(32.0, 32.0);
    let (path, _) = super::marker_path("square", center, 24.0);
    let geometry = kurbo::Stroke::new(10.0).with_join(kurbo::Join::Round);
    assert!(crate::compound::closed_outline(&path, &geometry, Some(Compound::ThinThick)).is_some());
    let (narrow, _) = super::marker_path("square", center, 12.0);
    assert!(crate::compound::closed_outline(&narrow, &geometry, Some(Compound::ThinThick)).is_none(), "unsupported narrow marker remains guarded");
    let image = render_with(&chart, |context, env| {
        super::marker_effects::shadow(context, env, &chart.series[0], Rgba::BLACK, "square", 24.0, center);
    });
    assert_eq!(pixel(&image, 56, 16)[3], 128, "thin exterior band");
    assert_eq!(pixel(&image, 56, 18)[3], 0, "transparent compound gap");
    assert_eq!(pixel(&image, 56, 22)[3], 128, "thick interior band");
    assert_eq!(pixel(&image, 56, 32)[3], 0, "no marker fill");
}

#[test]
fn marker_source_and_shadow_color_alpha_each_apply_once() {
    let mut chart = marker_chart();
    chart.series[0].fmt.marker_fill = None;
    let shadow = chart.series[0].fmt.marker_effects.as_mut().expect("marker effects").outer_shadow.as_mut().expect("marker shadow");
    shadow.color = ColorRef::rgb(Rgba::rgba(200, 100, 50, 128));
    let image = render_with(&chart, |context, env| {
        super::marker_effects::shadow(context, env, &chart.series[0], Rgba::rgba(10, 220, 90, 128), "square", 12.0, Point::new(32.0, 32.0));
    });
    let actual = pixel(&image, 56, 32);
    assert!(
        actual.iter().zip([50_u8, 25, 12, 64]).all(|(a, b)| a.abs_diff(b) <= 1),
        "source alpha times shadow alpha, then RGB premultiplication: {actual:?}"
    );
}
