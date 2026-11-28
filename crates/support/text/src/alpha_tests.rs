use super::*;
use kition_slide_color::ColorTransform;
use kition_slide_model::style::{Gradient, GradientShape, GradientStop};
use kition_slide_model::{ColorRef, LayoutType, SlideFile, Xfrm, defaults};

fn fixture(fill: Fill, math: bool) -> (SlideFile, Shape) {
    let mut pres = SlideFile::default();
    let layout = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, layout);
    let mut shape = slide.shapes.remove(1);
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, 300.0, 200.0));
    let mut body = TextBody::from_text("AB");
    body.body.autofit = Some(AutoFit::None);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    let run = &mut body.paragraphs[0].runs[0];
    run.props = RunProps { font: Some("Inter".into()), size: Some(24.0), fill: Some(fill), ..Default::default() };
    if math {
        run.kind = RunKind::Math {
            omml: r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>AB</m:t></m:r></m:oMath>"#.into(),
        };
    }
    shape.text = Some(body);
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    (pres, shape)
}

fn laid_out(color: ColorRef, math: bool) -> TextLayout {
    let (pres, shape) = fixture(Fill::solid(color), math);
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    layout(
        &ctx,
        &shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, 300.0, 200.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false },
    )
}

fn assert_same_paint_and_placement(transformed: ColorRef, expected: Rgba, math: bool) {
    let actual = laid_out(transformed, math);
    let direct = laid_out(ColorRef::rgb(expected), math);
    assert!(!actual.runs.is_empty());
    assert_eq!(actual.runs.len(), direct.runs.len());
    for (a, b) in actual.runs.iter().zip(&direct.runs) {
        assert_eq!(a.color, expected);
        assert_eq!(a.color, b.color);
        assert_eq!(a.alpha, 1.0);
        assert_eq!(a.alpha, b.alpha);
        assert_eq!(a.glyphs, b.glyphs);
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.size, b.size);
        assert!(Arc::ptr_eq(&a.face, &b.face));
    }
    assert_eq!(actual.lines.len(), direct.lines.len());
    for (a, b) in actual.lines.iter().zip(&direct.lines) {
        assert_eq!(a.baseline, b.baseline);
        assert_eq!(a.caret_x, b.caret_x);
    }
}

#[test]
fn solid_alpha_transforms_equal_direct_rgba_for_text_and_math() {
    for math in [false, true] {
        for (value, byte) in [(0, 0), (50_000, 128), (100_000, 255)] {
            assert_same_paint_and_placement(ColorRef::rgb(Rgba::BLACK).with(ColorTransform::Alpha(value)), Rgba::BLACK.with_alpha(byte), math);
        }
    }
}

#[test]
fn ordered_alpha_modifiers_are_applied_once() {
    let cases = [
        (ColorRef::rgb(Rgba::BLACK.with_alpha(128)).with(ColorTransform::AlphaMod(50_000)), 64),
        (ColorRef::rgb(Rgba::BLACK.with_alpha(64)).with(ColorTransform::AlphaOff(25_000)), 128),
        (
            ColorRef::rgb(Rgba::BLACK)
                .with(ColorTransform::Alpha(50_000))
                .with(ColorTransform::AlphaMod(50_000))
                .with(ColorTransform::AlphaOff(25_000)),
            128,
        ),
    ];
    for (color, byte) in cases {
        for math in [false, true] {
            assert_same_paint_and_placement(color.clone(), Rgba::BLACK.with_alpha(byte), math);
        }
    }
}

#[test]
fn gradient_stop_alpha_and_no_fill_remain_distinct() {
    let gradient = Gradient {
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::BLACK).with(ColorTransform::Alpha(50_000)) },
            GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::WHITE) },
        ],
        shape: GradientShape::Linear { angle: 0.0, scaled: false },
        tile_rect: None,
        rotate_with_shape: true,
    };
    let (pres, shape) = fixture(Fill::Gradient(gradient), false);
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let props = &shape.text.as_ref().unwrap().paragraphs[0].runs[0].props;
    let style = make_style(&ctx, props, 1.0, None);
    assert_eq!(style.alpha, 1.0);
    assert_eq!(style.color.a, 128);
    let resolved = style.gradient.unwrap();
    assert_eq!(resolved.stops[0].color, ColorRef::rgb(Rgba::BLACK.with_alpha(128)));
    assert_eq!(resolved.stops[1].color, ColorRef::rgb(Rgba::WHITE));
    let no_fill = make_style(&ctx, &RunProps { fill: Some(Fill::None), ..props.clone() }, 1.0, None);
    assert_eq!(no_fill.color, Rgba::TRANSPARENT);
    assert_eq!(no_fill.alpha, 0.0);
    assert!(no_fill.gradient.is_none());
}
