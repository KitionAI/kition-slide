//! Marker effects retain their independent source ownership after chart edits.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

fn fixture() -> SlideFile {
    let mut presentation = SlideFile::default();
    Arc::make_mut(&mut presentation.slides[0]).shapes =
        vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Scatter))), ..Default::default() }];
    let source = kition_slide_pptx::export(&presentation).unwrap();
    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
    <c:chart><c:plotArea><c:scatterChart><c:scatterStyle val="marker"/>
    <c:ser><c:idx val="0"/><c:order val="0"/>
      <c:spPr><a:effectLst><a:outerShdw dist="127000" dir="0"><a:srgbClr val="000000"/></a:outerShdw></a:effectLst></c:spPr>
      <c:marker><c:symbol val="circle"/><c:spPr><a:effectLst><a:outerShdw blurRad="25400" dist="38100" dir="0"><a:srgbClr val="102030"><a:alpha val="50000"/></a:srgbClr></a:outerShdw><a:glow rad="12700"><a:srgbClr val="405060"/></a:glow></a:effectLst></c:spPr></c:marker>
      <c:dPt><c:idx val="0"/><c:marker><c:size val="8"/></c:marker></c:dPt>
      <c:dPt><c:idx val="1"/><c:marker><c:spPr><a:effectLst/></c:spPr></c:marker></c:dPt>
      <c:dPt><c:idx val="2"/><c:marker><c:spPr><a:effectLst><a:outerShdw dist="76200" dir="5400000"><a:srgbClr val="708090"/></a:outerShdw></a:effectLst></c:spPr></c:marker></c:dPt>
      <c:yVal><c:numLit><c:ptCount val="3"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt><c:pt idx="2"><c:v>3</c:v></c:pt></c:numLit></c:yVal>
    </c:ser>
    <c:ser><c:idx val="1"/><c:order val="1"/><c:marker><c:symbol val="square"/><c:spPr><a:effectLst/></c:spPr></c:marker><c:yVal><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>4</c:v></c:pt></c:numLit></c:yVal></c:ser>
    <c:ser><c:idx val="2"/><c:order val="2"/><c:marker><c:symbol val="none"/></c:marker><c:yVal><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>5</c:v></c:pt></c:numLit></c:yVal></c:ser>
    </c:scatterChart></c:plotArea></c:chart></c:chartSpace>"#;
    let mut input = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut part = input.by_index(i).unwrap();
        let name = part.name().to_owned();
        let mut data = Vec::new();
        part.read_to_end(&mut data).unwrap();
        output.start_file(&name, zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(if name == "ppt/charts/chart1.xml" { xml.as_bytes() } else { &data }).unwrap();
    }
    kition_slide_pptx::import(&output.finish().unwrap().into_inner()).unwrap()
}

fn chart(presentation: &SlideFile) -> &Chart {
    let ShapeKind::Chart(chart) = &presentation.slides[0].shapes[0].kind else { panic!("chart missing") };
    chart
}

fn chart_mut(presentation: &mut SlideFile) -> &mut Chart {
    let ShapeKind::Chart(chart) = &mut Arc::make_mut(&mut presentation.slides[0]).shapes[0].kind else { panic!("chart missing") };
    chart
}

fn check_presence(chart: &Chart) {
    let first = &chart.series[0].fmt;
    let effects = first.marker_effects.as_ref().unwrap();
    let shadow = effects.outer_shadow.as_ref().unwrap();
    assert_eq!((shadow.blur, shadow.dist, shadow.dir), (2.0, 3.0, 0.0));
    assert!(effects.glow.is_some(), "unrendered supported effects still round-trip");
    assert!(first.point_markers[0].1.effects.is_none(), "missing point effects inherit the series marker");
    assert!(first.point_markers[1].1.effects.as_ref().unwrap().is_empty(), "empty point list is retained as a clear override");
    assert_eq!(first.point_markers[2].1.effects.as_ref().unwrap().outer_shadow.as_ref().unwrap().dist, 6.0);
    assert!(chart.series[1].fmt.marker_effects.as_ref().unwrap().is_empty());
    assert!(chart.series[2].fmt.marker_effects.is_none());
    assert_eq!(chart.series[2].marker.as_deref(), Some("none"));
}

#[test]
fn import_preserves_marker_effect_presence_separate_from_series_body() {
    let presentation = fixture();
    check_presence(chart(&presentation));
    assert_eq!(chart(&presentation).series[0].fmt.shadow.as_ref().unwrap().dist, 10.0);
}

#[test]
fn marker_effects_survive_unchanged_and_edited_chart_roundtrips() {
    let presentation = fixture();
    let unchanged = kition_slide_pptx::import(&kition_slide_pptx::export(&presentation).unwrap()).unwrap();
    check_presence(chart(&unchanged));
    let mut edited = unchanged;
    chart_mut(&mut edited).series[0].values[0] = Some(7.0);
    let edited = kition_slide_pptx::import(&kition_slide_pptx::export(&edited).unwrap()).unwrap();
    check_presence(chart(&edited));
    assert_eq!(chart(&edited).series[0].values[0], Some(7.0));
}

#[test]
fn effects_only_series_marker_and_empty_point_override_are_exported() {
    let mut presentation = fixture();
    let first = &mut chart_mut(&mut presentation).series[0];
    first.marker = None;
    first.fmt.marker_size = None;
    first.fmt.marker_fill = None;
    first.fmt.marker_line = None;
    first.values[0] = Some(8.0);
    let roundtrip = kition_slide_pptx::import(&kition_slide_pptx::export(&presentation).unwrap()).unwrap();
    check_presence(chart(&roundtrip));
}
