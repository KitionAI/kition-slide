//! Arrow decorations retain the shaft's paint coordinates and transparency.

use super::*;
use kition_slide_color::{ColorTransform, SchemeSlot};
use kition_slide_model::style::{ColorRef, Gradient, GradientShape, GradientStop, LineEnd};

fn ends(kind: &str) -> Line {
    let end = LineEnd { kind: kind.into(), w: "lg".into(), len: "lg".into() };
    Line { width: Some(8.0), head: Some(end.clone()), tail: Some(end), ..Default::default() }
}

fn gradient(angle: f64, alpha: i32) -> Fill {
    Fill::Gradient(Gradient {
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::scheme(SchemeSlot::Accent1).with(ColorTransform::Alpha(alpha)) },
            GradientStop { pos: 1.0, color: ColorRef::scheme(SchemeSlot::Accent2).with(ColorTransform::Alpha(alpha)) },
        ],
        shape: GradientShape::Linear { angle, scaled: false },
        tile_rect: None,
        rotate_with_shape: true,
    })
}

fn render_ends(line: &Line, transform: Affine) -> Image {
    let mut p = SlideFile::default();
    let master = Arc::make_mut(&mut p.masters[0]);
    master.theme.colors.set(SchemeSlot::Accent1, Rgba::rgb(255, 0, 0));
    master.theme.colors.set(SchemeSlot::Accent2, Rgba::rgb(0, 0, 255));
    let rctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let mut path = BezPath::new();
    path.move_to((40.0, 60.0));
    path.line_to((240.0, 60.0));
    let geo = Geometry {
        paths: vec![SubPath { path, fill: FillMode::None, stroke: true, even_odd: false }],
        text_rect: Rect::ZERO,
        handles: vec![],
        sites: vec![],
    };
    let mut ctx = RenderContext::new_with(300, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    arrowheads(&mut ctx, &rctx, line, None, &geo, transform, 8.0);
    Renderer::default().finish(ctx, 300, 120)
}

#[test]
fn gradient_arrowheads_use_opposite_shaft_end_colors_under_reversal_and_reflection() {
    for kind in ["triangle", "arrow"] {
        for angle in [0.0, 180.0] {
            for reflected in [false, true] {
                let line = Line { fill: Some(gradient(angle, 50000)), ..ends(kind) };
                let transform = if reflected { Affine::translate((280.0, 0.0)) * Affine::scale_non_uniform(-1.0, 1.0) } else { Affine::IDENTITY };
                let image = render_ends(&line, transform);
                let y = if kind == "arrow" { 68.0 } else { 70.0 };
                for (x, starts_red) in [(65.0, angle == 0.0), (215.0, angle != 0.0)] {
                    let point = transform * Point::new(x, y);
                    let pixel = image.pixel(point.x as u32, point.y as u32);
                    assert_eq!(pixel[3], 128, "authored alpha survives: {kind}, {pixel:?}");
                    assert_eq!(pixel[1], 0, "theme red/blue colors survive: {pixel:?}");
                    assert_eq!(pixel[0] > pixel[2], starts_red, "heads share the shaft frame: {kind}, {angle}, {pixel:?}");
                }
            }
        }
    }
}

#[test]
fn invisible_arrow_paints_stay_empty_and_solid_alpha_retains_legacy_behavior() {
    for kind in ["triangle", "arrow", "oval", "diamond", "stealth"] {
        for fill in [None, Some(Fill::None), Some(gradient(0.0, 0))] {
            let image = render_ends(&Line { fill, ..ends(kind) }, Affine::IDENTITY);
            assert!(image.pixels.iter().all(|byte| *byte == 0), "{kind} must not introduce black fallback pixels");
        }
    }
    for kind in ["triangle", "arrow"] {
        let line = Line { fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgba(20, 80, 160, 128)))), ..ends(kind) };
        let image = render_ends(&line, Affine::IDENTITY);
        let y = if kind == "arrow" { 68 } else { 70 };
        for x in [65, 215] {
            let pixel = image.pixel(x, y);
            assert_eq!(pixel[3], 128);
            for (actual, expected) in pixel[..3].iter().zip([20u8, 80, 160]) {
                assert!(actual.abs_diff(expected) <= 1, "solid paint is unchanged: {pixel:?}");
            }
        }
    }
}

#[test]
fn closed_shapes_keep_the_existing_arrow_decoration_gate() {
    let shape = Shape {
        id: ShapeId(20),
        xfrm: Some(kition_slide_geom::Xfrm::new(50.0, 50.0, 200.0, 80.0)),
        geom: Geom::preset("rect"),
        fill: Some(Fill::None),
        line: Some(Line { fill: Some(gradient(0.0, 50000)), ..ends("triangle") }),
        ..Default::default()
    };
    let mut p = fidelity_tests::slide_file_with(vec![shape]);
    let decorated = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    let line = Arc::make_mut(&mut p.slides[0]).shapes[0].line.as_mut().unwrap();
    line.head = None;
    line.tail = None;
    assert_eq!(decorated.pixels, render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() }).pixels);
}

fn render_sized_end(kind: &str, width: f64, transform: Affine) -> Image {
    let p = SlideFile::default();
    let rctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let line = Line {
        fill: Some(Fill::solid(ColorRef::rgb(Rgba::BLACK))),
        width: Some(width),
        tail: Some(LineEnd { kind: kind.into(), w: "med".into(), len: "med".into() }),
        ..Default::default()
    };
    let mut path = BezPath::new();
    path.move_to((20.0, 30.0));
    path.line_to((80.0, 30.0));
    let geo = Geometry {
        paths: vec![SubPath { path, fill: FillMode::None, stroke: true, even_odd: false }],
        text_rect: Rect::ZERO,
        handles: vec![],
        sites: vec![],
    };
    let mut ctx = RenderContext::new_with(240, 180, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    arrowheads(&mut ctx, &rctx, &line, None, &geo, transform, width);
    Renderer::default().finish(ctx, 240, 180)
}

#[test]
fn thin_filled_line_ends_retain_the_observed_minimum_size() {
    for kind in ["oval", "diamond", "triangle", "stealth"] {
        for transform in [
            Affine::IDENTITY,
            Affine::translate((180.0, 0.0)) * Affine::scale_non_uniform(-1.5, 2.0),
            Affine::translate((120.0, 10.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4),
        ] {
            let minimum = render_sized_end(kind, 2.0, transform);
            for width in [0.1, 0.5, 1.0, 1.5, 1.75] {
                assert_eq!(render_sized_end(kind, width, transform).pixels, minimum.pixels, "{kind}, {width}");
            }
        }
    }
}

#[test]
fn thick_line_ends_continue_growing_with_the_authored_width() {
    let extent = |image: &Image| {
        let mut bounds = (usize::MAX, usize::MAX, 0, 0);
        for (index, _) in image.pixels.as_chunks::<4>().0.iter().enumerate().filter(|(_, pixel)| pixel[3] != 0) {
            let (x, y) = (index % 240, index / 240);
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
        }
        (bounds.2 - bounds.0 + 1, bounds.3 - bounds.1 + 1)
    };
    for kind in ["oval", "diamond", "triangle", "stealth"] {
        let minimum = extent(&render_sized_end(kind, 2.0, Affine::scale(2.0)));
        let thick = extent(&render_sized_end(kind, 4.0, Affine::scale(2.0)));
        // Raster edge coverage can add one pixel at either end of an extent.
        assert!(thick.0.abs_diff(2 * minimum.0) <= 2, "{kind}: {minimum:?}, {thick:?}");
        assert!(thick.1.abs_diff(2 * minimum.1) <= 2, "{kind}: {minimum:?}, {thick:?}");
    }
}

#[test]
fn minimum_open_arrow_geometry_keeps_the_shaft_stroke_width() {
    let thin = render_sized_end("arrow", 0.5, Affine::scale(2.0));
    let wider = render_sized_end("arrow", 1.5, Affine::scale(2.0));
    let alpha_sum = |image: &Image| image.pixels.as_chunks::<4>().0.iter().map(|p| u64::from(p[3])).sum::<u64>();
    assert!(alpha_sum(&wider) > alpha_sum(&thin) * 3 / 2, "the two-point geometry floor must not thicken open arrow strokes");
}

#[test]
fn open_arrow_visible_tips_stay_at_the_authored_endpoint_under_transforms() {
    for transform in [
        Affine::IDENTITY,
        Affine::scale(2.0),
        Affine::translate((180.0, 0.0)) * Affine::scale_non_uniform(-1.5, 2.0),
        Affine::translate((120.0, 10.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4),
    ] {
        let endpoint = transform * Point::new(80.0, 30.0);
        let direction = (endpoint - transform * Point::new(20.0, 30.0)).normalize();
        for width in [0.75, 1.75, 3.0, 8.0] {
            let image = render_sized_end("arrow", width, transform);
            let furthest = image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, pixel)| pixel[3] > 32)
                .map(|(index, _)| (Point::new((index % 240) as f64 + 0.5, (index / 240) as f64 + 0.5) - endpoint).dot(direction))
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(furthest > -2.0 && furthest < 0.8, "visible tip must stay at its endpoint: {width}, {furthest}");
        }
    }
}
