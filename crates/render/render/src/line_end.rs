use kition_slide_model::style::LineEnd;

/// Full width and length of an endpoint decoration, in the shaft's local points.
/// Thin shafts retain their authored stroke width, but their decorations have a
/// two-point sizing basis. Shape and chart lines use the same size classes.
pub(crate) fn dimensions(end: &LineEnd, stroke_width: f64) -> (f64, f64) {
    let factor = |class: &str| match class {
        "sm" => 2.0,
        "lg" => 5.0,
        _ => 3.0,
    };
    let basis = stroke_width.max(2.0);
    (factor(&end.w) * basis, factor(&end.len) * basis)
}

/// Inset an open arrow's centerline apex so its stroked tip lands on the endpoint.
/// Acute joins beyond the stroke's miter limit use the bevel's smaller projection.
pub(crate) fn open_tip_inset(width: f64, length: f64, stroke_width: f64, miter_limit: f64) -> f64 {
    if ![width, length, stroke_width, miter_limit].iter().all(|value| value.is_finite() && *value > 0.0) {
        return 0.0;
    }
    let ratio = length.hypot(width / 2.0) / (width / 2.0);
    stroke_width / 2.0 * if ratio < miter_limit { ratio } else { 1.0 / ratio }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_apex_inset_matches_stroked_geometry_for_all_size_classes_and_bevel_fallback() {
        use kurbo::{BezPath, Shape, Stroke, StrokeOpts};
        for width_class in ["sm", "med", "lg"] {
            for length_class in ["sm", "med", "lg"] {
                let end = LineEnd { kind: "arrow".into(), w: width_class.into(), len: length_class.into() };
                for stroke_width in [0.75, 1.75, 3.0, 12.0] {
                    let (width, length) = dimensions(&end, stroke_width);
                    for miter_limit in [1.0, 4.0, 8.0] {
                        let stroke = Stroke::new(stroke_width).with_join(kurbo::Join::Miter).with_miter_limit(miter_limit);
                        let inset = open_tip_inset(width, length, stroke_width, miter_limit);
                        let mut path = BezPath::new();
                        path.move_to((-length - inset, width / 2.0));
                        path.line_to((-inset, 0.0));
                        path.line_to((-length - inset, -width / 2.0));
                        let outline = kurbo::stroke(path.iter(), &stroke, &StrokeOpts::default(), 0.001);
                        assert!(outline.bounding_box().x1.abs() < 1e-8, "{width_class}/{length_class}, {stroke_width}, {miter_limit}");
                    }
                }
            }
        }
    }

    #[test]
    fn endpoint_size_classes_are_independent_and_thin_sizes_do_not_collapse() {
        for (width_class, expected_width) in [("sm", 4.0), ("med", 6.0), ("lg", 10.0)] {
            for (length_class, expected_length) in [("sm", 4.0), ("med", 6.0), ("lg", 10.0)] {
                let end = LineEnd { kind: "triangle".into(), w: width_class.into(), len: length_class.into() };
                for width in [0.25, 0.5, 1.0, 1.75, 2.0] {
                    assert_eq!(dimensions(&end, width), (expected_width, expected_length));
                }
                assert_eq!(dimensions(&end, 4.0), (2.0 * expected_width, 2.0 * expected_length));
            }
        }
    }
}
