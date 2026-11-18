//! The comparator: one rendered slide against its reference render, both opaque RGB at the same
//! size. Every choice that changes a score is fixed here and named in [`manifest`]; change any of
//! them only together with [`METRIC_VERSION`].

use serde_json::{Value, json};

/// Version of the metric definitions below. Scores of different versions are not comparable.
/// `/2` adds the layout-level gate ([`compare_layout`]); every `/1` metric is unchanged.
pub const METRIC_VERSION: &str = "slide-visual-metric/2";
/// SSIM a slide needs to pass.
pub const PASS_SSIM: f64 = 0.999;
/// Largest share of pixels whose largest channel difference exceeds [`DIFF_THRESHOLD`] a passing
/// slide may have.
pub const PASS_DIFF_RATIO: f64 = 0.001;
/// Channel difference (0–255) above which a pixel counts as different.
pub const DIFF_THRESHOLD: u8 = 8;
/// SSIM window: Gaussian, this many taps per side, this sigma.
pub const WINDOW: usize = 11;
pub const SIGMA: f64 = 1.5;
pub const K1: f64 = 0.01;
pub const K2: f64 = 0.03;
/// Worst-tile SSIM a slide needs to pass the local gate (proposal: the whole-frame gate cannot see
/// a single wrong glyph on a 1280x720 slide).
pub const PASS_TILE_SSIM: f64 = 0.95;
/// Sigma (pixels) of the Gaussian blur both images get before the layout-level metrics: it hides
/// glyph-outline differences between a font and its substitute while lines, words and boxes
/// still have to be where the reference has them.
pub const LAYOUT_BLUR_SIGMA: f64 = 2.0;
/// Taps on each side of the blur kernel's centre (3 sigma).
pub const LAYOUT_BLUR_RADIUS: usize = 6;
/// Side of the square tiles the SSIM map is averaged over for the worst-region diagnostic.
pub const TILE: usize = 32;

/// The metric manifest written into every scorecard.
pub fn manifest() -> Value {
    json!({
        "version": METRIC_VERSION,
        "pass": format!("ssim >= {PASS_SSIM} and diff_ratio <= {PASS_DIFF_RATIO}"),
        "pass_local": format!("pass and worst_tile.ssim >= {PASS_TILE_SSIM} (proposed stricter gate: catches single-glyph substitutions the whole-frame gate misses)"),
        "alpha": "both images composited over opaque white before any metric (straight RGBA: c*a + 255*(1-a); the renderer's premultiplied RGBA: c + 255*(1-a)), rounded to 8-bit",
        "size": "the slide is rendered at the reference's pixel size (scale = reference width / slide width in points, output size forced to the reference size); no resampling of either image",
        "ssim": {
            "channel": "luma Y = 0.299 R + 0.587 G + 0.114 B on 8-bit values, no gamma linearization",
            "window": format!("{WINDOW}x{WINDOW} Gaussian, sigma {SIGMA}, normalized, separable"),
            "constants": { "K1": K1, "K2": K2, "L": 255 },
            "edges": "valid windows only (no padding): the mean is over the (w-10) x (h-10) window centres; images smaller than the window use one window over the whole image with uniform weights",
            "downsampling": "none (full resolution)",
            "precision": "f64 accumulation"
        },
        "exact_equal": "share of pixels whose three channels are equal",
        "diff_ratio": format!("share of pixels whose largest absolute channel difference exceeds {DIFF_THRESHOLD}"),
        "max_diff": "largest absolute channel difference over the image",
        "mean_abs_diff": "mean absolute channel difference over all channels and pixels",
        "bbox": "[x0, y0, x1, y1) of the pixels counted in diff_ratio, null when none",
        "worst_tile": format!("the {TILE}x{TILE} tile of the SSIM map (window-centre coordinates offset by 5) with the lowest mean, and that mean"),
        "pass_layout": format!(
            "layout-level gate, reported beside the strict one (which stays the target): both opaque images blurred with a separable Gaussian (sigma {LAYOUT_BLUR_SIGMA} px, {} taps, normalized, edge pixels repeated) per channel and rounded to 8-bit, then ssim and diff_ratio exactly as above on the blurred pair; passes when layout.ssim >= {PASS_SSIM} and layout.diff_ratio <= {PASS_DIFF_RATIO}",
            2 * LAYOUT_BLUR_RADIUS + 1
        ),
    })
}

/// An opaque 8-bit RGB image.
#[derive(Clone, Debug, PartialEq)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgb {
    pub fn new(width: u32, height: u32, fill: [u8; 3]) -> Rgb {
        Rgb { width, height, data: fill.repeat(width as usize * height as usize) }
    }

    /// From straight (unpremultiplied) RGBA, over white.
    pub fn from_straight_rgba(width: u32, height: u32, rgba: &[u8]) -> Rgb {
        let data = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = p[3] as u32;
                [0, 1, 2].map(|i| ((p[i] as u32 * a + 255 * (255 - a) + 127) / 255) as u8)
            })
            .collect();
        Rgb { width, height, data }
    }

    /// From premultiplied RGBA (the renderer's output), over white.
    pub fn from_premultiplied_rgba(width: u32, height: u32, rgba: &[u8]) -> Rgb {
        let data = rgba.as_chunks::<4>().0.iter().flat_map(|p| [0, 1, 2].map(|i| (p[i] as u32 + 255 - p[3] as u32).min(255) as u8)).collect();
        Rgb { width, height, data }
    }

    /// Read a PNG (or any format the `image` crate reads), over white.
    pub fn load(path: &std::path::Path) -> Result<Rgb, String> {
        let img = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
        Ok(Rgb::from_straight_rgba(img.width(), img.height(), img.as_raw()))
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    pub fn set(&mut self, x: u32, y: u32, c: [u8; 3]) {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        self.data[i..i + 3].copy_from_slice(&c);
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        image::save_buffer(path, &self.data, self.width, self.height, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
    }

    /// `left` and `right` next to each other on white (top-aligned), for looking at a pair.
    pub fn side_by_side(left: &Rgb, right: &Rgb) -> Rgb {
        let mut out = Rgb::new(left.width + right.width, left.height.max(right.height), [255; 3]);
        for (img, x0) in [(left, 0), (right, left.width)] {
            let row = img.width as usize * 3;
            for y in 0..img.height as usize {
                let at = (y * out.width as usize + x0 as usize) * 3;
                if let (Some(dst), Some(src)) = (out.data.get_mut(at..at + row), img.data.get(y * row..(y + 1) * row)) {
                    dst.copy_from_slice(src);
                }
            }
        }
        out
    }

    fn luma(&self) -> Vec<f64> {
        self.data.as_chunks::<3>().0.iter().map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64).collect()
    }
}

/// The metrics of one comparison (see [`manifest`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub ssim: f64,
    pub exact_equal: f64,
    pub diff_ratio: f64,
    pub diff_pixels: u64,
    pub max_diff: u8,
    pub mean_abs_diff: f64,
    pub bbox: Option<[u32; 4]>,
    /// `[x0, y0, x1, y1]` and mean SSIM of the worst tile.
    pub worst_tile: Option<([u32; 4], f64)>,
}

impl Metrics {
    pub fn pass(&self) -> bool {
        self.ssim >= PASS_SSIM && self.diff_ratio <= PASS_DIFF_RATIO
    }

    /// [`Metrics::pass`] and no tile below [`PASS_TILE_SSIM`].
    pub fn pass_local(&self) -> bool {
        self.pass() && self.worst_tile.is_none_or(|(_, s)| s >= PASS_TILE_SSIM)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "ssim": round(self.ssim, 6),
            "exact_equal": round(self.exact_equal, 6),
            "diff_ratio": round(self.diff_ratio, 6),
            "diff_pixels": self.diff_pixels,
            "max_diff": self.max_diff,
            "mean_abs_diff": round(self.mean_abs_diff, 4),
            "bbox": self.bbox,
            "worst_tile": self.worst_tile.map(|(r, s)| json!({ "rect": r, "ssim": round(s, 6) })),
            "pass": self.pass(),
            "pass_local": self.pass_local(),
        })
    }
}

pub fn round(v: f64, places: i32) -> f64 {
    let m = 10f64.powi(places);
    (v * m).round() / m
}

/// The layout-level metrics (see [`compare_layout`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMetrics {
    pub ssim: f64,
    pub diff_ratio: f64,
}

impl LayoutMetrics {
    /// The strict thresholds applied to the blurred pair.
    pub fn pass(&self) -> bool {
        self.ssim >= PASS_SSIM && self.diff_ratio <= PASS_DIFF_RATIO
    }

    pub fn to_json(&self) -> Value {
        json!({ "ssim": round(self.ssim, 6), "diff_ratio": round(self.diff_ratio, 6), "pass": self.pass() })
    }
}

/// The layout-level gate: SSIM and diff_ratio of both images after a Gaussian blur
/// ([`LAYOUT_BLUR_SIGMA`]). Glyph outlines of a substitute font mostly vanish in the blur; a line,
/// word or box in the wrong place does not. Errors when the sizes differ.
pub fn compare_layout(reference: &Rgb, actual: &Rgb) -> Result<LayoutMetrics, String> {
    let m = compare(&blur(reference), &blur(actual))?.metrics;
    Ok(LayoutMetrics { ssim: m.ssim, diff_ratio: m.diff_ratio })
}

pub(crate) fn blur_kernel() -> [f64; 2 * LAYOUT_BLUR_RADIUS + 1] {
    let mut k = [0.0; 2 * LAYOUT_BLUR_RADIUS + 1];
    for (i, v) in k.iter_mut().enumerate() {
        let d = i as f64 - LAYOUT_BLUR_RADIUS as f64;
        *v = (-(d * d) / (2.0 * LAYOUT_BLUR_SIGMA * LAYOUT_BLUR_SIGMA)).exp();
    }
    let s: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

/// Separable Gaussian blur of every channel ([`LAYOUT_BLUR_SIGMA`]), edge pixels repeated,
/// rounded back to 8-bit.
pub fn blur(img: &Rgb) -> Rgb {
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 {
        return img.clone();
    }
    let k = blur_kernel();
    let r = LAYOUT_BLUR_RADIUS as isize;
    let at = |i: isize, n: usize| i.clamp(0, n as isize - 1) as usize;
    // Stream adjacent samples so the compiler can vectorize without changing each
    // pixel's tap order, rounding, or edge extension.
    let mut tmp = vec![0.0f64; w * h * 3];
    for y in 0..h {
        let row_start = y * w * 3;
        let interior = LAYOUT_BLUR_RADIUS.min(w)..w.saturating_sub(LAYOUT_BLUR_RADIUS);
        for x in (0..w).filter(|x| !interior.contains(x)) {
            let mut sums = [0.0; 3];
            for (t, kv) in k.iter().enumerate() {
                let sx = at(x as isize + t as isize - r, w);
                let i = row_start + sx * 3;
                for (c, value) in sums.iter_mut().enumerate() {
                    *value += kv * img.data[i + c] as f64;
                }
            }
            tmp[row_start + x * 3..row_start + x * 3 + 3].copy_from_slice(&sums);
        }
        if interior.start < interior.end {
            let count = (interior.end - interior.start) * 3;
            let dst = &mut tmp[row_start + interior.start * 3..row_start + interior.end * 3];
            for (t, kv) in k.iter().enumerate() {
                let start = row_start + t * 3;
                let source = &img.data[start..start + count];
                for (value, source) in dst.iter_mut().zip(source) {
                    *value += kv * *source as f64;
                }
            }
        }
    }
    let mut data = vec![0u8; w * h * 3];
    let mut row = vec![0.0; w * 3];
    for y in 0..h {
        row.fill(0.0);
        for (t, kv) in k.iter().enumerate() {
            let sy = at(y as isize + t as isize - r, h);
            let source = &tmp[sy * w * 3..(sy + 1) * w * 3];
            for (value, source) in row.iter_mut().zip(source) {
                *value += kv * source;
            }
        }
        for (dst, value) in data[y * w * 3..(y + 1) * w * 3].iter_mut().zip(&row) {
            *dst = value.round().clamp(0.0, 255.0) as u8;
        }
    }
    Rgb { width: img.width, height: img.height, data }
}

/// A comparison: the metrics and each pixel's largest channel difference.
pub struct Comparison {
    pub metrics: Metrics,
    pub max_diff: Vec<u8>,
}

/// Compare `actual` with `reference`. Errors when the sizes differ.
pub fn compare(reference: &Rgb, actual: &Rgb) -> Result<Comparison, String> {
    compare_with_ssim(reference, actual, None)
}

/// Reuse verified convolution results while recalculating every pixel difference.
pub(crate) fn compare_with_ssim(reference: &Rgb, actual: &Rgb, cached: Option<(f64, Option<([u32; 4], f64)>)>) -> Result<Comparison, String> {
    if (reference.width, reference.height) != (actual.width, actual.height) {
        return Err(format!("size {}x{} vs reference {}x{}", actual.width, actual.height, reference.width, reference.height));
    }
    let (w, h) = (reference.width as usize, reference.height as usize);
    let n = w.checked_mul(h).ok_or("image dimensions overflow")?;
    if n == 0 {
        return Err("empty image".into());
    }
    let bytes = n.checked_mul(3).ok_or("image dimensions overflow")?;
    if reference.data.len() != bytes || actual.data.len() != bytes {
        return Err("invalid RGB buffer length".into());
    }
    let mut max_diff = vec![0u8; n];
    let (mut equal, mut over, mut sum, mut peak) = (0u64, 0u64, 0u64, 0u8);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, (a, b)) in reference.data.as_chunks::<3>().0.iter().zip(actual.data.as_chunks::<3>().0.iter()).enumerate() {
        let d = [0, 1, 2].map(|c| a[c].abs_diff(b[c]));
        let m = d[0].max(d[1]).max(d[2]);
        max_diff[i] = m;
        sum += d.iter().map(|&v| v as u64).sum::<u64>();
        peak = peak.max(m);
        if m == 0 {
            equal += 1;
        }
        if m > DIFF_THRESHOLD {
            over += 1;
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    let (ssim, worst_tile) = cached.unwrap_or_else(|| ssim(&reference.luma(), &actual.luma(), w, h));
    let metrics = Metrics {
        ssim,
        exact_equal: equal as f64 / n as f64,
        diff_ratio: over as f64 / n as f64,
        diff_pixels: over,
        max_diff: peak,
        mean_abs_diff: sum as f64 / (n * 3) as f64,
        bbox: (over > 0).then_some([x0 as u32, y0 as u32, x1 as u32, y1 as u32]),
        worst_tile,
    };
    Ok(Comparison { metrics, max_diff })
}

pub(crate) fn kernel() -> [f64; WINDOW] {
    let mut k = [0.0; WINDOW];
    let c = (WINDOW / 2) as f64;
    for (i, v) in k.iter_mut().enumerate() {
        let d = i as f64 - c;
        *v = (-(d * d) / (2.0 * SIGMA * SIGMA)).exp();
    }
    let s: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

fn ssim_term(mx: f64, my: f64, vx: f64, vy: f64, cov: f64) -> f64 {
    let c1 = (K1 * 255.0) * (K1 * 255.0);
    let c2 = (K2 * 255.0) * (K2 * 255.0);
    ((2.0 * mx * my + c1) * (2.0 * cov + c2)) / ((mx * mx + my * my + c1) * (vx + vy + c2))
}

/// Mean SSIM of two luma planes and the worst tile of the SSIM map.
fn ssim(a: &[f64], b: &[f64], w: usize, h: usize) -> (f64, Option<([u32; 4], f64)>) {
    if w < WINDOW || h < WINDOW {
        let n = (w * h) as f64;
        let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
        let (mut va, mut vb, mut cv) = (0.0, 0.0, 0.0);
        for (x, y) in a.iter().zip(b) {
            va += (x - ma) * (x - ma);
            vb += (y - mb) * (y - mb);
            cv += (x - ma) * (y - mb);
        }
        return (ssim_term(ma, mb, va / n, vb / n, cv / n), None);
    }
    let k = kernel();
    let (ow, oh) = (w - WINDOW + 1, h - WINDOW + 1);
    // Stream independent pixels through each tap. Every pixel still accumulates taps
    // in the original order; final image and tile reductions also remain row-major.
    // This enables vectorization without changing floating-point scores.
    let mut hz: [Vec<f64>; 5] = std::array::from_fn(|_| vec![0.0; ow * h]);
    for y in 0..h {
        let (ra, rb) = (&a[y * w..(y + 1) * w], &b[y * w..(y + 1) * w]);
        let [ha, hb, haa, hbb, hab] = &mut hz;
        let rows = [ha, hb, haa, hbb, hab].map(|v| &mut v[y * ow..(y + 1) * ow]);
        let [ma, mb, va, vb, cv] = rows;
        for (t, kv) in k.iter().enumerate() {
            for x in 0..ow {
                let (p, q) = (ra[x + t], rb[x + t]);
                ma[x] += kv * p;
                mb[x] += kv * q;
                va[x] += kv * p * p;
                vb[x] += kv * q * q;
                cv[x] += kv * p * q;
            }
        }
    }
    let (tw, th) = (ow.div_ceil(TILE), oh.div_ceil(TILE));
    let mut tiles = vec![(0.0f64, 0u32); tw * th];
    let mut total = 0.0;
    let mut row: [Vec<f64>; 5] = std::array::from_fn(|_| vec![0.0; ow]);
    for y in 0..oh {
        for (dst, plane) in row.iter_mut().zip(&hz) {
            dst.fill(0.0);
            for (t, kv) in k.iter().enumerate() {
                let src = &plane[(y + t) * ow..(y + t + 1) * ow];
                for (value, source) in dst.iter_mut().zip(src) {
                    *value += kv * source;
                }
            }
        }
        for x in 0..ow {
            let s = row.each_ref().map(|r| r[x]);
            let v = ssim_term(s[0], s[1], s[2] - s[0] * s[0], s[3] - s[1] * s[1], s[4] - s[0] * s[1]);
            total += v;
            let tile = &mut tiles[(y / TILE) * tw + x / TILE];
            tile.0 += v;
            tile.1 += 1;
        }
    }
    let half = (WINDOW / 2) as u32;
    let worst = tiles.iter().enumerate().map(|(i, (s, c))| (i, s / *c as f64)).min_by(|p, q| p.1.total_cmp(&q.1)).map(|(i, s)| {
        let (tx, ty) = ((i % tw * TILE) as u32, (i / tw * TILE) as u32);
        ([tx + half, ty + half, (tx + TILE as u32).min(ow as u32) + half, (ty + TILE as u32).min(oh as u32) + half], s)
    });
    (total / (ow * oh) as f64, worst)
}

/// How much of the diff to write as an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffImage {
    None,
    /// The reference faded to light grey with differing pixels in red, at half size.
    Half,
    /// Reference, render and the full-size diff side by side.
    Full,
}

/// The diff image of a comparison (see [`DiffImage`]).
pub fn diff_image(reference: &Rgb, actual: &Rgb, max_diff: &[u8], mode: DiffImage) -> Option<Rgb> {
    let (w, h) = (reference.width, reference.height);
    let faded = |c: [u8; 3]| {
        let y = (0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64) as u32;
        let v = (255 - (255 - y) / 4) as u8;
        [v, v, v]
    };
    let red = |d: u8| {
        let t = d as u32;
        [255, (200u32.saturating_sub(t * 2)) as u8, (200u32.saturating_sub(t * 2)) as u8]
    };
    match mode {
        DiffImage::None => None,
        DiffImage::Half => {
            let (hw, hh) = (w.div_ceil(2), h.div_ceil(2));
            let mut out = Rgb::new(hw, hh, [255; 3]);
            for y in 0..hh {
                for x in 0..hw {
                    let mut d = 0u8;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = (x * 2 + dx, y * 2 + dy);
                        if sx < w && sy < h {
                            d = d.max(max_diff[(sy * w + sx) as usize]);
                        }
                    }
                    out.set(x, y, if d > DIFF_THRESHOLD { red(d) } else { faded(reference.get(x * 2, y * 2)) });
                }
            }
            Some(out)
        }
        DiffImage::Full => {
            let mut out = Rgb::new(w * 3, h, [255; 3]);
            for y in 0..h {
                for x in 0..w {
                    let d = max_diff[(y * w + x) as usize];
                    out.set(x, y, reference.get(x, y));
                    out.set(x + w, y, actual.get(x, y));
                    out.set(x + 2 * w, y, if d > DIFF_THRESHOLD { red(d) } else { faded(reference.get(x, y)) });
                }
            }
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic "slide": white with a few dark bars and a coloured block.
    fn scene(shift: u32, block: [u8; 3], with_bars: bool) -> Rgb {
        let mut img = Rgb::new(320, 180, [255; 3]);
        for y in 40..140 {
            for x in 30..130 {
                img.set(x + shift, y, block);
            }
        }
        if with_bars {
            for (i, y0) in [30u32, 60, 90, 120].iter().enumerate() {
                for y in *y0..y0 + 8 {
                    for x in 170..(250 + i as u32 * 15) {
                        img.set(x + shift, y, [20, 20, 20]);
                    }
                }
            }
        }
        img
    }

    #[test]
    fn identical_images_score_perfectly() {
        let a = scene(0, [200, 60, 40], true);
        let c = compare(&a, &a.clone()).unwrap();
        assert_eq!(c.metrics.ssim, 1.0);
        assert_eq!(c.metrics.exact_equal, 1.0);
        assert_eq!(c.metrics.diff_ratio, 0.0);
        assert_eq!(c.metrics.bbox, None);
        assert!(c.metrics.pass());
    }

    #[test]
    fn a_one_pixel_shift_fails_and_is_located() {
        let (a, b) = (scene(0, [200, 60, 40], true), scene(1, [200, 60, 40], true));
        let m = compare(&a, &b).unwrap().metrics;
        assert!(!m.pass(), "{m:?}");
        assert!(m.ssim < PASS_SSIM && m.ssim > 0.9, "{m:?}");
        let [x0, y0, x1, y1] = m.bbox.unwrap();
        assert!(x0 >= 30 && x1 <= 296 && y0 >= 30 && y1 <= 140, "{:?}", m.bbox);
    }

    #[test]
    fn a_missing_object_fails_with_its_box() {
        let (a, b) = (scene(0, [200, 60, 40], true), scene(0, [200, 60, 40], false));
        let m = compare(&a, &b).unwrap().metrics;
        assert!(!m.pass());
        assert_eq!(m.bbox, Some([170, 30, 295, 128]));
        assert!(m.diff_ratio > 0.05, "{m:?}");
    }

    #[test]
    fn a_colour_change_fails_even_when_luma_barely_moves() {
        // Same luma within a unit, different hue: SSIM (luma) barely moves; the pixel gate catches it.
        let (a, b) = (scene(0, [200, 60, 40], true), scene(0, [96, 105, 101], true));
        let m = compare(&a, &b).unwrap().metrics;
        assert!(m.ssim > 0.999, "{m:?}");
        assert!(m.diff_ratio > 0.1 && !m.pass(), "{m:?}");
        assert_eq!(m.bbox, Some([30, 40, 130, 140]));
    }

    #[test]
    fn a_small_colour_drift_within_the_threshold_passes_the_pixel_gate() {
        let (a, b) = (scene(0, [200, 60, 40], true), scene(0, [205, 62, 44], true));
        let m = compare(&a, &b).unwrap().metrics;
        assert_eq!(m.diff_ratio, 0.0);
        assert!(m.exact_equal < 1.0 && m.max_diff == 5, "{m:?}");
    }

    #[test]
    fn alpha_is_composited_over_white() {
        let straight = Rgb::from_straight_rgba(1, 2, &[0, 0, 0, 0, 255, 0, 0, 128]);
        assert_eq!(straight.data, vec![255, 255, 255, 255, 127, 127]);
        let pre = Rgb::from_premultiplied_rgba(1, 2, &[0, 0, 0, 0, 128, 0, 0, 128]);
        assert_eq!(pre.data, vec![255, 255, 255, 255, 127, 127]);
    }

    #[test]
    fn sizes_must_match() {
        assert!(compare(&Rgb::new(10, 10, [0; 3]), &Rgb::new(10, 11, [0; 3])).is_err());
    }

    #[test]
    fn small_images_use_one_window() {
        let a = Rgb::new(5, 5, [10; 3]);
        assert_eq!(compare(&a, &a.clone()).unwrap().metrics.ssim, 1.0);
    }

    /// Lines of "glyphs" (dark 7x12 cells) on white; `edge` is the grey of each glyph's antialiased
    /// rim (the part of a glyph's outline a substitute font draws differently), `dy` moves every
    /// line down, `skip` leaves out one word.
    fn text_scene(edge: u8, dy: u32, skip: bool) -> Rgb {
        let mut img = Rgb::new(480, 270, [255; 3]);
        for line in 0..6u32 {
            for word in 0..6u32 {
                if skip && line == 2 && word == 3 {
                    continue;
                }
                for g in 0..5u32 {
                    let (x0, y0) = (20 + word * 74 + g * 12, 20 + line * 40 + dy);
                    for y in y0..y0 + 12 {
                        for x in x0..x0 + 7 {
                            let rim = x == x0 || x == x0 + 6 || y == y0 || y == y0 + 11;
                            // The rim differs pixel by pixel the way two fonts' antialiasing does.
                            let v = if rim { if (x + y) % 2 == 0 { edge } else { 255 - edge / 2 } } else { 40 };
                            img.set(x, y, [v; 3]);
                        }
                    }
                }
            }
        }
        img
    }

    #[test]
    fn blur_keeps_flat_images_and_spreads_a_point_symmetrically() {
        let flat = Rgb::new(40, 30, [90, 140, 200]);
        assert_eq!(blur(&flat), flat);
        let mut dot = Rgb::new(41, 41, [255; 3]);
        dot.set(20, 20, [0; 3]);
        let b = blur(&dot);
        assert!(b.get(20, 20)[0] > 240 && b.get(20, 20)[0] < 255, "{:?}", b.get(20, 20));
        assert_eq!(b.get(18, 20), b.get(22, 20));
        assert_eq!(b.get(20, 17), b.get(17, 20));
        assert_eq!(b.get(0, 0), [255; 3]);
    }

    #[test]
    fn identical_images_pass_the_layout_gate() {
        let a = text_scene(120, 0, false);
        let l = compare_layout(&a, &a.clone()).unwrap();
        assert_eq!(l.ssim, 1.0);
        assert_eq!(l.diff_ratio, 0.0);
        assert!(l.pass());
    }

    #[test]
    fn glyph_outline_differences_fail_strict_but_pass_layout() {
        // Same lines and words, different antialiased rims: a substitute font with the right metrics.
        let (a, b) = (text_scene(120, 0, false), text_scene(200, 0, false));
        let strict = compare(&a, &b).unwrap().metrics;
        assert!(!strict.pass(), "{strict:?}");
        let l = compare_layout(&a, &b).unwrap();
        assert!(l.pass(), "{l:?}");
        assert!(l.ssim > strict.ssim, "{l:?} vs {strict:?}");
    }

    #[test]
    fn a_line_shift_fails_the_layout_gate() {
        // Every line 3 px low (a baseline or line-height error).
        let (a, b) = (text_scene(120, 0, false), text_scene(120, 3, false));
        let l = compare_layout(&a, &b).unwrap();
        assert!(!l.pass() && l.diff_ratio > 0.01, "{l:?}");
        // Even a 1 px shift of every line stays visible after the blur.
        let l1 = compare_layout(&a, &text_scene(120, 1, false)).unwrap();
        assert!(!l1.pass(), "{l1:?}");
    }

    #[test]
    fn a_missing_word_fails_the_layout_gate() {
        let (a, b) = (text_scene(120, 0, false), text_scene(120, 0, true));
        let l = compare_layout(&a, &b).unwrap();
        assert!(!l.pass() && l.diff_ratio > PASS_DIFF_RATIO, "{l:?}");
        assert!(compare_layout(&a, &Rgb::new(480, 271, [255; 3])).is_err());
    }

    #[test]
    fn diff_images_have_the_mode_sizes() {
        let (a, b) = (scene(0, [200, 60, 40], true), scene(2, [200, 60, 40], true));
        let c = compare(&a, &b).unwrap();
        assert!(diff_image(&a, &b, &c.max_diff, DiffImage::None).is_none());
        let half = diff_image(&a, &b, &c.max_diff, DiffImage::Half).unwrap();
        assert_eq!((half.width, half.height), (160, 90));
        assert_eq!(half.get(15, 30)[0], 255);
        assert!(half.get(15, 30)[1] < 200 && half.get(40, 30)[0] == half.get(40, 30)[1]);
        let full = diff_image(&a, &b, &c.max_diff, DiffImage::Full).unwrap();
        assert_eq!((full.width, full.height), (960, 180));
    }
}

#[cfg(test)]
#[path = "compare_scalar_tests.rs"]
mod scalar_tests;
