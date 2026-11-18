//! Shared shape and chart-marker glow kernel.

use kition_slide_color::Rgba;
use kurbo::{Affine, BezPath, Shape as _};
use vello_cpu::RenderContext;

/// Draw a resolved glow beneath its foreground, using the same silhouette rules as shapes; an
/// `even_odd` silhouette (a donut, a frame) keeps its holes, so the glow rings them too.
pub(crate) fn draw(ctx: &mut RenderContext, path: &BezPath, m: Affine, c: Rgba, radius: f64, outline_only: Option<f64>, even_odd: bool) {
    if c.a == 0
        || path.elements().is_empty()
        || !m.is_finite()
        || !radius.is_finite()
        || outline_only.is_some_and(|width| !width.is_finite() || width <= 0.0)
    {
        return;
    }
    let sigma = radius * 0.35 * crate::scale_of(m);
    let stroke = radius.max(0.0) * 1.2 + outline_only.unwrap_or(0.0);
    let bounds = m
        .transform_rect_bbox(path.bounding_box().inflate(stroke / 2.0, stroke / 2.0))
        .inflate(3.0 * sigma.clamp(0.0, 500.0), 3.0 * sigma.clamp(0.0, 500.0));
    if !sigma.is_finite() || !bounds.is_finite() || [bounds.x0, bounds.y0, bounds.x1, bounds.y1].into_iter().any(|v| v.abs() > crate::EFFECT_LIMIT) {
        return;
    }
    ctx.set_transform(Affine::IDENTITY);
    ctx.push_layer(None, None, Some(c.a as f32 / 255.0), None, Some(crate::blur(sigma)));
    ctx.set_transform(m);
    ctx.set_paint(crate::color(c.with_alpha(255), 1.0));
    ctx.set_stroke(kurbo::Stroke::new(stroke).with_join(kurbo::Join::Round));
    crate::stroke::paint(ctx, path);
    if outline_only.is_none() {
        ctx.set_fill_rule(if even_odd { vello_cpu::peniko::Fill::EvenOdd } else { vello_cpu::peniko::Fill::NonZero });
        ctx.fill_path(path);
        ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
    }
    ctx.pop_layer();
}
