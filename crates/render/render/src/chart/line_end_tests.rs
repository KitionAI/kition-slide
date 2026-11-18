#![allow(clippy::expect_used)]

use super::{Env, Pal, Txt, stroke};
use kition_slide_color::Rgba;
use kition_slide_model::SlideFile;
use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorRef, Line, LineEnd};
use kurbo::{Affine, BezPath, Rect, Shape as _};
use vello_cpu::RenderContext;

fn line(with_ends: bool) -> Line {
    let end = || LineEnd { kind: "diamond".into(), w: "lg".into(), len: "lg".into() };
    Line { head: with_ends.then(end), tail: with_ends.then(end), ..Line::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)), 2.0) }
}

fn render(paths: &[BezPath], line: &Line) -> Vec<u8> {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).expect("default slide has a master");
    let chart = Chart::sample(ChartType::Column);
    let env = Env {
        rctx: &rctx,
        c: &chart,
        m: Affine::IDENTITY,
        pal: Pal { rctx: &rctx, scheme: None },
        base: Txt { font: None, font_ea: None, font_cs: None, size: 12.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None },
        size: (64.0, 64.0),
    };
    let mut context = RenderContext::new_with(64, 64, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    for path in paths {
        stroke(&mut context, &env, Some(line), Some(Rgba::BLACK), 2.0, path);
    }
    context.flush();
    let mut pixels = vec![0; 64 * 64 * 4];
    let pixmap = vello_cpu::PixmapMut::new(64, 64, &mut pixels).expect("pixel buffer has the requested size");
    context.render(pixmap, &mut vello_cpu::Resources::new());
    pixels
}

fn open_line(y: f64) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((30.0, y));
    path.line_to((55.0, y));
    path
}

fn alpha(pixels: &[u8], x: usize, y: usize) -> u8 {
    pixels[(y * 64 + x) * 4 + 3]
}

#[test]
fn closed_chart_outlines_ignore_authored_line_ends() {
    let rectangle = Rect::new(15.0, 15.0, 49.0, 49.0).to_path(0.1);
    let plain = render(std::slice::from_ref(&rectangle), &line(false));
    assert!(plain.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0), "the rectangle outline must remain visible");
    for kind in ["triangle", "stealth", "diamond", "oval", "arrow"] {
        let mut decorated = line(true);
        decorated.head.as_mut().expect("head exists").kind = kind.into();
        decorated.tail.as_mut().expect("tail exists").kind = kind.into();
        assert_eq!(render(std::slice::from_ref(&rectangle), &decorated), plain, "closed outline with {kind} ends");
    }
}

#[test]
fn open_chart_lines_keep_both_endpoint_decorations() {
    let path = open_line(32.0);
    let plain = render(std::slice::from_ref(&path), &line(false));
    let decorated = render(&[path], &line(true));
    for x in [30, 55] {
        assert_eq!(alpha(&plain, x, 28), 0);
        assert!(alpha(&decorated, x, 28) > 128, "arrow decoration at x={x}");
    }
}

#[test]
fn coincident_chart_endpoints_do_not_imply_a_closed_subpath() {
    let mut path = open_line(32.0);
    path.line_to((30.0, 32.0));
    let decorated = render(std::slice::from_ref(&path), &line(true));
    assert!(alpha(&decorated, 30, 28) > 128);

    path.close_path();
    let explicitly_closed = render(std::slice::from_ref(&path), &line(true));
    assert_eq!(explicitly_closed, render(&[path], &line(false)));
    assert_eq!(alpha(&explicitly_closed, 30, 28), 0);
}

#[test]
fn mixed_chart_subpaths_keep_only_the_open_line_decorations() {
    let rectangle = Rect::new(8.0, 8.0, 20.0, 20.0).to_path(0.1);
    let open = open_line(40.0);
    let expected = render(&[rectangle.clone(), open.clone()], &line(true));
    for paths in [[rectangle.clone(), open.clone()], [open.clone(), rectangle.clone()]] {
        let mut combined = BezPath::new();
        for path in paths {
            combined.extend(path.elements().iter().copied());
        }
        assert_eq!(render(&[combined], &line(true)), expected);
    }
    let plain_rectangle = render(&[rectangle], &line(false));
    for y in 0..25 {
        for x in 0..25 {
            assert_eq!(alpha(&expected, x, y), alpha(&plain_rectangle, x, y));
        }
    }
    assert!(alpha(&expected, 30, 36) > 128);
    assert!(alpha(&expected, 55, 36) > 128);
}

#[test]
fn disconnected_open_chart_runs_each_keep_their_own_ends() {
    let paths = [open_line(18.0), open_line(46.0)];
    let expected = render(&paths, &line(true));
    let mut combined = BezPath::new();
    for path in &paths {
        combined.extend(path.elements().iter().copied());
    }
    assert_eq!(render(&[combined], &line(true)), expected);
    for (x, y) in [(30, 14), (55, 14), (30, 42), (55, 42)] {
        assert!(alpha(&expected, x, y) > 128, "endpoint decoration at ({x}, {y})");
    }
}

fn render_decorations(paths: &[BezPath], line: &Line, transform: Affine) -> Vec<u8> {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).expect("default slide has a master");
    let chart = Chart::sample(ChartType::Column);
    let env = Env {
        rctx: &rctx,
        c: &chart,
        m: transform,
        pal: Pal { rctx: &rctx, scheme: None },
        base: Txt { font: None, font_ea: None, font_cs: None, size: 12.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None },
        size: (64.0, 64.0),
    };
    let mut context = RenderContext::new_with(64, 64, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    for path in paths {
        super::line_ends(&mut context, &env, line, Rgba::rgb(255, 0, 0), line.width.unwrap_or(2.0), path);
    }
    context.flush();
    let mut pixels = vec![0; 64 * 64 * 4];
    let pixmap = vello_cpu::PixmapMut::new(64, 64, &mut pixels).expect("pixel buffer has the requested size");
    context.render(pixmap, &mut vello_cpu::Resources::new());
    pixels
}

#[test]
fn thin_chart_filled_ends_keep_minimum_dimensions_under_transforms() {
    let transforms = [
        Affine::IDENTITY,
        Affine::translate((64.0, 0.0)) * Affine::scale_non_uniform(-1.0, 1.0),
        Affine::translate((64.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2),
        Affine::scale_non_uniform(0.75, 1.25),
    ];
    for kind in ["triangle", "stealth", "oval", "diamond"] {
        let mut line = line(true);
        for end in [&mut line.head, &mut line.tail].into_iter().flatten() {
            end.kind = kind.into();
            end.w = "med".into();
            end.len = "lg".into();
        }
        for transform in transforms {
            line.width = Some(2.0);
            let expected = render_decorations(&[open_line(32.0)], &line, transform);
            assert!(expected.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
            for width in [0.25, 0.5, 1.0, 1.75] {
                line.width = Some(width);
                assert_eq!(render_decorations(&[open_line(32.0)], &line, transform), expected, "{kind}, width={width}");
            }
        }
    }
}

#[test]
fn chart_open_arrow_strokes_keep_the_authored_width_below_the_size_floor() {
    let mut line = line(true);
    for end in [&mut line.head, &mut line.tail].into_iter().flatten() {
        end.kind = "arrow".into();
    }
    let area = |width| {
        let mut line = line.clone();
        line.width = Some(width);
        render_decorations(&[open_line(32.0)], &line, Affine::IDENTITY)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f64::from(pixel[3]) / 255.0)
            .sum::<f64>()
    };
    let thin = area(1.0);
    let thick = area(2.0);
    assert!(thick > thin * 1.8 && thick < thin * 2.2, "stroke area must scale while head dimensions stay fixed: {thin}, {thick}");
}

#[test]
fn axis_decorations_reverse_without_moving_or_rephasing_the_shaft() {
    use kition_slide_model::chart::Axis;
    let mut decorated = line(false);
    decorated.width = Some(4.0);
    decorated.dash = Some(kition_slide_model::style::Dash::Preset { name: "dash".into() });
    decorated.tail = Some(LineEnd { kind: "triangle".into(), w: "med".into(), len: "med".into() });
    let mut vertical = BezPath::new();
    vertical.move_to((32.0, 10.0));
    vertical.line_to((32.0, 54.0));
    let plain = render(std::slice::from_ref(&vertical), &Line { tail: None, ..decorated.clone() });
    for reversed in [false, true] {
        let axis = Axis { line: Some(decorated.clone()), reversed, ..Default::default() };
        let oriented = super::axis_line(&axis, false).expect("axis line exists");
        let pixels = render(std::slice::from_ref(&vertical), &oriented);
        let (arrow_y, empty_y) = if reversed { (44, 19) } else { (19, 44) };
        assert!(alpha(&pixels, 35, arrow_y) > 128);
        assert_eq!(alpha(&pixels, 35, empty_y), 0);
        for y in 25..38 {
            for x in 0..64 {
                assert_eq!(alpha(&pixels, x, y), alpha(&plain, x, y));
            }
        }
    }
}

#[test]
fn closed_chart_compound_bands_match_the_shared_geometry_and_keep_alpha_once() {
    use kition_slide_model::style::Compound;
    let narrow = Rect::new(20.0, 20.0, 44.0, 44.0).to_path(0.01);
    let thick = kurbo::Stroke::new(20.0).with_join(kurbo::Join::Round);
    assert!(
        crate::compound::closed_outline(&narrow, &thick, Some(Compound::ThinThick)).is_none(),
        "unclassifiable thick inner contour retains its fallback"
    );
    // Keep enough interior space for round-joined nested contours to retain
    // distinguishable inner/outer winding; a thick stroke around a tiny box falls back.
    for (compound, band_y, gap_y) in [(Compound::ThinThick, [4, 15], 7), (Compound::ThickThin, [7, 19], 15)] {
        let rectangle = Rect::new(12.0, 12.0, 52.0, 52.0).to_path(0.01);
        let line = Line { compound: Some(compound), ..Line::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)), 20.0) };
        let geometry = kurbo::Stroke::new(20.0).with_join(kurbo::Join::Round);
        assert!(
            crate::compound::closed_outline(&rectangle, &geometry, Some(compound)).is_some(),
            "exercise asymmetric bands, not the legacy fallback"
        );
        let image = render(std::slice::from_ref(&rectangle), &line);
        assert_eq!(image, render(&[rectangle.reverse_subpaths()], &line));
        for y in band_y {
            assert_eq!(alpha(&image, 32, y), 128, "{compound:?} at {y}");
        }
        assert_eq!(alpha(&image, 32, gap_y), 0);
        assert_eq!(alpha(&image, 32, 1), 0);
    }
}

// Paint the authored gradient independently of the stroke painter so the baseline's
// unrelated snapped-gradient fallback cannot become the expected result.
fn render_compound_gradient(path: &BezPath, line: &Line, transform: Affine, paint_only: bool) -> Vec<u8> {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).expect("default slide has a master");
    let chart = Chart::sample(ChartType::Column);
    let env = Env {
        rctx: &rctx,
        c: &chart,
        m: transform,
        pal: Pal { rctx: &rctx, scheme: None },
        base: Txt { font: None, font_ea: None, font_cs: None, size: 12.0, spacing: None, bold: false, italic: false, color: Rgba::BLACK, rot: None },
        size: (64.0, 64.0),
    };
    let mut context = RenderContext::new_with(64, 64, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    if paint_only {
        let width = line.width.expect("authored width");
        let bounds = path.bounding_box().inflate(width / 2.0, width / 2.0);
        super::fill_path(
            &mut context,
            &env,
            line.fill.as_ref().expect("authored fill"),
            &Rect::new(-64.0, -64.0, 128.0, 128.0).to_path(0.01),
            bounds,
        );
    } else {
        stroke(&mut context, &env, Some(line), Some(Rgba::BLACK), 2.0, path);
    }
    context.flush();
    let mut pixels = vec![0; 64 * 64 * 4];
    context.render(vello_cpu::PixmapMut::new(64, 64, &mut pixels).expect("pixel buffer size"), &mut vello_cpu::Resources::new());
    pixels
}

#[test]
fn snapped_asymmetric_chart_bands_keep_the_authored_gradient_frame() {
    use kition_slide_model::style::{Compound, Fill, Gradient, GradientShape, GradientStop};
    let rectangle = Rect::new(20.25, 20.25, 44.25, 44.25).to_path(0.01);
    let line = Line {
        width: Some(5.0),
        fill: Some(Fill::Gradient(Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)) },
                GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgba(0, 0, 255, 128)) },
            ],
            shape: GradientShape::Linear { angle: 0.0, scaled: false },
            tile_rect: None,
            rotate_with_shape: true,
        })),
        ..Default::default()
    };
    for transform in [Affine::IDENTITY, Affine::translate((2.0, 1.0)), Affine::new([1.2, 0.0, 0.0, 0.8, 2.0, 1.0])] {
        let reference = render_compound_gradient(&rectangle, &line, transform, true);
        let (snapped, width, _) = super::snap_lines(transform, &rectangle, 5.0).expect("axis-aligned rectangle snaps");
        assert_eq!(width, 5.0);
        let y0 = snapped.bounding_box().y0.floor() as usize;
        let x = (transform * kurbo::Point::new(32.0, 32.0)).x.floor() as usize;
        for (compound, bands, gap) in [(Compound::ThinThick, [y0 - 2, y0 + 1], y0 - 1), (Compound::ThickThin, [y0 - 1, y0 + 2], y0 + 1)] {
            let image = render_compound_gradient(&rectangle, &Line { compound: Some(compound), ..line.clone() }, transform, false);
            for y in bands {
                let offset = (y * 64 + x) * 4;
                assert!(
                    image[offset..offset + 4].iter().zip(&reference[offset..offset + 4]).all(|(a, b)| a.abs_diff(*b) <= 1),
                    "{compound:?} at {y}"
                );
                assert_eq!(alpha(&image, x, y), 128);
                assert!(image[offset] > 0 && image[offset + 2] > 0, "the gradient must not collapse to its first stop");
            }
            assert_eq!(alpha(&image, x, gap), 0);
        }
    }
}

#[test]
fn actual_circle_marker_paths_are_closed_and_support_asymmetric_bands() {
    use kition_slide_model::style::Compound;
    for center in [kurbo::Point::ZERO, kurbo::Point::new(32.25, 24.75), kurbo::Point::new(457.125, 203.625)] {
        for size in [4.9, 7.0, 10.0] {
            let (path, stroked) = super::marker_path("circle", center, size);
            assert!(!stroked);
            assert!(matches!(path.elements().last(), Some(kurbo::PathEl::ClosePath)));
            let width = size * 0.35;
            for compound in [Compound::ThinThick, Compound::ThickThin] {
                let outline = crate::compound::closed_outline(&path, &kurbo::Stroke::new(width), Some(compound)).expect("closed circle marker bands");
                let radius = size / 2.0;
                for (offset, painted) in [(-0.4, true), (-0.2, compound == Compound::ThinThick), (0.2, compound == Compound::ThickThin), (0.4, true)]
                {
                    let p = center + kurbo::Vec2::new(radius + offset * width, 0.0);
                    assert_eq!(outline.winding(p) != 0, painted, "{compound:?}, size {size}, offset {offset}");
                }
            }
        }
    }
}

#[test]
fn full_arc_input_remains_open_while_circle_markers_have_no_endpoint_decorations() {
    use kition_slide_model::style::Compound;
    let center = kurbo::Point::new(32.0, 32.0);
    let arc = kurbo::Ellipse::new(center, (10.0, 10.0), 0.0).to_path(0.1);
    assert!(crate::compound::closed_outline(&arc, &kurbo::Stroke::new(3.0), Some(Compound::ThinThick)).is_none());
    let (marker, _) = super::marker_path("circle", center, 20.0);
    for compound in [None, Some(Compound::Double), Some(Compound::ThinThick), Some(Compound::ThickThin)] {
        let plain = Line { compound, ..line(false) };
        let decorated = Line { compound, ..line(true) };
        assert_eq!(render(std::slice::from_ref(&marker), &plain), render(std::slice::from_ref(&marker), &decorated));
    }
}
