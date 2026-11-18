//! Hostile extents: shapes and effects far larger than the canvas never reach `vello_cpu`
//! unclamped (its tile coordinates are `u16`; the web build cannot catch a panic). Each test
//! renders through the renderer directly, without `render_slide`'s panic guard, so a panic fails.

use super::*;
use kition_slide_model::style::{ColorRef, Glow, Reflection, Shadow};
use kition_slide_model::{Effects, Shape, ShapeId, ShapeKind, ShapeStyle, SlideFile, Xfrm};

fn slide_file_with(shapes: Vec<Shape>) -> SlideFile {
    let mut p = SlideFile::default();
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = shapes;
    p
}

/// Render slide 1 without the panic guard.
fn render_unguarded(p: &SlideFile, scale: f64) -> Image {
    RENDERER.with(|r| r.borrow_mut().slide(p, &p.slides[0], 0, &RenderOpts { scale, ..Default::default() }))
}

fn shadow(blur: f64, dist: f64) -> Shadow {
    Shadow {
        color: ColorRef::rgb(Rgba::rgb(0, 0, 0)),
        blur,
        dist,
        dir: 45.0,
        inner: false,
        sx: 1.0,
        sy: 1.0,
        kx: 0.0,
        ky: 0.0,
        align: String::new(),
        rotate_with_shape: false,
    }
}

fn every_effect(r: f64) -> Effects {
    Effects {
        outer_shadow: Some(shadow(r, r)),
        inner_shadow: Some(Shadow { inner: true, ..shadow(r, r) }),
        glow: Some(Glow { color: ColorRef::rgb(Rgba::rgb(255, 0, 0)), radius: r }),
        soft_edge: Some(r),
        reflection: Some(Reflection { blur: r, start_alpha: 0.5, end_alpha: 0.0, end_pos: 0.5, dist: r, sy: -1.0, ..Default::default() }),
        ..Default::default()
    }
}

fn ellipse(id: u32, x: Xfrm, effects: Effects) -> Shape {
    Shape {
        id: ShapeId(id),
        xfrm: Some(x),
        geom: Geom::preset("ellipse"),
        style: Some(ShapeStyle::accent(kition_slide_color::SchemeSlot::Accent1)),
        effects: Some(effects),
        ..Default::default()
    }
}

/// A group whose child space is ~600 times smaller than its box (as some editors write it), holding
/// an ellipse with a 67 pt soft edge: the edge is absolute, never multiplied by the group's scale.
#[test]
fn soft_edge_in_a_strongly_scaled_group_renders() {
    let child = ellipse(11, Xfrm::new(0.43, 0.27, 0.3, 0.27), Effects { soft_edge: Some(67.0), ..Default::default() });
    let group = Shape {
        id: ShapeId(10),
        xfrm: Some(Xfrm::new(240.0, 130.0, 190.0, 170.0)),
        kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.43, 0.27, 0.3, 0.27) },
        ..Default::default()
    };
    let p = slide_file_with(vec![group]);
    let img = render_unguarded(&p, 1280.0 / 960.0);
    assert_eq!((img.width, img.height), (1280, 720));
    // The ellipse is drawn (faded towards its edge), not a blank fallback.
    let c = img.pixel(((240.0 + 95.0) * 1280.0 / 960.0) as u32, ((130.0 + 85.0) * 1280.0 / 960.0) as u32);
    assert_ne!(c, [255, 255, 255, 255]);
}

/// Every effect with an absurd radius, on a normal shape, a huge shape and a far-away shape.
#[test]
fn absurd_effect_extents_render() {
    let shapes = vec![
        ellipse(1, Xfrm::new(100.0, 100.0, 200.0, 100.0), every_effect(1.0e6)),
        ellipse(2, Xfrm::new(-1.0e6, -1.0e6, 2.0e6, 2.0e6), every_effect(50.0)),
        ellipse(3, Xfrm::new(5.0e5, 5.0e5, 100.0, 100.0), every_effect(80.0)),
        ellipse(4, Xfrm::new(400.0, 300.0, 1.0e-6, 1.0e-6), every_effect(30.0)),
    ];
    let p = slide_file_with(shapes);
    for scale in [0.1, 1.0, 4.0] {
        let img = render_unguarded(&p, scale);
        assert_eq!(img.width, (960.0 * scale) as u32);
    }
}

/// A shape scaled up by nested groups until it spans millions of pixels, with effects.
#[test]
fn effects_in_nested_magnifying_groups_render() {
    let mut s = ellipse(30, Xfrm::new(0.0, 0.0, 1.0, 1.0), every_effect(20.0));
    for i in 0..4 {
        s = Shape {
            id: ShapeId(40 + i),
            xfrm: Some(Xfrm::new(0.0, 0.0, 100.0, 100.0)),
            kind: ShapeKind::Group { children: vec![s], child: Xfrm::new(0.0, 0.0, 1.0, 1.0) },
            ..Default::default()
        };
    }
    let p = slide_file_with(vec![s]);
    let img = render_unguarded(&p, 2.0);
    assert_eq!(img.width, 1920);
}

/// `probe_slide` names the shapes whose effects are left out, and only those.
#[test]
fn probe_names_shapes_whose_effects_are_left_out() {
    let shapes = vec![
        ellipse(1, Xfrm::new(100.0, 100.0, 200.0, 100.0), every_effect(1.0e6)),
        ellipse(2, Xfrm::new(-1.0e6, -1.0e6, 2.0e6, 2.0e6), every_effect(50.0)),
        ellipse(5, Xfrm::new(300.0, 200.0, 100.0, 100.0), every_effect(10.0)),
    ];
    let p = slide_file_with(shapes);
    let probe = probe_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert!(!probe.panicked);
    assert_eq!(probe.effects_skipped, vec![ShapeId(1), ShapeId(2)]);
    // A later slide starts with a clean list.
    let probe = probe_slide(&slide_file_with(vec![ellipse(5, Xfrm::new(300.0, 200.0, 100.0, 100.0), every_effect(10.0))]), 0, &RenderOpts::default());
    assert_eq!(probe, RenderProbe::default());
    assert_eq!(probe_slide(&p, 9, &RenderOpts::default()), RenderProbe::default(), "no such slide");
}
