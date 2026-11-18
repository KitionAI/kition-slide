//! Picture and pattern text use the shared DrawingML painter inside glyph silhouettes.
//! Compatible fragments share the same line ink frame, including font fallbacks.

use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{Fill, PictureMode};
use kition_slide_text::GlyphRun;
use kurbo::{Affine, BezPath, Rect, Shape as _};
use vello_cpu::RenderContext;

use crate::{color, paint, text_effects};

fn fits(bounds: Rect, m: Affine, padding: f64) -> bool {
    let device = m.transform_rect_bbox(bounds.inflate(padding, padding));
    [device.x0, device.y0, device.x1, device.y1].into_iter().all(|v| v.is_finite() && v.abs() <= crate::EFFECT_LIMIT)
}

fn valid(fill: &Fill) -> bool {
    match fill {
        Fill::Pattern(_) => true,
        Fill::Picture(p) => {
            p.crop.iter().all(|v| v.is_finite())
                && p.alpha.is_none_or(f64::is_finite)
                && match &p.mode {
                    PictureMode::Stretch { fill_rect } => fill_rect.iter().all(|v| v.is_finite()),
                    PictureMode::Tile { tx, ty, sx, sy, .. } => [tx, ty, sx, sy].into_iter().all(|v| v.is_finite()),
                }
        }
        _ => false,
    }
}

pub(super) fn draw(ctx: &mut RenderContext, rctx: &Ctx, runs: &[GlyphRun], m: Affine, opacity: f64) {
    let Some(first) = runs.first() else { return };
    if !first.fill.as_ref().is_some_and(valid) {
        return;
    }
    if !m.is_finite() || !opacity.is_finite() {
        return;
    }
    let paths: Vec<BezPath> = runs.iter().map(text_effects::silhouette).collect();
    draw_paths(ctx, rctx, runs, &paths, m, opacity);
}

/// Prepared paths correspond one-to-one to runs and use shape-local point coordinates.
pub(super) fn draw_paths(ctx: &mut RenderContext, rctx: &Ctx, runs: &[GlyphRun], paths: &[BezPath], m: Affine, opacity: f64) {
    if runs.len() != paths.len() {
        return;
    }
    let Some(first) = runs.first() else { return };
    let Some(fill) = first.fill.as_ref().filter(|fill| valid(fill)) else { return };
    if !m.is_finite() || !opacity.is_finite() {
        return;
    }
    let bounds = paths.iter().filter(|p| !p.is_empty() && p.is_finite()).map(|p| p.bounding_box()).reduce(|a, b| a.union(b));
    let Some(bounds) = bounds.filter(|b| b.is_finite() && b.width() > 0.0 && b.height() > 0.0) else { return };
    if !fits(bounds, m, 0.0) {
        return;
    }
    for (run, path) in runs.iter().zip(paths) {
        if path.is_empty() || !path.is_finite() {
            continue;
        }
        // Missing-image diagnostics do not apply picture opacity. A fully
        // transparent authored fill must not expose that diagnostic as glyph ink.
        let transparent_picture = matches!(fill, Fill::Picture(picture) if picture.alpha.is_some_and(|alpha| alpha <= 0.0));
        if run.alpha.is_finite() && run.alpha > 0.0 && opacity > 0.0 && !transparent_picture {
            let mut fill = fill.clone();
            let alpha = (opacity * run.alpha).clamp(0.0, 1.0);
            // The shared picture painter owns image opacity; other fills use fill_path's alpha.
            if let Fill::Picture(picture) = &mut fill {
                picture.alpha = Some(picture.alpha.unwrap_or(1.0).clamp(0.0, 1.0) * alpha);
            }
            let draw = |ctx: &mut RenderContext, path: &BezPath| {
                paint::fill_path(ctx, rctx, &fill, None, path, bounds, m, kition_slide_geom::preset::FillMode::Norm, alpha, rctx.pres);
            };
            draw(ctx, path);
            if run.fake_bold && run.size.is_finite() && run.size > 0.0 && run.size <= 4000.0 && fits(bounds, m, run.size * 0.025 / 2.0) {
                let stroke = kurbo::Stroke::new(run.size * 0.025).with_join(kurbo::Join::Round);
                draw(ctx, &crate::stroke::outline(path, &stroke, 0.05));
            }
        }
        if let Some((color_value, width)) = run.outline
            && width.is_finite()
            && width > 0.0
            && fits(bounds, m, width / 2.0)
        {
            ctx.set_transform(m);
            ctx.set_paint(color(color_value, opacity));
            ctx.set_stroke(kurbo::Stroke::new(width).with_join(kurbo::Join::Round));
            crate::stroke::paint(ctx, path);
        }
    }
}

#[cfg(test)]
mod tests;
