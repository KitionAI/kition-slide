//! Reflection metadata and run effect-list replacement survive regenerated PPTX output.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::style::Reflection;
use kition_slide_model::text::{RunProps, TextBody};
use kition_slide_model::{Shape, ShapeId, SlideFile};

const BODY: &str = r#"<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:defRPr>
<a:effectLst><a:reflection blurRad="12700" stA="70000" stPos="15000" endA="10000" endPos="85000" dist="25400" dir="2700000" fadeDir="6000000" sx="75000" sy="-60000" kx="-720000" ky="480000" algn="tr" rotWithShape="0"/></a:effectLst>
</a:defRPr></a:lvl1pPr></a:lstStyle><a:p>
<a:r><a:rPr/><a:t>Inherited</a:t></a:r>
<a:r><a:rPr><a:effectLst/></a:rPr><a:t>Empty</a:t></a:r>
<a:r><a:rPr><a:effectLst><a:glow rad="12700"><a:srgbClr val="CC3300"/></a:glow></a:effectLst></a:rPr><a:t>Glow</a:t></a:r>
<a:r><a:rPr><a:effectLst><a:reflection/></a:effectLst></a:rPr><a:t>Default</a:t></a:r>
</a:p></p:txBody>"#;

fn explicit_reflection() -> Reflection {
    Reflection {
        blur: 1.0,
        start_alpha: 0.7,
        start_pos: 0.15,
        end_alpha: 0.1,
        end_pos: 0.85,
        dist: 2.0,
        dir: 45.0,
        fade_dir: 100.0,
        sx: 0.75,
        sy: -0.6,
        kx: -12.0,
        ky: 8.0,
        align: "tr".into(),
        rotate_with_shape: false,
    }
}

fn xml_default_reflection() -> Reflection {
    Reflection { sy: 1.0, dir: 0.0, align: "b".into(), rotate_with_shape: true, ..Default::default() }
}

fn xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn authored(body: &str) -> SlideFile {
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
            text.replace_range(start..end, body);
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
    let runs = &p.slides[0].shapes[0].text.as_ref().unwrap().paragraphs[0].runs;
    assert!(!runs[0].props.effects_explicit);
    assert!(runs[0].props.reflection.is_none());
    assert_eq!(resolved(p, 0).reflection, Some(explicit_reflection()));
    let empty = resolved(p, 1);
    assert!(empty.effects_explicit);
    assert!(empty.reflection.is_none() && empty.glow.is_none() && empty.shadow.is_none());
    let glow = resolved(p, 2);
    assert!(glow.reflection.is_none() && glow.shadow.is_none());
    assert_eq!(glow.glow.unwrap().radius, 1.0);
    assert_eq!(resolved(p, 3).reflection, Some(xml_default_reflection()));
}

#[test]
fn authored_reflection_inheritance_and_xml_defaults() {
    check(&authored(BODY));
}

#[test]
fn reflection_properties_survive_two_regenerated_generations() {
    let mut p = authored(BODY);
    for generation in 1..=2 {
        let edit = format!("Edited {generation}");
        Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs[0].text = edit.clone();
        let bytes = kition_slide_pptx::export(&p).unwrap();
        let written = xml(&bytes);
        assert!(written.contains(&edit));
        assert!(written.contains("stPos=\"15000\""));
        assert!(written.contains("fadeDir=\"6000000\""));
        assert!(written.contains("kx=\"-720000\""));
        assert!(written.contains("ky=\"480000\""));
        assert!(written.contains("sy=\"-60000\""));
        p = kition_slide_pptx::import(&bytes).unwrap();
        check(&p);
    }
}

#[test]
fn native_reflection_without_presence_marker_is_exported() {
    let mut p = SlideFile::default();
    let mut body = TextBody::from_text("Native reflection");
    body.paragraphs[0].runs[0].props.reflection = Some(explicit_reflection());
    Arc::make_mut(&mut p.slides[0]).shapes = vec![Shape { id: ShapeId(20), text: Some(body), ..Default::default() }];
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let imported = kition_slide_pptx::import(&bytes).unwrap();
    assert_eq!(resolved(&imported, 0).reflection, Some(explicit_reflection()));
}

#[test]
fn zero_reflection_scale_is_preserved() {
    let body = BODY.replace("sx=\"75000\" sy=\"-60000\"", "sx=\"0\" sy=\"0\"");
    let mut p = authored(&body);
    Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs[0].text = "Edited zero scale".into();
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let imported = kition_slide_pptx::import(&bytes).unwrap();
    let reflection = resolved(&imported, 0).reflection.unwrap();
    assert_eq!((reflection.sx, reflection.sy), (0.0, 0.0));
}
