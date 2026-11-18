use super::*;
use kition_slide_color::{ColorTransform, Rgba, SchemeSlot};
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorRef, Fill, GradientStop};
use kition_slide_model::text::TextBody;
use kition_slide_model::{Shape, SlideFile};
use kition_slide_text::{NoFields, Opts};
use kurbo::Rect;

fn gradient(first: Rgba, last: Rgba, angle: f64) -> Gradient {
    Gradient {
        stops: vec![GradientStop { pos: 0.0, color: ColorRef::rgb(first) }, GradientStop { pos: 1.0, color: ColorRef::rgb(last) }],
        shape: GradientShape::Linear { angle, scaled: false },
        tile_rect: None,
        rotate_with_shape: true,
    }
}

fn layout(text: &str, g: Gradient) -> TextLayout {
    let p = SlideFile::default();
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let mut body = TextBody::from_text(text);
    for para in &mut body.paragraphs {
        for run in &mut para.runs {
            run.props.font = Some("Ubuntu".into());
            run.props.size = Some(64.0);
            run.props.fill = Some(Fill::Gradient(g.clone()));
        }
    }
    let shape = Shape { text: Some(body.clone()), ..Default::default() };
    kition_slide_text::layout(
        &ctx,
        &shape,
        &body,
        &Opts { rect: Rect::new(0.0, 0.0, 400.0, 240.0), fields: &NoFields, prompt_color: None, no_shrink: false },
    )
}

fn render(l: &TextLayout, opacity: f64) -> crate::Image {
    let mut ctx = RenderContext::new_with(400, 240, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    ctx.set_paint(color(Rgba::WHITE, 1.0));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 400.0, 240.0));
    crate::draw_layout_with(
        &mut ctx,
        l,
        Rect::new(0.0, 0.0, 400.0, 240.0),
        Affine::IDENTITY,
        &[(0, crate::ParaState { opacity, ..Default::default() })],
    );
    crate::Renderer::default().finish(ctx, 400, 240)
}

#[test]
fn horizontal_gradient_uses_one_frame_across_glyphs_and_font_fragments() {
    let mut l = layout("MMMM", gradient(Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255), 0.0));
    assert_eq!(l.runs.len(), 1);
    let original = render(&l, 1.0);
    let mut left = l.runs[0].clone();
    let mut right = left.clone();
    left.glyphs.truncate(2);
    left.cells.truncate(2);
    left.chars.1 = 2;
    right.glyphs.drain(..2);
    right.cells.drain(..2);
    right.chars.0 = 2;
    l.runs = vec![left, right];
    assert_eq!(group_end(&l, 0), 2);
    assert_eq!(render(&l, 1.0).pixels, original.pixels, "font fragmentation cannot reset gradient coordinates");
    let red = original.pixels.as_chunks::<4>().0.iter().filter(|p| p[0] > 220 && p[1] < 40 && p[2] < 100).count();
    let blue = original.pixels.as_chunks::<4>().0.iter().filter(|p| p[2] > 220 && p[1] < 40 && p[0] < 100).count();
    assert!(red > 30 && blue > 30, "both ends of the run must receive their own stop colour");
}

#[test]
fn transparent_first_stop_does_not_hide_rest_and_animation_alpha_is_once() {
    let l = layout("MMMM", gradient(Rgba::BLACK.with_alpha(0), Rgba::BLACK, 0.0));
    let full = render(&l, 1.0);
    let half = render(&l, 0.5);
    assert!(full.pixels.as_chunks::<4>().0.iter().filter(|p| p[0] < 80).count() > 50);
    assert!(half.pixels.as_chunks::<4>().0.iter().all(|p| p[0] >= 126), "paragraph opacity must only be applied once");
    assert!(half.pixels.as_chunks::<4>().0.iter().any(|p| p[0] < 200));
}

#[test]
fn line_frames_restart_but_distinct_gradients_do_not_merge() {
    let mut l = layout("MM\nMM", gradient(Rgba::WHITE, Rgba::BLACK, 90.0));
    assert_eq!(l.lines.len(), 2);
    assert_eq!(group_end(&l, 0), 1);
    assert_eq!(group_end(&l, 1), 2);
    let mut second = l.runs[0].clone();
    second.chars = (1, 2);
    second.gradient = Some(gradient(Rgba::BLACK, Rgba::WHITE, 0.0));
    l.runs.insert(1, second);
    assert_eq!(group_end(&l, 0), 1);
}

#[test]
fn theme_stops_and_alpha_are_resolved_before_rendering() {
    let mut g = gradient(Rgba::BLACK, Rgba::WHITE, 0.0);
    g.stops[0].color = ColorRef::scheme(SchemeSlot::Accent1).with(ColorTransform::Alpha(50000));
    let p = SlideFile::default();
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let expected = ctx.color(&g.stops[0].color, None);
    let l = layout("M", g);
    assert_eq!(l.runs[0].gradient.as_ref().unwrap().stops[0].color, ColorRef::rgb(expected));
}

#[test]
fn outline_is_retained_when_all_gradient_stops_are_transparent() {
    let mut l = layout("MM", gradient(Rgba::TRANSPARENT, Rgba::TRANSPARENT, 0.0));
    l.runs[0].outline = Some((Rgba::rgb(0, 0, 255), 2.0));
    let image = render(&l, 1.0);
    assert!(image.pixels.as_chunks::<4>().0.iter().filter(|p| p[0] < 40 && p[1] < 40 && p[2] == 255).count() > 100);
}

#[test]
fn invalid_gradient_coordinates_are_not_sent_to_the_backend() {
    let mut l = layout("MM", gradient(Rgba::BLACK, Rgba::WHITE, 0.0));
    l.runs[0].gradient.as_mut().unwrap().stops[0].pos = f64::NAN;
    assert!(render(&l, 1.0).pixels.as_chunks::<4>().0.iter().all(|p| *p == [255; 4]));
    l.runs[0].gradient = Some(gradient(Rgba::BLACK, Rgba::WHITE, f64::INFINITY));
    assert!(render(&l, 1.0).pixels.as_chunks::<4>().0.iter().all(|p| *p == [255; 4]));
    l.runs[0].gradient = Some(gradient(Rgba::BLACK, Rgba::WHITE, 0.0));
    for (_, x, _) in &mut l.runs[0].glyphs {
        *x += 1e12;
    }
    assert!(render(&l, 1.0).pixels.as_chunks::<4>().0.iter().all(|p| *p == [255; 4]));
}

#[test]
fn excessive_outline_extent_is_rejected_without_hiding_the_fill() {
    let mut l = layout("MM", gradient(Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255), 0.0));
    let before = render(&l, 1.0);
    l.runs[0].outline = Some((Rgba::BLACK, 1e12));
    assert_eq!(render(&l, 1.0).pixels, before.pixels);
}

#[test]
fn prepared_gradient_uses_new_ink_and_one_frame_across_fragments() {
    let l = layout("M", gradient(Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255), 0.0));
    let runs = vec![l.runs[0].clone(); 2];
    let paths = [Rect::new(20.0, 20.0, 60.0, 60.0).to_path(0.1), Rect::new(60.0, 20.0, 100.0, 60.0).to_path(0.1)];
    let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_paths(&mut ctx, &runs, &paths, Affine::translate((10.0, 10.0)), 0.5);
    let image = crate::Renderer::default().finish(ctx, 160, 120);
    let left = image.pixel(35, 40);
    let right = image.pixel(105, 40);
    assert!(left[0] > left[2] * 5 && right[2] > right[0] * 5);
    assert!((127..=128).contains(&left[3]) && left[3] == right[3]);
    assert_eq!(image.pixel(20, 40), [0; 4]);
    assert_eq!(image.pixel(35, 75), [0; 4]);
}

#[test]
fn mismatched_prepared_gradient_paths_are_rejected() {
    let l = layout("M", gradient(Rgba::BLACK, Rgba::WHITE, 0.0));
    let mut ctx = RenderContext::new_with(160, 120, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    draw_paths(&mut ctx, &l.runs, &[], Affine::IDENTITY, 1.0);
    assert!(crate::Renderer::default().finish(ctx, 160, 120).pixels.iter().all(|v| *v == 0));
}
