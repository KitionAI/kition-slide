//! Intrinsic circle containment for an axis-aligned inside-end label box.

/// The furthest nonnegative distance along `angle` at which the centered label
/// rectangle fits inside the circle. The caller subtracts its existing label gap
/// from `radius`; this function introduces no additional spacing.
///
/// For unit direction u and half-extents h, the farthest corner has squared
/// distance r² + 2rq + |h|², where q = |u.x|h.x + |u.y|h.y. Solve its circle
/// intersection after normalizing by radius. Rationalizing the positive root
/// avoids cancellation when the label nearly fills the circle.
pub(super) fn inside_end_radius(radius: f64, half_width: f64, half_height: f64, angle: f64) -> Option<f64> {
    if !radius.is_finite()
        || radius <= 0.0
        || !half_width.is_finite()
        || !half_height.is_finite()
        || !angle.is_finite()
        || half_width < 0.0
        || half_height < 0.0
        || half_width > radius
        || half_height > radius
    {
        return None;
    }
    let x = half_width / radius;
    let y = half_height / radius;
    let diagonal = x.hypot(y);
    if diagonal > 1.0 {
        return None;
    }
    let (sin, cos) = angle.sin_cos();
    let projection = x * cos.abs() + y * sin.abs();
    let remaining = (1.0 - diagonal) * (1.0 + diagonal);
    let denominator = projection.hypot(remaining.sqrt()) + projection;
    let distance = if denominator == 0.0 { 0.0 } else { radius * (remaining / denominator) };
    (distance.is_finite() && distance >= 0.0).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::inside_end_radius;

    fn farthest_corner(distance: f64, half_width: f64, half_height: f64, angle: f64) -> f64 {
        let (sin, cos) = angle.sin_cos();
        let (cx, cy) = (distance * cos, distance * sin);
        [-1.0, 1.0]
            .into_iter()
            .flat_map(|sx| [-1.0, 1.0].into_iter().map(move |sy| (cx + sx * half_width).hypot(cy + sy * half_height)))
            .fold(0.0, f64::max)
    }

    #[test]
    fn all_corners_fit_and_any_further_advance_leaves_the_circle() {
        for (radius, width, height) in [(100.0, 10.0, 4.0), (36.0, 2.0, 18.0), (5.0, 0.0, 1.0)] {
            for angle in [0.0_f64, 15.0, 45.0, 89.0, 90.0, 140.0, 180.0, 225.0, 270.0, 359.0, -37.0] {
                let angle = angle.to_radians();
                let distance = inside_end_radius(radius, width, height, angle).unwrap();
                let farthest = farthest_corner(distance, width, height, angle);
                assert!((farthest - radius).abs() < radius * 1e-12);
                assert!(farthest_corner(distance + radius * 1e-6, width, height, angle) > radius);
            }
        }
    }

    #[test]
    fn uniform_scale_and_quadrant_reflections_preserve_the_solution() {
        let angle = 0.37;
        let expected = inside_end_radius(1.0, 0.2, 0.1, angle).unwrap();
        for scale in [1e-200, 0.01, 1.0, 1e6, 1e200] {
            let actual = inside_end_radius(scale, 0.2 * scale, 0.1 * scale, angle).unwrap();
            assert!((actual / scale - expected).abs() < 1e-14);
        }
        for reflected in [-angle, std::f64::consts::PI - angle, std::f64::consts::PI + angle] {
            assert!((inside_end_radius(1.0, 0.2, 0.1, reflected).unwrap() - expected).abs() < 1e-14);
        }
    }

    #[test]
    fn zero_and_nearly_full_boxes_have_stable_limits() {
        assert_eq!(inside_end_radius(12.0, 0.0, 0.0, 0.5), Some(12.0));
        assert_eq!(inside_end_radius(5.0, 3.0, 4.0, 0.5), Some(0.0));
        let tiny_space = inside_end_radius(1.0, 1.0 - 1e-12, 0.0, 0.0).unwrap();
        assert!(tiny_space > 0.0);
        assert!((tiny_space - 1e-12).abs() < 1e-16);
        assert!(inside_end_radius(f64::MAX, f64::MAX / 4.0, f64::MAX / 4.0, 0.3).unwrap().is_finite());
    }

    #[test]
    fn invalid_or_oversized_inputs_request_the_existing_fallback() {
        for (radius, width, height, angle) in [
            (0.0, 0.0, 0.0, 0.0),
            (-1.0, 0.0, 0.0, 0.0),
            (1.0, -0.1, 0.1, 0.0),
            (1.0, 0.1, -0.1, 0.0),
            (1.0, 1.1, 0.0, 0.0),
            (1.0, 0.8, 0.8, 0.0),
            (f64::NAN, 0.1, 0.1, 0.0),
            (f64::INFINITY, 0.1, 0.1, 0.0),
            (1.0, f64::NAN, 0.1, 0.0),
            (1.0, 0.1, f64::INFINITY, 0.0),
            (1.0, 0.1, 0.1, f64::NAN),
            (1.0, 0.1, 0.1, f64::INFINITY),
        ] {
            assert!(inside_end_radius(radius, width, height, angle).is_none());
        }
    }
}
