//! Pixel coverage for isolated, opaque, axis-aligned straight strokes.

use kition_slide_model::style::{Compound, Line, LineCap};
use kurbo::{Affine, BezPath, PathEl};

/// Return local geometry with a one-device-pixel pen. Only the normal coordinate moves;
/// the caller must retain the original paint bounds and transform. This deliberately does
/// not change closed outlines, joints, caps, arrowheads, or nonuniformly scaled strokes.
pub(super) fn segment(path: &BezPath, transform: Affine, width: f64, line: &Line) -> Option<(BezPath, f64)> {
    if line.cap.unwrap_or_default() != LineCap::Flat
        || line.compound.unwrap_or_default() != Compound::Single
        || line.dash.as_ref().is_some_and(|dash| !dash.pattern().is_empty())
        || line.head.as_ref().is_some_and(|end| end.kind != "none")
        || line.tail.as_ref().is_some_and(|end| end.kind != "none")
        || line.pen_align.as_ref().is_some_and(|align| align != "ctr")
    {
        return None;
    }
    let [PathEl::MoveTo(start), PathEl::LineTo(end)] = path.elements() else {
        return None;
    };
    let [xx, yx, xy, yy, _, _] = transform.as_coeffs();
    let (sx, sy) = (xx.hypot(yx), xy.hypot(yy));
    if !transform.is_finite()
        || !sx.is_finite()
        || !sy.is_finite()
        || sx <= 0.0
        || sy <= 0.0
        || (sx - sy).abs() > 1e-8 * sx.max(sy)
        || ((xx / sx) * (xy / sy) + (yx / sx) * (yy / sy)).abs() > 1e-8
        || !transform.determinant().is_finite()
    {
        return None;
    }
    let inverse = transform.inverse();
    // Restrict quantization to the nearest-integer bin for a one-pixel pen. Wider pens
    // and sub-half-pixel strokes retain their continuous coverage rather than changing bins.
    if !inverse.is_finite() || !(0.5..1.5).contains(&(width * sx)) {
        return None;
    }
    let (mut start, mut end) = (transform * *start, transform * *end);
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let delta = end - start;
    if delta.y.abs() < 1e-8 && delta.x.abs() >= 1e-8 {
        let center = pixel_center(start.y);
        start.y = center;
        end.y = center;
    } else if delta.x.abs() < 1e-8 && delta.y.abs() >= 1e-8 {
        let center = pixel_center(start.x);
        start.x = center;
        end.x = center;
    } else {
        return None;
    }
    let (start, end, width) = (inverse * start, inverse * end, 1.0 / sx);
    if !start.is_finite() || !end.is_finite() || !width.is_finite() {
        return None;
    }
    let mut aligned = BezPath::new();
    aligned.move_to(start);
    aligned.line_to(end);
    Some((aligned, width))
}

// Composed transforms can place an exact integer a few representable values below it.
// Stabilize only that rounding neighborhood; genuine subpixel phases keep their own bin.
fn pixel_center(coordinate: f64) -> f64 {
    let integer = coordinate.round();
    let (mut lower, mut upper) = (integer, integer);
    for _ in 0..4 {
        lower = lower.next_down();
        upper = upper.next_up();
    }
    let stable = if (lower..=upper).contains(&coordinate) { integer } else { coordinate };
    stable.floor() + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_color::Rgba;
    use kition_slide_geom::preset::FillMode;
    use kition_slide_model::style::{ColorRef, Dash, Fill, Gradient, GradientShape, GradientStop, LineEnd};
    use kition_slide_model::{CustomPath, Geom, Shape, ShapeId, Xfrm};
    use kurbo::Point;

    fn path(start: Point, end: Point) -> BezPath {
        let mut path = BezPath::new();
        path.move_to(start);
        path.line_to(end);
        path
    }

    #[test]
    fn normal_alignment_preserves_tangent_endpoints_across_scale_reflection_and_quarter_turns() {
        let line = Line::solid(ColorRef::rgb(Rgba::BLACK), 0.5);
        let original = path(Point::new(10.2, 20.5), Point::new(100.8, 20.5));
        for (numerator, denominator) in [(1_i64, 2_i64), (1, 1), (4, 3), (2, 1), (3, 1)] {
            let scale = numerator as f64 / denominator as f64;
            for (turn, rotate) in [0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI].into_iter().enumerate() {
                // Exact rational normal coordinates avoid both trigonometric rounding and
                // the implementation's boundary-stabilization logic in this oracle.
                let (normal_numerator, normal_denominator) = match turn {
                    0 => (101 * denominator + 41 * numerator, 2 * denominator),
                    1 => (321 * denominator - 164 * numerator, 8 * denominator),
                    _ => (101 * denominator - 41 * numerator, 2 * denominator),
                };
                let expected_normal = normal_numerator.div_euclid(normal_denominator) as f64 + 0.5;
                for reflect in [1.0, -1.0] {
                    let transform = Affine::translate((40.125, 50.5)) * Affine::rotate(rotate) * Affine::scale_non_uniform(scale * reflect, scale);
                    let (aligned, width) = segment(&original, transform, 0.8 / scale, &line).unwrap();
                    assert!((width * scale - 1.0).abs() < 1e-12);
                    let [PathEl::MoveTo(a), PathEl::LineTo(b)] = aligned.elements() else { panic!("single segment") };
                    let (a, b) = (transform * *a, transform * *b);
                    let (old_a, old_b) = (transform * Point::new(10.2, 20.5), transform * Point::new(100.8, 20.5));
                    if turn != 1 {
                        assert!((a.y - expected_normal).abs() < 1e-9 && (b.y - a.y).abs() < 1e-9);
                        assert!((a.x - old_a.x).abs() < 1e-9 && (b.x - old_b.x).abs() < 1e-9);
                    } else {
                        assert!((a.x - expected_normal).abs() < 1e-9 && (b.x - a.x).abs() < 1e-9);
                        assert!((a.y - old_a.y).abs() < 1e-9 && (b.y - old_b.y).abs() < 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn integer_boundaries_tolerate_only_four_representable_neighbors() {
        for integer in [-1000.0_f64, -1.0, 1.0, 1000.0] {
            let (mut below, mut above) = (integer, integer);
            for _ in 0..4 {
                below = below.next_down();
                above = above.next_up();
                assert_eq!(pixel_center(below), integer + 0.5);
                assert_eq!(pixel_center(above), integer + 0.5);
            }
            assert_eq!(pixel_center(below.next_down()), integer - 0.5);
            assert_eq!(pixel_center(above.next_up()), integer + 0.5);
            for phase in [-0.25, 0.25, 0.5, 0.75] {
                let value = integer + phase;
                assert_eq!(pixel_center(value), value.floor() + 0.5);
            }
        }
    }

    #[test]
    fn near_integer_device_rows_and_columns_remain_stable_in_raster_output() {
        for vertical in [false, true] {
            let image = render(vertical, 100.0_f64.next_down(), 1.0, Fill::solid(ColorRef::rgb(Rgba::BLACK)));
            for normal in 99..=101 {
                let pixel = if vertical { image.pixel(normal, 200) } else { image.pixel(200, normal) };
                assert_eq!(pixel, if normal == 100 { [0, 0, 0, 255] } else { [255; 4] });
            }
        }
    }

    #[test]
    fn continuous_geometry_is_retained_outside_the_one_pixel_uniform_straight_case() {
        let line = Line::solid(ColorRef::rgb(Rgba::BLACK), 0.5);
        let horizontal = path(Point::ZERO, Point::new(100.0, 0.0));
        for width in [0.0, 0.499, 1.5, 2.0, f64::NAN, f64::INFINITY] {
            assert!(segment(&horizontal, Affine::IDENTITY, width, &line).is_none());
        }
        for width in [0.5, 1.0, 1.499] {
            assert!(segment(&horizontal, Affine::IDENTITY, width, &line).is_some());
        }
        for transform in [
            Affine::scale_non_uniform(2.0, 1.0),
            Affine::new([1.0, 0.0, 0.2, 1.0, 0.0, 0.0]),
            Affine::rotate(std::f64::consts::FRAC_PI_4),
            Affine::scale(0.0),
            Affine::scale(f64::INFINITY),
        ] {
            assert!(segment(&horizontal, transform, 0.5, &line).is_none());
        }
        let mut closed = horizontal.clone();
        closed.close_path();
        let mut bent = horizontal.clone();
        bent.line_to((100.0, 50.0));
        let mut curve = BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.quad_to((50.0, 20.0), (100.0, 0.0));
        for other in [closed, bent, curve, path(Point::ZERO, Point::ZERO), path(Point::ZERO, Point::new(100.0, 10.0))] {
            assert!(segment(&other, Affine::IDENTITY, 0.5, &line).is_none());
        }
    }

    #[test]
    fn caps_dashes_compounds_arrowheads_and_inside_alignment_keep_authored_strokes() {
        let path = path(Point::ZERO, Point::new(100.0, 0.0));
        for line in [
            Line { cap: Some(LineCap::Round), ..Default::default() },
            Line { cap: Some(LineCap::Square), ..Default::default() },
            Line { dash: Some(Dash::Preset { name: "dash".into() }), ..Default::default() },
            Line { compound: Some(Compound::Double), ..Default::default() },
            Line { head: Some(LineEnd { kind: "triangle".into(), w: "med".into(), len: "med".into() }), ..Default::default() },
            Line { tail: Some(LineEnd { kind: "oval".into(), w: "med".into(), len: "med".into() }), ..Default::default() },
            Line { pen_align: Some("in".into()), ..Default::default() },
        ] {
            assert!(segment(&path, Affine::IDENTITY, 0.5, &line).is_none());
        }
    }

    fn render(vertical: bool, position: f64, scale: f64, fill: Fill) -> crate::Image {
        let shape = Shape {
            id: ShapeId(2),
            xfrm: Some(Xfrm::new(position, position, 200.0, 200.0)),
            geom: Geom::Custom {
                paths: vec![CustomPath {
                    w: 200.0,
                    h: 200.0,
                    d: if vertical { "M 0 0 L 0 200" } else { "M 0 0 L 200 0" }.into(),
                    fill: FillMode::None,
                    stroke: true,
                }],
                source: None,
            },
            fill: Some(Fill::None),
            line: Some(Line { fill: Some(fill), width: Some(0.5), ..Default::default() }),
            ..Default::default()
        };
        crate::render_slide(&crate::fidelity_tests::slide_file_with(vec![shape]), 0, &crate::RenderOpts { scale, ..Default::default() })
    }

    #[test]
    fn opaque_thin_lines_have_one_full_pixel_of_coverage_at_multiple_scales_and_phases() {
        for scale in [1.0, 4.0 / 3.0, 2.0] {
            for position in [40.125, 40.5] {
                for vertical in [false, true] {
                    let image = render(vertical, position, scale, Fill::solid(ColorRef::rgb(Rgba::BLACK)));
                    let normal = (position * scale).floor() as u32;
                    let tangent = ((position + 100.0) * scale).floor() as u32;
                    for offset in [-1_i32, 0, 1] {
                        let normal = (normal as i32 + offset) as u32;
                        let pixel = if vertical { image.pixel(normal, tangent) } else { image.pixel(tangent, normal) };
                        assert_eq!(
                            pixel,
                            if offset == 0 { [0, 0, 0, 255] } else { [255; 4] },
                            "scale {scale}, phase {position}, vertical {vertical}"
                        );
                    }
                }
            }
        }
    }

    fn gradient(vertical: bool, end_alpha: u8) -> Fill {
        Fill::Gradient(Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::BLACK) },
                GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgba(255, 255, 255, end_alpha)) },
            ],
            shape: GradientShape::Linear { angle: if vertical { 90.0 } else { 0.0 }, scaled: false },
            tile_rect: None,
            rotate_with_shape: true,
        })
    }

    #[test]
    fn gradient_coverage_is_quantized_without_moving_its_authored_color_field() {
        let (position, scale) = (40.125, 4.0 / 3.0);
        for vertical in [false, true] {
            let image = render(vertical, position, scale, gradient(vertical, 255));
            let normal = (position * scale).floor() as u32;
            for fraction in [0.2, 0.5, 0.8] {
                let tangent = ((position + 200.0 * fraction) * scale).floor() as u32;
                let t = (tangent as f64 + 0.5 - position * scale) / (200.0 * scale);
                let expected = (255.0 * (1.0 - (1.0 - t).powf(1.875))).round() as u8;
                let pixel = if vertical { image.pixel(normal, tangent) } else { image.pixel(tangent, normal) };
                assert!(pixel[0].abs_diff(expected) <= 2, "paint coordinate at {fraction}: {pixel:?}, expected {expected}");
                let outside = if vertical { image.pixel(normal + 1, tangent) } else { image.pixel(tangent, normal + 1) };
                assert_eq!(outside, [255; 4]);
            }
        }
    }

    #[test]
    fn translucent_solid_and_gradient_strokes_keep_fractional_coverage() {
        let (position, scale): (f64, f64) = (40.125, 4.0 / 3.0);
        for vertical in [false, true] {
            let normal = (position * scale).floor() as u32;
            let tangent = ((position + 100.0) * scale).floor() as u32;
            for (fill, expected) in [(Fill::solid(ColorRef::rgb(Rgba::rgba(0, 0, 0, 128))), 85.3), (gradient(vertical, 128), 34.7)] {
                let image = render(vertical, position, scale, fill);
                let darkness: u32 = (normal - 1..=normal + 1)
                    .map(|normal| {
                        let pixel = if vertical { image.pixel(normal, tangent) } else { image.pixel(tangent, normal) };
                        u32::from(255 - pixel[0])
                    })
                    .sum();
                assert!((darkness as f64 - expected).abs() < 3.0, "continuous alpha coverage: {darkness} vs {expected}");
            }
        }
    }
}
