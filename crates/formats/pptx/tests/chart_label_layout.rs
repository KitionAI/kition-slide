//! Authored label positions and sizes survive regeneration after chart edits.

use std::io::{Cursor, Read};
use std::sync::Arc;

use kition_slide_model::chart::{Chart, ChartType, DataLabels, ManualLayout};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};
use quick_xml::{Reader, events::Event};

fn chart(p: &SlideFile) -> &Chart {
    let ShapeKind::Chart(c) = &p.slides[0].shapes[0].kind else { panic!("chart missing") };
    c
}

#[test]
fn regenerated_chart_keeps_group_and_point_label_layouts_in_their_schema_locations() {
    let point_layout = ManualLayout { x: Some(0.25), y: Some(-0.1), w: Some(0.15), h: Some(0.08), x_edge: true, ..Default::default() };
    let group_layout = ManualLayout { x: Some(0.05), y: Some(0.1), w: Some(0.12), h: Some(0.06), ..Default::default() };
    let mut c = Chart::sample(ChartType::Line);
    c.fmt.labels = Some(DataLabels { value: true, layout: Some(group_layout.clone()), ..Default::default() });
    c.series[0].fmt.labels = Some(DataLabels {
        value: true,
        num_fmt: Some("0.00".into()),
        points: vec![
            (0, DataLabels { value: true, layout: Some(point_layout.clone()), ..Default::default() }),
            (1, DataLabels { value: true, num_fmt: Some("0.000".into()), ..Default::default() }),
        ],
        ..Default::default()
    });
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(c)), ..Default::default() }];
    let mut p = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    assert!(chart(&p).raw.is_some(), "the edited document starts with a reusable raw chart part");
    let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { unreachable!() };
    c.series[0].values[0] = Some(19.0);
    // Exercise the regenerated writer even when a kept chart can be patched after edits.
    c.raw = None;
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let restored = kition_slide_pptx::import(&bytes).unwrap();
    assert_eq!(chart(&restored).series[0].values[0], Some(19.0));
    assert_eq!(chart(&restored).fmt.labels.as_ref().unwrap().layout, Some(group_layout));
    assert_eq!(chart(&restored).series[0].fmt.labels.as_ref().unwrap().point(0).unwrap().layout, Some(point_layout));
    let labels = chart(&restored).series[0].fmt.labels.as_ref().unwrap();
    assert_eq!(labels.num_fmt.as_deref(), Some("0.00"));
    assert!(labels.point(0).unwrap().num_fmt.is_none(), "point source-linked reset survives a custom parent");
    assert_eq!(labels.point(1).unwrap().num_fmt.as_deref(), Some("0.000"));

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("ppt/charts/chart1.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(
        xml.contains("<c:xMode val=\"edge\"/><c:yMode val=\"factor\"/><c:x val=\"0.25\"/><c:y val=\"-0.1\"/>"),
        "layout modes precede coordinates"
    );
    assert!(xml.contains("<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>"), "point reset is explicit in regenerated XML");
    let mut reader = Reader::from_str(&xml);
    let mut stack: Vec<String> = vec![];
    let (mut standard, mut group, mut point) = (0, 0, 0);
    loop {
        match reader.read_event().unwrap() {
            Event::Start(e) => {
                let name = String::from_utf8(e.name().as_ref().to_vec()).unwrap();
                if name == "c:layout" {
                    assert_eq!(stack.last().map(String::as_str), Some("c:dLbl"), "CT_DLbls has no standard layout child");
                    standard += 1;
                }
                if name == "c15:layout" {
                    assert_eq!(stack.last().map(String::as_str), Some("c:ext"));
                    if stack.iter().any(|n| n == "c:dLbl") { point += 1 } else { group += 1 }
                }
                stack.push(name);
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    assert_eq!((standard, group, point), (1, 1, 1));
}
