//! Art text: text bent by its body's preset text warp (`a:prstTxWarp`, KitDrawing 1.3 DR26).
//!
//! The text is laid out flat as usual; its glyph outlines are then stretched over the text
//! rect, as PowerPoint fills the box with warped text, and bent by the shared warp geometry in
//! `kition_office_core::warp`, so Word, Sheet and Slide bend the same preset alike.

use kurbo::{Affine, BezPath, Rect, Shape as _};
use vello_cpu::RenderContext;

use crate::{color, glyph};

mod follow;
mod paint;

/// The glyph outlines of `run`, in the layout's coordinates.
fn glyph_outline(run: &kition_slide_text::GlyphRun, gid: u32, x: f64, y: f64) -> BezPath {
    let k = run.size / run.face.upem.max(1.0);
    let skew = if run.fake_italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
    let turn = if run.upright { Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { Affine::IDENTITY };
    Affine::translate((x, y)) * turn * skew * Affine::scale(k) * &*glyph(&run.face, gid)
}

/// Draw only verified single-path placement; `false` leaves the ordinary painter in charge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_draw(
    ctx: &mut RenderContext,
    rctx: &kition_slide_model::resolve::Ctx,
    l: &kition_slide_text::TextLayout,
    tr: Rect,
    m: Affine,
    preset: &str,
    adjust: &[(String, f64)],
    anchor: kition_slide_model::text::Anchor,
    paras: &[(usize, crate::ParaState)],
) -> bool {
    let Some(paths) = follow::prepare(l, tr, preset, adjust, anchor) else { return false };
    paint::draw(ctx, rctx, l, &paths, m, paras);
    true
}

#[cfg(test)]
mod integration_tests;

/// Does `preset` bend text (a warp this renderer draws, not `textPlain`)?
pub(crate) fn bends(preset: Option<&str>) -> bool {
    preset.is_some_and(|p| p != "textPlain" && kition_office_core::warp::is_warp(p))
}

/// The glyph outlines of `run`, in the layout's coordinates.
fn run_outlines(run: &kition_slide_text::GlyphRun) -> Vec<BezPath> {
    let k = run.size / run.face.upem.max(1.0);
    let skew = if run.fake_italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
    let turn = if run.upright { Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { Affine::IDENTITY };
    run.glyphs
        .iter()
        .filter_map(|&(gid, x, y)| {
            let path = glyph(&run.face, gid);
            (!path.elements().is_empty()).then(|| Affine::translate((x, y)) * turn * skew * Affine::scale(k) * &*path)
        })
        .collect()
}

/// Draw laid-out text `l` in text rect `tr`, bent by `preset`, through `m` (the text transform).
pub(crate) fn draw(ctx: &mut RenderContext, l: &kition_slide_text::TextLayout, tr: Rect, m: Affine, preset: &str, adjust: &[(String, f64)]) {
    let runs: Vec<(&kition_slide_text::GlyphRun, Vec<BezPath>)> = l.runs.iter().map(|r| (r, run_outlines(r))).collect();
    let ink = runs.iter().flat_map(|(_, ps)| ps.iter()).map(|p| p.bounding_box()).reduce(|a, b| a.union(b));
    let Some(ink) = ink.filter(|b| b.width() > 0.0 && b.height() > 0.0) else { return };
    let (w, h) = (tr.width().max(1.0), tr.height().max(1.0));
    // The ink fills the text rect, then bends with it.
    let fit = Affine::translate(tr.origin().to_vec2())
        * Affine::scale_non_uniform(w / ink.width(), h / ink.height())
        * Affine::translate(-ink.origin().to_vec2());
    let back = Affine::translate(tr.origin().to_vec2());
    let local = back.inverse() * fit;
    ctx.set_transform(m * back);
    for (run, paths) in &runs {
        let alpha = run.alpha;
        for p in paths {
            let bent = kition_office_core::warp::warp_path_adj(preset, adjust, w, h, &(local * p));
            if alpha > 0.0 && run.color.a > 0 {
                ctx.set_paint(color(run.color, alpha));
                ctx.fill_path(&bent);
            }
            if let Some((oc, ow)) = run.outline {
                ctx.set_paint(color(oc, 1.0));
                ctx.set_stroke(kurbo::Stroke::new(ow).with_join(kurbo::Join::Round));
                ctx.stroke_path(&bent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{RenderOpts, render_slide};
    use kition_slide_model::text::TextBody;
    use kition_slide_model::{Shape, ShapeId, SlideFile, Xfrm};

    fn render(warp: Option<&str>) -> crate::Image {
        render_aligned(warp, None)
    }

    fn render_aligned(warp: Option<&str>, align: Option<kition_slide_model::text::Align>) -> crate::Image {
        let mut t = TextBody::from_text("WAVE");
        t.paragraphs[0].runs[0].props.size = Some(40.0);
        t.paragraphs[0].props.align = align;
        t.body.warp = warp.map(String::from);
        let sh = Shape { id: ShapeId(700), xfrm: Some(Xfrm::new(100.0, 100.0, 400.0, 200.0)), text_box: true, text: Some(t), ..Default::default() };
        let mut p = SlideFile::default();
        std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
        render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() })
    }

    fn dark(img: &crate::Image, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> usize {
        y.flat_map(|y| x.clone().map(move |x| (x, y))).filter(|&(x, y)| img.pixel(x, y)[0] < 128).count()
    }

    #[test]
    fn warped_text_fills_and_bends_with_its_box() {
        let flat = render(None);
        let arch = render_aligned(Some("textArchUp"), Some(kition_slide_model::text::Align::Center));
        // Flat text sits at the top left; one line of arched text (Follow Path) runs along the
        // arch at its own size: it crosses the top middle in a band
        // about as high as the text, leaving the middle and bottom of the box empty.
        assert_eq!(dark(&flat, 280..320, 100..130), 0);
        assert!(dark(&arch, 280..320, 100..130) > 20);
        assert_eq!(dark(&arch, 250..350, 160..300), 0);
        // Left-aligned, it starts at the left foot of the arch.
        let left = render(Some("textArchUp"));
        assert!(dark(&left, 100..160, 150..260) > 20 && dark(&left, 280..320, 100..130) == 0);
        // Plain is flat text; an unknown preset is drawn flat too.
        assert_eq!(render(Some("textPlain")).pixels, flat.pixels);
        assert_eq!(render(Some("textSpiral")).pixels, flat.pixels);
        // Inflate fills the box: ink from the top to the bottom in the middle.
        let inflate = render(Some("textInflate"));
        assert!(dark(&inflate, 100..500, 100..115) > 0 && dark(&inflate, 100..500, 285..300) > 0);
    }
}
