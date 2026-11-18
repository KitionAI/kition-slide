//! Run-level art text effects, painted from the same glyph outlines as the foreground.

use kition_slide_color::Rgba;
use kition_slide_text::GlyphRun;
use kurbo::{Affine, BezPath, Point, Rect, Shape as _, Vec2};
use vello_cpu::RenderContext;

use crate::{EFFECT_LIMIT, blur, color, glyph, scale_of};

/// Build one silhouette per run so overlapping glyphs do not apply effect opacity repeatedly.
pub(super) fn silhouette(run: &GlyphRun) -> BezPath {
    let mut path = BezPath::new();
    let k = run.size / run.face.upem.max(1.0);
    let upright = if run.upright { Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { Affine::IDENTITY };
    let skew = if run.fake_italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
    for &(gid, x, y) in &run.glyphs {
        let mut p = (*glyph(&run.face, gid)).clone();
        p.apply_affine(Affine::translate((x, y)) * upright * skew * Affine::scale(k));
        path.extend(p);
    }
    path
}

fn anchor(bounds: Rect, align: &str) -> Point {
    let x = match align {
        "tl" | "l" | "bl" => bounds.x0,
        "tr" | "r" | "br" => bounds.x1,
        _ => bounds.center().x,
    };
    let y = match align {
        "tl" | "t" | "tr" => bounds.y0,
        "bl" | "b" | "br" => bounds.y1,
        _ => bounds.center().y,
    };
    Point::new(x, y)
}

/// An identical stationary silhouette is completely covered by the opaque foreground.
/// Painting it again would only increase antialiased edge coverage.
fn hidden_shadow(run: &GlyphRun, shadow: &kition_slide_model::style::Shadow, c: Rgba) -> bool {
    run.alpha == 1.0
        && run.color.a == 255
        && run.gradient.is_none()
        && run.fill.is_none()
        && run.outline.is_none_or(|(color, _)| color.a == 0)
        && (c.r, c.g, c.b) == (run.color.r, run.color.g, run.color.b)
        && shadow.blur == 0.0
        && shadow.dist == 0.0
        && shadow.sx == 1.0
        && shadow.sy == 1.0
        && shadow.kx == 0.0
        && shadow.ky == 0.0
}

/// Reject filter extents outside the backend's finite tile coordinate range.
fn fits(bounds: Rect, m: Affine, sigma: f64, stroke: f64) -> bool {
    if !m.is_finite() || !sigma.is_finite() || !stroke.is_finite() {
        return false;
    }
    let b = m.transform_rect_bbox(bounds.inflate(stroke.max(0.0) / 2.0, stroke.max(0.0) / 2.0)).inflate(3.0 * sigma, 3.0 * sigma);
    [b.x0, b.y0, b.x1, b.y1].into_iter().all(|v| v.is_finite() && v.abs() <= EFFECT_LIMIT)
}

#[allow(clippy::too_many_arguments)]
fn paint(ctx: &mut RenderContext, path: &BezPath, bounds: Rect, m: Affine, c: Rgba, opacity: f64, sigma: f64, stroke: f64, fill: bool) {
    let alpha = opacity * f64::from(c.a) / 255.0;
    if !alpha.is_finite() || alpha <= 0.0 || !fits(bounds, m, sigma, stroke) {
        return;
    }
    ctx.set_transform(Affine::IDENTITY);
    ctx.push_layer(None, None, Some(alpha.clamp(0.0, 1.0) as f32), None, Some(blur(sigma)));
    ctx.set_transform(m);
    ctx.set_paint(color(c.with_alpha(255), 1.0));
    if fill {
        ctx.fill_path(path);
    }
    if stroke > 0.0 {
        ctx.set_stroke(kurbo::Stroke::new(stroke).with_join(kurbo::Join::Round));
        crate::stroke::paint(ctx, path);
    }
    ctx.pop_layer();
}

pub(super) fn draw(ctx: &mut RenderContext, run: &GlyphRun, m: Affine, opacity: f64) {
    if run.shadow.is_none() && run.glow.is_none() {
        return;
    }
    draw_path(ctx, run, &silhouette(run), m, opacity);
}

/// Paint effects from prepared shape-local ink, including warped glyph outlines.
pub(super) fn draw_path(ctx: &mut RenderContext, run: &GlyphRun, path: &BezPath, m: Affine, opacity: f64) {
    let fill_alpha = (run.alpha * f64::from(run.color.a) / 255.0).clamp(0.0, 1.0);
    let outline_alpha = run.outline.map_or(0.0, |(c, _)| f64::from(c.a) / 255.0);
    let opacity = opacity * fill_alpha.max(outline_alpha);
    let k = scale_of(m);
    if !opacity.is_finite() || opacity <= 0.0 || !k.is_finite() || k <= 0.0 {
        return;
    }
    if path.is_empty() || !path.is_finite() {
        return;
    }
    let bounds = path.bounding_box();
    // Fully transparent outlines do not contribute ink to the effect silhouette.
    let stroke = run.outline.filter(|(color, _)| color.a > 0).map_or(0.0, |(_, w)| w).max(if run.fake_bold && fill_alpha > 0.0 {
        run.size * 0.025
    } else {
        0.0
    });
    if let Some((shadow, c)) = &run.shadow
        && !shadow.inner
        && !hidden_shadow(run, shadow, *c)
        && [shadow.blur, shadow.dist, shadow.dir, shadow.sx, shadow.sy, shadow.kx, shadow.ky].into_iter().all(f64::is_finite)
    {
        let center = anchor(bounds, &shadow.align).to_vec2();
        let local = Affine::translate(center)
            * Affine::new([shadow.sx, shadow.ky.to_radians().tan(), shadow.kx.to_radians().tan(), shadow.sy, 0.0, 0.0])
            * Affine::translate(-center);
        let offset = Vec2::new(shadow.dir.to_radians().cos(), shadow.dir.to_radians().sin()) * shadow.dist;
        let sm = if shadow.rotate_with_shape { m * Affine::translate(offset) * local } else { Affine::translate(offset * k) * m * local };
        paint(ctx, path, bounds, sm, *c, opacity, shadow.blur.max(0.0) * 0.5 * k, stroke, fill_alpha > 0.0);
    }
    if let Some((glow, c)) = &run.glow
        && glow.radius.is_finite()
        && glow.radius > 0.0
    {
        paint(ctx, path, bounds, m, *c, opacity, glow.radius * 0.35 * k, glow.radius * 1.2 + stroke, fill_alpha > 0.0);
    }
}

#[cfg(test)]
mod tests;
