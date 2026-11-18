//! Brightness-biased interpolation for complete endpoint gradient ramps.

use vello_cpu::peniko::Color;

const SEGMENTS: usize = 64;

/// Complete two-color ramps and symmetric three-color ramps favor the brighter channel
/// endpoint. Other stop layouts retain ordinary interpolation. Subdivision bounds the
/// renderer's work to 129 stops while preserving every original stop exactly.
pub(super) fn expand(mut stops: Vec<(f32, Color)>) -> Vec<(f32, Color)> {
    // Repeating the same stop cannot change a gradient's paint. Keep coincident
    // stops with different colors: those encode intentional hard transitions.
    stops.dedup();
    let eligible = match stops.as_slice() {
        [(0.0, _), (1.0, _)] => true,
        [(0.0, first), (middle, _), (1.0, last)] => *middle > 0.0 && *middle < 1.0 && first == last,
        _ => false,
    };
    if !eligible {
        return stops;
    }
    // A constant RGB ramp only changes alpha, whose interpolation is already linear.
    if let Some((_, first)) = stops.first()
        && stops.iter().all(|(_, color)| color.components[..3] == first.components[..3])
    {
        return stops;
    }
    let mut expanded = Vec::with_capacity((stops.len() - 1) * SEGMENTS + 1);
    for pair in stops.windows(2) {
        let [left, right] = pair else { continue };
        expanded.push(*left);
        for step in 1..SEGMENTS {
            let t = step as f32 / SEGMENTS as f32;
            expanded.push((left.0 + (right.0 - left.0) * t, interpolate(left.1, right.1, t)));
        }
    }
    if let Some(last) = stops.last() {
        expanded.push(*last);
    }
    expanded
}

fn interpolate(left: Color, right: Color, t: f32) -> Color {
    Color::new(std::array::from_fn(|channel| {
        let a = left.components[channel];
        let b = right.components[channel];
        if channel == 3 {
            a + (b - a) * t
        } else {
            // Express the curve as a distance from the brighter endpoint. Reversing
            // the two colors therefore reverses the ramp without changing its profile.
            let distance = if a <= b { 1.0 - t } else { t };
            a.max(b) - (b - a).abs() * distance.powf(1.875)
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_color::Rgba;
    use kition_slide_model::style::{ColorRef, Fill, Gradient, GradientShape, GradientStop};
    use kition_slide_model::{Line, Shape, ShapeId, Xfrm};

    #[test]
    fn repeated_identical_stops_keep_the_same_interpolation() {
        let a = Color::new([0.5, 0.5, 0.5, 1.0]);
        let b = Color::new([0.1, 0.1, 0.1, 1.0]);
        let plain = vec![(0.0, a), (1.0, b)];
        let repeated = vec![(0.0, a), (0.0, a), (0.0, a), (1.0, b), (1.0, b)];
        assert_eq!(expand(plain), expand(repeated));
        let hard = vec![(0.0, a), (0.5, a), (0.5, b), (1.0, b)];
        assert_eq!(expand(hard.clone()), hard);
    }

    #[test]
    fn endpoint_ramp_treats_rising_falling_channels_and_alpha_independently() {
        let a = Color::new([0.0, 1.0, 0.4, 0.2]);
        let b = Color::new([1.0, 0.0, 0.4, 0.8]);
        let source = vec![(0.0, a), (1.0, b)];
        let ramp = expand(source.clone());
        assert_eq!(ramp.len(), 65);
        assert_eq!(ramp.first(), source.first());
        assert_eq!(ramp.last(), source.last());
        let middle = ramp[32].1.components;
        assert!((middle[0] - 0.72737306).abs() < 1e-6);
        assert!((middle[1] - 0.72737306).abs() < 1e-6);
        assert_eq!(middle[2], 0.4);
        assert!((middle[3] - 0.5).abs() < 1e-6);
        let reversed = expand(vec![(0.0, b), (1.0, a)]);
        for (forward, backward) in ramp.iter().zip(reversed.iter().rev()) {
            for (a, b) in forward.1.components.iter().zip(backward.1.components) {
                assert!((a - b).abs() < 1e-6);
            }
            assert!((forward.1.components[3] - (0.2 + 0.6 * forward.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn symmetric_three_color_ramp_preserves_the_original_middle_position_and_colors() {
        let outer = Color::new([0.1, 0.7, 0.9, 0.3]);
        let inner = Color::new([0.8, 0.2, 0.4, 0.9]);
        let ramp = expand(vec![(0.0, outer), (0.3, inner), (1.0, outer)]);
        assert_eq!(ramp.len(), 129);
        assert_eq!(ramp[0], (0.0, outer));
        assert_eq!(ramp[64], (0.3, inner));
        assert_eq!(ramp[128], (1.0, outer));
        for index in 0..=64 {
            for (a, b) in ramp[index].1.components.iter().zip(ramp[128 - index].1.components) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn other_stop_layouts_remain_exactly_unchanged() {
        let a = Color::new([0.0, 0.1, 0.2, 0.3]);
        let b = Color::new([1.0, 0.9, 0.8, 0.7]);
        let alpha_variant = Color::new([0.0, 0.1, 0.2, 0.4]);
        for stops in [
            vec![],
            vec![(0.0, a)],
            vec![(0.2, a), (1.0, b)],
            vec![(0.0, a), (0.99, b)],
            vec![(0.0, a), (0.0, b), (1.0, a)],
            vec![(0.0, a), (0.4, b), (1.0, alpha_variant)],
            vec![(0.0, a), (0.3, b), (0.7, b), (1.0, a)],
            vec![(0.0, a), (1.0, alpha_variant)],
        ] {
            assert_eq!(expand(stops.clone()), stops);
        }
    }

    #[test]
    fn subdivision_stays_below_one_channel_level_between_generated_stops() {
        let ramp = expand(vec![(0.0, Color::new([0.0, 1.0, 0.0, 1.0])), (1.0, Color::new([1.0, 0.0, 0.0, 1.0]))]);
        for pair in ramp.windows(2) {
            for fraction in [0.25, 0.5, 0.75] {
                let t = pair[0].0 + (pair[1].0 - pair[0].0) * fraction;
                let expected = interpolate(ramp[0].1, ramp[64].1, t);
                for channel in [0, 1] {
                    let approximation = pair[0].1.components[channel] * (1.0 - fraction) + pair[1].1.components[channel] * fraction;
                    assert!((approximation - expected.components[channel]).abs() * 255.0 < 1.0);
                }
            }
        }
    }

    #[test]
    fn endpoint_ramp_renders_brighter_channels_without_changing_alpha_or_four_stop_controls() {
        let render = |colors: &[(f64, Rgba)]| {
            let shape = Shape {
                id: ShapeId(2),
                xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 40.0)),
                fill: Some(Fill::Gradient(Gradient {
                    stops: colors.iter().map(|(pos, color)| GradientStop { pos: *pos, color: ColorRef::rgb(*color) }).collect(),
                    shape: GradientShape::Linear { angle: 0.0, scaled: false },
                    tile_rect: None,
                    rotate_with_shape: true,
                })),
                line: Some(Line::none()),
                ..Default::default()
            };
            let slide_file = crate::fidelity_tests::slide_file_with(vec![shape]);
            crate::render_slide(&slide_file, 0, &crate::RenderOpts { scale: 1.0, ..Default::default() })
        };
        let orange = Rgba::rgba(244, 150, 114, 255);
        let pixel = render(&[(0.0, orange), (1.0, Rgba::WHITE)]).pixel(200, 120);
        for (actual, expected) in pixel[..3].iter().zip([252_u8, 227, 217]) {
            assert!(actual.abs_diff(expected) <= 2, "midpoint {pixel:?}");
        }
        let crossed = render(&[(0.0, Rgba::rgba(0, 255, 0, 255)), (1.0, Rgba::rgba(255, 0, 0, 255))]).pixel(200, 120);
        assert!((183..=188).contains(&crossed[0]) && (183..=188).contains(&crossed[1]));
        let alpha = render(&[(0.0, Rgba::BLACK), (1.0, Rgba::rgba(0, 0, 0, 0))]).pixel(200, 120);
        assert!((126..=131).contains(&alpha[0]), "alpha must remain linear: {alpha:?}");
        let control = render(&[(0.0, Rgba::BLACK), (0.25, Rgba::BLACK), (0.75, Rgba::WHITE), (1.0, Rgba::WHITE)]).pixel(200, 120);
        assert!((126..=132).contains(&control[0]), "four-stop control: {control:?}");
    }
}
