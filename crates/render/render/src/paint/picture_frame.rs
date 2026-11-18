//! Shape-local rotation for picture fills that stay fixed in their parent coordinate space.

use kition_slide_geom::Xfrm;
use kurbo::{Affine, BezPath, Rect, Shape as _};

pub(super) fn fixed(bounds: Rect, outline: &BezPath, to_device: Affine, shape: &Xfrm) -> Option<(Rect, Affine)> {
    if !shape.is_finite() || !bounds.is_finite() || !to_device.is_finite() {
        return None;
    }
    // Preserve unrotated images byte-for-byte, including their crop, tile and flip behavior.
    if shape.rot % 360.0 == 0.0 {
        return Some((bounds, to_device));
    }
    let local = shape.affine();
    let inverse = local.inverse();
    if !inverse.is_finite() || !to_device.inverse().is_finite() {
        return None;
    }
    let parent = to_device * inverse;
    if outline.is_empty() || !outline.is_finite() {
        return None;
    }
    // The image spans the actual transformed silhouette. Rotating its original rectangular
    // frame would enlarge rounded or concave shapes beyond the geometry they paint.
    let mut transformed = outline.clone();
    transformed.apply_affine(local);
    let region = transformed.bounding_box();
    let center = region.center().to_vec2();
    // rotWithShape controls rotation; horizontal and vertical flips remain independent.
    let reflection = Affine::translate(center)
        * Affine::scale_non_uniform(if shape.flip_h { -1.0 } else { 1.0 }, if shape.flip_v { -1.0 } else { 1.0 })
        * Affine::translate(-center);
    let paint = parent * reflection;
    (region.is_finite() && paint.is_finite()).then_some((region, paint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Point;

    #[test]
    fn fixed_fill_retains_parent_transform_and_shape_reflection() {
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        for group_angle in [0.0_f64, 30.0, 90.0] {
            let parent = Affine::translate((300.0, 200.0)) * Affine::rotate(group_angle.to_radians()) * Affine::scale_non_uniform(2.0, 3.0);
            for flip_h in [false, true] {
                for flip_v in [false, true] {
                    let shape = Xfrm { rot: 90.0, flip_h, flip_v, ..Xfrm::new(20.0, 40.0, 120.0, 60.0) };
                    let (region, paint) = fixed(bounds, &bounds.to_path(0.01), parent * shape.affine(), &shape).unwrap();
                    // A quarter turn exchanges the dimensions about the authored center (80,70).
                    for (actual, expected) in [(region.x0, 50.0), (region.y0, 10.0), (region.x1, 110.0), (region.y1, 130.0)] {
                        assert!((actual - expected).abs() < 1e-10);
                    }
                    let expected = parent * Point::new(if flip_h { 110.0 } else { 50.0 }, if flip_v { 130.0 } else { 10.0 });
                    assert!((paint * region.origin()).distance(expected) < 1e-10);
                }
            }
        }
    }

    #[test]
    fn rotated_curved_outlines_use_analytic_silhouette_extents() {
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        let shape = Xfrm { rot: 45.0, ..Xfrm::new(20.0, 40.0, 120.0, 60.0) };
        let round_rect = kurbo::RoundedRect::new(0.0, 0.0, 120.0, 60.0, 20.0).to_path(1e-8);
        let ellipse = kurbo::Ellipse::new(Point::new(60.0, 30.0), (60.0, 30.0), 0.0).to_path(1e-8);
        // Rounded rectangle: straight spans projected on the axis, plus the corner diameter.
        // Ellipse: the support radius is hypot(rx*cos(theta), ry*sin(theta)).
        let root_two = std::f64::consts::SQRT_2;
        for (path, extent) in [(round_rect, 100.0 / root_two + 40.0), (ellipse, 2.0 * (2250.0_f64).sqrt())] {
            let (region, _) = fixed(bounds, &path, shape.affine(), &shape).unwrap();
            assert!((region.width() - extent).abs() < 1e-5);
            assert!((region.height() - extent).abs() < 1e-5);
            assert!(region.center().distance(Point::new(80.0, 70.0)) < 1e-8);
        }
    }

    #[test]
    fn concave_and_multiple_subpaths_share_one_actual_geometry_region() {
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        let shape = Xfrm { rot: 45.0, ..Xfrm::new(20.0, 40.0, 120.0, 60.0) };
        let mut outline = BezPath::new();
        outline.move_to((0.0, 0.0));
        for point in [(120.0, 0.0), (120.0, 20.0), (20.0, 20.0), (20.0, 60.0), (0.0, 60.0)] {
            outline.line_to(point);
        }
        outline.close_path();
        // An interior disconnected path must not get a separate image coordinate system.
        outline.extend(Rect::new(40.0, 30.0, 50.0, 40.0).to_path(0.01));
        let (region, _) = fixed(bounds, &outline, shape.affine(), &shape).unwrap();
        assert!((region.width() - 180.0 / std::f64::consts::SQRT_2).abs() < 1e-10);
        assert!((region.height() - 140.0 / std::f64::consts::SQRT_2).abs() < 1e-10);
    }

    #[test]
    fn unrotated_fill_is_unchanged_and_invalid_transforms_do_not_escape() {
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        let shape = Xfrm { flip_h: true, ..Xfrm::new(20.0, 40.0, 120.0, 60.0) };
        let transform = Affine::rotate(0.7) * shape.affine();
        assert_eq!(fixed(bounds, &bounds.to_path(0.01), transform, &shape), Some((bounds, transform)));
        let rotated = Xfrm { rot: 45.0, ..shape };
        for bad in [Affine::scale(0.0), Affine::scale(f64::INFINITY), Affine::new([1.0, 1.0, 1.0, 1.0, 0.0, 0.0])] {
            assert!(fixed(bounds, &bounds.to_path(0.01), bad, &rotated).is_none());
        }
        assert!(fixed(bounds, &bounds.to_path(0.01), Affine::IDENTITY, &Xfrm { rot: f64::NAN, ..rotated }).is_none());
    }

    fn picture_deck(
        rotate: bool,
        picture_kind: bool,
        flip_h: bool,
        flip_v: bool,
        mode: kition_slide_model::style::PictureMode,
        crop: [f64; 4],
    ) -> kition_slide_model::SlideFile {
        use kition_slide_model::style::{Fill, Line, PictureFill};
        use kition_slide_model::{Geom, Shape, ShapeId, ShapeKind};
        let bitmap = image::RgbaImage::from_fn(48, 32, |x, y| {
            image::Rgba(match (x < 24, y < 16) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 0, 255],
            })
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        bitmap.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let mut pres = kition_slide_model::SlideFile::default();
        let media = pres.add_media("quadrants.png", "image/png", bytes.into_inner());
        let fill = PictureFill { media, rotate_with_shape: rotate, mode, crop, ..Default::default() };
        let mut shape = Shape {
            id: ShapeId(20),
            geom: Geom::preset("rect"),
            xfrm: Some(Xfrm { rot: 90.0, flip_h, flip_v, ..Xfrm::new(100.0, 100.0, 160.0, 80.0) }),
            line: Some(Line { fill: Some(Fill::None), ..Default::default() }),
            ..Default::default()
        };
        if picture_kind {
            shape.kind = ShapeKind::Picture { fill };
        } else {
            shape.fill = Some(Fill::Picture(fill));
        }
        std::sync::Arc::make_mut(&mut pres.slides[0]).shapes = vec![shape];
        pres
    }

    fn raster(pres: &kition_slide_model::SlideFile) -> crate::Image {
        crate::render_slide(pres, 0, &crate::RenderOpts { scale: 1.0, ..Default::default() })
    }

    #[test]
    fn fixed_pictures_and_shape_fills_keep_upright_quadrants_and_independent_flips() {
        for picture_kind in [false, true] {
            for (flip_h, flip_v, expected) in [(false, false, [255, 0, 0, 255]), (true, false, [0, 255, 0, 255]), (false, true, [0, 0, 255, 255])] {
                let pres = picture_deck(false, picture_kind, flip_h, flip_v, Default::default(), [0.0; 4]);
                let image = raster(&pres);
                assert_eq!(image.pixel(160, 80), expected);
                assert_eq!(image.pixel(139, 140), [255; 4], "the rotated shape still clips the image");
            }
            let rotates = raster(&picture_deck(true, picture_kind, false, false, Default::default(), [0.0; 4]));
            assert_eq!(rotates.pixel(160, 80), [0, 0, 255, 255]);
            assert_eq!(rotates.pixel(200, 80), [255, 0, 0, 255]);
        }
    }

    #[test]
    fn containing_group_rotation_still_rotates_a_fixed_child_picture() {
        use kition_slide_model::{Shape, ShapeId, ShapeKind};
        let mut pres = picture_deck(false, false, false, false, Default::default(), [0.0; 4]);
        let children = std::mem::take(&mut std::sync::Arc::make_mut(&mut pres.slides[0]).shapes);
        let group = Shape {
            id: ShapeId(21),
            xfrm: Some(Xfrm { rot: 90.0, ..Xfrm::new(0.0, 0.0, 400.0, 400.0) }),
            kind: ShapeKind::Group { children, child: Xfrm::new(0.0, 0.0, 400.0, 400.0) },
            ..Default::default()
        };
        std::sync::Arc::make_mut(&mut pres.slides[0]).shapes = vec![group];
        let image = raster(&pres);
        assert_eq!(image.pixel(320, 160), [255, 0, 0, 255]);
        assert_eq!(image.pixel(320, 200), [0, 255, 0, 255]);
    }

    #[test]
    fn fixed_picture_crop_stretch_and_tile_are_applied_in_the_same_parent_frame() {
        use kition_slide_model::style::PictureMode;
        let cropped = raster(&picture_deck(false, false, false, false, Default::default(), [0.5, 0.0, 0.0, 0.0]));
        assert_eq!(cropped.pixel(160, 80), [0, 255, 0, 255]);
        assert_eq!(cropped.pixel(160, 200), [255, 255, 0, 255]);
        let inset = PictureMode::Stretch { fill_rect: [0.25; 4] };
        let stretched = raster(&picture_deck(false, false, false, false, inset, [0.0; 4]));
        assert_eq!(stretched.pixel(170, 110), [255, 0, 0, 255]);
        assert_eq!(stretched.pixel(150, 80), [255; 4]);
        let mode = PictureMode::Tile { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, flip: "none".into(), align: "tl".into() };
        let tiled = raster(&picture_deck(false, false, false, false, mode, [0.0; 4]));
        assert_eq!(tiled.pixel(146, 66), [255, 0, 0, 255]);
        assert_eq!(tiled.pixel(170, 66), [0, 255, 0, 255]);
        assert_eq!(tiled.pixel(194, 98), [255, 0, 0, 255]);
    }

    #[test]
    fn missing_fixed_picture_uses_the_selected_fill_frame() {
        let mut pres = picture_deck(false, false, false, false, Default::default(), [0.0; 4]);
        pres.media.clear();
        let image = raster(&pres);
        assert_eq!(image.pixel(160, 80), [230, 230, 230, 255]);
        assert_eq!(image.pixel(139, 140), [255; 4]);
    }

    #[test]
    fn fixed_picture_reflection_mirrors_the_displayed_image() {
        use kition_slide_model::style::{Effects, Reflection};
        let mut pres = picture_deck(false, false, false, false, Default::default(), [0.0; 4]);
        std::sync::Arc::make_mut(&mut pres.slides[0]).shapes[0].effects = Some(Effects {
            reflection: Some(Reflection { blur: 0.0, dist: 0.0, sy: 1.0, start_alpha: 1.0, end_alpha: 1.0, end_pos: 1.0, ..Default::default() }),
            ..Default::default()
        });
        let image = raster(&pres);
        assert_eq!(image.pixel(160, 80), [255, 0, 0, 255]);
        assert_eq!(image.pixel(120, 80), [255, 0, 0, 255]);
        assert_eq!(image.pixel(80, 200), [255, 255, 0, 255]);
    }
}
