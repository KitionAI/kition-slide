//! Explicit effect-list presence survives import, inheritance, and regenerated PPTX output.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::{RunProps, TextBody};
use kition_slide_model::{Shape, ShapeId, SlideFile};

const BODY: &str = r#"<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:defRPr>
<a:effectLst><a:glow rad="38100"><a:srgbClr val="336699"/></a:glow><a:outerShdw dist="25400"><a:srgbClr val="000000"/></a:outerShdw></a:effectLst>
</a:defRPr></a:lvl1pPr></a:lstStyle><a:p>
<a:r><a:rPr><a:effectLst/></a:rPr><a:t>Empty</a:t></a:r>
<a:r><a:rPr/><a:t>Inherited</a:t></a:r>
<a:r><a:rPr><a:effectLst><a:glow rad="12700"><a:srgbClr val="CC3300"/></a:glow></a:effectLst></a:rPr><a:t>Glow</a:t></a:r>
</a:p></p:txBody>"#;

fn xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn authored() -> SlideFile {
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![Shape { id: ShapeId(20), text: Some(TextBody::from_text("Seed")), ..Default::default() }];
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if entry.name() == "ppt/slides/slide1.xml" {
            let mut text = String::from_utf8(data).unwrap();
            let start = text.find("<p:txBody>").unwrap();
            let end = text.find("</p:txBody>").unwrap() + "</p:txBody>".len();
            text.replace_range(start..end, BODY);
            data = text.into_bytes();
        }
        writer.start_file(entry.name(), zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(&data).unwrap();
    }
    kition_slide_pptx::import(&writer.finish().unwrap().into_inner()).unwrap()
}

fn resolved(p: &SlideFile, index: usize) -> RunProps {
    let slide = &p.slides[0];
    let shape = &slide.shapes[0];
    let para = &shape.text.as_ref().unwrap().paragraphs[0];
    resolve::run(&Ctx::for_slide(p, slide).unwrap(), shape, para, &para.runs[index].props)
}

fn check(p: &SlideFile) {
    let body = p.slides[0].shapes[0].text.as_ref().unwrap();
    let runs = &body.paragraphs[0].runs;
    assert!(runs[0].props.effects_explicit);
    assert!(!runs[1].props.effects_explicit);
    assert!(runs[2].props.effects_explicit);
    let empty = resolved(p, 0);
    assert!(empty.shadow.is_none() && empty.glow.is_none());
    let inherited = resolved(p, 1);
    assert_eq!(inherited.shadow.unwrap().dist, 2.0);
    assert_eq!(inherited.glow.unwrap().radius, 3.0);
    let glow = resolved(p, 2);
    assert!(glow.shadow.is_none());
    assert_eq!(glow.glow.unwrap().radius, 1.0);
}

#[test]
fn authored_empty_and_glow_only_lists_replace_inherited_effects() {
    check(&authored());
}

#[test]
fn regenerated_empty_effect_list_survives_export_and_reimport() {
    let mut p = authored();
    Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs[0].text = "Edited".into();
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let written = xml(&bytes);
    assert!(written.contains("<a:effectLst></a:effectLst>"));
    assert!(written.contains("Edited"));
    check(&kition_slide_pptx::import(&bytes).unwrap());
}
