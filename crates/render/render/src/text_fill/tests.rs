use super::*;
use kition_slide_color::Rgba;
use kition_slide_model::style::{ColorRef, PatternFill, PictureFill};
use kition_slide_model::text::TextBody;
use kition_slide_model::{Shape, SlideFile};
use kition_slide_text::{NoFields, Opts, TextLayout};

fn presentation() -> (SlideFile, PictureFill) {
    let mut pres = SlideFile::default();
    let image = image::RgbaImage::from_fn(32, 8, |x, _| if x < 16 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let media = pres.add_media("text-fill.png", "image/png", bytes.into_inner());
    (pres, PictureFill { media, ..Default::default() })
}

fn layout(rctx: &Ctx, fill: Fill) -> TextLayout {
    let mut body = TextBody::from_text("MMMM");
    let run = &mut body.paragraphs[0].runs[0];
    run.props.font = Some("Ubuntu".into());
    run.props.size = Some(64.0);
    run.props.fill = Some(fill);
    let shape = Shape { text: Some(body.clone()), ..Default::default() };
    kition_slide_text::layout(
        rctx,
        &shape,
        &body,
        &Opts { rect: Rect::new(0.0, 0.0, 400.0, 160.0), fields: &NoFields, prompt_color: None, no_shrink: false, empty_bullets: false },
    )
}

fn render(rctx: &Ctx, layout: &TextLayout, opacity: f64) -> crate::Image {
    let mut ctx = RenderContext::new_with(400, 160, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    crate::draw_layout_with_context(
        &mut ctx,
        rctx,
        layout,
        Rect::new(0.0, 0.0, 400.0, 160.0),
        Affine::IDENTITY,
        &[(0, crate::ParaState { opacity, ..Default::default() })],
    );
    crate::Renderer::default().finish(ctx, 400, 160)
}

#[test]
fn picture_text_keeps_both_image_regions_and_one_frame_across_fragments() {
    let (pres, picture) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let mut layout = layout(&rctx, Fill::Picture(picture));
    assert_eq!(layout.runs.len(), 1);
    let original = render(&rctx, &layout, 1.0);
    let pixels = original.pixels.as_chunks::<4>().0;
    assert!(pixels.iter().filter(|p| p[0] > 200 && p[2] < 30 && p[3] > 200).count() > 100);
    assert!(pixels.iter().filter(|p| p[2] > 200 && p[0] < 30 && p[3] > 200).count() > 100);
    let mut left = layout.runs[0].clone();
    let mut right = left.clone();
    left.glyphs.truncate(2);
    left.cells.truncate(2);
    left.chars.1 = 2;
    right.glyphs.drain(..2);
    right.cells.drain(..2);
    right.chars.0 = 2;
    layout.runs = vec![left, right];
    assert_eq!(crate::text_gradient::group_end(&layout, 0), 2);
    assert_eq!(render(&rctx, &layout, 1.0).pixels, original.pixels);
}

#[test]
fn picture_text_multiplies_picture_and_paragraph_alpha_once() {
    let (pres, mut picture) = presentation();
    picture.alpha = Some(0.5);
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let layout = layout(&rctx, Fill::Picture(picture));
    let image = render(&rctx, &layout, 0.5);
    let maximum = image.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap();
    assert!((63..=65).contains(&maximum), "expected quarter opacity, got {maximum}");
    assert_eq!(image.pixel(399, 159), [0, 0, 0, 0], "the image stays clipped to glyphs");
}

#[test]
fn text_picture_crop_uses_the_existing_picture_fill_path() {
    let (pres, mut picture) = presentation();
    picture.crop = [0.5, 0.0, 0.0, 0.0];
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let layout = layout(&rctx, Fill::Picture(picture));
    let image = render(&rctx, &layout, 1.0);
    let visible: Vec<_> = image.pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 200).collect();
    assert!(visible.len() > 100);
    assert!(visible.iter().all(|p| p[2] > p[0]), "left source half is cropped out");
}

#[test]
fn patterned_text_keeps_foreground_background_and_transparent_exterior() {
    let (pres, _) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let layout = layout(
        &rctx,
        Fill::Pattern(PatternFill { preset: "pct50".into(), fg: ColorRef::rgb(Rgba::rgb(255, 0, 0)), bg: ColorRef::rgb(Rgba::rgb(0, 0, 255)) }),
    );
    let image = render(&rctx, &layout, 1.0);
    let pixels = image.pixels.as_chunks::<4>().0;
    assert!(pixels.iter().filter(|p| p[0] > 200 && p[2] < 30 && p[3] > 200).count() > 100);
    assert!(pixels.iter().filter(|p| p[2] > 200 && p[0] < 30 && p[3] > 200).count() > 100);
    assert_eq!(image.pixel(399, 159), [0, 0, 0, 0]);
}

#[test]
fn ordinary_solid_and_empty_fills_are_unchanged_with_a_document_context() {
    let (pres, _) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    for fill in [Fill::solid(ColorRef::rgb(Rgba::rgb(200, 30, 70))), Fill::None] {
        let layout = layout(&rctx, fill);
        assert!(layout.runs.iter().all(|run| run.fill.is_none()));
        let contextual = render(&rctx, &layout, 1.0);
        let mut ctx = RenderContext::new_with(400, 160, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        crate::draw_layout(&mut ctx, &layout, Rect::new(0.0, 0.0, 400.0, 160.0), Affine::IDENTITY);
        assert_eq!(crate::Renderer::default().finish(ctx, 400, 160).pixels, contextual.pixels);
    }
}

#[test]
fn context_free_drawing_does_not_skip_picture_font_fragments() {
    let (pres, picture) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let mut layout = layout(&rctx, Fill::Picture(picture));
    let draw = |layout: &TextLayout| {
        let mut ctx = RenderContext::new_with(400, 160, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        crate::draw_layout(&mut ctx, layout, Rect::new(0.0, 0.0, 400.0, 160.0), Affine::IDENTITY);
        crate::Renderer::default().finish(ctx, 400, 160).pixels
    };
    let original = draw(&layout);
    let mut left = layout.runs[0].clone();
    let mut right = left.clone();
    left.glyphs.truncate(2);
    left.cells.truncate(2);
    left.chars.1 = 2;
    right.glyphs.drain(..2);
    right.cells.drain(..2);
    right.chars.0 = 2;
    layout.runs = vec![left, right];
    assert_eq!(draw(&layout), original);
}

#[test]
fn picture_fill_survives_layout_and_prompt_color_suppresses_it() {
    let (pres, picture) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let fill = Fill::Picture(picture);
    let mut body = TextBody::from_text("A");
    body.list_style.set(
        0,
        kition_slide_model::text::LevelStyle {
            run: kition_slide_model::text::RunProps { fill: Some(fill.clone()), ..Default::default() },
            ..Default::default()
        },
    );
    let shape = Shape { text: Some(body.clone()), ..Default::default() };
    let options = Opts { rect: Rect::new(0.0, 0.0, 400.0, 160.0), fields: &NoFields, prompt_color: None, no_shrink: false, empty_bullets: false };
    let actual = kition_slide_text::layout(&rctx, &shape, &body, &options);
    assert!(actual.runs.iter().all(|run| run.fill.as_ref() == Some(&fill)));
    let prompt = kition_slide_text::layout(&rctx, &shape, &body, &Opts { prompt_color: Some(Rgba::rgb(80, 80, 80)), ..options });
    assert!(prompt.runs.iter().all(|run| run.fill.is_none()));
    assert_eq!(body.list_style.level(0).unwrap().run.fill.as_ref(), Some(&fill));
    body.paragraphs[0].runs[0].props.fill = Some(Fill::None);
    let cleared_shape = Shape { text: Some(body.clone()), ..Default::default() };
    let cleared = kition_slide_text::layout(
        &rctx,
        &cleared_shape,
        &body,
        &Opts { rect: Rect::new(0.0, 0.0, 400.0, 160.0), fields: &NoFields, prompt_color: None, no_shrink: false, empty_bullets: false },
    );
    assert!(cleared.runs.iter().all(|run| run.fill.is_none() && run.alpha == 0.0));
}

#[test]
fn zero_alpha_picture_text_with_unavailable_media_keeps_only_its_outline() {
    for missing in [false, true] {
        let (mut pres, mut picture) = presentation();
        if missing {
            pres.media.clear();
        } else {
            pres.media[0].data = std::sync::Arc::new(b"invalid image data".to_vec());
        }
        picture.alpha = Some(0.0);
        let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
        let mut layout = layout(&rctx, Fill::Picture(picture));
        let transparent = render(&rctx, &layout, 1.0);
        assert!(transparent.pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0));
        for run in &mut layout.runs {
            run.outline = Some((Rgba::rgb(0, 255, 0), 2.0));
        }
        let outlined = render(&rctx, &layout, 1.0);
        let visible: Vec<_> = outlined.pixels.as_chunks::<4>().0.iter().filter(|pixel| pixel[3] > 200).collect();
        assert!(visible.len() > 100);
        assert!(visible.iter().all(|pixel| pixel[0] == 0 && pixel[1] > 200 && pixel[2] == 0));
    }
}

#[test]
fn structured_text_transforms_keep_the_glyph_clip_picture_direction_and_pattern_phase() {
    use kurbo::Point;
    let (pres, picture) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let rect = Rect::new(0.0, 0.0, 400.0, 160.0);
    let rotated = Affine::translate((180.0, 20.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
    let stretched = Affine::translate((40.0, 20.0)) * Affine::rotate(0.35) * Affine::scale_non_uniform(1.1, 0.65);
    let reflected = Affine::translate((500.0, 20.0)) * Affine::scale_non_uniform(-1.25, 0.75);
    // Reflected shape text remains readable about the 400-point text rectangle.
    let readable = Affine::translate((0.0, 20.0)) * Affine::scale_non_uniform(1.25, 0.75);
    let fills = [
        Fill::Picture(picture),
        Fill::Pattern(PatternFill { preset: "pct50".into(), fg: ColorRef::rgb(Rgba::rgb(255, 0, 0)), bg: ColorRef::rgb(Rgba::rgb(0, 0, 255)) }),
    ];
    for fill in fills {
        let picture_fill = matches!(fill, Fill::Picture(_));
        let mut actual = layout(&rctx, fill);
        assert_eq!(actual.runs.len(), 1);
        let ink = crate::text_effects::silhouette(&actual.runs[0]).bounding_box();
        let mut white = actual.clone();
        for run in &mut white.runs {
            run.fill = None;
            run.gradient = None;
            run.color = Rgba::WHITE;
            run.alpha = 1.0;
        }
        for (shape_transform, expected_text_transform) in [(rotated, rotated), (reflected, readable), (stretched, stretched)] {
            let raster = |text: &TextLayout| {
                let mut ctx = RenderContext::new_with(400, 300, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
                crate::draw_layout_with_context(&mut ctx, &rctx, text, rect, shape_transform, &[]);
                crate::Renderer::default().finish(ctx, 400, 300)
            };
            let image = raster(&actual);
            let mask = raster(&white);
            let mut checked = [0usize; 2];
            for y in 0..300 {
                for x in 0..400 {
                    let coverage = mask.pixel(x, y)[3];
                    let pixel = image.pixel(x, y);
                    if coverage == 0 {
                        assert_eq!(pixel[3], 0, "structured paint escaped the transformed glyph clip at {x},{y}");
                    } else if coverage == 255 {
                        assert_eq!(pixel[3], 255);
                        let red = if picture_fill {
                            let local = expected_text_transform.inverse() * Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                            if (local.x - ink.center().x).abs() < ink.width() / 8.0 {
                                continue; // Exclude interpolation around the image's two-color boundary.
                            }
                            local.x < ink.center().x
                        } else {
                            // These transforms keep the shared pattern's one-pixel cell size.
                            (x + y) % 2 == 0
                        };
                        let expected = if red { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
                        assert!(pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1), "wrong transformed paint at {x},{y}: {pixel:?}");
                        checked[usize::from(!red)] += 1;
                    }
                }
            }
            assert!(checked.into_iter().all(|count| count > 100));
            let original = actual.runs.clone();
            let mut left = original[0].clone();
            let mut right = left.clone();
            left.glyphs.truncate(2);
            left.cells.truncate(2);
            left.chars.1 = 2;
            right.glyphs.drain(..2);
            right.cells.drain(..2);
            right.chars.0 = 2;
            actual.runs = vec![left, right];
            assert_eq!(raster(&actual).pixels, image.pixels, "font fragments must keep one transformed paint frame");
            actual.runs = original;
        }
    }
}

#[test]
fn prepared_picture_uses_new_ink_shared_frame_and_single_opacity() {
    let (pres, mut picture) = presentation();
    picture.alpha = Some(0.5);
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let layout = layout(&rctx, Fill::Picture(picture));
    let runs = vec![layout.runs[0].clone(); 2];
    let paths = [Rect::new(20.0, 20.0, 60.0, 60.0).to_path(0.1), Rect::new(60.0, 20.0, 100.0, 60.0).to_path(0.1)];
    let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_paths(&mut ctx, &rctx, &runs, &paths, Affine::translate((10.0, 10.0)), 0.5);
    let image = crate::Renderer::default().finish(ctx, 160, 120);
    let left = image.pixel(35, 40);
    let right = image.pixel(105, 40);
    assert_eq!((left[0], left[2], right[0], right[2]), (255, 0, 0, 255));
    assert!((63..=65).contains(&left[3]) && left[3] == right[3]);
    assert_eq!(image.pixel(20, 40), [0; 4]);
    assert_eq!(image.pixel(35, 75), [0; 4]);
}

#[test]
fn mismatched_prepared_picture_paths_are_rejected() {
    let (pres, picture) = presentation();
    let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let layout = layout(&rctx, Fill::Picture(picture));
    let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_paths(&mut ctx, &rctx, &layout.runs, &[], Affine::IDENTITY, 1.0);
    assert!(crate::Renderer::default().finish(ctx, 160, 120).pixels.iter().all(|v| *v == 0));
}
