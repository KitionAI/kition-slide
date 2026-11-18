use super::*;
use kition_slide_geom::Affine;
use kition_slide_model::Fill;
use kition_slide_model::defaults;
use kition_slide_model::text::{Paragraph, Run};
use kition_slide_model::{LayoutType, SlideFile, Xfrm};

fn setup(text: &str, w: f64, h: f64) -> (SlideFile, Shape) {
    let mut p = SlideFile::default();
    let lay = defaults::layout_of_kind(&p, LayoutType::TitleAndContent).unwrap();
    let mut s = defaults::new_slide(&mut p, lay);
    let mut body = s.shapes.remove(1);
    body.xfrm = Some(Xfrm::new(0.0, 0.0, w, h));
    body.text = Some(TextBody::from_text(text));
    s.shapes = vec![body.clone()];
    p.slides = vec![std::sync::Arc::new(s)];
    (p, body)
}

fn lay(p: &SlideFile, sh: &Shape, w: f64, h: f64) -> TextLayout {
    let ctx = Ctx::for_slide(p, &p.slides[0]).unwrap();
    layout(&ctx, sh, sh.text.as_ref().unwrap(), &Opts { rect: Rect::new(0.0, 0.0, w, h), fields: &NoFields, prompt_color: None, no_shrink: false })
}

#[test]
fn wraps_long_text_into_lines() {
    let text = "The quick brown fox jumps over the lazy dog and keeps on running far away";
    let (p, sh) = setup(text, 200.0, 400.0);
    let l = lay(&p, &sh, 200.0, 400.0);
    assert!(l.lines.len() >= 3, "{} lines", l.lines.len());
    for li in &l.lines {
        assert!(li.caret_x.last().unwrap() <= &(200.0 + 0.5), "line too wide: {:?}", li.caret_x.last());
        assert_eq!(li.caret_x.len(), li.end - li.start + 1);
    }
    // Lines cover the text without gaps (spaces at wraps are skipped).
    assert_eq!(l.lines[0].start, 0);
    assert!(!l.runs.is_empty());
}

#[test]
fn bullets_are_drawn_for_body_text() {
    let (p, sh) = setup("One\nTwo", 600.0, 300.0);
    let l = lay(&p, &sh, 600.0, 300.0);
    // Two text runs + two bullet runs.
    assert!(l.runs.len() >= 4, "{:?}", l.runs);
    assert_eq!(l.para_lines.len(), 2);
}

#[test]
fn shrink_on_overflow() {
    let text = (0..30).map(|i| format!("Line number {i}")).collect::<Vec<_>>().join("\n");
    let (p, sh) = setup(&text, 600.0, 200.0);
    let l = lay(&p, &sh, 600.0, 200.0);
    assert!(l.font_scale < 1.0, "scale {}", l.font_scale);
    assert!(!l.overflow || l.font_scale <= 0.11);
}

#[test]
fn caret_and_hit_roundtrip() {
    let (p, sh) = setup("Hello world", 600.0, 100.0);
    let l = lay(&p, &sh, 600.0, 100.0);
    for ch in 0..=11 {
        let pos = Pos { para: 0, ch };
        let (x, top, bot) = l.caret(pos).unwrap();
        let back = l.hit(Point::new(x + 0.1, (top + bot) / 2.0));
        assert_eq!(back, pos, "at {ch}");
    }
    let r = l.selection_rects(Pos { para: 0, ch: 0 }, Pos { para: 0, ch: 5 });
    assert_eq!(r.len(), 1);
}

#[test]
fn alignment_center_and_right() {
    let (p, mut sh) = setup("Hi", 400.0, 100.0);
    sh.text.as_mut().unwrap().paragraphs[0].props.align = Some(Align::Center);
    sh.text.as_mut().unwrap().paragraphs[0].props.bullet = Some(Bullet::None);
    let c = lay(&p, &sh, 400.0, 100.0);
    sh.text.as_mut().unwrap().paragraphs[0].props.align = Some(Align::Right);
    let r = lay(&p, &sh, 400.0, 100.0);
    let cx = c.lines[0].caret_x[0];
    let rx = r.lines[0].caret_x[0];
    assert!(rx > cx && cx > 100.0, "center {cx} right {rx}");
}

#[test]
fn empty_and_hostile_bodies_never_panic() {
    let (p, mut sh) = setup("", 10.0, 10.0);
    let _ = lay(&p, &sh, 10.0, 10.0);
    let _ = lay(&p, &sh, 0.0, 0.0);
    let t = sh.text.as_mut().unwrap();
    t.paragraphs = vec![Paragraph {
        runs: vec![
            Run::new("\t\u{b}\u{b}🙂 ﷽ 日本語"),
            Run { text: String::new(), props: Default::default(), kind: kition_slide_model::text::RunKind::Break },
        ],
        ..Default::default()
    }];
    t.paragraphs[0].props.margin_left = Some(-50.0);
    t.paragraphs[0].props.indent = Some(f64::NAN);
    t.paragraphs[0].props.line_spacing = Some(Spacing::Pct(-3.0));
    t.body.columns = Some(1000);
    let l = lay(&p, &sh, 50.0, 50.0);
    let _ = l.hit(Point::new(-100.0, 1e9));
    let _ = l.caret(Pos { para: 99, ch: 99 });
    let _ = l.selection_rects(Pos { para: 5, ch: 0 }, Pos { para: 0, ch: 3 });
}

#[test]
fn autonum_formats() {
    assert_eq!(autonum("arabicPeriod", 3), "3.");
    assert_eq!(autonum("romanUcPeriod", 4), "IV.");
    assert_eq!(autonum("alphaLcParenR", 2), "b)");
    assert_eq!(autonum("arabicParenBoth", 1), "(1)");
    assert_eq!(autonum("alphaUcPeriod", 27), "AA.");
}

#[test]
fn numbered_list_counts() {
    let (p, mut sh) = setup("a\nb\nc", 400.0, 300.0);
    for para in &mut sh.text.as_mut().unwrap().paragraphs {
        para.props.bullet = Some(Bullet::AutoNum { scheme: "arabicPeriod".into(), start_at: 1 });
    }
    let l = lay(&p, &sh, 400.0, 300.0);
    assert!(l.runs.len() >= 6);
}

// ---------- Bidirectional text and Arabic typography ----------

const MIXED: &str = "سنة 2024 (Retrait) فقط";

/// A one-paragraph body with no bullet, `rtl` and `align` set.
fn bidi_setup(runs: Vec<Run>, w: f64, rtl: bool, align: Align) -> (SlideFile, Shape) {
    let (p, mut sh) = setup("x", w, 300.0);
    let t = sh.text.as_mut().unwrap();
    t.paragraphs = vec![Paragraph { runs, ..Default::default() }];
    let pp = &mut t.paragraphs[0].props;
    pp.rtl = Some(rtl);
    pp.align = Some(align);
    pp.bullet = Some(Bullet::None);
    pp.margin_left = Some(0.0);
    pp.indent = Some(0.0);
    (p, sh)
}

/// Is any face able to draw Arabic here (installed or bundled)? Shaping assertions that need
/// real Arabic glyphs are skipped without one.
fn arabic_face() -> Option<Arc<FontFace>> {
    let db = FontDb::global();
    let f = db.cascade(&['\u{0633}'], &[db.face("Inter", "Regular")], "Regular");
    f.covers('\u{0633}').then_some(f)
}

#[test]
fn rtl_mixed_string_is_reordered_per_line() {
    let (p, sh) = bidi_setup(vec![Run::new(MIXED)], 2000.0, true, Align::Right);
    let l = lay(&p, &sh, 2000.0, 300.0);
    assert_eq!(l.lines.len(), 1);
    let line = &l.lines[0];
    assert!(line.rtl);
    let n = MIXED.chars().count();
    assert_eq!(line.edges.len(), n);
    assert_eq!(line.caret_x.len(), n + 1);
    let mid = |k: usize| (line.edges[k].0 + line.edges[k].1) / 2.0;
    // Visual order right to left: سنة | 2024 | (Retrait) | فقط.
    assert!(mid(0) > mid(4), "first Arabic word right of the number");
    assert!(mid(4) > mid(10), "number right of the Latin word");
    assert!(mid(10) > mid(19), "Latin right of the last Arabic word");
    // Inside the Arabic words, logical order runs right to left; digits and Latin left to right.
    assert!(mid(0) > mid(1) && mid(1) > mid(2));
    assert!(mid(4) < mid(5) && mid(5) < mid(7));
    assert!(mid(10) < mid(16));
    // The brackets sit outside "Retrait": "(" (logically first) on its right.
    assert!(mid(9) > mid(16) && mid(17) < mid(10));
    // Right-aligned at the start edge S = W - rIns; the downstream caret at 0 is that edge.
    let right_edge = l.inner.x1;
    assert!((line.extent.1 - right_edge).abs() < 0.5, "{} vs {right_edge}", line.extent.1);
    assert!((line.caret_x[0] - line.extent.1).abs() < 0.01);
    // Levels after L1: Arabic odd, digits and Latin even.
    assert_eq!(line.levels[0], 1);
    assert_eq!(line.levels[4], 2);
    assert_eq!(line.levels[10], 2);
}

#[test]
fn rtl_paragraph_starts_at_the_right_with_the_bullet_outside() {
    let (p, mut sh) = bidi_setup(vec![Run::new("مرحبا بكم")], 400.0, true, Align::Right);
    let pp = &mut sh.text.as_mut().unwrap().paragraphs[0].props;
    pp.bullet = Some(Bullet::Char { char: "•".into() });
    pp.margin_left = Some(36.0);
    pp.indent = Some(-36.0);
    let l = lay(&p, &sh, 400.0, 300.0);
    let line = &l.lines[0];
    // Text starts marL from the right edge; the bullet hangs at S - (marL + indent) = S.
    let s_edge = l.inner.x1;
    assert!((line.extent.1 - (s_edge - 36.0)).abs() < 0.5, "text right {} expected {}", line.extent.1, s_edge - 36.0);
    let bullet = l.runs.iter().find(|r| r.chars == (0, 0)).expect("bullet run");
    let bx = bullet.glyphs[0].1;
    assert!(bx > line.extent.1 - 0.5, "bullet {bx} right of the text {}", line.extent.1);
    assert!(bx < s_edge, "bullet inside the box");
    // An LTR paragraph keeps the bullet on the left.
    sh.text.as_mut().unwrap().paragraphs[0].props.rtl = Some(false);
    sh.text.as_mut().unwrap().paragraphs[0].props.align = Some(Align::Left);
    let l = lay(&p, &sh, 400.0, 300.0);
    let bullet = l.runs.iter().find(|r| r.chars == (0, 0)).expect("bullet run");
    assert!(bullet.glyphs[0].1 < l.lines[0].extent.0);
}

#[test]
fn rtl_left_and_center_alignment() {
    let (p, sh) = bidi_setup(vec![Run::new("سلام")], 400.0, true, Align::Left);
    let l = lay(&p, &sh, 400.0, 300.0);
    assert!((l.lines[0].extent.0 - l.inner.x0).abs() < 0.5, "algn=l is flush left in RTL too");
    let (p, sh) = bidi_setup(vec![Run::new("سلام")], 400.0, true, Align::Center);
    let l = lay(&p, &sh, 400.0, 300.0);
    let c = (l.lines[0].extent.0 + l.lines[0].extent.1) / 2.0;
    assert!((c - (l.inner.x0 + l.inner.x1) / 2.0).abs() < 0.5);
}

#[test]
fn bidi_caret_stops_round_trip_through_hit_testing() {
    for rtl in [true, false] {
        let (p, sh) = bidi_setup(vec![Run::new(MIXED)], 2000.0, rtl, if rtl { Align::Right } else { Align::Left });
        let l = lay(&p, &sh, 2000.0, 300.0);
        let line = &l.lines[0];
        for (x, ch, aff) in line.visual_stops() {
            let (cx, ..) = l.caret_at(Pos { para: 0, ch }, aff).unwrap();
            assert!((cx - x).abs() < 0.01, "stop {ch} {aff:?}: caret {cx} vs stop {x}");
        }
        // Clicking just inside either side of each grapheme puts the caret on that side.
        for (a, b) in line.graphemes() {
            let Some((x0, x1)) = line.grapheme_box(a, b) else { continue };
            if x1 - x0 < 1.0 {
                continue;
            }
            let y = (line.top + line.bottom) / 2.0;
            for x in [x0 + 0.2, x1 - 0.2] {
                let (pos, aff) = l.hit_affinity(Point::new(x, y));
                let (cx, ..) = l.caret_at(pos, aff).unwrap();
                let side = if x < (x0 + x1) / 2.0 { x0 } else { x1 };
                assert!((cx - side).abs() < 0.01, "rtl={rtl} grapheme {a}: hit {x} -> {pos:?} {aff:?} caret {cx}, expected {side}");
            }
        }
    }
}

#[test]
fn visual_movement_walks_the_line_left_to_right() {
    let (p, sh) = bidi_setup(vec![Run::new(MIXED)], 2000.0, true, Align::Right);
    let l = lay(&p, &sh, 2000.0, 300.0);
    let stops = l.lines[0].visual_stops();
    let (x0, ch0, aff0) = stops[0];
    let (mut pos, mut aff) = (Pos { para: 0, ch: ch0 }, aff0);
    let mut xs = vec![x0];
    for _ in 0..200 {
        let (np, na) = l.move_caret(pos, aff, true, MoveMode::Visual);
        if (np, na) == (pos, aff) {
            break;
        }
        (pos, aff) = (np, na);
        xs.push(l.caret_at(pos, aff).unwrap().0);
    }
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "moving right always moves right: {xs:?}");
    let right = l.lines[0].extent.1;
    assert!((xs.last().unwrap() - right).abs() < 0.01, "reaches the right edge");
    // Right arrow at the right edge of an RTL paragraph's only line goes nowhere.
    assert_eq!(l.move_caret(pos, aff, true, MoveMode::Visual).0, pos);
    // Moving left from the right edge in an RTL paragraph is logically forward.
    let (p1, _) = l.move_caret(Pos { para: 0, ch: 0 }, Affinity::Downstream, false, MoveMode::Visual);
    assert_eq!(p1.ch, 1);
}

#[test]
fn logical_movement_steps_over_harakat() {
    // beh + fatha + teh: the fatha belongs to the beh's grapheme.
    let (p, sh) = bidi_setup(vec![Run::new("\u{0628}\u{064E}\u{062A}")], 400.0, true, Align::Right);
    let l = lay(&p, &sh, 400.0, 300.0);
    let (n1, _) = l.move_caret(Pos { para: 0, ch: 0 }, Affinity::Downstream, true, MoveMode::Logical);
    assert_eq!(n1.ch, 2);
    let (n2, _) = l.move_caret(n1, Affinity::Downstream, false, MoveMode::Logical);
    assert_eq!(n2.ch, 0);
    // No caret stop inside a grapheme.
    assert!(l.lines[0].visual_stops().iter().all(|s| s.1 != 1));
}

#[test]
fn selection_is_the_union_of_visual_boxes() {
    // LTR paragraph with an Arabic word: visually "ab ةنس cd".
    let (p, sh) = bidi_setup(vec![Run::new("ab سنة cd")], 2000.0, false, Align::Left);
    let l = lay(&p, &sh, 2000.0, 300.0);
    // Logical [1, 5) = "b", " ", "س", "ن": two separate boxes on screen.
    let r = l.selection_rects(Pos { para: 0, ch: 1 }, Pos { para: 0, ch: 5 });
    assert_eq!(r.len(), 2, "{r:?}");
    // The whole line is one box.
    assert_eq!(l.selection_rects(Pos { para: 0, ch: 0 }, Pos { para: 0, ch: 9 }).len(), 1);
}

#[test]
fn paint_only_style_changes_keep_arabic_joining() {
    let Some(_) = arabic_face() else {
        eprintln!("skipped: no Arabic face available");
        return;
    };
    let word = "سنة";
    let glyphs = |runs: Vec<Run>| {
        let (p, sh) = bidi_setup(runs, 400.0, true, Align::Right);
        let l = lay(&p, &sh, 400.0, 300.0);
        let mut g: Vec<(u32, f64)> = l.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| (g.0, g.1))).collect();
        g.sort_by(|a, b| a.1.total_cmp(&b.1));
        g.into_iter().map(|g| g.0).collect::<Vec<_>>()
    };
    let whole = glyphs(vec![Run::new(word)]);
    let mut red = Run::new("سن");
    red.props.fill = Some(Fill::solid(kition_slide_model::style::ColorRef::rgb(Rgba::from_hex("C00000").unwrap())));
    let split = glyphs(vec![red, Run::new("ة")]);
    assert_eq!(whole, split, "a colour change inside a word keeps the joined forms");
    // And the joined forms differ from the isolated letters.
    let isolated: Vec<u32> = word.chars().map(|c| arabic_face().unwrap().glyph_for(c)).collect();
    assert_ne!(whole.iter().rev().copied().collect::<Vec<_>>(), isolated);
}

#[test]
fn just_low_stretches_arabic_with_kashida() {
    if arabic_face().is_none() {
        eprintln!("skipped: no Arabic face available");
        return;
    }
    // Tatweel glyphs drawn, counted in each run's own face (the one the layout picked).
    let tatweels = |l: &TextLayout| {
        l.runs
            .iter()
            .map(|r| {
                let tg = r.face.glyph_for('\u{0640}');
                if tg == 0 { 0 } else { r.glyphs.iter().filter(|g| g.0 == tg).count() }
            })
            .sum::<usize>()
    };
    let text = "بسم الله الرحمن الرحيم نستعين به ونستغفره ونتوب إليه";
    let (p, sh) = bidi_setup(vec![Run::new(text)], 220.0, true, Align::JustLow);
    let l = lay(&p, &sh, 220.0, 600.0);
    assert!(l.lines.len() >= 2);
    let first = &l.lines[0];
    let w = l.inner.width();
    assert!((first.extent.1 - first.extent.0 - w).abs() < 0.5, "first line fills the box: {:?} vs {w}", first.extent);
    if l.runs.iter().all(|r| r.face.glyph_for('\u{0640}') == 0) {
        eprintln!("skipped: the Arabic face has no tatweel");
        return;
    }
    assert!(tatweels(&l) > 0, "kashida drawn");
    // The last line isn't stretched and sits at the start (right) edge.
    let last = l.lines.last().unwrap();
    assert!((last.extent.1 - l.inner.x1).abs() < 0.5);
    // Plain justify uses spaces: no tatweel.
    let (p, sh) = bidi_setup(vec![Run::new(text)], 220.0, true, Align::Justify);
    let l = lay(&p, &sh, 220.0, 600.0);
    assert_eq!(tatweels(&l), 0);
}

#[test]
fn wrapped_rtl_lines_break_in_logical_order() {
    let text = "واحد اثنان ثلاثة أربعة خمسة ستة سبعة ثمانية تسعة عشرة";
    let (p, sh) = bidi_setup(vec![Run::new(text)], 160.0, true, Align::Right);
    let l = lay(&p, &sh, 160.0, 600.0);
    assert!(l.lines.len() >= 2);
    // Lines cover the text in logical order: the first line holds the first words.
    assert_eq!(l.lines[0].start, 0);
    for w in l.lines.windows(2) {
        assert!(w[1].start >= w[0].end);
    }
    for line in &l.lines {
        assert!(line.extent.0 >= l.inner.x0 - 0.5, "line fits: {:?}", line.extent);
        assert!((line.extent.1 - l.inner.x1).abs() < 0.5, "flush right");
    }
    // Upstream affinity at a wrap keeps the caret on the earlier line.
    let end0 = l.lines[0].end;
    assert_eq!(l.line_of_affine(Pos { para: 0, ch: end0 }, Affinity::Upstream), Some(0));
}

#[test]
fn hostile_bidi_input_never_panics() {
    let nasty = format!("{}\t\u{b}\u{202E}abc\u{2069}{}(]))[ ١٢٣ \u{0640}\u{064E}\u{064E}\u{200D}", "\u{202B}".repeat(200), "\u{2067}".repeat(130));
    for rtl in [true, false] {
        for align in [Align::Left, Align::Right, Align::Center, Align::Justify, Align::JustLow, Align::Distributed] {
            let (p, sh) = bidi_setup(vec![Run::new(nasty.clone()), Run::new(MIXED)], 60.0, rtl, align);
            let l = lay(&p, &sh, 60.0, 60.0);
            for line in &l.lines {
                assert_eq!(line.caret_x.len(), line.end - line.start + 1);
                assert_eq!(line.edges.len(), line.end - line.start);
            }
            let _ = l.hit_affinity(Point::new(-1e9, 1e9));
            let _ = l.selection_rects(Pos { para: 0, ch: 0 }, Pos { para: 0, ch: 9999 });
            let mut pos = (Pos::default(), Affinity::Downstream);
            for i in 0..50 {
                pos = l.move_caret(pos.0, pos.1, i % 3 != 0, if i % 2 == 0 { MoveMode::Visual } else { MoveMode::Logical });
            }
            let _ = l.caret_at(Pos { para: 7, ch: 7 }, Affinity::Upstream);
        }
    }
}

/// Two paragraphs of `size`-point text in `family`, no bullets, in a tall box.
fn two_lines(family: &str, size: f64, text: &str) -> (SlideFile, Shape) {
    let (p, mut sh) = setup("x", 600.0, 400.0);
    let t = sh.text.as_mut().unwrap();
    t.paragraphs = (0..2)
        .map(|_| {
            let mut r = Run::new(text);
            r.props.size = Some(size);
            r.props.font = Some(family.into());
            let mut para = Paragraph { runs: vec![r], ..Default::default() };
            para.props.bullet = Some(Bullet::None);
            para.props.space_before = Some(Spacing::Pts(0.0));
            para.props.space_after = Some(Spacing::Pts(0.0));
            para.props.line_spacing = Some(Spacing::Pct(1.0));
            para
        })
        .collect();
    (p, sh)
}

fn pitch(l: &TextLayout) -> f64 {
    l.lines[1].baseline - l.lines[0].baseline
}

#[test]
fn single_spacing_is_one_point_two_font_sizes_in_any_face() {
    // The line pitch doesn't follow the face's own ascent and descent, so a substitute face
    // keeps the requested font's line positions.
    for (family, text) in [("Inter", "Line"), ("No Such Family", "Line"), ("Inter", "\u{4E2D}\u{6587}")] {
        let (p, sh) = two_lines(family, 20.0, text);
        let l = lay(&p, &sh, 600.0, 400.0);
        assert!((pitch(&l) - 24.0).abs() < 1e-6, "{family}: {}", pitch(&l));
        // The baseline splits the line in the face's ascent-to-descent proportion.
        let first = &l.lines[0];
        assert!(first.baseline > first.top + 12.0 && first.baseline < first.bottom, "{first:?}");
    }
    let (mut p, mut sh) = two_lines("Inter", 20.0, "Line");
    for para in &mut sh.text.as_mut().unwrap().paragraphs {
        para.props.line_spacing = Some(Spacing::Pct(1.5));
    }
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![sh.clone()];
    assert!((pitch(&lay(&p, &sh, 600.0, 400.0)) - 36.0).abs() < 1e-6);
}

#[test]
fn recorded_autofit_shrink_is_applied_as_recorded() {
    let (p, mut sh) = two_lines("Inter", 20.0, "Line");
    let t = sh.text.as_mut().unwrap();
    t.body.autofit = Some(AutoFit::Shrink { font_scale: 0.5, line_reduction: 0.2 });
    for para in &mut t.paragraphs {
        para.props.line_spacing = Some(Spacing::Pct(1.2));
    }
    // The text would fit unshrunk; the recorded shrink applies anyway, like the file shows it.
    let l = lay(&p, &sh, 600.0, 400.0);
    assert_eq!(l.font_scale, 0.5);
    assert!(l.runs.iter().all(|r| (r.size - 10.0).abs() < 1e-9), "{:?}", l.runs);
    // The reduction takes percentage points off: 120% - 20% = 100% of 1.2 x 10 pt.
    assert!((pitch(&l) - 12.0).abs() < 1e-6, "{}", pitch(&l));
    // Text far past the box at the recorded scale (edited since) shrinks further.
    let (p, mut sh) = setup(&(0..40).map(|i| format!("Line {i}")).collect::<Vec<_>>().join("\n"), 600.0, 200.0);
    sh.text.as_mut().unwrap().body.autofit = Some(AutoFit::Shrink { font_scale: 0.9, line_reduction: 0.0 });
    let l = lay(&p, &sh, 600.0, 200.0);
    assert!(l.font_scale < 0.9, "{}", l.font_scale);
}

#[test]
fn bold_east_asian_text_in_a_fallback_face_is_emboldened() {
    let db = FontDb::global();
    if !db.has_family(kition_slide_fonts::CJK_FALLBACK_FAMILY) {
        eprintln!("skipped: built without the bundled CJK face (set KITION_FONTS_DIR)");
        return;
    }
    let (p, mut sh) = two_lines("Microsoft YaHei", 20.0, "Ab\u{4E2D}\u{6587}");
    for para in &mut sh.text.as_mut().unwrap().paragraphs {
        para.runs[0].props.bold = Some(true);
    }
    let l = lay(&p, &sh, 600.0, 400.0);
    let cjk: Vec<&GlyphRun> = l.runs.iter().filter(|r| r.cells.iter().any(|(_, c)| *c == '\u{4E2D}')).collect();
    assert!(!cjk.is_empty());
    for r in cjk {
        // The missing family's East Asian text takes the bundled CJK face, which has no bold.
        assert_eq!(r.face.family, kition_slide_fonts::CJK_FALLBACK_FAMILY);
        assert!(r.fake_bold, "{r:?}");
    }
    // Latin text keeps a face with Latin letters.
    let latin = l.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == 'A')).unwrap();
    assert!(latin.face.covers('A'));
}

#[test]
fn east_asian_commas_hang_at_the_line_end() {
    if !FontDb::global().has_family(kition_slide_fonts::CJK_FALLBACK_FAMILY) {
        eprintln!("skipped: built without the bundled CJK face (set KITION_FONTS_DIR)");
        return;
    }
    // Five ideographs fill the line exactly; the comma after them hangs instead of taking the
    // fifth ideograph to the next line with it (it may not start a line).
    let ideographs: Vec<char> = (0..7).map(|i| char::from_u32(0x4E00 + i).unwrap()).collect();
    let mut text: String = ideographs[..5].iter().collect();
    text.push('\u{FF0C}');
    text.extend(&ideographs[5..]);
    let (mut p, mut sh) = two_lines("Microsoft YaHei", 20.0, &text);
    let t = sh.text.as_mut().unwrap();
    t.paragraphs.truncate(1);
    t.body.inset_l = Some(0.0);
    t.body.inset_r = Some(0.0);
    t.paragraphs[0].props.margin_left = Some(0.0);
    t.paragraphs[0].props.indent = Some(0.0);
    std::sync::Arc::make_mut(&mut p.slides[0]).shapes = vec![sh.clone()];
    let l = lay(&p, &sh, 100.5, 400.0);
    assert_eq!(l.lines.len(), 2, "{:?}", l.lines);
    assert_eq!((l.lines[0].start, l.lines[0].end), (0, 6));
    // Without the comma, the sixth ideograph wraps as usual.
    let plain: String = ideographs.iter().collect();
    sh.text.as_mut().unwrap().paragraphs[0].runs[0].text = plain;
    let l = lay(&p, &sh, 100.5, 400.0);
    assert_eq!((l.lines[0].start, l.lines[0].end), (0, 5));
}

#[test]
fn horizontal_anchor_centres_the_block_once_after_paragraph_alignment() {
    for align in [Align::Left, Align::Center, Align::Right] {
        let (p, mut sh) = bidi_setup(vec![Run::new("Anchor")], 400.0, false, align);
        sh.text.as_mut().unwrap().body.anchor_ctr = Some(true);
        let l = lay(&p, &sh, 400.0, 300.0);
        let (left, right) = l.lines[0].extent;
        assert!(((left + right) / 2.0 - l.inner.center().x).abs() < 1e-6, "{align:?}: {left}..{right}");
    }
}

#[test]
fn horizontal_anchor_preserves_relative_paragraph_alignment() {
    let (p, mut sh) = bidi_setup(vec![Run::new("The longer line")], 400.0, false, Align::Right);
    let t = sh.text.as_mut().unwrap();
    let mut second = t.paragraphs[0].clone();
    second.runs = vec![Run::new("Short")];
    t.paragraphs.push(second);
    t.body.anchor_ctr = Some(true);
    let l = lay(&p, &sh, 400.0, 300.0);
    assert_eq!(l.lines.len(), 2);
    let (left, right) = l.lines[0].extent;
    assert!(((left + right) / 2.0 - l.inner.center().x).abs() < 1e-6);
    assert!((l.lines[1].extent.1 - right).abs() < 1e-6);
    assert!(l.lines[1].extent.0 > left);
}

#[test]
fn horizontal_anchor_keeps_rtl_indents_on_the_start_edge() {
    let (p, mut sh) = bidi_setup(vec![Run::new("Anchor")], 400.0, true, Align::Right);
    let t = sh.text.as_mut().unwrap();
    t.body.anchor_ctr = Some(true);
    t.paragraphs[0].props.margin_left = Some(30.0);
    t.paragraphs[0].props.margin_right = Some(10.0);
    let l = lay(&p, &sh, 400.0, 300.0);
    let (left, right) = l.lines[0].extent;
    assert!(((left - 10.0 + right + 30.0) / 2.0 - l.inner.center().x).abs() < 1e-6);
}

#[test]
fn imported_full_size_autofit_preserves_saved_overflow() {
    let (p, mut sh) = two_lines("Inter", 20.0, "Line");
    sh.text.as_mut().unwrap().body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    sh.text.as_mut().unwrap().body.autofit_saved = Some(true);
    let l = lay(&p, &sh, 600.0, 40.0);
    assert!(l.overflow);
    assert_eq!(l.font_scale, 1.0);
    assert!(l.runs.iter().all(|r| (r.size - 20.0).abs() < 1e-9));
}

#[test]
fn invalidated_autofit_recalculates_from_full_size() {
    let (p, mut sh) = two_lines("Inter", 20.0, "Line");
    let b = &mut sh.text.as_mut().unwrap().body;
    b.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    b.autofit_saved = Some(false);
    let l = lay(&p, &sh, 600.0, 40.0);
    assert!(!l.overflow);
    assert!(l.font_scale < 1.0);
}

#[test]
fn autofit_receipt_is_stable_across_run_splits_ids_and_paint() {
    let (p, mut shape) = setup("Alpha Beta", 240.0, 40.0);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let rect = Rect::new(0.0, 0.0, 240.0, 40.0);
    let receipt = autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect);
    assert_eq!(receipt.len(), 67);
    assert_eq!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), Rect::new(-0.0, 1e-10, 240.0 + 1e-10, 40.0)), receipt);
    shape.id = kition_slide_model::ShapeId(999);
    let body = shape.text.as_mut().unwrap();
    body.paragraphs[0].runs = vec![Run::new("Alpha "), Run::new("Beta")];
    body.paragraphs[0].runs[1].props.fill = Some(Fill::solid(kition_slide_model::style::ColorRef::rgb(Rgba::BLACK)));
    body.paragraphs[0].runs[1].props.effects_explicit = true;
    body.paragraphs[0].runs[1].props.link = Some(kition_slide_model::text::Hyperlink {
        action: kition_slide_model::text::Action::Slide { slide: kition_slide_model::SlideId(999) },
        tooltip: String::new(),
        highlight_click: false,
    });
    body.body.autofit = Some(AutoFit::Shrink { font_scale: 0.5, line_reduction: 0.2 });
    body.body.autofit_saved = Some(true);
    body.body.autofit_input = Some(receipt.clone());
    assert_eq!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect), receipt);
    shape.text.as_mut().unwrap().paragraphs[0].runs[1].props.bold = Some(true);
    assert_ne!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect), receipt);
    assert_ne!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), Rect::new(0.0, 0.0, 200.0, 40.0)), receipt);
}

#[test]
fn stale_autofit_receipt_reflows_but_imported_result_is_authoritative() {
    let (p, mut shape) = setup("Base", 150.0, 40.0);
    shape.text.as_mut().unwrap().body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    shape.text.as_mut().unwrap().body.autofit_saved = Some(true);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let rect = Rect::new(0.0, 0.0, 150.0, 40.0);
    let receipt = autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect);
    shape.text.as_mut().unwrap().paragraphs = TextBody::from_text("Before\nBase\nAfter").paragraphs;
    // An imported saved result without a receipt is preserved, even when fonts overflow it.
    let imported = lay(&p, &shape, 150.0, 40.0);
    assert_eq!(imported.font_scale, 1.0);
    assert!(imported.overflow);
    shape.text.as_mut().unwrap().body.autofit_input = Some(receipt);
    let stale = lay(&p, &shape, 150.0, 40.0);
    assert!(stale.font_scale < 1.0);
    assert!(!stale.overflow);
}

#[test]
fn trailing_spaces_preserve_visible_alignment_and_editable_carets() {
    let hebrew: String = [0x05D0, 0x05D1, 0x05D2].into_iter().filter_map(char::from_u32).collect();
    for (text, rtl) in [("Tango", false), (hebrew.as_str(), true)] {
        for align in [Align::Left, Align::Center, Align::Right] {
            let layout_text = |suffix: &str| {
                let (p, mut shape) = setup(&format!("{text}{suffix}"), 300.0, 100.0);
                let body = shape.text.as_mut().unwrap();
                body.body.autofit = Some(AutoFit::None);
                let para = &mut body.paragraphs[0];
                para.props.align = Some(align);
                para.props.rtl = Some(rtl);
                para.props.bullet = Some(Bullet::None);
                lay(&p, &shape, 300.0, 100.0)
            };
            let plain = layout_text("");
            let spaced = layout_text("   ");
            let n = text.chars().count();
            assert_eq!(spaced.lines[0].end, n + 3, "trailing spaces remain editable");
            for (a, b) in plain.lines[0].edges.iter().zip(&spaced.lines[0].edges) {
                assert!((a.0 - b.0).abs() < 1e-8 && (a.1 - b.1).abs() < 1e-8, "rtl={rtl}, align={align:?}: {a:?} != {b:?}");
            }
            assert!((plain.content_width - spaced.content_width).abs() < 1e-8);
            let positions: Vec<_> = (n..=n + 3).map(|ch| spaced.caret(Pos { para: 0, ch }).unwrap()).collect();
            for (index, &(x, top, bottom)) in positions.iter().enumerate() {
                let point = Point::new(x + if rtl { -0.01 } else { 0.01 }, (top + bottom) / 2.0);
                assert_eq!(spaced.hit(point), Pos { para: 0, ch: n + index });
            }
            assert!(positions.windows(2).all(|p| if rtl { p[1].0 < p[0].0 } else { p[1].0 > p[0].0 }));
        }
    }
}

#[test]
fn trailing_spaces_before_soft_break_and_across_runs_do_not_shift_text() {
    let (p, mut shape) = setup("", 300.0, 150.0);
    let body = shape.text.as_mut().unwrap();
    body.body.autofit = Some(AutoFit::None);
    body.paragraphs[0].props.align = Some(Align::Center);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].runs = vec![Run::new("Tango"), Run { kind: RunKind::Break, ..Run::new("") }, Run::new("Tango")];
    let plain = lay(&p, &shape, 300.0, 150.0);
    let runs = &mut shape.text.as_mut().unwrap().paragraphs[0].runs;
    runs.insert(1, Run::new(" "));
    runs.insert(2, Run::with(" ", RunProps { fill: Some(Fill::solid(kition_slide_model::style::ColorRef::rgb(Rgba::BLACK))), ..Default::default() }));
    runs.push(Run::new("  "));
    let spaced = lay(&p, &shape, 300.0, 150.0);
    assert_eq!(spaced.lines.len(), 2);
    assert_eq!(spaced.lines[0].end, 7);
    assert_eq!(spaced.lines[1].end, 15);
    for (a, b) in plain.lines.iter().zip(&spaced.lines) {
        assert!((a.extent.0 - b.extent.0).abs() < 1e-8);
        assert!((a.extent.1 - b.extent.1).abs() < 1e-8);
        assert_eq!(a.baseline, b.baseline);
    }
}

#[test]
fn trailing_nonbreaking_spaces_and_tabs_keep_their_alignment_width() {
    for suffix in ["\u{a0}", "\t"] {
        let (p, mut shape) = setup("Tango", 300.0, 100.0);
        let body = shape.text.as_mut().unwrap();
        body.body.autofit = Some(AutoFit::None);
        body.paragraphs[0].props.align = Some(Align::Right);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        let plain = lay(&p, &shape, 300.0, 100.0);
        shape.text.as_mut().unwrap().paragraphs[0].runs[0].text.push_str(suffix);
        let spaced = lay(&p, &shape, 300.0, 100.0);
        assert!(spaced.lines[0].edges[0].0 < plain.lines[0].edges[0].0);
        assert!(spaced.content_width > plain.content_width);
    }
}

#[test]
fn vertical_writing_keeps_trailing_spaces_in_inline_alignment() {
    for direction in [TextDir::EaVertical, TextDir::Vertical, TextDir::Vertical270, TextDir::Stacked] {
        let (p, mut shape) = setup("Tango", 100.0, 300.0);
        let body = shape.text.as_mut().unwrap();
        body.body.autofit = Some(AutoFit::None);
        body.body.vert = Some(direction);
        body.paragraphs[0].props.align = Some(Align::Center);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        let plain = lay(&p, &shape, 100.0, 300.0);
        shape.text.as_mut().unwrap().paragraphs[0].runs[0].text.push(' ');
        let spaced = lay(&p, &shape, 100.0, 300.0);
        assert!(spaced.lines[0].edges[0].0 < plain.lines[0].edges[0].0);
        assert!(spaced.content_width > plain.content_width);
        assert!((plain.lines[0].extent.0 + plain.lines[0].extent.1 - spaced.lines[0].extent.0 - spaced.lines[0].extent.1).abs() < 1e-8);
    }
}

#[test]
fn nonwrapping_text_keeps_trailing_spaces_in_alignment_and_carets() {
    for align in [Align::Center, Align::Right] {
        let (p, mut shape) = setup("Tango", 300.0, 100.0);
        let body = shape.text.as_mut().unwrap();
        body.body.autofit = Some(AutoFit::None);
        body.body.wrap = Some(false);
        body.paragraphs[0].props.align = Some(align);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        let plain = lay(&p, &shape, 300.0, 100.0);
        shape.text.as_mut().unwrap().paragraphs[0].runs.push(Run::new("   "));
        let spaced = lay(&p, &shape, 300.0, 100.0);
        let line = &spaced.lines[0];
        let trailing: f64 = line.edges[5..].iter().map(|(left, right)| right - left).sum();
        let shift = if align == Align::Center { trailing / 2.0 } else { trailing };
        assert!(trailing > 0.0);
        assert!((plain.lines[0].edges[0].0 - line.edges[0].0 - shift).abs() < 1e-8);
        assert!((spaced.content_width - plain.content_width - trailing).abs() < 1e-8);
        assert_eq!(line.end, 8);
        assert_eq!(line.baseline, plain.lines[0].baseline);
        for ch in 5..=8 {
            let (x, top, bottom) = spaced.caret(Pos { para: 0, ch }).unwrap();
            assert_eq!(spaced.hit(Point::new(x + 0.01, (top + bottom) / 2.0)), Pos { para: 0, ch });
        }
        let end = spaced.caret(Pos { para: 0, ch: 8 }).unwrap().0;
        assert!((end - line.extent.1).abs() < 1e-8);
    }
}

#[test]
fn east_asian_vertical_uses_shaped_origins_and_keeps_latin_sideways() {
    let ideograph = char::from_u32(0x4e00).unwrap();
    let comma = char::from_u32(0x3001).unwrap();
    let full_stop = char::from_u32(0x3002).unwrap();
    let text = format!("A{ideograph}B{comma}{full_stop}");
    let (p, mut shape) = setup(&text, 100.0, 300.0);
    let body = shape.text.as_mut().unwrap();
    body.body.autofit = Some(AutoFit::None);
    body.body.wrap = Some(false);
    body.body.vert = Some(TextDir::EaVertical);
    let para = &mut body.paragraphs[0];
    para.props.bullet = Some(Bullet::None);
    para.runs[0].props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
    para.runs[0].props.size = Some(24.0);
    let vertical = lay(&p, &shape, 100.0, 300.0);
    let upright = vertical.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == ideograph)).unwrap();
    assert!(upright.upright);
    assert!(vertical.runs.iter().filter(|r| r.cells.iter().any(|(_, c)| c.is_ascii_alphabetic())).all(|r| !r.upright));
    let chars = ideograph.to_string();
    let shaped = kition_slide_fonts::shape_range(&upright.face, &chars, 0..chars.len(), &ShapeParams { vertical: true, ..Default::default() }, |c| c);
    let g = shaped[0];
    let line = &vertical.lines[0];
    let k = upright.size / upright.face.upem;
    assert!((line.edges[1].1 - line.edges[1].0 + g.y_advance as f64 * k).abs() < 1e-8);
    assert!((upright.glyphs[0].1 - (line.edges[1].0 - g.y_offset as f64 * k)).abs() < 1e-8);
    assert!((upright.glyphs[0].2 - ((line.top + line.bottom) / 2.0 - g.x_offset as f64 * k)).abs() < 1e-8);
    assert!(vertical.runs.iter().filter(|r| r.cells.iter().any(|(_, c)| *c == comma || *c == full_stop)).all(|r| r.upright));
    for run in &vertical.runs {
        for (glyph, (index, ch)) in run.glyphs.iter().zip(&run.cells) {
            if *ch == comma || *ch == full_stop {
                let slide_file = char::from_u32(if *ch == comma { 0xfe11 } else { 0xfe12 }).unwrap();
                let alternate = run.face.glyph_for(slide_file);
                let expected = if alternate != 0 { alternate } else { run.face.glyph_for(*ch) };
                assert_eq!(glyph.0, expected, "Tu fallback uses a covered Unicode presentation form or the nominal glyph");
                assert_eq!(text.chars().nth(*index), Some(*ch), "vertical shaping preserves the logical cluster");
            }
        }
    }
    for ch in 0..=5 {
        let (x, top, bottom) = vertical.caret(Pos { para: 0, ch }).unwrap();
        assert_eq!(vertical.hit(Point::new(x + 0.01, (top + bottom) / 2.0)), Pos { para: 0, ch });
    }
    for direction in [TextDir::Horizontal, TextDir::Vertical, TextDir::Vertical270, TextDir::Stacked] {
        shape.text.as_mut().unwrap().body.vert = Some(direction);
        let control = lay(&p, &shape, 100.0, 300.0);
        assert!(control.runs.iter().all(|r| !r.upright), "{direction:?}");
    }
}

#[test]
fn upright_superscript_and_subscript_scale_both_origin_coordinates() {
    let text = char::from_u32(0x4e00).unwrap().to_string();
    for shift in [-0.25, 0.3] {
        let (p, mut shape) = setup(&text, 100.0, 300.0);
        let body = shape.text.as_mut().unwrap();
        body.body.autofit = Some(AutoFit::None);
        body.body.vert = Some(TextDir::EaVertical);
        body.paragraphs[0].props.bullet = Some(Bullet::None);
        let props = &mut body.paragraphs[0].runs[0].props;
        props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
        props.size = Some(40.0);
        props.baseline = Some(shift);
        let l = lay(&p, &shape, 100.0, 300.0);
        let run = &l.runs[0];
        assert!(run.upright);
        assert_eq!(run.size, 40.0 * 0.66);
        let shaped = kition_slide_fonts::shape_range(&run.face, &text, 0..text.len(), &ShapeParams { vertical: true, ..Default::default() }, |c| c);
        let k = run.size / run.face.upem;
        let line = &l.lines[0];
        let cross = (line.top + line.bottom) / 2.0 - shift * 40.0;
        assert!((run.glyphs[0].1 - (line.edges[0].0 - shaped[0].y_offset as f64 * k)).abs() < 1e-8);
        assert!((run.glyphs[0].2 - (cross - shaped[0].x_offset as f64 * k)).abs() < 1e-8);
    }
}

#[test]
fn east_asian_vertical_preserves_physical_insets_and_two_glyph_column() {
    let text: String = [0x4e00, 0x4e8c].into_iter().map(|c| char::from_u32(c).unwrap()).collect();
    let (p, mut shape) = setup(&text, 48.0, 66.0);
    let body = shape.text.as_mut().unwrap();
    body.body.autofit = Some(AutoFit::None);
    body.body.vert = Some(TextDir::EaVertical);
    body.paragraphs[0].props.bullet = Some(Bullet::None);
    body.paragraphs[0].runs[0].props.size = Some(28.0);
    body.paragraphs[0].runs[0].props.font_ea = Some(kition_slide_fonts::CJK_FALLBACK_FAMILY.into());
    let l = lay(&p, &shape, 48.0, 66.0);
    assert_eq!(l.lines.len(), 1);
    assert!((l.inner.width() - 58.8).abs() < 1e-8);
    assert!((l.inner.height() - 33.6).abs() < 1e-8);
    let bp = &mut shape.text.as_mut().unwrap().body;
    bp.inset_l = Some(2.0);
    bp.inset_t = Some(3.0);
    bp.inset_r = Some(5.0);
    bp.inset_b = Some(7.0);
    let l = lay(&p, &shape, 48.0, 66.0);
    let physical = Affine::translate((24.0, 33.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2) * Affine::translate((-24.0, -33.0));
    let inner = physical.transform_rect_bbox(l.inner);
    for (actual, expected) in [(inner.x0, 2.0), (inner.y0, 3.0), (inner.x1, 43.0), (inner.y1, 59.0)] {
        assert!((actual - expected).abs() < 1e-8, "{inner:?}");
    }
}

#[test]
fn transformed_east_asian_text_retains_the_complete_legacy_layout() {
    let text = format!("A{}{}", char::from_u32(0x4e00).unwrap(), char::from_u32(0x3002).unwrap());
    let (p, mut shape) = setup(&text, 100.0, 200.0);
    let body = shape.text.as_mut().unwrap();
    body.body.autofit = Some(AutoFit::None);
    body.body.inset_l = Some(13.0);
    body.body.inset_t = Some(2.0);
    body.body.inset_r = Some(7.0);
    body.body.inset_b = Some(5.0);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let opts = Opts { rect: Rect::new(0.0, 0.0, 100.0, 200.0), fields: &NoFields, prompt_color: None, no_shrink: false };
    for (shape_rot, flip, body_rot, parent) in [
        (270.0, true, 0.0, Affine::IDENTITY),
        (36.0, false, 0.0, Affine::IDENTITY),
        (0.0, false, -90.0, Affine::IDENTITY),
        (0.0, false, 0.0, Affine::rotate(0.3)),
        (0.0, false, 0.0, Affine::scale_non_uniform(-1.0, 1.0)),
    ] {
        let x = shape.xfrm.as_mut().unwrap();
        x.rot = shape_rot;
        x.flip_h = flip;
        let transform = parent * x.affine();
        shape.text.as_mut().unwrap().body.rot = Some(body_rot);
        shape.text.as_mut().unwrap().body.vert = Some(TextDir::EaVertical);
        let actual = layout_with_transform(&ctx, &shape, shape.text.as_ref().unwrap(), &opts, transform);
        shape.text.as_mut().unwrap().body.vert = Some(TextDir::Vertical);
        let legacy = layout_with_transform(&ctx, &shape, shape.text.as_ref().unwrap(), &opts, transform);
        assert!(actual.runs.iter().all(|r| !r.upright));
        assert_eq!(actual.inner, legacy.inner);
        assert_eq!(actual.rotation, legacy.rotation);
        assert_eq!(actual.content_height, legacy.content_height);
        assert_eq!(actual.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>(), legacy.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>());
        for ch in 0..=text.chars().count() {
            assert_eq!(actual.caret(Pos { para: 0, ch }), legacy.caret(Pos { para: 0, ch }));
        }
    }
    shape.xfrm.as_mut().unwrap().rot = 360.0;
    shape.xfrm.as_mut().unwrap().flip_h = false;
    shape.text.as_mut().unwrap().body.rot = Some(-360.0);
    shape.text.as_mut().unwrap().body.vert = Some(TextDir::EaVertical);
    let standard = layout(&ctx, &shape, shape.text.as_ref().unwrap(), &opts);
    assert!(standard.runs.iter().any(|r| r.upright), "whole turns retain upright shaping");
}

#[test]
fn autofit_receipts_include_only_effective_east_asian_orientation() {
    let (p, mut shape) = setup("Layout receipt", 100.0, 200.0);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let rect = Rect::new(0.0, 0.0, 100.0, 200.0);
    let receipt = |shape: &Shape, transform| autofit_input_with_transform(&ctx, shape, shape.text.as_ref().unwrap(), rect, transform);
    let original = autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect);
    assert_eq!(receipt(&shape, Affine::rotate(0.5)), original, "horizontal receipts stay compatible");
    shape.text.as_mut().unwrap().body.vert = Some(TextDir::EaVertical);
    let upright = receipt(&shape, Affine::IDENTITY);
    assert_ne!(receipt(&shape, Affine::rotate(0.5)), upright);
    assert_ne!(receipt(&shape, Affine::scale_non_uniform(-1.0, 1.0)), upright);
    assert_eq!(receipt(&shape, Affine::translate((3.0, 5.0)) * Affine::scale(2.0)), upright);
    assert_eq!(receipt(&shape, Affine::rotate(0.5)), receipt(&shape, Affine::rotate(1.0)), "unsupported orientations share the legacy layout");
}

#[test]
fn autofit_receipt_ignores_kept_text_3d_metadata() {
    let (p, mut shape) = setup("Preserved scene", 240.0, 40.0);
    let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let rect = Rect::new(0.0, 0.0, 240.0, 40.0);
    let receipt = autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect);
    for text in [r#"<a:sp3d extrusionH="254000"/>"#, r#"<a:flatTx z="0"/>"#, "<a:sp3d/>"] {
        let body = &mut shape.text.as_mut().unwrap().body;
        body.scene3d_xml = Some(r#"<a:scene3d><a:camera prst="obliqueBottomRight"/><a:lightRig rig="flat" dir="t"/></a:scene3d>"#.into());
        body.text3d_xml = Some(text.into());
        assert_eq!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect), receipt);
    }
    shape.text.as_mut().unwrap().body.inset_l = Some(30.0);
    assert_ne!(autofit_input(&ctx, &shape, shape.text.as_ref().unwrap(), rect), receipt);
}
