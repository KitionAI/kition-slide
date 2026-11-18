use super::*;
use kition_slide_geom::Xfrm;
use kition_slide_model::style::{Fill, Glow, Shadow};
use kition_slide_model::text::TextBody;
use kition_slide_model::{ColorRef, Shape, ShapeId, SlideFile};
use std::sync::Arc;

fn slide_file() -> SlideFile {
    let mut p = SlideFile::default();
    let mut text = TextBody::from_text("MOM");
    let props = &mut text.paragraphs[0].runs[0].props;
    props.font = Some("Ubuntu".into());
    props.size = Some(64.0);
    props.fill = Some(Fill::solid(ColorRef::rgb(Rgba::BLACK)));
    Arc::make_mut(&mut p.slides[0]).shapes =
        vec![Shape { id: ShapeId(42), xfrm: Some(Xfrm::new(40.0, 40.0, 400.0, 160.0)), text: Some(text), ..Default::default() }];
    p
}

fn props(p: &mut SlideFile) -> &mut kition_slide_model::text::RunProps {
    &mut Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs[0].props
}

fn render(p: &SlideFile) -> crate::Image {
    crate::render_slide(p, 0, &crate::RenderOpts { scale: 1.0, ..Default::default() })
}

fn shadow() -> Shadow {
    Shadow {
        color: ColorRef::rgb(Rgba::rgb(0, 0, 255)),
        blur: 0.0,
        dist: 30.0,
        dir: 90.0,
        inner: false,
        sx: 1.0,
        sy: 1.0,
        kx: 0.0,
        ky: 0.0,
        align: "ctr".into(),
        rotate_with_shape: true,
    }
}

#[test]
fn run_shadow_follows_glyph_silhouette_without_moving_foreground() {
    let mut p = slide_file();
    let before = render(&p);
    props(&mut p).shadow = Some(shadow());
    let after = render(&p);
    let mut shadow_pixels = 0;
    for y in 0..before.height - 30 {
        for x in 0..before.width {
            if before.pixel(x, y) == [0, 0, 0, 255] {
                assert_eq!(after.pixel(x, y), [0, 0, 0, 255]);
                if before.pixel(x, y + 30) == [255; 4] {
                    assert_eq!(after.pixel(x, y + 30), [0, 0, 255, 255]);
                    shadow_pixels += 1;
                }
            }
        }
    }
    assert!(shadow_pixels > 200, "the shadow is translated glyph ink, not the text box");
}

#[test]
fn glow_is_visible_and_opacity_is_applied_once_to_overlapping_fill_and_stroke() {
    let mut p = slide_file();
    let before = render(&p);
    props(&mut p).glow = Some(Glow { color: ColorRef::rgb(Rgba::rgb(0, 0, 255).with_alpha(128)), radius: 8.0 });
    let after = render(&p);
    let mut halo = 0;
    for y in 0..before.height {
        for x in 0..before.width {
            let old = before.pixel(x, y);
            let new = after.pixel(x, y);
            if old == [0, 0, 0, 255] {
                assert_eq!(new, old);
            }
            if old == [255; 4] && new[0] < 245 && new[2] == 255 {
                assert!(new[0] >= 126, "half-opacity glow must not accumulate opacity at overlaps");
                assert_eq!(new[0], new[1]);
                halo += 1;
            }
        }
    }
    assert!(halo > 200);
}

#[test]
fn invisible_or_unsafe_run_effects_keep_plain_pixels() {
    let mut p = slide_file();
    let before = render(&p);
    props(&mut p).glow = Some(Glow { color: ColorRef::rgb(Rgba::TRANSPARENT), radius: 8.0 });
    assert_eq!(render(&p).pixels, before.pixels);
    props(&mut p).glow = Some(Glow { color: ColorRef::rgb(Rgba::BLACK), radius: f64::INFINITY });
    props(&mut p).shadow = Some(Shadow { dist: 1e12, ..shadow() });
    assert_eq!(render(&p).pixels, before.pixels);
    props(&mut p).shadow = Some(Shadow { kx: f64::NAN, ..shadow() });
    assert_eq!(render(&p).pixels, before.pixels);
}

#[test]
fn coincident_same_color_shadow_does_not_strengthen_antialiased_edges() {
    let mut p = slide_file();
    let before = render(&p);
    for alpha in [64, 128, 255] {
        props(&mut p).shadow = Some(Shadow { color: ColorRef::rgb(Rgba::BLACK.with_alpha(alpha)), dist: 0.0, ..shadow() });
        for outline in [
            None,
            Some(kition_slide_model::style::Line {
                width: Some(20.0),
                fill: Some(Fill::solid(ColorRef::rgb(Rgba::TRANSPARENT))),
                ..Default::default()
            }),
        ] {
            props(&mut p).outline = outline;
            assert_eq!(render(&p).pixels, before.pixels);
        }
    }
}

#[test]
fn effect_bounds_include_skew_and_filter_extent() {
    let bounds = Rect::new(0.0, 0.0, 100.0, 30.0);
    assert!(fits(bounds, Affine::IDENTITY, 2.0, 1.0));
    assert!(!fits(bounds, Affine::new([1.0, 0.0, 1e6, 1.0, 0.0, 0.0]), 2.0, 1.0));
    assert!(!fits(bounds, Affine::translate((EFFECT_LIMIT - 101.0, 0.0)), 2.0, 1.0));
    assert!(!fits(bounds, Affine::IDENTITY, f64::NAN, 1.0));
}

#[test]
fn transparent_text_outline_does_not_enlarge_glow_or_shadow() {
    for glow in [false, true] {
        let mut p = slide_file();
        if glow {
            props(&mut p).glow = Some(Glow { color: ColorRef::rgb(Rgba::rgb(0, 0, 255).with_alpha(128)), radius: 8.0 });
        } else {
            props(&mut p).shadow = Some(Shadow { blur: 4.0, ..shadow() });
        }
        let before = render(&p);
        props(&mut p).outline = Some(kition_slide_model::style::Line {
            width: Some(20.0),
            fill: Some(Fill::solid(ColorRef::rgb(Rgba::TRANSPARENT))),
            ..Default::default()
        });
        assert_eq!(render(&p).pixels, before.pixels, "an invisible outline must not change the visible effect silhouette");
    }
}

#[test]
fn resolved_and_transformed_fill_alpha_render_identically_with_text_effects() {
    use kition_slide_color::ColorTransform;
    for (value, alpha) in [(0, 0), (50_000, 128), (100_000, 255)] {
        for effect in 0..3 {
            let mut p = slide_file();
            if effect == 1 {
                props(&mut p).shadow = Some(Shadow { blur: 4.0, ..shadow() });
            } else if effect == 2 {
                props(&mut p).glow = Some(Glow { color: ColorRef::rgb(Rgba::rgb(0, 0, 255).with_alpha(128)), radius: 8.0 });
            }
            props(&mut p).fill = Some(Fill::solid(ColorRef::rgb(Rgba::BLACK.with_alpha(alpha))));
            let resolved = render(&p);
            props(&mut p).fill = Some(Fill::solid(ColorRef::rgb(Rgba::BLACK).with(ColorTransform::Alpha(value))));
            assert_eq!(render(&p).pixels, resolved.pixels, "equivalent fill alpha must affect glyphs and effects exactly once");
            if effect == 0 && alpha == 128 {
                assert!(resolved.pixels.as_chunks::<4>().0.iter().any(|p| p[0] == 127 && p[1] == 127 && p[2] == 127));
            }
        }
    }
}

#[test]
fn synthetic_bold_without_visible_fill_does_not_widen_outline_effects() {
    let p = slide_file();
    let shape = &p.slides[0].shapes[0];
    let source = kition_slide_model::resolve::Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let layout = kition_slide_text::layout(
        &source,
        shape,
        shape.text.as_ref().unwrap(),
        &kition_slide_text::Opts {
            rect: Rect::new(20.0, 20.0, 380.0, 200.0),
            fields: &kition_slide_text::NoFields,
            prompt_color: None,
            no_shrink: true,
        },
    );
    let mut run = layout.runs[0].clone();
    run.outline = Some((Rgba::rgb(0, 128, 0), 0.5));
    let effect_image = |run: &GlyphRun| {
        let mut ctx = RenderContext::new_with(400, 240, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        ctx.set_paint(color(Rgba::WHITE, 1.0));
        ctx.fill_rect(&Rect::new(0.0, 0.0, 400.0, 240.0));
        draw(&mut ctx, run, Affine::IDENTITY, 1.0);
        crate::Renderer::default().finish(ctx, 400, 240)
    };
    for glow in [false, true] {
        run.shadow = (!glow).then(|| (Shadow { blur: 4.0, ..shadow() }, Rgba::rgb(0, 0, 255)));
        run.glow = glow.then(|| (Glow { color: ColorRef::rgb(Rgba::rgb(0, 0, 255)), radius: 8.0 }, Rgba::rgb(0, 0, 255)));
        for (color, alpha) in [(Rgba::TRANSPARENT, 1.0), (Rgba::BLACK, 0.0)] {
            run.color = color;
            run.alpha = alpha;
            run.fake_bold = false;
            let plain = effect_image(&run);
            assert!(plain.pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[0] < 245));
            run.fake_bold = true;
            assert_eq!(effect_image(&run).pixels, plain.pixels, "only the visible outline contributes to the effect");
        }
        run.color = Rgba::BLACK;
        run.alpha = 1.0;
        run.fake_bold = false;
        let plain = effect_image(&run);
        run.fake_bold = true;
        assert_ne!(effect_image(&run).pixels, plain.pixels, "visible synthetic bold still contributes its wider silhouette");
    }
}

#[test]
fn prepared_effect_uses_relocated_ink_and_paragraph_opacity() {
    use kition_slide_text::{NoFields, Opts};
    let mut pres = slide_file();
    props(&mut pres).shadow = Some(shadow());
    let rctx = kition_slide_model::resolve::Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let shape = &pres.slides[0].shapes[0];
    let layout = kition_slide_text::layout(
        &rctx,
        shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, 400.0, 160.0), fields: &NoFields, prompt_color: None, no_shrink: false },
    );
    let path = Rect::new(20.0, 20.0, 60.0, 40.0).to_path(0.1);
    let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_path(&mut ctx, &layout.runs[0], &path, Affine::translate((10.0, 10.0)), 0.5);
    let image = crate::Renderer::default().finish(ctx, 160, 120);
    let shadow = image.pixel(40, 65);
    assert_eq!(shadow[0], 0);
    assert_eq!(shadow[1], 0);
    assert_eq!(shadow[2], 255);
    assert!((127..=128).contains(&shadow[3]), "unexpected shadow alpha: {shadow:?}");
    assert_eq!(image.pixel(40, 35), [0; 4], "the effect pass must not draw foreground ink");
    assert_eq!(image.pixel(40, 85), [0; 4]);
}
