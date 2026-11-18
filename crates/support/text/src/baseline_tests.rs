use super::*;
use kition_slide_model::text::Run;
use kition_slide_model::{LayoutType, SlideFile, Xfrm, defaults};

fn sample(shift: f64, script_size: f64, width: f64) -> TextLayout {
    let mut pres = SlideFile::default();
    let layout_id = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, layout_id);
    let mut shape = slide.shapes.remove(1);
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, width, 400.0));
    let mut body = TextBody::from_text("");
    body.body.autofit = Some(AutoFit::None);
    body.body.wrap = Some(true);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].props.align = Some(Align::Left);
    let normal = RunProps { font: Some("Inter".into()), size: Some(40.0), baseline: Some(0.0), ..Default::default() };
    let script = RunProps { size: Some(script_size), baseline: Some(shift), spacing: Some(1.5), ..normal.clone() };
    body.paragraphs[0].runs = vec![Run::with("A", normal.clone()), Run::with("12345678", script), Run::with("B", normal)];
    shape.text = Some(body);
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    layout(
        &ctx,
        &shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, width, 400.0), fields: &NoFields, prompt_color: None, no_shrink: true },
    )
}

#[test]
fn script_width_tracking_and_carets_match_painted_font_size() {
    let control = sample(0.0, 40.0 * 0.66, 1000.0);
    for shift in [-0.25, 0.3] {
        let actual = sample(shift, 40.0, 1000.0);
        assert_eq!(actual.lines.len(), 1);
        assert!((actual.content_width - control.content_width).abs() < 1e-8);
        for (a, b) in actual.runs.iter().zip(&control.runs) {
            assert_eq!(a.size, b.size);
            for (ga, gb) in a.glyphs.iter().zip(&b.glyphs) {
                assert_eq!(ga.0, gb.0);
                assert!((ga.1 - gb.1).abs() < 1e-8);
            }
        }
        for ch in 0..=10 {
            let pos = Pos { para: 0, ch };
            let (x, top, bottom) = actual.caret(pos).unwrap();
            assert!((x - control.caret(pos).unwrap().0).abs() < 1e-8);
            assert_eq!(actual.hit(Point::new(x + 0.01, (top + bottom) / 2.0)), pos);
        }
        let selected = actual.selection_rects(Pos { para: 0, ch: 1 }, Pos { para: 0, ch: 9 });
        let expected = control.selection_rects(Pos { para: 0, ch: 1 }, Pos { para: 0, ch: 9 });
        assert_eq!(selected.len(), 1);
        assert!((selected[0].width() - expected[0].width()).abs() < 1e-8);
    }
}

#[test]
fn script_advance_prevents_phantom_line_wrap() {
    let small = sample(0.0, 40.0 * 0.66, 1000.0);
    let full = sample(0.0, 40.0, 1000.0);
    let width = (small.content_width + full.content_width) / 2.0;
    assert!(sample(0.0, 40.0, width).lines.len() > 1);
    for shift in [-0.25, 0.3] {
        let actual = sample(shift, 40.0, width);
        assert_eq!(actual.lines.len(), 1);
        assert_eq!(actual.lines[0].end, 10);
        assert!((actual.content_width - small.content_width).abs() < 1e-8);
    }
}
