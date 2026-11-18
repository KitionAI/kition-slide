//! Adapter eligibility tests independent of the caller's legacy-envelope fallback policy.

use kition_slide_color::Rgba;
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorRef, Fill};
use kition_slide_model::text::{Align, Anchor, Bullet, RunProps, Strike, TextBody};
use kition_slide_model::{Shape, SlideFile};
use kition_slide_text::{NoFields, Opts, TextLayout};
use kurbo::{Affine, Rect};
use vello_cpu::RenderContext;

const RECT: Rect = Rect::new(20.0, 40.0, 280.0, 140.0);

fn layout(ctx: &Ctx, text: &str, props: RunProps) -> TextLayout {
    let mut body = TextBody::from_text(text);
    body.body.anchor = Some(Anchor::Middle);
    body.body.wrap = Some(false);
    for para in &mut body.paragraphs {
        para.props.align = Some(Align::Center);
        para.props.bullet = Some(Bullet::None);
        for run in &mut para.runs {
            run.props =
                RunProps { font: Some("Ubuntu".into()), size: Some(40.0), fill: Some(Fill::solid(ColorRef::rgb(Rgba::BLACK))), ..props.clone() };
        }
    }
    let shape = Shape { text: Some(body.clone()), text_box: true, ..Default::default() };
    kition_slide_text::layout(ctx, &shape, &body, &Opts { rect: RECT, fields: &NoFields, prompt_color: None, no_shrink: true })
}

/// Paint ordinary text first so a declined adapter must leave existing ink and decorations intact.
/// This deliberately does not invoke main's public dispatch or prescribe its legacy fallback.
fn ordinary_then_declined(ctx: &Ctx, layout: &TextLayout, attempt: Option<(&str, Anchor)>) -> crate::Image {
    let mut canvas = RenderContext::new_with(320, 180, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    canvas.set_paint(crate::color(Rgba::rgb(0, 80, 200), 1.0));
    canvas.fill_rect(&Rect::new(2.0, 2.0, 10.0, 10.0));
    crate::draw_layout_with_context(&mut canvas, ctx, layout, RECT, Affine::IDENTITY, &[]);
    if let Some((preset, anchor)) = attempt {
        assert!(!super::try_draw(&mut canvas, ctx, layout, RECT, Affine::IDENTITY, preset, &[], anchor, &[]), "{preset} {anchor:?}");
    }
    crate::Renderer::default().finish(canvas, 320, 180)
}

#[test]
fn unsupported_follow_modes_decline_without_painting() {
    let p = SlideFile::default();
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let line = layout(&ctx, "WAVE", RunProps::default());
    assert!(!line.runs.is_empty() && line.decos.is_empty());
    // Positive control: rejection assertions must not succeed because the fixture cannot shape.
    let mut canvas = RenderContext::new_with(320, 180, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
    assert!(super::try_draw(&mut canvas, &ctx, &line, RECT, Affine::IDENTITY, "textArchUp", &[], Anchor::Middle, &[]));
    let image = crate::Renderer::default().finish(canvas, 320, 180);
    assert!(image.pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
    let baseline = ordinary_then_declined(&ctx, &line, None);
    for preset in ["textNoShape", "textPlain", "textSpiral", "textInflate", "textArchUpPour", "textArchDownPour", "textCircle"] {
        assert_eq!(ordinary_then_declined(&ctx, &line, Some((preset, Anchor::Middle))).pixels, baseline.pixels, "{preset}");
    }
    for anchor in [Anchor::Justified, Anchor::Distributed] {
        for preset in ["textArchUp", "textArchDown"] {
            assert_eq!(ordinary_then_declined(&ctx, &line, Some((preset, anchor))).pixels, baseline.pixels, "{preset} {anchor:?}");
        }
    }
    // A single line anchored top or bottom follows the path exactly as a middle-anchored one.
    let follow = |anchor| {
        let mut canvas = RenderContext::new_with(320, 180, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
        assert!(super::try_draw(&mut canvas, &ctx, &line, RECT, Affine::IDENTITY, "textArchUp", &[], anchor, &[]), "{anchor:?}");
        crate::Renderer::default().finish(canvas, 320, 180).pixels
    };
    let middle = follow(Anchor::Middle);
    for anchor in [Anchor::Top, Anchor::Bottom] {
        assert_eq!(follow(anchor), middle, "{anchor:?}");
    }
    let multiline = layout(&ctx, "FIRST\nSECOND", RunProps::default());
    assert_eq!(multiline.lines.len(), 2);
    let baseline = ordinary_then_declined(&ctx, &multiline, None);
    for preset in ["textArchUp", "textArchDown"] {
        assert_eq!(ordinary_then_declined(&ctx, &multiline, Some((preset, Anchor::Middle))).pixels, baseline.pixels, "{preset}");
    }
}

fn visible_decoration_declines(props: RunProps) {
    let p = SlideFile::default();
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let plain = layout(&ctx, "WAVE", RunProps::default());
    let decorated = layout(&ctx, "WAVE", props);
    assert!(plain.decos.is_empty());
    assert!(!decorated.decos.is_empty());
    let baseline = ordinary_then_declined(&ctx, &decorated, None);
    assert_ne!(baseline.pixels, ordinary_then_declined(&ctx, &plain, None).pixels, "ordinary decoration must visibly change the fixture");
    for preset in ["textArchUp", "textArchDown"] {
        assert_eq!(ordinary_then_declined(&ctx, &decorated, Some((preset, Anchor::Middle))).pixels, baseline.pixels, "{preset}");
    }
}

#[test]
fn visible_underline_is_declined_without_painting() {
    visible_decoration_declines(RunProps { underline: Some("sng".into()), ..Default::default() });
}

#[test]
fn visible_strike_is_declined_without_painting() {
    visible_decoration_declines(RunProps { strike: Some(Strike::Single), ..Default::default() });
}

#[test]
fn visible_highlight_is_declined_without_painting() {
    visible_decoration_declines(RunProps { highlight: Some(ColorRef::rgb(Rgba::rgb(255, 230, 0))), ..Default::default() });
}
