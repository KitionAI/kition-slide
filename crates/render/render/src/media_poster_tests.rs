//! Authored media preview images are the visible object, without fallback controls.

use super::*;
use kition_slide_model::MediaClip;
use kition_slide_model::style::PictureFill;
use std::io::Cursor;

fn sample() -> (SlideFile, Shape, kition_slide_model::MediaId) {
    let mut p = SlideFile { slide_size: kurbo::Size::new(180.0, 120.0), ..Default::default() };
    let pixels = image::RgbaImage::from_fn(8, 8, |x, y| {
        if x < 3 {
            image::Rgba([240, 100, 20, 255])
        } else if y < 4 {
            image::Rgba([20, 180, 80, 128])
        } else {
            image::Rgba([0; 4])
        }
    });
    let mut bytes = Cursor::new(Vec::new());
    pixels.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let poster = p.add_media("preview.png", "image/png", bytes.into_inner());
    let clip = p.add_media("sample.wav", "audio/wav", b"synthetic-media-placeholder".to_vec());
    let shape = Shape {
        id: ShapeId(20),
        xfrm: Some(kition_slide_geom::Xfrm::new(20.0, 20.0, 120.0, 80.0)),
        kind: ShapeKind::Picture { fill: PictureFill { media: poster, ..Default::default() } },
        ..Default::default()
    };
    (p, shape, clip)
}

fn render(p: &mut SlideFile, shape: Shape) -> Image {
    Arc::make_mut(&mut p.slides[0]).shapes = vec![shape];
    render_slide(p, 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

#[test]
fn audio_and_video_posters_preserve_picture_colors_and_transparency() {
    let (mut p, shape, media) = sample();
    let ShapeKind::Picture { fill } = &shape.kind else { panic!("picture fixture") };
    let poster = fill.media;
    let expected = render(&mut p, shape.clone());
    for video in [false, true] {
        let mut shape = shape.clone();
        shape.kind = ShapeKind::Media(MediaClip { media, video, poster: Some(poster), ..Default::default() });
        assert_eq!(render(&mut p, shape).pixels, expected.pixels, "a supplied poster is not covered by a fallback glyph");
    }
}

#[test]
fn audio_and_video_without_posters_keep_visible_fallbacks() {
    let (mut p, shape, media) = sample();
    for video in [false, true] {
        let mut shape = shape.clone();
        shape.kind = ShapeKind::Media(MediaClip { media, video, ..Default::default() });
        let image = render(&mut p, shape);
        assert_ne!(image.pixel(80, 60), [255; 4]);
        assert_eq!(image.pixel(0, 0), [255; 4]);
        if video {
            assert_eq!(image.pixel(25, 25), [20, 20, 24, 255]);
        } else {
            assert_eq!(image.pixel(80, 60), [0x2e, 0x6f, 0xd8, 255]);
        }
    }
}
