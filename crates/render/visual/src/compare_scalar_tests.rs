//! Scalar reference implementations guard exact metric equivalence.

use super::*;

fn ssim_scalar(a: &[f64], b: &[f64], w: usize, h: usize) -> (f64, Option<([u32; 4], f64)>) {
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
    // Horizontal pass of the five moments, then the vertical pass row by row.
    let mut hz: [Vec<f64>; 5] = std::array::from_fn(|_| vec![0.0; ow * h]);
    for y in 0..h {
        let (ra, rb) = (&a[y * w..(y + 1) * w], &b[y * w..(y + 1) * w]);
        for x in 0..ow {
            let mut s = [0.0; 5];
            for (t, kv) in k.iter().enumerate() {
                let (p, q) = (ra[x + t], rb[x + t]);
                s[0] += kv * p;
                s[1] += kv * q;
                s[2] += kv * p * p;
                s[3] += kv * q * q;
                s[4] += kv * p * q;
            }
            for m in 0..5 {
                hz[m][y * ow + x] = s[m];
            }
        }
    }
    let (tw, th) = (ow.div_ceil(TILE), oh.div_ceil(TILE));
    let mut tiles = vec![(0.0f64, 0u32); tw * th];
    let mut total = 0.0;
    for y in 0..oh {
        for x in 0..ow {
            let mut s = [0.0; 5];
            for (t, kv) in k.iter().enumerate() {
                let i = (y + t) * ow + x;
                for m in 0..5 {
                    s[m] += kv * hz[m][i];
                }
            }
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

#[test]
fn optimized_ssim_matches_scalar_bit_for_bit() {
    for (w, h) in [(1, 1), (7, 19), (11, 11), (12, 49), (37, 53), (97, 71), (1280, 720)] {
        let mut a = vec![0.0; w * h];
        let mut b = a.clone();
        for (i, (x, y)) in a.iter_mut().zip(&mut b).enumerate() {
            *x = ((i * 137 + 17) % 256) as f64 * 0.299 + ((i * 31) % 256) as f64 * 0.701;
            *y = if i % 13 == 0 { ((i * 19) % 256) as f64 } else { *x };
        }
        assert_eq!(ssim(&a, &b, w, h), ssim_scalar(&a, &b, w, h), "{w}x{h}");
        assert_eq!(ssim(&a, &a, w, h), ssim_scalar(&a, &a, w, h), "identical {w}x{h}");
    }
}

fn blur_scalar(img: &Rgb) -> Rgb {
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 {
        return img.clone();
    }
    let k = blur_kernel();
    let r = LAYOUT_BLUR_RADIUS as isize;
    let at = |i: isize, n: usize| i.clamp(0, n as isize - 1) as usize;
    let mut tmp = vec![0.0f64; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0.0; 3];
            for (t, kv) in k.iter().enumerate() {
                let sx = at(x as isize + t as isize - r, w);
                let i = (y * w + sx) * 3;
                for (c, v) in s.iter_mut().enumerate() {
                    *v += kv * img.data[i + c] as f64;
                }
            }
            tmp[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&s);
        }
    }
    let mut data = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0.0; 3];
            for (t, kv) in k.iter().enumerate() {
                let sy = at(y as isize + t as isize - r, h);
                let i = (sy * w + x) * 3;
                for (c, v) in s.iter_mut().enumerate() {
                    *v += kv * tmp[i + c];
                }
            }
            for (c, v) in s.iter().enumerate() {
                data[(y * w + x) * 3 + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Rgb { width: img.width, height: img.height, data }
}

#[test]
fn optimized_blur_matches_scalar_pixels() {
    for (w, h) in [(1, 1), (2, 23), (10, 31), (12, 12), (13, 13), (37, 53), (1280, 720)] {
        let image = Rgb { width: w, height: h, data: (0..w * h * 3).map(|i| ((i * 137 + 17) % 256) as u8).collect() };
        assert_eq!(blur(&image), blur_scalar(&image), "{w}x{h}");
    }
}
