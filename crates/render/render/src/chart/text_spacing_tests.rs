use super::*;
use kition_slide_model::SlideFile;

#[test]
fn chart_tracking_inherits_and_uses_the_ordinary_text_measurement_path() {
    let presentation = SlideFile::default();
    let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).unwrap();
    let mut chart = Chart::sample(ChartType::Column);
    chart.fmt.text.spacing = Some(1.25);
    let base = Txt {
        font: Some("Inter".into()),
        font_ea: None,
        font_cs: None,
        size: 20.0,
        spacing: Some(1.25),
        bold: false,
        italic: false,
        color: Rgba::BLACK,
        rot: None,
    };
    let env = Env { rctx: &rctx, c: &chart, m: Affine::IDENTITY, pal: Pal { rctx: &rctx, scheme: None }, base, size: (960.0, 540.0) };
    let inherited = env.txt(&ChartText::default(), 20.0, false);
    let zero = env.txt(&ChartText { spacing: Some(0.0), ..Default::default() }, 20.0, false);
    let negative = env.txt(&ChartText { spacing: Some(-0.5), ..Default::default() }, 20.0, false);
    let text = "AB CD";
    let plain_width = measure(&env, text, &zero, None).0;
    assert!((measure(&env, text, &inherited, None).0 - plain_width - 6.25).abs() < 0.01);
    assert!((measure(&env, text, &negative, None).0 - plain_width + 2.5).abs() < 0.01);
    let body = body(text, &inherited, Align::Center, false);
    assert_eq!(body.paragraphs[0].runs[0].props.spacing, Some(1.25));
    let shape = Shape { text: Some(body.clone()), ..Default::default() };
    let layout = kition_slide_text::layout(
        &rctx,
        &shape,
        &body,
        &kition_slide_text::Opts {
            rect: Rect::new(0.0, 0.0, 900.0, 200.0),
            fields: &kition_slide_text::NoFields,
            prompt_color: None,
            no_shrink: true,
            empty_bullets: false,
        },
    );
    assert!((layout.content_width - measure(&env, text, &inherited, None).0).abs() < 0.01);
}
