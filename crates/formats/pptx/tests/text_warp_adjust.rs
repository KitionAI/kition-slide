//! Authored warp adjustments survive inherited bodies and regenerated XML.

use std::io::{Cursor, Read};
use std::sync::Arc;

use kition_slide_model::text::{BodyProps, TextBody};
use kition_slide_model::{Shape, ShapeId, SlideFile, Xfrm};

#[test]
fn text_edits_and_regenerated_export_keep_named_warp_adjustments() {
    let mut body = TextBody::from_text("Arc label");
    body.body.warp = Some("textArchUp".into());
    body.body.warp_adjust = vec![("adj".into(), 13_500_000.0)];
    let shape = Shape { id: ShapeId(410), xfrm: Some(Xfrm::new(20.0, 30.0, 200.0, 60.0)), text: Some(body), text_box: true, ..Default::default() };
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![shape];
    let mut p = kition_slide_pptx::import(&kition_slide_pptx::export(&p).unwrap()).unwrap();
    let text = Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap();
    assert_eq!(text.body.warp_adjust, vec![("adj".into(), 13_500_000.0)]);
    text.paragraphs[0].runs[0].text = "Edited arc label".into();
    text.body.warp_adjust[0].1 = 12_000_000.0;
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains("Edited arc label"));
    assert!(xml.contains("val 12000000"));
    assert!(!xml.contains("val 13500000"));
    let back = kition_slide_pptx::import(&bytes).unwrap();
    let body = &back.slides[0].shapes[0].text.as_ref().unwrap().body;
    assert_eq!(body.warp.as_deref(), Some("textArchUp"));
    assert_eq!(body.warp_adjust, vec![("adj".into(), 12_000_000.0)]);
}

#[test]
fn local_warp_and_explicit_empty_adjustments_block_parent_adjustments() {
    let parent = BodyProps { warp: Some("textArchUp".into()), warp_adjust: vec![("adj".into(), 12_000_000.0)], ..Default::default() };
    let mut inherited = BodyProps::default();
    inherited.inherit(&parent);
    assert_eq!(inherited.warp, parent.warp);
    assert_eq!(inherited.warp_adjust, parent.warp_adjust);
    for preset in ["textArchDown", "textPlain", "textNoShape"] {
        let mut local = BodyProps { warp: Some(preset.into()), ..Default::default() };
        local.inherit(&parent);
        assert_eq!(local.warp.as_deref(), Some(preset));
        assert!(local.warp_adjust.is_empty());
    }
    let old: BodyProps = serde_json::from_str(r#"{"warp":"textArchUp"}"#).unwrap();
    assert!(old.warp_adjust.is_empty());
    let restored: BodyProps = serde_json::from_slice(&serde_json::to_vec(&parent).unwrap()).unwrap();
    assert_eq!(restored.warp_adjust, parent.warp_adjust);
}

#[test]
fn explicit_no_shape_import_and_regeneration_block_inherited_arc() {
    let mut body = TextBody::from_text("Ordinary local text");
    body.body.warp = Some("textNoShape".into());
    let shape = Shape { id: ShapeId(411), text: Some(body), text_box: true, ..Default::default() };
    let mut p = SlideFile::default();
    Arc::make_mut(&mut p.slides[0]).shapes = vec![shape];
    let bytes = kition_slide_pptx::export(&p).unwrap();
    let mut imported = kition_slide_pptx::import(&bytes).unwrap();
    let local = Arc::make_mut(&mut imported.slides[0]).shapes[0].text.as_mut().unwrap();
    assert_eq!(local.body.warp.as_deref(), Some("textNoShape"));
    let parent = BodyProps { warp: Some("textArchUp".into()), warp_adjust: vec![("adj".into(), 12_000_000.0)], ..Default::default() };
    local.body.inherit(&parent);
    assert_eq!(local.body.warp.as_deref(), Some("textNoShape"));
    assert!(local.body.warp_adjust.is_empty());
    local.paragraphs[0].runs[0].text = "Edited ordinary local text".into();
    let bytes = kition_slide_pptx::export(&imported).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains("Edited ordinary local text"));
    assert!(xml.contains("prst=\"textNoShape\""));
    let restored = kition_slide_pptx::import(&bytes).unwrap();
    let body = &restored.slides[0].shapes[0].text.as_ref().unwrap().body;
    assert_eq!(body.warp.as_deref(), Some("textNoShape"));
    assert!(body.warp_adjust.is_empty());
}
