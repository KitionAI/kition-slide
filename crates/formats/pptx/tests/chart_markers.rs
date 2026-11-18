//! Series and point picture markers survive both unchanged and edited chart round trips.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::style::Fill;
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

fn picture_marker_fixture() -> (SlideFile, Vec<u8>) {
    let mut pres = SlideFile::default();
    Arc::make_mut(&mut pres.slides[0]).shapes =
        vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Line))), ..Default::default() }];
    let source = kition_slide_pptx::export(&pres).unwrap();
    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
    <c:style val="42"/><c:chart><c:plotArea><c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="0"/><c:order val="0"/>
    <c:marker><c:symbol val="picture"/><c:spPr><a:blipFill><a:blip r:embed="markerImage"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:ln><a:noFill/></a:ln></c:spPr></c:marker>
    <c:dPt><c:idx val="0"/><c:marker><c:symbol val="picture"/><c:size val="12"/><c:spPr><a:blipFill><a:blip r:embed="markerImage"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:ln w="25400"><a:solidFill><a:srgbClr val="112233"/></a:solidFill><a:prstDash val="dash"/></a:ln></c:spPr></c:marker></c:dPt>
    <c:dPt><c:idx val="1"/><c:marker><c:symbol val="none"/></c:marker></c:dPt>
    <c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>2</c:v></c:pt><c:pt idx="1"><c:v>3</c:v></c:pt></c:numLit></c:val>
    </c:ser></c:lineChart></c:plotArea><c:dispBlanksAs val="span"/></c:chart></c:chartSpace>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="markerImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/marker.png"/></Relationships>"#;
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(12, 8, image::Rgba([240, 20, 80, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let png = png.into_inner();
    let mut input = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut part = input.by_index(i).unwrap();
        let name = part.name().to_owned();
        if name == "ppt/charts/chart1.xml" || name == "ppt/charts/_rels/chart1.xml.rels" {
            continue;
        }
        let mut data = Vec::new();
        part.read_to_end(&mut data).unwrap();
        output.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(&data).unwrap();
    }
    for (name, data) in
        [("ppt/charts/chart1.xml", xml.as_bytes()), ("ppt/charts/_rels/chart1.xml.rels", rels.as_bytes()), ("ppt/media/marker.png", png.as_slice())]
    {
        output.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(data).unwrap();
    }
    let bytes = output.finish().unwrap().into_inner();
    (kition_slide_pptx::import(&bytes).unwrap(), png)
}

fn chart(doc: &SlideFile) -> &Chart {
    let ShapeKind::Chart(chart) = &doc.slides[0].shapes[0].kind else { panic!("chart missing") };
    chart
}

fn marker_fill(doc: &SlideFile, point: bool) -> &kition_slide_model::style::PictureFill {
    let series = &chart(doc).series[0];
    let fill = if point { &series.fmt.point_markers[0].1.fill } else { &series.fmt.marker_fill };
    let Some(Fill::Picture(fill)) = fill else { panic!("picture marker missing") };
    fill
}

fn marker_fill_mut(doc: &mut SlideFile, point: bool) -> &mut kition_slide_model::style::PictureFill {
    let ShapeKind::Chart(chart) = &mut Arc::make_mut(&mut doc.slides[0]).shapes[0].kind else { panic!("chart missing") };
    let series = &mut chart.series[0];
    let fill = if point { &mut series.fmt.point_markers[0].1.fill } else { &mut series.fmt.marker_fill };
    let Some(Fill::Picture(fill)) = fill else { panic!("picture marker missing") };
    fill
}

#[test]
fn picture_marker_keeps_its_image_after_roundtrip() {
    let (first, png) = picture_marker_fixture();
    let saved = kition_slide_pptx::export(&first).unwrap();
    let mut package = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    package.by_name("ppt/charts/chart1.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains("val=\"span\""), "unchanged charts retain unmodeled raw properties");
    let second = kition_slide_pptx::import(&saved).unwrap();
    let mut edited = second.clone();
    let ShapeKind::Chart(chart) = &mut Arc::make_mut(&mut edited.slides[0]).shapes[0].kind else { panic!("chart missing") };
    chart.series[0].values[0] = Some(7.0);
    let edited = kition_slide_pptx::import(&kition_slide_pptx::export(&edited).unwrap()).unwrap();
    for (doc, value) in [(&first, 2.0), (&second, 2.0), (&edited, 7.0)] {
        let chart = doc.slides[0]
            .shapes
            .iter()
            .find_map(|s| match &s.kind {
                ShapeKind::Chart(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(chart.style, 42);
        assert_eq!(chart.series[0].values[0], Some(value));
        assert_eq!(chart.series[0].marker.as_deref(), Some("picture"));
        assert_eq!(chart.series[0].fmt.marker_size, None);
        let Some(Fill::Picture(fill)) = &chart.series[0].fmt.marker_fill else { panic!("picture marker missing") };
        assert_eq!(doc.media(fill.media).unwrap().data.as_slice(), png);
        let points = &chart.series[0].fmt.point_markers;
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].0, 0);
        assert_eq!(points[0].1.size, Some(12.0));
        let line = points[0].1.line.as_ref().expect("point marker outline");
        assert_eq!(line.width, Some(2.0));
        assert_eq!(line.dash, Some(kition_slide_model::style::Dash::Preset { name: "dash".into() }));
        let Some(Fill::Picture(fill)) = &points[0].1.fill else { panic!("point picture marker missing") };
        assert_eq!(doc.media(fill.media).unwrap().data.as_slice(), png);
        assert_eq!(points[1].1.symbol.as_deref(), Some("none"));
    }
}

#[test]
fn changing_only_a_marker_image_is_not_lost_to_raw_reuse() {
    let (source, original) = picture_marker_fixture();
    let mut replacement = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(12, 8, image::Rgba([20, 240, 80, 255])).write_to(&mut replacement, image::ImageFormat::Png).unwrap();
    let replacement = replacement.into_inner();
    for point in [false, true] {
        let mut doc = source.clone();
        let media = kition_slide_model::MediaId(98_765);
        doc.media.push(kition_slide_model::MediaItem {
            id: media,
            name: "replacement.png".into(),
            content_type: "image/png".into(),
            data: Arc::new(replacement.clone()),
            link: None,
        });
        marker_fill_mut(&mut doc, point).media = media;
        let result = kition_slide_pptx::import(&kition_slide_pptx::export(&doc).unwrap()).unwrap();
        assert_eq!(result.media(marker_fill(&result, point).media).unwrap().data.as_slice(), replacement);
        assert_eq!(result.media(marker_fill(&result, !point).media).unwrap().data.as_slice(), original);
        assert_eq!(chart(&result).series[0].values, chart(&source).series[0].values);
    }
}

#[test]
fn changing_only_marker_picture_properties_is_not_lost_to_raw_reuse() {
    use kition_slide_model::style::PictureMode;
    let (source, _) = picture_marker_fixture();
    for point in [false, true] {
        for property in 0..4 {
            let mut doc = source.clone();
            let fill = marker_fill_mut(&mut doc, point);
            match property {
                0 => fill.crop = [0.1, 0.2, 0.3, 0.0],
                1 => fill.alpha = Some(0.37),
                2 => fill.mode = PictureMode::Tile { tx: 2.0, ty: 1.0, sx: 0.5, sy: 0.75, flip: "x".into(), align: "ctr".into() },
                _ => fill.adjust.brightness = 0.25,
            }
            let mut expected = fill.clone();
            let result = kition_slide_pptx::import(&kition_slide_pptx::export(&doc).unwrap()).unwrap();
            let actual = marker_fill(&result, point);
            expected.media = actual.media;
            assert_eq!(actual, &expected, "point={point}, property={property}");
            assert_eq!(chart(&result).series[0].values, chart(&source).series[0].values);
        }
    }
}
