//! The original pixel-by-pixel implementation is an independent equivalence oracle.
use super::*;
use kition_slide_model::Xfrm;
use std::sync::Arc;

fn scalar_counts(owner: &[u16], kinds: &[&str], diff: &[u8], threshold: u8) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for (i, d) in diff.iter().enumerate() {
        if *d > threshold {
            let k = owner.get(i).copied().unwrap_or(u16::MAX);
            let name = kinds.get(k as usize).copied().unwrap_or("backdrop");
            *out.entry(name.to_string()).or_default() += 1;
        }
    }
    out
}

#[test]
fn counters_match_original_for_thresholds_duplicate_kinds_and_mismatched_lengths() {
    let kinds = ["shape", "text", "shape", "backdrop", "connector"];
    let owner = [0, 1, 2, 3, 4, 5, u16::MAX, 0];
    let diff = [0, 8, 9, 254, 255, 0, 8, 9, 255, 1];
    for threshold in [0, 8, 9, 254, 255] {
        for olen in 0..=owner.len() {
            for dlen in 0..=diff.len() {
                assert_eq!(
                    attribute_counts(&owner[..olen], &kinds, &diff[..dlen], threshold),
                    scalar_counts(&owner[..olen], &kinds, &diff[..dlen], threshold)
                );
            }
        }
        assert_eq!(attribute_counts(&owner, &[], &diff, threshold), scalar_counts(&owner, &[], &diff, threshold));
    }
    assert!(attribute_counts(&owner, &kinds, &[8; 8], 8).is_empty());
}

#[test]
fn counters_preserve_existing_owner_saturation_and_sentinel_alias() {
    for len in [65_534, 65_535, 65_536, 65_537] {
        let mut kinds = vec!["shape"; len];
        kinds[65_533] = "text";
        if let Some(kind) = kinds.get_mut(65_535) {
            *kind = "connector";
        }
        let owner = [65_533, 65_534, 65_535];
        let diff = [255; 5];
        assert_eq!(attribute_counts(&owner, &kinds, &diff, 8), scalar_counts(&owner, &kinds, &diff, 8));
    }
}

#[test]
fn attribution_keeps_topmost_priority_hidden_shapes_and_duplicate_kind_merging() {
    let shape = |x, y, w, h, kind| Shape { xfrm: Some(Xfrm::new(x, y, w, h)), kind, ..Default::default() };
    let mut hidden = shape(0.0, 0.0, 12.0, 8.0, ShapeKind::Connector { start: None, end: None });
    hidden.hidden = true;
    let mut pres = SlideFile::default();
    Arc::make_mut(&mut pres.slides[0]).shapes = vec![
        shape(0.0, 0.0, 6.0, 6.0, ShapeKind::Shape),
        shape(2.0, 2.0, 6.0, 4.0, ShapeKind::Connector { start: None, end: None }),
        shape(4.0, 0.0, 4.0, 8.0, ShapeKind::Shape),
        hidden,
    ];
    let diff = vec![255; 12 * 8];
    let result = attribute(&pres, &pres.slides[0], 1.0, 1.0, 12, 8, &diff, 8);
    assert_eq!(map_json(&result), map_json(&scalar_attribute(&pres, &pres.slides[0], 1.0, 1.0, 12, 8, &diff, 8)));
    assert_eq!(result, BTreeMap::from([("backdrop".into(), 40), ("connector".into(), 8), ("shape".into(), 48)]));
    for angle in [0.0, 33.0, 90.0, -120.0] {
        Arc::make_mut(&mut pres.slides[0]).shapes[0].xfrm.as_mut().unwrap().rot = angle;
        for (sx, sy) in [(1.0, 1.0), (0.5, 2.0), (0.0, 1.0)] {
            assert_eq!(attribute(&pres, &pres.slides[0], sx, sy, 12, 8, &diff, 8), scalar_attribute(&pres, &pres.slides[0], sx, sy, 12, 8, &diff, 8));
        }
    }
}

fn scalar_attribute(
    pres: &SlideFile,
    slide: &Slide,
    scale_x: f64,
    scale_y: f64,
    width: u32,
    height: u32,
    max_diff: &[u8],
    threshold: u8,
) -> BTreeMap<String, u64> {
    let (w, h) = (width as usize, height as usize);
    let mut owner = vec![u16::MAX; w * h];
    let mut kinds: Vec<&'static str> = Vec::new();
    if let Some(ctx) = Ctx::for_slide(pres, slide) {
        for s in slide.shapes.iter().filter(|s| !s.hidden) {
            let x = resolve::xfrm(&ctx, s);
            let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
            let (sin, cos) = x.rot.to_radians().sin_cos();
            let hw = (x.w * cos.abs() + x.h * sin.abs()) / 2.0;
            let hh = (x.w * sin.abs() + x.h * cos.abs()) / 2.0;
            let clamp = |v: f64, m: usize| v.max(0.0).min(m as f64) as usize;
            let (x0, x1) = (clamp(((cx - hw) * scale_x).floor(), w), clamp(((cx + hw) * scale_x).ceil(), w));
            let (y0, y1) = (clamp(((cy - hh) * scale_y).floor(), h), clamp(((cy + hh) * scale_y).ceil(), h));
            let k = kinds.len().min(u16::MAX as usize - 1) as u16;
            kinds.push(kind(s));
            for row in y0..y1 {
                owner[row * w + x0..row * w + x1].fill(k);
            }
        }
    }
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    for (i, d) in max_diff.iter().enumerate() {
        if *d > threshold {
            let k = owner.get(i).copied().unwrap_or(u16::MAX);
            let name = kinds.get(k as usize).copied().unwrap_or("backdrop");
            *out.entry(name.to_string()).or_default() += 1;
        }
    }
    out
}
