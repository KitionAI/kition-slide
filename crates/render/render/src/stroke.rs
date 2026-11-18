//! Stroke expansion with shared zero-length cap geometry.

use kurbo::{BezPath, Stroke, StrokeOpts};
use vello_cpu::{RenderContext, peniko::Fill};

pub(crate) fn outline(path: &BezPath, style: &Stroke, tolerance: f64) -> BezPath {
    let mut outline = kurbo::stroke(path.iter(), style, &StrokeOpts::default(), tolerance);
    if let Some(caps) = kition_slide_geom::stroke::zero_length_caps(path, style, tolerance) {
        outline.extend(caps);
    }
    outline
}

pub(crate) fn paint(ctx: &mut RenderContext, path: &BezPath) {
    let transform = ctx.transform().as_coeffs();
    // Match vello_common 0.2's existing native expansion tolerance so adding a
    // zero-length subpath does not retessellate its nonzero neighbors differently.
    let tolerance = 0.25 / transform[0].abs().max(transform[3].abs()).max(1.0);
    let Some(caps) = kition_slide_geom::stroke::zero_length_caps(path, ctx.stroke(), tolerance) else {
        ctx.stroke_path(path);
        return;
    };
    let mut outline = kurbo::stroke(path.iter(), ctx.stroke(), &StrokeOpts::default(), tolerance);
    outline.extend(caps);
    let fill_rule = *ctx.fill_rule();
    ctx.set_fill_rule(Fill::NonZero);
    // One fill composites translucent or overlapping caps exactly once and
    // retains the current paint, transform, clip, mask and blend state.
    ctx.fill_path(&outline);
    ctx.set_fill_rule(fill_rule);
}

#[cfg(test)]
mod tests;
