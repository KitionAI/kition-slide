//! Invalidate imported autofit results when their layout inputs change.

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::{AutoFit, TextBody};
use kition_slide_model::{Shape, ShapeId, SlideFile, find_shape};

fn changed(old_ctx: &Ctx, old: &Shape, ctx: &Ctx, shape: &Shape) -> bool {
    let a = resolve::xfrm(old_ctx, old);
    let b = resolve::xfrm(ctx, shape);
    if old_ctx.master.theme.fonts != ctx.master.theme.fonts
        || (a.w, a.h) != (b.w, b.h)
        || old.geom != shape.geom
        || old.text != shape.text
        || old.text_box != shape.text_box
        || resolve::body(old_ctx, old) != resolve::body(ctx, shape)
    {
        return true;
    }
    if let Some(text) = &shape.text {
        for para in &text.paragraphs {
            if resolve::para(old_ctx, old, para) != resolve::para(ctx, shape, para)
                || resolve::run(old_ctx, old, para, &para.end_props) != resolve::run(ctx, shape, para, &para.end_props)
                || para.runs.iter().any(|run| resolve::run(old_ctx, old, para, &run.props) != resolve::run(ctx, shape, para, &run.props))
            {
                return true;
            }
        }
    }
    false
}

type Fits = BTreeMap<ShapeId, (Option<AutoFit>, Option<String>)>;

fn collect(old_ctx: &Ctx, old_shapes: &[Shape], ctx: &Ctx, shapes: &[Shape], root_shapes: &[Shape], ids: &mut Fits, repair: bool) {
    for shape in shapes {
        let current = resolve::body(ctx, shape);
        let transform = if current.vert == Some(kition_slide_model::text::TextDir::EaVertical) {
            crate::cmd::text::text_shape_transform(ctx, root_shapes, shape.id, kition_slide_geom::Affine::IDENTITY, 0)
                .unwrap_or_else(|| resolve::xfrm(ctx, shape).affine())
        } else {
            resolve::xfrm(ctx, shape).affine()
        };
        let stale = || {
            current.autofit_input.as_ref().is_some_and(|input| {
                let x = resolve::xfrm(ctx, shape);
                let geo = kition_slide_render::shape_geometry(shape, x.w, x.h);
                shape
                    .text
                    .as_ref()
                    .is_some_and(|body| *input != kition_slide_text::autofit_input_with_transform(ctx, shape, body, geo.text_rect, transform))
            })
        };
        let refit = if repair {
            matches!(current.autofit, Some(AutoFit::Shrink { .. })) && stale()
        } else {
            match find_shape(old_shapes, shape.id) {
                Some(old) => {
                    (matches!(resolve::body(old_ctx, old).autofit, Some(AutoFit::Shrink { .. }))
                        || matches!(current.autofit, Some(AutoFit::Shrink { .. })))
                        && (changed(old_ctx, old, ctx, shape) || stale())
                }
                // Imported results have no receipt and remain authoritative. Copies retain
                // computed results only while their new context still matches: duplication
                // can detach a placeholder and change its inherited typography.
                None => matches!(current.autofit, Some(AutoFit::Shrink { .. })) && (current.autofit_saved != Some(true) || stale()),
            }
        };
        if refit {
            let mut edited = shape.clone();
            let fit = if let Some(body) = &mut edited.text {
                clear(body);
                // Only the current resolved mode may request a new shrink result. A local
                // unset mode must keep following a parent that changed to None or Shape.
                if matches!(current.autofit, Some(AutoFit::Shrink { .. })) {
                    body.body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
                    let x = resolve::xfrm(ctx, &edited);
                    let geo = kition_slide_render::shape_geometry(&edited, x.w, x.h);
                    edited.text.as_ref().map(|body| {
                        let layout = kition_slide_text::layout_with_transform(
                            ctx,
                            &edited,
                            body,
                            &kition_slide_text::Opts {
                                rect: geo.text_rect,
                                fields: &kition_slide_text::NoFields,
                                prompt_color: None,
                                no_shrink: false,
                                empty_bullets: false,
                            },
                            transform,
                        );
                        (
                            AutoFit::Shrink { font_scale: layout.font_scale, line_reduction: layout.line_reduction },
                            kition_slide_text::autofit_input_with_transform(ctx, &edited, body, geo.text_rect, transform),
                        )
                    })
                } else {
                    None
                }
            } else {
                None
            };
            ids.insert(
                shape.id,
                match fit {
                    Some((fit, input)) => (Some(fit), Some(input)),
                    None => (None, None),
                },
            );
        }
        collect(old_ctx, old_shapes, ctx, shape.children(), root_shapes, ids, repair);
    }
}

fn clear(body: &mut TextBody) {
    // Explicit false also masks a saved result inherited from a placeholder.
    body.body.autofit_saved = Some(false);
    body.body.autofit_input = None;
}

fn apply(shapes: &mut [Shape], ids: &Fits) {
    for shape in shapes {
        if ids.contains_key(&shape.id)
            && let Some(body) = &mut shape.text
        {
            clear(body);
            if let Some((Some(fit), input)) = ids.get(&shape.id) {
                // The scale as the file stores it (C8-slide-5).
                body.body.autofit = Some(match *fit {
                    AutoFit::Shrink { font_scale, line_reduction } => AutoFit::Shrink {
                        font_scale: kition_slide_geom::stored::fraction(font_scale).clamp(0.01, 1.0),
                        line_reduction: kition_slide_geom::stored::fraction(line_reduction).clamp(0.0, 1.0),
                    },
                    other => other,
                });
                body.body.autofit_saved = Some(true);
                body.body.autofit_input.clone_from(input);
            }
        }
        if let Some(children) = shape.children_mut() {
            apply(children, ids);
        }
    }
}

/// The receipt a refit gives a shrink result, for `shape` as it is (no file stores receipts).
fn receipt(ctx: &Ctx, shape: &Shape, root_shapes: &[Shape]) -> Option<String> {
    let current = resolve::body(ctx, shape);
    let transform = if current.vert == Some(kition_slide_model::text::TextDir::EaVertical) {
        crate::cmd::text::text_shape_transform(ctx, root_shapes, shape.id, kition_slide_geom::Affine::IDENTITY, 0)
            .unwrap_or_else(|| resolve::xfrm(ctx, shape).affine())
    } else {
        resolve::xfrm(ctx, shape).affine()
    };
    let x = resolve::xfrm(ctx, shape);
    let geo = kition_slide_render::shape_geometry(shape, x.w, x.h);
    shape.text.as_ref().map(|body| kition_slide_text::autofit_input_with_transform(ctx, shape, body, geo.text_rect, transform))
}

/// Fresh receipts of imported saved results, including fingerprints kept in native files.
fn imported_receipts(ctx: &Ctx, shapes: &[Shape], root_shapes: &[Shape], out: &mut BTreeMap<ShapeId, String>) {
    for shape in shapes {
        if let Some(body) = &shape.text
            && matches!(body.body.autofit, Some(AutoFit::Shrink { .. }))
            && body.body.autofit_saved == Some(true)
            && let Some(input) = receipt(ctx, shape, root_shapes)
        {
            out.insert(shape.id, input);
        }
        imported_receipts(ctx, shape.children(), root_shapes, out);
    }
}

fn fill_receipts(shapes: &mut [Shape], receipts: &BTreeMap<ShapeId, String>) {
    for shape in shapes {
        if let (Some(input), Some(body)) = (receipts.get(&shape.id), shape.text.as_mut()) {
            body.body.autofit_input = Some(input.clone());
        }
        if let Some(children) = shape.children_mut() {
            fill_receipts(children, receipts);
        }
    }
}

/// Recomputes fingerprints when opening a file. PPTX omits them; native files can carry ones
/// computed before import normalization or by an older build. The imported saved scale is
/// authoritative, and later input changes invalidate it from this settled baseline.
///
/// Slides only: a master's or layout's result is inherited, receipt and all, by the placeholders
/// under it (`BodyProps::inherit`), which a refit gives receipts of their own; one given to the
/// master alone would read as stale in each of them. A master's or layout's receipt after a refit
/// is therefore not recomputed on load (a known difference).
pub(crate) fn receipts(doc: &mut SlideFile) {
    let mut slides = Vec::new();
    for (index, slide) in doc.slides.iter().enumerate() {
        let Some(ctx) = Ctx::for_slide(doc, slide) else { continue };
        let mut found = BTreeMap::new();
        imported_receipts(&ctx, &slide.shapes, &slide.shapes, &mut found);
        if !found.is_empty() {
            slides.push((index, found));
        }
    }
    for (index, found) in slides {
        if let Some(slide) = doc.slides.get_mut(index) {
            fill_receipts(&mut Arc::make_mut(slide).shapes, &found);
        }
    }
}

pub(crate) fn invalidate(doc: &mut SlideFile, old: &SlideFile) {
    update(doc, old, false);
}

/// Reconcile stale computed results after a character-level merge. Imported results have
/// no receipt and are authoritative. The caller must record repairs into its replica.
pub(crate) fn repair(doc: &mut SlideFile) -> bool {
    let old = doc.clone();
    update(doc, &old, true)
}

fn update(doc: &mut SlideFile, old: &SlideFile, repair: bool) -> bool {
    let inherited = doc.masters != old.masters || doc.default_text_style != old.default_text_style;
    // Source shape ids are local to each package part. Keep every update in its owner.
    let mut slides = Vec::new();
    let mut masters = Vec::new();
    let mut layouts = Vec::new();
    for (index, slide) in doc.slides.iter().enumerate() {
        let previous = old.slides.iter().find(|s| s.id == slide.id);
        if !repair && !inherited && previous.is_some_and(|p| Arc::ptr_eq(slide, p)) {
            continue;
        }
        let Some(ctx) = Ctx::for_slide(doc, slide) else { continue };
        let old_ctx = previous.and_then(|p| Ctx::for_slide(old, p));
        let mut ids = BTreeMap::new();
        collect(
            old_ctx.as_ref().unwrap_or(&ctx),
            previous.map_or(&[], |p| p.shapes.as_slice()),
            &ctx,
            &slide.shapes,
            &slide.shapes,
            &mut ids,
            repair,
        );
        if !ids.is_empty() {
            slides.push((index, ids));
        }
    }
    for (index, master) in doc.masters.iter().enumerate() {
        let previous = old.masters.iter().find(|m| m.id == master.id);
        if !repair && !inherited && previous.is_some_and(|p| Arc::ptr_eq(master, p)) {
            continue;
        }
        let ctx = Ctx::for_master(doc, master);
        let old_ctx = previous.map(|p| Ctx::for_master(old, p));
        let mut ids = BTreeMap::new();
        collect(
            old_ctx.as_ref().unwrap_or(&ctx),
            previous.map_or(&[], |p| p.shapes.as_slice()),
            &ctx,
            &master.shapes,
            &master.shapes,
            &mut ids,
            repair,
        );
        if !ids.is_empty() {
            masters.push((index, ids));
        }
        for (layout_index, layout) in master.layouts.iter().enumerate() {
            let prior = previous.and_then(|p| p.layouts.iter().find(|l| l.id == layout.id).map(|l| (p, l)));
            let ctx = Ctx::for_layout(doc, master, layout);
            let old_ctx = prior.map(|(m, l)| Ctx::for_layout(old, m, l));
            let mut ids = BTreeMap::new();
            collect(
                old_ctx.as_ref().unwrap_or(&ctx),
                prior.map_or(&[], |(_, l)| l.shapes.as_slice()),
                &ctx,
                &layout.shapes,
                &layout.shapes,
                &mut ids,
                repair,
            );
            if !ids.is_empty() {
                layouts.push((index, layout_index, ids));
            }
        }
    }
    let changed = !slides.is_empty() || !masters.is_empty() || !layouts.is_empty();
    for (index, ids) in slides {
        apply(&mut Arc::make_mut(&mut doc.slides[index]).shapes, &ids);
    }
    for (index, ids) in masters {
        apply(&mut Arc::make_mut(&mut doc.masters[index]).shapes, &ids);
    }
    for (index, layout, ids) in layouts {
        apply(&mut Arc::make_mut(&mut doc.masters[index]).layouts[layout].shapes, &ids);
    }
    changed
}

#[cfg(test)]
mod vertical_tests {
    use super::*;
    use kition_slide_geom::Affine;
    use kition_slide_model::{
        ShapeKind, Xfrm, defaults,
        text::{Bullet, TextDir},
    };

    fn document() -> SlideFile {
        let mut doc = SlideFile::default();
        let layout = doc.masters[0].layouts[0].id;
        let mut slide = defaults::new_slide(&mut doc, layout);
        let mut body = TextBody::from_text(&char::from_u32(0x4e00).unwrap().to_string().repeat(20));
        body.body.vert = Some(TextDir::EaVertical);
        body.body.autofit = Some(AutoFit::Shrink { font_scale: 0.83, line_reduction: 0.03 });
        body.body.autofit_saved = Some(true);
        body.body.inset_l = Some(20.0);
        body.body.inset_r = Some(20.0);
        body.body.inset_t = Some(2.0);
        body.body.inset_b = Some(2.0);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        body.paragraphs[0].runs[0].props.size = Some(24.0);
        let child = Shape { id: ShapeId(101), xfrm: Some(Xfrm::new(0.0, 0.0, 90.0, 100.0)), text: Some(body), ..Default::default() };
        slide.shapes = vec![Shape {
            id: ShapeId(100),
            xfrm: Some(Xfrm::new(0.0, 0.0, 90.0, 100.0)),
            kind: ShapeKind::Group { children: vec![child], child: Xfrm::new(0.0, 0.0, 90.0, 100.0) },
            ..Default::default()
        }];
        doc.slides = vec![Arc::new(slide)];
        doc
    }

    fn fresh_fit(doc: &SlideFile) -> (AutoFit, String) {
        let slide = &doc.slides[0];
        let ctx = Ctx::for_slide(doc, slide).unwrap();
        let mut shape = find_shape(&slide.shapes, ShapeId(101)).unwrap().clone();
        let transform = crate::cmd::text::text_shape_transform(&ctx, &slide.shapes, shape.id, Affine::IDENTITY, 0).unwrap();
        let bp = &mut shape.text.as_mut().unwrap().body;
        bp.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
        bp.autofit_saved = Some(false);
        bp.autofit_input = None;
        let x = resolve::xfrm(&ctx, &shape);
        let rect = kition_slide_render::shape_geometry(&shape, x.w, x.h).text_rect;
        let body = shape.text.as_ref().unwrap();
        let layout = kition_slide_text::layout_with_transform(
            &ctx,
            &shape,
            body,
            &kition_slide_text::Opts { rect, fields: &kition_slide_text::NoFields, prompt_color: None, no_shrink: false, empty_bullets: false },
            transform,
        );
        (
            AutoFit::Shrink { font_scale: layout.font_scale, line_reduction: layout.line_reduction },
            kition_slide_text::autofit_input_with_transform(&ctx, &shape, body, rect, transform),
        )
    }

    fn edit_text(doc: &mut SlideFile) {
        let old = doc.clone();
        let shape = kition_slide_model::find_shape_mut(&mut Arc::make_mut(&mut doc.slides[0]).shapes, ShapeId(101)).unwrap();
        shape.text.as_mut().unwrap().paragraphs[0].runs[0].text = char::from_u32(0x4e00).unwrap().to_string().repeat(30);
        invalidate(doc, &old);
    }

    fn assert_fresh(doc: &SlideFile) {
        let bp = &find_shape(&doc.slides[0].shapes, ShapeId(101)).unwrap().text.as_ref().unwrap().body;
        let (fit, input) = fresh_fit(doc);
        assert_eq!(bp.autofit, Some(fit));
        assert_eq!(bp.autofit_input.as_ref(), Some(&input));
    }

    #[test]
    fn edited_vertical_child_uses_its_transformed_group_layout() {
        let mut doc = document();
        Arc::make_mut(&mut doc.slides[0]).shapes[0].xfrm.as_mut().unwrap().rot = 45.0;
        edit_text(&mut doc);
        assert_fresh(&doc);
        assert!(!repair(&mut doc), "computed receipt is already consistent");
    }

    #[test]
    fn eligibility_changes_refit_computed_results_but_keep_imported_results() {
        for (ancestor, flip) in [(false, false), (true, false), (false, true), (true, true)] {
            let transform = |doc: &mut SlideFile| {
                let shapes = &mut Arc::make_mut(&mut doc.slides[0]).shapes;
                let target = kition_slide_model::find_shape_mut(shapes, ShapeId(if ancestor { 100 } else { 101 })).unwrap();
                let x = target.xfrm.as_mut().unwrap();
                if flip {
                    x.flip_h = true;
                } else {
                    x.rot = 45.0;
                }
            };
            let mut imported = document();
            let before = imported.clone();
            transform(&mut imported);
            invalidate(&mut imported, &before);
            assert_eq!(
                find_shape(&imported.slides[0].shapes, ShapeId(101)).unwrap().text,
                find_shape(&before.slides[0].shapes, ShapeId(101)).unwrap().text
            );
            assert!(!repair(&mut imported));

            let mut computed = document();
            edit_text(&mut computed);
            let before = computed.clone();
            transform(&mut computed);
            let mut remotely_changed = computed.clone();
            invalidate(&mut computed, &before);
            assert_fresh(&computed);
            let old_input = &find_shape(&before.slides[0].shapes, ShapeId(101)).unwrap().text.as_ref().unwrap().body.autofit_input;
            let new_input = &find_shape(&computed.slides[0].shapes, ShapeId(101)).unwrap().text.as_ref().unwrap().body.autofit_input;
            assert_ne!(old_input, new_input);
            assert!(repair(&mut remotely_changed));
            assert_fresh(&remotely_changed);
            assert!(!repair(&mut remotely_changed), "remote repair converges after one pass");
        }
    }
}
