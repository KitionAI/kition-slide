//! Text gradients share one ink frame across compatible font fragments on a line.
//! Wrapped lines restart the frame. Prepared paths retain the same paint behavior.

use kition_slide_model::style::{ColorBase, Gradient, GradientShape};
use kition_slide_text::{GlyphRun, TextLayout};
use kurbo::{Affine, BezPath, Rect, Shape as _};
use vello_cpu::{RenderContext, peniko};

use crate::{color, paint, text_effects};

/// Font fallback must not restart a structured fill. Keep distinct fills and lines separate.
pub(super) fn group_end(layout: &TextLayout, start: usize) -> usize {
    let Some(first) = layout.runs.get(start) else { return start };
    if first.gradient.is_none() && first.fill.is_none() {
        return start + 1;
    }
    let index = layout.lines.partition_point(|line| (line.para, line.end) <= (first.para, first.chars.0));
    let Some(line) = layout.lines.get(index).filter(|line| line.para == first.para && line.start <= first.chars.0) else { return start + 1 };
    start
        + 1
        + layout
            .runs
            .get(start + 1..)
            .unwrap_or_default()
            .iter()
            .take_while(|run| {
                run.para == first.para
                    && run.chars.0 >= line.start
                    && run.chars.1 <= line.end
                    && run.fill == first.fill
                    && run.gradient == first.gradient
                    && run.alpha == first.alpha
            })
            .count()
}

fn valid(g: &Gradient) -> bool {
    g.stops.iter().all(|s| s.pos.is_finite())
        && match &g.shape {
            GradientShape::Linear { angle, .. } => angle.is_finite(),
            GradientShape::Path { focus, .. } => focus.iter().all(|v| v.is_finite()),
        }
}

fn stops(run: &GlyphRun, opacity: f64) -> Vec<(f32, peniko::Color)> {
    run.gradient
        .iter()
        .flat_map(|g| &g.stops)
        .map(|s| {
            // make_style resolves all theme colours before the renderer sees a run.
            let c = match s.color.base {
                ColorBase::Rgb { rgb } => rgb,
                _ => run.color,
            };
            (s.pos.clamp(0.0, 1.0) as f32, color(c, opacity * run.alpha))
        })
        .collect()
}

fn fits(bounds: Rect, m: Affine, padding: f64) -> bool {
    let device = m.transform_rect_bbox(bounds.inflate(padding, padding));
    [device.x0, device.y0, device.x1, device.y1].into_iter().all(|v| v.is_finite() && v.abs() <= crate::EFFECT_LIMIT)
}

pub(super) fn draw(ctx: &mut RenderContext, runs: &[GlyphRun], m: Affine, opacity: f64) {
    let Some(first) = runs.first() else { return };
    let Some(g) = &first.gradient else { return };
    if !m.is_finite() || !opacity.is_finite() || !valid(g) {
        return;
    }
    let paths: Vec<BezPath> = runs.iter().map(text_effects::silhouette).collect();
    draw_paths(ctx, runs, &paths, m, opacity);
}

/// Prepared paths correspond one-to-one to runs and use shape-local point coordinates.
pub(super) fn draw_paths(ctx: &mut RenderContext, runs: &[GlyphRun], paths: &[BezPath], m: Affine, opacity: f64) {
    if runs.len() != paths.len() {
        return;
    }
    let Some(first) = runs.first() else { return };
    let Some(g) = &first.gradient else { return };
    if !m.is_finite() || !opacity.is_finite() || !valid(g) {
        return;
    }
    let bounds = paths.iter().filter(|p| !p.is_empty() && p.is_finite()).map(|p| p.bounding_box()).reduce(|a, b| a.union(b));
    let Some(bounds) = bounds.filter(|b| b.is_finite()) else { return };
    if !fits(bounds, m, 0.0) {
        return;
    }
    for (run, path) in runs.iter().zip(paths) {
        if path.is_empty() || !path.is_finite() {
            continue;
        }
        if run.alpha.is_finite() && run.alpha > 0.0 {
            paint::fill_gradient(ctx, g, stops(run, opacity), path, bounds, m);
            // Match the solid foreground's separate fill and synthetic-bold stroke passes.
            if run.fake_bold && run.size.is_finite() && run.size > 0.0 && run.size <= 4000.0 && fits(bounds, m, run.size * 0.025 / 2.0) {
                let sk = kurbo::Stroke::new(run.size * 0.025).with_join(kurbo::Join::Round);
                let outline = crate::stroke::outline(path, &sk, 0.05);
                paint::fill_gradient(ctx, g, stops(run, opacity), &outline, bounds, m);
            }
        }
        if let Some((c, width)) = run.outline
            && width.is_finite()
            && width > 0.0
            && fits(bounds, m, width / 2.0)
        {
            ctx.set_transform(m);
            ctx.set_paint(color(c, opacity));
            ctx.set_stroke(kurbo::Stroke::new(width).with_join(kurbo::Join::Round));
            crate::stroke::paint(ctx, path);
        }
    }
}

#[cfg(test)]
mod tests;
