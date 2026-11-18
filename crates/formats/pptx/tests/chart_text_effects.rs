//! Authored chart-label effects survive regeneration and effect-only edits.

use std::sync::Arc;

use kition_slide_color::{ColorTransform, Rgba};
use kition_slide_model::chart::{Chart, ChartText, ChartType, DataLabels};
use kition_slide_model::style::{ColorRef, Glow, Shadow};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

fn chart(p: &SlideFile) -> &Chart {
    let ShapeKind::Chart(c) = &p.slides[0].shapes[0].kind else { panic!("chart missing") };
    c
}

fn shadow() -> Shadow {
    let mut color = ColorRef::rgb(Rgba::BLACK);
    color.mods.push(ColorTransform::Alpha(40_000));
    Shadow { color, blur: 4.0, dist: 3.0, dir: 45.0, inner: false, sx: 1.0, sy: 1.0, kx: 0.0, ky: 0.0, align: "tl".into(), rotate_with_shape: false }
}

#[test]
fn regenerated_labels_keep_shadow_glow_and_explicit_empty_lists() {
    let glow = Glow { color: ColorRef::rgb(Rgba::BLACK), radius: 2.0 };
    let text = ChartText { shadow: Some(shadow()), glow: Some(glow.clone()), effects_explicit: true, ..Default::default() };
    let empty = ChartText { effects_explicit: true, ..Default::default() };
    let glow_only = ChartText { glow: Some(glow), effects_explicit: true, ..Default::default() };
    let mut c = Chart::sample(ChartType::Column);
    c.series[0].fmt.labels = Some(DataLabels {
        value: true,
        text: text.clone(),
        points: vec![
            (0, DataLabels { value: true, text: empty.clone(), ..Default::default() }),
            (1, DataLabels { value: true, text: glow_only.clone(), ..Default::default() }),
        ],
        ..Default::default()
    });
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(c)), ..Default::default() }];
    let mut p = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { unreachable!() };
    assert!(c.raw.is_some());
    c.series[0].values[0] = Some(19.0);
    c.raw = None;
    let restored = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    let labels = chart(&restored).series[0].fmt.labels.as_ref().unwrap();
    assert_eq!(labels.text, text);
    assert_eq!(labels.point(0).unwrap().text, empty);
    assert_eq!(labels.point(1).unwrap().text, glow_only);
    assert_eq!(chart(&restored).series[0].values[0], Some(19.0));

    // Editing only an effect must also invalidate the kept chart comparison.
    let mut edited = restored;
    let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut edited.slides[0]).shapes[0].kind else { unreachable!() };
    assert!(c.raw.is_some());
    c.series[0].fmt.labels.as_mut().unwrap().text.shadow.as_mut().unwrap().dist = 7.0;
    let edited = kition_slide_pptx::import(&kition_slide_pptx::export(&edited).unwrap()).unwrap();
    assert_eq!(chart(&edited).series[0].fmt.labels.as_ref().unwrap().text.shadow.as_ref().unwrap().dist, 7.0);
    assert_eq!(chart(&edited).series[0].fmt.labels.as_ref().unwrap().point(0).unwrap().text, empty);
}
