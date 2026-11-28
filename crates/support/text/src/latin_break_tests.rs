use super::*;
use kition_slide_model::{LayoutType, SlideFile, Xfrm, defaults};

fn sample(text: &str, latin_break: Option<bool>, width: f64) -> (TextLayout, ParaShaped) {
    let mut pres = SlideFile::default();
    let layout_id = defaults::layout_of_kind(&pres, LayoutType::TitleAndContent).unwrap();
    let mut slide = defaults::new_slide(&mut pres, layout_id);
    let mut shape = slide.shapes.remove(1);
    shape.xfrm = Some(Xfrm::new(0.0, 0.0, width, 400.0));
    let mut body = TextBody::from_text(text);
    body.body.autofit = Some(AutoFit::None);
    body.body.wrap = Some(true);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    let para = &mut body.paragraphs[0];
    para.props.bullet = Some(Bullet::None);
    para.props.align = Some(Align::Left);
    para.props.margin_left = Some(0.0);
    para.props.indent = Some(0.0);
    para.props.latin_line_break = latin_break;
    para.runs[0].props = RunProps { font: Some("Inter".into()), size: Some(24.0), ..Default::default() };
    shape.text = Some(body);
    slide.shapes = vec![shape.clone()];
    pres.slides = vec![Arc::new(slide)];
    let ctx = Ctx::for_slide(&pres, &pres.slides[0]).unwrap();
    let opts = Opts { rect: Rect::new(0.0, 0.0, width, 400.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false };
    let body = shape.text.as_ref().unwrap();
    (layout(&ctx, &shape, body, &opts), shape_para(&ctx, &shape, body, 0, 1.0, &opts, None, false))
}

fn ranges(layout: &TextLayout) -> Vec<(usize, usize)> {
    layout.lines.iter().map(|line| (line.start, line.end)).collect()
}

#[test]
fn enabled_latin_wrapping_uses_available_space_inside_a_word() {
    let text = "AA BBBBBB";
    let (wide, _) = sample(text, None, 1000.0);
    let width = wide.lines[0].caret_x[5] + 0.1;
    let (enabled, _) = sample(text, Some(true), width);
    let (disabled, _) = sample(text, Some(false), width);
    let (absent, _) = sample(text, None, width);
    assert_eq!(enabled.lines[0].end, 5);
    assert_eq!(disabled.lines[0].end, 2);
    assert_eq!(ranges(&disabled), ranges(&absent));
    assert_eq!(enabled.lines[1].start, 5);
    let pos = Pos { para: 0, ch: 4 };
    let (x, top, bottom) = enabled.caret(pos).unwrap();
    assert_eq!(enabled.hit(Point::new(x, (top + bottom) / 2.0)), pos);
}

#[test]
fn enabled_latin_wrapping_preserves_graphemes_and_shaping_clusters() {
    let accent = char::from_u32(0x301).unwrap();
    let text = format!("AA e{accent}e{accent} office");
    let (_, shaped) = sample(&text, Some(true), 1000.0);
    assert!(shaped.cells.iter().enumerate().any(|(i, cell)| !cell.breakable(i)));
    // Sweep across cluster widths, including a combining sequence and any font ligature.
    for width in [40.0, 55.0, 70.0, 85.0, 100.0] {
        let (layout, _) = sample(&text, Some(true), width);
        for line in &layout.lines {
            for boundary in [line.start, line.end] {
                assert!(shaped.cells.get(boundary).is_none_or(|cell| cell.breakable(boundary)), "boundary {boundary}");
            }
        }
    }
}

#[test]
fn latin_wrapping_does_not_add_breaks_inside_other_scripts_or_at_word_joiners() {
    let greek: String = [0x391, 0x392, 0x393, 0x394, 0x395, 0x396].into_iter().map(|cp| char::from_u32(cp).unwrap()).collect();
    let joiner = char::from_u32(0x2060).unwrap();
    for text in [format!("AA {greek}"), format!("AA B{joiner}B")] {
        let (wide, _) = sample(&text, None, 1000.0);
        let width = wide.lines[0].caret_x[4] + 0.1;
        let (enabled, _) = sample(&text, Some(true), width);
        let (disabled, _) = sample(&text, Some(false), width);
        assert_eq!(ranges(&enabled), ranges(&disabled));
    }
}
