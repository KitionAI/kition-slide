//! Effects authored on marker properties, independent of the series line.

use super::{Env, Pal, draw_marker, marker_geometry};
use kition_slide_color::Rgba;
use kition_slide_model::chart::Series;
use kition_slide_model::style::Fill;
use kurbo::{Affine, Point, Shape as _, Vec2};
use vello_common::filter_effects::{EdgeMode, Filter, FilterPrimitive};
use vello_cpu::RenderContext;

fn visible_fill(e: &Env, fill: Option<&Fill>, fallback: Rgba) -> bool {
    match fill {
        None => fallback.a > 0,
        Some(Fill::None | Fill::Group | Fill::Background) => false,
        Some(Fill::Solid { color }) => e.pal.col(color).a > 0,
        Some(Fill::Gradient(g)) => g.stops.iter().any(|s| e.pal.col(&s.color).a > 0),
        Some(Fill::Pattern(p)) => e.pal.col(&p.fg).a > 0 || e.pal.col(&p.bg).a > 0,
        Some(Fill::Picture(p)) => p.alpha.is_none_or(|a| a > 0.0),
    }
}

/// Paint only the shadow; callers draw marker and connecting-line foreground afterwards.
pub(super) fn shadow(ctx: &mut RenderContext, e: &Env, s: &Series, col: Rgba, symbol: &str, size: f64, p: Point) {
    let Some(effect) = s.fmt.marker_effects.as_ref().and_then(|fx| fx.outer_shadow.as_ref()) else { return };
    let color = e.pal.col(&effect.color);
    if symbol == "none" || color.a == 0 || effect.sx.abs() <= 1e-6 || effect.sy.abs() <= 1e-6 {
        return;
    }
    let (path, _) = marker_geometry(e, s, symbol, size, p);
    let bounds = path.bounding_box();
    let align = if effect.align.is_empty() { "b" } else { &effect.align };
    let anchor = Point::new(
        if align.ends_with('l') {
            bounds.x0
        } else if align.ends_with('r') {
            bounds.x1
        } else {
            bounds.center().x
        },
        if align.starts_with('t') {
            bounds.y0
        } else if align.starts_with('b') {
            bounds.y1
        } else {
            bounds.center().y
        },
    )
    .to_vec2();
    let offset = Vec2::new(effect.dir.to_radians().cos(), effect.dir.to_radians().sin()) * effect.dist;
    let local = Affine::translate(anchor)
        * Affine::new([effect.sx, effect.ky.to_radians().tan(), effect.kx.to_radians().tan(), effect.sy, 0.0, 0.0])
        * Affine::translate(-anchor);
    let transform = e.m * Affine::translate(offset) * local;
    let scale = crate::scale_of(e.m);
    let width = s.fmt.marker_line.as_ref().and_then(|line| line.width).unwrap_or(0.75);
    let extent = if width.is_finite() { width.max(0.0) / 2.0 } else { 0.0 };
    let device_bounds = transform.transform_rect_bbox(bounds.inflate(extent, extent)).inflate(effect.blur * scale * 2.0, effect.blur * scale * 2.0);
    // Preserve the chart shadow guard against filtered layers outside vello's tile range.
    if !(device_bounds.is_finite()
        && device_bounds.x0 > -30_000.0
        && device_bounds.y0 > -30_000.0
        && device_bounds.x1 < 30_000.0
        && device_bounds.y1 < 30_000.0)
    {
        return;
    }
    let filter = Filter::from_primitive(FilterPrimitive::DropShadowOnly {
        dx: 0.0,
        dy: 0.0,
        std_deviation: (effect.blur * 0.5 * scale).clamp(0.0, 500.0) as f32,
        color: crate::color(color, 1.0),
        edge_mode: EdgeMode::None,
    });
    let shadow_env =
        Env { rctx: e.rctx, c: e.c, m: transform, pal: Pal { rctx: e.pal.rctx, scheme: e.pal.scheme }, base: e.base.clone(), size: e.size };
    ctx.set_transform(Affine::IDENTITY);
    ctx.push_layer(None, None, None, None, Some(filter));
    // Use the actual combined fill/outline alpha once. This keeps border extents,
    // compound gaps, dashes and varying paint opacity without a binary silhouette.
    // DropShadowOnly discards source colors and does not composite foreground here.
    draw_marker(ctx, &shadow_env, s, col, symbol, size, p);
    ctx.pop_layer();
}

/// Paint only marker effects; the caller paints connecting lines and marker foreground next.
pub(super) fn below(ctx: &mut RenderContext, e: &Env, s: &Series, col: Rgba, symbol: &str, size: f64, p: Point) {
    shadow(ctx, e, s, col, symbol, size, p);
    let Some(glow) = s.fmt.marker_effects.as_ref().and_then(|fx| fx.glow.as_ref()) else { return };
    if symbol == "none" || !glow.radius.is_finite() || glow.radius <= 0.0 {
        return;
    }
    let (path, stroked) = marker_geometry(e, s, symbol, size, p);
    let outline_only = if !stroked && visible_fill(e, s.fmt.marker_fill.as_ref(), col) {
        None
    } else {
        let line = s.fmt.marker_line.as_ref();
        let width = line.and_then(|l| l.width).unwrap_or(0.75);
        if !width.is_finite() || width <= 0.0 || !visible_fill(e, line.and_then(|l| l.fill.as_ref()), col) {
            return;
        }
        Some(width)
    };
    crate::shape_glow::draw(ctx, &path, e.m, e.pal.col(&glow.color), glow.radius, outline_only, false);
}
