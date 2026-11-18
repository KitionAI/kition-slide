//! Windows metafiles (WMF and EMF) drawn as pictures.
//!
//! A clean-room reading of the record layouts the public [MS-WMF] and [MS-EMF] specifications
//! define, limited to what pictures and equation previews in slides use: the mapping
//! (window and viewport origins and extents, map modes, world transforms), pens, brushes and
//! fonts in an object table, saved device contexts, lines, polylines, polygons, rectangles,
//! ellipses, paths, text (with per-character advances, alignment and the current position), clip
//! rectangles and device-independent bitmaps. The records are drawn with vello_cpu into a pixmap
//! at the size the picture shows; anything else is skipped. EMF+ records in comments are ignored:
//! every EMF+ file carries the plain EMF records too.

#[cfg(test)]
mod empty_tests;
mod empty_wmf;

use std::sync::Arc;

use kition_slide_color::Rgba;
use kition_slide_fonts::{FontDb, FontFace};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use vello_cpu::{RenderContext, peniko};

/// The largest side a metafile is drawn at, in pixels.
const MAX_SIDE: u32 = 4096;
/// Most records read from one metafile (hostile files stop there).
const MAX_RECORDS: usize = 200_000;
/// Most points in one polygon or polyline record.
const MAX_POINTS: usize = 1_000_000;

/// Is `b` a Windows metafile (placeable or plain WMF, or EMF)?
pub fn is_metafile(b: &[u8]) -> bool {
    is_placeable(b) || is_emf(b) || plain_wmf_header(b)
}

fn is_placeable(b: &[u8]) -> bool {
    u32_at(b, 0) == Some(0x9AC6_CDD7)
}

fn is_emf(b: &[u8]) -> bool {
    u32_at(b, 0) == Some(1) && b.get(40..44) == Some(b" EMF")
}

fn plain_wmf_header(b: &[u8]) -> bool {
    matches!(u16_at(b, 0), Some(1 | 2)) && u16_at(b, 2) == Some(9) && matches!(u16_at(b, 4), Some(0x0100 | 0x0300))
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    b.get(i..i + 2).map(|v| u16::from_le_bytes([v[0], v[1]]))
}
fn i16_at(b: &[u8], i: usize) -> Option<i16> {
    u16_at(b, i).map(|v| v as i16)
}
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4).map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}
fn i32_at(b: &[u8], i: usize) -> Option<i32> {
    u32_at(b, i).map(|v| v as i32)
}
fn f32_at(b: &[u8], i: usize) -> Option<f64> {
    u32_at(b, i).map(|v| f32::from_bits(v) as f64).filter(|v| v.is_finite())
}

/// Draw the metafile so its longer side is about `want` pixels (capped), as straight RGBA on a
/// transparent background. Validated empty WMFs produce transparent images;
/// streams with no painted content that cannot be proven empty return `None`.
pub fn rasterize(b: &[u8], want: f64) -> Option<image::RgbaImage> {
    let frame = if is_emf(b) { emf_frame(b)? } else { wmf_frame(b)? };
    if !(frame.width().abs() > 1e-9 && frame.height().abs() > 1e-9 && frame.width().is_finite() && frame.height().is_finite()) {
        return None;
    }
    let aspect = (frame.width() / frame.height()).abs();
    let side = want.clamp(16.0, MAX_SIDE as f64);
    let (w, h) = if aspect >= 1.0 { (side, side / aspect) } else { (side * aspect, side) };
    let (w, h) = (w.round().clamp(1.0, MAX_SIDE as f64) as u16, h.round().clamp(1.0, MAX_SIDE as f64) as u16);
    // Frame (metafile units or device pixels) → output pixels.
    let out = Affine::scale_non_uniform(w as f64 / frame.width(), h as f64 / frame.height()) * Affine::translate(-frame.origin().to_vec2());
    let mut ctx = RenderContext::new_with(w, h, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    let mut p = Player::new(&mut ctx, out, w, h);
    if is_emf(b) {
        p.play_emf(b);
    } else {
        p.play_wmf(b);
    }
    if !p.drew && !empty_wmf::validated_empty(b) {
        return None;
    }
    p.end();
    ctx.flush();
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let pm = vello_cpu::PixmapMut::new(w, h, &mut px)?;
    ctx.render(pm, &mut vello_cpu::Resources::new());
    for c in px.as_chunks_mut::<4>().0 {
        let a = c[3] as u32;
        if a != 0 && a != 255 {
            for v in c.iter_mut().take(3) {
                *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(w as u32, h as u32, px)
}

/// The picture frame of a WMF in logical units: the placeable header's bounds, else the first
/// window origin and extent.
fn wmf_frame(b: &[u8]) -> Option<Rect> {
    if is_placeable(b) {
        let (l, t, r, bt) = (i16_at(b, 6)?, i16_at(b, 8)?, i16_at(b, 10)?, i16_at(b, 12)?);
        let rect = Rect::new(l as f64, t as f64, r as f64, bt as f64);
        if rect.width() != 0.0 && rect.height() != 0.0 {
            return Some(rect);
        }
    }
    let start = if is_placeable(b) { 22 } else { 0 };
    let mut org = Point::ZERO;
    for (func, p) in wmf_records(b, start) {
        match func {
            0x020B => org = Point::new(i16_at(p, 2)? as f64, i16_at(p, 0)? as f64),
            0x020C => {
                let (ey, ex) = (i16_at(p, 0)? as f64, i16_at(p, 2)? as f64);
                return Some(Rect::new(org.x, org.y, org.x + ex, org.y + ey));
            }
            _ => {}
        }
    }
    None
}

/// The records of a WMF from its header at `start`: (function, parameters).
fn wmf_records(b: &[u8], start: usize) -> impl Iterator<Item = (u16, &[u8])> {
    let header_words = u16_at(b, start + 2).unwrap_or(9) as usize;
    let mut i = start + header_words.max(9) * 2;
    std::iter::from_fn(move || {
        let size = u32_at(b, i)? as usize;
        let func = u16_at(b, i + 4)?;
        if size < 3 || func == 0 {
            return None;
        }
        let end = i.checked_add(size.checked_mul(2)?)?.min(b.len());
        let p = b.get(i + 6..end)?;
        i = end;
        Some((func, p))
    })
    .take(MAX_RECORDS)
}

/// The picture frame of an EMF in device pixels (its header's frame, in 0.01 mm, scaled by the
/// reference device's pixels per millimetre), else its bounds.
fn emf_frame(b: &[u8]) -> Option<Rect> {
    let r = |i: usize| -> Option<Rect> {
        Some(Rect::new(i32_at(b, i)? as f64, i32_at(b, i + 4)? as f64, i32_at(b, i + 8)? as f64, i32_at(b, i + 12)? as f64))
    };
    let bounds = r(8)?;
    let frame = r(24)?;
    let (dev_w, dev_h, mm_w, mm_h) = (i32_at(b, 72)? as f64, i32_at(b, 76)? as f64, i32_at(b, 80)? as f64, i32_at(b, 84)? as f64);
    if dev_w > 0.0 && dev_h > 0.0 && mm_w > 0.0 && mm_h > 0.0 && frame.width() > 0.0 && frame.height() > 0.0 {
        let (kx, ky) = (dev_w / (mm_w * 100.0), dev_h / (mm_h * 100.0));
        return Some(Rect::new(frame.x0 * kx, frame.y0 * ky, (frame.x1 + 1.0) * kx, (frame.y1 + 1.0) * ky));
    }
    Some(Rect::new(bounds.x0, bounds.y0, bounds.x1 + 1.0, bounds.y1 + 1.0))
}

#[derive(Clone, Debug)]
struct Pen {
    color: Rgba,
    /// Logical units; 0 = one pixel.
    width: f64,
    none: bool,
}

#[derive(Clone, Debug)]
struct Brush {
    color: Rgba,
    none: bool,
}

#[derive(Clone, Debug)]
struct Font {
    /// Negative: character height (the em); positive: cell height; logical units.
    height: f64,
    /// Tenths of a degree, counter-clockwise.
    escapement: f64,
    weight: i32,
    italic: bool,
    charset: u8,
    face: String,
}

#[derive(Clone, Debug)]
enum Obj {
    Pen(Pen),
    Brush(Brush),
    Font(Font),
    Other,
}

#[derive(Clone, Debug)]
struct Dc {
    pen: Pen,
    brush: Brush,
    font: Font,
    text_color: Rgba,
    text_align: u32,
    even_odd: bool,
    pos: Point,
    map_mode: u32,
    win_org: Point,
    win_ext: Option<(f64, f64)>,
    vp_org: Point,
    vp_ext: Option<(f64, f64)>,
    world: Affine,
    /// Clip rectangle in output pixels.
    clip: Option<Rect>,
}

impl Default for Dc {
    fn default() -> Self {
        Dc {
            pen: Pen { color: Rgba::BLACK, width: 0.0, none: false },
            brush: Brush { color: Rgba::WHITE, none: false },
            font: Font { height: -12.0, escapement: 0.0, weight: 400, italic: false, charset: 0, face: "Arial".into() },
            text_color: Rgba::BLACK,
            text_align: 0,
            even_odd: true,
            pos: Point::ZERO,
            map_mode: 1,
            win_org: Point::ZERO,
            win_ext: None,
            vp_org: Point::ZERO,
            vp_ext: None,
            world: Affine::IDENTITY,
            clip: None,
        }
    }
}

struct Player<'a> {
    ctx: &'a mut RenderContext,
    /// Frame → output pixels.
    out: Affine,
    size: (u16, u16),
    dc: Dc,
    saved: Vec<Dc>,
    objects: Vec<Option<Obj>>,
    /// An open path bracket (EMF `BEGINPATH`), and the finished path waiting to be drawn.
    path: Option<BezPath>,
    done_path: Option<BezPath>,
    wmf: bool,
    clipped: bool,
    drew: bool,
}

impl<'a> Player<'a> {
    fn new(ctx: &'a mut RenderContext, out: Affine, w: u16, h: u16) -> Self {
        Player {
            ctx,
            out,
            size: (w, h),
            dc: Dc::default(),
            saved: vec![],
            objects: vec![],
            path: None,
            done_path: None,
            wmf: false,
            clipped: false,
            drew: false,
        }
    }

    fn end(&mut self) {
        if self.clipped {
            self.ctx.pop_layer();
            self.clipped = false;
        }
    }

    /// Logical → output pixels.
    fn xf(&self) -> Affine {
        let d = &self.dc;
        let page = if self.wmf {
            // A WMF's window is its logical space; the frame is in the same units.
            Affine::IDENTITY
        } else {
            match (d.map_mode, d.win_ext, d.vp_ext) {
                (7 | 8, Some((wx, wy)), Some((vx, vy))) if wx != 0.0 && wy != 0.0 => {
                    let (sx, mut sy) = (vx / wx, vy / wy);
                    if d.map_mode == 7 {
                        // Isotropic: the same scale on both axes, keeping each sign.
                        let k = sx.abs().min(sy.abs());
                        sy = k * sy.signum();
                        return self.out
                            * Affine::translate(d.vp_org.to_vec2())
                            * Affine::scale_non_uniform(k * sx.signum(), sy)
                            * Affine::translate(-d.win_org.to_vec2())
                            * d.world;
                    }
                    Affine::translate(d.vp_org.to_vec2()) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-d.win_org.to_vec2())
                }
                _ => Affine::translate(d.vp_org.to_vec2() - d.win_org.to_vec2()),
            }
        };
        self.out * page * d.world
    }

    fn set_clip(&mut self, r: Option<Rect>) {
        if self.clipped {
            self.ctx.pop_layer();
            self.clipped = false;
        }
        self.dc.clip = r;
        if let Some(r) = r {
            self.ctx.set_transform(Affine::IDENTITY);
            self.ctx.push_clip_layer(&r.to_path(0.1));
            self.clipped = true;
        }
    }

    fn intersect_clip(&mut self, logical: Rect) {
        let r = self.xf().transform_rect_bbox(logical);
        let full = Rect::new(0.0, 0.0, self.size.0 as f64, self.size.1 as f64);
        let r = self.dc.clip.unwrap_or(full).intersect(r);
        self.set_clip(Some(r));
    }

    fn add_object(&mut self, o: Obj) {
        match self.objects.iter().position(Option::is_none) {
            Some(i) => self.objects[i] = Some(o),
            None => {
                if self.objects.len() < 65_536 {
                    self.objects.push(Some(o));
                }
            }
        }
    }

    fn put_object(&mut self, i: usize, o: Obj) {
        if i >= 65_536 {
            return;
        }
        if self.objects.len() <= i {
            self.objects.resize(i + 1, None);
        }
        self.objects[i] = Some(o);
    }

    fn select(&mut self, o: Obj) {
        match o {
            Obj::Pen(p) => self.dc.pen = p,
            Obj::Brush(b) => self.dc.brush = b,
            Obj::Font(f) => self.dc.font = f,
            Obj::Other => {}
        }
    }

    fn select_index(&mut self, i: usize) {
        if let Some(Some(o)) = self.objects.get(i).cloned() {
            self.select(o);
        }
    }

    /// Stock objects (EMF indices with the high bit set).
    fn select_stock(&mut self, i: u32) {
        let gray = |v: u8| Rgba::rgb(v, v, v);
        let o = match i {
            0 => Obj::Brush(Brush { color: Rgba::WHITE, none: false }),
            1 => Obj::Brush(Brush { color: gray(0xC0), none: false }),
            2 => Obj::Brush(Brush { color: gray(0x80), none: false }),
            3 => Obj::Brush(Brush { color: gray(0x40), none: false }),
            4 => Obj::Brush(Brush { color: Rgba::BLACK, none: false }),
            5 => Obj::Brush(Brush { color: Rgba::BLACK, none: true }),
            6 => Obj::Pen(Pen { color: Rgba::WHITE, width: 0.0, none: false }),
            7 => Obj::Pen(Pen { color: Rgba::BLACK, width: 0.0, none: false }),
            8 => Obj::Pen(Pen { color: Rgba::BLACK, width: 0.0, none: true }),
            _ => Obj::Other,
        };
        self.select(o);
    }

    fn fill_and_stroke(&mut self, path: &BezPath, fill: bool) {
        if self.path.is_some() {
            if let Some(p) = &mut self.path {
                p.extend(path.iter());
            }
            return;
        }
        let m = self.xf();
        if fill && !self.dc.brush.none {
            self.ctx.set_transform(m);
            self.ctx.set_fill_rule(if self.dc.even_odd { peniko::Fill::EvenOdd } else { peniko::Fill::NonZero });
            self.ctx.set_paint(paint(self.dc.brush.color));
            self.ctx.fill_path(path);
            self.ctx.set_fill_rule(peniko::Fill::NonZero);
            self.drew = true;
        }
        self.stroke(path, m);
    }

    fn stroke(&mut self, path: &BezPath, m: Affine) {
        if self.dc.pen.none {
            return;
        }
        // Stroke in output pixels so a pen keeps its width under non-uniform scales.
        let k = (m.determinant().abs()).sqrt();
        let w = if self.dc.pen.width > 0.0 { (self.dc.pen.width * k).max(1.0) } else { 1.0 };
        let mut p = path.clone();
        p.apply_affine(m);
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.set_paint(paint(self.dc.pen.color));
        self.ctx.set_stroke(kurbo::Stroke::new(w).with_join(kurbo::Join::Miter));
        crate::stroke::paint(self.ctx, &p);
        self.drew = true;
    }

    fn poly(&mut self, pts: &[Point], close: bool) {
        if pts.len() < 2 {
            return;
        }
        let mut p = BezPath::new();
        p.move_to(pts[0]);
        for q in &pts[1..] {
            p.line_to(*q);
        }
        if close {
            p.close_path();
            self.fill_and_stroke(&p, true);
        } else {
            self.fill_and_stroke(&p, false);
        }
    }

    fn line_to(&mut self, q: Point) {
        let mut p = BezPath::new();
        p.move_to(self.dc.pos);
        p.line_to(q);
        self.dc.pos = q;
        if let Some(open) = &mut self.path {
            if open.elements().is_empty() {
                open.move_to(p.elements().first().and_then(|e| e.end_point()).unwrap_or(q));
            }
            open.line_to(q);
            return;
        }
        let m = self.xf();
        self.stroke(&p, m);
    }

    fn rect_shape(&mut self, r: Rect, ellipse: bool) {
        let r = Rect::new(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
        let p = if ellipse { kurbo::Ellipse::from_rect(r).to_path(0.1) } else { r.to_path(0.1) };
        self.fill_and_stroke(&p, true);
    }

    /// A raster operation without a source over logical `r`: the brush (`PATCOPY`), black or
    /// white; operations that read the destination are left out.
    fn pattern_blt(&mut self, r: Rect, rop: u32) {
        let color = match rop {
            0x00F0_0021 if !self.dc.brush.none => self.dc.brush.color,
            0x0000_0042 => Rgba::BLACK,
            0x00FF_0062 => Rgba::WHITE,
            _ => return,
        };
        let m = self.xf();
        self.ctx.set_transform(m);
        self.ctx.set_paint(paint(color));
        self.ctx.fill_rect(&r);
        self.drew = true;
    }

    /// Draw `text` with the current font at logical `at` (or the current position), each
    /// character advanced by `dx` (logical units) when given.
    fn text(&mut self, at: Point, text: &[char], dx: &[f64]) {
        if text.is_empty() {
            return;
        }
        let font = self.dc.font.clone();
        let style = match (font.weight >= 600, font.italic) {
            (true, true) => "Bold Italic",
            (true, false) => "Bold",
            (false, true) => "Italic",
            (false, false) => "Regular",
        };
        let db = FontDb::global();
        let face = db.face(&font.face, style);
        let upem = face.units_per_em().max(1.0);
        let (asc, desc) = face.vertical_metrics();
        // A positive height is the cell (ascent + descent); a negative one the em.
        let em = if font.height < 0.0 {
            -font.height
        } else if font.height > 0.0 {
            font.height * upem / (asc + desc).max(1.0)
        } else {
            12.0
        };
        let k = em / upem;
        let glyphs: Vec<(Arc<FontFace>, u32, f64)> = text
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let (f, g) = if face.covers(c) {
                    (face.clone(), face.glyph_for(c))
                } else if let Some(f) = db.fallback_for(c, face.id()) {
                    let g = f.glyph_for(c);
                    (f, g)
                } else {
                    (face.clone(), 0)
                };
                let adv = dx.get(i).copied().unwrap_or_else(|| f.advance(g) * em / f.units_per_em().max(1.0));
                (f, g, adv)
            })
            .collect();
        let width: f64 = glyphs.iter().map(|g| g.2).sum();
        let align = self.dc.text_align;
        let update_cp = align & 1 != 0;
        let origin = if update_cp { self.dc.pos } else { at };
        let x0 = match align & 6 {
            2 => -width,
            6 => -width / 2.0,
            _ => 0.0,
        };
        let y0 = match align & 24 {
            24 => 0.0,
            8 => -desc * k,
            _ => asc * k,
        };
        if update_cp {
            self.dc.pos = Point::new(origin.x + width, origin.y);
        }
        let m = self.xf();
        // Glyphs stay upright whatever the mapping's signs: lay the run out in logical units,
        // then place each glyph in output pixels.
        let (sx, sy) =
            (Point::new(m.as_coeffs()[0], m.as_coeffs()[1]).to_vec2().hypot(), Point::new(m.as_coeffs()[2], m.as_coeffs()[3]).to_vec2().hypot());
        let rot = Affine::rotate(-font.escapement.to_radians() / 10.0);
        let color = paint(self.dc.text_color);
        let mut x = x0;
        for (f, g, adv) in glyphs {
            let local = rot * Point::new(x, y0);
            let p = m * (origin + local.to_vec2());
            x += adv;
            if g == 0 {
                continue;
            }
            let outline = db.outline(&f, g);
            if outline.elements().is_empty() {
                continue;
            }
            let gk = em / f.units_per_em().max(1.0);
            let skew = if font.italic && !f.italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
            let t = Affine::translate(p.to_vec2())
                * Affine::rotate(-font.escapement.to_radians() / 10.0)
                * Affine::scale_non_uniform(gk * sx, gk * sy)
                * skew;
            self.ctx.set_transform(t);
            self.ctx.set_paint(color);
            self.ctx.fill_path(&outline);
            self.drew = true;
        }
    }

    /// Draw a device-independent bitmap (`bmi` header and colour table, then `bits`) into the
    /// logical rectangle `dest`, taking the source rectangle `src` (pixels, top-down).
    fn dib(&mut self, bmi: &[u8], bits: &[u8], dest: Rect, src: Option<Rect>, rop: u32) {
        let Some(mut img) = dib_image(bmi, bits) else { return };
        // Masked icons draw a mask that clears (AND) and then colours that add (OR, XOR): on a
        // transparent canvas, keep the mask's dark pixels and the colours' non-black ones.
        let clear = match rop {
            0x0088_00C6 => Some([255u8, 255, 255]),
            0x00EE_0086 | 0x0066_0046 => Some([0u8, 0, 0]),
            _ => None,
        };
        if let Some(c) = clear {
            for p in img.pixels_mut() {
                if p.0[..3] == c {
                    p.0[3] = 0;
                }
            }
        }
        let (iw, ih) = (img.width() as f64, img.height() as f64);
        let Some(pm) = crate::images::pixmap_of(&img) else { return };
        let src = src.filter(|s| s.width() > 0.0 && s.height() > 0.0).unwrap_or(Rect::new(0.0, 0.0, iw, ih));
        let m = self.xf();
        let t = m
            * Affine::translate(dest.origin().to_vec2())
            * Affine::scale_non_uniform(dest.width() / src.width(), dest.height() / src.height())
            * Affine::translate(-src.origin().to_vec2());
        self.ctx.set_transform(t);
        self.ctx.set_paint(vello_cpu::Image {
            image: vello_cpu::ImageSource::Pixmap(pm),
            sampler: peniko::ImageSampler { quality: peniko::ImageQuality::High, ..Default::default() },
        });
        self.ctx.fill_rect(&src);
        self.drew = true;
    }

    fn play_wmf(&mut self, b: &[u8]) {
        self.wmf = true;
        let start = if is_placeable(b) { 22 } else { 0 };
        // The frame is in the placeable header's units, which are the window's logical units:
        // map the window onto the frame when the file sets one.
        let frame = wmf_frame(b);
        for (func, p) in wmf_records(b, start) {
            let s = |i: usize| i16_at(p, i * 2).map(f64::from).unwrap_or(0.0);
            match func {
                // SETWINDOWORG / SETWINDOWEXT: re-map the window onto the frame.
                0x020B => {
                    self.dc.win_org = Point::new(s(1), s(0));
                    self.wmf_window(frame);
                }
                0x020C => {
                    self.dc.win_ext = Some((s(1), s(0)));
                    self.wmf_window(frame);
                }
                0x0103 => self.dc.map_mode = u16_at(p, 0).unwrap_or(1) as u32,
                0x0106 => self.dc.even_odd = u16_at(p, 0) != Some(2),
                0x012E => self.dc.text_align = u16_at(p, 0).unwrap_or(0) as u32,
                0x0209 => self.dc.text_color = colorref(u32_at(p, 0).unwrap_or(0)),
                0x001E => self.saved.push(self.dc.clone()),
                0x0127 => self.restore(i16_at(p, 0).unwrap_or(-1) as i32),
                // Object table.
                0x02FA => {
                    let style = u16_at(p, 0).unwrap_or(0);
                    let width = s(1);
                    let color = colorref(u32_at(p, 6).unwrap_or(0));
                    self.add_object(Obj::Pen(Pen { color, width, none: style & 0xF == 5 }));
                }
                0x02FC => {
                    let style = u16_at(p, 0).unwrap_or(0);
                    let color = colorref(u32_at(p, 2).unwrap_or(0));
                    self.add_object(Obj::Brush(Brush { color, none: style == 1 }));
                }
                0x02FB => {
                    let face = p.get(18..50).map(ansi_face).unwrap_or_default();
                    let font = Font {
                        height: s(0),
                        escapement: s(2),
                        weight: s(4) as i32,
                        italic: p.get(10).is_some_and(|v| *v != 0),
                        charset: p.get(13).copied().unwrap_or(0),
                        face,
                    };
                    self.add_object(Obj::Font(font));
                }
                0x00F7 | 0x01F9 | 0x0142 | 0x06FF => self.add_object(Obj::Other),
                0x012D => self.select_index(u16_at(p, 0).unwrap_or(0) as usize),
                0x01F0 => {
                    if let Some(o) = self.objects.get_mut(u16_at(p, 0).unwrap_or(0) as usize) {
                        *o = None;
                    }
                }
                // Drawing.
                0x0214 => self.dc.pos = Point::new(s(1), s(0)),
                0x0213 => self.line_to(Point::new(s(1), s(0))),
                0x0324 | 0x0325 => {
                    let n = (s(0).max(0.0) as usize).min(MAX_POINTS);
                    let pts: Vec<Point> =
                        (0..n).map_while(|i| Some(Point::new(i16_at(p, 2 + i * 4)? as f64, i16_at(p, 4 + i * 4)? as f64))).collect();
                    self.poly(&pts, func == 0x0324);
                }
                0x0538 => {
                    let n = s(0).max(0.0) as usize;
                    let counts: Vec<usize> = (0..n).map(|i| s(1 + i).max(0.0) as usize).collect();
                    let mut at = 2 + n * 2;
                    let mut path = BezPath::new();
                    for c in counts {
                        let pts: Vec<Point> = (0..c.min(MAX_POINTS))
                            .map_while(|i| Some(Point::new(i16_at(p, at + i * 4)? as f64, i16_at(p, at + 2 + i * 4)? as f64)))
                            .collect();
                        at += c * 4;
                        if let Some(first) = pts.first() {
                            path.move_to(*first);
                            for q in &pts[1..] {
                                path.line_to(*q);
                            }
                            path.close_path();
                        }
                    }
                    self.fill_and_stroke(&path, true);
                }
                0x041B => self.rect_shape(Rect::new(s(3), s(2), s(1), s(0)), false),
                0x0418 => self.rect_shape(Rect::new(s(3), s(2), s(1), s(0)), true),
                0x061C => {
                    let r = Rect::new(s(5), s(4), s(3), s(2));
                    let r = Rect::new(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
                    let p = kurbo::RoundedRect::from_rect(r, (s(1).abs().min(s(0).abs()) / 2.0).max(0.0)).to_path(0.1);
                    self.fill_and_stroke(&p, true);
                }
                0x061D => {
                    // PATBLT: rop, height, width, y, x.
                    self.pattern_blt(Rect::new(s(5), s(4), s(5) + s(3), s(4) + s(2)), u32_at(p, 0).unwrap_or(0));
                }
                0x0416 => self.intersect_clip(Rect::new(s(3), s(2), s(1), s(0))),
                0x0521 => {
                    let n = s(0).max(0.0) as usize;
                    let Some(bytes) = p.get(2..2 + n) else { continue };
                    let at = 2 + n.div_ceil(2) * 2;
                    let (y, x) = (i16_at(p, at).unwrap_or(0) as f64, i16_at(p, at + 2).unwrap_or(0) as f64);
                    let chars = decode_text(bytes, &self.dc.font);
                    self.text(Point::new(x, y), &chars, &[]);
                }
                0x0A32 => {
                    let (y, x, n, opts) = (s(0), s(1), s(2).max(0.0) as usize, u16_at(p, 6).unwrap_or(0));
                    let mut at = 8;
                    if opts & 0x6 != 0 {
                        if opts & 0x2 != 0 {
                            // ETO_OPAQUE: fill the rectangle with the background colour, skipped.
                        }
                        at += 8;
                    }
                    let Some(bytes) = p.get(at..at + n) else { continue };
                    let dx_at = at + n.div_ceil(2) * 2;
                    let dx: Vec<f64> = (0..n).map_while(|i| i16_at(p, dx_at + i * 2).map(f64::from)).collect();
                    let chars = decode_text(bytes, &self.dc.font);
                    let dx = if dx.len() == n && chars.len() == n { dx } else { vec![] };
                    self.text(Point::new(x, y), &chars, &dx);
                }
                0x0B41 | 0x0F43 => {
                    // DIBSTRETCHBLT: rop(4), srcH, srcW, ySrc, xSrc, destH, destW, yDest, xDest, DIB.
                    // STRETCHDIB: rop(4), usage(2), then the same fields and the DIB.
                    let o = if func == 0x0F43 { 6 } else { 4 };
                    let v = |i: usize| i16_at(p, o + i * 2).map(f64::from).unwrap_or(0.0);
                    let (sh, sw, sy, sx, dh, dw, dy, dx) = (v(0), v(1), v(2), v(3), v(4), v(5), v(6), v(7));
                    let Some(dib) = p.get(o + 16..) else { continue };
                    let Some((bmi, bits)) = split_dib(dib) else { continue };
                    self.dib(bmi, bits, Rect::new(dx, dy, dx + dw, dy + dh), Some(Rect::new(sx, sy, sx + sw, sy + sh)), u32_at(p, 0).unwrap_or(0));
                }
                _ => {}
            }
        }
    }

    /// Map the current WMF window onto the frame (the placeable bounds, or the window itself).
    fn wmf_window(&mut self, frame: Option<Rect>) {
        let (Some((ex, ey)), Some(frame)) = (self.dc.win_ext, frame) else { return };
        if ex == 0.0 || ey == 0.0 {
            return;
        }
        let o = self.dc.win_org;
        self.dc.world = Affine::translate(frame.origin().to_vec2())
            * Affine::scale_non_uniform(frame.width() / ex, frame.height() / ey)
            * Affine::translate(-o.to_vec2());
    }

    fn restore(&mut self, n: i32) {
        let k = if n < 0 { (-n) as usize } else { self.saved.len().saturating_sub(n.max(0) as usize).max(1) };
        let mut dc = None;
        for _ in 0..k {
            dc = self.saved.pop().or(dc);
        }
        if let Some(d) = dc {
            let clip = d.clip;
            self.dc = d;
            self.set_clip(clip);
        }
    }

    fn play_emf(&mut self, b: &[u8]) {
        let mut i = 0usize;
        for _ in 0..MAX_RECORDS {
            let (Some(t), Some(size)) = (u32_at(b, i), u32_at(b, i + 4)) else { break };
            let size = size as usize;
            if size < 8 || i + size > b.len() {
                break;
            }
            let r = &b[i..i + size];
            self.emf_record(t, r);
            if t == 14 {
                break;
            }
            i += size;
        }
    }

    fn emf_record(&mut self, t: u32, r: &[u8]) {
        let i = |o: usize| i32_at(r, o).map(f64::from).unwrap_or(0.0);
        let u = |o: usize| u32_at(r, o).unwrap_or(0);
        let pt16 = |o: usize, n: usize| -> Vec<Point> {
            (0..n.min(MAX_POINTS)).map_while(|k| Some(Point::new(i16_at(r, o + k * 4)? as f64, i16_at(r, o + 2 + k * 4)? as f64))).collect()
        };
        let pt32 = |o: usize, n: usize| -> Vec<Point> {
            (0..n.min(MAX_POINTS)).map_while(|k| Some(Point::new(i32_at(r, o + k * 8)? as f64, i32_at(r, o + 4 + k * 8)? as f64))).collect()
        };
        match t {
            9 => self.dc.win_ext = Some((i(8), i(12))),
            10 => self.dc.win_org = Point::new(i(8), i(12)),
            11 => self.dc.vp_ext = Some((i(8), i(12))),
            12 => self.dc.vp_org = Point::new(i(8), i(12)),
            17 => self.dc.map_mode = u(8),
            19 => self.dc.even_odd = u(8) != 2,
            22 => self.dc.text_align = u(8),
            24 => self.dc.text_color = colorref(u(8)),
            33 => self.saved.push(self.dc.clone()),
            34 => self.restore(i32_at(r, 8).unwrap_or(-1)),
            35 => {
                if let Some(x) = xform(r, 8) {
                    self.dc.world = x;
                }
            }
            36 => {
                let x = xform(r, 8);
                match (u(32), x) {
                    (1, _) => self.dc.world = Affine::IDENTITY,
                    (2, Some(x)) => self.dc.world *= x,
                    (3, Some(x)) => self.dc.world = x * self.dc.world,
                    (4, Some(x)) => self.dc.world = x,
                    _ => {}
                }
            }
            37 => {
                let h = u(8);
                if h & 0x8000_0000 != 0 {
                    self.select_stock(h & 0x7FFF_FFFF);
                } else {
                    self.select_index(h as usize);
                }
            }
            38 => {
                let style = u(12);
                let width = i(16);
                self.put_object(u(8) as usize, Obj::Pen(Pen { color: colorref(u(24)), width, none: style & 0xF == 5 }));
            }
            95 => {
                let style = u(28);
                let width = u(32) as f64;
                let brush_style = u(36);
                self.put_object(u(8) as usize, Obj::Pen(Pen { color: colorref(u(40)), width, none: style & 0xF == 5 || brush_style == 1 }));
            }
            39 => self.put_object(u(8) as usize, Obj::Brush(Brush { color: colorref(u(16)), none: u(12) == 1 })),
            93 | 94 => self.put_object(u(8) as usize, Obj::Brush(Brush { color: Rgba::rgb(0x80, 0x80, 0x80), none: false })),
            82 => {
                let face: String = {
                    let units: Vec<u16> = (0..32).map_while(|k| u16_at(r, 40 + k * 2)).take_while(|c| *c != 0).collect();
                    String::from_utf16_lossy(&units)
                };
                let font = Font {
                    height: i(12),
                    escapement: i(20),
                    weight: i32_at(r, 28).unwrap_or(400),
                    italic: r.get(32).is_some_and(|v| *v != 0),
                    charset: r.get(35).copied().unwrap_or(0),
                    face,
                };
                self.put_object(u(8) as usize, Obj::Font(font));
            }
            40 => {
                if let Some(o) = self.objects.get_mut(u(8) as usize) {
                    *o = None;
                }
            }
            27 => self.dc.pos = Point::new(i(8), i(12)),
            54 => self.line_to(Point::new(i(8), i(12))),
            42 | 43 => self.rect_shape(Rect::new(i(8), i(12), i(16), i(20)), t == 42),
            44 => {
                let rr = Rect::new(i(8), i(12), i(16), i(20));
                let rr = Rect::new(rr.x0.min(rr.x1), rr.y0.min(rr.y1), rr.x0.max(rr.x1), rr.y0.max(rr.y1));
                let p = kurbo::RoundedRect::from_rect(rr, (i(24).abs().min(i(28).abs()) / 2.0).max(0.0)).to_path(0.1);
                self.fill_and_stroke(&p, true);
            }
            // Polygons and polylines, 32- and 16-bit points.
            3 | 4 | 86 | 87 => {
                let n = u(24) as usize;
                let pts = if t < 10 { pt32(28, n) } else { pt16(28, n) };
                self.poly(&pts, t == 3 || t == 86);
            }
            6 | 89 => {
                let n = u(24) as usize;
                let pts = if t == 6 { pt32(28, n) } else { pt16(28, n) };
                for q in pts {
                    self.line_to(q);
                }
            }
            2 | 5 | 85 | 88 => {
                let n = u(24) as usize;
                let pts = if t < 10 { pt32(28, n) } else { pt16(28, n) };
                let to = t == 5 || t == 88;
                let mut p = BezPath::new();
                let mut rest = &pts[..];
                if to {
                    p.move_to(self.dc.pos);
                } else if let Some((first, more)) = pts.split_first() {
                    p.move_to(*first);
                    rest = more;
                }
                let curves = rest.as_chunks::<3>().0;
                for c in curves {
                    p.curve_to(c[0], c[1], c[2]);
                }
                if let Some(last) = curves.last() {
                    self.dc.pos = last[2];
                }
                if let Some(open) = &mut self.path {
                    if to && open.elements().is_empty() {
                        open.move_to(self.dc.pos);
                    }
                    let skip = if to { 1 } else { 0 };
                    open.extend(p.iter().skip(skip));
                } else {
                    let m = self.xf();
                    self.stroke(&p, m);
                }
            }
            7 | 8 | 90 | 91 => {
                let n = u(24) as usize;
                let counts: Vec<usize> = (0..n.min(MAX_POINTS)).map_while(|k| u32_at(r, 32 + k * 4).map(|c| c as usize)).collect();
                let mut at = 32 + counts.len() * 4;
                let wide = t == 7 || t == 8;
                let mut path = BezPath::new();
                for c in counts {
                    let pts = if wide { pt32(at, c) } else { pt16(at, c) };
                    at += c * if wide { 8 } else { 4 };
                    if let Some(first) = pts.first() {
                        path.move_to(*first);
                        for q in &pts[1..] {
                            path.line_to(*q);
                        }
                        if t == 8 || t == 91 {
                            path.close_path();
                        }
                    }
                }
                self.fill_and_stroke(&path, t == 8 || t == 91);
            }
            // Paths.
            59 => self.path = Some(BezPath::new()),
            60 => self.done_path = self.path.take(),
            61 => {
                if let Some(p) = &mut self.path {
                    p.close_path();
                }
            }
            62..=64 => {
                if let Some(p) = self.done_path.take() {
                    let m = self.xf();
                    if t != 64 && !self.dc.brush.none {
                        self.ctx.set_transform(m);
                        self.ctx.set_fill_rule(if self.dc.even_odd { peniko::Fill::EvenOdd } else { peniko::Fill::NonZero });
                        self.ctx.set_paint(paint(self.dc.brush.color));
                        self.ctx.fill_path(&p);
                        self.ctx.set_fill_rule(peniko::Fill::NonZero);
                        self.drew = true;
                    }
                    if t != 62 {
                        self.stroke(&p, m);
                    }
                }
            }
            68 => self.path = None,
            // Clipping.
            30 => self.intersect_clip(Rect::new(i(8), i(12), i(16), i(20))),
            75 => {
                let (cb, mode) = (u(8) as usize, u(12));
                if mode == 5 && cb == 0 {
                    self.set_clip(None);
                } else if let Some(rgn) = r.get(16..16 + cb) {
                    // RGNDATA: the region's bounds (device units) stand for it.
                    let b = |o: usize| i32_at(rgn, o).map(f64::from).unwrap_or(0.0);
                    if rgn.len() >= 32 {
                        let dev = Rect::new(b(16), b(20), b(24), b(28));
                        let out = self.out.transform_rect_bbox(dev);
                        self.set_clip(Some(out));
                    }
                }
            }
            // Text.
            83 | 84 => {
                let n = u(44) as usize;
                let off = u(48) as usize;
                let opts = u(52);
                let off_dx = u(72) as usize;
                let at = Point::new(i(36), i(40));
                let chars: Vec<char> = if t == 84 {
                    let units: Vec<u16> = (0..n.min(MAX_POINTS)).map_while(|k| u16_at(r, off + k * 2)).collect();
                    char::decode_utf16(units).map(|c| c.unwrap_or('\u{FFFD}')).collect()
                } else {
                    r.get(off..off + n).map(|bytes| decode_text(bytes, &self.dc.font)).unwrap_or_default()
                };
                let step = if opts & 0x2000 != 0 { 8 } else { 4 };
                let dx: Vec<f64> =
                    if off_dx > 0 { (0..chars.len()).map_while(|k| i32_at(r, off_dx + k * step).map(f64::from)).collect() } else { vec![] };
                let dx = if dx.len() == chars.len() { dx } else { vec![] };
                self.text(at, &chars, &dx);
            }
            // Bitmaps.
            81 => {
                let (xd, yd, xs, ys, cxs, cys) = (i(24), i(28), i(32), i(36), i(40), i(44));
                let (ob, cbb, obits, cbits) = (u(48) as usize, u(52) as usize, u(56) as usize, u(60) as usize);
                let (cxd, cyd) = (i(72), i(76));
                if let (Some(bmi), Some(bits)) = (r.get(ob..ob + cbb), r.get(obits..obits + cbits)) {
                    self.dib(bmi, bits, Rect::new(xd, yd, xd + cxd, yd + cyd), Some(Rect::new(xs, ys, xs + cxs, ys + cys)), u(68));
                }
            }
            76 | 77 | 114 => {
                let (xd, yd, cxd, cyd) = (i(24), i(28), i(32), i(36));
                let (xs, ys) = (i(44), i(48));
                let base = 100;
                let (ob, cbb, obits, cbits) = (u(base - 16) as usize, u(base - 12) as usize, u(base - 8) as usize, u(base - 4) as usize);
                let (cxs, cys) = if t == 76 { (cxd, cyd) } else { (i(base), i(base + 4)) };
                let rop = if t == 114 { 0x00CC_0020 } else { u(40) };
                if cbb == 0 {
                    // No source bitmap: a pattern operation with the brush.
                    self.pattern_blt(Rect::new(xd, yd, xd + cxd, yd + cyd), rop);
                } else if let (Some(bmi), Some(bits)) = (r.get(ob..ob + cbb), r.get(obits..obits + cbits)) {
                    self.dib(bmi, bits, Rect::new(xd, yd, xd + cxd, yd + cyd), Some(Rect::new(xs, ys, xs + cxs, ys + cys)), rop);
                }
            }
            _ => {}
        }
    }
}

fn xform(r: &[u8], o: usize) -> Option<Affine> {
    Some(Affine::new([f32_at(r, o)?, f32_at(r, o + 4)?, f32_at(r, o + 8)?, f32_at(r, o + 12)?, f32_at(r, o + 16)?, f32_at(r, o + 20)?]))
}

fn paint(c: Rgba) -> peniko::Color {
    peniko::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// A `COLORREF` (`0x00BBGGRR`; palette flags ignored).
fn colorref(v: u32) -> Rgba {
    Rgba::rgb((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8)
}

/// A face name from a `LOGFONT` in the system code page (ASCII names; others keep their ASCII
/// part, which never names a font this finds anyway).
fn ansi_face(b: &[u8]) -> String {
    b.iter().take_while(|c| **c != 0).filter(|c| c.is_ascii()).map(|c| *c as char).collect()
}

/// Text bytes in the font's character set: the Symbol font's own encoding, Windows-1252 for the
/// Western sets; double-byte sets keep their ASCII characters and show the rest as nothing.
fn decode_text(bytes: &[u8], font: &Font) -> Vec<char> {
    let symbol = font.charset == 2 || font.face.eq_ignore_ascii_case("Symbol");
    bytes
        .iter()
        .map(|&c| {
            if symbol {
                symbol_char(c)
            } else if c < 0x80 {
                c as char
            } else if font.charset == 0 {
                cp1252(c)
            } else {
                '\u{0}'
            }
        })
        .collect()
}

/// Windows-1252 bytes 0x80–0xFF.
fn cp1252(c: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018,
        0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    match c {
        0x80..=0x9F => char::from_u32(HIGH[(c - 0x80) as usize] as u32).unwrap_or('\u{FFFD}'),
        _ => c as char,
    }
}

/// The Unicode character for a byte of the Symbol font's encoding (Greek letters, mathematical
/// operators, arrows and brackets); other bytes as themselves.
fn symbol_char(c: u8) -> char {
    const GREEK_UPPER: &str = "\u{391}\u{392}\u{3A7}\u{394}\u{395}\u{3A6}\u{393}\u{397}\u{399}\u{3D1}\u{39A}\u{39B}\u{39C}\u{39D}\u{39F}\u{3A0}\u{398}\u{3A1}\u{3A3}\u{3A4}\u{3A5}\u{3C2}\u{3A9}\u{39E}\u{3A8}\u{396}";
    const GREEK_LOWER: &str = "\u{3B1}\u{3B2}\u{3C7}\u{3B4}\u{3B5}\u{3C6}\u{3B3}\u{3B7}\u{3B9}\u{3D5}\u{3BA}\u{3BB}\u{3BC}\u{3BD}\u{3BF}\u{3C0}\u{3B8}\u{3C1}\u{3C3}\u{3C4}\u{3C5}\u{3D6}\u{3C9}\u{3BE}\u{3C8}\u{3B6}";
    let u = match c {
        b'A'..=b'Z' => return GREEK_UPPER.chars().nth((c - b'A') as usize).unwrap_or(c as char),
        b'a'..=b'z' => return GREEK_LOWER.chars().nth((c - b'a') as usize).unwrap_or(c as char),
        0x22 => 0x2200,
        0x24 => 0x2203,
        0x27 => 0x220B,
        0x2A => 0x2217,
        0x2D => 0x2212,
        0x40 => 0x2245,
        0x5C => 0x2234,
        0x5E => 0x22A5,
        0x60 => 0xF8E5,
        0x7E => 0x223C,
        0xA1 => 0x03D2,
        0xA2 => 0x2032,
        0xA3 => 0x2264,
        0xA4 => 0x2044,
        0xA5 => 0x221E,
        0xA6 => 0x0192,
        0xA7 => 0x2663,
        0xA8 => 0x2666,
        0xA9 => 0x2665,
        0xAA => 0x2660,
        0xAB => 0x2194,
        0xAC => 0x2190,
        0xAD => 0x2191,
        0xAE => 0x2192,
        0xAF => 0x2193,
        0xB0 => 0x00B0,
        0xB1 => 0x00B1,
        0xB2 => 0x2033,
        0xB3 => 0x2265,
        0xB4 => 0x00D7,
        0xB5 => 0x221D,
        0xB6 => 0x2202,
        0xB7 => 0x2022,
        0xB8 => 0x00F7,
        0xB9 => 0x2260,
        0xBA => 0x2261,
        0xBB => 0x2248,
        0xBC => 0x2026,
        0xC0 => 0x2135,
        0xC4 => 0x2297,
        0xC5 => 0x2295,
        0xC6 => 0x2205,
        0xC7 => 0x2229,
        0xC8 => 0x222A,
        0xC9 => 0x2283,
        0xCA => 0x2287,
        0xCB => 0x2284,
        0xCC => 0x2282,
        0xCD => 0x2286,
        0xCE => 0x2208,
        0xCF => 0x2209,
        0xD0 => 0x2220,
        0xD1 => 0x2207,
        0xD5 => 0x220F,
        0xD6 => 0x221A,
        0xD7 => 0x22C5,
        0xD8 => 0x00AC,
        0xD9 => 0x2227,
        0xDA => 0x2228,
        0xDB => 0x21D4,
        0xDC => 0x21D0,
        0xDD => 0x21D1,
        0xDE => 0x21D2,
        0xDF => 0x21D3,
        0xE0 => 0x25CA,
        0xE1 => 0x2329,
        0xE5 => 0x2211,
        0xE6 => 0x239B,
        0xE7 => 0x239C,
        0xE8 => 0x239D,
        0xE9 => 0x23A1,
        0xEA => 0x23A2,
        0xEB => 0x23A3,
        0xEC => 0x23A7,
        0xED => 0x23A8,
        0xEE => 0x23A9,
        0xEF => 0x23AA,
        0xF1 => 0x232A,
        0xF2 => 0x222B,
        0xF3 => 0x2320,
        0xF4 => 0x23AE,
        0xF5 => 0x2321,
        0xF6 => 0x239E,
        0xF7 => 0x239F,
        0xF8 => 0x23A0,
        0xF9 => 0x23A4,
        0xFA => 0x23A5,
        0xFB => 0x23A6,
        0xFC => 0x23AB,
        0xFD => 0x23AC,
        0xFE => 0x23AD,
        _ => c as u32,
    };
    char::from_u32(u).unwrap_or(c as char)
}

/// A packed DIB (header, colour table, bits) split into its header with colour table and its bits.
fn split_dib(dib: &[u8]) -> Option<(&[u8], &[u8])> {
    let hs = u32_at(dib, 0)? as usize;
    let (bpp, used, compression) =
        if hs == 12 { (u16_at(dib, 10)? as usize, 0, 0) } else { (u16_at(dib, 14)? as usize, u32_at(dib, 32)? as usize, u32_at(dib, 16)?) };
    let entry = if hs == 12 { 3 } else { 4 };
    let colors = if bpp <= 8 { if used > 0 { used.min(256) } else { 1 << bpp } } else { 0 };
    let masks = if compression == 3 && hs == 40 { 12 } else { 0 };
    let n = hs.checked_add(colors * entry)?.checked_add(masks)?;
    if n > dib.len() {
        return None;
    }
    Some(dib.split_at(n))
}

/// Decode a DIB by giving it the file header a `.bmp` file has.
fn dib_image(bmi: &[u8], bits: &[u8]) -> Option<image::RgbaImage> {
    if bmi.len() < 12 || bits.is_empty() || bmi.len() + bits.len() > 256 << 20 {
        return None;
    }
    let total = 14 + bmi.len() + bits.len();
    let mut f = Vec::with_capacity(total);
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&(total as u32).to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    f.extend_from_slice(&((14 + bmi.len()) as u32).to_le_bytes());
    f.extend_from_slice(bmi);
    f.extend_from_slice(bits);
    let img = image::load_from_memory_with_format(&f, image::ImageFormat::Bmp).ok()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || (w as u64) * (h as u64) > 100_000_000 {
        return None;
    }
    Some(img.to_rgba8())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A placeable WMF of `records` (function, parameter words) over a 1000 x 500 unit frame.
    fn wmf(records: &[(u16, Vec<u16>)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(0x9AC6_CDD7u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        for v in [0i16, 0, 1000, 500] {
            b.extend(v.to_le_bytes());
        }
        b.extend(1440u16.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        // META_HEADER: type 1, 9 words, version 0x300, size, objects, max record, members.
        for v in [1u16, 9, 0x300, 0, 0, 16, 0, 0, 0] {
            b.extend(v.to_le_bytes());
        }
        let mut all = vec![(0x020Bu16, vec![0u16, 0]), (0x020C, vec![500, 1000])];
        all.extend_from_slice(records);
        all.push((0, vec![]));
        for (f, p) in all {
            b.extend((3 + p.len() as u32).to_le_bytes());
            b.extend(f.to_le_bytes());
            for w in p {
                b.extend(w.to_le_bytes());
            }
        }
        b
    }

    fn px(img: &image::RgbaImage, x: u32, y: u32) -> [u8; 4] {
        img.get_pixel(x, y).0
    }

    #[test]
    fn a_wmf_polygon_fills_with_the_selected_brush() {
        let red = [0x00FF, 0x0000];
        let b = wmf(&[
            (0x02FC, vec![0, red[0], red[1], 0]),
            (0x012D, vec![0]),
            (0x02FA, vec![5, 0, 0, 0, 0]),
            (0x012D, vec![1]),
            // A rectangle over the left half (x 0..500).
            (0x0324, vec![4, 0, 0, 500, 0, 500, 500, 0, 500]),
        ]);
        assert!(is_metafile(&b));
        let img = rasterize(&b, 200.0).expect("drawn");
        assert_eq!((img.width(), img.height()), (200, 100));
        assert_eq!(px(&img, 50, 50), [255, 0, 0, 255]);
        // The right half stays transparent.
        assert_eq!(px(&img, 150, 50)[3], 0);
    }

    #[test]
    fn wmf_text_draws_with_per_character_advances() {
        // CREATEFONTINDIRECT: height -300, width, escapement, orientation, weight 400, italic etc.,
        // face "Arial".
        let mut font = vec![(-300i16) as u16, 0, 0, 0, 400, 0, 0, 0, 0];
        let mut face = b"Arial".to_vec();
        face.resize(32, 0);
        font.extend(face.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        let b = wmf(&[
            (0x02FB, font),
            (0x012D, vec![0]),
            (0x012E, vec![24]),
            // EXTTEXTOUT at (100, 400): "II" with 600 units between them.
            (0x0A32, vec![400, 100, 2, 0, u16::from_le_bytes(*b"II"), 600, 100]),
        ]);
        let img = rasterize(&b, 1000.0).expect("drawn");
        let ink = |x0: u32, x1: u32| (x0..x1).flat_map(|x| (0..500).map(move |y| (x, y))).filter(|&(x, y)| px(&img, x, y)[3] > 128).count();
        // Ink at both letters, nothing between them.
        assert!(ink(90, 200) > 100, "{}", ink(90, 200));
        assert!(ink(690, 800) > 100);
        assert_eq!(ink(300, 650), 0);
    }

    #[test]
    fn the_symbol_font_maps_to_greek_and_operators() {
        let f = Font { height: -10.0, escapement: 0.0, weight: 400, italic: false, charset: 2, face: "Symbol".into() };
        assert_eq!(decode_text(b"Dpa-", &f), ['\u{394}', '\u{3C0}', '\u{3B1}', '\u{2212}']);
        let w = Font { charset: 0, face: "Arial".into(), ..f };
        assert_eq!(decode_text(&[b'A', 0x93], &w), ['A', '\u{201C}']);
    }

    #[test]
    fn hostile_metafiles_draw_nothing() {
        assert!(rasterize(b"", 100.0).is_none());
        assert!(rasterize(&0x9AC6_CDD7u32.to_le_bytes(), 100.0).is_none());
        let mut b = wmf(&[(0x0324, vec![u16::MAX, 0, 0])]);
        b.truncate(b.len() - 2);
        let _ = rasterize(&b, 100.0);
        // A zero-sized frame.
        let mut z = wmf(&[]);
        z[10..14].copy_from_slice(&[0; 4]);
        let _ = rasterize(&z, 100.0);
    }
}
