use super::*;
use kition_slide_model::style::Reflection;
use kition_slide_model::text::{LevelStyle, Run};
use kition_slide_model::{LayoutType, SlideFile, Xfrm, defaults};

fn reflection() -> Reflection {
    Reflection { blur: 1.25, start_alpha: 0.7, end_alpha: 0.1, start_pos: 0.2, end_pos: 0.8, dist: 3.0, sy: -0.75, ..Default::default() }
}

fn sample(mut runs: Vec<Run>, inherited: Option<Reflection>, bullet: Bullet) -> TextLayout {
    let mut pres = SlideFile::default();
    let layout_id = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, layout_id);
    let mut shape = slide.shapes.remove(1);
    shape.ph = None;
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, 500.0, 200.0));
    let mut body = TextBody::from_text("");
    body.body.autofit = Some(AutoFit::None);
    body.paragraphs[0].props.bullet = Some(bullet);
    for run in &mut runs {
        run.props.font = Some("Inter".into());
        run.props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
        run.props.size = Some(24.0);
    }
    body.paragraphs[0].runs = runs;
    body.list_style.set(0, LevelStyle { run: RunProps { reflection: inherited, ..Default::default() }, ..Default::default() });
    shape.text = Some(body);
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    layout(
        &ctx,
        &shape,
        shape.text.as_ref().unwrap(),
        &Opts { rect: Rect::new(0.0, 0.0, 500.0, 200.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false },
    )
}

fn assert_same_geometry(actual: &TextLayout, plain: &TextLayout) {
    assert_eq!((actual.content_width, actual.content_height), (plain.content_width, plain.content_height));
    assert_eq!(actual.runs.len(), plain.runs.len());
    for (actual, plain) in actual.runs.iter().zip(&plain.runs) {
        assert!(Arc::ptr_eq(&actual.face, &plain.face));
        assert_eq!(actual.size, plain.size);
        assert_eq!(actual.glyphs, plain.glyphs);
        assert_eq!(actual.chars, plain.chars);
    }
    assert_eq!(actual.lines.len(), plain.lines.len());
    for (actual, plain) in actual.lines.iter().zip(&plain.lines) {
        assert_eq!(actual.caret_x, plain.caret_x);
        assert_eq!(actual.edges, plain.edges);
        assert_eq!((actual.top, actual.baseline, actual.bottom), (plain.top, plain.baseline, plain.bottom));
    }
}

#[test]
fn inherited_reflection_survives_font_fallback_and_style_splits_without_reflow() {
    let cjk = char::from_u32(0x4e00).unwrap();
    let runs = vec![Run::new(format!("A{cjk}B")), Run::with("C", RunProps { bold: Some(true), ..Default::default() })];
    let actual = sample(runs.clone(), Some(reflection()), Bullet::None);
    let plain = sample(runs, None, Bullet::None);
    assert!(actual.runs.len() >= 3);
    assert!(actual.runs.iter().any(|run| !Arc::ptr_eq(&run.face, &actual.runs[0].face)));
    assert!(actual.runs.iter().all(|run| run.reflection.as_ref() == Some(&reflection())));
    assert_same_geometry(&actual, &plain);
}

#[test]
fn explicit_effect_lists_clear_or_replace_inherited_reflection() {
    let replacement = Reflection { dist: 9.0, ..reflection() };
    let runs = vec![
        Run::new("A"),
        Run::with("B", RunProps { effects_explicit: true, ..Default::default() }),
        Run::with("C", RunProps { reflection: Some(replacement.clone()), effects_explicit: true, ..Default::default() }),
    ];
    let actual = sample(runs, Some(reflection()), Bullet::None);
    assert_eq!(actual.runs.len(), 3);
    assert_eq!(actual.runs[0].reflection.as_ref(), Some(&reflection()));
    assert_eq!(actual.runs[1].reflection, None);
    assert_eq!(actual.runs[2].reflection.as_ref(), Some(&replacement));
}

#[test]
fn formula_and_bullet_emissions_preserve_resolved_reflection_without_reflow() {
    let mut formula = Run::new("x+1");
    formula.kind = RunKind::Math {
        omml: r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x+1</m:t></m:r></m:oMath>"#.into(),
    };
    for bullet in [Bullet::Char { char: "*".into() }, Bullet::AutoNum { scheme: "arabicPeriod".into(), start_at: 1 }] {
        let actual = sample(vec![formula.clone()], Some(reflection()), bullet.clone());
        let plain = sample(vec![formula.clone()], None, bullet.clone());
        assert!(actual.runs.iter().any(|run| run.chars == (0, 0)), "bullet emitted");
        assert!(actual.runs.iter().any(|run| run.chars == (0, 3)), "formula emitted");
        assert!(actual.runs.iter().all(|run| run.reflection.as_ref() == Some(&reflection())));
        assert_same_geometry(&actual, &plain);

        let mut cleared = formula.clone();
        cleared.props.effects_explicit = true;
        let cleared = sample(vec![cleared], Some(reflection()), bullet);
        assert!(cleared.runs.iter().all(|run| run.reflection.is_none()));
    }
}
