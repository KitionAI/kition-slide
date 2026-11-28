//! The Kition Slide renderer: draws slides, layouts and masters with `vello_cpu`.
//!
//! One code path serves the editor canvas, thumbnails, image/PDF-raster export and the slide show:
//! [`render_slide`] draws the background, the master and layout graphics, then the slide's shapes
//! (fills with theme shading, gradients, pictures, outlines with dashes and arrowheads, shadows,
//! glow and soft edges, text, tables, charts, groups), optionally with per-shape animation state.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod chart;
mod compound;
mod images;
mod line_end;
mod metafile;
mod morph_path;
mod morph_text;
mod paint;
mod placed;
mod shape_glow;
mod stroke;
mod stroke_align;
mod table;
mod text_effects;
mod text_fill;
mod text_gradient;
mod warp_text;

#[cfg(test)]
mod media_poster_tests;

use std::collections::HashMap;
use std::sync::Arc;

use kition_slide_color::Rgba;
use kition_slide_fonts::FontDb;
use kition_slide_geom::preset::{self, FillMode, Geometry, SubPath};
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::style::{Effects, Fill, Line};
use kition_slide_model::text::TextBody;
use kition_slide_model::{Geom, PhType, Shape, ShapeId, ShapeKind, Slide, SlideFile};
use kition_slide_text::datetime::{self, DateTime};
use kition_slide_text::{Fields, Opts};
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use vello_common::filter_effects::{EdgeMode, Filter, FilterPrimitive};
use vello_cpu::{RenderContext, Resources, peniko};

pub use images::{decode as decode_image, gif_frame};
pub use morph_path::PathMorph;
pub use morph_text::TextMorph;
pub use placed::{Placed, PlacedLink, PlacedText, place_slide};
pub use table::{cell_body as table_cell_body, cell_rects as table_cell_rects, row_heights as table_row_heights};

/// Largest pixmap side we render (vello_cpu uses u16 sizes).
pub const MAX_SIDE: u32 = 16_000;

/// A rendered image: premultiplied RGBA8.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// Straight (unpremultiplied) RGBA.
    pub fn to_straight(&self) -> Vec<u8> {
        let mut out = self.pixels.clone();
        for px in out.as_chunks_mut::<4>().0 {
            let a = px.get(3).copied().unwrap_or(255) as u32;
            if a != 0 && a != 255 {
                for c in px.iter_mut().take(3) {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }
    pub fn to_png(&self) -> Vec<u8> {
        let mut px = self.to_straight();
        px.resize(self.width as usize * self.height as usize * 4, 0);
        let Some(img) = image::RgbaImage::from_raw(self.width, self.height, px) else { return Vec::new() };
        let mut buf = Vec::new();
        if let Err(e) = img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png) {
            log::error!("PNG encode: {e}");
            return Vec::new();
        }
        buf
    }
    pub fn to_jpeg(&self, quality: u8) -> Vec<u8> {
        let straight = self.to_straight();
        let rgb: Vec<u8> = straight.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let Some(img) = image::RgbImage::from_raw(self.width, self.height, rgb) else { return Vec::new() };
        let mut buf = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
        if let Err(e) = img.write_with_encoder(enc) {
            log::error!("JPEG encode: {e}");
            return Vec::new();
        }
        buf
    }
    /// Pixel at (x, y) as straight RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.pixels.get(i..i + 4).unwrap_or(&[0, 0, 0, 0]);
        let a = p.get(3).copied().unwrap_or(0) as u32;
        let un = |c: u8| (c as u32 * 255 + a / 2).checked_div(a).map(|v| v.min(255) as u8).unwrap_or(0);
        [un(p[0]), un(p[1]), un(p[2]), a as u8]
    }
}

/// Animation state of one shape during a slide show or preview.
#[derive(Clone, Debug)]
pub struct ShapeState {
    pub visible: bool,
    pub opacity: f64,
    /// Applied in slide space around the shape's centre: translation (pt), scale, rotation (deg).
    pub offset: Vec2,
    pub scale: (f64, f64),
    pub rotate: f64,
    /// Reveal clip in shape-local unit space (0..1): l, t, r, b.
    pub clip: Option<[f64; 4]>,
    /// Colour override for emphasis effects (fill).
    pub tint: Option<Rgba>,
    /// Per-paragraph states of by-paragraph text builds (paragraph index, state).
    pub paras: Vec<(usize, ParaState)>,
}

impl Default for ShapeState {
    fn default() -> Self {
        ShapeState { visible: true, opacity: 1.0, offset: Vec2::ZERO, scale: (1.0, 1.0), rotate: 0.0, clip: None, tint: None, paras: vec![] }
    }
}

/// Animation state of one paragraph, relative to its shape.
#[derive(Clone, Copy, Debug)]
pub struct ParaState {
    pub visible: bool,
    pub opacity: f64,
    /// Translation in shape-local points.
    pub offset: Vec2,
}

impl Default for ParaState {
    fn default() -> Self {
        ParaState { visible: true, opacity: 1.0, offset: Vec2::ZERO }
    }
}

pub struct RenderOpts<'a> {
    /// Pixels per point.
    pub scale: f64,
    /// Editor view: placeholder prompts and dashed placeholder outlines.
    pub edit: bool,
    /// Draw shapes marked hidden (Selection pane) — the editor hides them like the show does.
    pub show_hidden: bool,
    /// Per-shape animation state.
    pub state: Option<&'a dyn Fn(ShapeId) -> ShapeState>,
    /// Don't draw these shapes (being edited in place, drawn by the UI).
    pub skip: &'a [ShapeId],
    /// Hide the text of this shape (the UI draws it while editing).
    pub skip_text: Option<ShapeId>,
    /// The shape whose text the editor is editing (with `edit`): its empty paragraphs show their
    /// bullets and numbers, and an empty placeholder its bullet instead of its prompt.
    pub editing: Option<ShapeId>,
    /// Animated GIF pictures and how far into their play they are (seconds); others show frame 1.
    pub gif_times: &'a [(ShapeId, f64)],
    /// Worker threads (0 = this thread; required when effects use filters).
    pub threads: u16,
    /// Translate the slide within the output (pixels) and output size override.
    pub offset: (f64, f64),
    pub size: Option<(u32, u32)>,
    /// Draw a background colour outside the slide (output larger than slide).
    pub clear: Option<Rgba>,
    /// Show this text in every date field instead of the date.
    pub date_text: Option<String>,
    /// The local date and time date fields show (`None`: the system clock).
    pub now: Option<DateTime>,
}

impl Default for RenderOpts<'_> {
    fn default() -> Self {
        RenderOpts {
            scale: 1.0,
            edit: false,
            show_hidden: false,
            state: None,
            skip: &[],
            skip_text: None,
            editing: None,
            gif_times: &[],
            threads: 0,
            offset: (0.0, 0.0),
            size: None,
            clear: None,
            date_text: None,
            now: None,
        }
    }
}

struct SlideFields {
    num: u32,
    /// Text for every date field instead of the date ([`RenderOpts::date_text`]).
    date: Option<String>,
    footer: String,
    /// The local date and time date fields show; `None`: they keep their saved text.
    now: Option<DateTime>,
}

impl SlideFields {
    fn new(num: u32, footer: String, opts: &RenderOpts) -> Self {
        SlideFields { num, date: opts.date_text.clone(), footer, now: opts.now.or_else(DateTime::now_local) }
    }
}

impl Fields for SlideFields {
    fn field(&self, kind: &str) -> Option<String> {
        self.field_in(kind, None)
    }

    fn field_in(&self, kind: &str, lang: Option<&str>) -> Option<String> {
        if kind == "slidenum" {
            return Some(self.num.to_string());
        }
        if datetime::is_date_field(kind) {
            // An automatic date: today's, in the format of its type. (A fixed date is plain text
            // in the slide's placeholder, not a field.)
            return self.date.clone().or_else(|| datetime::field_text(kind, lang, self.now?));
        }
        if kind == "footer" {
            return Some(self.footer.clone());
        }
        None
    }
}

pub struct Renderer {
    resources: Resources,
    /// Outlines morphing in the frame being drawn by `blend`.
    paths: Vec<PathMorph>,
    /// Shapes of the slide being drawn whose effects were left out ([`effects_fit`]).
    skipped_effects: Vec<ShapeId>,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer { resources: Resources::new(), paths: Vec::new(), skipped_effects: Vec::new() }
    }
}

thread_local! {
    static RENDERER: std::cell::RefCell<Renderer> = std::cell::RefCell::new(Renderer::default());
}

/// Does anything on this slide (or its layout/master graphics) use blur-based effects?
pub fn needs_filters(pres: &SlideFile, slide: &Slide) -> bool {
    let Some(rctx) = Ctx::for_slide(pres, slide) else { return false };
    let mut any = false;
    let mut check = |shapes: &[Shape]| {
        kition_slide_model::walk(shapes, &mut |s, _| {
            if any {
                return;
            }
            let (e, _) = resolve::effects(&rctx, s);
            if e.is_some_and(|e| !e.is_empty()) {
                any = true;
            }
        });
    };
    check(&slide.shapes);
    if let Some(l) = rctx.layout {
        check(&l.shapes);
    }
    check(&rctx.master.shapes);
    any
}

/// Render slide `index` at `opts.scale` pixels per point. A panic inside the renderer becomes a
/// blank image (logged), never a crash.
pub fn render_slide(pres: &SlideFile, index: usize, opts: &RenderOpts) -> Image {
    let Some(slide) = pres.slides.get(index) else { return Image::default() };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| RENDERER.with(|r| r.borrow_mut().slide(pres, slide, index, opts)))) {
        Ok(img) => img,
        Err(_) => {
            log::error!("rendering slide {index} panicked; showing a blank slide");
            RENDERER.with(|r| *r.borrow_mut() = Renderer::default());
            let (w, h) = output_size(pres, opts);
            Image { width: w as u32, height: h as u32, pixels: vec![255; w as usize * h as usize * 4] }
        }
    }
}

/// What drawing one slide met ([`probe_slide`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderProbe {
    /// The renderer panicked: [`render_slide`] shows the slide blank. Only a native build can tell;
    /// a WebAssembly build aborts on a panic instead (the extent guards exist so it never does).
    pub panicked: bool,
    /// Shapes (of the slide, its layout or its master) whose effects reach past what the
    /// rasterizer can draw at this scale: they are drawn without them.
    pub effects_skipped: Vec<ShapeId>,
}

/// Draws slide `index` as [`render_slide`] does and reports what went wrong ([`RenderProbe`]).
pub fn probe_slide(pres: &SlideFile, index: usize, opts: &RenderOpts) -> RenderProbe {
    let Some(slide) = pres.slides.get(index) else { return RenderProbe::default() };
    let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        RENDERER.with(|r| {
            let mut r = r.borrow_mut();
            r.slide(pres, slide, index, opts);
            std::mem::take(&mut r.skipped_effects)
        })
    }));
    match drawn {
        Ok(effects_skipped) => RenderProbe { panicked: false, effects_skipped },
        Err(_) => {
            log::error!("rendering slide {index} panicked");
            RENDERER.with(|r| *r.borrow_mut() = Renderer::default());
            RenderProbe { panicked: true, effects_skipped: Vec::new() }
        }
    }
}

/// Render a frame between slides `old` and `new` (indices into `pres.slides`), e.g. of Morph: the
/// backdrop (background, master and layout graphics) of `old` cross-fading into that of `new` by
/// `mix` (0..1), then `shapes` in order, each resolved against the slide it comes from (`true`:
/// `old`). Pairs of shapes in `text` draw their boxes as usual but their texts morph by words or
/// characters ([`TextMorph`]), on top of the later shape of the pair. Shapes listed in `paths` draw
/// an outline morphing from another geometry into their own ([`PathMorph`]). `opts.state` applies
/// to `shapes` only.
pub fn render_blend(
    pres: &SlideFile,
    old: usize,
    new: usize,
    mix: f64,
    shapes: &[(Shape, bool)],
    text: &[TextMorph],
    paths: &[PathMorph],
    opts: &RenderOpts,
) -> Image {
    let (Some(a), Some(b)) = (pres.slides.get(old), pres.slides.get(new)) else { return Image::default() };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        RENDERER.with(|r| r.borrow_mut().blend(pres, (a, old), (b, new), mix, shapes, text, paths, opts))
    })) {
        Ok(img) => img,
        Err(_) => {
            log::error!("rendering a frame between slides {old} and {new} panicked; showing the new slide");
            RENDERER.with(|r| *r.borrow_mut() = Renderer::default());
            render_slide(pres, new, opts)
        }
    }
}

/// Render a layout (Slide Master view thumbnail and canvas).
pub fn render_layout(pres: &SlideFile, master: usize, layout: Option<usize>, opts: &RenderOpts) -> Image {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| RENDERER.with(|r| r.borrow_mut().layout(pres, master, layout, opts)))) {
        Ok(img) => img,
        Err(_) => {
            log::error!("rendering a layout panicked; showing a blank image");
            RENDERER.with(|r| *r.borrow_mut() = Renderer::default());
            Image::default()
        }
    }
}

/// A group's children, each with the transform from the group's parent space to the space it is
/// drawn in. As in PowerPoint a child is drawn at the size it shows: when the group scales its
/// child space, the child's box is scaled ([`kition_slide_geom::group_child_xfrm`]) but its line
/// widths, text and effects are not. Ink keeps the scaled child space its points are in. Children
/// filled with the group's fill (`grpFill`) take `group_fill` (no fill without one).
pub(crate) fn group_children<'a>(
    rctx: &Ctx,
    x: &kition_slide_geom::Xfrm,
    child: &kition_slide_geom::Xfrm,
    children: &'a [Shape],
    group_fill: Option<&Fill>,
) -> Vec<(std::borrow::Cow<'a, Shape>, Affine)> {
    use std::borrow::Cow;
    let ch = Rect::new(child.x, child.y, child.x + child.w, child.y + child.h);
    let scaled = kition_slide_geom::group_child_affine(x, ch);
    let unscaled = (child.w.abs() < 1e-9 || (x.w / child.w - 1.0).abs() < 1e-9) && (child.h.abs() < 1e-9 || (x.h / child.h - 1.0).abs() < 1e-9);
    children
        .iter()
        .map(|c| {
            let mut c = Cow::Borrowed(c);
            if matches!(c.fill, Some(Fill::Group)) {
                c.to_mut().fill = Some(group_fill.filter(|f| !matches!(f, Fill::Group)).cloned().unwrap_or(Fill::None));
            }
            if unscaled || matches!(c.kind, ShapeKind::Ink { .. }) {
                return (c, scaled);
            }
            let cx = kition_slide_geom::group_child_xfrm(x, ch, &resolve::xfrm(rctx, &c));
            c.to_mut().xfrm = Some(cx);
            (c, x.affine())
        })
        .collect()
}

/// Fill `path` (drawn with `m`) with the background of the slide being drawn, in place as it shows
/// behind the slide (a shape with `useBgFill`).
fn shape_background(ctx: &mut RenderContext, f: &Frame, rctx: &Ctx, path: &BezPath, m: Affine, alpha: f64) {
    let (fill, ph) = resolve::background(rctx, f.slide);
    if matches!(fill, Fill::Background | Fill::Group) {
        return;
    }
    let view = Affine::translate(f.opts.offset) * Affine::scale(f.opts.scale);
    let r = Rect::new(0.0, 0.0, f.pres.slide_size.width, f.pres.slide_size.height);
    ctx.set_transform(m);
    ctx.push_clip_layer(path);
    paint::fill_path(ctx, rctx, &fill, ph, &r.to_path(0.1), r, view, FillMode::Norm, alpha, f.pres);
    ctx.pop_layer();
}

/// Does the body hold a field (slide number, date, …)?
fn has_field(body: &TextBody) -> bool {
    body.paragraphs.iter().any(|p| p.runs.iter().any(|r| matches!(r.kind, kition_slide_model::text::RunKind::Field { .. })))
}

fn output_size(pres: &SlideFile, opts: &RenderOpts) -> (u16, u16) {
    let (w, h) = opts
        .size
        .unwrap_or(((pres.slide_size.width * opts.scale).round().max(1.0) as u32, (pres.slide_size.height * opts.scale).round().max(1.0) as u32));
    (w.clamp(1, MAX_SIDE) as u16, h.clamp(1, MAX_SIDE) as u16)
}

pub(crate) fn color(c: Rgba, alpha: f64) -> peniko::Color {
    let a = (c.a as f64 * alpha.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
    peniko::Color::from_rgba8(c.r, c.g, c.b, a)
}

fn shade(c: Rgba, m: FillMode) -> Rgba {
    match m {
        FillMode::Lighten => c.lerp(Rgba::WHITE.with_alpha(c.a), 0.4),
        FillMode::LightenLess => c.lerp(Rgba::WHITE.with_alpha(c.a), 0.2),
        FillMode::Darken => c.lerp(Rgba::BLACK.with_alpha(c.a), 0.4),
        FillMode::DarkenLess => c.lerp(Rgba::BLACK.with_alpha(c.a), 0.2),
        _ => c,
    }
}

/// Geometry of a shape at its size.
pub fn shape_geometry(shape: &Shape, w: f64, h: f64) -> Geometry {
    geom_geometry(&shape.geom, w, h)
}

/// Geometry `geom` at a size.
pub fn geom_geometry(geom: &Geom, w: f64, h: f64) -> Geometry {
    match geom {
        Geom::Preset { name, adj } => preset::build(name, w, h, adj).or_else(|| preset::build("rect", w, h, &[])).unwrap_or(Geometry {
            paths: vec![],
            text_rect: Rect::new(0.0, 0.0, w, h),
            handles: vec![],
            sites: vec![],
        }),
        Geom::Custom { paths, .. } => {
            let mut out = vec![];
            for p in paths {
                let (pw, ph) = (if p.w > 0.0 { p.w } else { w.max(1e-9) }, if p.h > 0.0 { p.h } else { h.max(1e-9) });
                let mut bp = parse_path(&p.d);
                bp.apply_affine(Affine::scale_non_uniform(w / pw, h / ph));
                out.push(SubPath { path: bp, fill: p.fill, stroke: p.stroke, even_odd: false });
            }
            Geometry { paths: out, text_rect: Rect::new(0.0, 0.0, w, h), handles: vec![], sites: vec![] }
        }
    }
}

/// Parse our custom path syntax (`M x y L x y C … Q … Z`).
pub fn parse_path(d: &str) -> BezPath {
    let mut bp = BezPath::new();
    let toks: Vec<&str> = d.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let mut i = 0;
    let num = |i: usize| toks.get(i).and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite());
    let mut started = false;
    while i < toks.len() {
        let t = toks.get(i).copied().unwrap_or("");
        match t {
            "M" => {
                if let (Some(x), Some(y)) = (num(i + 1), num(i + 2)) {
                    bp.move_to((x, y));
                    started = true;
                }
                i += 3;
            }
            "L" => {
                if let (Some(x), Some(y)) = (num(i + 1), num(i + 2)) {
                    if !started {
                        bp.move_to((x, y));
                        started = true;
                    } else {
                        bp.line_to((x, y));
                    }
                }
                i += 3;
            }
            "C" => {
                if let (Some(a), Some(b), Some(c), Some(d2), Some(e), Some(f)) =
                    (num(i + 1), num(i + 2), num(i + 3), num(i + 4), num(i + 5), num(i + 6))
                    && started
                {
                    bp.curve_to((a, b), (c, d2), (e, f));
                }
                i += 7;
            }
            "Q" => {
                if let (Some(a), Some(b), Some(c), Some(d2)) = (num(i + 1), num(i + 2), num(i + 3), num(i + 4))
                    && started
                {
                    bp.quad_to((a, b), (c, d2));
                }
                i += 5;
            }
            "Z" | "z" => {
                if started {
                    bp.close_path();
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    bp
}

/// The rectangle a picture's image occupies given its crop (fractions; negative = padding).
pub fn crop_dest(bounds: Rect, crop: [f64; 4]) -> Rect {
    let [l, t, r, b] = crop.map(|v| if v.is_finite() { v.clamp(-10.0, 0.99) } else { 0.0 });
    let vis_w = (1.0 - l - r).max(1e-6);
    let vis_h = (1.0 - t - b).max(1e-6);
    let fw = bounds.width() / vis_w;
    let fh = bounds.height() / vis_h;
    Rect::new(bounds.x0 - l * fw, bounds.y0 - t * fh, bounds.x0 - l * fw + fw, bounds.y0 - t * fh + fh)
}

struct Frame<'a> {
    pres: &'a SlideFile,
    opts: &'a RenderOpts<'a>,
    fields: SlideFields,
    /// The slide being drawn: shapes filled with the background (`useBgFill`) show its background.
    slide: Option<&'a Slide>,
}

impl Renderer {
    fn canvas(pres: &SlideFile, opts: &RenderOpts, filters: bool) -> (RenderContext, u16, u16, Affine) {
        let (w, h) = output_size(pres, opts);
        let mut ctx = RenderContext::new_with(
            w,
            h,
            vello_cpu::RenderSettings { num_threads: if cfg!(target_arch = "wasm32") || filters { 0 } else { opts.threads }, ..Default::default() },
        );
        if let Some(c) = opts.clear {
            ctx.set_paint(color(c, 1.0));
            ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        let view = Affine::translate((opts.offset.0, opts.offset.1)) * Affine::scale(opts.scale);
        (ctx, w, h, view)
    }

    fn fields(pres: &SlideFile, index: usize, opts: &RenderOpts) -> SlideFields {
        SlideFields::new(pres.slide_number(index), pres.header_footer.footer_text.clone(), opts)
    }

    /// Background plus the master and layout graphics the slide shows.
    fn backdrop(&mut self, ctx: &mut RenderContext, f: &Frame, rctx: &Ctx, slide: &Slide, view: Affine) {
        self.background(ctx, f, rctx, Some(slide), view);
        let (show_layout, show_master) = resolve::show_master_shapes(slide, rctx.layout);
        if show_master {
            let mctx = Ctx::for_master(f.pres, rctx.master);
            for s in &rctx.master.shapes {
                if s.ph.is_none() {
                    self.shape(ctx, f, &mctx, s, view, 0);
                }
            }
        }
        if show_layout && let Some(l) = rctx.layout {
            let lctx = Ctx::for_layout(f.pres, rctx.master, l);
            for s in &l.shapes {
                if s.ph.is_none() {
                    self.shape(ctx, f, &lctx, s, view, 0);
                }
            }
        }
    }

    fn slide(&mut self, pres: &SlideFile, slide: &Slide, index: usize, opts: &RenderOpts) -> Image {
        self.skipped_effects.clear();
        let (mut ctx, w, h, view) = Self::canvas(pres, opts, needs_filters(pres, slide));
        let f = Frame { pres, opts, fields: Self::fields(pres, index, opts), slide: Some(slide) };
        if let Some(rctx) = Ctx::for_slide(pres, slide) {
            self.backdrop(&mut ctx, &f, &rctx, slide, view);
            for s in &slide.shapes {
                self.shape(&mut ctx, &f, &rctx, s, view, 0);
            }
        }
        self.finish(ctx, w, h)
    }

    #[allow(clippy::too_many_arguments)]
    fn blend(
        &mut self,
        pres: &SlideFile,
        old: (&Slide, usize),
        new: (&Slide, usize),
        mix: f64,
        shapes: &[(Shape, bool)],
        text: &[TextMorph],
        paths: &[PathMorph],
        opts: &RenderOpts,
    ) -> Image {
        let mix = if mix.is_finite() { mix.clamp(0.0, 1.0) } else { 1.0 };
        let (mut ctx, w, h, view) = Self::canvas(pres, opts, needs_filters(pres, old.0) || needs_filters(pres, new.0));
        let (Some(co), Some(cn)) = (Ctx::for_slide(pres, old.0), Ctx::for_slide(pres, new.0)) else { return self.finish(ctx, w, h) };
        // Backdrop graphics never take shape states: their ids may clash with the frame's shapes.
        let bopts = RenderOpts { state: None, date_text: opts.date_text.clone(), ..*opts };
        if mix < 1.0 {
            let f = Frame { pres, opts: &bopts, fields: Self::fields(pres, old.1, opts), slide: Some(old.0) };
            self.backdrop(&mut ctx, &f, &co, old.0, view);
        }
        if mix > 0.0 {
            let f = Frame { pres, opts: &bopts, fields: Self::fields(pres, new.1, opts), slide: Some(new.0) };
            let layered = mix < 1.0;
            if layered {
                ctx.push_layer(None, None, Some(mix as f32), None, None);
            }
            self.backdrop(&mut ctx, &f, &cn, new.0, view);
            if layered {
                ctx.pop_layer();
            }
        }
        let fo = Frame { pres, opts, fields: Self::fields(pres, old.1, opts), slide: Some(old.0) };
        let fnew = Frame { pres, opts, fields: Self::fields(pres, new.1, opts), slide: Some(new.0) };
        let mut drawn: Vec<ShapeId> = Vec::new();
        // Set only now: the backdrop's ids may clash with the frame's.
        self.paths = paths.to_vec();
        for (s, from_old) in shapes {
            let (f, c) = if *from_old { (&fo, &co) } else { (&fnew, &cn) };
            let Some(tm) = text.iter().find(|m| m.old == s.id || m.new == s.id) else {
                self.shape(&mut ctx, f, c, s, view, 0);
                continue;
            };
            self.shape(&mut ctx, f, c, &Shape { text: None, ..s.clone() }, view, 0);
            drawn.push(s.id);
            if !(drawn.contains(&tm.old) && drawn.contains(&tm.new)) {
                continue;
            }
            let find = |id: ShapeId| shapes.iter().map(|(s, _)| s).find(|s| s.id == id);
            if let (Some(a), Some(b)) = (find(tm.old), find(tm.new)) {
                let now = b.xfrm.unwrap_or_else(|| resolve::xfrm(&cn, b));
                morph_text::draw(&mut ctx, (&co, a, &fo.fields), (&cn, b, &fnew.fields), tm, now, view);
            }
        }
        self.paths.clear();
        self.finish(ctx, w, h)
    }

    fn layout(&mut self, pres: &SlideFile, mi: usize, li: Option<usize>, opts: &RenderOpts) -> Image {
        let (w, h) = output_size(pres, opts);
        let mut ctx = RenderContext::new_with(w, h, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        let view = Affine::translate((opts.offset.0, opts.offset.1)) * Affine::scale(opts.scale);
        let f = Frame { pres, opts, fields: SlideFields::new(1, String::new(), opts), slide: None };
        if let Some(m) = pres.masters.get(mi) {
            match li.and_then(|i| m.layouts.get(i)) {
                Some(l) => {
                    let c = Ctx::for_layout(pres, m, l);
                    self.background(&mut ctx, &f, &c, None, view);
                    if l.show_master_shapes {
                        let mc = Ctx::for_master(pres, m);
                        for s in m.shapes.iter().filter(|s| s.ph.is_none()) {
                            self.shape(&mut ctx, &f, &mc, s, view, 0);
                        }
                    }
                    for s in &l.shapes {
                        self.shape(&mut ctx, &f, &c, s, view, 0);
                    }
                }
                None => {
                    let c = Ctx::for_master(pres, m);
                    self.background(&mut ctx, &f, &c, None, view);
                    for s in &m.shapes {
                        self.shape(&mut ctx, &f, &c, s, view, 0);
                    }
                }
            }
        }
        self.finish(ctx, w, h)
    }

    fn finish(&mut self, mut ctx: RenderContext, w: u16, h: u16) -> Image {
        ctx.flush();
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        if let Some(pm) = vello_cpu::PixmapMut::new(w, h, &mut pixels) {
            ctx.render(pm, &mut self.resources);
        }
        Image { width: w as u32, height: h as u32, pixels }
    }

    fn background(&mut self, ctx: &mut RenderContext, f: &Frame, rctx: &Ctx, slide: Option<&Slide>, view: Affine) {
        let (fill, ph) = resolve::background(rctx, slide);
        let r = Rect::new(0.0, 0.0, f.pres.slide_size.width, f.pres.slide_size.height);
        ctx.set_transform(view);
        let path = r.to_path(0.1);
        paint::fill_path(ctx, rctx, &fill, ph, &path, r, view, FillMode::Norm, 1.0, f.pres);
    }

    fn shape(&mut self, ctx: &mut RenderContext, f: &Frame, rctx: &Ctx, s: &Shape, parent: Affine, depth: usize) {
        if depth > 32 || (s.hidden && !f.opts.show_hidden) || f.opts.skip.contains(&s.id) {
            return;
        }
        // Footer placeholders on layouts/masters show only on slides that include them.
        let st = f.opts.state.map(|g| g(s.id)).unwrap_or_default();
        if !st.visible || st.opacity <= 0.001 {
            return;
        }
        let x = resolve::xfrm(rctx, s);
        if !x.is_finite() {
            return;
        }
        let mut m = parent * x.affine();
        if st.offset != Vec2::ZERO || st.scale != (1.0, 1.0) || st.rotate != 0.0 {
            let c = Point::new(x.w / 2.0, x.h / 2.0);
            let around = Affine::translate(c.to_vec2())
                * Affine::rotate(st.rotate.to_radians())
                * Affine::scale_non_uniform(st.scale.0, st.scale.1)
                * Affine::translate(-c.to_vec2());
            m = parent * Affine::translate(st.offset) * x.affine() * around;
        }
        let layered = st.opacity < 0.999 || st.clip.is_some();
        if layered {
            ctx.set_transform(m);
            let clip = st.clip.map(|[l, t, r, b]| Rect::new(l * x.w, t * x.h, r * x.w, b * x.h).to_path(0.1));
            ctx.push_layer(clip.as_ref(), None, Some(st.opacity.clamp(0.0, 1.0) as f32), None, None);
        }
        match &s.kind {
            ShapeKind::Group { children, child } => {
                for (c, gm) in group_children(rctx, &x, child, children, s.fill.as_ref()) {
                    self.shape(ctx, f, rctx, &c, parent * gm, depth + 1);
                }
            }
            ShapeKind::Table(t) => table::draw(ctx, f.pres, rctx, t, m, x.w, x.h, &f.fields),
            ShapeKind::Chart(c) => chart::draw(ctx, rctx, c, m, x.w, x.h),
            ShapeKind::Ink { strokes } => {
                for sk in strokes {
                    let bp = kition_slide_geom::ink_centre_line(sk.points.iter().map(|&(x, y, _)| Point::new(x, y)));
                    ctx.set_transform(parent);
                    ctx.set_paint(color(sk.color, if sk.highlighter { 0.4 } else { 1.0 }));
                    ctx.set_stroke(kurbo::Stroke::new(sk.width.max(0.1)).with_caps(kurbo::Cap::Round).with_join(kurbo::Join::Round));
                    crate::stroke::paint(ctx, &bp);
                }
            }
            _ => self.geometry_shape(ctx, f, rctx, s, m, x.w, x.h, &st),
        }
        if layered {
            ctx.pop_layer();
        }
    }

    fn geometry_shape(&mut self, ctx: &mut RenderContext, f: &Frame, rctx: &Ctx, s: &Shape, m: Affine, w: f64, h: f64, st: &ShapeState) {
        let (geo, fades) = match self.paths.iter().find(|p| p.id == s.id) {
            Some(pm) => morph_path::geometry(pm, s, w, h),
            None => (geom_geometry(resolve::geom(rctx, s), w, h), Vec::new()),
        };
        let fade = |i: usize| fades.get(i).copied().unwrap_or(kition_slide_geom::morph::Fade { fill: 1.0, stroke: 1.0 });
        let (fill, fill_ph) = resolve::fill(rctx, s);
        let (line, line_ph) = resolve::line(rctx, s);
        let (effects, fx_ph) = resolve::effects(rctx, s);
        let fits = effects.as_ref().is_none_or(|e| effects_fit(e, m, w, h));
        if !fits {
            self.skipped_effects.push(s.id);
        }
        let effects = effects.filter(|_| fits);
        let is_pic = matches!(s.kind, ShapeKind::Picture { .. });
        let outline = geo.outline();
        // Placeholders in the editor: dashed outline and prompt, nothing else when empty.
        let empty_ph = s.ph.is_some() && s.text.as_ref().is_none_or(TextBody::is_empty) && !is_pic && !matches!(s.kind, ShapeKind::Media(_));
        let fill = if let Some(t) = st.tint { Some(Fill::solid(kition_slide_model::ColorRef::rgb(t))) } else { fill };
        // Effects below the shape.
        if let Some(e) = &effects {
            // A shape without a fill casts its shadow and glow from its outline only.
            let unfilled = !is_pic && !matches!(s.kind, ShapeKind::Media(_)) && fill.as_ref().is_none_or(|f| matches!(f, Fill::None));
            let lw = if line.fill.as_ref().is_some_and(|f| !f.is_none()) { line.width.unwrap_or(0.75).max(0.0) } else { 0.0 };
            self.effects_below(ctx, rctx, e, fx_ph, &geo, &fill, unfilled.then_some(lw), m, w, h);
        }
        let soft = effects.as_ref().and_then(|e| e.soft_edge).filter(|r| *r > 0.0);
        if let Some(r) = soft {
            ctx.set_transform(Affine::IDENTITY);
            ctx.push_layer(None, None, None, None, None);
            let _ = r;
        }
        // Fill.
        match &s.kind {
            ShapeKind::Picture { fill: pf } => {
                ctx.set_transform(m);
                let bounds = Rect::new(0.0, 0.0, w, h);
                ctx.push_clip_layer(&outline);
                let t = f.opts.gif_times.iter().find(|g| g.0 == s.id).map(|g| g.1);
                paint::picture_for_shape(ctx, rctx, pf, bounds, m, &resolve::xfrm(rctx, s), &outline, t);
                ctx.pop_layer();
            }
            // An object kept as XML (an OLE object, an equation) shows its preview picture.
            ShapeKind::Opaque { preview: Some(media), .. } => {
                ctx.set_transform(m);
                let pf = kition_slide_model::style::PictureFill { media: *media, ..Default::default() };
                ctx.push_clip_layer(&outline);
                paint::picture(ctx, rctx, &pf, None, Rect::new(0.0, 0.0, w, h), m, None);
                ctx.pop_layer();
            }
            ShapeKind::Media(mc) => {
                ctx.set_transform(m);
                let bounds = Rect::new(0.0, 0.0, w, h);
                if let Some(p) = mc.poster {
                    let pf = kition_slide_model::style::PictureFill { media: p, ..Default::default() };
                    paint::picture(ctx, rctx, &pf, None, bounds, m, None);
                } else if mc.video {
                    ctx.set_paint(peniko::Color::from_rgba8(20, 20, 24, 255));
                    ctx.fill_rect(&bounds);
                }
                if mc.poster.is_none() {
                    paint::media_glyph(ctx, mc.video, bounds);
                }
            }
            _ => {
                if let Some(fl) = &fill
                    && !(empty_ph && f.opts.edit && s.fill.is_none())
                {
                    let fixed_picture = if let Fill::Picture(picture) = fl
                        && !picture.rotate_with_shape
                    {
                        let (bounds, paint_m) = paint::picture_shape_frame(picture, Rect::new(0.0, 0.0, w, h), m, &resolve::xfrm(rctx, s), &outline);
                        Some((picture, bounds, paint_m))
                    } else {
                        None
                    };
                    for (i, sp) in geo.paths.iter().enumerate() {
                        if sp.fill == FillMode::None || fade(i).fill <= 0.0 {
                            continue;
                        }
                        ctx.set_transform(m);
                        ctx.set_fill_rule(if sp.even_odd { peniko::Fill::EvenOdd } else { peniko::Fill::NonZero });
                        if matches!(fl, Fill::Background) {
                            shape_background(ctx, f, rctx, &sp.path, m, fade(i).fill);
                            ctx.set_fill_rule(peniko::Fill::NonZero);
                            continue;
                        }
                        if let Some((picture, bounds, paint_m)) = fixed_picture {
                            ctx.set_transform(m);
                            ctx.push_clip_layer(&sp.path);
                            ctx.set_transform(paint_m);
                            paint::picture(ctx, rctx, picture, fill_ph, bounds, paint_m, None);
                            ctx.pop_layer();
                        } else {
                            paint::fill_path(ctx, rctx, fl, fill_ph, &sp.path, Rect::new(0.0, 0.0, w, h), m, sp.fill, fade(i).fill, f.pres);
                        }
                        ctx.set_fill_rule(peniko::Fill::NonZero);
                    }
                }
            }
        }
        // Outline.
        if line.fill.as_ref().is_some_and(|f| !f.is_none()) {
            let lw = line.width.unwrap_or(0.75).max(0.0);
            let path_bounds: Vec<_> = geo.paths.iter().map(|p| p.path.bounding_box()).collect();
            for (i, sp) in geo.paths.iter().enumerate().filter(|(i, p)| p.stroke && fade(*i).stroke > 0.0) {
                let bounds = path_bounds.get(i).copied().unwrap_or_default();
                let isolated = geo.paths.iter().enumerate().all(|(j, other)| {
                    if j == i || other.fill == FillMode::None || other.path.is_empty() {
                        return true;
                    }
                    let inner = path_bounds.get(j).copied().unwrap_or_default();
                    inner.x0 > bounds.x0 && inner.y0 > bounds.y0 && inner.x1 < bounds.x1 && inner.y1 < bounds.y1
                });
                stroke(ctx, rctx, &line, line_ph, &sp.path, m, lw, fade(i).stroke, isolated);
            }
            if s.is_line() || geo.is_open() {
                arrowheads(ctx, rctx, &line, line_ph, &geo, m, lw);
            }
        }
        if let Some(rf) = effects.as_ref().and_then(|e| e.reflection.as_ref()) {
            reflection(ctx, rctx, rf, fill.as_ref(), fill_ph, &line, line_ph, &geo, m, w, h, &resolve::xfrm(rctx, s));
        }
        if let Some(r) = soft {
            // Feather: fade the shape's alpha towards its outline. The mask is the outline shrunk
            // by 0.83 of the radius and blurred by 0.3 of it, which reproduces the reference
            // renders' edge profile (transparent at the outline, opaque about 1.6 radii inside).
            ctx.set_transform(m);
            ctx.push_layer(
                None,
                Some(peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn)),
                None,
                None,
                Some(blur(r * 0.3 * scale_of(m))),
            );
            ctx.set_paint(peniko::Color::from_rgba8(0, 0, 0, 255));
            ctx.fill_path(&outline);
            ctx.set_blend_mode(peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestOut));
            ctx.set_stroke(kurbo::Stroke::new(r * 0.83 * 2.0).with_join(kurbo::Join::Round));
            crate::stroke::paint(ctx, &outline);
            ctx.set_blend_mode(peniko::BlendMode::default());
            ctx.pop_layer();
            ctx.pop_layer();
        }
        // Inner shadow.
        if let Some(e) = &effects
            && let Some(sh) = &e.inner_shadow
        {
            ctx.set_transform(m);
            ctx.push_clip_layer(&outline);
            let c = rctx.color(&sh.color, fx_ph);
            let d = Vec2::new(sh.dir.to_radians().cos(), sh.dir.to_radians().sin()) * sh.dist;
            ctx.push_layer(None, None, None, None, Some(blur(sh.blur * 0.5 * scale_of(m))));
            ctx.set_paint(color(c, 1.0));
            let mut ring = Rect::new(0.0, 0.0, w, h).inflate(w.max(h), w.max(h)).to_path(0.1);
            let mut hole = outline.clone();
            // The ring is the inverse silhouette: move its hole opposite the authored
            // shadow offset so the visible inset lies on the requested side.
            hole.apply_affine(Affine::translate(-d));
            ring.extend(hole.iter());
            ctx.set_fill_rule(peniko::Fill::EvenOdd);
            ctx.fill_path(&ring);
            ctx.set_fill_rule(peniko::Fill::NonZero);
            ctx.pop_layer();
            ctx.pop_layer();
        }
        // Text (or the placeholder prompt in the editor).
        let skip_text = f.opts.skip_text == Some(s.id);
        // While editing, a smart graphic node's empty paragraphs show their prompts as its text would.
        let prompted = s.text.as_ref().filter(|_| f.opts.edit).and_then(|body| with_prompts(body, &s.prompts));
        if let Some(body) = prompted.as_ref().or(s.text.as_ref())
            && !skip_text
        {
            let tr = geo.text_rect;
            let editing = f.opts.edit && f.opts.editing == Some(s.id);
            // A field without a saved value (a date with no cached text) still shows its value.
            if !body.is_empty() || has_field(body) || editing {
                let l = kition_slide_text::layout_with_transform(
                    rctx,
                    s,
                    body,
                    &Opts { rect: tr, fields: &f.fields, prompt_color: None, no_shrink: false, empty_bullets: editing },
                    m,
                );
                let bp = resolve::body(rctx, s);
                let warped = bp.warp.as_deref().is_some_and(|preset| {
                    warp_text::try_draw(
                        ctx,
                        rctx,
                        &l,
                        tr,
                        text_transform(m, tr, l.rotation),
                        preset,
                        &bp.warp_adjust,
                        bp.anchor.unwrap_or(kition_slide_model::text::Anchor::Top),
                        &st.paras,
                    )
                });
                if !warped {
                    match body.body.warp.as_deref().filter(|w| warp_text::bends(Some(w))) {
                        // Art text (DR26).
                        Some(preset) => warp_text::draw(ctx, &l, tr, text_transform(m, tr, l.rotation), preset, &body.body.warp_adjust),
                        None => draw_layout_with_context(ctx, rctx, &l, tr, m, &st.paras),
                    }
                }
            } else if f.opts.edit && s.ph.is_some() {
                let kind = s.ph_type().unwrap_or(PhType::Body);
                let prompt_body = prompt_body(rctx, s, kind);
                if !prompt_body.is_empty() {
                    let grey = Rgba::rgb(0x59, 0x59, 0x59);
                    draw_text(ctx, rctx, s, &prompt_body, tr, m, &f.fields, Some(grey));
                }
            }
        }
        if empty_ph && f.opts.edit {
            // Dashed placeholder outline.
            ctx.set_transform(m);
            let px = 1.0 / scale_of(m).max(1e-6);
            ctx.set_paint(peniko::Color::from_rgba8(0xA6, 0xA6, 0xA6, 255));
            ctx.set_stroke(kurbo::Stroke::new(px).with_dashes(0.0, [3.0 * px, 2.0 * px]));
            crate::stroke::paint(ctx, &Rect::new(0.0, 0.0, w, h).to_path(0.1));
            if matches!(s.ph_type(), Some(PhType::Picture | PhType::Chart | PhType::Table | PhType::Media | PhType::Diagram)) {
                paint::placeholder_icon(ctx, s.ph_type().unwrap_or(PhType::Picture), Rect::new(0.0, 0.0, w, h));
            }
        }
    }

    fn effects_below(
        &mut self,
        ctx: &mut RenderContext,
        rctx: &Ctx,
        e: &Effects,
        ph: Option<Rgba>,
        geo: &Geometry,
        fill: &Option<Fill>,
        outline_only: Option<f64>,
        m: Affine,
        w: f64,
        h: f64,
    ) {
        if outline_only.is_some_and(|lw| lw <= 0.0) {
            return;
        }
        // A donut's or frame's hole stays empty under its shadow and glow (even-odd sub-paths).
        let mut even_odd = false;
        let silhouette = {
            let mut p = BezPath::new();
            for sp in &geo.paths {
                if sp.fill != FillMode::None || fill.is_none() {
                    p.extend(sp.path.iter());
                    even_odd |= sp.even_odd;
                }
            }
            if p.elements().is_empty() { geo.outline() } else { p }
        };
        let k = scale_of(m);
        if let Some(sh) = &e.outer_shadow {
            let c = rctx.color(&sh.color, ph);
            let d = Vec2::new(sh.dir.to_radians().cos(), sh.dir.to_radians().sin()) * sh.dist;
            let c0 = Point::new(w / 2.0, h / 2.0);
            let local = Affine::translate(c0.to_vec2())
                * Affine::new([sh.sx, sh.ky.to_radians().tan(), sh.kx.to_radians().tan(), sh.sy, 0.0, 0.0])
                * Affine::translate(-c0.to_vec2());
            // Shadow offset is in slide space (doesn't rotate with the shape unless asked).
            let sm = if sh.rotate_with_shape { m * Affine::translate(d) * local } else { pre_translate(m, d * k) * local };
            ctx.set_transform(Affine::IDENTITY);
            ctx.push_layer(None, None, Some((c.a as f32) / 255.0), None, Some(blur(sh.blur * 0.5 * k)));
            ctx.set_transform(sm);
            ctx.set_paint(color(c.with_alpha(255), 1.0));
            match outline_only {
                Some(lw) => {
                    ctx.set_stroke(kurbo::Stroke::new(lw).with_join(kurbo::Join::Round));
                    crate::stroke::paint(ctx, &silhouette);
                }
                None => {
                    ctx.set_fill_rule(if even_odd { peniko::Fill::EvenOdd } else { peniko::Fill::NonZero });
                    ctx.fill_path(&silhouette);
                    ctx.set_fill_rule(peniko::Fill::NonZero);
                }
            }
            ctx.pop_layer();
        }
        if let Some(g) = &e.glow {
            shape_glow::draw(ctx, &silhouette, m, rctx.color(&g.color, ph), g.radius, outline_only, even_odd);
        }
    }
}

/// A reflection: the shape's fill and outline mirrored about its bottom edge, `dist` below it and
/// scaled by `sy`, blurred, fading from the start to the end opacity over `end_pos` of its height.
#[allow(clippy::too_many_arguments)]
fn reflection(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    rf: &kition_slide_model::style::Reflection,
    fill: Option<&Fill>,
    fill_ph: Option<Rgba>,
    line: &Line,
    line_ph: Option<Rgba>,
    geo: &Geometry,
    m: Affine,
    w: f64,
    h: f64,
    shape: &kition_slide_geom::Xfrm,
) {
    let sy = if rf.sy.is_finite() && rf.sy.abs() > 1e-3 { rf.sy.abs().min(10.0) } else { 1.0 };
    let dist = if rf.dist.is_finite() { rf.dist } else { 0.0 };
    // Local y = h (the bottom edge) lands `dist` below the shape; y = 0 lands sy·h further down.
    let mm = m * Affine::translate((0.0, h + dist)) * Affine::scale_non_uniform(1.0, -sy) * Affine::translate((0.0, -h));
    let k = scale_of(m);
    ctx.set_transform(Affine::IDENTITY);
    ctx.push_layer(None, None, None, None, Some(blur(rf.blur.max(0.0) * 0.5 * k)));
    if let Some(fl) = fill {
        let fixed_picture = if let Fill::Picture(picture) = fl
            && !picture.rotate_with_shape
        {
            let (bounds, paint_m) = paint::picture_shape_frame(picture, Rect::new(0.0, 0.0, w, h), mm, shape, &geo.outline());
            Some((picture, bounds, paint_m))
        } else {
            None
        };
        for sp in geo.paths.iter().filter(|p| p.fill != FillMode::None) {
            if let Some((picture, bounds, paint_m)) = fixed_picture {
                ctx.set_transform(mm);
                ctx.push_clip_layer(&sp.path);
                ctx.set_transform(paint_m);
                paint::picture(ctx, rctx, picture, fill_ph, bounds, paint_m, None);
                ctx.pop_layer();
            } else {
                paint::fill_path(ctx, rctx, fl, fill_ph, &sp.path, Rect::new(0.0, 0.0, w, h), mm, sp.fill, 1.0, rctx.pres);
            }
        }
    }
    if line.fill.as_ref().is_some_and(|f| !f.is_none()) {
        let lw = line.width.unwrap_or(0.75).max(0.0);
        for sp in geo.paths.iter().filter(|p| p.stroke) {
            stroke(ctx, rctx, line, line_ph, &sp.path, mm, lw, 1.0, false);
        }
    }
    // Fade from the mirrored bottom edge (local y = h) over end_pos of the height.
    ctx.set_transform(mm);
    ctx.push_layer(None, Some(peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn)), None, None, None);
    let a = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let grad = peniko::Gradient::new_linear(Point::new(0.0, h), Point::new(0.0, h * (1.0 - rf.end_pos.clamp(0.01, 1.0)))).with_stops(
        [(0.0, peniko::Color::from_rgba8(0, 0, 0, a(rf.start_alpha))), (1.0, peniko::Color::from_rgba8(0, 0, 0, a(rf.end_alpha)))].as_slice(),
    );
    ctx.set_paint(grad);
    ctx.fill_rect(&Rect::new(0.0, 0.0, w, h).inflate(w.max(h), w.max(h)));
    ctx.pop_layer();
    ctx.pop_layer();
}

/// Largest device coordinate (pixels, either sign) drawn inside an effect's filter layer. vello_cpu
/// keeps tile coordinates in `u16` and panics past them when it records a filtered layer; the web
/// build cannot catch that, so effects whose extents leave this box are not drawn.
const EFFECT_LIMIT: f64 = 60_000.0;

/// Do the effects of a `w` x `h` shape drawn with `m` stay within [`EFFECT_LIMIT`]? Covers the
/// largest thing each effect fills: the shape grown by shadow offsets and blurs, glow and soft-edge
/// radii, the reflection below it, and the inner shadow's ring (the box grown by its larger side).
fn effects_fit(e: &Effects, m: Affine, w: f64, h: f64) -> bool {
    let shadow = |s: &Option<kition_slide_model::style::Shadow>| {
        s.as_ref().map_or(0.0, |s| (s.dist.abs() + s.blur.abs() * 2.0) * s.sx.abs().max(s.sy.abs()).max(1.0))
    };
    let pad = [
        shadow(&e.outer_shadow),
        shadow(&e.inner_shadow) + if e.inner_shadow.is_some() { w.abs().max(h.abs()) } else { 0.0 },
        e.glow.as_ref().map_or(0.0, |g| g.radius.abs() * 3.0),
        e.soft_edge.map_or(0.0, |r| r.abs() * 2.0),
        e.reflection.as_ref().map_or(0.0, |r| 2.0 * h.abs() + r.dist.abs() + r.blur.abs() * 2.0),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    let b = m.transform_rect_bbox(Rect::new(0.0, 0.0, w, h).inflate(pad, pad));
    let within = |v: f64| v.is_finite() && v.abs() <= EFFECT_LIMIT;
    let fits = within(b.x0) && within(b.y0) && within(b.x1) && within(b.y1);
    if !fits {
        log::warn!("effects extend past the drawable range; drawing the shape without them");
    }
    fits
}

fn pre_translate(m: Affine, d: Vec2) -> Affine {
    Affine::translate(d) * m
}

pub(crate) fn scale_of(m: Affine) -> f64 {
    m.determinant().abs().sqrt()
}

fn blur(sigma: f64) -> Filter {
    Filter::from_primitive(FilterPrimitive::GaussianBlur { std_deviation: sigma.clamp(0.0, 500.0) as f32, edge_mode: EdgeMode::None })
}

fn stroke(ctx: &mut RenderContext, rctx: &Ctx, line: &Line, ph: Option<Rgba>, path: &BezPath, m: Affine, lw: f64, alpha: f64, snap: bool) {
    use kition_slide_model::style::{LineCap, LineJoin};
    // Quantize coverage independently of the paint's authored coordinate system.
    let paint_path = path;
    let aligned = if snap && alpha == 1.0 {
        stroke_align::segment(path, m, lw, line).filter(|_| match &line.fill {
            Some(Fill::Solid { color }) => rctx.color(color, ph).a == 255,
            Some(Fill::Gradient(gradient)) => !gradient.stops.is_empty() && gradient.stops.iter().all(|stop| rctx.color(&stop.color, ph).a == 255),
            _ => false,
        })
    } else {
        None
    };
    let (path, lw) = aligned.as_ref().map_or((path, lw), |(path, width)| (path, *width));
    let cap = match line.cap.unwrap_or_default() {
        LineCap::Flat => kurbo::Cap::Butt,
        LineCap::Round => kurbo::Cap::Round,
        LineCap::Square => kurbo::Cap::Square,
    };
    let join = match line.join.unwrap_or_default() {
        LineJoin::Round => kurbo::Join::Round,
        LineJoin::Bevel => kurbo::Join::Bevel,
        LineJoin::Miter => kurbo::Join::Miter,
    };
    // Hairlines stay visible at any zoom.
    let min_w = 0.5 / scale_of(m).max(1e-6);
    let mut sk = kurbo::Stroke::new(lw.max(min_w)).with_caps(cap).with_join(join);
    if let Some(d) = &line.dash {
        let pat: Vec<f64> = d.pattern().iter().map(|v| v * lw.max(0.75)).collect();
        if !pat.is_empty() && pat.iter().all(|v| *v > 0.0) {
            sk = sk.with_dashes(0.0, pat);
        }
    }
    let bands: &[f64] = match line.compound {
        Some(kition_slide_model::style::Compound::Double) => &[1.0, 1.0 / 3.0],
        Some(kition_slide_model::style::Compound::Triple) => &[1.0, 2.0 / 3.0, 1.0 / 3.0],
        _ => &[],
    };
    let asymmetric = compound::straight_outline(path, &sk, line.compound).or_else(|| compound::closed_outline(path, &sk, line.compound));
    if matches!(line.fill, Some(Fill::Gradient(_))) || !bands.is_empty() || asymmetric.is_some() {
        let [a, b, c, d, _, _] = m.as_coeffs();
        // The Frobenius norm bounds the maximum stretch, including anisotropy and shear.
        let stretch = a.hypot(b).hypot(c.hypot(d));
        if !stretch.is_finite() || stretch <= 1e-12 {
            return;
        }
        let tolerance = (0.1 / stretch).max(f64::EPSILON);
        let mut outline = asymmetric.unwrap_or_else(|| crate::stroke::outline(path, &sk, tolerance));
        // Nested stroke boundaries alternate painted bands and transparent gaps. Fill them
        // together so translucent paint is composited once and the shape beneath stays visible.
        for (index, fraction) in bands.iter().enumerate().skip(1) {
            let mut inner = sk.clone();
            inner.width *= fraction;
            let boundary = crate::stroke::outline(path, &inner, tolerance);
            outline.extend(if index % 2 == 1 { boundary.reverse_subpaths() } else { boundary });
        }
        if let Some(fill) = &line.fill {
            // Opposite winding subtracts gaps without XOR holes where painted bands cross.
            ctx.set_fill_rule(peniko::Fill::NonZero);
            paint::fill_path(ctx, rctx, fill, ph, &outline, paint_path.bounding_box(), m, FillMode::Norm, alpha, rctx.pres);
        }
        return;
    }
    ctx.set_transform(m);
    let opaque = match &line.fill {
        Some(Fill::Solid { color: c }) => {
            let resolved = rctx.color(c, ph);
            ctx.set_paint(color(resolved, alpha));
            resolved.a == 255 && alpha == 1.0
        }
        _ => {
            ctx.set_paint(color(Rgba::BLACK, alpha));
            false
        }
    };
    // Translucent strokes must retain fractional coverage where they overlap their fill.
    if let Some((snapped, width)) = (snap && opaque).then(|| pixel_aligned_outline(path, m, lw, line)).flatten() {
        ctx.set_transform(Affine::IDENTITY);
        ctx.set_stroke(kurbo::Stroke::new(width).with_caps(cap).with_join(join));
        crate::stroke::paint(ctx, &snapped);
    } else {
        ctx.set_stroke(sk);
        crate::stroke::paint(ctx, path);
    }
}

/// Office raster output aligns thin, rectilinear outlines to pixel centers.
/// Curves, rotated edges and patterned strokes retain their continuous geometry.
fn pixel_aligned_outline(path: &BezPath, m: Affine, width: f64, line: &Line) -> Option<(BezPath, f64)> {
    if line.dash.as_ref().is_some_and(|d| !d.pattern().is_empty()) || !matches!(line.fill, Some(Fill::Solid { .. })) {
        return None;
    }
    let [xx, yx, xy, yy, _, _] = m.as_coeffs();
    if !m.as_coeffs().iter().all(|v| v.is_finite()) || (xx.hypot(yx) - xy.hypot(yy)).abs() > 1e-8 || (xx * xy + yx * yy).abs() > 1e-8 {
        return None;
    }
    let device_width = width * scale_of(m);
    if !(0.75..1.5).contains(&device_width) {
        return None;
    }
    let mut snapped = BezPath::new();
    let mut start = None;
    let mut previous = None;
    let aligned = |a: Point, b: Point| (a.x - b.x).abs() < 1e-8 || (a.y - b.y).abs() < 1e-8;
    let pixel = |p: Point| Point::new(p.x.floor() + 0.5, p.y.floor() + 0.5);
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(p) if start.is_none() => {
                let p = m * p;
                start = Some(p);
                previous = Some(p);
                snapped.move_to(pixel(p));
            }
            PathEl::LineTo(p) => {
                let p = m * p;
                if !aligned(previous?, p) {
                    return None;
                }
                previous = Some(p);
                snapped.line_to(pixel(p));
            }
            PathEl::ClosePath => {
                if !aligned(previous?, start?) {
                    return None;
                }
                snapped.close_path();
                start = None;
                previous = None;
            }
            _ => return None,
        }
    }
    if start.is_some() || snapped.is_empty() {
        return None;
    }
    Some((snapped, 1.0))
}

fn end_points(path: &BezPath) -> Option<((Point, Point), (Point, Point))> {
    let els: Vec<PathEl> = path.elements().to_vec();
    let mut pts: Vec<Point> = vec![];
    for e in &els {
        match e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(*p),
            PathEl::QuadTo(a, b) => {
                pts.push(*a);
                pts.push(*b);
            }
            PathEl::CurveTo(a, b, c) => {
                pts.push(*a);
                pts.push(*b);
                pts.push(*c);
            }
            PathEl::ClosePath => {}
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let first = *pts.first()?;
    let second = pts.iter().skip(1).find(|p| (**p - first).hypot() > 1e-6).copied().unwrap_or(*pts.get(1)?);
    let last = *pts.last()?;
    let prev = pts.iter().rev().skip(1).find(|p| (**p - last).hypot() > 1e-6).copied().unwrap_or(*pts.get(pts.len() - 2)?);
    Some(((first, second), (last, prev)))
}

fn arrowheads(ctx: &mut RenderContext, rctx: &Ctx, line: &Line, ph: Option<Rgba>, geo: &Geometry, m: Affine, lw: f64) {
    let Some(fill) = line.fill.as_ref().filter(|fill| !fill.is_none()) else { return };
    let Some(sp) = geo.paths.iter().find(|p| p.stroke) else { return };
    let Some(((a, a2), (b, b2))) = end_points(&sp.path) else { return };
    let col = match fill {
        Fill::Solid { color: c } => Some(rctx.color(c, ph)),
        _ => None,
    };
    // Decorations share the shaft's authored paint frame; fitting each head separately
    // would restart its gradient and lose the line's endpoint color and transparency.
    let paint_bounds = sp.path.bounding_box();
    let lw = lw.max(0.75);
    for (end, tip, from) in [(&line.head, a, a2), (&line.tail, b, b2)] {
        let Some(e) = end else { continue };
        if e.kind == "none" {
            continue;
        }
        let (wl, ll) = line_end::dimensions(e, lw);
        let dir = (tip - from).normalize();
        if !dir.x.is_finite() {
            continue;
        }
        let n = Vec2::new(-dir.y, dir.x);
        let base = tip - dir * ll;
        let mut p = BezPath::new();
        match e.kind.as_str() {
            "oval" => {
                let c = kurbo::Ellipse::new(tip, (ll / 2.0, wl / 2.0), dir.y.atan2(dir.x));
                p = c.to_path(0.05);
            }
            "diamond" => {
                p.move_to(tip + dir * ll / 2.0);
                p.line_to(tip + n * wl / 2.0);
                p.line_to(tip - dir * ll / 2.0);
                p.line_to(tip - n * wl / 2.0);
                p.close_path();
            }
            "stealth" => {
                p.move_to(tip);
                p.line_to(base + n * wl / 2.0);
                p.line_to(tip - dir * ll * 0.6);
                p.line_to(base - n * wl / 2.0);
                p.close_path();
            }
            "arrow" => {
                let stroke = kurbo::Stroke::new(lw).with_join(kurbo::Join::Miter);
                let inset = line_end::open_tip_inset(wl, ll, lw, stroke.miter_limit);
                let tip = tip - dir * inset;
                let base = base - dir * inset;
                let mut o = BezPath::new();
                o.move_to(base + n * wl / 2.0);
                o.line_to(tip);
                o.line_to(base - n * wl / 2.0);
                if let Some(col) = col {
                    ctx.set_transform(m);
                    ctx.set_paint(color(col, 1.0));
                    ctx.set_stroke(stroke);
                    crate::stroke::paint(ctx, &o);
                } else {
                    let [a, b, c, d, _, _] = m.as_coeffs();
                    let stretch = a.hypot(b).hypot(c.hypot(d));
                    if !stretch.is_finite() || stretch <= 1e-12 {
                        continue;
                    }
                    let outline = crate::stroke::outline(&o, &stroke, (0.1 / stretch).max(f64::EPSILON));
                    paint::fill_path(ctx, rctx, fill, ph, &outline, paint_bounds, m, FillMode::Norm, 1.0, rctx.pres);
                }
                continue;
            }
            _ => {
                p.move_to(tip);
                p.line_to(base + n * wl / 2.0);
                p.line_to(base - n * wl / 2.0);
                p.close_path();
            }
        }
        if let Some(col) = col {
            ctx.set_transform(m);
            ctx.set_paint(color(col, 1.0));
            ctx.fill_path(&p);
        } else {
            paint::fill_path(ctx, rctx, fill, ph, &p, paint_bounds, m, FillMode::Norm, 1.0, rctx.pres);
        }
    }
}

/// `body` with each empty paragraph that has a prompt (see [`Shape::prompts`]) showing it in the
/// paragraph's own formatting; `None` when no prompt shows.
fn with_prompts(body: &TextBody, prompts: &[Option<String>]) -> Option<TextBody> {
    if prompts.iter().all(Option::is_none) {
        return None;
    }
    let mut out = body.clone();
    let mut shown = false;
    for (i, prompt) in prompts.iter().enumerate().take(1000) {
        let Some(prompt) = prompt else { continue };
        if out.paragraphs.len() <= i {
            let Some(last) = out.paragraphs.last().cloned() else { break };
            out.paragraphs.push(kition_slide_model::text::Paragraph { runs: vec![], ..last });
        }
        let Some(p) = out.paragraphs.get_mut(i) else { break };
        if p.is_empty() {
            p.runs = vec![kition_slide_model::text::Run::with(prompt.as_str(), p.end_props.clone())];
            shown = true;
        }
    }
    shown.then_some(out)
}

/// The prompt shown in an empty placeholder, formatted like its content would be.
fn prompt_body(rctx: &Ctx, s: &Shape, kind: PhType) -> TextBody {
    // Layout/master prompt text when the layout supplies a custom one.
    let (lp, _) = resolve::parents(rctx, s);
    let custom = lp.and_then(|l| l.ph.as_ref().filter(|p| p.has_custom_prompt).and(l.text.as_ref())).filter(|t| !t.is_empty());
    if let Some(t) = custom {
        return t.clone();
    }
    if matches!(rctx.owner, resolve::Owner::Layout | resolve::Owner::Master) {
        return TextBody::default();
    }
    let txt = kind.prompt();
    if txt.is_empty() || kind.is_footer_kind() {
        return TextBody::default();
    }
    let mut t = TextBody::from_text(txt);
    if matches!(kind, PhType::Picture | PhType::Chart | PhType::Table | PhType::Media | PhType::Diagram | PhType::ClipArt) {
        t.body.anchor = Some(kition_slide_model::text::Anchor::Bottom);
        if let Some(p) = t.paragraphs.first_mut() {
            p.props.align = Some(kition_slide_model::text::Align::Center);
            p.props.bullet = Some(kition_slide_model::text::Bullet::None);
            p.props.margin_left = Some(0.0);
            p.props.indent = Some(0.0);
            for r in &mut p.runs {
                r.props.size = Some(14.0);
            }
        }
    }
    t
}

thread_local! {
    static GLYPH_CACHE: std::cell::RefCell<HashMap<(u32, u32), Arc<BezPath>>> = std::cell::RefCell::new(HashMap::new());
}

fn glyph(face: &kition_slide_fonts::FontFace, gid: u32) -> Arc<BezPath> {
    let key = (face.id(), gid);
    if let Some(p) = GLYPH_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return p;
    }
    let p = FontDb::global().outline(face, gid);
    GLYPH_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 200_000 {
            c.clear();
        }
        c.insert(key, p.clone());
    });
    p
}

/// Draw a text body into shape space `m` within the text rect.
pub fn draw_text(ctx: &mut RenderContext, rctx: &Ctx, s: &Shape, body: &TextBody, tr: Rect, m: Affine, fields: &dyn Fields, prompt: Option<Rgba>) {
    let l = kition_slide_text::layout_with_transform(
        rctx,
        s,
        body,
        &Opts { rect: tr, fields, prompt_color: prompt, no_shrink: false, empty_bullets: false },
        m,
    );
    draw_layout_with_context(ctx, rctx, &l, tr, m, &[]);
}

/// Where text laid out in text rect `tr` is drawn, for a shape drawn with `m` (shape-local →
/// output): `m`, without mirroring, then the layout's `rotation` about the rect's centre.
///
/// Flips mirror a shape's geometry, never its text. When `m` reflects (flipH or flipV, the shape's
/// own or a group's) the text rect keeps its mirrored place and the text is mirrored back about
/// the rect's centre: a flipH shape's text stays readable and a flipV shape's text is turned 180°
/// (both flips: 180°), as in PowerPoint. Rendering, hit testing and the caret all use this.
pub fn text_transform(m: Affine, tr: Rect, rotation: f64) -> Affine {
    let c = tr.center().to_vec2();
    let mut t = m;
    if m.determinant() < 0.0 && c.is_finite() {
        t = t * Affine::translate(c) * Affine::scale_non_uniform(-1.0, 1.0) * Affine::translate(-c);
    }
    if rotation != 0.0 && rotation.is_finite() && c.is_finite() {
        t = t * Affine::translate(c) * Affine::rotate(rotation.to_radians()) * Affine::translate(-c);
    }
    t
}

/// Draw an already laid-out text block.
pub fn draw_layout(ctx: &mut RenderContext, l: &kition_slide_text::TextLayout, tr: Rect, m: Affine) {
    draw_layout_with(ctx, l, tr, m, &[]);
}

/// Draw a laid-out text block with per-paragraph animation states.
pub fn draw_layout_with(ctx: &mut RenderContext, l: &kition_slide_text::TextLayout, tr: Rect, m: Affine, paras: &[(usize, ParaState)]) {
    draw_layout_inner(ctx, None, l, tr, m, paras);
}

/// Draw text with its document resources, including picture and pattern fills.
/// Context-free drawing retains its existing flat fallback for these fills.
pub fn draw_layout_with_context(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    l: &kition_slide_text::TextLayout,
    tr: Rect,
    m: Affine,
    paras: &[(usize, ParaState)],
) {
    draw_layout_inner(ctx, Some(rctx), l, tr, m, paras);
}

fn draw_layout_inner(
    ctx: &mut RenderContext,
    rctx: Option<&Ctx>,
    l: &kition_slide_text::TextLayout,
    tr: Rect,
    m: Affine,
    paras: &[(usize, ParaState)],
) {
    let para = |i: usize| paras.iter().find(|(p, _)| *p == i).map(|(_, s)| *s).unwrap_or_default();
    let m = text_transform(m, tr, l.rotation);
    let deco = |ctx: &mut RenderContext, d: &kition_slide_text::Deco| {
        let ps = para(d.para);
        if !ps.visible || ps.opacity <= 0.001 {
            return;
        }
        ctx.set_transform(m * Affine::translate(ps.offset));
        ctx.set_paint(color(d.color, ps.opacity));
        ctx.fill_rect(&d.rect);
    };
    for d in l.decos.iter().filter(|d| d.behind) {
        deco(ctx, d);
    }
    for run in &l.runs {
        let ps = para(run.para);
        if ps.visible && ps.opacity > 0.001 {
            text_effects::draw(ctx, run, m * Affine::translate(ps.offset), ps.opacity);
        }
    }
    let mut index = 0;
    while let Some(run) = l.runs.get(index) {
        let end = if rctx.is_some() || run.fill.is_none() { text_gradient::group_end(l, index) } else { index + 1 };
        let ps = para(run.para);
        if ps.visible && ps.opacity > 0.001 {
            let m = m * Affine::translate(ps.offset);
            if run.fill.is_some()
                && let Some(rctx) = rctx
            {
                if let Some(runs) = l.runs.get(index..end) {
                    text_fill::draw(ctx, rctx, runs, m, ps.opacity);
                }
            } else if run.gradient.is_some() {
                if let Some(runs) = l.runs.get(index..end) {
                    text_gradient::draw(ctx, runs, m, ps.opacity);
                }
            } else {
                for (gid, x, y) in &run.glyphs {
                    draw_glyph(ctx, run, *gid, m * Affine::translate((*x, *y)), ps.opacity);
                }
            }
        }
        index = end;
    }
    for d in l.decos.iter().filter(|d| !d.behind) {
        deco(ctx, d);
    }
}

/// Draw glyph `gid` of `run` with its origin (on the baseline) mapped by `at`, faded by `opacity`.
pub(crate) fn draw_glyph(ctx: &mut RenderContext, run: &kition_slide_text::GlyphRun, gid: u32, at: Affine, opacity: f64) {
    let alpha = run.alpha * opacity;
    if (alpha <= 0.0 || run.color.a == 0) && run.outline.is_none() {
        return;
    }
    let path = glyph(&run.face, gid);
    if path.elements().is_empty() {
        return;
    }
    let at = if run.upright { at * Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { at };
    let k = run.size / run.face.upem.max(1.0);
    let skew = if run.fake_italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
    ctx.set_transform(at * skew * Affine::scale(k));
    if alpha > 0.0 && run.color.a > 0 {
        ctx.set_paint(color(run.color, alpha));
        ctx.fill_path(&path);
        if run.fake_bold {
            ctx.set_stroke(kurbo::Stroke::new(run.face.upem * 0.025).with_join(kurbo::Join::Round));
            crate::stroke::paint(ctx, &path);
        }
    }
    if let Some((oc, ow)) = run.outline {
        ctx.set_paint(color(oc, opacity));
        ctx.set_stroke(kurbo::Stroke::new(ow / k.max(1e-9)).with_join(kurbo::Join::Round));
        crate::stroke::paint(ctx, &path);
    }
}

/// Render one shape alone onto a transparent canvas (drag previews, copy as picture).
pub fn render_shape(pres: &SlideFile, slide: &Slide, id: ShapeId, scale: f64) -> Option<Image> {
    let rctx = Ctx::for_slide(pres, slide)?;
    let s = slide.shape(id)?;
    let x = resolve::xfrm(&rctx, s);
    let b = x.bounds().inflate(8.0, 8.0);
    let w = (b.width() * scale).ceil().clamp(1.0, MAX_SIDE as f64) as u16;
    let h = (b.height() * scale).ceil().clamp(1.0, MAX_SIDE as f64) as u16;
    let mut ctx = RenderContext::new_with(w, h, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    let opts = RenderOpts { scale, ..Default::default() };
    let f = Frame { pres, opts: &opts, fields: SlideFields::new(1, String::new(), &opts), slide: Some(slide) };
    let view = Affine::scale(scale) * Affine::translate(-b.origin().to_vec2());
    RENDERER.with(|r| {
        let mut r = r.borrow_mut();
        r.shape(&mut ctx, &f, &rctx, s, view, 0);
        Some(r.finish(ctx, w, h))
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod robust_tests;

#[cfg(test)]
mod fidelity_tests;

#[cfg(test)]
mod arrowhead_tests;

#[cfg(test)]
mod mt_tests {
    use super::*;

    /// Regression: multithreaded rendering of a slide with shadows panicked inside vello_cpu.
    #[test]
    fn effects_render_with_threads_requested() {
        let mut s = kition_slide_model::SlideFile::default();
        let sl = std::sync::Arc::make_mut(&mut s.slides[0]);
        let mut sh = kition_slide_model::Shape {
            id: ShapeId(900),
            xfrm: Some(kition_slide_geom::Xfrm::new(10.0, 10.0, 100.0, 100.0)),
            style: Some(kition_slide_model::ShapeStyle::accent(kition_slide_color::SchemeSlot::Accent1)),
            ..Default::default()
        };
        sh.effects = Some(kition_slide_model::Effects { soft_edge: Some(5.0), ..Default::default() });
        sl.shapes.push(sh);
        assert!(needs_filters(&s, &s.slides[0]));
        let img = RENDERER.with(|r| r.borrow_mut().slide(&s, &s.slides[0], 0, &RenderOpts { scale: 0.5, threads: 4, ..Default::default() }));
        assert_eq!(img.width, 480);
        // The shape itself is drawn (not a blank fallback).
        assert_ne!(img.pixel(30, 30), [255, 255, 255, 255]);
    }
}

#[cfg(test)]
mod vertical_text_tests;
