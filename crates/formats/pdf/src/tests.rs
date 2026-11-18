use std::sync::Arc;

use super::*;
use kition_slide_model::text::TextBody;

fn slide_file(n: usize) -> SlideFile {
    let mut p = kition_slide_model::defaults::new_slide_file(None);
    let layout = p.slides[0].layout;
    while p.slides.len() < n {
        let s = kition_slide_model::defaults::new_slide(&mut p, layout);
        p.slides.push(Arc::new(s));
    }
    for (i, s) in p.slides.iter_mut().enumerate() {
        let s = Arc::make_mut(s);
        if let Some(sh) = s.shapes.first_mut() {
            sh.text = Some(TextBody::from_text(&format!("Slide title {i}")));
        }
        s.notes = TextBody::from_text("Speaker notes that wrap across the page width to test line breaking of long notes.");
    }
    p
}

fn opts(layout: PageLayout) -> PdfOptions {
    PdfOptions { layout, dpi: 48.0, ..Default::default() }
}

#[test]
fn exports_slides_notes_and_handouts() {
    let p = slide_file(5);
    for layout in [PageLayout::Slides, PageLayout::Notes, PageLayout::Handouts { per_page: 3 }, PageLayout::Handouts { per_page: 6 }] {
        let bytes = export(&p, &opts(layout)).expect("export");
        assert!(bytes.starts_with(b"%PDF-"), "{layout:?}");
        assert!(bytes.len() > 1000);
    }
}

#[test]
fn empty_range_is_an_error() {
    let p = slide_file(1);
    let o = PdfOptions { slides: Some(vec![7]), ..opts(PageLayout::Slides) };
    assert!(matches!(export(&p, &o), Err(PdfError::NoPages)));
}

#[test]
fn hidden_slides_skipped_unless_asked() {
    let mut p = slide_file(2);
    Arc::make_mut(&mut p.slides[1]).hidden = true;
    let a = export(&p, &opts(PageLayout::Slides)).unwrap();
    let b = export(&p, &PdfOptions { include_hidden: true, ..opts(PageLayout::Slides) }).unwrap();
    assert!(b.len() > a.len());
}

#[test]
fn handout_boxes_fit_the_paper() {
    for n in [1, 2, 3, 4, 6, 9] {
        let v = handout_boxes(n, (612.0, 792.0), 16.0 / 9.0);
        assert_eq!(v.len(), n as usize);
        for r in &v {
            assert!(r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= 612.0 && r.y1 <= 792.0, "{n}: {r:?}");
            assert!((r.width() / r.height() - 16.0 / 9.0).abs() < 1e-6);
        }
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                assert!(a.intersect(*b).area() <= 0.0, "{n}: overlap");
            }
        }
    }
}

#[test]
fn east_asian_vertical_text_layer_uses_each_upright_glyph_origin() {
    use kition_slide_model::text::{AutoFit, Bullet, TextDir};
    let text: String = [0x4e00, 0x4e8c].into_iter().map(|c| char::from_u32(c).unwrap()).collect();
    let mut p = slide_file(1);
    let slide = Arc::make_mut(&mut p.slides[0]);
    slide.shapes.truncate(1);
    let shape = &mut slide.shapes[0];
    shape.text = Some(TextBody::from_text(&text));
    let body = shape.text.as_mut().unwrap();
    body.body.vert = Some(TextDir::EaVertical);
    body.body.autofit = Some(AutoFit::None);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].runs[0].props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
    let placed = kition_slide_render::place_slide(&p, 0);
    let pt = placed.texts.first().unwrap();
    let glyphs: Vec<_> = pt.layout.runs.iter().filter(|r| r.upright).flat_map(|r| &r.glyphs).collect();
    assert_eq!(glyphs.len(), 2);
    let options = opts(PageLayout::Slides);
    let mut exporter = Exporter { pres: &p, opts: &options, fonts: HashMap::new() };
    let mut pdf = krilla::Document::new_with(krilla::SerializeSettings { compress_content_streams: false, ..Default::default() });
    let mut page = pdf.start_page_with(PageSettings::from_wh(960.0, 540.0).unwrap());
    let mut surface = page.surface();
    exporter.text_layer(&mut surface, pt, Affine::IDENTITY);
    surface.finish();
    page.finish();
    let bytes = pdf.finish().unwrap();
    let content = String::from_utf8_lossy(&bytes);
    let matrices: Vec<Vec<f64>> = content
        .lines()
        .filter_map(|line| line.strip_prefix("q ")?.split_once(" cm").map(|(matrix, _)| matrix))
        .filter_map(|line| line.split_whitespace().map(str::parse).collect::<std::result::Result<Vec<f64>, _>>().ok())
        .collect();
    for glyph in glyphs {
        let origin = pt.transform * kurbo::Point::new(glyph.1, glyph.2);
        assert!(
            matrices.iter().any(|m| m.len() == 6
                && (m[0] - 1.0).abs() < 1e-5
                && m[1].abs() < 1e-5
                && m[2].abs() < 1e-5
                // PDF page coordinates start at the bottom left; Krilla flips the surface.
                && (m[3] + 1.0).abs() < 1e-5
                && (m[4] - origin.x).abs() < 0.01
                && (m[5] - (540.0 - origin.y)).abs() < 0.01),
            "missing upright glyph origin {origin:?}: {matrices:?}"
        );
    }
    assert_eq!(content.matches(")Tj").count(), 2);
}
