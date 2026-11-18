//! A chart made or whose data were edited is saved with a workbook of its data, which its series
//! name (`c:externalData`), so PowerPoint's Edit Data opens them as cells; an unchanged chart
//! keeps the workbook it came with.

use std::io::{Cursor, Read};
use std::sync::Arc;

use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

fn parts_like(bytes: &[u8], like: &str) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    let names: Vec<String> = z.file_names().filter(|n| n.contains(like)).map(str::to_string).collect();
    names
        .into_iter()
        .map(|n| {
            let mut b = Vec::new();
            z.by_name(&n).unwrap().read_to_end(&mut b).unwrap();
            (n, b)
        })
        .collect()
}

fn sheet_of(book: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(Cursor::new(book.to_vec())).unwrap();
    let mut s = String::new();
    z.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn chart_of(p: &SlideFile) -> &Chart {
    p.slides[0].shapes.iter().find_map(|s| if let ShapeKind::Chart(c) = &s.kind { Some(c.as_ref()) } else { None }).unwrap()
}

#[test]
fn new_and_edited_charts_are_saved_with_a_workbook_of_their_data() {
    let mut pres = SlideFile::default();
    let mut chart = Chart::sample(ChartType::Column);
    chart.categories = vec!["North".into(), "South".into()];
    chart.series.truncate(1);
    chart.series[0].name = "Sales".into();
    chart.series[0].values = vec![Some(3.0), Some(4.0)];
    Arc::make_mut(&mut pres.slides[0]).shapes = vec![Shape { id: ShapeId(900), kind: ShapeKind::Chart(Box::new(chart)), ..Default::default() }];
    let bytes = kition_slide_pptx::export(&pres).unwrap();
    let part = String::from_utf8(parts_like(&bytes, "ppt/charts/chart1.xml").remove(0).1).unwrap();
    assert!(part.contains("<c:externalData") && part.contains("<c:f>Sheet1!$B$2:$B$3</c:f>"), "{part}");
    let rels = String::from_utf8(parts_like(&bytes, "ppt/charts/_rels/chart1.xml.rels").remove(0).1).unwrap();
    assert!(rels.contains("relationships/package") && rels.contains("../embeddings/"), "{rels}");
    let books = parts_like(&bytes, "ppt/embeddings/");
    assert_eq!(books.len(), 1);
    let sheet = sheet_of(&books[0].1);
    assert!(sheet.contains("North") && sheet.contains("Sales") && sheet.contains("<v>4</v>"), "{sheet}");

    // Reopened unchanged, it keeps its workbook; with its data edited, it gets a new one.
    let back = kition_slide_pptx::import(&bytes).unwrap();
    assert_eq!(chart_of(&back).series[0].values, vec![Some(3.0), Some(4.0)]);
    let same = kition_slide_pptx::export(&back).unwrap();
    assert_eq!(parts_like(&same, "ppt/embeddings/").len(), 1);
    let mut edited = back.clone();
    if let ShapeKind::Chart(c) = &mut Arc::make_mut(&mut edited.slides[0]).shapes[0].kind {
        c.series[0].values = vec![Some(30.0), Some(40.0)];
    }
    let again = kition_slide_pptx::export(&edited).unwrap();
    let books = parts_like(&again, "ppt/embeddings/");
    assert_eq!(books.len(), 1, "{:?}", books.iter().map(|b| &b.0).collect::<Vec<_>>());
    let sheet = sheet_of(&books[0].1);
    assert!(sheet.contains("<v>40</v>") && !sheet.contains("<v>4</v>"), "{sheet}");
    let rels = String::from_utf8(parts_like(&again, "ppt/charts/_rels/chart1.xml.rels").remove(0).1).unwrap();
    assert_eq!(rels.matches("relationships/package").count(), 1, "{rels}");
    assert_eq!(chart_of(&kition_slide_pptx::import(&again).unwrap()).series[0].values, vec![Some(30.0), Some(40.0)]);
}
