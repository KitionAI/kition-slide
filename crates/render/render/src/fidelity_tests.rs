//! Rendering as PowerPoint draws it: group scaling, backgrounds and inherited graphics, pictures,
//! fills. Synthetic slides only.

use super::*;
use kition_slide_model::style::ColorRef;
use kition_slide_model::{Line, Shape, ShapeId, ShapeKind, SlideFile, Xfrm};

pub(crate) fn slide_file_with(shapes: Vec<Shape>) -> SlideFile {
    let mut p = SlideFile::default();
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = shapes;
    p
}

#[test]
fn compound_outlines_keep_transparent_gaps_over_the_shape_fill() {
    use kition_slide_model::style::Compound;
    for (compound, width, bands, gaps) in
        [(Compound::Double, 12.0, vec![35, 44], vec![39, 40]), (Compound::Triple, 24.0, vec![29, 37, 42, 50], vec![34, 45])]
    {
        let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 200.0, 100.0));
        shape.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0))));
        shape.line = Some(Line { compound: Some(compound), ..Line::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)), width) });
        let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
        for y in bands {
            let pixel = image.pixel(140, y);
            assert!(pixel[0] >= 127, "{compound:?} band at {y}: {pixel:?}");
            if y >= 40 {
                assert!((60..=68).contains(&pixel[1]), "paint alpha must be applied once: {pixel:?}");
            }
        }
        for y in gaps {
            let underlay = if y < 40 { [255; 4] } else { [0, 128, 0, 255] };
            assert_eq!(image.pixel(140, y), underlay, "{compound:?} gap at {y}");
        }
    }
}

#[test]
fn asymmetric_straight_outlines_keep_the_authored_width_and_transparent_gap() {
    use kition_slide_model::style::Compound;
    for (compound, thick, gap, thin) in [(Compound::ThinThick, 44, 36, 32), (Compound::ThickThin, 35, 43, 47)] {
        let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 200.0, 1.0));
        shape.geom = Geom::Custom {
            paths: vec![kition_slide_model::CustomPath { w: 200.0, h: 1.0, d: "M 0 0 L 200 0".into(), fill: FillMode::None, stroke: true }],
            source: None,
        };
        shape.line = Some(Line { compound: Some(compound), ..Line::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)), 20.0) });
        let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
        for y in [thick, thin] {
            let pixel = image.pixel(140, y);
            assert_eq!(pixel[0], 255);
            assert!((125..=129).contains(&pixel[1]), "{compound:?} band at {y}: {pixel:?}");
        }
        for y in [29, gap, 50] {
            assert_eq!(image.pixel(140, y), [255; 4], "{compound:?} gap or outside at {y}");
        }
    }
}

#[test]
fn compound_gradient_outlines_use_one_gradient_across_both_bands() {
    use kition_slide_model::style::{Compound, Gradient, GradientShape, GradientStop};
    let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 200.0, 100.0));
    shape.line = Some(Line {
        width: Some(12.0),
        compound: Some(Compound::Double),
        fill: Some(Fill::Gradient(Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(255, 0, 0)) },
                GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0, 0, 255)) },
            ],
            shape: GradientShape::Linear { angle: 0.0, scaled: false },
            tile_rect: None,
            rotate_with_shape: true,
        })),
        ..Default::default()
    });
    let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    for y in [35, 44] {
        assert!(image.pixel(50, y)[0] > 220);
        assert!(image.pixel(230, y)[2] > 220);
    }
    assert_eq!(image.pixel(140, 40), [255; 4]);
}

#[test]
fn compound_outline_bands_share_the_same_dash_positions() {
    use kition_slide_model::style::{Compound, Dash};
    let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 200.0, 100.0));
    shape.line = Some(Line {
        compound: Some(Compound::Double),
        dash: Some(Dash::Preset { name: "dash".into() }),
        ..Line::solid(ColorRef::rgb(Rgba::BLACK), 12.0)
    });
    let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    for y in [35, 44] {
        assert_eq!(image.pixel(60, y), [0, 0, 0, 255]);
        assert_eq!(image.pixel(100, y), [255; 4]);
        assert_eq!(image.pixel(140, y), [0, 0, 0, 255]);
    }
    assert_eq!(image.pixel(60, 40), [255; 4]);
}

#[test]
fn crossing_compound_subpaths_paint_their_shared_bands_once() {
    for path in ["M 0 50 L 100 50 M 50 0 L 50 100", "M 100 50 L 0 50 M 50 100 L 50 0"] {
        let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 100.0, 100.0));
        shape.geom = Geom::Custom {
            paths: vec![kition_slide_model::CustomPath { w: 100.0, h: 100.0, d: path.into(), fill: FillMode::None, stroke: true }],
            source: None,
        };
        shape.line = Some(Line {
            compound: Some(kition_slide_model::style::Compound::Double),
            ..Line::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)), 12.0)
        });
        let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
        for (x, y) in [(94, 94), (90, 94), (94, 90)] {
            let pixel = image.pixel(x, y);
            assert_eq!(pixel[0], 255);
            assert!((125..=129).contains(&pixel[1]), "crossing band at ({x}, {y}): {pixel:?}");
        }
        assert_eq!(image.pixel(90, 90), [255; 4]);
    }
}

#[test]
fn gradient_outline_keeps_both_endpoint_colors_and_the_unpainted_interior() {
    use kition_slide_model::style::{Gradient, GradientShape, GradientStop};
    let mut shape = Shape {
        id: ShapeId(2),
        xfrm: Some(Xfrm::new(40.0, 40.0, 200.0, 100.0)),
        fill: Some(Fill::None),
        line: Some(Line {
            width: Some(6.0),
            fill: Some(Fill::Gradient(Gradient {
                stops: vec![
                    GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(255, 0, 0)) },
                    GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0, 0, 255)) },
                ],
                shape: GradientShape::Linear { angle: 0.0, scaled: false },
                tile_rect: None,
                rotate_with_shape: true,
            })),
            ..Default::default()
        }),
        ..Default::default()
    };
    let image = render_slide(&slide_file_with(vec![shape.clone()]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    let left = image.pixel(50, 40);
    let right = image.pixel(230, 40);
    assert!(left[0] > 220 && left[2] < 35);
    assert!(right[2] > 220 && right[0] < 35);
    assert_eq!(image.pixel(140, 90), [255; 4]);

    if let Some(Fill::Gradient(g)) = shape.line.as_mut().and_then(|line| line.fill.as_mut()) {
        g.stops[0].color = ColorRef::rgb(Rgba::rgba(255, 0, 0, 128));
        g.stops[1].color = ColorRef::rgb(Rgba::rgba(0, 0, 255, 128));
    }
    let underlay = Shape {
        id: ShapeId(3),
        xfrm: Some(Xfrm::new(20.0, 20.0, 240.0, 140.0)),
        fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0)))),
        line: Some(Line::none()),
        ..Default::default()
    };
    let image = render_slide(&slide_file_with(vec![underlay, shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    let left = image.pixel(50, 40);
    assert!(left[0] > 110 && left[0] < 135 && left[2] < 25);
    assert!((60..=68).contains(&left[1]), "outline alpha must be applied exactly once: {left:?}");
    assert_eq!(image.pixel(140, 90), [0, 128, 0, 255]);
}

#[test]
fn thin_rectilinear_outline_has_crisp_edges_at_fractional_positions() {
    let mut shape = outlined(2, Xfrm::new(40.3, 40.3, 100.3, 60.3));
    shape.line.as_mut().unwrap().width = Some(1.0);
    let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert_eq!(image.pixel(39, 60), [255; 4]);
    assert_eq!(image.pixel(40, 60), [0, 0, 0, 255]);
    assert_eq!(image.pixel(41, 60), [255; 4]);
    assert_eq!(image.pixel(140, 60), [0, 0, 0, 255]);
    assert_eq!(image.pixel(141, 60), [255; 4]);
}

#[test]
fn translucent_rectilinear_outline_retains_fractional_coverage() {
    let mut shape = outlined(2, Xfrm::new(40.3, 40.3, 100.3, 60.3));
    shape.line = Some(Line::solid(ColorRef::rgb(Rgba::rgba(0, 0, 0, 128)), 1.0));
    let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
    // A 1px stroke centered at 40.3 covers 20% of pixel 39 and 80% of pixel 40.
    let outer = image.pixel(39, 60)[0];
    let inner = image.pixel(40, 60)[0];
    assert!((225..=235).contains(&outer), "fractional outer coverage: {outer}");
    assert!((148..=158).contains(&inner), "fractional inner coverage: {inner}");
    assert_eq!(image.pixel(41, 60), [255; 4]);
}

#[test]
fn pixel_alignment_rejects_equal_length_sheared_axes() {
    let transform = Affine::new([1.0, 0.0, 0.6, 0.8, 0.0, 0.0]);
    let path = transform.inverse() * Rect::new(40.3, 40.3, 140.6, 100.6).to_path(0.1);
    let line = Line::solid(ColorRef::rgb(Rgba::BLACK), 1.0);
    assert!(pixel_aligned_outline(&path, transform, 1.0, &line).is_none());
}

pub(crate) fn dark_pixels(img: &Image, r: (u32, u32, u32, u32)) -> usize {
    let mut n = 0;
    for y in r.1..r.3.min(img.height) {
        for x in r.0..r.2.min(img.width) {
            if img.pixel(x, y)[0] < 128 {
                n += 1;
            }
        }
    }
    n
}

/// A rectangle with a black 2 pt outline and no fill.
fn outlined(id: u32, x: Xfrm) -> Shape {
    Shape {
        id: ShapeId(id),
        xfrm: Some(x),
        fill: Some(Fill::None),
        line: Some(Line { width: Some(2.0), fill: Some(Fill::solid(ColorRef::rgb(Rgba::BLACK))), ..Default::default() }),
        ..Default::default()
    }
}

/// A group whose child space is 100 times smaller than its box: the child is drawn at the size it
/// shows with its own 2 pt outline, not a 200 pt one covering the slide.
#[test]
fn scaled_group_children_keep_their_line_width() {
    let child = outlined(2, Xfrm::new(0.0, 0.0, 2.0, 1.0));
    let group = Shape {
        id: ShapeId(1),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 100.0)),
        kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.0, 0.0, 2.0, 1.0) },
        ..Default::default()
    };
    let p = slide_file_with(vec![group]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    // The outline runs along the group's box…
    assert!(dark_pixels(&img, (100, 99, 300, 102)) > 300);
    // …and is only about 2 px wide: the inside and the outside stay white.
    assert_eq!(dark_pixels(&img, (110, 110, 290, 190)), 0);
    assert_eq!(dark_pixels(&img, (0, 0, 90, 90)), 0);
}

/// An unscaled group with a child-space offset draws its children where they were.
#[test]
fn unscaled_group_offsets_children() {
    let child = outlined(2, Xfrm::new(1000.0, 1000.0, 200.0, 100.0));
    let group = Shape {
        id: ShapeId(1),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 100.0)),
        kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(1000.0, 1000.0, 200.0, 100.0) },
        ..Default::default()
    };
    let p = slide_file_with(vec![group]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert!(dark_pixels(&img, (100, 99, 300, 102)) > 300);
    assert_eq!(dark_pixels(&img, (110, 110, 290, 190)), 0);
}

fn png_bytes(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap_or_default();
    out
}

fn set_background(p: &mut SlideFile, fill: Fill) {
    std::sync::Arc::make_mut(&mut p.slides[0]).background = Some(kition_slide_model::Background::Fill { fill });
}

/// A tiled picture without a stated resolution repeats at one point per pixel, from the top left.
#[test]
fn tiles_repeat_at_the_pictures_natural_size() {
    let tile = image::RgbaImage::from_fn(8, 8, |x, _| if x < 4 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
    let mut p = SlideFile::default();
    let media = p.add_media("tile.png", "image/png", png_bytes(&tile));
    let mode = kition_slide_model::style::PictureMode::Tile { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, flip: "none".into(), align: "tl".into() };
    set_background(&mut p, Fill::Picture(kition_slide_model::style::PictureFill { media, mode, ..Default::default() }));
    for scale in [1.0, 2.0] {
        let img = render_slide(&p, 0, &RenderOpts { scale, ..Default::default() });
        let red = |x: f64| {
            let px = img.pixel((x * scale) as u32, 20);
            px[0] > 200 && px[2] < 60
        };
        assert!(red(1.0) && !red(6.0) && red(9.0) && !red(14.0) && red(801.0), "scale {scale}: {:?}", img.pixel((6.0 * scale) as u32, 20));
    }
}

/// Pattern fills draw their 8x8 cell, one bit per pixel at 96 dpi, pinned to the output's pixels.
#[test]
fn patterns_draw_their_cells_on_pixels() {
    let mut p = SlideFile::default();
    let pattern = kition_slide_model::style::PatternFill { preset: "pct50".into(), fg: ColorRef::rgb(Rgba::BLACK), bg: ColorRef::rgb(Rgba::WHITE) };
    set_background(&mut p, Fill::Pattern(pattern));
    let img = render_slide(&p, 0, &RenderOpts { scale: 4.0 / 3.0, ..Default::default() });
    for (x, y) in [(0, 0), (1, 1), (100, 50), (641, 359)] {
        assert_eq!(img.pixel(x, y), [0, 0, 0, 255], "({x}, {y})");
        assert_eq!(img.pixel(x + 1, y), [255, 255, 255, 255], "({x}, {y})");
    }
    // A cell measured from the reference renders.
    assert_eq!(paint::pattern_cell_for_tests("pct5"), [0, 0, 0, 0x10, 0, 0, 0, 0x01]);
}

/// A shape without a fill casts its shadow from its outline, not from its whole area.
#[test]
fn unfilled_shapes_cast_outline_shadows() {
    let mut sh = outlined(1, Xfrm::new(100.0, 100.0, 200.0, 100.0));
    let shadow = kition_slide_model::style::Shadow {
        color: ColorRef::rgb(Rgba::rgb(255, 0, 0)),
        blur: 2.0,
        dist: 10.0,
        dir: 0.0,
        inner: false,
        sx: 1.0,
        sy: 1.0,
        kx: 0.0,
        ky: 0.0,
        align: String::new(),
        rotate_with_shape: false,
    };
    sh.effects = Some(kition_slide_model::Effects { outer_shadow: Some(shadow), ..Default::default() });
    let p = slide_file_with(vec![sh]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    // Inside the box: white, no shadow fill.
    assert_eq!(img.pixel(200, 150), [255, 255, 255, 255]);
    // The outline's shadow, 10 pt to the right of the left edge.
    let px = img.pixel(110, 150);
    assert!(px[0] > 200 && px[1] < 200, "{px:?}");
}

fn filled(id: u32, x: Xfrm, fill: Fill) -> Shape {
    Shape { id: ShapeId(id), xfrm: Some(x), fill: Some(fill), line: Some(Line::none()), ..Default::default() }
}

/// A child filled with its group's fill (`grpFill`) takes the group's fill.
#[test]
fn group_fill_reaches_children() {
    let red = Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)));
    let child = filled(2, Xfrm::new(0.0, 0.0, 100.0, 100.0), Fill::Group);
    let group = Shape {
        id: ShapeId(1),
        xfrm: Some(Xfrm::new(100.0, 100.0, 100.0, 100.0)),
        fill: Some(red),
        kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.0, 0.0, 100.0, 100.0) },
        ..Default::default()
    };
    let p = slide_file_with(vec![group]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert_eq!(img.pixel(150, 150), [255, 0, 0, 255]);
}

/// A shape filled with the background (`useBgFill`) shows the slide's background over what is
/// below it.
#[test]
fn background_filled_shapes_show_the_slide_background() {
    let black = filled(1, Xfrm::new(0.0, 0.0, 400.0, 300.0), Fill::solid(ColorRef::rgb(Rgba::BLACK)));
    let cover = filled(2, Xfrm::new(100.0, 100.0, 100.0, 100.0), Fill::Background);
    let mut p = slide_file_with(vec![black, cover]);
    set_background(&mut p, Fill::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255))));
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert_eq!(img.pixel(150, 150), [0, 0, 255, 255]);
    assert_eq!(img.pixel(50, 50), [0, 0, 0, 255]);
}

/// A slide placeholder that names no geometry draws its layout placeholder's.
#[test]
fn placeholders_inherit_their_layouts_geometry() {
    let mut p = SlideFile::default();
    let ph = kition_slide_model::Placeholder { kind: kition_slide_model::PhType::Body, idx: 17, ..Default::default() };
    let black = Fill::solid(ColorRef::rgb(Rgba::BLACK));
    let lay = Shape {
        id: ShapeId(50),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 200.0)),
        geom: Geom::preset("ellipse"),
        fill: Some(black),
        ph: Some(ph.clone()),
        ..Default::default()
    };
    let layout_id = p.slides[0].layout;
    for m in &mut p.masters {
        for l in &mut std::sync::Arc::make_mut(m).layouts {
            if l.id == layout_id {
                l.shapes.push(lay.clone());
            }
        }
    }
    let slide_ph = Shape { id: ShapeId(2), ph: Some(ph), text: Some(kition_slide_model::TextBody::from_text("x")), ..Default::default() };
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![slide_ph];
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    // Inside the ellipse: filled; the box's corner: outside the ellipse.
    assert_eq!(img.pixel(200, 140), [0, 0, 0, 255]);
    assert_eq!(img.pixel(105, 105), [255, 255, 255, 255]);
}

/// An object kept as XML (an OLE object) shows its preview picture.
#[test]
fn kept_objects_show_their_preview() {
    let mut p = SlideFile::default();
    let red = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
    let media = p.add_media("preview.png", "image/png", png_bytes(&red));
    let obj = Shape {
        id: ShapeId(1),
        xfrm: Some(Xfrm::new(100.0, 100.0, 100.0, 50.0)),
        kind: ShapeKind::Opaque { xml: String::new(), preview: Some(media), preview_adjust: None, label: "Object".into(), rels: vec![] },
        ..Default::default()
    };
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![obj];
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    assert_eq!(img.pixel(150, 125), [255, 0, 0, 255]);
    assert_eq!(img.pixel(50, 50), [255, 255, 255, 255]);
}

/// A soft edge fades a shape from transparent at its outline to opaque about 1.6 radii inside,
/// following the outline (here a circle), as the reference renders draw it.
#[test]
fn soft_edges_follow_the_outline() {
    let sh = Shape {
        id: ShapeId(1),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 200.0)),
        geom: Geom::preset("ellipse"),
        fill: Some(Fill::solid(ColorRef::rgb(Rgba::BLACK))),
        line: Some(Line::none()),
        effects: Some(kition_slide_model::Effects { soft_edge: Some(10.0), ..Default::default() }),
        ..Default::default()
    };
    let p = slide_file_with(vec![sh]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    let grey = |x: u32| img.pixel(x, 200)[0];
    // At the outline: nearly white; half way at about 0.8 radii; opaque by 2 radii.
    assert!(grey(101) > 245, "{}", grey(101));
    assert!((60..200).contains(&grey(108)), "{}", grey(108));
    assert!(grey(122) < 10, "{}", grey(122));
    // Along the diagonal the fade follows the circle, not its box.
    let d = 100.0 + 100.0 - 100.0 / std::f64::consts::SQRT_2;
    assert!(img.pixel(d as u32 + 1, d as u32 + 1)[0] > 245);
}

/// A reflection mirrors the shape's fill and outline below it, scaled by its `sy`.
#[test]
fn reflections_mirror_the_outline_at_their_scale() {
    let mut sh = outlined(1, Xfrm::new(100.0, 100.0, 200.0, 100.0));
    let rf = kition_slide_model::style::Reflection {
        blur: 0.0,
        start_alpha: 1.0,
        end_alpha: 1.0,
        end_pos: 1.0,
        dist: 0.0,
        sy: -0.5,
        ..Default::default()
    };
    sh.effects = Some(kition_slide_model::Effects { reflection: Some(rf), ..Default::default() });
    let p = slide_file_with(vec![sh]);
    let img = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    // The mirrored top edge lies half the height below the bottom edge…
    assert!(dark_pixels(&img, (150, 248, 250, 252)) > 150, "{}", dark_pixels(&img, (150, 248, 250, 252)));
    // …not a full height below it, and nothing is drawn inside.
    assert_eq!(dark_pixels(&img, (150, 290, 250, 310)), 0);
    assert_eq!(dark_pixels(&img, (150, 210, 250, 240)), 0);
}

#[test]
fn inner_shadows_follow_the_authored_direction_through_shape_transforms() {
    use kition_slide_model::style::{Effects, Shadow};
    for direction in [0.0_f64, 90.0, 180.0, 270.0, 315.0] {
        for (rotation, reflected) in [(0.0, false), (90.0, true)] {
            let xfrm = Xfrm { rot: rotation, flip_h: reflected, ..Xfrm::new(60.0, 60.0, 100.0, 100.0) };
            let transform = xfrm.affine();
            let shape = Shape {
                id: ShapeId(801),
                xfrm: Some(xfrm),
                geom: Geom::preset("ellipse"),
                fill: Some(Fill::solid(ColorRef::rgb(Rgba::WHITE))),
                line: Some(Line { fill: Some(Fill::None), ..Default::default() }),
                effects: Some(Effects {
                    inner_shadow: Some(Shadow {
                        color: ColorRef::rgb(Rgba::rgba(0, 0, 0, 128)),
                        blur: 0.0,
                        dist: 12.0,
                        dir: direction,
                        inner: true,
                        sx: 1.0,
                        sy: 1.0,
                        kx: 0.0,
                        ky: 0.0,
                        align: String::new(),
                        rotate_with_shape: true,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
            let axis = Vec2::new(direction.to_radians().cos(), direction.to_radians().sin());
            for (sign, expected) in [(1.0, 127u8), (-1.0, 255u8)] {
                let point = transform * (Point::new(50.0, 50.0) + axis * (43.0 * sign));
                let pixel = image.pixel(point.x.floor() as u32, point.y.floor() as u32);
                assert!(
                    pixel[..3].iter().all(|channel| channel.abs_diff(expected) <= 1),
                    "direction={direction}, rotation={rotation}, reflected={reflected}, side={sign}: {pixel:?}"
                );
                assert_eq!(pixel[3], 255);
            }
            assert_eq!(image.pixel(110, 110), [255; 4]);
            assert_eq!(image.pixel(50, 50), [255; 4], "the inner shadow remains clipped to the ellipse");
        }
    }
}

#[test]
fn asymmetric_closed_outlines_keep_interior_exterior_bands_and_one_alpha_composite() {
    use kition_slide_model::style::Compound;
    for (compound, bands, gap) in [(Compound::ThinThick, [32, 43], 35), (Compound::ThickThin, [35, 47], 43)] {
        let mut shape = outlined(2, Xfrm::new(40.0, 40.0, 200.0, 100.0));
        shape.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0))));
        shape.line = Some(Line { compound: Some(compound), ..Line::solid(ColorRef::rgb(Rgba::rgba(255, 0, 0, 128)), 20.0) });
        let image = render_slide(&slide_file_with(vec![shape]), 0, &RenderOpts { scale: 1.0, ..Default::default() });
        for y in bands {
            let pixel = image.pixel(140, y);
            let expected = if y < 40 { [255, 127, 127, 255] } else { [128, 64, 0, 255] };
            assert!(pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1), "{compound:?} at {y}: {pixel:?}");
        }
        assert_eq!(image.pixel(140, gap), if gap < 40 { [255; 4] } else { [0, 128, 0, 255] });
        assert_eq!(image.pixel(140, 29), [255; 4]);
    }
}
