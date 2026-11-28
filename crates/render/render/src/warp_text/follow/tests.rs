use super::*;
use kition_slide_model::resolve::Ctx;
use kition_slide_model::text::{Bullet, TextBody};
use kition_slide_model::{Shape, SlideFile, Xfrm};
use kition_slide_text::{NoFields, Opts};
use kurbo::Point;

fn fixture(text: &str, rect: Rect) -> TextLayout {
    let p = SlideFile::default();
    let mut body = TextBody::from_text(text);
    body.body.wrap = Some(false);
    body.body.anchor = Some(Anchor::Middle);
    for para in &mut body.paragraphs {
        para.props.align = Some(Align::Center);
        para.props.bullet = Some(Bullet::None);
        for run in &mut para.runs {
            run.props.font = Some("Ubuntu".into());
            run.props.size = Some(20.0);
        }
    }
    let shape =
        Shape { xfrm: Some(Xfrm::new(0.0, 0.0, rect.width(), rect.height())), text: Some(body.clone()), text_box: true, ..Default::default() };
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    kition_slide_text::layout(&ctx, &shape, &body, &Opts { rect, fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false })
}

fn cluster_edges(line: &LineInfo, head: usize) -> Option<(f64, f64)> {
    cluster_positions(line)?.get(&head).copied()
}

fn cluster_at(path: &FollowPath, line: &LineInfo, head: usize) -> Option<Affine> {
    cluster_transform(path, line, cluster_edges(line, head)?)
}

fn line() -> LineInfo {
    LineInfo {
        para: 0,
        start: 0,
        end: 4,
        baseline: 31.0,
        extent: (20.0, 60.0),
        edges: vec![(20.0, 27.0), (27.0, 34.0), (34.0, 45.0), (45.0, 60.0)],
        cluster_heads: vec![0, 0, 0, 3],
        alignment: Some(Align::Center),
        ..Default::default()
    }
}

#[test]
fn ligature_uses_all_character_caret_edges_and_repeated_glyphs_share_transform() {
    let line = line();
    assert_eq!(cluster_edges(&line, 0), Some((20.0, 45.0)));
    assert_eq!(cluster_edges(&line, 3), Some((45.0, 60.0)));
    assert!(cluster_edges(&line, 1).is_none());
    let p = FollowPath::from_preset("textArchUp", &[], 100.0, 100.0).unwrap();
    let m = cluster_at(&p, &line, 0).unwrap();
    let origin = Point::new(32.5, 31.0);
    // The first 25-point cluster center is 7.5 points before the centered line's midpoint.
    let t = std::f64::consts::PI * 1.5 - 7.5 / 50.0;
    let expected = Point::new(50.0 + 50.0 * t.cos(), 50.0 + 50.0 * t.sin());
    assert!((m * origin - expected).hypot() < 1e-6);
    // A base and its combining mark retain their offset magnitude under one rigid transform.
    let mark = origin + kurbo::Vec2::new(3.0, -9.0);
    assert!(((m * mark - m * origin).hypot() - (mark - origin).hypot()).abs() < 1e-9);
    assert!((m.determinant() - 1.0).abs() < 1e-9);
}

#[test]
fn rtl_visual_edges_and_alignment_are_not_replaced_by_logical_character_order() {
    let mut l = line();
    l.rtl = true;
    l.edges = vec![(53.0, 60.0), (46.0, 53.0), (35.0, 46.0), (20.0, 35.0)];
    assert_eq!(cluster_edges(&l, 0), Some((35.0, 60.0)));
    let p = FollowPath::from_preset("textArchDown", &[], 100.0, 100.0).unwrap();
    let a = cluster_at(&p, &l, 0).unwrap() * Point::new(47.5, 31.0);
    let b = cluster_at(&p, &l, 3).unwrap() * Point::new(27.5, 31.0);
    assert!(a.x > b.x);
    l.alignment = Some(Align::Right);
    let right = cluster_at(&p, &l, 0).unwrap() * Point::new(47.5, 31.0);
    l.alignment = Some(Align::Left);
    let left = cluster_at(&p, &l, 0).unwrap() * Point::new(47.5, 31.0);
    assert!(right.x > left.x);
}

#[test]
fn a_centered_glyph_keeps_font_ink_height_in_a_shallow_box() {
    let rect = Rect::new(10.0, 20.0, 210.0, 50.0);
    let l = fixture("H", rect);
    let flat: Vec<_> = l.runs.iter().map(super::super::run_outlines).collect();
    let before = bounds(&flat).unwrap();
    for preset in ["textArchUp", "textArchDown"] {
        let prepared = prepare(&l, rect, preset, &[], Anchor::Middle).unwrap();
        let after = bounds(&prepared).unwrap();
        assert!((after.height() - before.height()).abs() < 1e-5);
        let guide_y = if preset == "textArchUp" { rect.y0 } else { rect.y1 };
        assert!((after.center().y - guide_y).abs() < 1e-5);
        assert!(after.height() > rect.height() * 0.225 + 1.0, "no radial-band compression");
    }
}

#[test]
fn top_middle_and_bottom_anchors_follow_the_guide_alike() {
    // A single line follows the path whatever its vertical anchor;
    // justified and distributed anchors stay on the existing renderer.
    let rect = Rect::new(5.0, 7.0, 205.0, 77.0);
    let l = fixture("WIDE", rect);
    for preset in ["textArchUp", "textArchDown"] {
        let middle = prepare(&l, rect, preset, &[], Anchor::Middle).unwrap();
        for anchor in [Anchor::Top, Anchor::Bottom] {
            assert_eq!(prepare(&l, rect, preset, &[], anchor).unwrap(), middle, "{preset} {anchor:?}");
        }
        for anchor in [Anchor::Justified, Anchor::Distributed] {
            assert!(prepare(&l, rect, preset, &[], anchor).is_none());
        }
    }
}

#[test]
fn guide_center_is_independent_of_flat_baseline_and_glyph_descender() {
    // A single centered glyph has a horizontal tangent at either default arc midpoint.
    // Its vertical ink center therefore lies on y=0 or y=h, independently of font metrics.
    let rect = Rect::new(13.0, 29.0, 213.0, 69.0);
    for text in ["H", "g", "j"] {
        let mut l = fixture(text, rect);
        for preset in ["textArchUp", "textArchDown"] {
            let expected_y = if preset == "textArchUp" { rect.y0 } else { rect.y1 };
            let initial = bounds(&prepare(&l, rect, preset, &[], Anchor::Middle).unwrap()).unwrap();
            assert!((initial.center().y - expected_y).abs() < 1e-5);
            // Flat layout's block translation must not displace the authored arc guide.
            l.lines[0].baseline += 37.0;
            for run in &mut l.runs {
                for (_, _, y) in &mut run.glyphs {
                    *y += 37.0;
                }
            }
            let moved = bounds(&prepare(&l, rect, preset, &[], Anchor::Middle).unwrap()).unwrap();
            assert!((moved.center() - initial.center()).hypot() < 1e-5);
            assert!((moved.height() - initial.height()).abs() < 1e-5);
        }
    }
}

#[test]
fn unsupported_metadata_and_multiline_remain_on_existing_path() {
    let rect = Rect::new(0.0, 0.0, 200.0, 70.0);
    let mut l = fixture("Hi", rect);
    assert!(prepare(&l, rect, "textInflate", &[], Anchor::Middle).is_none());
    l.lines[0].cluster_heads.clear();
    assert!(prepare(&l, rect, "textArchUp", &[], Anchor::Middle).is_none());
    let mut missing_alignment = fixture("Hi", rect);
    missing_alignment.lines[0].alignment = None;
    assert!(prepare(&missing_alignment, rect, "textArchUp", &[], Anchor::Middle).is_none());
    let l = fixture("One\nTwo", rect);
    assert!(prepare(&l, rect, "textArchDown", &[], Anchor::Middle).is_none());
}

#[test]
fn actual_grouped_renderer_matches_equivalent_displayed_box_with_reflection_and_rotation() {
    let mut body = TextBody::from_text("Group");
    body.body.wrap = Some(false);
    body.body.anchor = Some(Anchor::Middle);
    body.body.warp = Some("textArchUp".into());
    body.paragraphs[0].props.align = Some(Align::Center);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].runs[0].props.font = Some("Ubuntu".into());
    body.paragraphs[0].runs[0].props.size = Some(20.0);
    let leaf = Shape {
        id: kition_slide_model::ShapeId(81),
        xfrm: Some(Xfrm::new(0.0, 0.0, 200.0, 200.0)),
        text: Some(body),
        text_box: true,
        ..Default::default()
    };
    let mut displayed = Xfrm::new(150.0, 150.0, 400.0, 100.0);
    displayed.rot = 31.0;
    displayed.flip_h = true;
    let direct = Shape { xfrm: Some(displayed), ..leaf.clone() };
    let group = Shape {
        id: kition_slide_model::ShapeId(80),
        xfrm: Some(displayed),
        kind: kition_slide_model::ShapeKind::Group { children: vec![leaf], child: Xfrm::new(0.0, 0.0, 200.0, 200.0) },
        ..Default::default()
    };
    let render = |shape| {
        let mut p = SlideFile::default();
        std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![shape];
        crate::render_slide(&p, 0, &crate::RenderOpts::default())
    };
    let a = render(group);
    let b = render(direct);
    assert_eq!(a.pixels, b.pixels);
    assert!(a.pixels.as_chunks::<4>().0.iter().any(|p| p[0] < 128 && p[1] < 128 && p[2] < 128));
}

#[test]
fn layout_retains_actual_ligature_heads_and_combining_cluster_membership() {
    let rect = Rect::new(0.0, 0.0, 200.0, 70.0);
    let l = fixture("ffi", rect);
    let line = &l.lines[0];
    assert_eq!(line.cluster_heads, vec![0, 0, 0]);
    assert_eq!(cluster_edges(line, 0), Some(line.extent));
    for run in &l.runs {
        assert!(run.cells.iter().all(|(head, _)| *head == 0));
    }
    let accent = fixture("a\u{0301}", rect);
    assert_eq!(accent.lines[0].cluster_heads, vec![0, 0]);
    assert!(prepare(&accent, rect, "textArchDown", &[], Anchor::Middle).is_some());
}

#[test]
fn spaces_keep_shaped_advance_and_empty_ink_needs_no_placeholder() {
    let rect = Rect::new(0.0, 0.0, 200.0, 70.0);
    let l = fixture("A A", rect);
    let line = &l.lines[0];
    let a = cluster_edges(line, 0).unwrap();
    let b = cluster_edges(line, 2).unwrap();
    let space = cluster_edges(line, 1).unwrap();
    assert!(space.1 > space.0);
    assert!((b.0 - a.1 - (space.1 - space.0)).abs() < 1e-8);
    assert!(prepare(&l, rect, "textArchUp", &[], Anchor::Middle).is_some());
    let empty = fixture("   ", rect);
    let prepared = prepare(&empty, rect, "textArchUp", &[], Anchor::Middle).unwrap();
    assert!(prepared.iter().flatten().all(|p| p.is_empty()));
}

#[test]
fn distributed_single_character_keeps_the_layout_center_for_both_directions() {
    let path = FollowPath::from_preset("textArchUp", &[], 100.0, 100.0).unwrap();
    for rtl in [false, true] {
        let l = LineInfo {
            start: 0,
            end: 1,
            baseline: 31.0,
            extent: (20.0, 45.0),
            edges: vec![(20.0, 45.0)],
            cluster_heads: vec![0],
            alignment: Some(Align::Distributed),
            rtl,
            ..Default::default()
        };
        let center = cluster_at(&path, &l, 0).unwrap() * Point::new(32.5, 31.0);
        assert!((center - Point::new(50.0, 0.0)).hypot() < 1e-6);
    }
}

#[test]
fn synthetic_italic_with_stroke_retains_ordinary_painting() {
    let rect = Rect::new(0.0, 0.0, 200.0, 70.0);
    let mut l = fixture("Hi", rect);
    assert!(!l.runs.is_empty());
    for run in &mut l.runs {
        run.fake_italic = true;
        run.fake_bold = false;
        run.outline = None;
    }
    for preset in ["textArchUp", "textArchDown"] {
        assert!(prepare(&l, rect, preset, &[], Anchor::Middle).is_some(), "unstroked italic remains supported");
        for (bold, outline) in [(true, false), (false, true), (true, true)] {
            let mut stroked = l.clone();
            for run in &mut stroked.runs {
                run.fake_bold = bold;
                run.outline = outline.then_some((kition_slide_color::Rgba::BLACK, 2.0));
            }
            assert!(prepare(&stroked, rect, preset, &[], Anchor::Middle).is_none(), "{preset}, bold={bold}, outline={outline}");
            for run in &mut stroked.runs {
                run.fake_italic = false;
            }
            assert!(prepare(&stroked, rect, preset, &[], Anchor::Middle).is_some(), "upright stroke remains supported");
        }
    }
}
