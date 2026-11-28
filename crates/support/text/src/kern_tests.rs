use super::*;
use kition_slide_model::text::Run;
use kition_slide_model::{LayoutType, SlideFile, Xfrm, defaults};

fn run(text: &str, kern: Option<f64>) -> Run {
    Run::with(text, RunProps { font: Some("Inter".into()), size: Some(20.0), kern, ..Default::default() })
}

fn sample(runs: Vec<Run>) -> TextLayout {
    let mut pres = SlideFile { default_text_style: Default::default(), ..Default::default() };
    let layout_id = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, layout_id);
    let mut shape = slide.shapes.remove(1);
    shape.ph = None;
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, 1000.0, 400.0));
    let mut body = TextBody::from_text("");
    body.body.autofit = Some(AutoFit::None);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].props.align = Some(Align::Left);
    body.paragraphs[0].runs = runs;
    shape.text = Some(body);
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    layout(
        &ctx,
        &shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, 1000.0, 400.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false },
    )
}

fn glyph_x(layout: &TextLayout) -> Vec<(u32, f64)> {
    layout.runs.iter().flat_map(|run| run.glyphs.iter().map(|&(id, x, _)| (id, x))).collect()
}

#[test]
fn explicit_threshold_controls_width_glyph_positions_and_carets() {
    let text = "AVATAR To WA";
    let enabled = sample(vec![run(text, Some(0.0))]);
    let disabled = sample(vec![run(text, Some(24.0))]);
    let absent = sample(vec![run(text, None)]);
    let equal = sample(vec![run(text, Some(20.0))]);
    assert_eq!(glyph_x(&enabled), glyph_x(&absent));
    assert_eq!(glyph_x(&enabled), glyph_x(&equal));
    assert!(disabled.content_width > enabled.content_width + 0.1);
    let face = &disabled.runs[0].face;
    let features = [kition_slide_fonts::feature("-kern").unwrap()];
    let shaped = kition_slide_fonts::shape_range(face, text, 0..text.len(), &ShapeParams { features: &features, ..Default::default() }, |c| c);
    let expected_width = shaped.iter().map(|glyph| f64::from(glyph.x_advance)).sum::<f64>() * 20.0 / face.upem;
    assert!((disabled.content_width - expected_width).abs() < 1e-8);
    assert_ne!(glyph_x(&disabled), glyph_x(&enabled));
    for layout in [&enabled, &disabled] {
        for ch in 0..=text.len() {
            let pos = Pos { para: 0, ch };
            let (x, top, bottom) = layout.caret(pos).unwrap();
            assert_eq!(layout.hit(Point::new(x + 0.01, (top + bottom) / 2.0)), pos);
        }
        let end = layout.caret(Pos { para: 0, ch: text.len() }).unwrap().0;
        assert!((end - layout.content_width).abs() < 1e-8);
    }
}

#[test]
fn mixed_thresholds_split_items_only_when_the_effective_policy_changes() {
    let mixed = sample(vec![run("AV", Some(24.0)), run("AV", Some(0.0))]);
    let first = sample(vec![run("AV", Some(24.0))]);
    let second = sample(vec![run("AV", Some(0.0))]);
    assert!((mixed.content_width - first.content_width - second.content_width).abs() < 1e-8);
    assert!((mixed.caret(Pos { para: 0, ch: 2 }).unwrap().0 - first.content_width).abs() < 1e-8);
    let mixed_x = glyph_x(&mixed);
    assert_eq!(&mixed_x[..2], glyph_x(&first));
    for (actual, expected) in mixed_x[2..].iter().zip(glyph_x(&second)) {
        assert_eq!(actual.0, expected.0);
        assert!((actual.1 - expected.1 - first.content_width).abs() < 1e-8);
    }
    let same_policy = sample(vec![run("AV", None), run("AV", Some(0.0))]);
    assert_eq!(glyph_x(&same_policy), glyph_x(&sample(vec![run("AVAV", Some(20.0))])));

    // A threshold change on a combining mark cannot divide its base grapheme.
    let accent = char::from_u32(0x301).unwrap();
    let separate = sample(vec![run("A", Some(24.0)), run(&accent.to_string(), Some(0.0)), run("V", Some(24.0))]);
    let together = sample(vec![run(&format!("A{accent}V"), Some(24.0))]);
    assert_eq!(glyph_x(&separate), glyph_x(&together));
}

#[test]
fn baseline_and_small_caps_use_the_resolved_run_size_for_the_threshold() {
    for (text, baseline, caps) in [("AV", 0.3, Caps::None), ("AV", -0.25, Caps::None), ("av", 0.0, Caps::Small)] {
        let mut threshold = run(text, Some(18.0));
        threshold.props.baseline = Some(baseline);
        threshold.props.caps = Some(caps);
        let mut zero = threshold.clone();
        zero.props.kern = Some(0.0);
        let mut disabled = threshold.clone();
        disabled.props.kern = Some(24.0);
        let actual = sample(vec![threshold]);
        let control = sample(vec![zero]);
        assert_eq!(glyph_x(&actual), glyph_x(&control));
        assert!(sample(vec![disabled]).content_width > actual.content_width);
    }
}
