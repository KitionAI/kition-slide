//! Round trips of slides built in code: model → PPTX → model.

use std::sync::Arc;

use kition_slide_color::{ColorTransform, Rgba, SchemeSlot};
use kition_slide_geom::Xfrm;
use kition_slide_model::anim::{AnimClass, AnimStart, Animation, TextBuild, Transition};
use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::style::{
    ColorRef, Dash, Effects, Fill, Glow, Gradient, GradientShape, GradientStop, Line, LineEnd, PatternFill, PictureAdjust, PictureFill, PictureMode,
    Shadow,
};
use kition_slide_model::table::Table;
use kition_slide_model::text::{
    Action, Align, Anchor, AutoFit, BodyProps, Bullet, Caps, Hyperlink, Paragraph, Run, RunKind, RunProps, Spacing, Strike, TextBody,
};
use kition_slide_model::{
    Background, Comment, CustomPath, CustomShow, Geom, InkStroke, MediaClip, Section, Shape, ShapeId, ShapeKind, Slide, SlideFile, SlideId, defaults,
};

fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).expect("png");
    out.into_inner()
}

fn shape(p: &mut SlideFile, name: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape { id: ShapeId(p.alloc_id()), name: name.into(), xfrm: Some(Xfrm::new(x, y, w, h)), ..Default::default() }
}

fn new_slide(p: &mut SlideFile, title: &str) -> Slide {
    let layout = defaults::layout_of_kind(p, kition_slide_model::LayoutType::TitleOnly).unwrap_or_default();
    let mut s = defaults::new_slide(p, layout);
    if let Some(t) = s.shapes.first_mut() {
        t.text = Some(TextBody::from_text(title));
    }
    s
}

/// A slide exercising most of what the model can hold.
pub fn rich_slide_file() -> SlideFile {
    let mut p = defaults::new_slide_file(None);
    p.props.title = "Rich slide".into();
    p.props.author = "Kition Slide tests".into();
    p.props.created = "2025-01-02T03:04:05Z".into();
    let img = p.add_media("image1.png", "image/png", png(40, 30, [200, 40, 40]));
    let img2 = p.add_media("image2.png", "image/png", png(8, 8, [20, 140, 220]));
    let video = p.add_media("clip.mp4", "video/mp4", b"\x00\x00\x00\x18ftypmp42not-a-real-video".to_vec());

    // Slide 1: title slide with notes and formatted text.
    {
        let first = p.slides[0].id;
        let s = p.slide_mut(first).expect("slide");
        s.notes = TextBody::from_text("Speaker notes line 1\nline 2");
        let t = &mut s.shapes[0];
        t.text = Some(TextBody {
            paragraphs: vec![Paragraph {
                runs: vec![
                    Run::with("Bold ", RunProps { bold: Some(true), size: Some(40.0), ..Default::default() }),
                    Run::with(
                        "red italic",
                        RunProps {
                            italic: Some(true),
                            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)))),
                            underline: Some("sng".into()),
                            strike: Some(Strike::Single),
                            caps: Some(Caps::Small),
                            spacing: Some(1.5),
                            baseline: Some(0.3),
                            font: Some("Lato".into()),
                            lang: Some("en-GB".into()),
                            ..Default::default()
                        },
                    ),
                    Run { text: String::new(), props: RunProps::default(), kind: RunKind::Break },
                    Run::with(
                        "link",
                        RunProps {
                            link: Some(Hyperlink {
                                action: Action::Url { url: "https://example.org/a?b=1&c=2".into() },
                                tooltip: "tip".into(),
                                highlight_click: false,
                            }),
                            ..Default::default()
                        },
                    ),
                ],
                ..Default::default()
            }],
            ..Default::default()
        });
        s.transition = Some(Transition {
            kind: "fade".into(),
            option: "throughBlack".into(),
            duration_ms: 700,
            advance_after_ms: Some(3000),
            ..Default::default()
        });
    }

    // Slide 2: shapes, fills, lines, effects, custom geometry, group, connector.
    let mut s = new_slide(&mut p, "Shapes");
    let mut a = shape(&mut p, "Gradient Star", 100.0, 120.0, 150.0, 150.0);
    a.geom = Geom::Preset { name: "star5".into(), adj: vec![20000.0] };
    a.fill = Some(Fill::Gradient(Gradient {
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::scheme(SchemeSlot::Accent1) },
            GradientStop { pos: 1.0, color: ColorRef::scheme(SchemeSlot::Accent2).with(ColorTransform::LumMod(75000)) },
        ],
        shape: GradientShape::Linear { angle: 45.0, scaled: false },
        tile_rect: None,
        rotate_with_shape: true,
    }));
    a.line = Some(Line { width: Some(3.0), dash: Some(Dash::Preset { name: "dash".into() }), ..Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 0)), 3.0) });
    a.effects = Some(Effects {
        outer_shadow: Some(Shadow {
            color: ColorRef::rgb(Rgba::BLACK).with(ColorTransform::Alpha(40000)),
            blur: 6.0,
            dist: 3.0,
            dir: 90.0,
            inner: false,
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            align: "b".into(),
            rotate_with_shape: false,
        }),
        glow: Some(Glow { color: ColorRef::scheme(SchemeSlot::Accent6), radius: 8.0 }),
        soft_edge: Some(2.0),
        ..Default::default()
    });
    a.xfrm = Some(Xfrm { rot: 30.0, flip_h: true, ..Xfrm::new(100.0, 120.0, 150.0, 150.0) });
    let mut b = shape(&mut p, "Pattern Box", 300.0, 120.0, 120.0, 80.0);
    b.geom = Geom::Preset { name: "roundRect".into(), adj: vec![30000.0] };
    b.fill =
        Some(Fill::Pattern(PatternFill { preset: "dkDnDiag".into(), fg: ColorRef::scheme(SchemeSlot::Accent3), bg: ColorRef::rgb(Rgba::WHITE) }));
    b.text = Some(TextBody {
        body: BodyProps { anchor: Some(Anchor::Bottom), inset_l: Some(10.0), wrap: Some(false), autofit: Some(AutoFit::Shape), ..Default::default() },
        paragraphs: vec![Paragraph {
            props: kition_slide_model::text::ParaProps {
                align: Some(Align::Right),
                line_spacing: Some(Spacing::Pct(1.5)),
                space_before: Some(Spacing::Pts(6.0)),
                bullet: Some(Bullet::AutoNum { scheme: "romanUcPeriod".into(), start_at: 3 }),
                ..Default::default()
            },
            runs: vec![Run::new("numbered")],
            level: 2,
            ..Default::default()
        }],
        ..Default::default()
    });
    b.click = Some(Hyperlink { action: Action::NextSlide, tooltip: String::new(), highlight_click: true });
    let mut c = shape(&mut p, "Freeform", 450.0, 120.0, 100.0, 100.0);
    c.geom = Geom::Custom {
        paths: vec![CustomPath {
            w: 100000.0,
            h: 100000.0,
            d: "M 0 0 L 100000 0 L 50000 100000 Z".into(),
            fill: kition_slide_geom::preset::FillMode::Norm,
            stroke: true,
        }],
        source: None,
    };
    c.fill = Some(Fill::solid(ColorRef::scheme(SchemeSlot::Accent4)));
    let child1 = Shape { geom: Geom::preset("ellipse"), ..shape(&mut p, "Child 1", 0.0, 0.0, 50.0, 50.0) };
    let child2 = Shape { geom: Geom::preset("rect"), ..shape(&mut p, "Child 2", 60.0, 0.0, 50.0, 50.0) };
    let (c1, c2) = (child1.id, child2.id);
    let mut g = shape(&mut p, "Group", 600.0, 120.0, 220.0, 100.0);
    g.kind = ShapeKind::Group { children: vec![child1, child2], child: Xfrm::new(0.0, 0.0, 110.0, 50.0) };
    let mut cx = shape(&mut p, "Connector", 600.0, 300.0, 200.0, 0.0);
    cx.kind = ShapeKind::Connector { start: Some((c1, 2)), end: Some((c2, 0)) };
    cx.geom = Geom::preset("straightConnector1");
    cx.line = Some(Line {
        tail: Some(LineEnd { kind: "triangle".into(), w: "med".into(), len: "lg".into() }),
        ..Line::solid(ColorRef::scheme(SchemeSlot::Tx1), 2.0)
    });
    let mut tb = shape(&mut p, "Text Box", 100.0, 400.0, 300.0, 40.0);
    tb.text_box = true;
    tb.text = Some(TextBody::from_text("A text box"));
    tb.descr = "alt text".into();
    let mut bgs = shape(&mut p, "Uses background", 450.0, 400.0, 60.0, 60.0);
    bgs.fill = Some(Fill::Background);
    s.shapes.extend([a, b, c, g, cx, tb, bgs]);
    s.background = Some(Background::Fill { fill: Fill::solid(ColorRef::rgb(Rgba::rgb(250, 248, 240))) });
    s.transition = Some(Transition { kind: "morph".into(), option: "words".into(), duration_ms: 2000, ..Default::default() });
    let ids: Vec<ShapeId> = s.shapes.iter().map(|x| x.id).collect();
    s.animations = vec![
        Animation { shape: ids[1], effect: "fly".into(), option: "l".into(), duration_ms: 750, ..Default::default() },
        Animation { shape: ids[2], effect: "wipe".into(), option: "t".into(), start: AnimStart::WithPrevious, delay_ms: 250, ..Default::default() },
        Animation {
            shape: ids[3],
            class: AnimClass::Emphasis,
            effect: "spin".into(),
            start: AnimStart::AfterPrevious,
            duration_ms: 2000,
            ..Default::default()
        },
        Animation { shape: ids[4], class: AnimClass::Exit, effect: "fadeOut".into(), duration_ms: 400, ..Default::default() },
        Animation {
            shape: ids[2],
            class: AnimClass::Path,
            effect: "lines".into(),
            option: "right".into(),
            path: Some("M 0 0 L 0.25 0 E".into()),
            duration_ms: 2000,
            ..Default::default()
        },
        Animation { shape: ids[6], effect: "zoom".into(), option: "in".into(), trigger: Some(ids[3]), duration_ms: 500, ..Default::default() },
    ];
    s.comments = vec![Comment {
        author: "Ada".into(),
        initials: "AL".into(),
        text: "Nice star".into(),
        date: "2025-02-03T04:05:06.000".into(),
        x: 120.0,
        y: 64.0,
        ..Default::default()
    }];
    p.slides.push(Arc::new(s));

    // Slide 3: pictures, media, ink.
    let mut s = new_slide(&mut p, "Pictures");
    let mut pic = shape(&mut p, "Picture", 100.0, 120.0, 200.0, 150.0);
    pic.kind = ShapeKind::Picture {
        fill: PictureFill {
            media: img,
            rotate_with_shape: false,
            crop: [0.1, 0.05, 0.2, 0.0],
            mode: PictureMode::Stretch { fill_rect: [0.0; 4] },
            alpha: Some(0.5),
            adjust: PictureAdjust { brightness: 0.2, contrast: -0.1, grayscale: true, ..Default::default() },
            ..Default::default()
        },
    };
    let mut tiled = shape(&mut p, "Tiled", 350.0, 120.0, 200.0, 150.0);
    tiled.fill = Some(Fill::Picture(PictureFill {
        media: img2,
        mode: PictureMode::Tile { tx: 0.0, ty: 0.0, sx: 0.5, sy: 0.5, flip: "none".into(), align: "tl".into() },
        ..Default::default()
    }));
    let mut vid = shape(&mut p, "Video", 600.0, 120.0, 240.0, 135.0);
    vid.kind = ShapeKind::Media(MediaClip {
        media: video,
        video: true,
        poster: Some(img),
        volume: 1.0,
        trim_start_ms: 500,
        fade_in_ms: 250,
        ..Default::default()
    });
    let mut ink = shape(&mut p, "Ink", 100.0, 350.0, 200.0, 100.0);
    ink.kind = ShapeKind::Ink {
        strokes: vec![InkStroke {
            points: vec![(100.0, 350.0, 0.5), (150.0, 400.0, 0.5), (300.0, 360.0, 0.5)],
            color: Rgba::rgb(0, 0, 255),
            width: 3.0,
            highlighter: false,
        }],
    };
    s.shapes.extend([pic, tiled, vid, ink]);
    s.transition = Some(Transition { kind: "vortex".into(), option: "r".into(), duration_ms: 3000, ..Default::default() });
    s.hidden = true;
    p.slides.push(Arc::new(s));

    // Slide 4: table with merges and a chart per type.
    let mut s = new_slide(&mut p, "Table");
    let mut t = Table::new(3, 3, 450.0, 30.0);
    for r in 0..3 {
        for c in 0..3 {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.text = TextBody::from_text(&format!("r{r}c{c}"));
            }
        }
    }
    t.merge(1, 0, 2, 1);
    t.style = "light2-accent3".into();
    if let Some(cell) = t.cell_mut(0, 2) {
        cell.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 255, 0))));
        cell.margins = Some([5.0, 5.0, 2.0, 2.0]);
        cell.anchor = Some(Anchor::Middle);
        cell.borders[3] = Some(Line::solid(ColorRef::rgb(Rgba::BLACK), 2.0));
    }
    let mut ts = shape(&mut p, "Table 1", 100.0, 120.0, 450.0, 90.0);
    ts.kind = ShapeKind::Table(t);
    s.shapes.push(ts);
    s.transition = Some(Transition {
        kind: "split".into(),
        option: "vertIn".into(),
        duration_ms: 1500,
        advance_on_click: false,
        advance_after_ms: Some(2000),
        ..Default::default()
    });
    p.slides.push(Arc::new(s));

    let kinds = [
        ChartType::Column,
        ChartType::StackedBar,
        ChartType::PercentColumn,
        ChartType::Line,
        ChartType::LineMarkers,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Scatter,
        ChartType::Radar,
        ChartType::Bubble,
    ];
    let mut s = new_slide(&mut p, "Charts");
    for (i, k) in kinds.iter().enumerate() {
        let mut ch = shape(&mut p, &format!("Chart {i}"), 20.0 + (i % 4) as f64 * 230.0, 100.0 + (i / 4) as f64 * 140.0, 220.0, 130.0);
        let mut chart = Chart::sample(*k);
        chart.title = Some(k.label().to_string());
        ch.kind = ShapeKind::Chart(Box::new(chart));
        s.shapes.push(ch);
    }
    let mut combo = Chart::sample(ChartType::Column);
    combo.kind = ChartType::Combo;
    combo.series[0].kind = Some(ChartType::Column);
    combo.series[1].kind = Some(ChartType::Line);
    combo.series[2].kind = Some(ChartType::Line);
    let mut ch = shape(&mut p, "Combo", 710.0, 380.0, 220.0, 130.0);
    ch.kind = ShapeKind::Chart(Box::new(combo));
    s.shapes.push(ch);
    p.slides.push(Arc::new(s));

    // Slide 6: bulleted content with a by-paragraph build, a slide link and an equation.
    let layout = defaults::layout_of_kind(&p, kition_slide_model::LayoutType::TitleAndContent).unwrap_or_default();
    let mut s = defaults::new_slide(&mut p, layout);
    s.shapes[0].text = Some(TextBody::from_text("Build"));
    s.shapes[1].text = Some(TextBody::from_text("One\nTwo\nThree"));
    let body = s.shapes[1].id;
    s.animations = vec![Animation { shape: body, effect: "fade".into(), text_build: TextBuild::ByParagraph, duration_ms: 500, ..Default::default() }];
    let target = p.slides[1].id;
    let mut link = shape(&mut p, "Go to shapes", 600.0, 450.0, 200.0, 40.0);
    link.text = Some(TextBody::from_text("Back to shapes"));
    link.click = Some(Hyperlink { action: Action::Slide { slide: target }, tooltip: String::new(), highlight_click: false });
    let mut eq = shape(&mut p, "Equation", 100.0, 450.0, 300.0, 40.0);
    eq.text = Some(TextBody {
        paragraphs: vec![Paragraph {
            runs: vec![Run {
                text: "x=1".into(),
                props: RunProps::default(),
                kind: RunKind::Math {
                    omml: r#"<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:oMath><m:r><m:t>x=1</m:t></m:r></m:oMath></m:oMathPara>"#.into(),
                },
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    s.shapes.extend([link, eq]);
    s.transition = Some(Transition { kind: "curtains".into(), duration_ms: 2000, ..Default::default() });
    p.slides.push(Arc::new(s));

    let ids: Vec<SlideId> = p.slides.iter().map(|s| s.id).collect();
    p.sections = vec![
        Section { identity: kition_slide_model::identity::fresh(), name: "Start".into(), slides: ids[..2].to_vec() },
        Section { identity: kition_slide_model::identity::fresh(), name: "Rest".into(), slides: ids[2..].to_vec() },
    ];
    p.custom_shows = vec![CustomShow { name: "Short".into(), slides: vec![ids[0], ids[3]] }];
    p.first_slide_number = 0;
    p
}

fn round(p: &SlideFile) -> SlideFile {
    let bytes = kition_slide_pptx::export(p).expect("export");
    kition_slide_pptx::import(&bytes).expect("import")
}

fn by_name<'a>(s: &'a Slide, name: &str) -> &'a Shape {
    let mut found = None;
    kition_slide_model::walk(&s.shapes, &mut |x, _| {
        if found.is_none() && x.name == name {
            found = Some(x);
        }
    });
    found.unwrap_or_else(|| panic!("no shape {name}"))
}

#[test]
fn rich_slide_file_writes_to_dir() {
    let p = rich_slide_file();
    let bytes = kition_slide_pptx::export(&p).expect("export");
    if let Ok(dir) = std::env::var("KITION_SLIDE_PPTX_OUT") {
        std::fs::write(format!("{dir}/rich.pptx"), &bytes).expect("write");
    }
    assert!(bytes.len() > 1000);
}

#[test]
fn slide_file_level_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    assert_eq!(q.slides.len(), p.slides.len());
    assert!(q.validate().is_empty(), "{:?}", q.validate());
    assert_eq!(q.slide_size, p.slide_size);
    assert_eq!(q.first_slide_number, 0);
    assert_eq!(q.props.title, "Rich slide");
    assert_eq!(q.props.author, "Kition Slide tests");
    assert_eq!(q.props.created, "2025-01-02T03:04:05Z");
    assert_eq!(
        q.sections.iter().map(|s| (s.name.clone(), s.slides.len())).collect::<Vec<_>>(),
        vec![("Start".to_string(), 2), ("Rest".to_string(), 4)]
    );
    assert_eq!(q.custom_shows.len(), 1);
    assert_eq!(q.custom_shows[0].slides, vec![q.slides[0].id, q.slides[3].id]);
    assert!(q.slides[2].hidden);
    // Themes and layouts.
    assert_eq!(q.masters.len(), 1);
    let (pm, qm) = (&p.masters[0], &q.masters[0]);
    assert_eq!(qm.theme.colors.colors, pm.theme.colors.colors);
    assert_eq!(qm.theme.fonts.major.latin, pm.theme.fonts.major.latin);
    assert_eq!(qm.layouts.len(), pm.layouts.len());
    for (a, b) in pm.layouts.iter().zip(qm.layouts.iter()) {
        assert_eq!((a.name.as_str(), a.kind, a.shapes.len()), (b.name.as_str(), b.kind, b.shapes.len()));
    }
    assert_eq!(qm.title_style, pm.title_style);
    assert_eq!(qm.body_style, pm.body_style);
    assert_eq!(q.default_text_style, p.default_text_style);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        let (la, lb) = (p.layout(a.layout).map(|x| &x.1.name), q.layout(b.layout).map(|x| &x.1.name));
        assert_eq!(la, lb);
    }
}

#[test]
fn text_and_formatting_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    assert_eq!(q.slides[0].notes_text(), "Speaker notes line 1\nline 2");
    let (a, b) = (&p.slides[0].shapes[0], &q.slides[0].shapes[0]);
    assert_eq!(a.text, b.text);
    assert!(b.xfrm.is_none(), "placeholder without xfrm inherits");
    let sa = by_name(&p.slides[1], "Pattern Box");
    let sb = by_name(&q.slides[1], "Pattern Box");
    assert_eq!(sa.text, sb.text);
    assert_eq!(sa.click, sb.click);
    assert!(by_name(&q.slides[1], "Text Box").text_box);
    assert_eq!(by_name(&q.slides[1], "Text Box").descr, "alt text");
    let la = by_name(&p.slides[5], "Go to shapes");
    let lb = by_name(&q.slides[5], "Go to shapes");
    assert_eq!(lb.click.as_ref().map(|h| h.action.clone()), Some(Action::Slide { slide: q.slides[1].id }));
    assert!(la.click.is_some());
    let eq = by_name(&q.slides[5], "Equation");
    let run = &eq.text.as_ref().expect("text").paragraphs[0].runs[0];
    assert!(matches!(&run.kind, RunKind::Math { omml } if omml.contains("oMath")), "{run:?}");
    assert_eq!(run.text, "x=1");
}

#[test]
fn shapes_fills_lines_effects_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    for name in ["Gradient Star", "Pattern Box", "Freeform", "Uses background"] {
        let (a, b) = (by_name(&p.slides[1], name), by_name(&q.slides[1], name));
        assert_eq!(a.geom, b.geom, "{name}");
        assert_eq!(a.fill, b.fill, "{name}");
        assert_eq!(a.line, b.line, "{name}");
        assert_eq!(a.effects, b.effects, "{name}");
        assert_eq!(a.xfrm, b.xfrm, "{name}");
    }
    let g = by_name(&q.slides[1], "Group");
    assert!(matches!(&g.kind, ShapeKind::Group { children, child } if children.len() == 2 && child.w == 110.0));
    let c = by_name(&q.slides[1], "Connector");
    let (c1, c2) = (by_name(&q.slides[1], "Child 1").id, by_name(&q.slides[1], "Child 2").id);
    assert!(matches!(c.kind, ShapeKind::Connector { start: Some((s, 2)), end: Some((e, 0)) } if s == c1 && e == c2), "{:?}", c.kind);
    assert_eq!(c.line, by_name(&p.slides[1], "Connector").line);
    // Ids are kept.
    assert_eq!(by_name(&q.slides[1], "Group").id, by_name(&p.slides[1], "Group").id);
}

#[test]
fn pictures_media_and_ink_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    let (a, b) = (by_name(&p.slides[2], "Picture"), by_name(&q.slides[2], "Picture"));
    let (ShapeKind::Picture { fill: fa }, ShapeKind::Picture { fill: fb }) = (&a.kind, &b.kind) else { panic!("not pictures") };
    assert!(!fa.rotate_with_shape && !fb.rotate_with_shape);
    assert_eq!(fa.crop, fb.crop);
    assert_eq!(fa.alpha, fb.alpha);
    assert_eq!(fa.adjust, fb.adjust);
    assert_eq!(p.media(fa.media).map(|m| m.data.clone()), q.media(fb.media).map(|m| m.data.clone()));
    let t = by_name(&q.slides[2], "Tiled");
    assert!(matches!(&t.fill, Some(Fill::Picture(pf)) if matches!(pf.mode, PictureMode::Tile { sx, .. } if sx == 0.5)));
    let v = by_name(&q.slides[2], "Video");
    let ShapeKind::Media(m) = &v.kind else { panic!("not media: {:?}", v.kind) };
    assert!(m.video);
    assert_eq!((m.trim_start_ms, m.fade_in_ms), (500, 250));
    assert!(q.media(m.media).is_some_and(|x| x.data.starts_with(b"\x00\x00\x00\x18ftyp")));
    assert!(m.poster.is_some());
    // Ink is written as freeform lines in a group and comes back as ink: its strokes, with their
    // points, colour and width (pressure is not kept in the file).
    let ink = by_name(&q.slides[2], "Ink");
    let (ShapeKind::Ink { strokes: was }, ShapeKind::Ink { strokes: now }) = (&by_name(&p.slides[2], "Ink").kind, &ink.kind) else {
        panic!("ink: {:?}", ink.kind)
    };
    assert_eq!(now.len(), was.len());
    for (a, b) in was.iter().zip(now) {
        assert_eq!((a.color, a.width, a.highlighter, a.points.len()), (b.color, b.width, b.highlighter, b.points.len()));
        assert!(a.points.iter().zip(&b.points).all(|(x, y)| (x.0 - y.0).abs() < 0.02 && (x.1 - y.1).abs() < 0.02), "{:?} {:?}", a.points, b.points);
    }
}

#[test]
fn tables_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    let (ShapeKind::Table(a), ShapeKind::Table(b)) = (&by_name(&p.slides[3], "Table 1").kind, &by_name(&q.slides[3], "Table 1").kind) else {
        panic!()
    };
    // A cell always has at least one paragraph in the file; compare without empty ones.
    let norm = |t: &Table| {
        let mut t = t.clone();
        for r in &mut t.rows {
            for c in &mut r.cells {
                c.text.paragraphs.retain(|p| !p.is_empty());
            }
        }
        t
    };
    assert_eq!(norm(a), norm(b));
}

#[test]
fn charts_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    for (sa, sb) in p.slides[4].shapes.iter().zip(q.slides[4].shapes.iter()) {
        let (ShapeKind::Chart(a), ShapeKind::Chart(b)) = (&sa.kind, &sb.kind) else { continue };
        assert_eq!(a.kind, b.kind, "{}", sa.name);
        assert_eq!(a.title, b.title);
        assert_eq!(a.series.len(), b.series.len());
        for (x, y) in a.series.iter().zip(b.series.iter()) {
            assert_eq!(x.name, y.name);
            assert_eq!(x.values, y.values, "{}", sa.name);
        }
        if !matches!(a.kind, ChartType::Scatter | ChartType::Bubble) {
            assert_eq!(a.categories, b.categories);
        }
        assert!(b.raw.is_some());
    }
    // Second export reuses the kept chart XML unchanged.
    let r = round(&q);
    for (sa, sb) in q.slides[4].shapes.iter().zip(r.slides[4].shapes.iter()) {
        if let (ShapeKind::Chart(a), ShapeKind::Chart(b)) = (&sa.kind, &sb.kind) {
            let (mut a, mut b) = (a.clone(), b.clone());
            a.raw = None;
            b.raw = None;
            assert_eq!(a, b);
        }
    }
}

#[test]
fn transitions_and_animations_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        match (&a.transition, &b.transition) {
            (Some(x), Some(y)) => {
                assert_eq!(
                    (&x.kind, &x.option, x.duration_ms, x.advance_on_click, x.advance_after_ms),
                    (&y.kind, &y.option, y.duration_ms, y.advance_on_click, y.advance_after_ms)
                );
            }
            (None, None) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(a.animations.len(), b.animations.len());
        for (x, y) in a.animations.iter().zip(b.animations.iter()) {
            assert_eq!(
                (&x.effect, &x.option, x.class, x.start, x.duration_ms, x.delay_ms, x.text_build),
                (&y.effect, &y.option, y.class, y.start, y.duration_ms, y.delay_ms, y.text_build)
            );
            assert_eq!(x.shape, y.shape);
            assert_eq!(x.trigger, y.trigger);
            if x.class == AnimClass::Path {
                assert_eq!(x.path, y.path);
            }
        }
    }
}

#[test]
fn comments_round_trip() {
    let p = rich_slide_file();
    let q = round(&p);
    let c = &q.slides[1].comments;
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].author.as_str(), c[0].initials.as_str(), c[0].text.as_str()), ("Ada", "AL", "Nice star"));
    assert_eq!((c[0].x, c[0].y), (120.0, 64.0));
}

#[test]
fn every_transition_kind_survives() {
    let mut p = defaults::new_slide_file(None);
    let base = p.slides[0].as_ref().clone();
    p.slides.clear();
    for (kind, ..) in kition_slide_model::anim::TRANSITIONS {
        let mut s = base.clone();
        s.id = SlideId(p.alloc_id());
        s.transition = Some(Transition { kind: kind.to_string(), duration_ms: 1200, ..Default::default() });
        p.slides.push(Arc::new(s));
    }
    let q = round(&p);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        let k = a.transition.as_ref().map(|t| t.kind.clone()).unwrap_or_default();
        if k == "none" {
            continue;
        }
        assert_eq!(b.transition.as_ref().map(|t| t.kind.clone()), Some(k.clone()));
        assert_eq!(b.transition.as_ref().map(|t| t.duration_ms), Some(1200), "{k}");
    }
}

#[test]
fn every_animation_effect_survives() {
    let mut p = defaults::new_slide_file(None);
    let mut s = p.slides[0].as_ref().clone();
    let mut sh = shape(&mut p, "Target", 100.0, 100.0, 100.0, 100.0);
    sh.text = Some(TextBody::from_text("hi"));
    let id = sh.id;
    s.shapes.push(sh);
    for (effect, _, class, _, dur, opts) in kition_slide_model::anim::ANIMATIONS {
        s.animations.push(Animation {
            shape: id,
            class: *class,
            effect: effect.to_string(),
            option: opts.first().map(|o| o.to_string()).unwrap_or_default(),
            duration_ms: (*dur).max(1),
            ..Default::default()
        });
    }
    p.slides[0] = Arc::new(s);
    let q = round(&p);
    let (a, b) = (&p.slides[0].animations, &q.slides[0].animations);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        // Effects sharing a preset number come back as the first effect with that number.
        let same_preset = kition_slide_model::anim::animation_info(&x.effect, x.class).map(|i| i.3)
            == kition_slide_model::anim::animation_info(&y.effect, y.class).map(|i| i.3);
        assert!(x.effect == y.effect || same_preset, "{} vs {}", x.effect, y.effect);
        assert_eq!(x.class, y.class);
    }
}

/// Writes bisection variants of the rich slide to `KITION_SLIDE_PPTX_BISECT` (manual checks).
#[test]
fn bisect_variants() {
    let Ok(dir) = std::env::var("KITION_SLIDE_PPTX_BISECT") else { return };
    let p = rich_slide_file();
    let n = p.slides.len();
    for i in 0..n {
        let mut q = p.clone();
        q.slides = vec![p.slides[i].clone()];
        q.sections.clear();
        q.custom_shows.clear();
        std::fs::write(format!("{dir}/bis-slide{i}.pptx"), kition_slide_pptx::export(&q).expect("export")).expect("write");
    }
    let mut q = p.clone();
    q.sections.clear();
    q.custom_shows.clear();
    q.first_slide_number = 1;
    for s in q.slides.iter_mut() {
        Arc::make_mut(s).comments.clear();
    }
    std::fs::write(format!("{dir}/bis-nodeck.pptx"), kition_slide_pptx::export(&q).expect("export")).expect("write");
}

/// Finer bisection of one slide (`KITION_SLIDE_PPTX_BISECT_SLIDE=<index>`): one variant per removed shape,
/// plus no animations / no transition.
#[test]
fn bisect_slide_variants() {
    let (Ok(dir), Ok(idx)) = (std::env::var("KITION_SLIDE_PPTX_BISECT"), std::env::var("KITION_SLIDE_PPTX_BISECT_SLIDE")) else { return };
    let p = rich_slide_file();
    let i: usize = idx.parse().expect("index");
    let base = p.slides[i].as_ref().clone();
    let one = |s: Slide| {
        let mut q = p.clone();
        q.slides = vec![Arc::new(s)];
        q.sections.clear();
        q.custom_shows.clear();
        kition_slide_pptx::export(&q).expect("export")
    };
    let mut s = base.clone();
    s.animations.clear();
    std::fs::write(format!("{dir}/v-noanim.pptx"), one(s)).expect("write");
    let mut s = base.clone();
    s.transition = None;
    std::fs::write(format!("{dir}/v-notrans.pptx"), one(s)).expect("write");
    for k in 0..base.shapes.len() {
        let mut s = base.clone();
        s.animations.clear();
        s.transition = None;
        let keep = s.shapes[k].clone();
        s.shapes.retain(|x| x.id == keep.id);
        std::fs::write(format!("{dir}/v-only{k}.pptx"), one(s)).expect("write");
    }
}

#[test]
fn rtl_paragraphs_script_fonts_and_just_low_round_trip() {
    let mut p = rich_slide_file();
    let fonts = &mut Arc::make_mut(&mut p.masters[0]).theme.fonts;
    fonts.minor.cs = "Noto Naskh Arabic".into();
    fonts.minor.scripts = vec![
        kition_slide_model::theme::ScriptFont { script: "Arab".into(), typeface: "Noto Naskh Arabic".into() },
        kition_slide_model::theme::ScriptFont { script: "Hebr".into(), typeface: "Noto Sans Hebrew".into() },
    ];
    let slide = Arc::make_mut(&mut p.slides[0]);
    let sh = slide.shapes.iter_mut().find(|s| s.text.is_some()).expect("a text shape");
    let body = sh.text.as_mut().expect("text");
    body.paragraphs = vec![Paragraph { runs: vec![Run::new("سنة 2024 (Retrait) فقط")], ..Default::default() }];
    body.paragraphs[0].props.rtl = Some(true);
    body.paragraphs[0].props.align = Some(Align::JustLow);
    let name = sh.name.clone();
    let q = round(&p);
    assert_eq!(q.masters[0].theme.fonts.minor.scripts, p.masters[0].theme.fonts.minor.scripts);
    assert_eq!(q.masters[0].theme.fonts.minor.script("arab"), Some("Noto Naskh Arabic"));
    let para = &by_name(&q.slides[0], &name).text.as_ref().expect("text").paragraphs[0];
    assert_eq!(para.props.rtl, Some(true));
    assert_eq!(para.props.align, Some(Align::JustLow));
    assert_eq!(para.runs[0].text, "سنة 2024 (Retrait) فقط");
}

#[test]
fn date_fields_keep_their_type_and_saved_text() {
    // An automatic date is a field (its saved text is what readers without a clock show); a fixed
    // date is plain text.
    let mut p = SlideFile::default();
    let mut auto = shape(&mut p, "Auto date", 10.0, 10.0, 200.0, 40.0);
    let mut body = TextBody::from_text("");
    body.paragraphs[0].runs =
        vec![Run { text: "March 3, 2025".into(), props: RunProps::default(), kind: RunKind::Field { field: "datetime4".into() } }];
    auto.text = Some(body);
    let mut fixed = shape(&mut p, "Fixed date", 10.0, 60.0, 200.0, 40.0);
    fixed.text = Some(TextBody::from_text("Launch day"));
    Arc::make_mut(&mut p.slides[0]).shapes = vec![auto, fixed];
    let q = round(&p);
    for name in ["Auto date", "Fixed date"] {
        let mut back = by_name(&q.slides[0], name).text.clone().expect("text");
        // The middle anchor these shapes show with is written out (a file without one means top).
        assert_eq!(back.body.anchor, Some(kition_slide_model::text::Anchor::Middle), "{name}");
        back.body.anchor = None;
        assert_eq!(Some(back), by_name(&p.slides[0], name).text, "{name}");
    }
}

#[test]
fn east_asian_line_breaking_options_round_trip() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Breaks", 10.0, 10.0, 300.0, 60.0);
    let mut body = TextBody::from_text("Line breaking");
    let props = &mut body.paragraphs[0].props;
    (props.east_asian_line_break, props.latin_line_break, props.hanging_punct) = (Some(false), Some(true), Some(false));
    sh.text = Some(body);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let master = Arc::make_mut(&mut p.masters[0]);
    let mut level = master.body_style.level(0).cloned().unwrap_or_default();
    (level.para.latin_line_break, level.para.hanging_punct) = (Some(false), Some(true));
    master.body_style.set(0, level);
    let q = round(&p);
    let props = &by_name(&q.slides[0], "Breaks").text.as_ref().expect("text").paragraphs[0].props;
    assert_eq!((props.east_asian_line_break, props.latin_line_break, props.hanging_punct), (Some(false), Some(true), Some(false)));
    let level = q.masters[0].body_style.level(0).expect("level 1");
    assert_eq!((level.para.latin_line_break, level.para.hanging_punct), (Some(false), Some(true)));
}

#[test]
fn a_notes_master_keeps_its_own_content_without_notes() {
    // A slide file whose slides have no notes still keeps the file's notes master: its shapes,
    // background, colour map and notes text style.
    let mut p = SlideFile::default();
    assert!(p.slides.iter().all(|s| s.notes.is_empty()));
    let mut theme = p.masters[0].theme.clone();
    theme.name = "Notes theme".into();
    let mut header = shape(&mut p, "Header Placeholder 1", 0.0, 0.0, 200.0, 40.0);
    header.text = Some(TextBody::from_text("Quarterly review"));
    let mut style = kition_slide_model::text::ListStyle::default();
    style.set(0, kition_slide_model::text::LevelStyle { run: RunProps { size: Some(14.0), ..Default::default() }, ..Default::default() });
    p.notes_master = Some(Arc::new(kition_slide_model::Master {
        name: "Notes Master".into(),
        theme,
        shapes: vec![header],
        background: Some(Background::Fill { fill: Fill::solid(ColorRef::rgb(Rgba::rgb(250, 240, 230))) }),
        body_style: style.clone(),
        color_map: vec![("bg1".into(), "dk1".into()), ("tx1".into(), "lt1".into())],
        ..Default::default()
    }));
    let q = round(&p);
    let m = q.notes_master.as_deref().expect("the notes master is written");
    assert_eq!(m.theme.name, "Notes theme");
    assert_eq!(m.shapes.len(), 1);
    assert_eq!(m.shapes[0].text.as_ref().map(TextBody::text).as_deref(), Some("Quarterly review"));
    assert_eq!(m.background, Some(Background::Fill { fill: Fill::solid(ColorRef::rgb(Rgba::rgb(250, 240, 230))) }));
    assert_eq!(m.body_style.level(0).and_then(|l| l.run.size), Some(14.0));
    assert!(m.color_map.contains(&("bg1".into(), "dk1".into())));
}

#[test]
fn an_empty_theme_latin_font_stays_empty() {
    let mut p = SlideFile::default();
    let theme = &mut Arc::make_mut(&mut p.masters[0]).theme;
    theme.fonts.major.latin.clear();
    theme.fonts.minor.ea = "SimSun".into();
    let q = round(&p);
    assert_eq!(q.masters[0].theme.fonts.major.latin, "");
    assert_eq!(q.masters[0].theme.fonts.minor.ea, "SimSun");
}

#[test]
fn empty_theme_extras_are_not_kept_as_raw_markup() {
    let q = round(&SlideFile::default());
    assert_eq!(q.masters[0].theme.raw_extra, None);
    assert_eq!(round(&q).masters[0].theme, q.masters[0].theme);
}

#[test]
fn line_pen_alignment_and_miter_limit_round_trip() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Outlined", 10.0, 10.0, 100.0, 60.0);
    sh.line = Some(Line {
        join: Some(kition_slide_model::style::LineJoin::Miter),
        miter_limit: Some(8.0),
        pen_align: Some("in".into()),
        ..Line::solid(ColorRef::scheme(SchemeSlot::Accent1), 2.0)
    });
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let q = round(&p);
    let line = by_name(&q.slides[0], "Outlined").line.clone().expect("line");
    assert_eq!((line.join, line.miter_limit, line.pen_align.as_deref()), (Some(kition_slide_model::style::LineJoin::Miter), Some(8.0), Some("in")));
}

/// `bytes` with `from` replaced by `to` in every XML part.
fn patch_package(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut zin = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut zout = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..zin.len() {
        let mut entry = zin.by_index(i).expect("entry");
        let name = entry.name().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).expect("read");
        if name.ends_with(".xml") {
            data = String::from_utf8(data).expect("utf-8").replace(from, to).into_bytes();
        }
        zout.start_file(name, zip::write::SimpleFileOptions::default()).expect("start");
        zout.write_all(&data).expect("write");
    }
    zout.finish().expect("finish").into_inner()
}

#[test]
fn a_line_feed_inside_run_text_reads_as_a_line_break() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Lines", 10.0, 10.0, 300.0, 80.0);
    sh.text = Some(TextBody::from_text("FIRSTLINE"));
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = patch_package(&kition_slide_pptx::export(&p).expect("export"), "FIRSTLINE", "one&#10;two");
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let runs = &by_name(&q.slides[0], "Lines").text.as_ref().expect("text").paragraphs[0].runs;
    let kinds: Vec<(&str, bool)> = runs.iter().map(|r| (r.text.as_str(), matches!(r.kind, RunKind::Break))).collect();
    assert_eq!(kinds, [("one", false), ("", true), ("two", false)]);
    assert_eq!(round(&q).slides[0].shapes, q.slides[0].shapes);
}

/// Part names and contents of a package.
fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).expect("entry");
            let mut d = Vec::new();
            e.read_to_end(&mut d).expect("read");
            (e.name().to_string(), d)
        })
        .collect()
}

fn part_text(bytes: &[u8], name: &str) -> String {
    parts(bytes).into_iter().find(|(n, _)| n == name).map(|(_, d)| String::from_utf8(d).expect("utf-8")).unwrap_or_default()
}

const OLE_FRAME: &str = r#"<p:graphicFrame xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:nvGraphicFramePr><p:cNvPr id="40" name="Equation"/><p:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="12700" y="12700"/><a:ext cx="1270000" cy="635000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/presentationml/2006/ole"><p:oleObj name="Equation" r:id="rId1" imgW="1270000" imgH="635000" progId="Equation.Test"><p:embed/><p:pic><p:nvPicPr><p:cNvPr id="0" name=""/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="12700" y="12700"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:oleObj></a:graphicData></a:graphic></p:graphicFrame>"#;

const RT_OLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject";
const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// A slide file whose first slide holds an OLE object carrying its embedding and preview picture.
fn ole_slide_file() -> (SlideFile, Vec<u8>, Vec<u8>) {
    let mut p = SlideFile::default();
    let bin = b"\xD0\xCF\x11\xE0 synthetic OLE storage".to_vec();
    let pic = png(4, 2, [10, 20, 30]);
    let ole = p.add_media("oleObject1.bin", "application/vnd.openxmlformats-officedocument.oleObject", bin.clone());
    let img = p.add_media("image1.png", "image/png", pic.clone());
    let mut sh = shape(&mut p, "Equation", 1.0, 1.0, 100.0, 50.0);
    sh.kind = ShapeKind::Opaque {
        xml: OLE_FRAME.into(),
        preview: Some(img),
        preview_adjust: None,
        label: "Equation.Test".into(),
        rels: vec![
            kition_slide_model::KeptRel { id: "rId1".into(), rel_type: RT_OLE.into(), media: ole, rels: vec![] },
            kition_slide_model::KeptRel { id: "rId2".into(), rel_type: RT_IMAGE.into(), media: img, rels: vec![] },
        ],
    };
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    (p, bin, pic)
}

#[test]
fn an_ole_object_keeps_its_embedding_and_preview() {
    let (p, bin, pic) = ole_slide_file();
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let files = parts(&bytes);
    let (emb, _) = files.iter().find(|(n, d)| n.starts_with("ppt/embeddings/") && *d == bin).expect("the embedding is written");
    let slide = part_text(&bytes, "ppt/slides/slide1.xml");
    assert!(slide.contains("<p:oleObj") && slide.contains("progId=\"Equation.Test\""), "{slide}");
    let rels = part_text(&bytes, "ppt/slides/_rels/slide1.xml.rels");
    let target = emb.trim_start_matches("ppt/");
    assert!(rels.contains(&format!("Type=\"{RT_OLE}\" Target=\"../{target}\"")), "{rels}");
    assert!(part_text(&bytes, "[Content_Types].xml").contains("application/vnd.openxmlformats-officedocument.oleObject"));

    let q = kition_slide_pptx::import(&bytes).expect("import");
    let ShapeKind::Opaque { xml, preview, rels, .. } = &q.slides[0].shapes[0].kind else { panic!("not kept: {:?}", q.slides[0].shapes[0].kind) };
    assert_eq!(xml.matches("r:id=\"rId1\"").count() + xml.matches("r:embed=\"rId2\"").count(), 2, "{xml}");
    let data = |id| q.media(id).map(|m| m.data.as_ref().clone()).unwrap_or_default();
    assert_eq!(
        rels.iter().map(|r| (r.id.as_str(), r.rel_type.as_str(), data(r.media))).collect::<Vec<_>>(),
        [("rId1", RT_OLE, bin.clone()), ("rId2", RT_IMAGE, pic.clone())]
    );
    assert_eq!(preview.map(data), Some(pic));
    // Stable from then on.
    assert_eq!(round(&q).slides[0].shapes[0].kind, q.slides[0].shapes[0].kind);

    // Moved and resized in the editor: saved where it now is.
    let mut m = q.clone();
    Arc::make_mut(&mut m.slides[0]).shapes[0].xfrm = Some(Xfrm::new(100.0, 50.0, 200.0, 80.0));
    let slide = part_text(&kition_slide_pptx::export(&m).expect("export"), "ppt/slides/slide1.xml");
    assert!(slide.contains(r#"<p:xfrm><a:off x="1270000" y="635000"/><a:ext cx="2540000" cy="1016000"/></p:xfrm>"#), "{slide}");
    assert_eq!(
        kition_slide_pptx::import(&kition_slide_pptx::export(&m).expect("export")).expect("import").slides[0].shapes[0].xfrm,
        m.slides[0].shapes[0].xfrm
    );
}

#[test]
fn an_ole_object_without_its_parts_is_written_as_its_preview() {
    let (mut p, ..) = ole_slide_file();
    let s = Arc::make_mut(&mut p.slides[0]);
    if let ShapeKind::Opaque { rels, .. } = &mut s.shapes[0].kind {
        rels.retain(|r| r.id != "rId1");
    }
    let bytes = kition_slide_pptx::export(&p).expect("export");
    assert!(!part_text(&bytes, "ppt/slides/slide1.xml").contains("oleObj"));
    assert!(!parts(&bytes).iter().any(|(n, _)| n.starts_with("ppt/embeddings/")));
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert!(matches!(q.slides[0].shapes[0].kind, ShapeKind::Picture { .. }), "{:?}", q.slides[0].shapes[0].kind);
}

#[test]
fn kept_xml_naming_a_part_with_relationships_is_not_carried() {
    // An object pointing at the slide layout (a part with relationships of its own) cannot carry it.
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Marker", 1.0, 1.0, 10.0, 10.0);
    sh.text = Some(TextBody::from_text("MARKER"));
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let rels = part_text(&bytes, "ppt/slides/_rels/slide1.xml.rels");
    let layout_rid = rels.split("<Relationship ").find(|r| r.contains("/slideLayout\"")).and_then(|r| r.split('"').nth(1)).expect("layout rel");
    let frame = OLE_FRAME.replace("r:id=\"rId1\"", &format!("r:id=\"{layout_rid}\"")).replace(" r:embed=\"rId2\"", "");
    let slide = part_text(&bytes, "ppt/slides/slide1.xml");
    let start = slide.find("<p:sp>").expect("shape");
    let end = slide.find("</p:sp>").expect("shape end") + "</p:sp>".len();
    let patched = patch_package(&bytes, &slide[start..end], &frame);
    let q = kition_slide_pptx::import(&patched).expect("import");
    let ShapeKind::Opaque { rels, .. } = &q.slides[0].shapes[0].kind else { panic!("not kept") };
    assert!(rels.is_empty());
    assert!(!part_text(&kition_slide_pptx::export(&q).expect("export"), "ppt/slides/slide1.xml").contains("oleObj"));
}

#[test]
fn tag_lists_round_trip_on_every_owner() {
    let mut p = SlideFile::default();
    let tags = |k: &str| vec![(format!("{k}_NAME"), format!("{k} value")), ("SECOND".to_string(), "2".to_string())];
    p.tags = vec![tags("DECK")];
    let m = Arc::make_mut(&mut p.masters[0]);
    m.tags = vec![tags("MASTER")];
    m.layouts[0].tags = vec![tags("LAYOUT")];
    if let Some(sh) = m.layouts[0].shapes.first_mut() {
        sh.tags = vec![tags("LAYOUT_SHAPE")];
    }
    let mut sh = shape(&mut p, "Tagged", 1.0, 1.0, 10.0, 10.0);
    sh.tags = vec![tags("SHAPE"), tags("SHAPE2")];
    let s = Arc::make_mut(&mut p.slides[0]);
    s.tags = vec![tags("SLIDE")];
    s.shapes.push(sh);
    if let Some(ph) = s.shapes.first_mut() {
        ph.tags = vec![tags("PLACEHOLDER")];
    }
    let bytes = kition_slide_pptx::export(&p).expect("export");
    assert!(part_text(&bytes, "ppt/slides/slide1.xml").contains("<p:custDataLst><p:tags r:id="));
    assert!(part_text(&bytes, "[Content_Types].xml").contains("presentationml.tags+xml"));
    assert!(parts(&bytes).iter().filter(|(n, _)| n.starts_with("ppt/tags/tag")).count() >= 8);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert_eq!(q.tags, p.tags);
    assert_eq!(q.masters[0].tags, p.masters[0].tags);
    assert_eq!(q.masters[0].layouts[0].tags, p.masters[0].layouts[0].tags);
    assert_eq!(q.masters[0].layouts[0].shapes.first().map(|s| &s.tags), p.masters[0].layouts[0].shapes.first().map(|s| &s.tags));
    assert_eq!(q.slides[0].tags, p.slides[0].tags);
    assert_eq!(by_name(&q.slides[0], "Tagged").tags, vec![tags("SHAPE"), tags("SHAPE2")]);
    assert_eq!(q.slides[0].shapes.first().map(|s| &s.tags), p.slides[0].shapes.first().map(|s| &s.tags));
}

#[test]
fn custom_geometry_keeps_its_guides_until_its_paths_change() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Freeform", 10.0, 10.0, 100.0, 50.0);
    sh.geom =
        Geom::Custom { paths: vec![CustomPath { w: 200.0, h: 100.0, d: "M 0 0 L 200 0 L 100 100 Z".into(), ..Default::default() }], source: None };
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let plain = kition_slide_pptx::export(&p).expect("export");
    // Give it a guide, a connection site and a text box, as a file would.
    let rich = patch_package(&plain, "<a:gdLst/>", r#"<a:gdLst><a:gd name="mid" fmla="*/ w 1 2"/></a:gdLst>"#);
    let rich = patch_package(&rich, "<a:cxnLst/>", r#"<a:cxnLst><a:cxn ang="0"><a:pos x="mid" y="h"/></a:cxn></a:cxnLst>"#);
    let rich = patch_package(&rich, r#"<a:rect l="l" t="t" r="r" b="b"/>"#, r#"<a:rect l="l" t="t" r="mid" b="b"/>"#);
    let rich = patch_package(&rich, r#"<a:pt x="100" y="100"/>"#, r#"<a:pt x="mid" y="h"/>"#);
    let q = kition_slide_pptx::import(&rich).expect("import");
    let Geom::Custom { paths, source } = &q.slides[0].shapes[0].geom else { panic!("not custom") };
    assert_eq!(paths[0].d, "M 0 0 L 200 0 L 100 100 Z");
    assert!(source.as_deref().is_some_and(|s| s.contains("fmla=\"*/ w 1 2\"")), "{source:?}");
    let out = kition_slide_pptx::export(&q).expect("export");
    let slide = part_text(&out, "ppt/slides/slide1.xml");
    assert!(slide.contains(r#"<a:pos x="mid" y="h"/>"#) && slide.contains(r#"r="mid""#), "{slide}");
    assert_eq!(kition_slide_pptx::import(&out).expect("import").slides[0].shapes[0].geom, q.slides[0].shapes[0].geom);

    // Edited paths are written as plain paths again.
    let mut e = q.clone();
    if let Geom::Custom { paths, .. } = &mut Arc::make_mut(&mut e.slides[0]).shapes[0].geom {
        paths[0].d = "M 0 0 L 200 0 L 0 100 Z".into();
    }
    let slide = part_text(&kition_slide_pptx::export(&e).expect("export"), "ppt/slides/slide1.xml");
    assert!(!slide.contains("mid") && slide.contains(r#"<a:pt x="0" y="100"/>"#), "{slide}");

    // A file's custom geometry with nothing but paths keeps no copy.
    let Geom::Custom { source, .. } = &kition_slide_pptx::import(&plain).expect("import").slides[0].shapes[0].geom else { panic!("not custom") };
    assert!(source.is_none());
}

#[test]
fn a_kept_chart_part_reads_back_with_the_same_text() {
    let bytes = kition_slide_pptx::export(&rich_slide_file()).expect("export");
    // A line end after the declaration and an external data reference, as other writers leave them.
    let bytes = patch_package(&bytes, "?>", "?>\r\n");
    let bytes = patch_package(&bytes, "<c:chart>", "<c:externalData r:id=\"rId9\"><c:autoUpdate val=\"0\"/></c:externalData><c:chart>");
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let r = round(&q);
    let raws = |p: &SlideFile| -> Vec<Option<String>> {
        p.slides[4].shapes.iter().filter_map(|s| if let ShapeKind::Chart(c) = &s.kind { Some(c.raw.clone()) } else { None }).collect()
    };
    let before = raws(&q);
    assert!(!before.is_empty());
    // The reference nothing resolves (`rId9`) is dropped with the workbook it named.
    assert!(before.iter().all(|r| r.as_deref().is_some_and(|r| !r.contains('\r') && !r.contains("externalData"))), "{before:?}");
    // Saving gives the charts a workbook of their data again; from then on the part reads back
    // with the same text.
    // (A bubble chart's data are more than the workbook's layout holds: it keeps them as written.)
    assert!(raws(&r).iter().all(|x| x.as_deref().is_some_and(|x| x.contains("externalData") || x.contains("bubbleChart"))));
    assert_eq!(raws(&round(&r)), raws(&r));
}

#[test]
fn typefaces_keep_their_face_attributes() {
    use kition_slide_model::FontFace;
    let mut p = SlideFile::default();
    let face = |slot: &str, typeface: &str, panose: Option<&str>, charset: Option<&str>, pitch: Option<&str>| FontFace {
        slot: slot.into(),
        typeface: typeface.into(),
        panose: panose.map(String::from),
        charset: charset.map(String::from),
        pitch_family: pitch.map(String::from),
    };
    p.font_faces = vec![
        face("buFont", "Test Symbols", None, Some("2"), Some("2")),
        face("ea", "Test Hei", Some("02010600030101010101"), Some("-122"), Some("2")),
        face("latin", "Test Sans", Some("020B0604020202020204"), Some("0"), Some("34")),
    ];
    let mut sh = shape(&mut p, "Faces", 10.0, 10.0, 300.0, 80.0);
    let mut body = TextBody::from_text("text");
    body.paragraphs[0].props.bullet_font = Some("Test Symbols".into());
    for r in &mut body.paragraphs[0].runs {
        r.props.font = Some("Test Sans".into());
        r.props.font_ea = Some("Test Hei".into());
        r.props.font_cs = Some("Test Sans".into());
    }
    sh.text = Some(body);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let slide = part_text(&bytes, "ppt/slides/slide1.xml");
    assert!(slide.contains(r#"<a:latin typeface="Test Sans" panose="020B0604020202020204" pitchFamily="34" charset="0"/>"#), "{slide}");
    assert!(slide.contains(r#"<a:buFont typeface="Test Symbols" pitchFamily="2" charset="2"/>"#), "{slide}");
    // A slot the file never described stays bare.
    assert!(slide.contains(r#"<a:cs typeface="Test Sans"/>"#), "{slide}");
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert_eq!(q.font_faces, p.font_faces);
    assert_eq!(round(&q).font_faces, p.font_faces);
}

/// A click "fly in" on shape `spid` built from a keyframed property animation (`p:tavLst`) with a
/// formula Kition Slide's own writer never makes, so a written file shows which tree it holds.
fn keyframed_timing(spid: u32) -> String {
    format!(
        r##"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="2" presetClass="entr" presetSubtype="4" fill="hold" grpId="0" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set><p:anim calcmode="lin" valueType="num"><p:cBhvr additive="base"><p:cTn id="7" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:strVal val="1+#ppt_h/2+0.0123"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="#ppt_y"/></p:val></p:tav></p:tavLst></p:anim></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst><p:bldLst><p:bldP spid="{spid}" grpId="0" animBg="1"/></p:bldLst></p:timing>"##
    )
}

/// A slide file whose first slide animates shape "Flyer" (id 5) with [`keyframed_timing`].
fn keyframed_slide_file() -> Vec<u8> {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Flyer", 10.0, 10.0, 100.0, 60.0);
    sh.id = ShapeId(5);
    let other = shape(&mut p, "Still", 10.0, 100.0, 100.0, 60.0);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh, other];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    patch_package(&bytes, "</p:clrMapOvr>", &format!("</p:clrMapOvr>{}", keyframed_timing(5)))
}

#[test]
fn a_keyframed_timing_tree_is_written_back_while_its_animations_are_unchanged() {
    let q = kition_slide_pptx::import(&keyframed_slide_file()).expect("import");
    let slide = &q.slides[0];
    assert_eq!(slide.animations.len(), 1, "{:?}", slide.animations);
    let kept = slide.timing.as_ref().expect("the timing tree is kept");
    assert_eq!(kept.targets.len(), 1);

    // Saved unchanged: the file's tree, keyframes and all.
    let bytes = kition_slide_pptx::export(&q).expect("export");
    let xml = part_text(&bytes, "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"<p:strVal val="1+#ppt_h/2+0.0123"/>"#) && xml.contains(r#"<p:tav tm="100000">"#), "{xml}");
    assert_eq!(xml.matches("<p:timing").count(), 1);
    let again = kition_slide_pptx::import(&bytes).expect("import again");
    assert_eq!(again.slides[0].animations, slide.animations);
    assert_eq!(again.slides[0].timing.as_ref().map(|t| &t.digest), Some(&kept.digest));

    // The shape renumbered (as a replica's reader mints ids): the tree names it by its new id.
    let mut renumbered = q.clone();
    {
        let s = Arc::make_mut(&mut renumbered.slides[0]);
        let flyer = s.shapes.iter_mut().find(|sh| sh.name == "Flyer").expect("flyer");
        flyer.id = ShapeId(77);
        s.animations[0].shape = ShapeId(77);
        if let Some(t) = s.timing.as_mut() {
            t.targets[0].shape = ShapeId(77);
        }
    }
    let xml = part_text(&kition_slide_pptx::export(&renumbered).expect("export"), "ppt/slides/slide1.xml");
    assert!(xml.contains("1+#ppt_h/2+0.0123") && xml.contains(r#"<p:spTgt spid="77"/>"#) && !xml.contains(r#"spid="5""#), "{xml}");

    // An animation edited in Kition Slide: the tree is built from the animations.
    let mut edited = q.clone();
    Arc::make_mut(&mut edited.slides[0]).animations[0].duration_ms = 1500;
    let xml = part_text(&kition_slide_pptx::export(&edited).expect("export"), "ppt/slides/slide1.xml");
    assert!(xml.contains("<p:timing") && !xml.contains("0.0123"), "{xml}");
    let back = kition_slide_pptx::import(&kition_slide_pptx::export(&edited).expect("export")).expect("import");
    assert_eq!(back.slides[0].animations[0].duration_ms, 1500);

    // The animated shape deleted: no tree names it.
    let mut deleted = q.clone();
    {
        let s = Arc::make_mut(&mut deleted.slides[0]);
        s.shapes.retain(|sh| sh.name != "Flyer");
        s.animations.clear();
    }
    let xml = part_text(&kition_slide_pptx::export(&deleted).expect("export"), "ppt/slides/slide1.xml");
    assert!(!xml.contains("<p:timing"), "{xml}");
}

#[test]
fn a_timing_tree_naming_an_unknown_shape_or_a_sound_is_not_kept() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Flyer", 10.0, 10.0, 100.0, 60.0);
    sh.id = ShapeId(5);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let dangling = patch_package(
        &bytes,
        "</p:clrMapOvr>",
        &format!("</p:clrMapOvr>{}", keyframed_timing(5).replace("<p:bldLst>", r#"<p:bldLst><p:bldP spid="999" grpId="0"/>"#)),
    );
    let q = kition_slide_pptx::import(&dangling).expect("import");
    assert!(q.slides[0].timing.is_none());
    let sound = keyframed_timing(5).replace(
        r#"<p:set>"#,
        r#"<p:audio><p:cMediaNode><p:cTn id="99"/><p:tgtEl><p:sndTgt r:embed="rId9"/></p:tgtEl></p:cMediaNode></p:audio><p:set>"#,
    );
    let q = kition_slide_pptx::import(&patch_package(&bytes, "</p:clrMapOvr>", &format!("</p:clrMapOvr>{sound}"))).expect("import");
    assert!(q.slides[0].timing.is_none());
}

#[test]
fn an_empty_colour_scheme_name_stays_empty() {
    let p = SlideFile::default();
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let theme = part_text(&bytes, "ppt/theme/theme1.xml");
    let name = theme.split(r#"<a:clrScheme name=""#).nth(1).and_then(|r| r.split('"').next()).expect("scheme name").to_string();
    let empty = patch_package(&bytes, &format!(r#"<a:clrScheme name="{name}""#), r#"<a:clrScheme name="""#);
    let q = kition_slide_pptx::import(&empty).expect("import");
    assert_eq!(q.masters[0].theme.colors.name, "");
    let theme = part_text(&kition_slide_pptx::export(&q).expect("export"), "ppt/theme/theme1.xml");
    assert!(theme.contains(r#"<a:clrScheme name="">"#), "{theme}");
}

#[test]
fn proofing_state_and_alternate_language_of_runs_round_trip() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Proofed", 10.0, 10.0, 300.0, 80.0);
    let props =
        RunProps { lang: Some("zh-CN".into()), alt_lang: Some("en-US".into()), dirty: Some(false), smt_clean: Some(false), ..Default::default() };
    let mut body = TextBody::from_text("checked");
    body.paragraphs[0].runs = vec![Run::with("checked", props.clone())];
    body.paragraphs[0].end_props = RunProps { dirty: Some(false), alt_lang: Some("en-US".into()), ..Default::default() };
    sh.text = Some(body);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let xml = part_text(&bytes, "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"altLang="en-US""#) && xml.contains(r#"dirty="0""#) && xml.contains(r#"smtClean="0""#), "{xml}");
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let text = by_name(&q.slides[0], "Proofed").text.as_ref().expect("text");
    assert_eq!(text.paragraphs[0].runs[0].props, props);
    assert_eq!(text.paragraphs[0].end_props.dirty, Some(false));
}

#[test]
fn custom_properties_theme_extensions_and_user_drawn_layouts_are_kept() {
    let p = SlideFile::default();
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let custom = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Project"><vt:lpwstr>Synthetic</vt:lpwstr></property></Properties>"#;
    let family = r#"<a:extLst><a:ext uri="{05A4C25C-085E-4340-85A3-A5531E510DB2}"><thm15:themeFamily xmlns:thm15="http://schemas.microsoft.com/office/thememl/2012/main" name="Synthetic" id="{00000000-0000-0000-0000-000000000001}" vid="{00000000-0000-0000-0000-000000000002}"/></a:ext></a:extLst></a:theme>"#;
    let mut bytes = patch_package(&bytes, "</a:theme>", family);
    bytes = patch_package(&bytes, "<p:sldLayout ", r#"<p:sldLayout userDrawn="1" "#);
    // The custom part and its package relationship.
    let mut files = parts(&bytes);
    for (name, data) in files.iter_mut() {
        if name == "_rels/.rels" {
            let text = String::from_utf8(data.clone()).expect("utf-8").replace(
                "</Relationships>",
                r#"<Relationship Id="rId99" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/></Relationships>"#,
            );
            *data = text.into_bytes();
        }
    }
    files.push(("docProps/custom.xml".into(), custom.as_bytes().to_vec()));
    let bytes = {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in &files {
            z.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).expect("start");
            z.write_all(data).expect("write");
        }
        z.finish().expect("finish").into_inner()
    };
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert_eq!(q.props.custom_xml.as_deref(), Some(custom));
    assert!(q.masters[0].layouts.iter().all(|l| l.user_drawn));
    let out = kition_slide_pptx::export(&q).expect("export");
    assert_eq!(part_text(&out, "docProps/custom.xml"), custom);
    assert!(part_text(&out, "_rels/.rels").contains("relationships/custom-properties"));
    assert!(part_text(&out, "[Content_Types].xml").contains("custom-properties+xml"));
    assert!(part_text(&out, "ppt/theme/theme1.xml").contains(r#"<thm15:themeFamily"#));
    assert!(part_text(&out, "ppt/slideLayouts/slideLayout1.xml").contains(r#"userDrawn="1""#));
    let again = kition_slide_pptx::import(&out).expect("import again");
    assert_eq!(again.props, q.props);
    assert_eq!(again.masters[0].theme, q.masters[0].theme);
}

/// `bytes` with each part `edit` changes (its name, its text) and the `added` parts (in place of
/// parts of the same name).
fn rewrite_package(bytes: &[u8], edit: &dyn Fn(&str, String) -> String, added: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts(bytes) {
        if added.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let data = match String::from_utf8(data) {
            Ok(text) => edit(&name, text).into_bytes(),
            Err(e) => e.into_bytes(),
        };
        z.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).expect("start");
        z.write_all(&data).expect("write");
    }
    for (name, data) in added {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).expect("start");
        z.write_all(data).expect("write");
    }
    z.finish().expect("finish").into_inner()
}

const RELS_HEAD: &str =
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#;

#[test]
fn theme_system_colours_stay_system_colours_until_changed() {
    let bytes = kition_slide_pptx::export(&SlideFile::default()).expect("export");
    let theme = part_text(&bytes, "ppt/theme/theme1.xml");
    let slot = |tag: &str| theme.split(&format!("<a:{tag}>")).nth(1).and_then(|r| r.split(&format!("</a:{tag}>")).next()).expect("slot").to_string();
    let (dk1, lt1) = (slot("dk1"), slot("lt1"));
    let bytes = patch_package(&bytes, &format!("<a:dk1>{dk1}</a:dk1>"), r#"<a:dk1><a:sysClr val="windowText" lastClr="101010"/></a:dk1>"#);
    let bytes = patch_package(&bytes, &format!("<a:lt1>{lt1}</a:lt1>"), r#"<a:lt1><a:sysClr val="window" lastClr="FAFAFA"/></a:lt1>"#);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let t = &q.masters[0].theme;
    assert_eq!(t.colors.get(SchemeSlot::Dk1), Rgba::rgb(0x10, 0x10, 0x10), "the colour it had resolves the slot");
    assert_eq!(t.system_colors.len(), 2);
    let out = part_text(&kition_slide_pptx::export(&q).expect("export"), "ppt/theme/theme1.xml");
    assert!(out.contains(r#"<a:dk1><a:sysClr val="windowText" lastClr="101010"/></a:dk1>"#), "{out}");
    assert!(out.contains(r#"<a:lt1><a:sysClr val="window" lastClr="FAFAFA"/></a:lt1>"#), "{out}");
    assert_eq!(round(&q).masters[0].theme, q.masters[0].theme);
    // A slot changed in Kition is written as its new colour; the other stays a system colour.
    let mut r = q.clone();
    Arc::make_mut(&mut r.masters[0]).theme.colors.set(SchemeSlot::Dk1, Rgba::rgb(0x20, 0x30, 0x40));
    let out = part_text(&kition_slide_pptx::export(&r).expect("export"), "ppt/theme/theme1.xml");
    assert!(out.contains(r#"<a:dk1><a:srgbClr val="203040"/></a:dk1>"#), "{out}");
    assert!(out.contains(r#"<a:sysClr val="window" lastClr="FAFAFA"/>"#), "{out}");
}

#[test]
fn a_handout_master_round_trips_with_its_theme_and_switches() {
    let mut p = SlideFile::default();
    let mut theme = p.masters[0].theme.clone();
    theme.name = "Handout theme".into();
    let mut header = shape(&mut p, "Header Placeholder 1", 0.0, 0.0, 200.0, 30.0);
    header.ph = Some(kition_slide_model::Placeholder { kind: kition_slide_model::PhType::Header, ..Default::default() });
    header.text = Some(TextBody::from_text("Handout header"));
    p.handout_master = Some(Arc::new(kition_slide_model::Master {
        name: "Handout Master".into(),
        theme,
        shapes: vec![header],
        color_map: vec![("bg1".into(), "lt1".into()), ("tx1".into(), "dk1".into())],
        hf: Some(r#"<p:hf xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" sldNum="0" hdr="0"/>"#.into()),
        ..Default::default()
    }));
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let pres = part_text(&bytes, "ppt/presentation.xml");
    assert!(pres.contains("<p:handoutMasterIdLst><p:handoutMasterId r:id="), "{pres}");
    assert!(part_text(&bytes, "[Content_Types].xml").contains("presentationml.handoutMaster+xml"));
    let xml = part_text(&bytes, "ppt/handoutMasters/handoutMaster1.xml");
    assert!(xml.contains(r#"sldNum="0" hdr="0"/></p:handoutMaster>"#), "{xml}");
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let m = q.handout_master.as_deref().expect("the handout master is read");
    assert_eq!(m.theme.name, "Handout theme");
    assert_eq!(m.shapes.len(), 1);
    assert_eq!(m.shapes[0].text.as_ref().map(TextBody::text).as_deref(), Some("Handout header"));
    assert!(m.hf.as_deref().is_some_and(|hf| hf.contains(r#"hdr="0""#)));
    assert_eq!(round(&q).handout_master, q.handout_master);
}

fn chart_slide_file() -> SlideFile {
    let mut p = SlideFile::default();
    let mut ch = shape(&mut p, "Chart", 40.0, 40.0, 400.0, 300.0);
    ch.kind = ShapeKind::Chart(Box::new(Chart::sample(ChartType::Column)));
    Arc::make_mut(&mut p.slides[0]).shapes = vec![ch];
    p
}

const CHART_STYLE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cs:chartStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" id="201"><cs:axisTitle><cs:lnRef idx="0"/></cs:axisTitle></cs:chartStyle>"#;
const CHART_COLORS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cs:colorStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" meth="cycle" id="10"><cs:variation/></cs:colorStyle>"#;

/// A slide whose chart names its style, colour style and embedded workbook, as Office saves it.
fn styled_chart_package() -> Vec<u8> {
    let bytes = kition_slide_pptx::export(&chart_slide_file()).expect("export");
    let rels = format!(
        r#"{RELS_HEAD}<Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2011/relationships/chartColorStyle" Target="colors1.xml"/><Relationship Id="rId2" Type="http://schemas.microsoft.com/office/2011/relationships/chartStyle" Target="style1.xml"/><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet1.xlsx"/></Relationships>"#
    );
    let edit = |name: &str, text: String| {
        match name {
        // The exported part names its own workbook; this package's is `rId1`.
        "ppt/charts/chart1.xml" => {
            let at = text.find("<c:externalData").expect("a workbook");
            let end = text[at..].find("</c:externalData>").expect("its end") + at + "</c:externalData>".len();
            format!("{}{}{}", &text[..at], r#"<c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData>"#, &text[end..])
        }
        "[Content_Types].xml" => text.replace(
            "</Types>",
            r#"<Default Extension="xlsx" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/><Override PartName="/ppt/charts/style1.xml" ContentType="application/vnd.ms-office.chartstyle+xml"/><Override PartName="/ppt/charts/colors1.xml" ContentType="application/vnd.ms-office.chartcolorstyle+xml"/></Types>"#,
        ),
        _ => text,
    }
    };
    rewrite_package(
        &bytes,
        &edit,
        &[
            ("ppt/charts/_rels/chart1.xml.rels", rels.as_bytes()),
            ("ppt/charts/style1.xml", CHART_STYLE.as_bytes()),
            ("ppt/charts/colors1.xml", CHART_COLORS.as_bytes()),
            ("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx", b"synthetic workbook"),
        ],
    )
}

fn the_chart(p: &SlideFile) -> &Chart {
    match &by_name(&p.slides[0], "Chart").kind {
        ShapeKind::Chart(c) => c,
        other => panic!("not a chart: {other:?}"),
    }
}

#[test]
fn a_chart_keeps_its_style_parts_and_its_workbook_while_unchanged() {
    let q = kition_slide_pptx::import(&styled_chart_package()).expect("import");
    assert_eq!(the_chart(&q).rels.len(), 3, "{:?}", the_chart(&q).rels);
    let out = kition_slide_pptx::export(&q).expect("export");
    assert_eq!(part_text(&out, "ppt/charts/style1.xml"), CHART_STYLE);
    assert_eq!(part_text(&out, "ppt/charts/colors1.xml"), CHART_COLORS);
    let rels = part_text(&out, "ppt/charts/_rels/chart1.xml.rels");
    let chart = part_text(&out, "ppt/charts/chart1.xml");
    let id = chart.split(r#"<c:externalData r:id=""#).nth(1).and_then(|r| r.split('"').next()).expect("the workbook stays named");
    assert!(rels.contains(&format!(r#"Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet1.xlsx""#)), "{rels}");
    assert_eq!(
        parts(&out).into_iter().find(|(n, _)| n == "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx").map(|(_, d)| d),
        Some(b"synthetic workbook".to_vec())
    );
    let ct = part_text(&out, "[Content_Types].xml");
    assert!(ct.contains("chartstyle+xml") && ct.contains("chartcolorstyle+xml"), "{ct}");
    let again = kition_slide_pptx::import(&out).expect("import again");
    assert_eq!(the_chart(&again).rels.len(), 3);

    // Edited in Kition: the old workbook no longer holds its data, so a new one of them is written
    // in its place; the style parts still describe the chart's look.
    let mut r = q.clone();
    let s = Arc::make_mut(&mut r.slides[0]);
    if let ShapeKind::Chart(c) = &mut s.shapes[0].kind {
        c.series[0].values[0] = Some(99.0);
    }
    let out = kition_slide_pptx::export(&r).expect("export");
    assert!(part_text(&out, "ppt/charts/chart1.xml").contains("externalData"));
    let books: Vec<Vec<u8>> = parts(&out).into_iter().filter(|(n, _)| n.starts_with("ppt/embeddings/")).map(|(_, d)| d).collect();
    assert_eq!(books.len(), 1);
    assert_ne!(books[0], b"synthetic workbook".to_vec(), "a workbook of the new data, not the old one");
    assert_eq!(part_text(&out, "ppt/charts/style1.xml"), CHART_STYLE);
    let rels = part_text(&out, "ppt/charts/_rels/chart1.xml.rels");
    assert_eq!(rels.matches("relationships/package\"").count(), 1, "{rels}");
}

#[test]
fn an_empty_notes_page_is_kept_while_the_notes_stay_empty() {
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).notes = TextBody::from_text("PLACEHOLDER NOTES");
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let page = part_text(&bytes, "ppt/notesSlides/notesSlide1.xml");
    // The same page with its notes emptied and a date placeholder of its own.
    let empty = page
        .split("<a:p>")
        .next()
        .map(|head| format!("{head}<a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id=\"9\" name=\"Date Placeholder 8\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"dt\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"))
        .expect("page");
    let bytes = rewrite_package(&bytes, &|name, text| if name == "ppt/notesSlides/notesSlide1.xml" { empty.clone() } else { text }, &[]);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert!(q.slides[0].notes.is_empty());
    assert!(q.slides[0].notes_page.as_deref().is_some_and(|x| x.contains("Date Placeholder 8")));
    let out = kition_slide_pptx::export(&q).expect("export");
    assert!(part_text(&out, "ppt/notesSlides/notesSlide1.xml").contains("Date Placeholder 8"));
    assert!(part_text(&out, "ppt/slides/_rels/slide1.xml.rels").contains("/notesSlide"));
    assert_eq!(round(&q).slides[0].notes_page, q.slides[0].notes_page);
    // Notes typed in Kition: the page is built for them.
    let mut r = q.clone();
    Arc::make_mut(&mut r.slides[0]).notes = TextBody::from_text("Typed later");
    let out = part_text(&kition_slide_pptx::export(&r).expect("export"), "ppt/notesSlides/notesSlide1.xml");
    assert!(out.contains("Typed later") && !out.contains("Date Placeholder 8"), "{out}");
    // A page naming another part (a picture) is not kept.
    let linked =
        empty.replace("<p:spPr/></p:sp></p:spTree>", r#"<p:spPr><a:blipFill><a:blip r:embed="rId9"/></a:blipFill></p:spPr></p:sp></p:spTree>"#);
    let bytes = rewrite_package(&bytes, &|name, text| if name == "ppt/notesSlides/notesSlide1.xml" { linked.clone() } else { text }, &[]);
    assert_eq!(kition_slide_pptx::import(&bytes).expect("import").slides[0].notes_page, None);
}

#[test]
fn presentation_and_view_properties_are_kept_around_the_show_settings() {
    let mut p = SlideFile::default();
    p.show.loop_until_esc = true;
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let pres_props = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:prnPr prnWhat="handouts6" frameSlides="1"/><p:showPr showNarration="1"><p:present/><p:sldAll/></p:showPr><p:clrMru><a:srgbClr val="FF0000"/></p:clrMru><p:extLst><p:ext uri="{E76CE94A-603C-4142-B9EB-6D1370010A27}"><p14:discardImageEditData xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" val="0"/></p:ext></p:extLst></p:presentationPr>"#;
    let view_props = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" lastView="sldSorterView"><p:normalViewPr horzBarState="maximized"><p:restoredLeft sz="15987" autoAdjust="0"/><p:restoredTop sz="94660"/></p:normalViewPr><p:slideViewPr><p:cSldViewPr snapToGrid="0"><p:cViewPr varScale="1"><p:scale><a:sx n="110" d="100"/><a:sy n="110" d="100"/></p:scale><p:origin x="0" y="0"/></p:cViewPr><p:guideLst><p:guide orient="horz" pos="2160"/><p:guide pos="3840"/></p:guideLst></p:cSldViewPr></p:slideViewPr><p:outlineViewPr><p:cViewPr><p:scale><a:sx n="33" d="100"/><a:sy n="33" d="100"/></p:scale><p:origin x="0" y="0"/></p:cViewPr><p:sldLst><p:sld r:id="rId1" collapse="1"/></p:sldLst></p:outlineViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#;
    let edit = |name: &str, text: String| match name {
        "ppt/presProps.xml" => pres_props.to_string(),
        "ppt/viewProps.xml" => view_props.to_string(),
        _ => text,
    };
    let bytes = rewrite_package(&bytes, &edit, &[]);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert!(!q.show.loop_until_esc, "the show settings are the file's");
    let mut r = q.clone();
    r.show.loop_until_esc = true;
    let out = kition_slide_pptx::export(&r).expect("export");
    let pp = part_text(&out, "ppt/presProps.xml");
    let at = |s: &str| pp.find(s).unwrap_or_else(|| panic!("{s} missing in {pp}"));
    assert!(at("<p:prnPr") < at("<p:showPr") && at("<p:showPr") < at("<p:clrMru") && at("<p:clrMru") < at("p14:discardImageEditData"), "{pp}");
    assert!(pp.contains(r#"loop="1""#), "the show settings are Kition's: {pp}");
    let vp = part_text(&out, "ppt/viewProps.xml");
    assert!(vp.contains(r#"lastView="sldSorterView""#) && vp.contains(r#"<p:guide orient="horz" pos="2160"/>"#), "{vp}");
    assert!(!vp.contains("outlineViewPr"), "the outline view names slides by relationship: {vp}");
    let again = kition_slide_pptx::import(&out).expect("import again");
    assert_eq!((&again.pres_props, &again.view_props), (&r.pres_props, &r.view_props));
}

#[test]
fn default_paragraph_styles_line_spacing_compatibility_and_header_footer_switches_round_trip() {
    let mut p = SlideFile::default();
    let lang = kition_slide_model::text::LevelStyle { run: RunProps { lang: Some("zh-CN".into()), ..Default::default() }, ..Default::default() };
    p.default_text_style.default_level = Some(Box::new(lang.clone()));
    let hf = r#"<p:hf xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" hdr="0" ftr="0" dt="0"/>"#;
    let m = Arc::make_mut(&mut p.masters[0]);
    m.body_style.default_level = Some(Box::new(lang.clone()));
    m.hf = Some(hf.into());
    m.layouts[0].hf = Some(hf.into());
    m.theme.raw_extra = Some(r#"<a:extLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:ext uri="{05A4C25C-085E-4340-85A3-A5531E510DB2}"/></a:extLst>"#.into());
    let mut sh = shape(&mut p, "Compat", 10.0, 10.0, 200.0, 50.0);
    let mut body = TextBody::from_text("legacy spacing");
    body.body.compat_line_spacing = Some(true);
    sh.text = Some(body);
    Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    assert!(part_text(&bytes, "ppt/presentation.xml").contains(r#"<a:defPPr><a:defRPr lang="zh-CN"/></a:defPPr><a:lvl1pPr"#));
    let theme = part_text(&bytes, "ppt/theme/theme1.xml");
    assert!(theme.contains("<a:objectDefaults/><a:extraClrSchemeLst/><a:extLst"), "{theme}");
    assert!(part_text(&bytes, "ppt/slideMasters/slideMaster1.xml").contains(r#"dt="0"/><p:txStyles>"#));
    let q = kition_slide_pptx::import(&bytes).expect("import");
    assert_eq!(q.default_text_style.default_level.as_deref(), Some(&lang));
    assert_eq!(q.masters[0].body_style.default_level.as_deref(), Some(&lang));
    assert_eq!(q.masters[0].hf.as_deref().map(|h| h.contains(r#"dt="0""#)), Some(true));
    assert_eq!(q.masters[0].layouts[0].hf.as_deref().map(|h| h.contains(r#"dt="0""#)), Some(true));
    assert_eq!(by_name(&q.slides[0], "Compat").text.as_ref().and_then(|t| t.body.compat_line_spacing), Some(true));
    assert_eq!(q.masters[0].theme.raw_extra, p.masters[0].theme.raw_extra);
    let r = round(&q);
    assert_eq!(r.masters, q.masters);
    assert_eq!(r.default_text_style, q.default_text_style);
}

#[test]
fn the_extended_properties_give_the_slide_file_statistics() {
    let mut p = SlideFile::default();
    let mut sh = shape(&mut p, "Words", 10.0, 10.0, 200.0, 50.0);
    let mut body = TextBody::from_text("three small words");
    body.paragraphs[0].runs[0].props.font = Some("Test Serif".into());
    sh.text = Some(body);
    Arc::make_mut(&mut p.slides[0]).shapes.push(sh);
    let app = part_text(&kition_slide_pptx::export(&p).expect("export"), "docProps/app.xml");
    assert!(app.contains("<PresentationFormat>Widescreen</PresentationFormat>"), "{app}");
    assert!(app.contains("<vt:lpstr>Fonts Used</vt:lpstr>") && app.contains("<vt:lpstr>Test Serif</vt:lpstr>"), "{app}");
    assert!(app.contains("<vt:lpstr>Theme</vt:lpstr>") && app.contains("<vt:lpstr>Slide Titles</vt:lpstr>"), "{app}");
    let words: usize = app.split("<Words>").nth(1).and_then(|r| r.split('<').next()).and_then(|n| n.parse().ok()).expect("words");
    assert!(words >= 3, "{app}");
}

/// The file's slide ids (`p:sldId/@id`) are written back; a copied slide gets a fresh one.
#[test]
fn slide_ids_from_the_file_survive() {
    let mut p = defaults::new_slide_file(None);
    p.slides.clear();
    let mut a = new_slide(&mut p, "A");
    a.file_id = Some(1000);
    let mut b = new_slide(&mut p, "B");
    b.file_id = Some(300);
    let mut copy = new_slide(&mut p, "Copy of A");
    copy.file_id = Some(1000);
    let fresh = new_slide(&mut p, "New");
    p.slides = vec![Arc::new(a), Arc::new(b), Arc::new(copy), Arc::new(fresh)];
    let back = round(&p);
    let ids: Vec<Option<u32>> = back.slides.iter().map(|s| s.file_id).collect();
    assert_eq!(&ids[..2], &[Some(1000), Some(300)]);
    let (c, n) = (ids[2].unwrap_or(0), ids[3].unwrap_or(0));
    assert!(c > 1000 && n > 1000 && c != n, "{ids:?}");
}

/// A picture's blip extensions (an SVG original, artistic-effect layers) and the parts they
/// name survive, and the embedded font list keeps its fonts.
#[test]
fn blip_extensions_and_embedded_fonts_survive() {
    let mut p = defaults::new_slide_file(None);
    p.slides.clear();
    let img = p.add_media("image1.png", "image/png", png(4, 4, [10, 20, 30]));
    let svg = p.add_media("image2.svg", "image/svg+xml", b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec());
    let font = p.add_media("font1.fntdata", "application/x-fontdata", b"not-a-real-font".to_vec());
    let mut s = new_slide(&mut p, "Pictures");
    let mut pic = shape(&mut p, "Vector", 10.0, 10.0, 100.0, 100.0);
    let ext = r#"<a:extLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" r:embed="rId1"/></a:ext></a:extLst>"#;
    pic.kind = ShapeKind::Picture {
        fill: PictureFill {
            media: img,
            ext: Some(ext.into()),
            ext_rels: vec![kition_slide_model::KeptRel {
                id: "rId1".into(),
                rel_type: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image".into(),
                media: svg,
                rels: vec![],
            }],
            ..Default::default()
        },
    };
    s.shapes.push(pic);
    p.slides.push(Arc::new(s));
    p.embedded_font_list = Some(
        r#"<p:embeddedFontLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:embeddedFont><p:font typeface="Kept"/><p:regular r:id="rId1"/></p:embeddedFont></p:embeddedFontLst>"#.into(),
    );
    p.embedded_font_rels = vec![kition_slide_model::KeptRel {
        id: "rId1".into(),
        rel_type: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font".into(),
        media: font,
        rels: vec![],
    }];
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let names: Vec<String> = {
        let z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).expect("zip");
        z.file_names().map(String::from).collect()
    };
    assert!(names.iter().any(|n| n.starts_with("ppt/media/") && n.ends_with(".svg")), "{names:?}");
    assert!(names.iter().any(|n| n.starts_with("ppt/fonts/")), "{names:?}");
    let back = kition_slide_pptx::import(&bytes).expect("import");
    let ShapeKind::Picture { fill } = &by_name(&back.slides[0], "Vector").kind else { panic!("not a picture") };
    assert!(fill.ext.as_deref().is_some_and(|x| x.contains("svgBlip")));
    let kept = back.media(fill.ext_rels[0].media).expect("svg media");
    assert_eq!(kept.data.as_slice(), b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    assert!(back.embedded_font_list.as_deref().is_some_and(|x| x.contains("Kept")));
    assert_eq!(back.media(back.embedded_font_rels[0].media).map(|m| m.data.as_slice().to_vec()), Some(b"not-a-real-font".to_vec()));
}

/// A smart graphic group's kept diagram is written back while the group is as read, with its data
/// part naming the drawing by its new id; an edited group is written as its shapes.
#[test]
fn kept_smart_graphic_is_written_back_until_edited() {
    use kition_slide_model::{KeptDiagram, KeptRel, shapes_fingerprint};
    const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
    let mut p = defaults::new_slide_file(None);
    p.slides.clear();
    let data_xml = r#"<dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:extLst><a:ext xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" uri="http://schemas.microsoft.com/office/drawing/2008/diagram"><dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" relId="rId9" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>"#;
    let part = |p: &mut SlideFile, name: &str, ct: &str, body: &str| p.add_media(name, ct, body.as_bytes().to_vec());
    let ct = "application/vnd.openxmlformats-officedocument.drawingml.";
    let data = part(&mut p, "data1.xml", &format!("{ct}diagramData+xml"), data_xml);
    let layout = part(
        &mut p,
        "layout1.xml",
        &format!("{ct}diagramLayout+xml"),
        "<dgm:layoutDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let style = part(
        &mut p,
        "quickStyle1.xml",
        &format!("{ct}diagramStyle+xml"),
        "<dgm:styleDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let colors = part(
        &mut p,
        "colors1.xml",
        &format!("{ct}diagramColors+xml"),
        "<dgm:colorsDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let drawing = part(
        &mut p,
        "drawing1.xml",
        "application/vnd.ms-office.drawingml.diagramDrawing+xml",
        "<dsp:drawing xmlns:dsp=\"http://schemas.microsoft.com/office/drawing/2008/diagram\"><dsp:spTree/></dsp:drawing>",
    );
    let rel = |id: &str, ty: &str, media| KeptRel { id: id.into(), rel_type: format!("{RT}{ty}"), media, rels: vec![] };
    let frame = r#"<p:graphicFrame xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:nvGraphicFramePr><p:cNvPr id="4" name="Diagram 1"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId1" r:lo="rId2" r:qs="rId3" r:cs="rId4"/></a:graphicData></a:graphic></p:graphicFrame>"#;
    let mut box1 = shape(&mut p, "Box", 0.0, 0.0, 50.0, 20.0);
    box1.text = Some(TextBody::from_text("Kept"));
    let children = vec![box1];
    let mut g = shape(&mut p, "Diagram 1", 100.0, 100.0, 300.0, 200.0);
    g.diagram = Some(Box::new(KeptDiagram {
        frame: frame.into(),
        parts: vec![
            rel("rId1", "diagramData", data),
            rel("rId2", "diagramLayout", layout),
            rel("rId3", "diagramQuickStyle", style),
            rel("rId4", "diagramColors", colors),
        ],
        drawing: Some(KeptRel {
            id: String::new(),
            rel_type: "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing".into(),
            media: drawing,
            rels: vec![],
        }),
        drawing_ref: "rId9".into(),
        fingerprint: shapes_fingerprint(&children, &p.media),
    }));
    g.kind = ShapeKind::Group { children, child: Xfrm::new(0.0, 0.0, 300.0, 200.0) };
    let mut s = new_slide(&mut p, "Smart graphic");
    s.shapes.push(g.clone());
    p.slides.push(Arc::new(s.clone()));

    let read = |bytes: &[u8], name: &str| -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        let mut f = z.by_name(name).expect(name);
        let mut out = String::new();
        std::io::Read::read_to_string(&mut f, &mut out).expect("utf8");
        out
    };
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let slide = read(&bytes, "ppt/slides/slide1.xml");
    assert!(slide.contains("relIds"), "kept frame written");
    let rels = read(&bytes, "ppt/slides/_rels/slide1.xml.rels");
    let names: Vec<String> = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).expect("zip").file_names().map(String::from).collect();
    let data_name = names.iter().find(|n| n.starts_with("ppt/diagrams/data")).expect("data part");
    let data_out = read(&bytes, data_name);
    let new_ref = data_out.split("relId=\"").nth(1).and_then(|r| r.split('"').next()).expect("relId");
    assert!(rels.contains(&format!("Id=\"{new_ref}\"")) && rels.contains("diagramDrawing"), "{rels}\n{data_out}");
    assert!(rels.split("<Relationship ").any(|r| r.contains(&format!("Id=\"{new_ref}\"")) && r.contains("diagramDrawing")));

    // Edited: written as the group's shapes, the edit kept.
    if let ShapeKind::Group { children, .. } = &mut s.shapes.last_mut().expect("group").kind {
        children[0].text = Some(TextBody::from_text("Edited"));
    }
    p.slides[0] = Arc::new(s);
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let slide = read(&bytes, "ppt/slides/slide1.xml");
    assert!(!slide.contains("relIds") && slide.contains("Edited"), "edited group written as shapes");
}

#[test]
fn picture_bilevel_thresholds_survive_regeneration_including_zero() {
    for threshold in [0.0, 0.25, 1.0] {
        let mut p = SlideFile::default();
        let media = p.add_media("threshold.png", "image/png", png(2, 2, [20, 40, 180]));
        let mut picture = shape(&mut p, "Threshold", 10.0, 10.0, 40.0, 40.0);
        picture.kind = ShapeKind::Picture {
            fill: PictureFill { media, adjust: PictureAdjust { bi_level: Some(threshold), ..Default::default() }, ..Default::default() },
        };
        Arc::make_mut(&mut p.slides[0]).shapes = vec![picture];
        let bytes = kition_slide_pptx::export(&p).unwrap();
        let slide = part_text(&bytes, "ppt/slides/slide1.xml");
        assert!(slide.contains(&format!("<a:biLevel thresh=\"{}\"", (threshold * 100_000.0) as u32)), "{slide}");
        let restored = kition_slide_pptx::import(&bytes).unwrap();
        let ShapeKind::Picture { fill } = &restored.slides[0].shapes[0].kind else { panic!("expected picture") };
        assert_eq!(fill.adjust.bi_level, Some(threshold));
        assert_eq!(restored.media(fill.media).unwrap().data.as_slice(), p.media(media).unwrap().data.as_slice());
    }
}

#[test]
fn gradient_tile_rectangles_survive_regeneration_including_outsets() {
    for offsets in [[0.0; 4], [-0.25, 0.0, -1.0, -0.5], [0.25, 0.5, 0.0, 0.0]] {
        let mut p = SlideFile::default();
        let mut sh = shape(&mut p, "Expanded gradient", 20.0, 30.0, 100.0, 80.0);
        let fill = Gradient {
            stops: vec![GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::BLACK) }, GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::WHITE) }],
            shape: GradientShape::Path { path: "circle".into(), focus: [0.5; 4] },
            tile_rect: Some(offsets),
            rotate_with_shape: false,
        };
        sh.fill = Some(Fill::Gradient(fill.clone()));
        Arc::make_mut(&mut p.slides[0]).shapes = vec![sh];
        for _ in 0..2 {
            let bytes = kition_slide_pptx::export(&p).unwrap();
            let slide = part_text(&bytes, "ppt/slides/slide1.xml");
            assert!(slide.contains("<a:tileRect "), "{slide}");
            p = kition_slide_pptx::import(&bytes).unwrap();
            assert_eq!(p.slides[0].shapes[0].fill, Some(Fill::Gradient(fill.clone())));
        }
    }
}

#[test]
fn object_preview_corrections_survive_preserved_xml_and_picture_fallback() {
    let (mut p, _, _) = ole_slide_file();
    let ShapeKind::Opaque { xml, .. } = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { panic!("expected object") };
    *xml = xml.replace("<a:blip r:embed=\"rId2\"/>", "<a:blip r:embed=\"rId2\"><a:biLevel thresh=\"50000\"/></a:blip>");
    let mut restored = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    for _ in 0..2 {
        let ShapeKind::Opaque { preview, preview_adjust, .. } = &restored.slides[0].shapes[0].kind else { panic!("expected object") };
        assert!(preview.is_some());
        assert_eq!(preview_adjust.as_ref().and_then(|a| a.bi_level), Some(0.5));
        restored = kition_slide_pptx::import(&kition_slide_pptx::export(&restored).unwrap()).unwrap();
    }
    let ShapeKind::Opaque { xml, .. } = &mut Arc::make_mut(&mut restored.slides[0]).shapes[0].kind else { panic!("expected object") };
    *xml = "<invalid".into();
    let fallback = kition_slide_pptx::import(&kition_slide_pptx::export(&restored).unwrap()).unwrap();
    let ShapeKind::Picture { fill } = &fallback.slides[0].shapes[0].kind else { panic!("expected preview picture") };
    assert_eq!(fill.adjust.bi_level, Some(0.5));
}

/// A slide's id comes from its file id (`p:sldId/@id`) when no shape uses that number, so after
/// another program deletes a slide and the file is read again, the other slides keep the ids an
/// agent read before (khp-bench tasks/conv "unseen": ids had been numbered by position). A file
/// id that a shape also uses is not kept: ids name one thing each.
#[test]
fn slide_ids_survive_reading_a_changed_file_again() {
    let mut p = defaults::new_slide_file(None);
    p.slides.clear();
    let mut slides = Vec::new();
    for (i, file_id) in [300u32, 301, 302, 303].into_iter().enumerate() {
        let mut s = new_slide(&mut p, &format!("S{i}"));
        s.file_id = Some(file_id);
        slides.push(Arc::new(s));
    }
    p.slides = slides;
    let first = round(&p);
    let ids: Vec<u32> = first.slides.iter().map(|s| s.id.0).collect();
    assert_eq!(ids, vec![300, 301, 302, 303]);
    let mut changed = first.clone();
    changed.slides.remove(1);
    let again = round(&changed);
    assert_eq!(again.slides.iter().map(|s| s.id.0).collect::<Vec<_>>(), vec![300, 302, 303]);

    let mut clash = defaults::new_slide_file(None);
    clash.slides.clear();
    let mut s = new_slide(&mut clash, "Clash");
    s.file_id = Some(400);
    if let Some(first_shape) = s.shapes.first_mut() {
        first_shape.id = ShapeId(400);
    }
    clash.slides = vec![Arc::new(s)];
    let read = round(&clash);
    assert_ne!(read.slides[0].id.0, 400, "a file id that a shape uses is not the slide's id");
}

/// Empty smart graphic nodes keep their data model's placeholder prompts ("[Text]"), by the drawing
/// shapes' `modelId`, for the editor to show while their paragraphs are empty.
#[test]
fn smart_graphic_placeholder_nodes_keep_their_prompts() {
    use kition_slide_model::{KeptDiagram, KeptRel, shapes_fingerprint};
    const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
    let mut p = defaults::new_slide_file(None);
    p.slides.clear();
    let data_xml = r#"<dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:ptLst><dgm:pt modelId="{A}"><dgm:prSet phldrT="[Text]" phldr="1"/></dgm:pt><dgm:pt modelId="{S}" type="pres"><dgm:prSet presAssocID="{A}"/></dgm:pt></dgm:ptLst><dgm:cxnLst><dgm:cxn modelId="{1}" type="presOf" srcId="{A}" destId="{S}" srcOrd="0" destOrd="0"/></dgm:cxnLst><dgm:extLst><a:ext xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" uri="http://schemas.microsoft.com/office/drawing/2008/diagram"><dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" relId="rId9" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>"#;
    let drawing_xml = r#"<dsp:drawing xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/><dsp:sp modelId="{S}"><dsp:nvSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></dsp:spPr><dsp:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US" sz="2000"/></a:p></dsp:txBody></dsp:sp></dsp:spTree></dsp:drawing>"#;
    let part = |p: &mut SlideFile, name: &str, ct: &str, body: &str| p.add_media(name, ct, body.as_bytes().to_vec());
    let ct = "application/vnd.openxmlformats-officedocument.drawingml.";
    let data = part(&mut p, "data1.xml", &format!("{ct}diagramData+xml"), data_xml);
    let layout = part(
        &mut p,
        "layout1.xml",
        &format!("{ct}diagramLayout+xml"),
        "<dgm:layoutDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let style = part(
        &mut p,
        "quickStyle1.xml",
        &format!("{ct}diagramStyle+xml"),
        "<dgm:styleDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let colors = part(
        &mut p,
        "colors1.xml",
        &format!("{ct}diagramColors+xml"),
        "<dgm:colorsDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
    );
    let drawing = part(&mut p, "drawing1.xml", "application/vnd.ms-office.drawingml.diagramDrawing+xml", drawing_xml);
    let rel = |id: &str, ty: &str, media| KeptRel { id: id.into(), rel_type: format!("{RT}{ty}"), media, rels: vec![] };
    let frame = r#"<p:graphicFrame xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:nvGraphicFramePr><p:cNvPr id="4" name="Diagram 1"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId1" r:lo="rId2" r:qs="rId3" r:cs="rId4"/></a:graphicData></a:graphic></p:graphicFrame>"#;
    let children = vec![shape(&mut p, "Node", 0.0, 0.0, 100.0, 50.0)];
    let mut g = shape(&mut p, "Diagram 1", 0.0, 0.0, 100.0, 50.0);
    g.diagram = Some(Box::new(KeptDiagram {
        frame: frame.into(),
        parts: vec![
            rel("rId1", "diagramData", data),
            rel("rId2", "diagramLayout", layout),
            rel("rId3", "diagramQuickStyle", style),
            rel("rId4", "diagramColors", colors),
        ],
        drawing: Some(KeptRel {
            id: String::new(),
            rel_type: "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing".into(),
            media: drawing,
            rels: vec![],
        }),
        drawing_ref: "rId9".into(),
        fingerprint: shapes_fingerprint(&children, &p.media),
    }));
    g.kind = ShapeKind::Group { children, child: Xfrm::new(0.0, 0.0, 100.0, 50.0) };
    let mut s = new_slide(&mut p, "Smart graphic");
    s.shapes.push(g);
    p.slides.push(Arc::new(s));
    let back = kition_slide_pptx::import(&kition_slide_pptx::export(&p).expect("export")).expect("import");
    let ShapeKind::Group { children, .. } = &by_name(&back.slides[0], "Diagram 1").kind else { panic!("not a group") };
    assert_eq!(children[0].prompts, vec![Some("[Text]".to_string())]);
}
