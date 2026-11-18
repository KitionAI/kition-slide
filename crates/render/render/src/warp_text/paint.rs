//! Paint prepared art text ink through the ordinary text fill and effect helpers.

use kition_slide_model::resolve::Ctx;
use kition_slide_text::{GlyphRun, TextLayout};
use kurbo::{Affine, BezPath, Shape as _};
use vello_cpu::RenderContext;

use crate::{ParaState, color, text_effects, text_fill, text_gradient};

fn fits(path: &BezPath, m: Affine, padding: f64) -> bool {
    if path.is_empty() || !path.is_finite() || !m.is_finite() || !padding.is_finite() {
        return false;
    }
    let bounds = m.transform_rect_bbox(path.bounding_box().inflate(padding, padding));
    [bounds.x0, bounds.y0, bounds.x1, bounds.y1].into_iter().all(|v| v.is_finite() && v.abs() <= crate::EFFECT_LIMIT)
}

fn solid(ctx: &mut RenderContext, run: &GlyphRun, path: &BezPath, m: Affine, opacity: f64) {
    if !opacity.is_finite() || !fits(path, m, 0.0) {
        return;
    }
    ctx.set_transform(m);
    let alpha = run.alpha * opacity;
    if alpha.is_finite() && alpha > 0.0 && run.color.a > 0 {
        ctx.set_paint(color(run.color, alpha));
        ctx.fill_path(path);
        if run.fake_bold && run.size.is_finite() && run.size > 0.0 && run.size <= 4000.0 && fits(path, m, run.size * 0.025 / 2.0) {
            ctx.set_stroke(kurbo::Stroke::new(run.size * 0.025).with_join(kurbo::Join::Round));
            ctx.stroke_path(path);
        }
    }
    if let Some((c, width)) = run.outline
        && width.is_finite()
        && width > 0.0
        && fits(path, m, width / 2.0)
    {
        ctx.set_paint(color(c, opacity));
        ctx.set_stroke(kurbo::Stroke::new(width).with_join(kurbo::Join::Round));
        ctx.stroke_path(path);
    }
}

/// Prepared paths share one local point coordinate space. The caller supplies its transform
/// once (shape-local for rigid follow, warp-local for the envelope); paragraph animation
/// then translates and fades the already warped ink.
pub(super) fn draw(ctx: &mut RenderContext, rctx: &Ctx, layout: &TextLayout, paths: &[Vec<BezPath>], m: Affine, paras: &[(usize, ParaState)]) {
    if layout.runs.len() != paths.len() || !m.is_finite() {
        return;
    }
    let para = |i| paras.iter().find(|(p, _)| *p == i).map(|(_, state)| *state).unwrap_or_default();
    let silhouettes: Vec<BezPath> = paths.iter().map(|glyphs| glyphs.iter().flat_map(|path| path.elements().iter().copied()).collect()).collect();
    for (run, path) in layout.runs.iter().zip(&silhouettes) {
        let state = para(run.para);
        if state.visible && state.opacity > 0.001 {
            text_effects::draw_path(ctx, run, path, m * Affine::translate(state.offset), state.opacity);
        }
    }
    let mut index = 0;
    while let Some(run) = layout.runs.get(index) {
        let end = text_gradient::group_end(layout, index);
        let state = para(run.para);
        if state.visible && state.opacity > 0.001 {
            let transform = m * Affine::translate(state.offset);
            if let (Some(runs), Some(ink)) = (layout.runs.get(index..end), silhouettes.get(index..end)) {
                if run.fill.is_some() {
                    text_fill::draw_paths(ctx, rctx, runs, ink, transform, state.opacity);
                } else if run.gradient.is_some() {
                    text_gradient::draw_paths(ctx, runs, ink, transform, state.opacity);
                } else if let Some(glyphs) = paths.get(index) {
                    for path in glyphs {
                        solid(ctx, run, path, transform, state.opacity);
                    }
                }
            }
        }
        index = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_color::Rgba;
    use kition_slide_model::style::{ColorRef, Fill, Gradient, GradientShape, GradientStop, Shadow};
    use kition_slide_model::text::TextBody;
    use kition_slide_model::{Shape, SlideFile};
    use kition_slide_text::{NoFields, Opts};
    use kurbo::{Rect, Vec2};

    fn layout(rctx: &Ctx) -> TextLayout {
        let mut text = TextBody::from_text("MM");
        text.paragraphs[0].runs[0].props.font = Some("Ubuntu".into());
        text.paragraphs[0].runs[0].props.size = Some(40.0);
        text.paragraphs[0].runs[0].props.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0))));
        let shape = Shape { text: Some(text.clone()), ..Default::default() };
        kition_slide_text::layout(
            rctx,
            &shape,
            &text,
            &Opts { rect: Rect::new(0.0, 0.0, 160.0, 120.0), fields: &NoFields, prompt_color: None, no_shrink: false },
        )
    }

    fn render(rctx: &Ctx, layout: &TextLayout, paths: &[Vec<BezPath>], state: ParaState) -> crate::Image {
        let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        draw(&mut ctx, rctx, layout, paths, Affine::IDENTITY, &[(0, state)]);
        crate::Renderer::default().finish(ctx, 160, 120)
    }

    #[test]
    fn prepared_art_text_retains_effects_outline_and_paragraph_state() {
        let pres = SlideFile::default();
        let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
        let mut layout = layout(&rctx);
        let run = &mut layout.runs[0];
        run.outline = Some((Rgba::rgb(0, 255, 0), 2.0));
        run.shadow = Some((
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
            },
            Rgba::rgb(0, 0, 255),
        ));
        let paths = [vec![Rect::new(20.0, 20.0, 60.0, 40.0).to_path(0.1)]];
        let state = ParaState { opacity: 0.5, offset: Vec2::new(10.0, 10.0), ..Default::default() };
        let image = render(&rctx, &layout, &paths, state);
        for (x, y, color) in [(40, 35, [255, 0, 0]), (40, 65, [0, 0, 255]), (29, 35, [0, 255, 0])] {
            let pixel = image.pixel(x, y);
            assert_eq!(&pixel[..3], &color);
            assert!((127..=128).contains(&pixel[3]), "{pixel:?}");
        }
        assert_eq!(image.pixel(20, 35), [0; 4]);
        let hidden = render(&rctx, &layout, &paths, ParaState { visible: false, ..state });
        assert!(hidden.pixels.iter().all(|p| *p == 0));
    }

    #[test]
    fn prepared_art_text_groups_gradient_fragments_in_the_new_ink_frame() {
        let pres = SlideFile::default();
        let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
        let mut layout = layout(&rctx);
        let gradient = Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(255, 0, 0)) },
                GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0, 0, 255)) },
            ],
            shape: GradientShape::Linear { angle: 0.0, scaled: false },
            tile_rect: None,
            rotate_with_shape: true,
        };
        let mut left = layout.runs[0].clone();
        left.gradient = Some(gradient);
        left.chars = (0, 1);
        let mut right = left.clone();
        right.chars = (1, 2);
        layout.runs = vec![left, right];
        let paths = [vec![Rect::new(20.0, 20.0, 60.0, 60.0).to_path(0.1)], vec![Rect::new(60.0, 20.0, 100.0, 60.0).to_path(0.1)]];
        let image = render(&rctx, &layout, &paths, ParaState::default());
        let left = image.pixel(25, 40);
        let right = image.pixel(95, 40);
        assert!(u32::from(left[0]) > u32::from(left[2]) * 5 && u32::from(right[2]) > u32::from(right[0]) * 5);
        assert_eq!((left[3], right[3]), (255, 255));
    }

    #[test]
    fn prepared_art_text_synthetic_bold_only_expands_visible_fill() {
        let pres = SlideFile::default();
        let rctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
        let mut layout = layout(&rctx);
        let paths = [vec![Rect::new(20.0, 20.0, 60.0, 40.0).to_path(0.1)]];
        layout.runs[0].fake_bold = false;
        let plain = render(&rctx, &layout, &paths, ParaState::default());
        layout.runs[0].fake_bold = true;
        let bold = render(&rctx, &layout, &paths, ParaState::default());
        assert_eq!(plain.pixel(19, 30), [0; 4]);
        assert!(bold.pixel(19, 30)[3] > 0);
        layout.runs[0].color = Rgba::TRANSPARENT;
        assert!(render(&rctx, &layout, &paths, ParaState::default()).pixels.iter().all(|p| *p == 0));
    }
}
