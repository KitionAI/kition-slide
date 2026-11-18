//! Kept text-body 3-D elements survive inheritance, editing and regenerated PPTX output.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::BodyProps;
use kition_slide_model::{Layout, PhType, SlideFile};

const SCENE: &str = r#"<a:scene3d><a:camera prst="obliqueBottomRight" fov="5400000" zoom="125000"><a:rot lat="600000" lon="1200000" rev="1800000"/></a:camera><a:lightRig rig="flat" dir="t"/><a:backdrop><a:anchor x="0" y="0" z="12700"/><a:norm dx="0" dy="0" dz="1"/><a:up dx="0" dy="1" dz="0"/></a:backdrop><a:extLst><a:ext uri="urn:example:scene"><keep:flag keep:value="A &amp; B"/></a:ext></a:extLst></a:scene3d>"#;
const DEPTH: &str = r#"<a:sp3d z="-6350" extrusionH="254000" contourW="6350" prstMaterial="matte"><a:bevelT w="12700" h="25400" prst="angle"/><a:bevelB w="6350" h="12700"/><a:extrusionClr><a:schemeClr val="accent1"><a:lumMod val="20000"/><a:lumOff val="80000"/></a:schemeClr></a:extrusionClr><a:contourClr><a:srgbClr val="5645D4"/></a:contourClr><a:extLst><a:ext uri="urn:example:depth"><keep:depth value="0"/></a:ext></a:extLst></a:sp3d>"#;
const FRONT: &str = r#"<a:scene3d><a:camera prst="orthographicFront"/><a:lightRig rig="threePt" dir="t"/></a:scene3d>"#;

fn first_body_xml(xml: &str) -> &str {
    let start = xml.find("<a:bodyPr").unwrap();
    let open_end = start + xml[start..].find('>').unwrap() + 1;
    let end =
        if xml[start..open_end].ends_with("/>") { open_end } else { open_end + xml[open_end..].find("</a:bodyPr>").unwrap() + "</a:bodyPr>".len() };
    &xml[start..end]
}

fn replace_body(mut xml: String, body: &str) -> String {
    let old = first_body_xml(&xml).to_owned();
    xml = xml.replacen(&old, body, 1);
    // The kept extension prefix is declared only on the part root, not on its 3-D subtree.
    let start = xml.find("<p:").unwrap();
    let end = start + xml[start..].find(' ').unwrap();
    xml.insert_str(end, r#" xmlns:keep="urn:example:kept-3d""#);
    xml
}

fn authored(slide_elements: &str) -> SlideFile {
    let bytes = kition_slide_pptx::export(&SlideFile::default()).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        let body = match name.as_str() {
            "ppt/slides/slide1.xml" => Some(format!("<a:bodyPr>{slide_elements}</a:bodyPr>")),
            "ppt/slideLayouts/slideLayout1.xml" => {
                Some(format!(r#"<a:bodyPr><a:prstTxWarp prst="textArchUp"><a:avLst/></a:prstTxWarp><a:noAutofit/>{SCENE}{DEPTH}</a:bodyPr>"#))
            }
            _ => None,
        };
        if let Some(body) = body {
            data = replace_body(String::from_utf8(data).unwrap(), &body).into_bytes();
        }
        writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(&data).unwrap();
    }
    kition_slide_pptx::import(&writer.finish().unwrap().into_inner()).unwrap()
}

fn title_body(p: &SlideFile) -> &BodyProps {
    &p.slides[0].shapes[0].text.as_ref().unwrap().body
}

fn layout(p: &SlideFile) -> &Layout {
    p.master_for(&p.slides[0]).unwrap().1.unwrap()
}

fn effective(p: &SlideFile) -> BodyProps {
    let slide = &p.slides[0];
    resolve::body(&Ctx::for_slide(p, slide).unwrap(), &slide.shapes[0])
}

fn part_xml(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn check_inherited(p: &SlideFile) {
    assert!(title_body(p).scene3d_xml.is_none());
    assert!(title_body(p).text3d_xml.is_none());
    let body = effective(p);
    let scene = body.scene3d_xml.unwrap();
    assert!(scene.contains("obliqueBottomRight"));
    assert!(scene.contains("<a:backdrop>"));
    assert!(scene.contains(r#"xmlns:keep="urn:example:kept-3d""#));
    assert!(scene.contains(r#"keep:value="A &amp; B""#));
    let depth = body.text3d_xml.unwrap();
    assert!(depth.contains(r#"extrusionH="254000""#));
    assert!(depth.contains(r#"z="-6350""#));
    assert!(depth.contains(r#"<a:lumMod val="20000""#));
    assert!(depth.contains("<a:bevelB"));
    assert!(depth.contains(r#"xmlns:keep="urn:example:kept-3d""#));
}

#[test]
fn layout_text_scene_and_depth_inherit_as_separate_whole_elements() {
    let mut p = authored("");
    check_inherited(&p);
    let body = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().body;
    body.scene3d_xml = Some(FRONT.into());
    let effective = effective(&p);
    assert_eq!(effective.scene3d_xml.as_deref(), Some(FRONT));
    assert!(effective.text3d_xml.unwrap().contains("extrusionH"));
}

#[test]
fn flat_text_and_empty_depth_block_inherited_extrusion() {
    for explicit in [r#"<a:flatTx z="0"/>"#, "<a:sp3d/>"] {
        let mut p = authored(explicit);
        let before = title_body(&p).text3d_xml.clone();
        let body = effective(&p);
        assert_eq!(body.text3d_xml, before);
        assert!(!body.text3d_xml.unwrap().contains("extrusionH"));
        assert!(body.scene3d_xml.unwrap().contains("obliqueBottomRight"));
        Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs = vec![kition_slide_model::Run::new("Edited override")];
        let bytes = kition_slide_pptx::export(&p).unwrap();
        let slide_xml = part_xml(&bytes, "ppt/slides/slide1.xml");
        assert!(slide_xml.contains("Edited override"));
        let body_xml = first_body_xml(&slide_xml);
        assert!(!body_xml.contains("extrusionH"));
        if explicit.contains("flatTx") {
            assert!(body_xml.contains("<a:flatTx"));
            assert!(body_xml.contains(r#"z="0""#));
            assert!(!body_xml.contains("<a:sp3d"));
        }
        let back = kition_slide_pptx::import(&bytes).unwrap();
        assert_eq!(effective(&back).text3d_xml, before);
    }
}

#[test]
fn empty_scene_blocks_layout_scene_but_still_inherits_depth() {
    let p = authored("<a:scene3d/>");
    let scene = title_body(&p).scene3d_xml.clone();
    let body = effective(&p);
    assert_eq!(body.scene3d_xml, scene);
    assert!(!body.scene3d_xml.unwrap().contains("camera"));
    assert!(body.text3d_xml.unwrap().contains("extrusionH"));
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let back = kition_slide_pptx::import(&bytes).unwrap();
    assert_eq!(effective(&back).scene3d_xml, scene);
}

#[test]
fn master_scene_is_used_when_layout_and_slide_omit_it() {
    let mut p = authored("");
    let layout_id = p.slides[0].layout;
    let master = Arc::make_mut(&mut p.masters[0]);
    let layout = master.layouts.iter_mut().find(|layout| layout.id == layout_id).unwrap();
    layout.shapes[0].text.as_mut().unwrap().body.scene3d_xml = None;
    master.shapes.iter_mut().find(|shape| shape.ph_type() == Some(PhType::Title)).unwrap().text.as_mut().unwrap().body.scene3d_xml =
        Some(FRONT.into());
    let body = effective(&p);
    assert_eq!(body.scene3d_xml.as_deref(), Some(FRONT));
    assert!(body.text3d_xml.unwrap().contains("extrusionH"));
}

#[test]
fn text_edits_force_regeneration_without_flattening_or_losing_kept_3d_namespaces() {
    let mut p = authored("");
    for generation in 1..=2 {
        let edit = format!("Edited title {generation}");
        Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().paragraphs[0].runs = vec![kition_slide_model::Run::new(&edit)];
        let bytes = kition_slide_pptx::export(&p).unwrap();
        let slide_xml = part_xml(&bytes, "ppt/slides/slide1.xml");
        assert!(slide_xml.contains(&edit));
        assert!(!first_body_xml(&slide_xml).contains("scene3d"));
        let layout_xml = part_xml(&bytes, "ppt/slideLayouts/slideLayout1.xml");
        let body_xml = first_body_xml(&layout_xml);
        let positions: Vec<_> = ["<a:prstTxWarp", "<a:noAutofit", "<a:scene3d", "<a:sp3d"].iter().map(|tag| body_xml.find(tag).unwrap()).collect();
        assert!(positions.windows(2).all(|positions| positions[0] < positions[1]));
        assert!(body_xml.contains("keep:flag") && body_xml.contains("keep:depth"));
        p = kition_slide_pptx::import(&bytes).unwrap();
        check_inherited(&p);
        assert!(layout(&p).shapes[0].text.as_ref().unwrap().body.scene3d_xml.is_some());
    }
}
