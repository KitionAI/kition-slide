use super::*;
use kition_slide_model::text::{Paragraph, Run};
use kition_slide_model::{LayoutType, SlideFile, Xfrm, defaults};

const EQUATION: &str = "<m:oMath><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f></m:oMath>";

fn display(properties: &str) -> String {
    format!("<m:oMathPara>{properties}{EQUATION}</m:oMathPara>")
}

fn run(xml: String) -> Run {
    Run {
        text: "12".into(),
        props: RunProps { font: Some("Inter".into()), size: Some(24.0), ..Default::default() },
        kind: RunKind::Math { omml: xml },
    }
}

fn aligned_run(value: &str) -> Run {
    run(display(&format!("<m:oMathParaPr><m:jc m:val=\"{value}\"/></m:oMathParaPr>")))
}

fn laid(runs: Vec<Run>, outer: Align, vert: TextDir, bullet: Bullet) -> TextLayout {
    laid_with_width(runs, outer, vert, bullet, 300.0)
}

fn laid_with_width(runs: Vec<Run>, outer: Align, vert: TextDir, bullet: Bullet, width: f64) -> TextLayout {
    let mut pres = SlideFile::default();
    let id = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, id);
    let mut shape = slide.shapes.remove(1);
    shape.ph = None;
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, width, 200.0));
    let mut body = TextBody::default();
    body.body.inset_l = Some(10.0);
    body.body.inset_r = Some(20.0);
    body.body.inset_t = Some(5.0);
    body.body.inset_b = Some(5.0);
    body.body.autofit = Some(AutoFit::None);
    body.body.wrap = Some(false);
    body.body.vert = Some(vert);
    body.paragraphs = vec![Paragraph {
        runs,
        props: ParaProps {
            align: Some(outer),
            bullet: Some(bullet),
            margin_left: Some(0.0),
            margin_right: Some(0.0),
            indent: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    }];
    shape.text = Some(body.clone());
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let result = crate::layout(
        &ctx,
        &shape,
        &body,
        &Opts { rect: Rect::new(0.0, 0.0, width, 200.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false },
    );
    assert_eq!(shape.text.as_ref().unwrap(), &body, "display alignment is layout data, not a source mutation");
    result
}

fn equivalent(a: &TextLayout, b: &TextLayout) {
    assert_eq!(a.inner, b.inner);
    assert_eq!(a.content_height, b.content_height);
    assert_eq!(a.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>(), b.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>());
    assert_eq!(a.decos.iter().map(|d| d.rect).collect::<Vec<_>>(), b.decos.iter().map(|d| d.rect).collect::<Vec<_>>());
    assert_eq!(a.lines.len(), b.lines.len());
    for (a, b) in a.lines.iter().zip(&b.lines) {
        assert_eq!((a.top, a.baseline, a.bottom), (b.top, b.baseline, b.bottom));
        assert_eq!(a.caret_x, b.caret_x);
    }
}

#[test]
fn display_defaults_and_explicit_values_follow_math_justification() {
    for properties in ["", "<m:oMathParaPr/>", "<m:oMathParaPr><m:jc/></m:oMathParaPr>"] {
        assert_eq!(alignment(&display(properties)), Some(Align::Center));
    }
    for (value, expected) in [("left", Align::Left), ("right", Align::Right), ("center", Align::Center), ("centerGroup", Align::Center)] {
        assert_eq!(alignment(&display(&format!("<m:oMathParaPr><m:jc m:val=\"{value}\"/></m:oMathParaPr>"))), Some(expected));
    }
}

#[test]
fn malformed_foreign_and_multi_equation_paragraphs_keep_host_alignment() {
    for xml in [
        EQUATION.to_owned(),
        "<m:oMathPara/>".into(),
        format!("<m:oMathPara>{EQUATION}{EQUATION}</m:oMathPara>"),
        format!("{}{}", display(""), display("")),
        format!("<a:oMathPara>{EQUATION}</a:oMathPara>"),
        display("<a:oMathParaPr><a:jc val=\"right\"/></a:oMathParaPr>"),
        display("<m:oMathParaPr><a:jc val=\"right\"/></m:oMathParaPr>"),
        display("<m:oMathParaPr><m:jc m:val=\"invalid\"/></m:oMathParaPr>"),
        display("<m:oMathParaPr><m:jc m:val=\"\"/></m:oMathParaPr>"),
        display("<m:oMathParaPr><m:jc/><m:jc/></m:oMathParaPr>"),
        display("<m:oMathParaPr/><m:oMathParaPr/>"),
        format!("text{}", display("")),
        "<m:oMathPara>".into(),
    ] {
        assert_eq!(alignment(&xml), None, "{xml}");
    }
}

#[test]
fn standalone_alignment_moves_glyphs_rules_selection_and_carets_together_without_baseline_changes() {
    let left = laid(vec![aligned_run("left")], Align::Right, TextDir::Horizontal, Bullet::None);
    let line = &left.lines[0];
    let start = Pos { para: 0, ch: 0 };
    let end = Pos { para: 0, ch: 2 };
    let width = left.caret(end).unwrap().0 - left.caret(start).unwrap().0;
    for (value, factor) in [("center", 0.5), ("centerGroup", 0.5), ("right", 1.0)] {
        let actual = laid(vec![aligned_run(value)], Align::Left, TextDir::Horizontal, Bullet::None);
        let shift = (left.inner.width() - width) * factor;
        assert_eq!((actual.lines[0].top, actual.lines[0].baseline, actual.lines[0].bottom), (line.top, line.baseline, line.bottom));
        for pos in [start, end] {
            let (x, top, bottom) = actual.caret(pos).unwrap();
            assert!((x - left.caret(pos).unwrap().0 - shift).abs() < 1e-8);
            let inside = if pos == start { 0.1 } else { -0.1 };
            assert_eq!(actual.hit(Point::new(x + inside, (top + bottom) / 2.0)), pos);
        }
        for (a, b) in actual.runs.iter().zip(&left.runs) {
            for (a, b) in a.glyphs.iter().zip(&b.glyphs) {
                assert!((a.1 - b.1 - shift).abs() < 1e-8);
                assert_eq!(a.2, b.2);
            }
        }
        assert!(!left.decos.is_empty());
        for (a, b) in actual.decos.iter().zip(&left.decos) {
            assert!((a.rect.x0 - b.rect.x0 - shift).abs() < 1e-8);
            assert_eq!(a.rect.y0, b.rect.y0);
        }
        let selection = actual.selection_rects(start, end);
        let old = left.selection_rects(start, end);
        assert_eq!(selection.len(), old.len());
        assert!((selection[0].x0 - old[0].x0 - shift).abs() < 1e-8);
    }
}

#[test]
fn only_truly_empty_text_runs_are_ignored_for_standalone_detection() {
    let bare = laid(vec![aligned_run("center")], Align::Left, TextDir::Horizontal, Bullet::None);
    let mut empty = Run::new("");
    empty.props = aligned_run("center").props;
    let padded = laid(vec![empty.clone(), aligned_run("center"), empty], Align::Left, TextDir::Horizontal, Bullet::None);
    equivalent(&bare, &padded);
    for neighbor in [
        Run::new(" "),
        Run::new("text"),
        Run { text: String::new(), props: Default::default(), kind: RunKind::Break },
        Run { text: String::new(), props: Default::default(), kind: RunKind::Field { field: "unknown".into() } },
        aligned_run("center"),
    ] {
        let left = laid(vec![aligned_run("left"), neighbor.clone()], Align::Left, TextDir::Horizontal, Bullet::None);
        let right = laid(vec![aligned_run("right"), neighbor], Align::Left, TextDir::Horizontal, Bullet::None);
        equivalent(&left, &right);
    }
}

#[test]
fn bullets_vertical_and_inline_formulas_keep_outer_alignment() {
    for (vert, bullet) in
        [(TextDir::Horizontal, Bullet::Char { char: "*".into() }), (TextDir::Vertical, Bullet::None), (TextDir::EaVertical, Bullet::None)]
    {
        let left = laid(vec![aligned_run("left")], Align::Left, vert, bullet.clone());
        let right = laid(vec![aligned_run("right")], Align::Left, vert, bullet);
        equivalent(&left, &right);
    }
    // A display equation is set in display style (a fraction grows), so the inline one is
    // compared on an equation that looks the same either way.
    let plain = "<m:oMath><m:r><m:t>12</m:t></m:r></m:oMath>";
    let inline = laid(vec![run(plain.into())], Align::Right, TextDir::Horizontal, Bullet::None);
    let display = laid(
        vec![run(format!("<m:oMathPara><m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>{plain}</m:oMathPara>"))],
        Align::Left,
        TextDir::Horizontal,
        Bullet::None,
    );
    equivalent(&inline, &display);
}

#[test]
fn non_fitting_display_equations_retain_authored_left_and_right_overflow_placement() {
    let equation = "<m:oMath><m:r><m:t>abcdefghijklmnopqrstuv</m:t></m:r></m:oMath>";
    for outer in [Align::Left, Align::Right] {
        let original = laid_with_width(vec![run(equation.into())], outer, TextDir::Horizontal, Bullet::None, 70.0);
        let line = &original.lines[0];
        assert!(line.extent.1 - line.extent.0 > original.inner.width());
        for justification in ["left", "right", "center", "centerGroup"] {
            let xml = format!("<m:oMathPara><m:oMathParaPr><m:jc m:val=\"{justification}\"/></m:oMathParaPr>{equation}</m:oMathPara>");
            let actual = laid_with_width(vec![run(xml)], outer, TextDir::Horizontal, Bullet::None, 70.0);
            equivalent(&original, &actual);
            for ch in [0, 2] {
                assert_eq!(actual.caret(Pos { para: 0, ch }), original.caret(Pos { para: 0, ch }));
            }
        }
    }
}
