//! The comparator validated on clean renders of synthetic slides: an unchanged slide passes; a
//! shifted shape, a missing object, a substituted word and a colour change each fail, with the
//! difference boxed where the change is and attributed to the right kind of object.

use std::sync::Arc;

use kition_slide_model::style::ColorRef;
use kition_slide_model::text::TextBody;
use kition_slide_model::{Fill, Rgba, Shape, ShapeId, SlideFile, Xfrm};
use kition_slide_visual::compare::{Rgb, compare};
use kition_slide_visual::regions;
use kition_slide_visual::runner::{FROZEN_DATE, render_clean};

const W: u32 = 1280;
const H: u32 = 720;

struct Spec {
    box_x: f64,
    box_color: Rgba,
    with_box: bool,
    words: &'static str,
}

impl Default for Spec {
    fn default() -> Self {
        Spec { box_x: 100.0, box_color: Rgba::rgb(200, 60, 40), with_box: true, words: "Quarterly results" }
    }
}

fn slide_file(spec: &Spec) -> SlideFile {
    let mut shapes = Vec::new();
    if spec.with_box {
        shapes.push(Shape {
            id: ShapeId(10),
            xfrm: Some(Xfrm::new(spec.box_x, 300.0, 200.0, 150.0)),
            fill: Some(Fill::Solid { color: ColorRef::rgb(spec.box_color) }),
            ..Default::default()
        });
    }
    let mut text = TextBody::from_text(spec.words);
    text.paragraphs[0].runs[0].props.size = Some(40.0);
    shapes.push(Shape { id: ShapeId(11), xfrm: Some(Xfrm::new(80.0, 60.0, 800.0, 120.0)), text_box: true, text: Some(text), ..Default::default() });
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = shapes;
    p
}

fn render(spec: &Spec) -> (SlideFile, Rgb) {
    let p = slide_file(spec);
    let img = render_clean(&p, 0, W, H, FROZEN_DATE);
    (p, img)
}

/// Pixels per point at the test size.
const S: f64 = W as f64 / 960.0;

fn inside(bbox: [u32; 4], x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let pad = 3.0;
    bbox[0] as f64 >= x0 * S - pad && bbox[1] as f64 >= y0 * S - pad && bbox[2] as f64 <= x1 * S + pad && bbox[3] as f64 <= y1 * S + pad
}

#[test]
fn an_unchanged_slide_file_passes_and_renders_are_deterministic() {
    let (_, a) = render(&Spec::default());
    let (_, b) = render(&Spec::default());
    let m = compare(&a, &b).unwrap().metrics;
    assert_eq!(m.ssim, 1.0);
    assert_eq!(m.exact_equal, 1.0);
    assert!(m.pass());
}

#[test]
fn a_shift_fails_within_the_shapes_box() {
    let (_, reference) = render(&Spec::default());
    for dx in [0.75, 4.0] {
        let (p, actual) = render(&Spec { box_x: 100.0 + dx, ..Default::default() });
        let c = compare(&reference, &actual).unwrap();
        assert!(!c.metrics.pass(), "shift {dx}: {:?}", c.metrics);
        assert!(inside(c.metrics.bbox.unwrap(), 100.0, 300.0, 304.0, 450.0), "shift {dx}: {:?}", c.metrics.bbox);
        // The box's new place is on the shape; the place it left is backdrop.
        let regions = regions::attribute(&p, &p.slides[0], S, S, W, H, &c.max_diff, 8);
        assert!(regions.contains_key("shape") && regions.keys().all(|k| k == "shape" || k == "backdrop"), "{regions:?}");
    }
}

#[test]
fn a_missing_object_fails_and_lands_on_the_backdrop() {
    let (_, reference) = render(&Spec::default());
    let (p, actual) = render(&Spec { with_box: false, ..Default::default() });
    let c = compare(&reference, &actual).unwrap();
    assert!(!c.metrics.pass());
    assert!(c.metrics.diff_ratio > 0.03, "{:?}", c.metrics);
    assert!(inside(c.metrics.bbox.unwrap(), 100.0, 300.0, 300.0, 450.0), "{:?}", c.metrics.bbox);
    let regions = regions::attribute(&p, &p.slides[0], S, S, W, H, &c.max_diff, 8);
    assert_eq!(regions.keys().collect::<Vec<_>>(), vec!["backdrop"], "{regions:?}");
}

#[test]
fn a_substituted_word_fails_within_the_text_box() {
    let (_, reference) = render(&Spec::default());
    let (p, actual) = render(&Spec { words: "Quarterly resulfs", ..Default::default() });
    let c = compare(&reference, &actual).unwrap();
    // One wrong glyph is under 0.1% of a 1280x720 frame: the whole-frame gate cannot see it (a
    // known blind spot, measured here); the local gate (worst tile) does.
    assert!(c.metrics.pass(), "{:?}", c.metrics);
    assert!(!c.metrics.pass_local(), "{:?}", c.metrics);
    assert!(c.metrics.diff_pixels > 100 && c.metrics.ssim < 1.0, "{:?}", c.metrics);
    // A whole substituted word fails both gates.
    let (_, actual) = render(&Spec { words: "Quarterly revenue", ..Default::default() });
    let m = compare(&reference, &actual).unwrap().metrics;
    assert!(!m.pass() && !m.pass_local(), "{m:?}");
    assert!(inside(c.metrics.bbox.unwrap(), 80.0, 60.0, 880.0, 180.0), "{:?}", c.metrics.bbox);
    let regions = regions::attribute(&p, &p.slides[0], S, S, W, H, &c.max_diff, 8);
    assert_eq!(regions.keys().collect::<Vec<_>>(), vec!["text"], "{regions:?}");
}

#[test]
fn a_colour_change_fails_on_the_pixel_gate() {
    let (_, reference) = render(&Spec::default());
    // About the same luma, another hue.
    let (_, actual) = render(&Spec { box_color: Rgba::rgb(96, 105, 101), ..Default::default() });
    let m = compare(&reference, &actual).unwrap().metrics;
    assert!(!m.pass(), "{m:?}");
    assert!(m.diff_ratio > 0.03, "{m:?}");
    assert!(inside(m.bbox.unwrap(), 100.0, 300.0, 300.0, 450.0), "{:?}", m.bbox);
}
