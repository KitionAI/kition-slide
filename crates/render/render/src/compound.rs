//! Unequal compound bands on straight and closed drawing paths.

use kition_slide_model::style::Compound;
use kurbo::{Affine, BezPath, PathEl, Shape as _, Stroke, StrokeOpts, Vec2};

/// Keep the authored centerline, overall width and dash phase. The thin band and
/// gap each occupy one fifth of the width; the thick band occupies three fifths.
/// Curved and joined asymmetric paths still use the caller's existing fallback.
pub(crate) fn straight_outline(path: &BezPath, stroke: &Stroke, compound: Option<Compound>) -> Option<BezPath> {
    let direction = match compound? {
        Compound::ThinThick => 1.0,
        Compound::ThickThin => -1.0,
        _ => return None,
    };
    let [PathEl::MoveTo(start), PathEl::LineTo(end)] = path.elements() else {
        return None;
    };
    let delta = *end - *start;
    let length = delta.hypot();
    if !length.is_finite() || length <= 1e-12 || !stroke.width.is_finite() || stroke.width <= 0.0 {
        return None;
    }
    let normal = Vec2::new(-delta.y / length, delta.x / length) * (direction * stroke.width);
    let mut outline = BezPath::new();
    for (width, offset) in [(0.6, 0.2), (0.2, -0.4)] {
        let mut band = stroke.clone();
        band.width *= width;
        let centerline = Affine::translate(normal * offset) * path;
        outline.extend(kurbo::stroke(centerline.iter(), &band, &StrokeOpts::default(), 0.01));
    }
    Some(outline)
}

/// Closed outlines use exterior/interior bands independently of the source winding.
/// Thin-thick has an exterior thin band and an interior thick band. Keep all
/// boundaries in one nonzero fill so gaps stay transparent and alpha is applied once.
/// Open, dashed, concave, self-crossing, and unclassifiable paths retain the caller's fallback.
pub(crate) fn closed_outline(path: &BezPath, stroke: &Stroke, compound: Option<Compound>) -> Option<BezPath> {
    let exterior_gap = match compound? {
        Compound::ThinThick => true,
        Compound::ThickThin => false,
        _ => return None,
    };
    if !stroke.width.is_finite()
        || stroke.width <= 0.0
        || !stroke.dash_pattern.is_empty()
        || path.subpaths().count() != 1
        || !matches!(path.elements().last(), Some(PathEl::ClosePath))
        || !convex_control_polygon(path)
    {
        return None;
    }
    let (outer, inner) = closed_boundaries(path, stroke, 1.0)?;
    let (wide_outer, wide_inner) = closed_boundaries(path, stroke, 0.6)?;
    let (narrow_outer, narrow_inner) = closed_boundaries(path, stroke, 0.2)?;
    let mut outline = BezPath::new();
    append_boundary(&mut outline, outer, true);
    append_boundary(&mut outline, inner, false);
    if exterior_gap {
        append_boundary(&mut outline, wide_outer, false);
        append_boundary(&mut outline, narrow_outer, true);
    } else {
        append_boundary(&mut outline, narrow_inner, false);
        append_boundary(&mut outline, wide_inner, true);
    }
    Some(outline)
}

// A convex ordered control polygon is a conservative sufficient bound for these
// closed curves. Requiring every vertex to lie on the interior side of every edge
// excludes stars as well as concave shapes. Complex curves keep the old fallback.
fn convex_control_polygon(path: &BezPath) -> bool {
    let mut points = Vec::new();
    for element in path.elements() {
        let candidates: &[kurbo::Point] = match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
            PathEl::QuadTo(a, b) => &[*a, *b],
            PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            PathEl::ClosePath => &[],
        };
        for point in candidates {
            if !point.x.is_finite() || !point.y.is_finite() || points.len() >= 128 {
                return false;
            }
            if points.last() != Some(point) {
                points.push(*point);
            }
        }
    }
    if points.first() == points.last() {
        points.pop();
    }
    if points.len() < 3 {
        return false;
    }
    for (i, point) in points.iter().enumerate() {
        if points[..i].contains(point) {
            return false;
        }
    }
    let origin = points[0];
    let area = points.iter().zip(points.iter().cycle().skip(1)).map(|(a, b)| (*a - origin).cross(*b - origin)).sum::<f64>();
    if !area.is_finite() || area.abs() <= f64::EPSILON {
        return false;
    }
    let extent = points.iter().map(|p| (*p - origin).hypot()).fold(0.0_f64, f64::max);
    let tolerance = f64::EPSILON * extent * extent * 64.0;
    if !tolerance.is_finite() {
        return false;
    }
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        for point in &points {
            let side = (*b - *a).cross(*point - *a) * area.signum();
            if !side.is_finite() || side < -tolerance {
                return false;
            }
        }
    }
    true
}

fn closed_boundaries(path: &BezPath, stroke: &Stroke, fraction: f64) -> Option<(BezPath, BezPath)> {
    let mut band = stroke.clone();
    band.width *= fraction;
    let outline = kurbo::stroke(path.iter(), &band, &StrokeOpts::default(), 0.01);
    let mut contours = outline.subpaths().map(|part| BezPath::from_vec(part.to_vec()));
    let mut outer = contours.next()?;
    let mut inner = contours.next()?;
    if contours.next().is_some() {
        return None;
    }
    let (a, b) = (outer.area(), inner.area());
    if !a.is_finite() || !b.is_finite() || a * b >= 0.0 || a.abs() == b.abs() {
        return None;
    }
    if a.abs() < b.abs() {
        std::mem::swap(&mut outer, &mut inner);
    }
    Some((outer, inner))
}

fn append_boundary(outline: &mut BezPath, boundary: BezPath, positive: bool) {
    outline.extend(if (boundary.area() > 0.0) == positive { boundary } else { boundary.reverse_subpaths() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Point, Rect};

    #[test]
    fn closed_bands_preserve_the_extent_and_do_not_exchange_sides_when_reversed() {
        for path in [Circle::new((0.0, 0.0), 20.0).to_path(0.01), Rect::new(-20.0, -20.0, 20.0, 20.0).to_path(0.01)] {
            for compound in [Compound::ThinThick, Compound::ThickThin] {
                for source in [path.clone(), path.reverse_subpaths(), Affine::scale_non_uniform(-1.0, 1.0) * &path] {
                    let outline = closed_outline(&source, &Stroke::new(10.0), Some(compound)).unwrap();
                    assert!((outline.bounding_box().width() - 50.0).abs() < 0.02);
                    assert!((outline.bounding_box().height() - 50.0).abs() < 0.02);
                    for (radius, painted) in [
                        (0.0, false),
                        (14.0, false),
                        (16.0, true),
                        (18.0, compound == Compound::ThinThick),
                        (20.0, true),
                        (22.0, compound == Compound::ThickThin),
                        (24.0, true),
                        (26.0, false),
                    ] {
                        assert_eq!(outline.winding(Point::new(radius, 0.0)) != 0, painted, "{compound:?} at {radius}");
                    }
                }
            }
        }
    }

    #[test]
    fn closed_bands_leave_unsupported_paths_to_the_existing_painter() {
        let rectangle = Rect::new(10.0, 10.0, 50.0, 50.0).to_path(0.01);
        let dashed = Stroke::new(10.0).with_dashes(0.0, [5.0, 5.0]);
        assert!(closed_outline(&rectangle, &dashed, Some(Compound::ThinThick)).is_none());
        let mut multi = rectangle.clone();
        multi.extend(rectangle);
        assert!(closed_outline(&multi, &Stroke::new(10.0), Some(Compound::ThinThick)).is_none());
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((20.0, 20.0));
        assert!(closed_outline(&open, &Stroke::new(10.0), Some(Compound::ThinThick)).is_none());
    }

    #[test]
    fn closed_bands_reject_concave_crossing_and_retraced_contours() {
        let mut polygons = vec![
            vec![(0.0, 0.0), (40.0, 0.0), (20.0, 20.0), (40.0, 40.0), (0.0, 40.0)],
            vec![(0.0, 0.0), (60.0, 40.0), (0.0, 40.0), (20.0, 0.0)],
            vec![(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0), (0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)],
        ];
        polygons.push(
            (0..5)
                .map(|i| {
                    let angle = f64::from((i * 2) % 5) * std::f64::consts::TAU / 5.0;
                    (40.0 * angle.cos(), 40.0 * angle.sin())
                })
                .collect(),
        );
        polygons.push(
            (0..129)
                .map(|i| {
                    let angle = f64::from(i) * std::f64::consts::TAU / 129.0;
                    (40.0 * angle.cos(), 40.0 * angle.sin())
                })
                .collect(),
        );
        for points in polygons {
            let mut path = BezPath::new();
            path.move_to(points[0]);
            for point in points.into_iter().skip(1) {
                path.line_to(point);
            }
            path.close_path();
            for source in [path.clone(), path.reverse_subpaths()] {
                for compound in [Compound::ThinThick, Compound::ThickThin] {
                    assert!(closed_outline(&source, &Stroke::new(4.0), Some(compound)).is_none());
                }
            }
        }
        let invalid = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(f64::NAN, 0.0)),
            PathEl::LineTo(Point::new(20.0, 0.0)),
            PathEl::LineTo(Point::new(20.0, 20.0)),
            PathEl::ClosePath,
        ]);
        assert!(closed_outline(&invalid, &Stroke::new(4.0), Some(Compound::ThinThick)).is_none());
    }
}
