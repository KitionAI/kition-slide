//! Resolve picture recoloring at paint time, before its colors enter the decoded-image cache.

use std::borrow::Cow;

use kition_slide_color::Rgba;
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorBase, ColorRef, PictureAdjust};

pub(super) fn resolve<'a>(ctx: &Ctx, adjust: &'a PictureAdjust, ph: Option<Rgba>) -> Cow<'a, PictureAdjust> {
    let Some((dark, light)) = &adjust.duotone else { return Cow::Borrowed(adjust) };
    // The image decoder already applies transforms on RGB colors. Keep its existing path
    // and cache keys unchanged when neither endpoint depends on a theme or named color.
    if matches!(dark.base, ColorBase::Rgb { .. }) && matches!(light.base, ColorBase::Rgb { .. }) {
        return Cow::Borrowed(adjust);
    }
    let mut resolved = adjust.clone();
    resolved.duotone = Some((ColorRef::rgb(ctx.color(dark, ph)), ColorRef::rgb(ctx.color(light, ph))));
    Cow::Owned(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_color::{ColorTransform, SchemeSlot};
    use kition_slide_model::style::{Fill, PictureFill};
    use kition_slide_model::{Geom, Line, Shape, ShapeId, ShapeKind, SlideFile, Xfrm};
    use std::sync::Arc;

    fn named_adjust() -> PictureAdjust {
        PictureAdjust {
            duotone: Some((
                ColorRef::scheme(SchemeSlot::Accent1).with(ColorTransform::Shade(50000)),
                ColorRef { base: ColorBase::Preset { name: "white".into() }, mods: vec![ColorTransform::Shade(75000)] },
            )),
            ..Default::default()
        }
    }

    #[test]
    fn picture_colors_resolve_named_modifiers_and_placeholder_color_without_mutating_source() {
        let mut p = SlideFile::default();
        Arc::make_mut(&mut p.masters[0]).theme.colors.set(SchemeSlot::Accent1, Rgba::rgb(200, 80, 40));
        let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
        let adjust = named_adjust();
        let original = adjust.clone();
        let resolved = resolve(&ctx, &adjust, None);
        assert_eq!(adjust, original);
        let dark = kition_slide_color::apply(Rgba::rgb(200, 80, 40), &[ColorTransform::Shade(50000)]);
        let light = kition_slide_color::apply(Rgba::WHITE, &[ColorTransform::Shade(75000)]);
        assert_eq!(resolved.duotone, Some((ColorRef::rgb(dark), ColorRef::rgb(light))));
        let placeholder = PictureAdjust {
            duotone: Some((ColorRef::scheme(SchemeSlot::PhClr).with(ColorTransform::Shade(50000)), ColorRef::rgb(Rgba::WHITE))),
            ..Default::default()
        };
        assert_eq!(
            resolve(&ctx, &placeholder, Some(Rgba::rgb(40, 120, 200))).duotone.as_ref().unwrap().0,
            ColorRef::rgb(kition_slide_color::apply(Rgba::rgb(40, 120, 200), &[ColorTransform::Shade(50000)]))
        );
    }

    #[test]
    fn absent_and_rgb_duotones_keep_the_existing_decoder_path_including_modifiers() {
        let p = SlideFile::default();
        let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
        for adjust in [
            PictureAdjust::default(),
            PictureAdjust {
                duotone: Some((ColorRef::rgb(Rgba::rgb(200, 80, 40)).with(ColorTransform::Shade(50000)), ColorRef::rgb(Rgba::WHITE))),
                ..Default::default()
            },
        ] {
            let resolved = resolve(&ctx, &adjust, None);
            assert!(matches!(resolved, Cow::Borrowed(_)));
            assert!(std::ptr::eq(resolved.as_ref(), &adjust));
        }
    }

    fn picture_deck(kind: u8) -> SlideFile {
        let bitmap = image::RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba(if y >= 8 {
                [0, 0, 0, 0]
            } else if x < 8 {
                [0, 0, 0, 128]
            } else {
                [255, 255, 255, 255]
            })
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        bitmap.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let mut p = SlideFile::default();
        let media = p.add_media("duotone.png", "image/png", bytes.into_inner());
        let fill = PictureFill { media, adjust: named_adjust(), rotate_with_shape: kind != 2, ..Default::default() };
        let mut shape = Shape {
            id: ShapeId(20),
            xfrm: Some(Xfrm { rot: if kind == 2 { 90.0 } else { 0.0 }, ..Xfrm::new(100.0, 100.0, 80.0, 80.0) }),
            geom: Geom::preset("rect"),
            line: Some(Line::none()),
            ..Default::default()
        };
        if kind == 0 {
            shape.kind = ShapeKind::Picture { fill };
        } else {
            shape.fill = Some(Fill::Picture(fill));
        }
        Arc::make_mut(&mut p.slides[0]).shapes = vec![shape];
        p
    }

    #[test]
    fn shared_picture_bytes_use_resolved_theme_cache_keys_for_pictures_and_shape_fills() {
        let render = |p: &SlideFile| crate::render_slide(p, 0, &crate::RenderOpts { scale: 1.0, ..Default::default() });
        for kind in 0..3 {
            let mut p = picture_deck(kind);
            let original = p.slides[0].shapes[0].clone();
            let mut outputs = Vec::new();
            for theme in [Rgba::rgb(200, 80, 40), Rgba::rgb(40, 80, 200), Rgba::rgb(200, 80, 40)] {
                Arc::make_mut(&mut p.masters[0]).theme.colors.set(SchemeSlot::Accent1, theme);
                let actual = render(&p);
                let mut baked = p.clone();
                assert!(Arc::ptr_eq(&p.media[0].data, &baked.media[0].data));
                let shape = &mut Arc::make_mut(&mut baked.slides[0]).shapes[0];
                let fill = match &mut shape.kind {
                    ShapeKind::Picture { fill } => fill,
                    _ => match shape.fill.as_mut().unwrap() {
                        Fill::Picture(fill) => fill,
                        _ => panic!("expected picture fill"),
                    },
                };
                fill.adjust.duotone =
                    Some((ColorRef::rgb(theme).with(ColorTransform::Shade(50000)), ColorRef::rgb(Rgba::WHITE).with(ColorTransform::Shade(75000))));
                assert_eq!(actual.pixels, render(&baked).pixels, "picture kind {kind}");
                let light = kition_slide_color::apply(Rgba::WHITE, &[ColorTransform::Shade(75000)]);
                assert_eq!(actual.pixel(160, 120), [light.r, light.g, light.b, light.a]);
                assert_eq!(actual.pixel(120, 160), [255; 4], "source transparency must survive recoloring");
                let pixel = actual.pixel(120, 120);
                assert!(pixel[0] > 127 && pixel[2] > 127, "half-transparent dark pixels blend over white: {pixel:?}");
                outputs.push(actual.pixels);
                assert_eq!(p.slides[0].shapes[0], original, "rendering must not replace authored color references");
            }
            assert_ne!(outputs[0], outputs[1], "different themes must not reuse the first decoded recoloring");
            assert_eq!(outputs[0], outputs[2], "returning to a theme must reproduce its original paint");
        }
    }
}
