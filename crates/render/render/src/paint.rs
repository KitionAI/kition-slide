//! Fills: solid, gradient, picture and pattern paints.

mod gradient_stops;
mod linear_gradient;
mod path_gradient;
mod picture_colors;
mod picture_frame;

use kition_slide_color::Rgba;
use kition_slide_geom::preset::FillMode;
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{Fill, Gradient, GradientShape, PictureFill, PictureMode};
use kition_slide_model::{PhType, SlideFile};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use vello_cpu::{RenderContext, peniko};

use crate::{color, images, shade};

#[allow(clippy::too_many_arguments)]
pub fn fill_path(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    fill: &Fill,
    ph: Option<Rgba>,
    path: &BezPath,
    bounds: Rect,
    m: Affine,
    mode: FillMode,
    alpha: f64,
    _pres: &SlideFile,
) {
    ctx.set_transform(m);
    match fill {
        Fill::None | Fill::Group | Fill::Background => {}
        Fill::Solid { color: c } => {
            let col = shade(rctx.color(c, ph), mode);
            ctx.set_paint(color(col, alpha));
            ctx.fill_path(path);
        }
        Fill::Gradient(g) => {
            let stops = g.stops.iter().map(|s| (s.pos.clamp(0.0, 1.0) as f32, color(shade(rctx.color(&s.color, ph), mode), alpha))).collect();
            fill_gradient(ctx, g, stops, path, bounds, m);
        }
        Fill::Picture(pf) => {
            ctx.push_clip_layer(path);
            picture(ctx, rctx, pf, ph, bounds, m, None);
            ctx.pop_layer();
        }
        Fill::Pattern(p) => {
            let fg = rctx.color(&p.fg, ph);
            let bg = rctx.color(&p.bg, ph);
            match pattern_paint(&p.preset, fg, bg, alpha, m) {
                Some((paint, at)) => {
                    ctx.set_paint_transform(at);
                    ctx.set_paint(paint);
                    ctx.fill_path(path);
                    ctx.reset_paint_transform();
                }
                None => {
                    ctx.set_paint(color(bg, alpha));
                    ctx.fill_path(path);
                }
            }
        }
    }
}

/// Fill an outline with already-resolved colors in a shared local gradient frame.
/// Text uses the same paint geometry and interpolation as shape fills.
pub(crate) fn fill_gradient(ctx: &mut RenderContext, g: &Gradient, mut stops: Vec<(f32, peniko::Color)>, path: &BezPath, bounds: Rect, m: Affine) {
    if stops.is_empty() {
        return;
    }
    ctx.set_transform(m);
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let stops = gradient_stops::expand(stops);
    let stops: Vec<peniko::ColorStop> = stops.into_iter().map(|(o, c)| peniko::ColorStop { offset: o, color: c.into() }).collect();
    if let Some(at) = path_gradient::ellipse_transform(&g.shape, path, bounds) {
        ctx.set_paint_transform(at);
        ctx.set_paint(peniko::Gradient::new_radial(Point::ZERO, 1.0).with_stops(stops.as_slice()));
        ctx.fill_path(path);
        ctx.reset_paint_transform();
        return;
    }
    let grad = match &g.shape {
        GradientShape::Linear { angle, scaled } => {
            let (start, end) = linear_gradient::endpoints(*angle, *scaled, bounds);
            peniko::Gradient::new_linear(start, end).with_stops(stops.as_slice())
        }
        GradientShape::Path { focus, .. } => {
            let fx = bounds.x0 + bounds.width() * (focus[0] + (1.0 - focus[2])) / 2.0;
            let fy = bounds.y0 + bounds.height() * (focus[1] + (1.0 - focus[3])) / 2.0;
            // Reach the farthest corner, so a gradient from a corner covers the whole box.
            let r = [(bounds.x0, bounds.y0), (bounds.x1, bounds.y0), (bounds.x0, bounds.y1), (bounds.x1, bounds.y1)]
                .iter()
                .map(|(x, y)| ((x - fx).powi(2) + (y - fy).powi(2)).sqrt())
                .fold(1e-6, f64::max);
            peniko::Gradient::new_radial(Point::new(fx, fy), r as f32).with_stops(stops.as_slice())
        }
    };
    ctx.set_paint(grad);
    ctx.fill_path(path);
}

/// The 8x8 cell of each preset pattern (`ST_PresetPatternVal`), top row first, most significant bit
/// leftmost, 1 = foreground. The cells are anchored at the output's origin, one bit per pixel at
/// 96 dpi, as the reference renders draw them (several cells measured from them).
fn pattern_cell(preset: &str) -> [u8; 8] {
    let rep2 = |a: u8, b: u8| [a, b, a, b, a, b, a, b];
    let rep4 = |a: u8, b: u8, c: u8, d: u8| [a, b, c, d, a, b, c, d];
    match preset {
        "pct5" => [0, 0, 0, 0x10, 0, 0, 0, 0x01],
        "pct10" => [0x80, 0, 0x08, 0, 0x80, 0, 0x08, 0],
        "pct20" => rep4(0x88, 0, 0x22, 0),
        "pct25" => rep4(0x88, 0x22, 0x88, 0x22),
        "pct30" => rep4(0xAA, 0x44, 0xAA, 0x11),
        "pct40" => [0xA8, 0x55, 0xAA, 0x55, 0x8A, 0x55, 0xAA, 0x55],
        "pct50" => rep2(0xAA, 0x55),
        "pct60" => rep4(0xEE, 0x55, 0xBB, 0x55),
        "pct70" => rep4(0x55, 0xBB, 0x55, 0xEE),
        "pct75" => rep4(0xEE, 0xFF, 0xBB, 0xFF),
        "pct80" => rep4(0xF7, 0xFF, 0x7F, 0xFF),
        "pct90" => [0xFE, 0xFF, 0xFF, 0xFF, 0xEF, 0xFF, 0xFF, 0xFF],
        "horz" => [0xFF, 0, 0, 0, 0, 0, 0, 0],
        "ltHorz" => rep4(0xFF, 0, 0, 0),
        "narHorz" => rep2(0xFF, 0),
        "dkHorz" => rep4(0xFF, 0xFF, 0, 0),
        "vert" => [0x80; 8],
        "ltVert" => [0x88; 8],
        "narVert" => [0xAA; 8],
        "dkVert" => [0xCC; 8],
        "dnDiag" => [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01],
        "upDiag" => [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
        "ltDnDiag" => rep4(0x88, 0x44, 0x22, 0x11),
        "ltUpDiag" => rep4(0x11, 0x22, 0x44, 0x88),
        "dkDnDiag" => rep4(0x99, 0xCC, 0x66, 0x33),
        "dkUpDiag" => rep4(0x99, 0x33, 0x66, 0xCC),
        "wdDnDiag" => [0xC1, 0xE0, 0x70, 0x38, 0x1C, 0x0E, 0x07, 0x83],
        "wdUpDiag" => [0x83, 0x07, 0x0E, 0x1C, 0x38, 0x70, 0xE0, 0xC1],
        "dashDnDiag" => [0, 0, 0x88, 0x44, 0x22, 0x11, 0, 0],
        "dashUpDiag" => [0, 0, 0x11, 0x22, 0x44, 0x88, 0, 0],
        "dashHorz" => [0xF0, 0, 0, 0, 0x0F, 0, 0, 0],
        "dashVert" => [0x80, 0x80, 0x80, 0x80, 0x08, 0x08, 0x08, 0x08],
        "smConfetti" => [0x80, 0x08, 0x40, 0x02, 0x10, 0x01, 0x20, 0x04],
        "lgConfetti" => [0xB1, 0x30, 0x03, 0x1B, 0xD8, 0xC0, 0x0C, 0x8D],
        "zigZag" => rep4(0x81, 0x42, 0x24, 0x18),
        "wave" => [0, 0x18, 0xA4, 0x03, 0, 0x18, 0xA4, 0x03],
        "diagBrick" => [0x80, 0x40, 0x20, 0x10, 0x18, 0x24, 0x42, 0x81],
        "horzBrick" => [0xFF, 0x80, 0x80, 0x80, 0xFF, 0x08, 0x08, 0x08],
        "weave" => [0x88, 0x54, 0x22, 0x45, 0x88, 0x14, 0x22, 0x51],
        "plaid" => [0xAA, 0x55, 0xAA, 0x55, 0xF0, 0xF0, 0xF0, 0xF0],
        "divot" => [0, 0x10, 0x08, 0x10, 0, 0x01, 0x80, 0x01],
        "dotGrid" => [0xAA, 0, 0x80, 0, 0x80, 0, 0x80, 0],
        "dotDmnd" => [0x80, 0, 0x22, 0, 0x08, 0, 0x22, 0],
        "shingle" => [0x03, 0x84, 0x48, 0x30, 0x0C, 0x02, 0x01, 0x01],
        "trellis" => rep4(0xFF, 0x66, 0xFF, 0x99),
        "sphere" => [0x77, 0x89, 0x8F, 0x8F, 0x77, 0x98, 0xF8, 0xF8],
        "smGrid" => rep4(0xFF, 0x88, 0x88, 0x88),
        "lgGrid" | "cross" => [0xFF, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        "smCheck" => rep4(0xCC, 0xCC, 0x33, 0x33),
        "lgCheck" => [0xF0, 0xF0, 0xF0, 0xF0, 0x0F, 0x0F, 0x0F, 0x0F],
        "openDmnd" => [0x80, 0x41, 0x22, 0x14, 0x08, 0x14, 0x22, 0x41],
        "solidDmnd" => [0x08, 0x1C, 0x3E, 0x7F, 0x3E, 0x1C, 0x08, 0],
        "diagCross" => [0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81],
        _ => rep2(0xAA, 0x55),
    }
}

/// A repeating image paint of `preset` in `fg` on `bg`, and the paint transform that pins its cells
/// to output pixels for a path drawn with `m` (`None` when `m` cannot be inverted).
fn pattern_paint(preset: &str, fg: Rgba, bg: Rgba, alpha: f64, m: Affine) -> Option<(vello_cpu::Image, Affine)> {
    if m.determinant().abs() < 1e-12 || !m.determinant().is_finite() {
        return None;
    }
    let premul = |c: Rgba| {
        let a = (c.a as f64 * alpha.clamp(0.0, 1.0)).round() as u16;
        let k = |v: u8| ((v as u16 * a + 127) / 255) as u8;
        vello_cpu::color::PremulRgba8 { r: k(c.r), g: k(c.g), b: k(c.b), a: a as u8 }
    };
    let (f, b) = (premul(fg), premul(bg));
    let cell = pattern_cell(preset);
    let data: Vec<_> = (0..64).map(|i| if cell[i / 8] & (0x80 >> (i % 8)) != 0 { f } else { b }).collect();
    let pm = std::sync::Arc::new(vello_cpu::Pixmap::from_parts(data, 8, 8));
    // One cell bit per 96-dpi pixel: a device pixel at the usual 4/3 px per pt, more when zoomed in.
    let k = (crate::scale_of(m) * 0.75).round().clamp(1.0, 64.0);
    let image = vello_cpu::Image {
        image: vello_cpu::ImageSource::Pixmap(pm),
        sampler: peniko::ImageSampler {
            quality: peniko::ImageQuality::Low,
            x_extend: peniko::Extend::Repeat,
            y_extend: peniko::Extend::Repeat,
            ..Default::default()
        },
    };
    Some((image, m.inverse() * Affine::scale(k)))
}

/// Select a picture's frame once for all of the containing shape's clipped subpaths.
pub fn picture_shape_frame(pf: &PictureFill, bounds: Rect, m: Affine, shape: &kition_slide_geom::Xfrm, outline: &BezPath) -> (Rect, Affine) {
    if pf.rotate_with_shape { (bounds, m) } else { picture_frame::fixed(bounds, outline, m, shape).unwrap_or((bounds, m)) }
}

/// A fixed picture fill retains its parent group's transform while the shape's own
/// rotation changes only its clip and fill region.
#[allow(clippy::too_many_arguments)]
pub fn picture_for_shape(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    pf: &PictureFill,
    bounds: Rect,
    m: Affine,
    shape: &kition_slide_geom::Xfrm,
    outline: &BezPath,
    gif_time: Option<f64>,
) {
    let (bounds, m) = picture_shape_frame(pf, bounds, m, shape, outline);
    ctx.set_transform(m);
    picture(ctx, rctx, pf, None, bounds, m, gif_time);
}

/// Draw a picture fill into `bounds` (shape-local) honouring crop and stretch/tile modes; an animated
/// GIF shows its frame at `gif_time` seconds.
pub fn picture(ctx: &mut RenderContext, rctx: &Ctx, pf: &PictureFill, ph: Option<Rgba>, bounds: Rect, m: Affine, gif_time: Option<f64>) {
    let Some(item) = rctx.pres.media(pf.media) else {
        missing(ctx, bounds);
        return;
    };
    let px_scale = crate::scale_of(m);
    let want = (bounds.width().max(bounds.height()) * px_scale).max(1.0);
    let adjust = picture_colors::resolve(rctx, &pf.adjust, ph);
    let frame = gif_time.and_then(|t| images::gif_pixmap(&item.data, &adjust, t));
    let Some(pm) = frame.or_else(|| images::decode_for(&item.data, want, &adjust)) else {
        missing(ctx, bounds);
        return;
    };
    let (iw, ih) = (pm.width() as f64, pm.height() as f64);
    if iw < 1.0 || ih < 1.0 {
        return;
    }
    let dest = match &pf.mode {
        PictureMode::Stretch { fill_rect } => {
            let [l, t, r, b] = *fill_rect;
            let inner = Rect::new(
                bounds.x0 + l * bounds.width(),
                bounds.y0 + t * bounds.height(),
                bounds.x1 - r * bounds.width(),
                bounds.y1 - b * bounds.height(),
            );
            crate::crop_dest(inner, pf.crop)
        }
        PictureMode::Tile { .. } => bounds,
    };
    let alpha = pf.alpha.unwrap_or(1.0).clamp(0.0, 1.0);
    let image = vello_cpu::Image {
        image: vello_cpu::ImageSource::Pixmap(pm.clone()),
        sampler: peniko::ImageSampler { quality: peniko::ImageQuality::High, ..Default::default() },
    };
    if let PictureMode::Tile { sx, sy, tx, ty, flip, align } = &pf.mode {
        // One tile is the picture at its natural size times the scale, placed by the alignment and
        // offset, repeated (mirrored every other tile when flipped) over the bounds.
        let (nw, nh) = images::natural_size(&item.data).unwrap_or((iw, ih));
        let k = |v: f64| if v.is_finite() && v.abs() > 1e-3 { v.abs() } else { 1.0 };
        // Whole output pixels per tile, as the reference renders repeat them.
        let px = crate::scale_of(m);
        let whole = |v: f64| if px > 1e-9 && px.is_finite() { (v * px).round().max(1.0) / px } else { v };
        let (tw, th) = (whole((nw * k(*sx)).max(0.01)), whole((nh * k(*sy)).max(0.01)));
        let ax = match align.as_str() {
            "t" | "ctr" | "b" => (bounds.width() - tw) / 2.0,
            "tr" | "r" | "br" => bounds.width() - tw,
            _ => 0.0,
        };
        let ay = match align.as_str() {
            "l" | "ctr" | "r" => (bounds.height() - th) / 2.0,
            "bl" | "b" | "br" => bounds.height() - th,
            _ => 0.0,
        };
        let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
        let origin = Point::new(bounds.x0 + ax + finite(*tx), bounds.y0 + ay + finite(*ty));
        let extend = |mirror: bool| if mirror { peniko::Extend::Reflect } else { peniko::Extend::Repeat };
        let tiled = vello_cpu::Image {
            image: vello_cpu::ImageSource::Pixmap(pm.clone()),
            // Bilinear, as the reference renders resample tiles.
            sampler: peniko::ImageSampler {
                quality: peniko::ImageQuality::Medium,
                x_extend: extend(matches!(flip.as_str(), "x" | "xy")),
                y_extend: extend(matches!(flip.as_str(), "y" | "xy")),
                ..Default::default()
            },
        };
        ctx.set_transform(m);
        ctx.set_paint_transform(Affine::translate(origin.to_vec2()) * Affine::scale_non_uniform(tw / iw, th / ih));
        if alpha < 0.999 {
            ctx.push_layer(None, None, Some(alpha as f32), None, None);
        }
        ctx.set_paint(tiled);
        ctx.fill_rect(&bounds);
        if alpha < 0.999 {
            ctx.pop_layer();
        }
        ctx.reset_paint_transform();
        return;
    }
    draw_image(ctx, &image, iw, ih, dest, m, alpha);
}

fn draw_image(ctx: &mut RenderContext, image: &vello_cpu::Image, iw: f64, ih: f64, dest: Rect, m: Affine, alpha: f64) {
    let t = m * Affine::translate(dest.origin().to_vec2()) * Affine::scale_non_uniform(dest.width() / iw, dest.height() / ih);
    ctx.set_transform(t);
    if alpha < 0.999 {
        ctx.push_layer(None, None, Some(alpha as f32), None, None);
    }
    ctx.set_paint(image.clone());
    ctx.fill_rect(&Rect::new(0.0, 0.0, iw, ih));
    if alpha < 0.999 {
        ctx.pop_layer();
    }
}

fn missing(ctx: &mut RenderContext, b: Rect) {
    ctx.set_paint(peniko::Color::from_rgba8(230, 230, 230, 255));
    ctx.fill_rect(&b);
    ctx.set_paint(peniko::Color::from_rgba8(160, 160, 160, 255));
    ctx.set_stroke(kurbo::Stroke::new(1.0));
    let mut p = BezPath::new();
    p.move_to((b.x0, b.y0));
    p.line_to((b.x1, b.y1));
    p.move_to((b.x1, b.y0));
    p.line_to((b.x0, b.y1));
    crate::stroke::paint(ctx, &p);
}

/// The speaker / film glyph on audio and video objects without a poster.
pub fn media_glyph(ctx: &mut RenderContext, video: bool, b: Rect) {
    let s = b.width().min(b.height());
    let c = b.center();
    if video {
        let r = s * 0.18;
        ctx.set_paint(peniko::Color::from_rgba8(255, 255, 255, 220));
        ctx.fill_path(&kurbo::Circle::new(c, r * 1.4).to_path(0.1));
        let mut tri = BezPath::new();
        tri.move_to((c.x - r * 0.45, c.y - r * 0.7));
        tri.line_to((c.x + r * 0.75, c.y));
        tri.line_to((c.x - r * 0.45, c.y + r * 0.7));
        tri.close_path();
        ctx.set_paint(peniko::Color::from_rgba8(30, 30, 30, 255));
        ctx.fill_path(&tri);
        return;
    }
    let k = s / 48.0;
    let mut p = BezPath::new();
    p.move_to((c.x - 14.0 * k, c.y - 6.0 * k));
    p.line_to((c.x - 6.0 * k, c.y - 6.0 * k));
    p.line_to((c.x + 4.0 * k, c.y - 15.0 * k));
    p.line_to((c.x + 4.0 * k, c.y + 15.0 * k));
    p.line_to((c.x - 6.0 * k, c.y + 6.0 * k));
    p.line_to((c.x - 14.0 * k, c.y + 6.0 * k));
    p.close_path();
    ctx.set_paint(peniko::Color::from_rgba8(0x2E, 0x6F, 0xD8, 255));
    ctx.fill_path(&p);
    let mut w = BezPath::new();
    for r in [9.0, 15.0] {
        let arc = kurbo::Arc::new((c.x + 4.0 * k, c.y), (r * k, r * k), -0.8, 1.6, 0.0);
        w.extend(arc.path_elements(0.1));
    }
    ctx.set_stroke(kurbo::Stroke::new(2.5 * k).with_caps(kurbo::Cap::Round));
    crate::stroke::paint(ctx, &w);
}

/// A content-type icon drawn in empty picture/chart/table placeholders.
pub fn placeholder_icon(ctx: &mut RenderContext, kind: PhType, b: Rect) {
    let s = (b.width().min(b.height()) * 0.18).clamp(10.0, 48.0);
    let c = Point::new(b.center().x, b.center().y - s * 0.3);
    let r = Rect::new(c.x - s / 2.0, c.y - s / 2.0, c.x + s / 2.0, c.y + s / 2.0);
    ctx.set_paint(peniko::Color::from_rgba8(0x8C, 0x8C, 0x8C, 255));
    ctx.set_stroke(kurbo::Stroke::new(s * 0.06).with_join(kurbo::Join::Round));
    match kind {
        PhType::Chart => {
            for (i, h) in [0.45, 0.8, 0.6].iter().enumerate() {
                let x = r.x0 + s * (0.1 + 0.3 * i as f64);
                ctx.fill_rect(&Rect::new(x, r.y1 - s * h, x + s * 0.2, r.y1));
            }
        }
        PhType::Table => {
            crate::stroke::paint(ctx, &r.to_path(0.1));
            let mut p = BezPath::new();
            for k in 1..3 {
                let x = r.x0 + s * k as f64 / 3.0;
                let y = r.y0 + s * k as f64 / 3.0;
                p.move_to((x, r.y0));
                p.line_to((x, r.y1));
                p.move_to((r.x0, y));
                p.line_to((r.x1, y));
            }
            crate::stroke::paint(ctx, &p);
        }
        _ => {
            crate::stroke::paint(ctx, &r.to_path(0.1));
            let mut m = BezPath::new();
            m.move_to((r.x0, r.y1));
            m.line_to((r.x0 + s * 0.35, r.y0 + s * 0.45));
            m.line_to((r.x0 + s * 0.6, r.y0 + s * 0.7));
            m.line_to((r.x0 + s * 0.75, r.y0 + s * 0.55));
            m.line_to((r.x1, r.y1));
            m.close_path();
            ctx.fill_path(&m);
            ctx.fill_path(&kurbo::Circle::new((r.x0 + s * 0.72, r.y0 + s * 0.25), s * 0.1).to_path(0.1));
        }
    }
}

#[cfg(test)]
pub(crate) fn pattern_cell_for_tests(preset: &str) -> [u8; 8] {
    pattern_cell(preset)
}
