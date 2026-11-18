use super::*;
use kition_slide_model::Xfrm;
use kition_slide_model::text::{AutoFit, Bullet, TextBody, TextDir};

#[test]
fn east_asian_vertical_ideograph_is_upright_while_plain_vertical_rotates_it() {
    let ideograph = char::from_u32(0x4e00).unwrap().to_string();
    let render = |direction| {
        let mut shape = Shape {
            id: ShapeId(2),
            xfrm: Some(Xfrm::new(100.0, 100.0, 100.0, 200.0)),
            text: Some(TextBody::from_text(&ideograph)),
            ..Default::default()
        };
        let body = shape.text.as_mut().unwrap();
        body.body.autofit = Some(AutoFit::None);
        body.body.vert = Some(direction);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        body.paragraphs[0].runs[0].props.size = Some(40.0);
        body.paragraphs[0].runs[0].props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
        let p = super::fidelity_tests::slide_file_with(vec![shape]);
        let image = render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
        let mut bounds = (image.width, image.height, 0, 0);
        for y in 0..image.height {
            for x in 0..image.width {
                if image.pixel(x, y)[0] < 128 {
                    bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
                }
            }
        }
        (bounds.2 - bounds.0 + 1, bounds.3 - bounds.1 + 1)
    };
    let upright = render(TextDir::EaVertical);
    let rotated = render(TextDir::Vertical);
    assert!(upright.0 > upright.1 * 3, "upright horizontal stroke: {upright:?}");
    assert!(rotated.1 > rotated.0 * 3, "rotated horizontal stroke: {rotated:?}");
}

#[test]
fn morph_between_upright_and_sideways_runs_keeps_the_final_orientation() {
    let text = char::from_u32(0x4e00).unwrap().to_string();
    let x = Xfrm::new(100.0, 100.0, 200.0, 200.0);
    let mut old = Shape { id: ShapeId(2), xfrm: Some(x), text: Some(TextBody::from_text(&text)), ..Default::default() };
    let body = old.text.as_mut().unwrap();
    body.body.autofit = Some(AutoFit::None);
    body.body.vert = Some(TextDir::EaVertical);
    body.body.rot = Some(-90.0);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].runs[0].props.size = Some(40.0);
    body.paragraphs[0].runs[0].props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
    let mut new = old.clone();
    new.text.as_mut().unwrap().body.vert = Some(TextDir::Horizontal);
    new.text.as_mut().unwrap().body.rot = None;
    let mut p = super::fidelity_tests::slide_file_with(vec![old.clone()]);
    let mut slide = (*p.slides[0]).clone();
    slide.shapes = vec![new.clone()];
    p.slides.push(std::sync::Arc::new(slide));
    old.id = ShapeId(11);
    new.id = ShapeId(10);
    let options = RenderOpts { scale: 1.0, ..Default::default() };
    for t in [0.0, 1.0] {
        let morph = TextMorph { old: old.id, new: new.id, from: x, to: x, chars: true, t };
        let frame = render_blend(&p, 0, 1, t, &[(new.clone(), false), (old.clone(), true)], &[morph], &[], &options);
        let expected = render_slide(&p, t as usize, &options);
        assert!(frame.pixels == expected.pixels, "morph endpoint {t}");
    }
}
