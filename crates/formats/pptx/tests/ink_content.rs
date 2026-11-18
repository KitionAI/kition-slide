//! Synthetic InkML content parts exercise editable paths and atomic picture fallbacks.

use std::io::{Cursor, Read, Write};

use kition_slide_geom::preset::FillMode;
use kition_slide_model::style::{Fill, LineCap, LineJoin};
use kition_slide_model::{Geom, ShapeKind, SlideFile, defaults};

const INK: &str = r##"<i:ink xmlns:i="http://www.w3.org/2003/InkML"><i:definitions>
<i:context xml:id="context"><i:inkSource><i:traceFormat><i:channel name="X" type="decimal" units="cm"/><i:channel name="Y" type="decimal" units="cm"/></i:traceFormat></i:inkSource></i:context>
<i:brush xml:id="pen"><i:brushProperty name="width" value="0.127" units="cm"/><i:brushProperty name="height" value="0.127" units="cm"/><i:brushProperty name="color" value="#2050a0"/></i:brush>
</i:definitions><i:trace contextRef="#context" brushRef="#pen">10 20,'5'5,&quot;0&quot;0</i:trace></i:ink>"##;

const CONTENT: &str = r#"<p14:contentPart r:id="ink"><p14:nvContentPartPr><p14:cNvPr id="2" name="Pen" descr="An editable stroke"/><p14:cNvContentPartPr/><p:nvPr/></p14:nvContentPartPr><p14:xfrm rot="1800000" flipH="1"><a:off x="127000" y="254000"/><a:ext cx="1270000" cy="635000"/></p14:xfrm></p14:contentPart>"#;
const FALLBACK: &str = r#"<p:pic><p:nvPicPr><p:cNvPr id="2" name="Fallback"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="image"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr></p:pic><p:sp><p:nvSpPr><p:cNvPr id="3" name="Fallback annotation"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/></p:sp>"#;

fn package(ink: &str, content: &str, requires: &str, external: bool) -> Vec<u8> {
    let original = kition_slide_pptx::export(&defaults::new_slide_file(None)).expect("export");
    let mut input = zip::ZipArchive::new(Cursor::new(original)).expect("zip");
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let slide = format!(
        r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><p:cSld><p:spTree><mc:AlternateContent><mc:Choice Requires="{requires}">{content}</mc:Choice><mc:Fallback>{FALLBACK}</mc:Fallback></mc:AlternateContent></p:spTree></p:cSld></p:sld>"#
    );
    for i in 0..input.len() {
        let mut entry = input.by_index(i).expect("entry");
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read");
        if name == "ppt/slides/slide1.xml" {
            bytes = slide.as_bytes().to_vec();
        } else if name == "ppt/slides/_rels/slide1.xml.rels" {
            let text = String::from_utf8(bytes).expect("xml");
            let mode = if external { " TargetMode=\"External\"" } else { "" };
            let relationships = format!(
                r#"<Relationship Id="ink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../ink/pen.xml"{mode}/><Relationship Id="image" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/fallback.png"/>"#
            );
            bytes = text.replace("</Relationships>", &(relationships + "</Relationships>")).into_bytes();
        }
        output.start_file(name, zip::write::SimpleFileOptions::default()).expect("start");
        output.write_all(&bytes).expect("write");
    }
    output.start_file("ppt/ink/pen.xml", zip::write::SimpleFileOptions::default()).expect("ink part");
    output.write_all(ink.as_bytes()).expect("ink");
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 50, 80, 255])).write_to(&mut png, image::ImageFormat::Png).expect("png");
    output.start_file("ppt/media/fallback.png", zip::write::SimpleFileOptions::default()).expect("image part");
    output.write_all(&png.into_inner()).expect("image");
    output.finish().expect("finish").into_inner()
}

fn import(ink: &str) -> SlideFile {
    kition_slide_pptx::import(&package(ink, CONTENT, "p14", false)).expect("import")
}

fn assert_fallback(p: &SlideFile) {
    let shapes = &p.slides[0].shapes;
    assert_eq!(shapes.len(), 2);
    assert_eq!(shapes[0].id.0, 2, "failed choice must not consume the fallback ID");
    assert_eq!(shapes[0].name, "Fallback");
    assert!(matches!(shapes[0].kind, ShapeKind::Picture { .. }));
    assert_eq!(shapes[1].name, "Fallback annotation");
}

#[test]
fn inkml_round_pen_imports_an_editable_path_with_physical_width() {
    let p = import(INK);
    let shape = &p.slides[0].shapes[0];
    assert_eq!(p.slides[0].shapes.len(), 1);
    assert_eq!(shape.name, "Pen");
    assert_eq!(shape.descr, "An editable stroke");
    let x = shape.xfrm.expect("frame");
    assert_eq!((x.x, x.y, x.w, x.h, x.rot, x.flip_h), (10.0, 20.0, 100.0, 50.0, 30.0, true));
    assert_eq!(shape.fill, Some(Fill::None));
    let line = shape.line.as_ref().expect("line");
    assert!((line.width.expect("width") - 3.6).abs() < 1e-10);
    assert_eq!(line.cap, Some(LineCap::Round));
    assert_eq!(line.join, Some(LineJoin::Round));
    let Geom::Custom { paths, .. } = &shape.geom else { panic!("editable path") };
    assert_eq!((paths[0].w, paths[0].h, paths[0].fill, paths[0].stroke), (100.0, 50.0, FillMode::None, true));
    assert_eq!(paths[0].d, "M 0,0 L 50,25 L 100,50");
    // Regeneration preserves editable geometry rather than returning to the picture.
    let again = kition_slide_pptx::import(&kition_slide_pptx::export(&p).expect("export")).expect("reimport");
    let geometry = kition_slide_engine::render::geom_geometry;
    assert_eq!(geometry(&again.slides[0].shapes[0].geom, x.w, x.h).paths[0].path, geometry(&shape.geom, x.w, x.h).paths[0].path);
    assert_eq!(again.slides[0].shapes[0].line, shape.line);
}

#[test]
fn unsupported_brushes_channels_and_trace_forms_keep_every_fallback_shape() {
    for ink in [
        INK.replace("</i:brush>", r#"<i:brushProperty name="tip" value="rectangle"/></i:brush>"#),
        INK.replace("</i:brush>", r#"<i:brushProperty name="rasterOp" value="maskPen"/></i:brush>"#),
        INK.replace("</i:traceFormat>", r#"<i:channel name="F" units="N"/></i:traceFormat>"#),
        INK.replace("10 20,'5'5,&quot;0&quot;0", "10 20,broken"),
        INK.replace("#pen", "#missing"),
        INK.replace("0.127", "NaN"),
        INK.replace("http://www.w3.org/2003/InkML", "urn:unsupported"),
        INK.replace("</i:ink>", "<i:trace>0 0</i:trace></i:ink>"),
    ] {
        assert_fallback(&import(&ink));
    }
}

#[test]
fn unsupported_context_namespace_and_frame_data_keep_the_picture() {
    for ink in [
        INK.replace("name=\"X\"", "name=\"X\" orientation=\"-ve\""),
        INK.replace("<i:context ", "<i:context canvasTransformRef=\"#transform\" "),
        INK.replace("<i:context ", "<i:context traceFormatRef=\"#format\" "),
        INK.replace("<i:context ", "<i:context canvasRef=\"#canvas\" "),
        INK.replace("<i:trace ", "<i:trace type=\"penUp\" "),
        INK.replace("<i:trace ", "<i:trace xmlns:i=\"urn:foreign\" "),
        INK.replace("<i:brushProperty ", "<i:brushProperty xmlns:i=\"urn:foreign\" "),
        INK.replace("xml:id=\"pen\"", "xmlns:other=\"urn:foreign\" other:id=\"pen\""),
    ] {
        assert_fallback(&import(&ink));
    }
    for content in [
        CONTENT.replace("cx=\"1270000\"", "cx=\"NaN\""),
        CONTENT.replace("cx=\"1270000\"", "cx=\"-1\""),
        CONTENT.replace("x=\"127000\"", "x=\"1e30\""),
        CONTENT.replace("<a:ext cx=\"1270000\" cy=\"635000\"/>", ""),
    ] {
        assert_fallback(&kition_slide_pptx::import(&package(INK, &content, "p14", false)).expect("import"));
    }
}

#[test]
fn unknown_mc_requirement_external_link_and_mixed_choice_fall_back_atomically() {
    for (requires, external, content) in
        [("future", false, CONTENT.to_string()), ("p14", true, CONTENT.to_string()), ("p14", false, format!("{CONTENT}{FALLBACK}"))]
    {
        let p = kition_slide_pptx::import(&package(INK, &content, requires, external)).expect("import");
        assert_fallback(&p);
    }
}

#[test]
fn changing_object_extent_does_not_scale_the_pen_width() {
    let p = import(INK);
    let enlarged = CONTENT.replace("1270000", "2540000").replace("635000", "1270000");
    let larger = kition_slide_pptx::import(&package(INK, &enlarged, "p14", false)).expect("import");
    assert_eq!(p.slides[0].shapes[0].line, larger.slides[0].shapes[0].line);
    assert_eq!(larger.slides[0].shapes[0].xfrm.expect("frame").w, 200.0);
}

#[test]
fn round_pen_has_visible_physical_ink() {
    use kition_slide_engine::render::{RenderOpts, render_slide};
    let content = CONTENT.replace("rot=\"1800000\" flipH=\"1\"", "");
    let is_blue = |p: &[u8; 4]| u16::from(p[2]) > u16::from(p[0]) + 40;
    for trace in ["0 0,10 0", "0 0,5 0,10 0"] {
        let ink = INK.replace("10 20,'5'5,&quot;0&quot;0", trace);
        let p = kition_slide_pptx::import(&package(&ink, &content, "p14", false)).expect("import");
        let image = render_slide(&p, 0, &RenderOpts { scale: 2.0, edit: false, ..Default::default() });
        let pixels = image.pixels.as_chunks::<4>().0;
        let center = 90 * image.width as usize + 120;
        assert!(is_blue(&pixels[center]), "center of {trace}");
        assert!(!is_blue(&pixels[center + 5 * image.width as usize]), "physical width bounds");
        let rows = (80..100).filter(|y| is_blue(&pixels[y * image.width as usize + 120])).count();
        assert!((6..=8).contains(&rows), "3.6pt ink at 2x occupies about seven pixels, got {rows}");
        assert!(is_blue(&pixels[90 * image.width as usize + 18]), "round cap extends before the path endpoint");
    }
}

#[test]
fn stationary_ink_is_an_editable_visible_dot_after_export() {
    use kition_slide_engine::render::{RenderOpts, geom_geometry, render_slide};
    for trace in ["5 5", "5 5,5 5", "5 5,'0'0,**"] {
        let ink = INK.replace("10 20,'5'5,&quot;0&quot;0", trace);
        for (width, height) in [(100.0, 50.0), (0.0, 50.0), (100.0, 0.0), (0.0, 0.0)] {
            let content = CONTENT
                .replace("cx=\"1270000\"", &format!("cx=\"{}\"", width * 12700.0))
                .replace("cy=\"635000\"", &format!("cy=\"{}\"", height * 12700.0));
            let mut presentation = kition_slide_pptx::import(&package(&ink, &content, "p14", false)).expect("import");
            presentation.slide_size = kition_slide_geom::Size::new(128.0, 128.0);
            assert_eq!(presentation.slides[0].shapes.len(), 1, "the editable choice replaces the complete fallback");
            let shape = &presentation.slides[0].shapes[0];
            assert_eq!(shape.name, "Pen");
            assert!(matches!(shape.geom, Geom::Custom { .. }));
            assert!(!matches!(shape.kind, ShapeKind::Picture { .. }));
            let frame = shape.xfrm.expect("frame");
            let path = &geom_geometry(&shape.geom, frame.w, frame.h).paths[0].path;
            let point = kition_slide_geom::Point::new(width / 2.0, height / 2.0);
            assert_eq!(path.elements()[0], kition_slide_geom::PathEl::MoveTo(point));
            assert!(path.elements()[1..].iter().all(|el| *el == kition_slide_geom::PathEl::LineTo(point)));
            let options = RenderOpts { scale: 2.0, edit: false, ..Default::default() };
            let image = render_slide(&presentation, 0, &options);
            let (x, y) = ((10.0 + width / 2.0) as u32 * 2, (20.0 + height / 2.0) as u32 * 2);
            assert_eq!(image.pixel(x, y), [32, 80, 160, 255], "physical center of {trace} in {width}x{height}");
            assert_eq!(image.pixel(x + 5, y), [255; 4], "round dot keeps its 3.6pt physical diameter");
            assert_eq!(image.pixel(x + 3, y + 3), [255; 4], "round cap does not become a square");
            let again = kition_slide_pptx::import(&kition_slide_pptx::export(&presentation).expect("export")).expect("reimport");
            assert_eq!(again.slides[0].shapes[0].line, shape.line);
            assert_eq!(render_slide(&again, 0, &options).pixels, image.pixels, "the editable dot survives export");
        }
    }
}

#[test]
fn duplicate_singleton_structures_preserve_the_complete_fallback() {
    for ink in [
        INK.replace("</i:ink>", "<i:definitions/></i:ink>"),
        INK.replace("</i:context>", "<i:inkSource/></i:context>"),
        INK.replace("</i:inkSource>", "<i:traceFormat/></i:inkSource>"),
    ] {
        assert_fallback(&import(&ink));
    }
    for content in [
        CONTENT.replace("</p14:contentPart>", "<p14:xfrm/></p14:contentPart>"),
        CONTENT.replace("</p14:xfrm>", "<a:off x=\"0\" y=\"0\"/></p14:xfrm>"),
    ] {
        assert_fallback(&kition_slide_pptx::import(&package(INK, &content, "p14", false)).expect("import"));
    }
}
