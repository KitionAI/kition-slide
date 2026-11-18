//! Separator-only point overrides survive import and regenerated chart output.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

const LABELS: &str = r#"<c:dLbls>
<c:dLbl><c:idx val="0"/><c:separator> &amp; </c:separator></c:dLbl>
<c:dLbl><c:idx val="1"/><c:separator/></c:dLbl>
<c:numFmt formatCode="0.00" sourceLinked="0"/>
<c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1200"/></a:pPr></a:p></c:txPr>
<c:showVal val="1"/><c:showCatName val="1"/><c:separator> / </c:separator>
</c:dLbls>"#;

fn chart(p: &SlideFile) -> &Chart {
    let ShapeKind::Chart(c) = &p.slides[0].shapes[0].kind else { panic!("chart missing") };
    c
}

fn authored() -> SlideFile {
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes =
        vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Column))), ..Default::default() }];
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if entry.name() == "ppt/charts/chart1.xml" {
            let mut text = String::from_utf8(data).unwrap();
            // dLbls precedes the series category/value caches in CT_BarSer.
            let series = text.find("<c:ser>").unwrap();
            let offset = series + text[series..].find("<c:cat>").unwrap();
            text.insert_str(offset, LABELS);
            data = text.into_bytes();
        }
        writer.start_file(entry.name(), zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(&data).unwrap();
    }
    kition_slide_pptx::import(&writer.finish().unwrap().into_inner()).unwrap()
}

fn check(p: &SlideFile, custom: &str) {
    let labels = chart(p).series[0].fmt.labels.as_ref().unwrap();
    assert_eq!(labels.points.len(), 2, "separator-only records must become point overrides");
    for (index, separator) in [(0, custom), (1, ""), (2, " / ")] {
        let point = labels.point(index).unwrap();
        assert_eq!(point.separator.as_deref(), Some(separator));
        assert!(point.value && point.category);
        assert_eq!(point.num_fmt.as_deref(), Some("0.00"));
        assert_eq!(point.text.size, Some(12.0));
    }
}

#[test]
fn authored_separator_overrides_survive_data_and_separator_only_edits() {
    let mut p = authored();
    check(&p, " & ");
    let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { unreachable!() };
    assert!(c.raw.is_some());
    c.series[0].values[0] = Some(19.0);
    c.raw = None;
    let mut p = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    check(&p, " & ");
    assert_eq!(chart(&p).series[0].values[0], Some(19.0));

    let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].kind else { unreachable!() };
    assert!(c.raw.is_some());
    c.series[0].fmt.labels.as_mut().unwrap().points[0].1.separator = Some(" :: ".into());
    let p = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    check(&p, " :: ");
}
