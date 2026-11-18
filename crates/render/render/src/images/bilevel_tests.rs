#![allow(clippy::expect_used)]

use super::{apply_adjust, decode_for, gif_pixmap};
use image::{Rgba, RgbaImage};
use kition_slide_model::style::PictureAdjust;
use std::sync::Arc;

fn adjustment(threshold: Option<f64>) -> PictureAdjust {
    PictureAdjust { bi_level: threshold, ..Default::default() }
}

#[test]
fn bilevel_uses_luminance_and_preserves_threshold_equality_and_alpha() {
    for (rgb, threshold, white) in [
        ([0, 0, 0], 0.0, true),
        ([0, 0, 0], 0.5, false),
        ([127, 127, 127], 0.5, false),
        ([128, 128, 128], 0.5, true),
        ([128, 128, 128], 128.0 / 255.0, true),
        ([254, 254, 254], 1.0, false),
        ([255, 255, 255], 1.0, true),
        ([255, 0, 0], 0.299, true),
        ([255, 0, 0], 0.299f64.next_up(), false),
        ([0, 255, 0], 0.587, true),
        ([0, 0, 255], 0.114, true),
    ] {
        for alpha in [0, 17, 128, 255] {
            let mut image = RgbaImage::from_pixel(1, 1, Rgba([rgb[0], rgb[1], rgb[2], alpha]));
            apply_adjust(&mut image, &adjustment(Some(threshold)));
            let value = if white { 255 } else { 0 };
            assert_eq!(image.get_pixel(0, 0).0, [value, value, value, alpha], "rgb={rgb:?}, threshold={threshold}, alpha={alpha}");
        }
    }
}

#[test]
fn bilevel_png_cache_distinguishes_absence_zero_and_changed_thresholds() {
    let mut encoded = Vec::new();
    RgbaImage::from_pixel(1, 1, Rgba([96, 96, 96, 128]))
        .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
        .expect("synthetic PNG encodes");
    let bytes = Arc::new(encoded);
    let mut cached = Vec::new();
    for (threshold, expected) in
        [(None, [48, 48, 48, 128]), (Some(0.0), [128, 128, 128, 128]), (Some(0.5), [0, 0, 0, 128]), (Some(0.25), [128, 128, 128, 128])]
    {
        let adjust = adjustment(threshold);
        let pixmap = decode_for(&bytes, 16.0, &adjust).expect("synthetic PNG decodes");
        assert_eq!(pixmap.data_as_u8_slice(), &expected);
        assert!(cached.iter().all(|other| !Arc::ptr_eq(&pixmap, other)), "different adjustments must not share an entry");
        assert!(Arc::ptr_eq(&pixmap, &decode_for(&bytes, 16.0, &adjust).expect("cached PNG decodes")));
        cached.push(pixmap);
    }
    assert_eq!(decode_for(&bytes, 16.0, &adjustment(None)).expect("original cache entry remains").data_as_u8_slice(), &[48, 48, 48, 128]);
}

#[test]
fn bilevel_animation_frames_use_their_own_adjustment_cache() {
    let frames = [64, 192].into_iter().map(|value| image::Frame::new(RgbaImage::from_pixel(1, 1, Rgba([value, value, value, 255]))));
    let mut encoded = Vec::new();
    image::codecs::gif::GifEncoder::new(&mut encoded).encode_frames(frames).expect("synthetic GIF encodes");
    let bytes = Arc::new(encoded);
    for (threshold, expected) in [(None, [64, 192]), (Some(0.0), [255, 255]), (Some(0.5), [0, 255])] {
        for (time, value) in [0.0, 0.11].into_iter().zip(expected) {
            let adjust = adjustment(threshold);
            let pixmap = gif_pixmap(&bytes, &adjust, time).expect("synthetic GIF frame decodes");
            assert_eq!(pixmap.data_as_u8_slice(), &[value, value, value, 255]);
            assert!(Arc::ptr_eq(&pixmap, &gif_pixmap(&bytes, &adjust, time).expect("cached GIF frame decodes")));
        }
    }
}
