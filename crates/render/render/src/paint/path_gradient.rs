//! A shape gradient follows the outline, unlike a circle gradient's circumscribed circle.

use kition_slide_model::style::GradientShape;
use kurbo::{Affine, Arc, BezPath, PathEl, Point, Rect, Shape as _, Vec2};

/// Map a centered unit radial paint onto a closed elliptical outline. Other outlines and
/// non-point focus rectangles keep their existing rendering until their interpolation is supported.
pub(super) fn ellipse_transform(gradient: &GradientShape, path: &BezPath, bounds: Rect) -> Option<Affine> {
    let GradientShape::Path { path: kind, focus } = gradient else { return None };
    if kind != "shape" || *focus != [0.5; 4] || !bounds.is_finite() || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }
    let center = bounds.center();
    let radii = (bounds.width() / 2.0, bounds.height() / 2.0);
    let transform = Affine::translate(center.to_vec2()) * Affine::scale_non_uniform(radii.0, radii.1);
    if !transform.inverse().is_finite() || path.elements().len() > 256 {
        return None;
    }
    // Compare the actual outline, not a preset name. The same arc tolerance constructs the
    // engine's ellipse geometry. A bounded iterator also rejects extreme or compound paths.
    let ellipse = Arc::new(center, Vec2::new(radii.0, radii.1), 0.0, std::f64::consts::TAU, 0.0);
    let mut expected = ellipse.path_elements(0.05).chain(std::iter::once(PathEl::ClosePath)).take(257);
    let close = |a: Point, b: Point| ((a.x - b.x) / radii.0).abs() <= 1e-9 && ((a.y - b.y) / radii.1).abs() <= 1e-9;
    for element in path.elements() {
        let matches = match (element, expected.next()) {
            (PathEl::MoveTo(a), Some(PathEl::MoveTo(b))) => close(*a, b),
            (PathEl::CurveTo(a, b, c), Some(PathEl::CurveTo(d, e, f))) => close(*a, d) && close(*b, e) && close(*c, f),
            (PathEl::ClosePath, Some(PathEl::ClosePath)) => true,
            _ => false,
        };
        if !matches {
            return None;
        }
    }
    expected.next().is_none().then_some(transform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_color::Rgba;
    use kition_slide_geom::preset;
    use kition_slide_model::style::{ColorRef, Fill, Gradient, GradientStop};
    use kition_slide_model::{Geom, Line, Shape, ShapeId, Xfrm};

    fn gradient(kind: &str) -> GradientShape {
        GradientShape::Path { path: kind.into(), focus: [0.5; 4] }
    }

    #[test]
    fn recognizes_ellipses_without_reclassifying_other_outlines_or_focus_rectangles() {
        for (w, h) in [(240.0, 60.0), (40.0, 200.0), (100.0, 100.0)] {
            let bounds = Rect::new(0.0, 0.0, w, h);
            let ellipse = preset::build("ellipse", w, h, &[]).unwrap();
            let transform = ellipse_transform(&gradient("shape"), &ellipse.paths[0].path, bounds).unwrap();
            assert_eq!(transform * Point::ZERO, bounds.center());
            assert_eq!(transform * Point::new(1.0, 0.0), Point::new(w, h / 2.0));
            assert_eq!(transform * Point::new(0.0, 1.0), Point::new(w / 2.0, h));
            assert!(ellipse_transform(&gradient("circle"), &ellipse.paths[0].path, bounds).is_none());
            let area_focus = GradientShape::Path { path: "shape".into(), focus: [0.25; 4] };
            assert!(ellipse_transform(&area_focus, &ellipse.paths[0].path, bounds).is_none());
            for name in ["rect", "roundRect", "donut", "cloud"] {
                let other = preset::build(name, w, h, &[]).unwrap();
                assert!(ellipse_transform(&gradient("shape"), &other.paths[0].path, bounds).is_none(), "{name}");
            }
        }
        for bounds in [Rect::ZERO, Rect::new(0.0, 0.0, f64::INFINITY, 10.0)] {
            assert!(ellipse_transform(&gradient("shape"), &BezPath::new(), bounds).is_none());
        }
    }

    #[test]
    fn elliptical_shape_gradient_fades_at_both_axes_and_resets_paint_transform() {
        let render = |kind| {
            let shape = Shape {
                id: ShapeId(2),
                xfrm: Some(Xfrm::new(100.0, 100.0, 240.0, 80.0)),
                geom: Geom::Preset { name: "ellipse".into(), adj: vec![] },
                fill: Some(Fill::Gradient(Gradient {
                    stops: vec![
                        GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::BLACK) },
                        GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgba(0, 0, 0, 0)) },
                    ],
                    shape: gradient(kind),
                    tile_rect: None,
                    rotate_with_shape: true,
                })),
                line: Some(Line::none()),
                ..Default::default()
            };
            let control = Shape {
                id: ShapeId(3),
                xfrm: Some(Xfrm::new(400.0, 100.0, 40.0, 40.0)),
                fill: Some(Fill::Gradient(Gradient {
                    stops: vec![
                        GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::BLACK) },
                        GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::WHITE) },
                    ],
                    shape: GradientShape::Linear { angle: 0.0, scaled: false },
                    tile_rect: None,
                    rotate_with_shape: true,
                })),
                line: Some(Line::none()),
                ..Default::default()
            };
            let slide_file = crate::fidelity_tests::slide_file_with(vec![shape, control]);
            crate::render_slide(&slide_file, 0, &crate::RenderOpts { scale: 1.0, ..Default::default() })
        };
        let image = render("shape");
        let horizontal = image.pixel(310, 140)[0];
        let vertical = image.pixel(220, 170)[0];
        assert!(horizontal.abs_diff(vertical) < 6, "equal relative radii: {horizontal}, {vertical}");
        assert!(image.pixel(220, 101)[0] > 240, "the short-axis edge must fade out");
        assert!((180..=192).contains(&image.pixel(420, 120)[0]), "the following linear paint must keep its own coordinates");
        assert!(render("circle").pixel(220, 101)[0] < 100, "circle gradients retain their circumscribed-circle extent");
    }
}
