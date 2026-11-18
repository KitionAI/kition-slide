use super::*;
use crate::{Image, Renderer};
use kition_slide_color::Rgba;
use kurbo::{Affine, Cap, Circle, Point, Rect, Shape};

fn render(path: &BezPath, style: &Stroke, corrected: bool, setup: impl FnOnce(&mut RenderContext)) -> Image {
    let mut ctx = RenderContext::new_with(128, 128, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    ctx.set_paint(crate::color(Rgba::rgb(30, 80, 200), 0.5));
    ctx.set_stroke(style.clone());
    setup(&mut ctx);
    if corrected {
        paint(&mut ctx, path);
    } else {
        ctx.stroke_path(path);
    }
    Renderer::default().finish(ctx, 128, 128)
}

fn alpha_bounds(image: &Image) -> Option<(usize, usize, usize, usize)> {
    let mut bounds = None;
    for (i, pixel) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let (x, y) = (i % 128, i / 128);
        bounds =
            Some(bounds.map_or((x, y, x + 1, y + 1), |(l, t, r, b): (usize, usize, usize, usize)| (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1))));
    }
    bounds
}

#[test]
fn drawable_zero_segments_have_pixels_with_authored_width_and_opacity() {
    for source in ["M32 32L32 32", "M32 32Z", "M32 32Q32 32 32 32", "M32 32C32 32 32 32 32 32"] {
        let path = BezPath::from_svg(source).unwrap();
        for cap in [Cap::Round, Cap::Square] {
            let stroke = Stroke::new(8.0).with_caps(cap);
            assert!(alpha_bounds(&render(&path, &stroke, false, |_| {})).is_none(), "native missing-cap control");
            let corrected = render(&path, &stroke, true, |_| {});
            assert_eq!(alpha_bounds(&corrected), Some((28, 28, 36, 36)), "{source} {cap:?}");
            assert!((127..=129).contains(&corrected.pixel(32, 32)[3]));
            if cap == Cap::Round {
                assert_eq!(corrected.pixel(28, 28)[3], 0);
            } else {
                assert!((127..=129).contains(&corrected.pixel(28, 28)[3]));
            }
        }
    }
}

#[test]
fn moves_butt_caps_and_all_nonzero_paths_remain_byte_identical() {
    for source in ["M32 32", "M32 32M48 48", "M20 20L70 35", "M20 20C70 20 70 70 20 20Z", "M20 20L20 20L70 35"] {
        let path = BezPath::from_svg(source).unwrap();
        for cap in [Cap::Round, Cap::Square, Cap::Butt] {
            let stroke = Stroke::new(7.0).with_caps(cap);
            assert_eq!(render(&path, &stroke, true, |_| {}).pixels, render(&path, &stroke, false, |_| {}).pixels, "{source} {cap:?}");
        }
    }
    let point = BezPath::from_svg("M32 32L32 32").unwrap();
    assert!(alpha_bounds(&render(&point, &Stroke::new(8.0).with_caps(Cap::Butt), true, |_| {})).is_none());
}

#[test]
fn mixed_paths_keep_nonzero_pixels_and_composite_overlapping_caps_once() {
    let base = BezPath::from_svg("M20 32C40 10 60 54 80 32").unwrap();
    let mut mixed = base.clone();
    mixed.move_to((32.0, 80.0));
    mixed.line_to((32.0, 80.0));
    let stroke = Stroke::new(8.0).with_caps(Cap::Round);
    let before = render(&base, &stroke, false, |_| {});
    let after = render(&mixed, &stroke, true, |_| {});
    for y in 0..60 {
        for x in 0..128 {
            assert_eq!(before.pixel(x, y), after.pixel(x, y), "nonzero neighbor at {x},{y}");
        }
    }
    let square = BezPath::from_svg("M20 20L80 20L80 80L20 80Z").unwrap();
    for mut path in [square.clone(), square.reverse_subpaths()] {
        path.move_to((40.0, 20.0));
        path.line_to((40.0, 20.0));
        path.move_to((40.0, 20.0));
        path.close_path();
        let image = render(&path, &stroke, true, |_| {});
        assert!((127..=129).contains(&image.pixel(40, 20)[3]), "overlaps must not multiply paint alpha");
        assert_eq!(image.pixel(40, 40)[3], 0, "closed stroke center stays empty");
    }
}

fn gradient_setup(ctx: &mut RenderContext, transform: Affine) {
    let stops = [
        vello_cpu::peniko::ColorStop { offset: 0.0, color: crate::color(Rgba::rgb(255, 0, 0), 0.5).into() },
        vello_cpu::peniko::ColorStop { offset: 1.0, color: crate::color(Rgba::rgb(0, 0, 255), 0.5).into() },
    ];
    ctx.set_paint(vello_cpu::peniko::Gradient::new_linear(Point::new(20.0, 32.0), Point::new(44.0, 32.0)).with_stops(stops.as_slice()));
    ctx.set_paint_transform(Affine::translate((3.0, -2.0)));
    ctx.set_transform(transform);
}

#[test]
fn transformed_gradient_caps_match_explicit_shapes_and_preserve_fill_rule() {
    let path = BezPath::from_svg("M32 32L32 32").unwrap();
    let transform = Affine::translate((64.0, 48.0)) * Affine::rotate(0.4) * Affine::scale_non_uniform(-1.5, 0.6) * Affine::translate((-32.0, -32.0));
    for cap in [Cap::Round, Cap::Square] {
        let mut ctx = RenderContext::new_with(128, 128, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        gradient_setup(&mut ctx, transform);
        ctx.set_stroke(Stroke::new(8.0).with_caps(cap));
        ctx.set_fill_rule(Fill::EvenOdd);
        paint(&mut ctx, &path);
        assert_eq!(*ctx.fill_rule(), Fill::EvenOdd);
        let actual = Renderer::default().finish(ctx, 128, 128);
        let mut expected_ctx = RenderContext::new_with(128, 128, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        gradient_setup(&mut expected_ctx, transform);
        let tolerance = 0.25 / transform.as_coeffs()[0].abs().max(transform.as_coeffs()[3].abs()).max(1.0);
        let expected_path =
            if cap == Cap::Round { Circle::new((32.0, 32.0), 4.0).to_path(tolerance) } else { Rect::new(28.0, 28.0, 36.0, 36.0).to_path(tolerance) };
        expected_ctx.fill_path(&expected_path);
        let expected = Renderer::default().finish(expected_ctx, 128, 128);
        assert_eq!(actual.pixels, expected.pixels);
        assert!((127..=129).contains(&actual.pixel(64, 48)[3]));
    }
}

#[test]
fn clipped_cap_and_layer_opacity_are_preserved() {
    let path = BezPath::from_svg("M32 32L32 32").unwrap();
    let mut ctx = RenderContext::new_with(128, 128, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    ctx.push_clip_layer(&Rect::new(0.0, 0.0, 32.0, 128.0).to_path(0.1));
    ctx.push_opacity_layer(0.5);
    ctx.set_paint(crate::color(Rgba::rgb(30, 80, 200), 0.5));
    ctx.set_stroke(Stroke::new(8.0).with_caps(Cap::Round));
    paint(&mut ctx, &path);
    ctx.pop_layer();
    ctx.pop_layer();
    let image = Renderer::default().finish(ctx, 128, 128);
    assert!((63..=65).contains(&image.pixel(31, 32)[3]));
    assert_eq!(image.pixel(32, 32)[3], 0);
}

#[test]
fn native_ink_single_samples_use_the_shared_drawable_center_line() {
    use kition_slide_model::{InkStroke, Shape, ShapeId, ShapeKind, SlideFile, Xfrm};
    for points in [vec![(32.0, 32.0, 0.5)], vec![(32.0, 32.0, 0.5), (32.0, 32.0, 0.5)]] {
        let mut presentation = SlideFile { slide_size: kurbo::Size::new(128.0, 128.0), ..Default::default() };
        std::sync::Arc::make_mut(&mut presentation.slides[0]).shapes = vec![Shape {
            id: ShapeId(700),
            xfrm: Some(Xfrm::new(0.0, 0.0, 128.0, 128.0)),
            kind: ShapeKind::Ink { strokes: vec![InkStroke { points, width: 8.0, color: Rgba::rgb(30, 80, 200), highlighter: false }] },
            ..Default::default()
        }];
        let image = crate::render_slide(&presentation, 0, &crate::RenderOpts { scale: 1.0, edit: false, ..Default::default() });
        assert_eq!(image.pixel(32, 32), [30, 80, 200, 255]);
        assert_eq!(image.pixel(36, 32), [255, 255, 255, 255]);
        assert_eq!(image.pixel(28, 28), [255, 255, 255, 255]);
    }
}
