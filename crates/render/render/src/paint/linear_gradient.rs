//! Linear gradient coordinates in the shape's local fill region.

use kurbo::{Point, Rect};

pub(super) fn endpoints(angle: f64, scaled: bool, bounds: Rect) -> (Point, Point) {
    let a = angle.to_radians();
    let (mut dx, mut dy) = (a.cos(), a.sin());
    let (w, h) = (bounds.width(), bounds.height());
    if scaled && w != h && angle % 90.0 != 0.0 && w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() {
        // A scaled gradient preserves constant-color lines under a nonuniform resize.
        // Their normal transforms by the inverse transpose: (cos(a)/w, sin(a)/h),
        // equivalently (h*cos(a), w*sin(a)). Divide by the larger dimension first
        // to avoid overflowing the products. Cardinal and square fills are unchanged.
        let extent = w.max(h);
        let (x, y) = (dx * (h / extent), dy * (w / extent));
        let length = x.hypot(y);
        if length.is_finite() && length > 0.0 {
            (dx, dy) = (x / length, y / length);
        }
    }
    let c = bounds.center();
    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
    (Point::new(c.x - dx * half, c.y - dy * half), Point::new(c.x + dx * half, c.y + dy * half))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(point: Point, endpoints: (Point, Point)) -> f64 {
        let (start, end) = endpoints;
        let direction = end - start;
        (point - start).dot(direction) / direction.hypot2()
    }

    #[test]
    fn scaled_gradient_preserves_colors_under_nonuniform_resize() {
        for angle in [30.0, 45.0, 135.0, 210.0, 315.0] {
            let square = Rect::new(0.0, 0.0, 100.0, 100.0);
            let original = endpoints(angle, false, square);
            for (w, h) in [(300.0, 80.0), (40.0, 250.0), (100.0, 100.0)] {
                let bounds = Rect::new(20.0, 30.0, 20.0 + w, 30.0 + h);
                let resized = endpoints(angle, true, bounds);
                for (x, y) in [(0.0, 0.0), (0.2, 0.7), (0.5, 0.5), (0.9, 0.3), (1.0, 1.0)] {
                    let before = position(Point::new(x * 100.0, y * 100.0), original);
                    let after = position(Point::new(20.0 + x * w, 30.0 + y * h), resized);
                    assert!((before - after).abs() < 1e-12, "angle {angle}, size {w} x {h}, point {x},{y}: {before} vs {after}");
                }
            }
        }
    }

    #[test]
    fn cardinal_and_square_gradients_keep_the_unscaled_endpoints() {
        for bounds in [Rect::new(20.0, 30.0, 320.0, 110.0), Rect::new(20.0, 30.0, 60.0, 280.0)] {
            for angle in [0.0, 90.0, 180.0, 270.0, 360.0] {
                assert_eq!(endpoints(angle, true, bounds), endpoints(angle, false, bounds));
            }
        }
        for angle in [0.0, 30.0, 45.0, 135.0, 210.0, 315.0] {
            let square = Rect::new(20.0, 30.0, 120.0, 130.0);
            assert_eq!(endpoints(angle, true, square), endpoints(angle, false, square));
        }
    }

    #[test]
    fn unscaled_diagonal_keeps_its_angle_and_extreme_scaled_dimensions_stay_finite() {
        let bounds = Rect::new(0.0, 0.0, 300.0, 100.0);
        let (start, end) = endpoints(45.0, false, bounds);
        let direction = end - start;
        assert!((direction.x - direction.y).abs() < 1e-12);
        let (start, end) = endpoints(45.0, true, bounds);
        let direction = end - start;
        assert!((direction.y / direction.x - 3.0).abs() < 1e-12);
        for (w, h) in [(1e300, 1e-200), (1e-200, 1e300), (1e300, 2e300)] {
            let (start, end) = endpoints(30.0, true, Rect::new(0.0, 0.0, w, h));
            assert!(start.is_finite() && end.is_finite());
        }
    }
}
