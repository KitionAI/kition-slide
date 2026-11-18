//! Where a shape's text sits when its `a:bodyPr` names no anchor: top for a shape's own text (the
//! schema default, as PowerPoint shows it), inherited for placeholders.

use std::io::{Read, Write};

use kition_slide_model::text::Anchor;
use kition_slide_model::{SlideFile, defaults};

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
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

/// `bytes` with part `name` replaced by `xml`.
fn replace_part(bytes: &[u8], name: &str, xml: &str) -> Vec<u8> {
    let mut zout = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut done = false;
    for (n, mut data) in parts(bytes) {
        if n == name {
            data = xml.as_bytes().to_vec();
            done = true;
        }
        zout.start_file(n, zip::write::SimpleFileOptions::default()).expect("start");
        zout.write_all(&data).expect("write");
    }
    assert!(done, "no part {name}");
    zout.finish().expect("finish").into_inner()
}

const SLIDE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Plain"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr wrap="square"/><a:lstStyle/><a:p><a:r><a:t>No anchor</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Centred"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="2000000"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr anchor="ctr"/><a:lstStyle/><a:p><a:r><a:t>Centred</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="4" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#;

fn anchor_of(p: &SlideFile, name: &str) -> Option<Anchor> {
    p.slides[0].shapes.iter().find(|s| s.name == name).and_then(|s| s.text.as_ref()).and_then(|t| t.body.anchor)
}

#[test]
fn a_shapes_own_text_without_an_anchor_is_top_anchored() {
    let bytes = kition_slide_pptx::export(&defaults::new_slide_file(None)).expect("export");
    let bytes = replace_part(&bytes, "ppt/slides/slide1.xml", SLIDE);
    let p = kition_slide_pptx::import(&bytes).expect("import");
    assert_eq!(anchor_of(&p, "Plain"), Some(Anchor::Top));
    assert_eq!(anchor_of(&p, "Centred"), Some(Anchor::Middle));
    // A placeholder keeps inheriting its anchor from the layout.
    assert_eq!(anchor_of(&p, "Title"), None);
    // Written back explicitly, the anchor survives a round trip.
    let again = kition_slide_pptx::import(&kition_slide_pptx::export(&p).expect("export")).expect("import");
    assert_eq!(anchor_of(&again, "Plain"), Some(Anchor::Top));
}

#[test]
fn kitions_middle_default_is_written_out() {
    // A shape Kition made shows its text middle-anchored with no anchor of its own; the file
    // says so, so other readers (and Kition, reading it back) show it there too.
    let mut p = defaults::new_slide_file(None);
    let slide = std::sync::Arc::make_mut(&mut p.slides[0]);
    slide.shapes.push(kition_slide_model::Shape {
        id: kition_slide_model::ShapeId(900),
        name: "Made here".into(),
        xfrm: Some(kition_slide_geom::Xfrm::new(10.0, 10.0, 200.0, 100.0)),
        text: Some(kition_slide_model::text::TextBody::from_text("Centred")),
        ..Default::default()
    });
    assert_eq!(anchor_of(&p, "Made here"), None);
    let back = kition_slide_pptx::import(&kition_slide_pptx::export(&p).expect("export")).expect("import");
    assert_eq!(anchor_of(&back, "Made here"), Some(Anchor::Middle));
}
