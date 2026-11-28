use serde_json::json;

use crate::*;

fn session() -> Session {
    Session::with_new()
}

#[test]
fn new_slide_file_and_slides() {
    let mut s = session();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 1);
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    assert_eq!(s.doc().unwrap().selection.slide, 2);
    s.execute("slide.duplicate", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("slide.move", &json!({"from": 3, "to": 0})).unwrap();
    s.execute("slide.delete", &json!({"index": 0})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
}

#[test]
fn insert_shape_and_format() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "roundRect", "rect": [100, 100, 200, 100], "text": "Hi"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"color": "#ff0000"})).unwrap();
    s.execute("shape.line", &json!({"width": 3, "color": "accent2", "dash": "dash"})).unwrap();
    s.execute("shape.rotate", &json!({"deg": 30})).unwrap();
    s.execute("shape.move", &json!({"dx": 10, "dy": -5})).unwrap();
    s.execute("format.bold", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 110.0);
    assert_eq!(v["box"]["rot"], 30.0);
    let runs = &v["shape"]["text"]["paragraphs"][0]["runs"][0]["props"];
    assert_eq!(runs["bold"], true);
    // Undo restores step by step.
    for _ in 0..5 {
        s.execute("edit.undo", &json!({})).unwrap();
    }
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert!(v["shape"]["fill"].is_null());
}

#[test]
fn text_editing_flow() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    s.execute("format.italic", &json!({})).unwrap();
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    s.execute("text.delete", &json!({"dir": "backward"})).unwrap();
    s.execute("text.insert", &json!({"text": "D\nSecond"})).unwrap();
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "Hello worlD\nSecond");
    s.execute("text.exit", &json!({})).unwrap();
    assert!(s.doc().unwrap().selection.text.is_none());
    assert_eq!(s.doc().unwrap().selection.shapes.len(), 1);
}

#[test]
fn imported_autofit_survives_roundtrip_and_reflows_after_edits() {
    use model::text::AutoFit;
    let mut original = session();
    let id =
        original.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 240, 35], "text": "First line\nSecond line"})).unwrap()["id"]
            .as_u64()
            .unwrap();
    // A file can store a full-size result even when local font metrics overflow it.
    let mut source = (*original.doc().unwrap().doc).clone();
    let shape = model::find_shape_mut(&mut std::sync::Arc::make_mut(&mut source.slides[0]).shapes, ShapeId(id as u32)).unwrap();
    shape.text.as_mut().unwrap().body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    let repeated = shape.clone();
    // PPTX ids are part-local: an edit must not refit matching ids in another owner.
    let mut other = (*source.slides[0]).clone();
    other.id = SlideId(source.next_id);
    source.next_id += 1;
    source.slides.push(std::sync::Arc::new(other));
    let master = std::sync::Arc::make_mut(&mut source.masters[0]);
    master.shapes.push(repeated.clone());
    master.layouts[0].shapes.push(repeated);
    let bytes = kition_slide_pptx::export(&source).unwrap();
    let imported = kition_slide_pptx::import(&bytes).unwrap();
    let id = model::find_shape(&imported.slides[0].shapes, ShapeId(id as u32)).unwrap().id;
    let body = model::find_shape(&imported.slides[0].shapes, id).unwrap().text.as_ref().unwrap();
    assert_eq!(body.body.autofit_saved, Some(true));
    assert_eq!(body.body.autofit, Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 }));
    let again = kition_slide_pptx::import(&kition_slide_pptx::export(&imported).unwrap()).unwrap();
    assert_eq!(model::find_shape(&again.slides[0].shapes, id).unwrap().text.as_ref().unwrap().body, body.body);
    let mut duplicate = Session::new();
    duplicate.add_document(DocState::new(imported.clone(), None, "Saved layout copy".into()));
    duplicate.doc_mut().unwrap().selection.shapes = vec![id];
    duplicate.execute("edit.duplicate", &json!({})).unwrap();
    let copy_id = duplicate.doc().unwrap().selection.shapes[0];
    assert_ne!(copy_id, id);
    assert_eq!(duplicate.doc().unwrap().shape(copy_id).unwrap().text.as_ref().unwrap().body, body.body);

    for (command, params) in [
        ("shape.resize", json!({"w": 120})),
        ("text.set", json!({"text": "First modified line\nSecond modified line\nThird modified line"})),
        ("format.bold", json!({})),
    ] {
        let mut s = Session::new();
        s.add_document(DocState::new(imported.clone(), None, "Autofit".into()));
        s.doc_mut().unwrap().selection.shapes = vec![id];
        s.execute("shape.move", &json!({"dx": 5, "dy": 8})).unwrap();
        assert_eq!(s.doc().unwrap().shape(id).unwrap().text.as_ref().unwrap().body.autofit_saved, Some(true));
        s.execute(command, &params).unwrap();
        assert_eq!(s.doc().unwrap().doc.slides[1], imported.slides[1], "{command}: unrelated slide");
        assert_eq!(s.doc().unwrap().doc.masters, imported.masters, "{command}: unrelated master/layout");
        let edited = &s.doc().unwrap().shape(id).unwrap().text.as_ref().unwrap().body;
        assert_eq!(edited.autofit_saved, Some(true), "{command}");
        let Some(AutoFit::Shrink { font_scale, line_reduction }) = edited.autofit else { panic!("missing shrink result") };
        assert!(font_scale < 1.0, "{command}: {font_scale}");
        let reopened = kition_slide_pptx::import(&kition_slide_pptx::export(&s.doc().unwrap().doc).unwrap()).unwrap();
        let Some(AutoFit::Shrink { font_scale: saved_scale, line_reduction: saved_reduction }) =
            model::find_shape(&reopened.slides[0].shapes, id).unwrap().text.as_ref().unwrap().body.autofit
        else {
            panic!("missing saved shrink result")
        };
        assert!((saved_scale - font_scale).abs() < 0.00001);
        assert!((saved_reduction - line_reduction).abs() < 0.00001);
        s.execute("shape.resize", &json!({"w": 800, "h": 500})).unwrap();
        assert_eq!(
            s.doc().unwrap().shape(id).unwrap().text.as_ref().unwrap().body.autofit,
            Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 }),
            "{command}: grow back to full size"
        );
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        let restored = s
            .doc()
            .unwrap()
            .current_slide()
            .unwrap()
            .shapes
            .iter()
            .filter_map(|shape| shape.text.as_ref())
            .find(|text| text.text() == "First line\nSecond line")
            .unwrap();
        assert_eq!(restored.body.autofit, body.body.autofit, "{command}");
        assert_eq!(restored.body.autofit_saved, Some(true), "{command}");
    }
}

#[test]
fn authored_autofit_and_duplicate_keep_their_scale_on_reopen() {
    use model::text::AutoFit;
    fn fitted(shapes: &[Shape]) -> Vec<model::text::BodyProps> {
        let mut out = Vec::new();
        for shape in shapes {
            if let Some(text) = &shape.text
                && matches!(text.body.autofit, Some(AutoFit::Shrink { .. }))
            {
                out.push(text.body.clone());
            }
            out.extend(fitted(shape.children()));
        }
        out
    }
    let mut s = session();
    let long = "An intentionally long process label ".repeat(20);
    s.execute("smartGraphic.insert", &json!({"kind": "process", "items": [long.clone(), long.clone(), long]})).unwrap();
    let before = fitted(&s.doc().unwrap().current_slide().unwrap().shapes);
    assert_eq!(before.len(), 3);
    assert!(
        before.iter().all(|b| b.autofit_saved == Some(true) && matches!(b.autofit, Some(AutoFit::Shrink { font_scale, .. }) if font_scale < 1.0))
    );
    s.execute("edit.duplicate", &json!({})).unwrap();
    let copies = fitted(&s.doc().unwrap().current_slide().unwrap().shapes);
    assert_eq!(copies.len(), 6);
    assert_eq!(&copies[..3], &copies[3..]);
    let reopened = kition_slide_pptx::import(&kition_slide_pptx::export(&s.doc().unwrap().doc).unwrap()).unwrap();
    let saved = fitted(&reopened.slides[0].shapes);
    assert_eq!(saved.len(), copies.len());
    for (a, b) in copies.iter().zip(saved.iter()) {
        let (Some(AutoFit::Shrink { font_scale: x, .. }), Some(AutoFit::Shrink { font_scale: y, .. })) = (a.autofit, b.autofit) else {
            panic!("missing shrink result")
        };
        assert!((x - y).abs() < 0.00001);
    }
}

#[test]
fn autocorrect_recalculates_saved_autofit() {
    use model::text::AutoFit;
    let mut s = session();
    let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 45, 40], "text": "(tm)"})).unwrap()["id"].as_u64().unwrap();
    s.execute("format.fontName", &json!({"family": "Inter"})).unwrap();
    s.execute("format.fontSize", &json!({"size": 20})).unwrap();
    s.execute("format.autofit", &json!({"mode": "shrink"})).unwrap();
    let before = s.doc().unwrap().shape(ShapeId(id as u32)).unwrap().text.as_ref().unwrap().body.autofit;
    assert!(matches!(before, Some(AutoFit::Shrink { font_scale, .. }) if font_scale < 1.0));
    s.execute("text.edit", &json!({"id": id, "end": true})).unwrap();
    s.execute("text.insert", &json!({"text": " "})).unwrap();
    let text = s.doc().unwrap().shape(ShapeId(id as u32)).unwrap().text.as_ref().unwrap();
    assert_eq!(text.text(), "™ ");
    assert_eq!(text.body.autofit, Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 }));
    assert_eq!(text.body.autofit_saved, Some(true));
    let reopened = kition_slide_pptx::import(&kition_slide_pptx::export(&s.doc().unwrap().doc).unwrap()).unwrap();
    let shape = model::find_shape(&reopened.slides[0].shapes, ShapeId(id as u32)).unwrap();
    assert_eq!(shape.text.as_ref().unwrap().body.autofit, text.body.autofit);
}

#[test]
fn smart_quotes_and_autocorrect() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    for c in ["\"", "h", "i", "\"", " ", "t", "e", "h", " "] {
        s.execute("text.insert", &json!({"text": c})).unwrap();
    }
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "“hi” the ");
}

#[test]
fn clipboard_copy_paste_shapes_and_slides() {
    let mut s = session();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [10, 10, 50, 50]})).unwrap();
    s.execute("edit.copy", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    let n = s.doc().unwrap().current_slide().unwrap().shapes.len();
    assert_eq!(n, 2 + 3);
    s.execute("edit.copy", &json!({"scope": "slides"})).unwrap();
    s.execute("edit.paste", &json!({"scope": "slides"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 2);
    assert!(s.doc().unwrap().doc.validate().is_empty());
}

#[test]
fn group_ungroup_roundtrip_positions() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let g = s.execute("arrange.group", &json!({})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.move", &json!({"id": g, "dx": 10})).unwrap();
    s.execute("arrange.ungroup", &json!({"ids": [g]})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": b})).unwrap();
    assert!((v["box"]["x"].as_f64().unwrap() - 310.0).abs() < 1e-6, "{v}");
}

#[test]
fn align_and_distribute() {
    let mut s = session();
    let mut ids = vec![];
    for (x, y) in [(10.0, 10.0), (200.0, 50.0), (500.0, 90.0)] {
        ids.push(s.execute("shape.insert", &json!({"preset": "rect", "rect": [x, y, 40, 40]})).unwrap()["id"].as_u64().unwrap());
    }
    s.execute("edit.select", &json!({"ids": ids})).unwrap();
    s.execute("arrange.align", &json!({"edge": "top"})).unwrap();
    s.execute("arrange.distribute", &json!({"dir": "horizontal"})).unwrap();
    let ys: Vec<f64> = ids.iter().map(|i| s.execute("shape.inspect", &json!({"id": i})).unwrap()["box"]["y"].as_f64().unwrap()).collect();
    assert!(ys.iter().all(|y| (*y - 10.0).abs() < 1e-6));
    let x1 = s.execute("shape.inspect", &json!({"id": ids[1]})).unwrap()["box"]["x"].as_f64().unwrap();
    assert!((x1 - 255.0).abs() < 1e-6, "{x1}");
}

#[test]
fn pointer_draw_move_resize() {
    let mut s = session();
    s.set_tool(ToolKind::Shape { preset: "rect".into() });
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 100.0, 100.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 200.0, 180.0)).unwrap();
    let r = s.pointer(ev(PointerKind::Up, 200.0, 180.0)).unwrap();
    let id = r["id"].as_u64().unwrap();
    assert_eq!(s.tool.kind, ToolKind::Select);
    // Move by dragging the middle.
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 170.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 170.0, 150.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 120.0);
    assert_eq!(v["box"]["y"], 110.0);
    // One undo step per gesture.
    s.execute("edit.undo", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 100.0);
    // Resize from the SE handle.
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.pointer(ev(PointerKind::Down, 200.0, 180.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 250.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 250.0, 200.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["w"], 150.0);
}

#[test]
fn click_into_placeholder_starts_editing() {
    let mut s = session();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 480.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 480.0, 200.0)).unwrap();
    assert!(s.doc().unwrap().selection.text.is_some());
    s.execute("text.insert", &json!({"text": "Typed"})).unwrap();
    assert_eq!(s.doc().unwrap().current_slide().unwrap().title(), "Typed");
}

#[test]
fn sample_slide_file_builds_and_renders() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let st = s.doc().unwrap();
    assert!(st.doc.slides.len() >= 8);
    assert!(st.doc.validate().is_empty());
    assert!(!st.is_dirty());
    for i in 0..st.doc.slides.len() {
        let img = kition_slide_render::render_slide(&st.doc, i, &kition_slide_render::RenderOpts { scale: 0.2, ..Default::default() });
        assert_eq!(img.width, 192);
    }
    let v = s.execute("file.inspect", &json!({})).unwrap();
    assert!(v["slides"].as_array().unwrap().len() >= 8);
}

#[test]
fn export_bytes_answers_the_file_without_writing_one() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let n = s.doc().unwrap().doc.slides.len();
    let revision = s.doc().unwrap().revision;
    let decode = |v: &serde_json::Value| cmd::base64_decode(v.as_str().unwrap()).unwrap();
    let pdf = s.execute("file.exportBytes", &json!({})).unwrap();
    assert_eq!(pdf["format"], "pdf");
    assert!(decode(&pdf["data"]).starts_with(b"%PDF-"), "a PDF by default");
    let png = s.execute("file.exportBytes", &json!({"format": "png", "slide": 1, "scale": 0.25})).unwrap();
    assert!(decode(&png["data"]).starts_with(b"\x89PNG"));
    assert!(png.get("files").is_none(), "one slide, one file");
    let jpg = s.execute("file.exportBytes", &json!({"format": "jpg", "all": true, "scale": 0.1})).unwrap();
    let files = jpg["files"].as_array().unwrap();
    assert_eq!(files.len(), n, "every slide");
    assert!(files.iter().all(|file| decode(&file["data"]).starts_with(&[0xff, 0xd8])));
    assert_eq!(files[1]["slide"], 1);
    assert!(s.execute("file.exportBytes", &json!({"format": "png", "slide": 999})).is_err());
    assert!(s.execute("file.exportBytes", &json!({"format": "gif"})).is_err());
    let st = s.doc().unwrap();
    assert_eq!(st.revision, revision, "an export changes nothing");
    assert!(!st.is_dirty());
}

#[test]
fn save_and_reopen_native() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let bytes = s.execute("file.saveBytes", &json!({})).unwrap()["data"].as_str().unwrap().to_string();
    let n = s.doc().unwrap().doc.slides.len();
    s.execute("file.openBytes", &json!({"name": "x.kslide", "data": bytes})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), n);
    assert_eq!(s.documents().len(), 2);
}

#[test]
fn every_command_survives_empty_and_hostile_params() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    s.execute("edit.select", &json!({"ids": [s.doc().unwrap().current_slide().unwrap().shapes[0].id.0]})).unwrap();
    let ids: Vec<&str> = command_specs()
        .iter()
        .map(|c| c.id)
        .filter(|id| !matches!(*id, "file.close" | "file.open" | "file.save" | "file.saveAs" | "file.export" | "file.saveTemplate"))
        .collect();
    for p in [
        json!({}),
        json!(null),
        json!([1, 2]),
        json!({"index": 99999, "ids": [999999], "id": -1, "text": "x", "rect": "bad", "color": 5, "size": 1e308}),
    ] {
        for id in &ids {
            let _ = s.execute(id, &p);
        }
    }
    assert!(s.active().is_some());
    for d in s.documents() {
        assert!(d.doc.validate().is_empty(), "{:?}", d.doc.validate());
    }
}

#[test]
fn base64_roundtrip() {
    for data in [vec![], vec![0u8], vec![1, 2], vec![1, 2, 3], (0..=255u8).collect::<Vec<_>>()] {
        assert_eq!(cmd::base64_decode(&cmd::base64_encode(&data)).unwrap(), data);
    }
    assert!(cmd::base64_decode("!!").is_none());
}

#[test]
fn layout_change_keeps_text() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "titleAndContent", "title": "T", "body": "B"})).unwrap();
    s.execute("slide.layout", &json!({"layout": "twoContent"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    let texts: Vec<String> = v["shapes"].as_array().unwrap().iter().filter_map(|x| x["text"].as_str().map(String::from)).collect();
    assert!(texts.contains(&"T".to_string()) && texts.contains(&"B".to_string()), "{v}");
    assert_eq!(v["layout"], "Two Content");
}

#[test]
fn themes_and_slide_size() {
    let mut s = session();
    s.execute("theme.set", &json!({"name": "Ember"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.masters[0].theme.name, "Ember");
    s.execute("slide.size", &json!({"preset": "standard"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slide_size.width, 720.0);
    s.execute("slide.headerFooter", &json!({"slideNumber": true, "footer": true, "footerText": "Hello"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert!(v["shapes"].as_array().unwrap().iter().any(|x| x["placeholder"] == "sldNum"));
}

fn line_ends(s: &Session, id: u64) -> (kition_slide_geom::Point, kition_slide_geom::Point) {
    let st = s.doc().unwrap();
    let sh = st.shape(kition_slide_model::ShapeId(id as u32)).unwrap();
    connect::endpoints(&cmd::xfrm_of(&st.doc, &st.selection, sh))
}

#[test]
fn connectors_follow_glued_shapes() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 300, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let r = s.execute("shape.connect", &json!({"from": a, "to": b, "preset": "bentConnector3"})).unwrap();
    let c = r["id"].as_u64().unwrap();
    let (p0, p1) = line_ends(&s, c);
    // Closest pair: right side of A → left side of B... or bottom/top; either way on the boxes.
    let sites_a: Vec<Vec<f64>> = serde_json::from_value(s.execute("shape.sites", &json!({"id": a})).unwrap()).unwrap();
    assert!(sites_a.iter().any(|q| (q[0] - p0.x).abs() < 1e-6 && (q[1] - p0.y).abs() < 1e-6));
    // Move B: the connector's end follows, its start stays.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.execute("shape.move", &json!({"dx": 50, "dy": 20})).unwrap();
    let (q0, q1) = line_ends(&s, c);
    assert!((q0 - p0).hypot() < 1e-6);
    assert!(((q1 - p1) - kition_slide_geom::Vec2::new(50.0, 20.0)).hypot() < 1e-6, "{p1:?} → {q1:?}");
    // Undo puts both back in one step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((line_ends(&s, c).1 - p1).hypot() < 1e-6);
    // Deleting a glued shape unglues that end and leaves the line where it was.
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.execute("edit.delete", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let sh = st.shape(kition_slide_model::ShapeId(c as u32)).unwrap();
    assert!(matches!(sh.kind, kition_slide_model::ShapeKind::Connector { start: None, end: Some(_) }));
}

#[test]
fn drawing_a_line_between_shapes_glues_it() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [100, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    s.tool.kind = tools::ToolKind::Shape { preset: "straightConnector1".into() };
    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 4.0 };
    // Right site of the ellipse (200,150) → left site of the rect (400,150), a little off.
    s.pointer(ev(tools::PointerKind::Down, 202.0, 151.0)).unwrap();
    s.pointer(ev(tools::PointerKind::Drag, 300.0, 150.0)).unwrap();
    assert!(!s.tool.sites.is_empty() || s.tool.glue.is_none());
    s.pointer(ev(tools::PointerKind::Drag, 398.0, 149.0)).unwrap();
    assert!(s.tool.glue.is_some());
    s.pointer(ev(tools::PointerKind::Up, 398.0, 149.0)).unwrap();
    let st = s.doc().unwrap();
    let line = st.shapes().iter().find(|x| x.is_line()).unwrap();
    match line.kind {
        kition_slide_model::ShapeKind::Connector { start: Some((x, _)), end: Some((y, _)) } => {
            assert_eq!((x.0 as u64, y.0 as u64), (a, b));
        }
        ref k => panic!("not glued: {k:?}"),
    }
    let id = line.id.0 as u64;
    let (p0, p1) = line_ends(&s, id);
    assert!((p0.x - 200.0).abs() < 1e-6 && (p1.x - 400.0).abs() < 1e-6, "{p0:?} {p1:?}");
}

#[test]
fn freeform_tools_and_command() {
    let mut s = session();
    let before = s.doc().unwrap().shapes().len();
    // Composite command: one undo step.
    let r = s.execute("shape.freeform", &json!({"points": [[100, 100], [200, 120], [150, 220]], "closed": true})).unwrap();
    assert!(r["id"].as_u64().is_some());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before + 1);

    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 3.0 };
    // Polygon: click, click, click, click on the start → closed, filled.
    s.set_tool(tools::ToolKind::Shape { preset: "freeform".into() });
    for (x, y) in [(300.0, 300.0), (400.0, 300.0), (400.0, 400.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    assert!(s.tool.drawing_freeform());
    s.pointer(ev(tools::PointerKind::Down, 301.0, 301.0)).unwrap();
    assert!(!s.tool.drawing_freeform());
    let st = s.doc().unwrap();
    let sh = st.shapes().last().unwrap();
    match &sh.geom {
        kition_slide_model::Geom::Custom { paths, .. } => {
            assert!(paths[0].d.ends_with('Z') && paths[0].d.matches(" L ").count() == 2, "{}", paths[0].d)
        }
        g => panic!("{g:?}"),
    }
    let x = sh.xfrm.unwrap();
    assert_eq!((x.x, x.y, x.w, x.h), (300.0, 300.0, 100.0, 100.0));

    // Scribble: one drag, open path.
    s.set_tool(tools::ToolKind::Shape { preset: "scribble".into() });
    s.pointer(ev(tools::PointerKind::Down, 10.0, 10.0)).unwrap();
    for i in 1..20 {
        s.pointer(ev(tools::PointerKind::Drag, 10.0 + i as f64 * 5.0, 10.0 + (i as f64).sin() * 10.0)).unwrap();
    }
    let r = s.pointer(ev(tools::PointerKind::Up, 110.0, 10.0)).unwrap();
    assert!(r["id"].as_u64().is_some());
    assert_eq!(s.tool.kind, tools::ToolKind::Select);

    // Curve: clicks then Enter → smooth open path.
    s.set_tool(tools::ToolKind::Shape { preset: "curve".into() });
    for (x, y) in [(500.0, 100.0), (550.0, 50.0), (600.0, 100.0), (650.0, 50.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    let r = s.key("Enter", Default::default()).unwrap();
    let id = kition_slide_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let st = s.doc().unwrap();
    match &st.shape(id).unwrap().geom {
        kition_slide_model::Geom::Custom { paths, .. } => assert_eq!(paths[0].d.matches(" C ").count(), 3),
        g => panic!("{g:?}"),
    }
}

#[test]
fn double_click_in_table_uses_grown_row_heights() {
    // Rows stored 4 pt tall are drawn tall enough for their text, so 6 pt down is still row 0.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("table.insert", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::DoubleClick, 120.0, 106.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().and_then(|t| t.cell), Some((0, 0)));
}

#[test]
fn grown_table_frame_covers_its_drawn_rows() {
    // Rows stored 4 pt tall are drawn taller: the box, its handles and hit tests follow the drawn rows.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("table.insert", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let st = s.doc().unwrap();
    let sh = st.shapes().iter().find(|sh| json!(sh.id) == id).unwrap().clone();
    let kition_slide_model::ShapeKind::Table(t) = &sh.kind else { panic!("not a table") };
    let drawn: f64 = tools::table_rows(st, t).iter().sum();
    assert!(drawn > 12.0, "rows should grow: {drawn}");
    let x = cmd::xfrm_of(&st.doc, &st.selection, &sh);
    assert!((x.h - drawn).abs() < 1e-6, "frame {} vs drawn {drawn}", x.h);
    let below_stored = kition_slide_geom::Point::new(120.0, 100.0 + drawn - 2.0);
    assert!(matches!(s.hit_test(below_stored, 1.0), Some(tools::Hit::Shape { .. })));
}

#[test]
fn right_to_left_paragraphs() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "سنة 2024 (Retrait) فقط"})).unwrap();
    s.execute("format.rtl", &json!({})).unwrap();
    let para = |s: &Session| s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().paragraphs[0].props.clone();
    assert_eq!(para(&s).rtl, Some(true));
    // Left (the default) becomes right: the text stays at its start edge.
    assert_eq!(para(&s).align, Some(kition_slide_model::text::Align::Right));
    s.execute("format.align", &json!({"align": "justifyLow"})).unwrap();
    assert_eq!(para(&s).align, Some(kition_slide_model::text::Align::JustLow));
    // Toggling back keeps a justified alignment.
    s.execute("format.rtl", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(false));
    assert_eq!(para(&s).align, Some(kition_slide_model::text::Align::JustLow));
    s.execute("format.rtl", &json!({"on": true})).unwrap();
    s.execute("format.alignLeft", &json!({})).unwrap();
    s.execute("format.rtl", &json!({"on": false})).unwrap();
    assert_eq!(para(&s).align, Some(kition_slide_model::text::Align::Right));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(true));
}

#[test]
fn header_footer_fixed_date_is_the_slides_own_text() {
    use kition_slide_model::text::RunKind;
    let mut s = session();
    let date = |s: &Session| {
        let slide = s.doc().unwrap().current_slide().unwrap();
        slide.shapes.iter().find(|x| x.ph_type() == Some(kition_slide_model::PhType::Date)).and_then(|x| x.text.clone()).unwrap()
    };
    let kind = |s: &Session| date(s).paragraphs[0].runs[0].kind.clone();
    // Automatic: a date field, saved with today's date.
    s.execute("slide.headerFooter", &json!({"date": true})).unwrap();
    assert_eq!(kind(&s), RunKind::Field { field: "datetime1".into() });
    assert_eq!(date(&s).text().matches('/').count(), 2, "M/D/YYYY");
    // Fixed: plain text in the slide's own placeholder (other slides' fields aren't affected).
    s.execute("slide.headerFooter", &json!({"date": true, "dateText": "Launch day"})).unwrap();
    assert_eq!((kind(&s), date(&s).text()), (RunKind::Text, "Launch day".to_string()));
    // Back to automatic.
    s.execute("slide.headerFooter", &json!({"date": true, "dateText": ""})).unwrap();
    assert!(matches!(kind(&s), RunKind::Field { .. }));
    // A new date field is saved with the date it shows.
    s.execute("text.edit", &json!({"id": s.doc().unwrap().current_slide().unwrap().shapes[0].id.0})).unwrap();
    s.execute("text.insert", &json!({"text": "Updated "})).unwrap();
    s.execute("field.insertDateTime", &json!({"format": "datetime10"})).unwrap();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = title.paragraphs[0].runs.iter().find(|r| matches!(r.kind, RunKind::Field { .. })).cloned().unwrap();
    assert!(run.text.contains(':'), "H:mm {run:?}");
}

/// (text, linked?) of each run in the first paragraph of the current slide's title.
fn title_runs(s: &Session) -> Vec<(String, bool)> {
    let sh = &s.doc().unwrap().current_slide().unwrap().shapes[0];
    let p = &sh.text.as_ref().unwrap().paragraphs[0];
    p.runs.iter().map(|r| (r.text.clone(), r.props.link.is_some())).collect()
}

#[test]
fn hyperlink_with_a_bare_caret_inserts_the_address() {
    // With the caret at the end of the text, Insert Hyperlink used to format an empty range
    // and change nothing.
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "See "})).unwrap();
    s.execute("link.insert", &json!({"url": "https://example.org"})).unwrap();
    s.execute("text.insert", &json!({"text": " now"})).unwrap();
    assert_eq!(
        title_runs(&s),
        vec![("See ".into(), false), ("https://example.org".into(), true), (" now".into(), false)],
        "the address is inserted as a link, and typing after it doesn't extend it"
    );
}

#[test]
fn hyperlink_display_text_and_targets() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("link.insert", &json!({"url": "mailto:a@b.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true)], "email links show the address without mailto:");
    s.execute("text.insert", &json!({"text": " "})).unwrap();
    s.execute("link.insert", &json!({"slide": 0, "text": "start"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true), (" ".into(), false), ("start".into(), true)]);
}

#[test]
fn hyperlink_on_a_word_or_shape_links_it() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "left"})).unwrap();
    s.execute("link.insert", &json!({"url": "https://example.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("Hello ".into(), false), ("world".into(), true)], "a caret inside a word links the word");
    s.execute("text.exit", &json!({})).unwrap();
    s.execute("link.insert", &json!({"url": "https://example.org"})).unwrap();
    assert!(s.doc().unwrap().current_slide().unwrap().shapes[0].click.is_some(), "a selected shape gets the link as its click action");
}

#[test]
fn clicks_in_flipped_shapes_hit_the_text_where_it_is_drawn() {
    // Flips don't mirror text, so a click lands on the character drawn under it.
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 400, 100], "text": "Hello flipped world"})).unwrap();
    let id = kition_slide_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let pos = |s: &Session, x: f64, y: f64| {
        let st = s.doc().unwrap();
        tools::text_pos(st, st.shape(id).unwrap(), kition_slide_geom::Point::new(x, y))
    };
    let xs = [240.0, 300.0, 360.0];
    let plain = xs.map(|x| pos(&s, x, 150.0));
    assert!(plain[0].1 < plain[1].1 && plain[1].1 < plain[2].1, "{plain:?}");
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, x, 150.0)), plain);
    // flipV turns the text 180° about the box centre (300, 150).
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    s.execute("shape.flip", &json!({"axis": "vertical"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, 600.0 - x, 150.0)), plain);
}

#[test]
fn inserted_hyperlinks_look_like_links() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Read the docs"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
    s.execute("link.insert", &json!({"url": "https://example.org"})).unwrap();
    // Underlined (and hlink-coloured: same resolution as files) without touching the run's own props.
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], true);
    let body = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = body.paragraphs[0].runs.iter().find(|r| r.props.link.is_some()).unwrap();
    assert!(run.props.fill.is_none() && run.props.underline.is_none(), "{run:?}");
    // Underline off is the run's own u="none", as in PowerPoint.
    s.execute("format.underline", &json!({})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
}

#[test]
fn inherited_autofit_mode_changes_do_not_create_local_shrink() {
    use model::resolve::{self, Ctx};
    use model::text::AutoFit;
    use std::sync::Arc;

    for from_master in [false, true] {
        for (command_mode, expected) in [("none", AutoFit::None), ("resize", AutoFit::Shape)] {
            let mut doc = SlideFile::default();
            let layout_id = model::defaults::layout_of_kind(&doc, model::LayoutType::TitleAndContent).unwrap();
            let slide = model::defaults::new_slide(&mut doc, layout_id);
            doc.slides = vec![Arc::new(slide)];
            let slide_shape = Arc::make_mut(&mut doc.slides[0]).shapes.iter_mut().find(|s| s.ph_type() == Some(model::PhType::Title)).unwrap();
            let slide_id = slide_shape.id;
            let body = &mut slide_shape.text.as_mut().unwrap().body;
            body.autofit = None;
            body.autofit_saved = Some(true);
            body.autofit_input = Some("stale-local-receipt".into());
            let master = Arc::make_mut(&mut doc.masters[0]);
            let layout_index = master.layouts.iter().position(|l| l.id == layout_id).unwrap();
            let layout_shape = master.layouts[layout_index].shapes.iter_mut().find(|s| s.ph_type() == Some(model::PhType::Title)).unwrap();
            let layout_shape_id = layout_shape.id;
            layout_shape.text.as_mut().unwrap().body.autofit = None;
            let parent = if from_master {
                master.shapes.iter_mut().find(|s| s.ph_type() == Some(model::PhType::Title)).unwrap()
            } else {
                model::find_shape_mut(&mut master.layouts[layout_index].shapes, layout_shape_id).unwrap()
            };
            let parent_id = parent.id;
            let body = &mut parent.text.as_mut().unwrap().body;
            body.autofit = Some(AutoFit::Shrink { font_scale: 0.8, line_reduction: 0.1 });
            body.autofit_saved = Some(true);
            let ctx = Ctx::for_slide(&doc, &doc.slides[0]).unwrap();
            assert!(matches!(resolve::body(&ctx, model::find_shape(&doc.slides[0].shapes, slide_id).unwrap()).autofit, Some(AutoFit::Shrink { .. })));
            let mut s = Session::new();
            s.add_document(DocState::new(doc, None, "Inherited autofit".into()));
            let selection = &mut s.doc_mut().unwrap().selection;
            selection.target = if from_master { Target::Master { master: 0 } } else { Target::Layout { master: 0, layout: layout_index } };
            selection.shapes = vec![parent_id];
            s.execute("format.autofit", &json!({"mode": command_mode})).unwrap();
            let doc = &s.doc().unwrap().doc;
            let shape = model::find_shape(&doc.slides[0].shapes, slide_id).unwrap();
            let body = &shape.text.as_ref().unwrap().body;
            assert_eq!(body.autofit, None, "master={from_master}, mode={command_mode}: preserve inheritance");
            assert_eq!(body.autofit_saved, Some(false));
            assert_eq!(body.autofit_input, None);
            let ctx = Ctx::for_slide(doc, &doc.slides[0]).unwrap();
            assert_eq!(resolve::body(&ctx, shape).autofit, Some(expected));
            if from_master {
                let layout_shape = model::find_shape(&doc.masters[0].layouts[layout_index].shapes, layout_shape_id).unwrap();
                assert_eq!(layout_shape.text.as_ref().unwrap().body.autofit, None, "the layout must inherit the new master mode too");
            }
        }
    }
}

fn table_cell_text(s: &Session, id: &serde_json::Value, r: usize, c: usize) -> String {
    let st = s.doc().unwrap();
    let sh = st.shapes().iter().find(|sh| json!(sh.id) == *id).unwrap();
    let kition_slide_model::ShapeKind::Table(t) = &sh.kind else { panic!("not a table") };
    t.cell(r, c).unwrap().text.text()
}

#[test]
fn arrow_keys_leave_a_cell_only_at_its_edge() {
    let mut s = session();
    let data = json!([["ab", "cd"], ["ef", "gh"]]);
    let id = s.execute("table.insert", &json!({"rows": 2, "cols": 2, "data": data})).unwrap()["id"].clone();
    s.execute("text.edit", &json!({"id": id, "cell": [0, 0], "at": [0, 1]})).unwrap();
    let cell = |s: &Session| s.doc().unwrap().selection.text.as_ref().and_then(|t| t.cell);
    let caret = |s: &Session| s.doc().unwrap().selection.text.as_ref().map(|t| t.caret);
    s.key("ArrowRight", Mods::default()).unwrap();
    assert_eq!((cell(&s), caret(&s)), (Some((0, 0)), Some((0, 2))), "inside the text first");
    s.key("ArrowRight", Mods::default()).unwrap();
    assert_eq!((cell(&s), caret(&s)), (Some((0, 1)), Some((0, 0))), "then the next cell, at its start");
    s.key("ArrowDown", Mods::default()).unwrap();
    assert_eq!(cell(&s), Some((1, 1)));
    s.key("ArrowLeft", Mods::default()).unwrap();
    assert_eq!((cell(&s), caret(&s)), (Some((1, 0)), Some((0, 2))), "back into a cell lands at its end");
    // At the table's edge the key is still taken (it must not nudge the table).
    s.key("ArrowLeft", Mods { shift: false, alt: false, cmd: false }).unwrap();
    s.key("ArrowLeft", Mods::default()).unwrap();
    assert_eq!(cell(&s), Some((1, 0)));
    // Shift at the edge turns into a cell range.
    s.key("ArrowUp", Mods { shift: true, ..Mods::default() }).unwrap();
    let st = s.doc().unwrap();
    assert!(st.selection.text.is_none());
    assert_eq!(st.selection.cells.map(|c| (c.1, c.2)), Some(((1, 0), (0, 0))));
}

#[test]
fn delete_with_cells_selected_clears_them_instead_of_the_table() {
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"]]);
    let id = s.execute("table.insert", &json!({"rows": 2, "cols": 2, "data": data})).unwrap()["id"].clone();
    s.execute("table.selectCells", &json!({"id": id, "from": [0, 0], "to": [0, 1]})).unwrap();
    s.key("Delete", Mods::default()).unwrap();
    assert_eq!(table_cell_text(&s, &id, 0, 0), "");
    assert_eq!(table_cell_text(&s, &id, 0, 1), "");
    assert_eq!(table_cell_text(&s, &id, 1, 0), "c", "only the selected cells");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(table_cell_text(&s, &id, 0, 1), "b");
    // The Delete command (menus, agents) does the same.
    s.execute("table.selectCells", &json!({"id": id, "from": [1, 0], "to": [1, 0]})).unwrap();
    s.execute("edit.delete", &json!({})).unwrap();
    assert_eq!(table_cell_text(&s, &id, 1, 0), "");
    assert_eq!(table_cell_text(&s, &id, 1, 1), "d");
    // Shift+arrows grow a range; arrows alone move to a cell's text.
    s.execute("table.selectCells", &json!({"id": id, "from": [0, 0], "to": [0, 0]})).unwrap();
    s.key("ArrowDown", Mods { shift: true, ..Mods::default() }).unwrap();
    assert_eq!(s.doc().unwrap().selection.cells.map(|c| (c.1, c.2)), Some(((0, 0), (1, 0))));
    s.key("ArrowRight", Mods::default()).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().and_then(|t| t.cell), Some((1, 1)));
}

#[test]
fn borders_apply_to_edges_of_the_selection_and_round_trip_through_pptx() {
    let mut s = session();
    let id = s.execute("table.insert", &json!({"rows": 3, "cols": 3})).unwrap()["id"].clone();
    s.execute("table.selectCells", &json!({"id": id, "from": [1, 1], "to": [1, 1]})).unwrap();
    s.execute("table.border", &json!({"edges": "outside", "color": "#FF0000", "width": 3.0})).unwrap();
    s.execute("table.border", &json!({"id": id, "edges": "diagDown", "from": [0, 0]})).unwrap();
    assert!(s.execute("table.border", &json!({"id": id, "edges": "sideways"})).is_err());
    assert!(s.execute("table.border", &json!({"id": id, "edges": "top", "width": 99.0})).is_err());
    let raw = cmd::file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
    let mut reopened = Session::new();
    cmd::file::add_opened(&mut reopened, "t.pptx", None, cmd::file::open_slide_file("t.pptx", &raw).unwrap());
    let st = reopened.doc().unwrap();
    let t = st
        .shapes()
        .iter()
        .find_map(|sh| match &sh.kind {
            kition_slide_model::ShapeKind::Table(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap();
    let red = |l: &Option<kition_slide_model::Line>| {
        l.as_ref().is_some_and(|l| l.width == Some(3.0) && matches!(&l.fill, Some(kition_slide_model::Fill::Solid { .. })))
    };
    let center = t.cell(1, 1).unwrap();
    assert!(center.borders.iter().all(red), "{:?}", center.borders);
    assert!(red(&t.cell(0, 1).unwrap().borders[3]), "the cell above shares the top line");
    assert!(t.cell(0, 0).unwrap().diag_down.is_some());
    assert!(t.cell(2, 2).unwrap().borders.iter().all(Option::is_none));
}

/// The schemas declare a shape id as a number or a string of one. A string id used to read as no
/// id at all, so text.set wrote into whatever shape was selected; an id that is not a shape id is
/// now an error, never the selection.
#[test]
fn a_shape_id_may_be_a_number_string_and_never_falls_back_to_the_selection() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let shapes: Vec<u32> = s.doc().unwrap().current_slide().unwrap().shapes.iter().filter(|sh| sh.text.is_some()).map(|sh| sh.id.0).collect();
    let (named, selected) = (shapes[0], shapes[1]);
    s.execute("edit.select", &json!({"ids": [selected]})).unwrap();
    s.execute("text.set", &json!({"id": named.to_string(), "text": "by string"})).unwrap();
    assert_eq!(s.execute("text.get", &json!({"id": named})).unwrap()["text"], "by string");
    assert_eq!(s.execute("text.get", &json!({"id": format!(" {named} ")})).unwrap()["text"], "by string");
    let before = s.execute("text.get", &json!({"id": selected})).unwrap()["text"].clone();
    for cmd in ["text.set", "text.edit", "text.get", "shape.inspect"] {
        let err = s.execute(cmd, &json!({"id": "Title 1", "text": "wrong"})).unwrap_err().to_string();
        assert!(err.contains("a shape id is a number"), "{cmd}: {err}");
    }
    assert_eq!(s.execute("text.get", &json!({"id": selected})).unwrap()["text"], before);
    s.execute("edit.select", &json!({"ids": [named.to_string(), selected.to_string()]})).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.len(), 2);
}

/// A file keeps text only in shapes and table cells. text.set on a group answered ok and the text
/// was lost on saving (khp-bench, a remembered id that named a group on another slide); it is
/// refused now, and the group is left as it was.
#[test]
fn text_set_on_a_group_is_refused_not_dropped() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let ids: Vec<u32> = s.doc().unwrap().current_slide().unwrap().shapes.iter().take(2).map(|sh| sh.id.0).collect();
    s.execute("edit.select", &json!({"ids": ids})).unwrap();
    s.execute("arrange.group", &json!({})).unwrap();
    let group = s
        .doc()
        .unwrap()
        .current_slide()
        .unwrap()
        .shapes
        .iter()
        .find(|sh| matches!(sh.kind, kition_slide_model::ShapeKind::Group { .. }))
        .unwrap()
        .id
        .0;
    let err = s.execute("text.set", &json!({"id": group, "text": "lost"})).unwrap_err().to_string();
    assert!(err.contains("a group"), "{err}");
    let sh = s.doc().unwrap().current_slide().unwrap().shapes.iter().find(|sh| sh.id.0 == group).unwrap().clone();
    assert!(sh.text.as_ref().is_none_or(|t| t.text().is_empty()), "the group got text");
}

/// The drawn corners of connector `id` in slide points (its first sub-path's move and line points).
fn connector_corners(s: &Session, id: u64) -> Vec<kition_slide_geom::Point> {
    let st = s.doc().unwrap();
    let sh = st.shape(kition_slide_model::ShapeId(id as u32)).unwrap();
    let x = cmd::xfrm_of(&st.doc, &st.selection, sh);
    let g = kition_slide_render::shape_geometry(sh, x.w, x.h);
    let a = x.affine();
    let mut out = vec![];
    for el in g.paths[0].path.elements() {
        match el {
            kition_slide_geom::PathEl::MoveTo(p) | kition_slide_geom::PathEl::LineTo(p) => out.push(a * *p),
            _ => {}
        }
    }
    out
}

#[test]
fn an_elbow_from_a_bottom_site_to_a_top_site_leaves_downward() {
    // bentConnector3 from a bottom site to a top site went sideways first,
    // along the box edges. PowerPoint turns the connector a quarter turn: down, across, down.
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 80]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 300, 200, 80]})).unwrap()["id"].as_u64().unwrap();
    fn sites(s: &mut Session, id: u64) -> Vec<Vec<f64>> {
        serde_json::from_value(s.execute("shape.sites", &json!({"id": id})).unwrap()).unwrap()
    }
    let bottom = sites(&mut s, a).iter().position(|q| (q[1] - 180.0).abs() < 1e-6).unwrap();
    let top = sites(&mut s, b).iter().position(|q| (q[1] - 300.0).abs() < 1e-6).unwrap();
    let r = s.execute("shape.connect", &json!({"from": a, "to": b, "fromSite": bottom, "toSite": top, "preset": "bentConnector3"})).unwrap();
    let c = r["id"].as_u64().unwrap();
    let pts = connector_corners(&s, c);
    assert!(pts.len() >= 4, "{pts:?}");
    let near = |p: kition_slide_geom::Point, x: f64, y: f64| (p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6;
    assert!(near(pts[0], 200.0, 180.0), "starts at A's bottom site: {pts:?}");
    assert!(near(*pts.last().unwrap(), 500.0, 300.0), "ends at B's top site: {pts:?}");
    assert!((pts[1].x - 200.0).abs() < 1e-6 && pts[1].y > 180.0 + 1.0, "first leg goes down: {pts:?}");
    let n = pts.len();
    assert!((pts[n - 2].x - 500.0).abs() < 1e-6 && pts[n - 2].y < 300.0 - 1.0, "last leg comes down into B: {pts:?}");
    // Moving B keeps the route vertical-first.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.execute("shape.move", &json!({"dx": -250, "dy": 40})).unwrap();
    let pts = connector_corners(&s, c);
    assert!(near(pts[0], 200.0, 180.0) && near(*pts.last().unwrap(), 250.0, 340.0), "{pts:?}");
    assert!((pts[1].x - 200.0).abs() < 1e-6 && pts[1].y > 181.0, "{pts:?}");
    // Right site to left site still goes across first.
    let right = sites(&mut s, a).iter().position(|q| (q[0] - 300.0).abs() < 1e-6).unwrap();
    let d = s.execute("shape.insert", &json!({"preset": "rect", "rect": [500, 500, 100, 60]})).unwrap()["id"].as_u64().unwrap();
    let left = sites(&mut s, d).iter().position(|q| (q[0] - 500.0).abs() < 1e-6).unwrap();
    let r = s.execute("shape.connect", &json!({"from": a, "to": d, "fromSite": right, "toSite": left, "preset": "bentConnector3"})).unwrap();
    let pts = connector_corners(&s, r["id"].as_u64().unwrap());
    assert!(near(pts[0], 300.0, 140.0) && (pts[1].y - 140.0).abs() < 1e-6 && pts[1].x > 301.0, "{pts:?}");
}

fn render_current(s: &Session, scale: f64) -> kition_slide_render::Image {
    let st = s.doc().unwrap();
    kition_slide_render::render_slide(&st.doc, st.selection.slide, &kition_slide_render::RenderOpts { scale, ..Default::default() })
}

#[test]
fn a_glow_leaves_a_donuts_hole_empty_and_the_outer_shadow_shows() {
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    // A glow rings a donut inside and out; its hole stays the background.
    let d = s.execute("shape.insert", &json!({"preset": "donut", "rect": [100, 100, 200, 200]})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.effects", &json!({"ids": [d], "glow": {"color": "#ff0000", "radius": 10}})).unwrap();
    let img = render_current(&s, 1.0);
    assert_eq!(img.pixel(200, 200), [255, 255, 255, 255], "the hole is filled");
    let rim = img.pixel(200, 151);
    assert!(rim[0] > 200 && rim[1] < 230, "the glow rings the hole: {rim:?}");
    // The Outer shadow preset (PowerPoint's Offset: Bottom Right) is as dark as PowerPoint draws
    // it: a 40 % black shadow 3 pt down and right, blurred 4 pt.
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [500, 100, 200, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"ids": [r], "color": "#ffffff"})).unwrap();
    s.execute("shape.line", &json!({"ids": [r], "none": true})).unwrap();
    s.execute("shape.effects", &json!({"ids": [r], "shadow": "outer"})).unwrap();
    let img = render_current(&s, 4.0);
    // 1.5 pt right of the right edge, half way down.
    let px = img.pixel(4 * 700 + 6, 4 * 150);
    assert!(px[0] < 205, "the shadow is barely visible: {px:?}");
    // Freeforms are named like other new shapes, by their place on the slide, not their id.
    let f = s.execute("shape.freeform", &json!({"points": [[100, 400], [200, 400], [150, 480]], "closed": true})).unwrap()["id"].as_u64().unwrap();
    let st = s.doc().unwrap();
    let n = st.shapes().len();
    let name = &st.shape(kition_slide_model::ShapeId(f as u32)).unwrap().name;
    assert_eq!(name, &format!("Freeform {n}"));
    assert_ne!(n as u64, f, "the test needs the id and the place to differ");
}

#[test]
fn inspect_follows_a_resized_group_and_a_moved_child() {
    // After shape.setBounds on a group, slide.inspect kept the children's
    // old boxes; moving a child left the group's box stale; theme.colors answered null.
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [a, b]})).unwrap()["id"].as_u64().unwrap();
    let find = |v: &serde_json::Value, id: u64| -> serde_json::Value {
        let shapes = v["shapes"].as_array().unwrap();
        let group = shapes.iter().find(|x| x["id"] == g).unwrap().clone();
        if id == g {
            return group;
        }
        group["children"].as_array().unwrap().iter().find(|x| x["id"] == id).unwrap().clone()
    };
    s.execute("shape.setBounds", &json!({"id": g, "x": 50, "y": 50, "w": 600, "h": 300})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert_eq!(find(&v, g)["box"], json!([50.0, 50.0, 600.0, 300.0]));
    assert_eq!(find(&v, a)["box"], json!([50.0, 50.0, 200.0, 100.0]), "{v}");
    assert_eq!(find(&v, b)["box"], json!([450.0, 250.0, 200.0, 100.0]), "{v}");
    // Moving a child grows the group's box to hold it; the other child stays where it is drawn.
    s.execute("shape.move", &json!({"ids": [b], "dx": 100, "dy": 50})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    let gb: Vec<f64> = serde_json::from_value(find(&v, g)["box"].clone()).unwrap();
    let ab: Vec<f64> = serde_json::from_value(find(&v, a)["box"].clone()).unwrap();
    let bb: Vec<f64> = serde_json::from_value(find(&v, b)["box"].clone()).unwrap();
    assert_eq!(ab, vec![50.0, 50.0, 200.0, 100.0], "{v}");
    // The move is in slide points, as the boxes are listed.
    assert_eq!(bb, vec![550.0, 300.0, 200.0, 100.0], "{v}");
    assert_eq!(gb, vec![50.0, 50.0, 700.0, 350.0], "{v}");
    // A child's bounds are set in slide points too.
    s.execute("shape.setBounds", &json!({"ids": [a], "x": 100, "y": 100, "w": 100, "h": 50})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert_eq!(find(&v, a)["box"], json!([100.0, 100.0, 100.0, 50.0]), "{v}");
    assert_eq!(find(&v, b)["box"], json!([550.0, 300.0, 200.0, 100.0]), "{v}");
    assert_eq!(find(&v, g)["box"], json!([100.0, 100.0, 650.0, 300.0]), "{v}");
    // Formatting a member's text leaves the boxes exactly as they were (a smart graphic's frame larger
    // than its shapes is not shrunk to them).
    let before = s.doc().unwrap().shape(kition_slide_model::ShapeId(g as u32)).unwrap().clone();
    s.execute("format.fontSize", &json!({"ids": [a], "size": 30})).unwrap();
    let after = s.doc().unwrap().shape(kition_slide_model::ShapeId(g as u32)).unwrap().clone();
    assert_eq!(after.xfrm, before.xfrm);
    let boxes = |sh: &kition_slide_model::Shape| match &sh.kind {
        kition_slide_model::ShapeKind::Group { children, child } => (children.iter().map(|c| c.xfrm).collect::<Vec<_>>(), *child),
        _ => panic!(),
    };
    assert_eq!(boxes(&after), boxes(&before));
    // theme.colors answers the scheme, and {} only reads it.
    let c = s.execute("theme.colors", &json!({"colors": {"accent1": "#123456"}})).unwrap();
    assert_eq!(c["colors"]["accent1"], "#123456");
    assert_eq!(c["name"], "Custom");
    let read = s.execute("theme.colors", &json!({})).unwrap();
    assert_eq!(read, c);
    assert!(s.execute("theme.colors", &json!({"colors": {"accent9": "#123456"}})).is_err());
}

fn low_contrast(s: &mut Session) -> Vec<u64> {
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    v["design"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["code"] == "design.low-contrast")
        .flat_map(|a| a["shapes"].as_array().unwrap().iter().map(|id| id.as_u64().unwrap()))
        .collect()
}

#[test]
fn contrast_backgrounds_for_all_and_a_dark_theme() {
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    // White text on a shape filled by its style (no fill of its own), on a gradient and on a
    // filled shape with a glow is measured against that fill, not the white slide.
    let styled = s.execute("shape.insert", &json!({"preset": "roundRect", "rect": [40, 40, 260, 120], "text": "Styled card"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    let grad =
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [340, 40, 260, 120], "text": "Gradient card"})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"ids": [grad], "gradient": {"stops": [[0, "#0F172A"], [1, "#1E3A8A"]]}})).unwrap();
    let glow =
        s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [640, 40, 260, 120], "text": "Glowing"})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"ids": [glow], "color": "#1E3A8A"})).unwrap();
    s.execute("shape.effects", &json!({"ids": [glow], "glow": {"color": "#60A5FA", "radius": 12}})).unwrap();
    // A text box on the styled card is measured against the card too.
    let over = s.execute("textBox.insert", &json!({"rect": [60, 120, 200, 30], "text": "On the card"})).unwrap()["id"].as_u64().unwrap();
    s.execute("format.color", &json!({"ids": [styled, grad, glow, over], "color": "#FFFFFF"})).unwrap();
    assert_eq!(low_contrast(&mut s), Vec::<u64>::new());
    // White text straight on the white slide still is low contrast.
    let bare = s.execute("textBox.insert", &json!({"rect": [40, 300, 300, 40], "text": "On the slide"})).unwrap()["id"].as_u64().unwrap();
    s.execute("format.color", &json!({"ids": [bare], "color": "#FFFFFF"})).unwrap();
    assert_eq!(low_contrast(&mut s), vec![bare]);

    // Apply to All reaches slides added later.
    s.execute("slide.background", &json!({"color": "#102030", "all": true})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert_eq!(v["background"], "#102030", "{v}");
    assert_eq!(s.execute("slide.inspect", &json!({"index": 0})).unwrap()["background"], "#102030");

    // Nocturne keeps dark in dk1 and light in lt1 and maps the background to the dark ones, so a
    // medium2-accent1 table draws dark text on its light body tints.
    let mut s = session();
    s.execute("theme.set", &json!({"name": "Nocturne"})).unwrap();
    let c = s.execute("theme.colors", &json!({})).unwrap();
    assert_eq!((c["colors"]["dk1"].as_str(), c["colors"]["lt1"].as_str()), (Some("#121826"), Some("#FFFFFF")));
    s.execute("slide.new", &json!({"layout": "titleOnly"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert_eq!(v["background"], "#121826", "{v}");
    let title = v["shapes"][0]["id"].as_u64().unwrap();
    s.execute("text.set", &json!({"id": title, "text": "Running order"})).unwrap();
    assert_eq!(low_contrast(&mut s), Vec::<u64>::new(), "the title is light on the dark slide");
    let t = s
        .execute("table.insert", &json!({"rows": 3, "cols": 2, "rect": [60, 150, 840, 180], "data": [["Time", "Session"], ["MMMMMMMM", "MMMMMMMM"], ["MMMMMMMM", "MMMMMMMM"]]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute("table.style", &json!({"id": t, "style": "medium2-accent1"})).unwrap();
    s.execute("format.fontSize", &json!({"ids": [t], "size": 28})).unwrap();
    let img = render_current(&s, 1.0);
    // The darkest pixel in the first body row: dark text, not the tint.
    let mut darkest = u32::MAX;
    for y in 215..265 {
        for x in 70..440 {
            let p = img.pixel(x, y);
            darkest = darkest.min(u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]));
        }
    }
    assert!(darkest < 300, "body text is light on the light tint: {darkest}");
}

#[test]
fn smart_art_takes_a_rect_draws_its_links_and_is_formatted_as_a_group() {
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    let group_shapes = |s: &Session, id: u64| -> Vec<kition_slide_model::Shape> {
        let st = s.doc().unwrap();
        match &st.shape(kition_slide_model::ShapeId(id as u32)).unwrap().kind {
            kition_slide_model::ShapeKind::Group { children, .. } => children.clone(),
            k => panic!("not a group: {k:?}"),
        }
    };
    // `rect` places it.
    let g = s
        .execute("smartGraphic.insert", &json!({"kind": "hierarchy", "items": ["CEO", "Sales", "Product", "Finance"], "rect": [100, 80, 600, 300]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let v = s.execute("shape.inspect", &json!({"id": g})).unwrap();
    let b = &v["box"];
    assert!((b["x"].as_f64().unwrap() - 100.0).abs() < 1.0 && (b["y"].as_f64().unwrap() - 80.0).abs() < 1.0, "{b}");
    assert!((b["w"].as_f64().unwrap() - 600.0).abs() < 1.0 && (b["h"].as_f64().unwrap() - 300.0).abs() < 1.0, "{b}");
    // The hierarchy joins the root to each child.
    let kids = group_shapes(&s, g);
    assert_eq!(kids.iter().filter(|c| c.is_line()).count(), 3, "lines from the root to its three children");
    // Formatting the group formats its shapes.
    s.execute("shape.fill", &json!({"ids": [g], "color": "#123456"})).unwrap();
    s.execute("format.fontSize", &json!({"ids": [g], "size": 30})).unwrap();
    s.execute("format.color", &json!({"ids": [g], "color": "#FFEE00"})).unwrap();
    for c in group_shapes(&s, g).iter().filter(|c| !c.is_line()) {
        assert_eq!(c.fill, Some(kition_slide_model::Fill::solid(kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::rgb(0x12, 0x34, 0x56)))));
        let run = &c.text.as_ref().unwrap().paragraphs[0].runs[0].props;
        assert_eq!(run.size, Some(30.0), "{}", c.name);
        assert!(run.fill.is_some());
    }
    let st = s.doc().unwrap();
    assert!(st.shape(kition_slide_model::ShapeId(g as u32)).unwrap().text.as_ref().is_none_or(|t| t.text().is_empty()));
    // A cycle sizes its nodes to the area, draws an arrow between each node and the next, and
    // sets text no wider than its nodes.
    let small = s
        .execute("smartGraphic.insert", &json!({"kind": "cycle", "items": ["Plan", "Build", "Measure", "Learn"], "rect": [0, 0, 200, 200]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let big = s
        .execute("smartGraphic.insert", &json!({"kind": "cycle", "items": ["Plan", "Build", "Measure", "Learn"], "rect": [300, 0, 500, 500]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let node = |id| group_shapes(&s, id).into_iter().find(|c| c.geom.preset_name() == Some("ellipse")).unwrap().xfrm.unwrap().w;
    assert!(node(big) > node(small) * 2.0, "{} vs {}", node(big), node(small));
    let shapes = group_shapes(&s, big);
    assert_eq!(shapes.iter().filter(|c| c.geom.preset_name() == Some("rightArrow")).count(), 4);
    // "Measure" fits across a small node: its size is under 20 pt.
    let sizes: Vec<f64> = group_shapes(&s, small)
        .iter()
        .filter(|c| c.geom.preset_name() == Some("ellipse"))
        .map(|c| c.text.as_ref().unwrap().paragraphs[0].runs[0].props.size.unwrap())
        .collect();
    assert!(sizes.iter().all(|z| *z < 20.0 && *z >= 8.0), "{sizes:?}");
}

/// Rows of `img` (between `x0..x1`) holding a dark pixel: the first and last.
fn ink_rows(img: &kition_slide_render::Image, x0: u32, x1: u32) -> Option<(u32, u32)> {
    let rows: Vec<u32> = (0..img.height).filter(|y| (x0..x1).any(|x| img.pixel(x, *y)[0] < 128)).collect();
    Some((*rows.first()?, *rows.last()?))
}

#[test]
fn vertical_text_runs_down_its_box_and_an_arch_keeps_its_size() {
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    // A resizing text box turned vertical keeps its height, its line runs down it, and its
    // width fits the one column.
    let t = s.execute("textBox.insert", &json!({"rect": [240, 40, 300, 40], "text": "Vertical caption text"})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.setBounds", &json!({"ids": [t], "w": 40, "h": 300})).unwrap();
    s.execute("format.direction", &json!({"ids": [t], "dir": "rotate90"})).unwrap();
    let b = s.execute("shape.inspect", &json!({"id": t})).unwrap()["box"].clone();
    assert_eq!(b["h"], 300.0, "{b}");
    assert!(b["w"].as_f64().unwrap() < 45.0 && b["w"].as_f64().unwrap() > 20.0, "{b}");
    let st = s.doc().unwrap();
    let sh = st.shape(kition_slide_model::ShapeId(t as u32)).unwrap().clone();
    let ctx = cmd::text::ctx_for(&st.doc, &st.selection).unwrap();
    let x = cmd::xfrm_of(&st.doc, &st.selection, &sh);
    let rect = kition_slide_render::shape_geometry(&sh, x.w, x.h).text_rect;
    let opts = kition_slide_text::Opts { rect, fields: &kition_slide_text::NoFields, prompt_color: None, no_shrink: true, empty_bullets: false };
    let laid = kition_slide_text::layout(&ctx, &sh, sh.text.as_ref().unwrap(), &opts);
    assert_eq!(laid.lines.len(), 1, "one column down the box");
    assert!(!laid.overflow);
    // A tall narrow shape with vertical text that fits down it does not overflow.
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [40, 40, 60, 400], "text": "Quarterly results overview"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute("format.direction", &json!({"ids": [r], "dir": "rotate90"})).unwrap();
    let st = s.doc().unwrap();
    let sh = st.shape(kition_slide_model::ShapeId(r as u32)).unwrap().clone();
    let rect = kition_slide_render::shape_geometry(&sh, 60.0, 400.0).text_rect;
    let opts = kition_slide_text::Opts { rect, fields: &kition_slide_text::NoFields, prompt_color: None, no_shrink: true, empty_bullets: false };
    let laid = kition_slide_text::layout(&cmd::text::ctx_for(&st.doc, &st.selection).unwrap(), &sh, sh.text.as_ref().unwrap(), &opts);
    assert!(!laid.overflow && laid.lines.len() == 1);

    // Arch Up on one line in a (top-anchored) text box follows the arch at the text's own size:
    // bigger text draws a thicker band.
    let arch = |size: f64| {
        let mut s = session();
        s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
        let w = s.execute("textBox.insert", &json!({"rect": [100, 100, 600, 200], "text": "Arch over the top"})).unwrap()["id"].as_u64().unwrap();
        s.execute("format.autofit", &json!({"ids": [w], "mode": "none"})).unwrap();
        s.execute("shape.setBounds", &json!({"ids": [w], "x": 100, "y": 100, "w": 600, "h": 200})).unwrap();
        s.execute("format.fontSize", &json!({"ids": [w], "size": size})).unwrap();
        s.execute("format.align", &json!({"ids": [w], "align": "center"})).unwrap();
        s.execute("format.transform", &json!({"ids": [w], "preset": "textArchUp"})).unwrap();
        let img = render_current(&s, 1.0);
        // The band at the arch's top, in the middle of the box.
        let (top, bottom) = ink_rows(&img, 390, 410).unwrap();
        bottom - top
    };
    let (small, big) = (arch(18.0), arch(40.0));
    assert!(big as f64 > small as f64 * 1.6, "band {small} px at 18 pt, {big} px at 40 pt");
}

fn chart_of(s: &Session, id: u64) -> kition_slide_model::Chart {
    match &s.doc().unwrap().shape(kition_slide_model::ShapeId(id as u32)).unwrap().kind {
        kition_slide_model::ShapeKind::Chart(c) => (**c).clone(),
        k => panic!("not a chart: {k:?}"),
    }
}

/// The colours drawn in `img` inside `r` (points at scale 1), as counts of each colour.
fn colours_in(img: &kition_slide_render::Image, r: [u32; 4]) -> std::collections::HashMap<[u8; 3], usize> {
    let mut out = std::collections::HashMap::new();
    for y in r[1]..r[3] {
        for x in r[0]..r[2] {
            let p = img.pixel(x, y);
            *out.entry([p[0], p[1], p[2]]).or_default() += 1;
        }
    }
    out
}

#[test]
fn charts_refuse_unknown_types_and_draw_waterfall_funnel_treemap_and_combo() {
    let mut s = session();
    s.execute("slide.layout", &json!({"layout": "blank"})).unwrap();
    // Unknown chart types and palettes are refused.
    assert!(s.execute("chart.insert", &json!({"type": "sankey"})).is_err());
    let data = json!({"categories": ["A", "B", "C", "D"], "series": [{"name": "Cash", "values": [100, -30, 50, 120], "totals": [3]}]});
    let mut p = data.clone();
    p["type"] = json!("waterfall");
    p["rect"] = json!([40, 40, 400, 300]);
    let w = s.execute("chart.insert", &p).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("chart.type", &json!({"id": w, "type": "pyramid3d"})).is_err());
    assert!(s.execute("chart.options", &json!({"id": w, "palette": "rainbow"})).is_err());
    s.execute("chart.options", &json!({"id": w, "palette": "colorful2", "gapWidth": 50})).unwrap();
    let c = chart_of(&s, w);
    assert_eq!((c.palette.as_str(), c.gap_width), ("colorful2", 0.5));
    // A chart given data has no placeholder title: one series is titled with its name, unless
    // it is a waterfall, funnel or treemap.
    assert_eq!(c.title, None);
    let one = s
        .execute("chart.insert", &json!({"type": "column", "series": [{"name": "Revenue", "values": [1, 2]}], "rect": [500, 400, 100, 100]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    assert_eq!(chart_of(&s, one).title.as_deref(), Some("Revenue"));
    // The waterfall floats: B (a decrease) hangs from A's top, D is a total from zero.
    let steps = chart_of(&s, w).series[0].waterfall();
    let spans: Vec<(f64, f64)> = steps.iter().map(|x| x.map(|x| (x.from, x.to)).unwrap()).collect();
    assert_eq!(spans, vec![(0.0, 100.0), (100.0, 70.0), (70.0, 120.0), (0.0, 120.0)]);
    let img = render_current(&s, 1.0);
    // Under B's bar (between 0 and 70) there is no bar colour: only the plot's background.
    let accents: Vec<[u8; 3]> = {
        let st = s.doc().unwrap();
        let m = &st.doc.masters[0].theme.colors;
        [kition_slide_color::SchemeSlot::Accent2, kition_slide_color::SchemeSlot::Accent3, kition_slide_color::SchemeSlot::Accent4]
            .iter()
            .map(|slot| {
                let c = m.get(*slot);
                [c.r, c.g, c.b]
            })
            .collect()
    };
    let whole = colours_in(&img, [40, 40, 440, 340]);
    let present = |col: &[u8; 3]| whole.get(col).copied().unwrap_or(0) > 50;
    // colorful2: increase accent 2, decrease accent 3, total accent 4.
    assert!(accents.iter().all(present), "waterfall colours by step: {accents:?}");
    s.execute("edit.delete", &json!({"ids": [w, one]})).unwrap();

    // Funnel and treemap are not drawn as lines: their bars and tiles fill much of the plot.
    for kind in ["funnel", "treemap"] {
        let mut p = data.clone();
        p["series"][0]["values"] = json!([400, 300, 150, 60]);
        p["type"] = json!(kind);
        p["rect"] = json!([40, 40, 400, 300]);
        let id = s.execute("chart.insert", &p).unwrap()["id"].as_u64().unwrap();
        let img = render_current(&s, 1.0);
        let filled: usize = colours_in(&img, [40, 40, 440, 340]).iter().filter(|(c, _)| c[0] < 200 || c[1] < 200 || c[2] < 200).map(|(_, n)| n).sum();
        assert!(filled > 400 * 300 / 4, "{kind}: {filled} filled pixels");
        s.execute("edit.delete", &json!({"ids": [id]})).unwrap();
    }

    // Combo: columns with the last series as a line, unless a series says otherwise.
    let combo = s
        .execute(
            "chart.insert",
            &json!({"type": "combo", "categories": ["A", "B"], "series": [{"name": "Units", "values": [3, 4]}, {"name": "Share", "values": [1, 2]}]}),
        )
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let kinds: Vec<_> = chart_of(&s, combo).series.iter().map(|x| x.kind).collect();
    assert_eq!(kinds, vec![Some(kition_slide_model::chart::ChartType::Column), Some(kition_slide_model::chart::ChartType::Line)]);

    // Chart text takes the text formatting commands; series take colours.
    s.execute("format.fontSize", &json!({"ids": [combo], "size": 16})).unwrap();
    s.execute("format.color", &json!({"ids": [combo], "color": "#AA0000"})).unwrap();
    s.execute("format.bold", &json!({"ids": [combo]})).unwrap();
    s.execute("chart.options", &json!({"id": combo, "colors": ["#112233", "#445566"]})).unwrap();
    let c = chart_of(&s, combo);
    assert_eq!(c.fmt.text.size, Some(16.0));
    assert_eq!(c.fmt.text.bold, Some(true));
    assert_eq!(c.fmt.text.color, Some(kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::rgb(0xAA, 0, 0))));
    assert_eq!(
        c.series[0].fill,
        Some(kition_slide_model::Fill::solid(kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::rgb(0x11, 0x22, 0x33))))
    );
}

/// Plan 0B (walkthrough D7; case regressions/issue-hand-d7-handle-hit): a resize handle reaches
/// 7 screen pixels, the nearest handle wins, the rotation handle sits 18 screen pixels above the
/// top edge at any zoom, and a press just outside a selected frame keeps the selection.
#[test]
fn handles_reach_seven_pixels_and_a_near_miss_keeps_the_selection() {
    use crate::tools::{Hit, TOL_PX};
    use kition_slide_geom::Point;
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [200, 100, 200, 100]})).unwrap()["id"].as_u64().unwrap();
    let shape = kition_slide_model::ShapeId(id as u32);
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    for zoom in [1.0, 2.0, 0.5] {
        let tol = TOL_PX / zoom;
        let px = 1.0 / zoom;
        // 6.5 px off the SE corner, diagonally out: the SE handle.
        let off = 6.5 / std::f64::consts::SQRT_2 * px;
        assert_eq!(s.hit_test(Point::new(400.0 + off, 200.0 + off), tol), Some(Hit::Resize { id: shape, handle: 4 }), "zoom {zoom}");
        // 18 px above the top edge's middle: the rotation handle; 18 pt at 1x was not it at 2x.
        assert_eq!(s.hit_test(Point::new(300.0, 100.0 - 18.0 * px), tol), Some(Hit::Rotate { id: shape }), "zoom {zoom}");
        // 9 px straight out of the E handle: past its reach, but on the frame's band.
        assert_eq!(s.hit_test(Point::new(400.0 + 9.0 * px, 150.0), tol), Some(Hit::Shape { id: shape, text: false }), "zoom {zoom}");
        // 12 px out: the empty slide.
        assert_eq!(s.hit_test(Point::new(400.0 + 12.0 * px, 150.0), tol), None, "zoom {zoom}");
    }
    // A press 9 pt (9 px at 1x) right of the E handle keeps the selection; a drag from it moves the shape.
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: TOL_PX };
    s.pointer(ev(PointerKind::Down, 409.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 409.0, 150.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes, vec![shape], "a near miss keeps the selection");
    // 6 px off the E handle's centre, dragged 40 px: a resize.
    s.pointer(ev(PointerKind::Down, 406.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 446.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 446.0, 150.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["w"], 240.0, "{v}");
    assert_eq!(v["box"]["x"], 200.0, "{v}");
}

/// Plan 0B (walkthrough C3; case regressions/issue-hand-c3-cursors): what the pointer would do,
/// which the editor shows as the pointer's shape.
#[test]
fn pointing_follows_what_a_press_would_do() {
    use crate::tools::{Pointing, TOL_PX};
    use kition_slide_geom::Point;
    let mut s = session();
    let tol = TOL_PX;
    let p = |x: f64, y: f64| Some(Point::new(x, y));
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let star = s.execute("shape.insert", &json!({"preset": "star5", "rect": [100, 100, 200, 200]})).unwrap()["id"].as_u64().unwrap();
    let rect = s.execute("shape.insert", &json!({"preset": "rect", "rect": [500, 100, 200, 100], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    assert_eq!(s.pointing(p(20.0, 400.0), tol), Pointing::Default, "the empty slide");
    assert_eq!(s.pointing(p(200.0, 200.0), tol), Pointing::Move, "an unselected shape moves");
    s.execute("edit.select", &json!({"ids": [star]})).unwrap();
    assert_eq!(s.pointing(p(200.0, 200.0), tol), Pointing::Move, "a selected star without text is no text");
    assert_eq!(s.pointing(p(300.0, 200.0), tol), Pointing::Resize { angle: 0.0 }, "E handle");
    assert_eq!(s.pointing(p(300.0, 300.0), tol), Pointing::Resize { angle: 45.0 }, "SE handle");
    assert_eq!(s.pointing(p(200.0, 100.0 - 18.0), tol), Pointing::Rotate);
    // Turned by 30°: the E handle pulls at 30°.
    s.execute("shape.rotate", &json!({"ids": [star], "deg": 30})).unwrap();
    let turned = {
        let st = s.doc().unwrap();
        crate::cmd::xfrm_of(&st.doc, &st.selection, st.shape(kition_slide_model::ShapeId(star as u32)).unwrap())
    };
    assert!((turned.rot - 30.0).abs() < 1e-6, "{turned:?}");
    let (handles, rotate) = crate::tools::handles(&turned, 1.0);
    match s.pointing(p(handles[3].x, handles[3].y), tol) {
        Pointing::Resize { angle } => assert!((angle - 30.0).abs() < 1e-6, "{angle}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(s.pointing(p(rotate.x, rotate.y), tol), Pointing::Rotate, "the turned rotation handle");
    // The selected rectangle with text: the text cursor on its words, the move cursor beside them.
    s.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    assert_eq!(s.pointing(p(600.0, 150.0), tol), Pointing::Text, "on its text");
    assert_eq!(s.pointing(p(510.0, 110.0), tol), Pointing::Move, "in its corner, away from the text");
    // Format Painter loaded: the copy cursor.
    s.execute("format.painter", &json!({})).unwrap();
    assert_eq!(s.pointing(p(200.0, 200.0), tol), Pointing::Copy);
    s.execute("edit.deselectAll", &json!({})).unwrap();
    s.painter = None;
    s.set_tool(ToolKind::Shape { preset: "rect".into() });
    assert_eq!(s.pointing(p(20.0, 400.0), tol), Pointing::Draw);
    s.set_tool(ToolKind::TextBox);
    assert_eq!(s.pointing(p(20.0, 400.0), tol), Pointing::TextTool);
}

/// WPS W-Q1 (case regressions/issue-hand-q1-delete-nothing): with nothing selected on the slide,
/// Delete deletes nothing, even with the slide chosen; `scope: "slides"` still deletes it.
#[test]
fn delete_with_nothing_selected_keeps_the_slide() {
    let mut s = session();
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.go", &json!({"index": 1})).unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    assert!(!s.doc().unwrap().selection.slides.is_empty(), "the slide is chosen");
    s.key("Delete", Mods::default()).unwrap();
    s.execute("edit.delete", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 2);
    s.execute("edit.delete", &json!({"scope": "slides"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 1);
}

/// Walkthrough D3 (plan 0B's pointer.mjs): a Shift drag stays on its axis, even where the grid or
/// a guide would pull the other way.
#[test]
fn shift_drag_keeps_its_axis_against_the_snap() {
    let mut s = session();
    s.prefs.snap_to_grid = true;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [331, 201, 200, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let shift = Mods { shift: true, ..Mods::default() };
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: shift, tol: 4.0 };
    s.pointer(ev(PointerKind::Down, 430.0, 250.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 520.0, 280.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 580.0, 280.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 580.0, 280.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["y"], 201.0, "{v}");
    assert!(v["box"]["x"].as_f64().unwrap() > 400.0, "{v}");
}

/// Plan stage 1 (WPS W-A12, W-P2; case regressions/issue-hand-g5-align-direct): Cmd+G groups and
/// Cmd+Shift+G ungroups; a Format Painter kept loaded (a double click) paints shape after shape
/// until Escape puts it down, and Escape then leaves the selection alone.
#[test]
fn group_keys_and_a_kept_painter_put_down_by_escape() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 80]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 100, 100, 80]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let cmd = Mods { cmd: true, ..Mods::default() };
    s.key("g", cmd).unwrap();
    let top = |s: &Session| s.doc().unwrap().doc.slides[1].shapes.len();
    assert_eq!(top(&s), 1, "Cmd+G groups the two");
    s.key("g", Mods { shift: true, ..cmd }).unwrap();
    assert_eq!(top(&s), 2, "Cmd+Shift+G ungroups them");
    s.key("g", Mods { alt: true, ..cmd }).unwrap();
    assert_eq!(top(&s), 2, "Cmd+Alt+G is no longer Group");

    let c = s.execute("shape.insert", &json!({"preset": "rect", "rect": [500, 100, 100, 80]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.execute("shape.fill", &json!({"color": "#FF0000"})).unwrap();
    s.execute("format.painter", &json!({"sticky": true})).unwrap();
    let click = |s: &mut Session, x: f64, y: f64| {
        let ev = |kind| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
        s.pointer(ev(PointerKind::Down)).unwrap();
        s.pointer(ev(PointerKind::Up)).unwrap();
    };
    let fill = |s: &mut Session, id: u64| s.execute("shape.inspect", &json!({"id": id})).unwrap()["shape"]["fill"].to_string();
    let red = fill(&mut s, a);
    click(&mut s, 350.0, 140.0);
    click(&mut s, 550.0, 140.0);
    assert_eq!(fill(&mut s, b), red, "the kept painter paints the first shape");
    assert_eq!(fill(&mut s, c), red, "and the next");
    let selected = s.doc().unwrap().selection.shapes.clone();
    assert_eq!(s.key("Escape", Mods::default()).unwrap()["handled"], true);
    assert!(s.painter.is_none(), "Escape puts the painter down");
    assert_eq!(s.doc().unwrap().selection.shapes, selected, "and leaves the selection alone");
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.execute("shape.fill", &json!({"color": "#00FF00"})).unwrap();
    let green = fill(&mut s, b);
    s.execute("edit.deselectAll", &json!({})).unwrap();
    click(&mut s, 350.0, 140.0);
    assert_eq!(fill(&mut s, b), green, "a click after Escape paints nothing");
}

// Stage 2 of the slide-by-hand plan: the undo steps of typing and nudging (walkthrough I2, I4;
// WPS W-H1, W-H2, W-R2). A run of keystrokes of one kind is one undo step, whatever its length
// and however long the person paused (the engine has no clock: nothing but another action ends a
// run); moving the caret, a click, Enter, another command, leaving the text, another shape, an
// agent's edit and a merged draft end it.

/// A new blank slide with an empty text box being edited (as the text box tool leaves it);
/// answers the box's id.
fn typing_box(s: &mut Session) -> u64 {
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("textBox.insert", &json!({"rect": [100, 100, 400, 40]})).unwrap()["id"].as_u64().unwrap()
}

/// Types `text` one key at a time, as the editor's text sink sends it.
fn type_keys(s: &mut Session, text: &str) {
    for c in text.chars() {
        s.type_text(&c.to_string()).unwrap();
    }
}

fn press(s: &mut Session, key: &str) {
    s.key(key, Mods::default()).unwrap();
}

fn text_of(s: &mut Session, id: u64) -> String {
    s.execute("text.get", &json!({"id": id})).unwrap()["text"].as_str().unwrap().to_owned()
}

fn undo_steps(s: &Session) -> usize {
    s.doc().unwrap().history.undo.len()
}

/// The replica's own undo steps.
fn replica_undo_steps(s: &Session) -> usize {
    s.kdm().map_or(0, |k| k.steps().0)
}

fn undo(s: &mut Session) {
    s.execute("edit.undo", &json!({})).unwrap();
}

fn redo(s: &mut Session) {
    s.execute("edit.redo", &json!({})).unwrap();
}

#[test]
fn typing_is_one_undo_step() {
    let mut s = session();
    let id = typing_box(&mut s);
    let (steps, replica) = (undo_steps(&s), replica_undo_steps(&s));
    type_keys(&mut s, "one two three");
    assert_eq!(text_of(&mut s, id), "one two three");
    assert_eq!(undo_steps(&s), steps + 1, "one history step for the whole run");
    assert_eq!(replica_undo_steps(&s), replica + 1, "one replica step");
    assert_eq!(s.doc().unwrap().history.undo.last().map(|e| e.steps), Some(1));
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "", "one undo takes the whole run back (WPS W-H1)");
    redo(&mut s);
    assert_eq!(text_of(&mut s, id), "one two three", "one redo brings it back");
}

#[test]
fn text_typed_over_a_selection_takes_its_first_characters_formatting() {
    // Plan stage 6B (WPS W-J1): a word made bold from the floating toolbar, then typed over, stays
    // bold; the typed characters used to take the formatting of the text before the selection.
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "Hello world");
    s.execute("text.selectWord", &json!({"at": [0, 8]})).unwrap();
    s.execute("format.bold", &json!({})).unwrap();
    type_keys(&mut s, "Zed");
    assert_eq!(text_of(&mut s, id), "Hello Zed");
    let shape = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    let runs = shape["shape"]["text"]["paragraphs"][0]["runs"].as_array().cloned().unwrap_or_default();
    let bold: String = runs.iter().filter(|r| r["props"]["bold"] == json!(true)).filter_map(|r| r["text"].as_str()).collect();
    assert_eq!(bold, "Zed", "{runs:?}");
}

#[test]
fn a_run_longer_than_the_replicas_step_limit_is_still_one_step() {
    let mut s = session();
    let id = typing_box(&mut s);
    let long = "abcdefghij ".repeat(15);
    type_keys(&mut s, &long);
    assert_eq!(text_of(&mut s, id), long);
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "", "165 keystrokes, one undo");
    redo(&mut s);
    assert_eq!(text_of(&mut s, id), long);
}

#[test]
fn moving_the_caret_and_switching_between_typing_and_deleting_end_the_run() {
    // WPS W-H2: type " dd", move the caret, type " ee", Backspace twice, type "XY"; four undos
    // take back "XY", the two deleted characters, " ee" and " dd" in turn.
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, " dd");
    press(&mut s, "ArrowLeft");
    press(&mut s, "ArrowRight");
    type_keys(&mut s, " ee");
    press(&mut s, "Backspace");
    press(&mut s, "Backspace");
    type_keys(&mut s, "XY");
    assert_eq!(text_of(&mut s, id), " dd XY");
    let mut seen = Vec::new();
    for _ in 0..4 {
        undo(&mut s);
        seen.push(text_of(&mut s, id));
    }
    assert_eq!(seen, [" dd ", " dd ee", " dd", ""]);
}

#[test]
fn backspaces_in_a_row_are_one_step() {
    let mut s = session();
    let id = typing_box(&mut s);
    s.type_text("Hello world").unwrap();
    let steps = undo_steps(&s);
    for _ in 0..3 {
        press(&mut s, "Backspace");
    }
    press(&mut s, "Delete");
    assert_eq!(text_of(&mut s, id), "Hello wo");
    assert_eq!(undo_steps(&s), steps + 1, "Backspace and Delete in a row: one step");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "Hello world");
}

#[test]
fn enter_is_a_step_of_its_own() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    press(&mut s, "Enter");
    type_keys(&mut s, "cd");
    assert_eq!(text_of(&mut s, id), "ab\ncd");
    let mut seen = Vec::new();
    for _ in 0..3 {
        undo(&mut s);
        seen.push(text_of(&mut s, id));
    }
    assert_eq!(seen, ["ab\n", "ab", ""]);
}

#[test]
fn another_command_ends_the_run() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    let r = s.key("b", Mods { cmd: true, ..Mods::default() }).unwrap();
    assert_eq!(r["command"], "format.bold");
    type_keys(&mut s, "cd");
    assert_eq!(text_of(&mut s, id), "abcd");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "ab", "the typing after the formatting is a step of its own");
}

#[test]
fn leaving_the_text_or_going_to_another_shape_ends_the_run() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    // Out of the text and back to the very same caret (the end of the text).
    press(&mut s, "Escape");
    s.execute("text.edit", &json!({"id": id})).unwrap();
    type_keys(&mut s, "cd");
    // The same, through commands only (an editor's Escape, a click on the text).
    s.execute("text.exit", &json!({})).unwrap();
    s.execute("text.edit", &json!({"id": id})).unwrap();
    type_keys(&mut s, "ef");
    // Another shape, and back.
    let other = s.execute("textBox.insert", &json!({"rect": [100, 300, 400, 40]})).unwrap()["id"].as_u64().unwrap();
    type_keys(&mut s, "x");
    s.execute("text.edit", &json!({"id": id})).unwrap();
    type_keys(&mut s, "gh");
    assert_eq!(text_of(&mut s, id), "abcdefgh");
    assert_eq!(text_of(&mut s, other), "x");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "abcdef");
    undo(&mut s);
    assert_eq!(text_of(&mut s, other), "");
    undo(&mut s);
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "abcd");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "ab");
}

#[test]
fn a_click_ends_the_run_even_where_the_caret_was() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    // A press with nothing under it changes the selection; the run ends at the press itself.
    let ev = |kind| PointerEvent { kind, x: 900.0, y: 500.0, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down)).unwrap();
    s.pointer(ev(PointerKind::Up)).unwrap();
    s.execute("text.edit", &json!({"id": id})).unwrap();
    type_keys(&mut s, "cd");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "ab");
}

#[test]
fn an_input_methods_commit_is_a_step_of_its_own() {
    // WPS W-R2: commit "你好", then "中"; one undo takes back only "中".
    let mut s = session();
    let id = typing_box(&mut s);
    s.type_text("你好").unwrap();
    s.type_text("中").unwrap();
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "你好");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "");
}

#[test]
fn an_agents_typing_is_never_joined() {
    let mut s = session();
    let id = typing_box(&mut s);
    let agent = Origin::agent("test");
    let steps = undo_steps(&s);
    let previous = s.set_origin(Some(agent));
    type_keys(&mut s, "abc");
    s.set_origin(previous);
    assert_eq!(text_of(&mut s, id), "abc");
    assert_eq!(undo_steps(&s), steps + 3, "each of the agent's keystrokes is its own step");
}

#[test]
fn held_arrow_keys_nudge_in_one_step() {
    let mut s = session();
    // Snap to grid on: an arrow key moves one 6 pt grid step (plan stage 8, WPS W-D1).
    s.prefs.snap_to_grid = true;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 80]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 100, 100, 80]})).unwrap()["id"].as_u64().unwrap();
    let x = |s: &mut Session, id: u64| s.execute("shape.inspect", &json!({"id": id})).unwrap()["box"]["x"].as_f64().unwrap();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    let steps = undo_steps(&s);
    for _ in 0..10 {
        press(&mut s, "ArrowRight");
    }
    press(&mut s, "ArrowLeft");
    assert_eq!(x(&mut s, a), 154.0);
    assert_eq!(undo_steps(&s), steps + 1, "a held arrow key (and a turn back) is one step");
    // Another selection: a step of its own.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    press(&mut s, "ArrowDown");
    press(&mut s, "ArrowDown");
    // Back to the first shape: a new run, not the old one.
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    press(&mut s, "ArrowRight");
    assert_eq!(undo_steps(&s), steps + 3);
    undo(&mut s);
    assert_eq!(x(&mut s, a), 154.0);
    undo(&mut s);
    assert_eq!(s.execute("shape.inspect", &json!({"id": b})).unwrap()["box"]["y"].as_f64(), Some(100.0));
    undo(&mut s);
    assert_eq!(x(&mut s, a), 100.0, "one undo takes all the first run's nudges back");
}

/// Plan stage 3 (walkthrough E3; case regressions/issue-hand-e3-cmd-arrows): the Mac's text keys.
#[test]
fn cmd_arrows_go_to_the_ends_of_the_line_and_the_text() {
    let mut s = session();
    let id = typing_box(&mut s);
    s.execute("text.edit", &json!({"id": id})).unwrap();
    s.execute("text.insert", &json!({"text": "one two\nthree four"})).unwrap();
    let cmd = Mods { cmd: true, ..Mods::default() };
    let caret = |s: &Session| {
        let t = s.doc().unwrap().selection.text.clone().unwrap();
        (t.anchor, t.caret)
    };
    s.execute("text.select", &json!({"anchor": [1, 3], "caret": [1, 3]})).unwrap();
    s.key("ArrowLeft", cmd).unwrap();
    assert_eq!(caret(&s), ((1, 0), (1, 0)), "Cmd+Left: the line's start");
    s.key("ArrowRight", cmd).unwrap();
    assert_eq!(caret(&s), ((1, 10), (1, 10)), "Cmd+Right: the line's end");
    s.key("ArrowUp", cmd).unwrap();
    assert_eq!(caret(&s), ((0, 0), (0, 0)), "Cmd+Up: the text's start");
    s.key("ArrowDown", cmd).unwrap();
    assert_eq!(caret(&s), ((1, 10), (1, 10)), "Cmd+Down: the text's end");
    s.key("ArrowLeft", Mods { shift: true, ..cmd }).unwrap();
    assert_eq!(caret(&s), ((1, 10), (1, 0)), "Shift+Cmd+Left extends to the line's start");
    s.key("Home", cmd).unwrap();
    assert_eq!(caret(&s), ((0, 0), (0, 0)), "Cmd+Home: the text's start");
    s.execute("text.select", &json!({"anchor": [1, 5], "caret": [1, 5]})).unwrap();
    s.key("Backspace", cmd).unwrap();
    assert_eq!(text_of(&mut s, id), "one two\n four", "Cmd+Backspace deletes to the line's start");
    s.key("Delete", cmd).unwrap();
    assert_eq!(text_of(&mut s, id), "one two\n", "Cmd+Delete deletes to the line's end");
    s.key("Backspace", cmd).unwrap();
    assert_eq!(text_of(&mut s, id), "one two", "at the line's start it joins the lines");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "one two\n", "each Cmd+Backspace is a step of its own");
}

/// Plan stage 3 (WPS W-F2, W-F3; walkthrough C8, D12, R8; cases regressions/issue-hand-c8-click-shape-text,
/// regressions/issue-hand-d12-drag-on-text-selects): a press on a shape's text puts the caret there at
/// once and a drag from it selects text; anywhere else on the shape it takes the object, as the
/// pointer says; the band along an edited text box's frame moves the box.
#[test]
fn a_press_on_text_edits_and_elsewhere_takes_the_object() {
    use crate::tools::{Pointing, TOL_PX};
    use kition_slide_geom::Point;
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let rect = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 300, 100], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    let boxed = s.execute("textBox.insert", &json!({"rect": [100, 300, 400, 40], "text": "Drag over these words"})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: TOL_PX };
    let click = |s: &mut Session, x: f64, y: f64| {
        s.pointer(ev(PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(PointerKind::Up, x, y)).unwrap();
    };
    let editing = |s: &Session| s.doc().unwrap().selection.text.as_ref().map(|t| t.shape.0 as u64);
    let p = |x: f64, y: f64| Some(Point::new(x, y));
    assert_eq!(s.pointing(p(250.0, 150.0), TOL_PX), Pointing::Text, "an unselected shape's words: the text cursor");
    click(&mut s, 250.0, 150.0);
    assert_eq!(editing(&s), Some(rect), "the first click on its words puts the caret there");
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes, vec![model::ShapeId(rect as u32)]);
    assert_eq!(s.pointing(p(110.0, 110.0), TOL_PX), Pointing::Move, "away from its words: move");
    click(&mut s, 110.0, 110.0);
    assert_eq!(editing(&s), None, "a click away from the words of the selected shape keeps the object selected");
    assert_eq!(s.doc().unwrap().selection.shapes, vec![model::ShapeId(rect as u32)]);
    // A drag on an unselected text box's words selects them and leaves the box where it is.
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let before = s.execute("shape.inspect", &json!({"id": boxed})).unwrap()["box"].clone();
    s.pointer(ev(PointerKind::Down, 112.0, 320.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 180.0, 320.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 250.0, 320.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 250.0, 320.0)).unwrap();
    let t = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!(t.shape.0 as u64, boxed);
    assert!(t.is_range(), "a drag on the words selects them: {t:?}");
    assert_eq!(s.execute("shape.inspect", &json!({"id": boxed})).unwrap()["box"], before, "and moves nothing");
    // While it is edited, the band along its frame takes the box: a drag there moves it.
    assert_eq!(s.pointing(p(200.0, 302.0), TOL_PX), Pointing::Move, "the frame's band (its top edge, away from the handles): move");
    s.pointer(ev(PointerKind::Down, 200.0, 302.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 230.0, 312.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 250.0, 322.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 250.0, 322.0)).unwrap();
    let after = s.execute("shape.inspect", &json!({"id": boxed})).unwrap()["box"].clone();
    assert_eq!(after["x"].as_f64(), Some(150.0), "{after}");
    assert_eq!(after["y"].as_f64(), Some(320.0), "{after}");
    assert_eq!(editing(&s), None, "the box is selected as an object");
}

/// Plan stage 3 (WPS W-F4, W-F5; cases regressions/issue-hand-e9-enter-selects-all,
/// regressions/issue-hand-e9-typing-at-last-caret): Enter and F2 edit a selected shape's text with
/// all of it selected; typing on it goes where its caret last was, at its end if it was never
/// edited, and keeps the text.
#[test]
fn enter_selects_all_and_typing_goes_on_at_the_last_caret() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 300, 100], "text": "Back"})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.type_text("H").unwrap();
    s.type_text("i").unwrap();
    assert_eq!(text_of(&mut s, a), "BackHi", "never edited: at the end, the text kept");
    s.key("Escape", Mods::default()).unwrap();
    s.execute("text.edit", &json!({"id": a, "at": [0, 2]})).unwrap();
    s.key("Escape", Mods::default()).unwrap();
    assert!(s.doc().unwrap().selection.text.is_none());
    s.type_text("X").unwrap();
    assert_eq!(text_of(&mut s, a), "BaXckHi", "where the caret last was");
    s.key("Escape", Mods::default()).unwrap();
    s.key("Enter", Mods::default()).unwrap();
    let t = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!(t.ordered(), ((0, 0), (0, 7)), "Enter selects all of it");
    s.type_text("Z").unwrap();
    assert_eq!(text_of(&mut s, a), "Z", "and typing replaces it");
    s.key("Escape", Mods::default()).unwrap();
    s.key("F2", Mods::default()).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.clone().unwrap().ordered(), ((0, 0), (0, 1)), "F2 as Enter");
    // A selected picture or line takes no typing: the keys stay the editor's.
    s.key("Escape", Mods::default()).unwrap();
    let line = s.execute("shape.insert", &json!({"preset": "line", "rect": [100, 300, 200, 0]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [line]})).unwrap();
    assert!(s.doc().unwrap().typing_target().is_none());
    assert_eq!(s.type_text("q").unwrap()["handled"], false);
}

/// Plan stage 3 (walkthrough E11, WPS W-E3; case regressions/issue-hand-e11-empty-textbox): a text
/// box the Text Box tool made and the person left without typing goes, leaving no undo or redo step.
#[test]
fn an_empty_text_box_left_goes_without_a_step() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let kept = s.execute("shape.insert", &json!({"preset": "rect", "rect": [600, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let shapes = |s: &Session| s.doc().unwrap().shapes().len();
    let (n, steps) = (shapes(&s), undo_steps(&s));
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 4.0 };
    let click = |s: &mut Session, x: f64, y: f64| {
        s.pointer(ev(PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(PointerKind::Up, x, y)).unwrap();
    };
    s.set_tool(ToolKind::TextBox);
    click(&mut s, 200.0, 300.0);
    assert!(s.doc().unwrap().selection.text.is_some(), "the click made a box to type in");
    assert_eq!(shapes(&s), n + 1);
    // A click on another shape leaves it: the box goes, the other shape is selected.
    click(&mut s, 650.0, 150.0);
    assert_eq!(shapes(&s), n, "the empty box went");
    assert_eq!(undo_steps(&s), steps, "no undo step");
    assert!(s.doc().unwrap().history.redo.is_empty(), "no redo step");
    assert_eq!(s.doc().unwrap().selection.shapes, vec![model::ShapeId(kept as u32)]);
    // Escape leaves it too; a box typed in stays.
    s.set_tool(ToolKind::TextBox);
    click(&mut s, 200.0, 300.0);
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(shapes(&s), n, "Escape: the empty box went");
    s.set_tool(ToolKind::TextBox);
    click(&mut s, 200.0, 300.0);
    s.type_text("a").unwrap();
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(shapes(&s), n + 1, "a box typed in stays");
}

/// Plan stage 3 (walkthrough E6; case regressions/issue-hand-e6-empty-bullet): an empty bulleted
/// paragraph keeps its bullet's room (the caret is where its text will be) and, while edited, shows
/// the bullet; Backspace at its start takes the bullet away, the next joins the paragraphs.
#[test]
fn an_empty_bullet_keeps_its_place_and_backspace_takes_it_away() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "titleAndContent", "title": "T", "body": "B"})).unwrap();
    let body = s.doc().unwrap().current_slide().unwrap().shapes[1].id.0 as u64;
    s.execute("text.edit", &json!({"id": body})).unwrap();
    s.key("Enter", Mods::default()).unwrap();
    let x_of = |s: &Session, pos: (usize, usize)| {
        let st = s.doc().unwrap();
        let t = st.selection.text.clone().unwrap();
        let layout = cmd::text::layout_for(st, &t).unwrap();
        layout.caret(kition_slide_text::Pos { para: pos.0, ch: pos.1 }).unwrap().0
    };
    let (full, empty) = (x_of(&s, (0, 0)), x_of(&s, (1, 0)));
    assert!((full - empty).abs() < 0.01, "the empty item's caret is where the text of a bulleted one starts ({empty} vs {full})");
    let paras = |s: &Session| s.doc().unwrap().shape(model::ShapeId(body as u32)).unwrap().text.as_ref().unwrap().paragraphs.clone();
    s.key("Backspace", Mods::default()).unwrap();
    let after = paras(&s);
    assert_eq!(after.len(), 2, "the first Backspace keeps the paragraph");
    assert_eq!(after[1].props.bullet, Some(model::text::Bullet::None), "and takes its bullet away");
    s.key("Backspace", Mods::default()).unwrap();
    assert_eq!(paras(&s).len(), 1, "the next joins it to the one before");
}

// Stage 4 of the slide-by-hand plan: an input method's composition (walkthrough E10; WPS W-R1,
// W-R2). It shows in the text being edited without changing the document; what it commits is one
// undo step of its own.

/// The text of shape `id` in the slide file as the editor shows it while composing.
fn shown_text(s: &Session, id: u64) -> String {
    let p = s.preedit().expect("a composition shows");
    let st = s.doc().unwrap();
    let shape = kition_slide_model::find_shape(shapes_of(&p.shown, &st.selection), kition_slide_model::ShapeId(id as u32)).unwrap();
    shape.text.as_ref().unwrap().paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_composition_shows_in_the_text_without_changing_the_document() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    let (doc, steps, dirty) = (s.doc().unwrap().doc.clone(), undo_steps(&s), s.doc().unwrap().is_dirty());
    for (typed, cursor) in [("n", 1), ("ni", 2), ("ni hao", 6)] {
        assert!(s.set_preedit(typed, cursor).unwrap(), "composing {typed} shows");
        assert_eq!(shown_text(&s, id), format!("ab{typed}"), "at the caret");
    }
    let p = s.preedit().unwrap();
    assert_eq!((p.start, p.end, p.cursor), ((0, 2), (0, 8), (0, 8)));
    assert!(std::sync::Arc::ptr_eq(&s.doc().unwrap().doc, &doc), "the document is as it was");
    assert_eq!(undo_steps(&s), steps, "no undo step");
    assert_eq!(s.doc().unwrap().is_dirty(), dirty, "nor unsaved");
    assert_eq!(text_of(&mut s, id), "ab");
    // While it is drawn, the caret is at the input method's cursor in the shown text.
    let shown = s.show_preedit().unwrap();
    let caret = s.doc().unwrap().selection.text.as_ref().map(|t| t.caret);
    s.unshow_preedit(shown);
    assert_eq!(caret, Some((0, 8)));
    assert_eq!(s.doc().unwrap().selection.text.as_ref().map(|t| t.caret), Some((0, 2)), "and back after it");
    assert!(std::sync::Arc::ptr_eq(&s.doc().unwrap().doc, &doc));
    // Escape in the input method empties the composition: nothing was typed.
    assert!(!s.set_preedit("", 0).unwrap());
    assert!(s.preedit().is_none());
    assert!(std::sync::Arc::ptr_eq(&s.doc().unwrap().doc, &doc));
    assert_eq!(undo_steps(&s), steps);
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "", "the one undo step is still the typing before");
}

#[test]
fn a_committed_character_is_its_own_undo_step_between_typing() {
    // WPS W-H1 and W-R2: "ab" typed, "中" committed by an input method, "cd" typed: three steps.
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    s.set_preedit("zhong", 5).unwrap();
    s.type_text("中").unwrap();
    assert!(s.preedit().is_none(), "the commit ends the composition");
    type_keys(&mut s, "cd");
    assert_eq!(text_of(&mut s, id), "ab中cd");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "ab中");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "ab");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "");
}

#[test]
fn a_composition_replaces_the_selected_text_only_when_committed() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "hello");
    s.execute("text.selectAll", &json!({})).unwrap();
    s.set_preedit("ni", 2).unwrap();
    assert_eq!(shown_text(&s, id), "ni");
    assert_eq!(text_of(&mut s, id), "hello");
    s.type_text("你").unwrap();
    assert_eq!(text_of(&mut s, id), "你");
    undo(&mut s);
    assert_eq!(text_of(&mut s, id), "hello");
}

#[test]
fn a_composition_hides_when_the_caret_moves_and_leaves_out_line_breaks() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "ab");
    assert!(s.set_preedit("x\ny\u{b}z", 9).unwrap());
    let p = s.preedit().unwrap();
    assert_eq!((p.text.as_str(), p.cursor), ("xyz", (0, 5)), "no line breaks; the cursor within the composition");
    press(&mut s, "ArrowLeft");
    assert!(s.preedit().is_none(), "a moved caret hides it until the input method sends it again");
    assert!(s.set_preedit(&"字".repeat(ime::MOST + 10), 0).unwrap());
    assert_eq!(s.preedit().unwrap().text.chars().count(), ime::MOST);
    assert_eq!(text_of(&mut s, id), "ab");
}

#[test]
fn composing_on_a_selected_shape_edits_its_text_and_with_nothing_to_type_into_shows_nothing() {
    let mut s = session();
    let id = typing_box(&mut s);
    type_keys(&mut s, "abc");
    press(&mut s, "ArrowLeft");
    press(&mut s, "Escape");
    assert!(s.doc().unwrap().selection.text.is_none());
    // WPS W-F5: at the shape's last caret.
    assert!(s.set_preedit("ni", 2).unwrap());
    assert_eq!(shown_text(&s, id), "abnic");
    s.type_text("你").unwrap();
    assert_eq!(text_of(&mut s, id), "ab你c");
    press(&mut s, "Escape");
    s.execute("edit.deselectAll", &json!({})).unwrap();
    // WPS W-R4: nothing selected, nothing edited: no composition shows, nothing changes.
    let doc = s.doc().unwrap().doc.clone();
    assert!(!s.set_preedit("ni", 2).unwrap());
    assert!(s.preedit().is_none());
    assert!(std::sync::Arc::ptr_eq(&s.doc().unwrap().doc, &doc));
}

/// A blank slide with two rectangles (`A` at 100,100 and `B` at 400,100, 200 x 100, each with a
/// word in its middle) and a 2 x 2 table at 100,300 (200 x 80); nothing selected.
fn context_slide() -> (Session, u64, u64, u64) {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "A"})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 100, 200, 100], "text": "B"})).unwrap()["id"].as_u64().unwrap();
    let t = s.execute("table.insert", &json!({"rows": 2, "cols": 2, "rect": [100, 300, 200, 80]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    (s, a, b, t)
}

fn right_click(s: &mut Session, x: f64, y: f64) -> serde_json::Value {
    s.pointer(PointerEvent { kind: PointerKind::Context, x, y, mods: Mods::default(), tol: 3.0 }).unwrap()
}

fn selected_ids(s: &Session) -> Vec<u64> {
    s.doc().unwrap().selection.shapes.iter().map(|id| u64::from(id.0)).collect()
}

#[test]
fn a_right_click_selects_the_object_under_it_and_starts_nothing() {
    let (mut s, a, b, _) = context_slide();
    let steps = s.doc().unwrap().history.undo.len();
    // On an unselected shape (away from its word): that shape alone, an object menu.
    let answer = right_click(&mut s, 120.0, 115.0);
    assert_eq!(selected_ids(&s), vec![a]);
    assert_eq!(answer["menu"], "objects", "{answer}");
    assert_eq!(answer["kinds"][0], "shape", "{answer}");
    assert!(s.doc().unwrap().selection.text.is_none(), "a right-click never enters text");
    assert!(!s.tool.dragging(), "a right-click starts no gesture");
    // On its word too: still the object, no caret.
    right_click(&mut s, 200.0, 150.0);
    assert!(s.doc().unwrap().selection.text.is_none());
    // On another shape: that one instead.
    right_click(&mut s, 420.0, 115.0);
    assert_eq!(selected_ids(&s), vec![b]);
    // On the empty slide: nothing, the slide's menu.
    let answer = right_click(&mut s, 800.0, 480.0);
    assert!(selected_ids(&s).is_empty());
    assert_eq!(answer["menu"], "slide", "{answer}");
    assert_eq!(s.doc().unwrap().history.undo.len(), steps, "a right-click leaves no undo step");
}

#[test]
fn a_right_click_in_the_selection_keeps_it() {
    let (mut s, a, b, _) = context_slide();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let answer = right_click(&mut s, 420.0, 115.0);
    assert_eq!(selected_ids(&s), vec![a, b], "{answer}");
    assert_eq!(answer["ids"], json!([a, b]));
    // On a selected object's handle (its bottom-right corner) too.
    right_click(&mut s, 600.0, 200.0);
    assert_eq!(selected_ids(&s), vec![a, b]);
}

#[test]
fn a_right_click_in_the_text_being_edited_keeps_the_caret() {
    let (mut s, a, _, _) = context_slide();
    s.execute("text.edit", &json!({"id": a, "all": true})).unwrap();
    let before = s.doc().unwrap().selection.text.clone().unwrap();
    let answer = right_click(&mut s, 110.0, 190.0);
    assert_eq!(answer["menu"], "text", "{answer}");
    let after = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!((after.shape, after.anchor, after.caret), (before.shape, before.anchor, before.caret));
    // Outside it: the text is left, as a press there would.
    let answer = right_click(&mut s, 800.0, 480.0);
    assert!(s.doc().unwrap().selection.text.is_none());
    assert_eq!(answer["menu"], "slide");
}

#[test]
fn a_right_click_in_a_table_puts_the_caret_in_its_cell() {
    let (mut s, _, _, t) = context_slide();
    // The bottom-right cell (row 1, column 1).
    let answer = right_click(&mut s, 250.0, 360.0);
    assert_eq!(answer["menu"], "cell", "{answer}");
    let text = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!((u64::from(text.shape.0), text.cell), (t, Some((1, 1))));
    // A row inserted from there goes below that cell's row.
    s.execute("table.insertRowBelow", &json!({})).unwrap();
    let rows = s.execute("table.state", &json!({"id": t})).unwrap()["rows"].as_u64().unwrap();
    assert_eq!(rows, 3);
    // Cells selected stay selected when one of them is right-clicked.
    s.execute("table.selectCells", &json!({"id": t, "from": [0, 0], "to": [0, 1]})).unwrap();
    right_click(&mut s, 150.0, 310.0);
    assert!(s.doc().unwrap().selection.cells.is_some());
}

#[test]
fn a_right_click_with_a_drawing_tool_changes_nothing() {
    let (mut s, a, _, _) = context_slide();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.set_tool(ToolKind::Shape { preset: "rect".into() });
    right_click(&mut s, 800.0, 480.0);
    assert_eq!(selected_ids(&s), vec![a]);
}

/// The titles of the deck's slides, in order.
fn deck_titles(s: &Session) -> Vec<String> {
    s.doc().unwrap().doc.slides.iter().map(|slide| slide.title()).collect()
}

/// A deck of slides titled "1" to "n".
fn titled(n: usize) -> Session {
    let mut s = session();
    let first = s.doc().unwrap().doc.slides[0].id.0;
    for k in 1..=n {
        s.execute("slide.new", &json!({"at": k, "layout": "titleOnly", "title": k.to_string()})).unwrap();
    }
    s.execute("slide.delete", &json!({"ids": [first]})).unwrap();
    s
}

#[test]
fn several_slides_move_together_in_deck_order_as_one_step() {
    // Plan stage 7 (walkthrough B4): the slide panel drags slides 2 to 4 after slide 6.
    let mut s = titled(7);
    assert_eq!(deck_titles(&s), ["1", "2", "3", "4", "5", "6", "7"]);
    let ids: Vec<u32> = s.doc().unwrap().doc.slides[1..4].iter().map(|slide| slide.id.0).collect();
    // Selected in another order: they stay in deck order.
    let given = [ids[2], ids[0], ids[1]];
    s.execute("slide.go", &json!({"index": 2})).unwrap();
    s.execute("slide.move", &json!({"ids": given, "to": 3})).unwrap();
    assert_eq!(deck_titles(&s), ["1", "5", "6", "2", "3", "4", "7"]);
    let st = s.doc().unwrap();
    assert_eq!(st.selection.slide, 4, "the current slide follows its slide");
    assert_eq!(st.selection.slides.iter().map(|id| id.0).collect::<Vec<_>>(), ids, "the moved slides stay selected");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(deck_titles(&s), ["1", "2", "3", "4", "5", "6", "7"], "one undo step");
    // Past the end: after the last slide.
    s.execute("slide.move", &json!({"ids": [ids[0]], "to": 99})).unwrap();
    assert_eq!(deck_titles(&s), ["1", "3", "4", "5", "6", "7", "2"]);
    assert!(s.execute("slide.move", &json!({"ids": [], "to": 0})).is_err());
    assert!(s.execute("slide.move", &json!({"ids": [999_999], "to": 0})).is_err());
    assert!(s.execute("slide.move", &json!({"ids": [ids[0]], "from": 0, "to": 0})).is_err());
}

#[test]
fn slides_copied_together_paste_in_deck_order() {
    // Plan stage 7: slides selected 3 then 1 paste as 1, 3.
    let mut s = titled(3);
    s.execute("slide.selectSlides", &json!({"indices": [2, 0]})).unwrap();
    s.execute("edit.copy", &json!({"scope": "slides"})).unwrap();
    s.execute("slide.go", &json!({"index": 2})).unwrap();
    s.execute("edit.paste", &json!({"scope": "slides"})).unwrap();
    assert_eq!(deck_titles(&s), ["1", "2", "3", "1", "3"]);
}

/// Plan stage 8: a blank slide with `rects` on it (nothing selected), the pointer events a person
/// sends at 100% (4 px of tolerance), and a shape's box.
fn stage8(rects: &[[f64; 4]]) -> (Session, Vec<u64>) {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let ids = rects.iter().map(|r| s.execute("shape.insert", &json!({"preset": "rect", "rect": r})).unwrap()["id"].as_u64().unwrap()).collect();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    (s, ids)
}

fn gesture(s: &mut Session, mods: Mods, points: &[(f64, f64)]) {
    let ev = |kind, (x, y): (f64, f64)| PointerEvent { kind, x, y, mods, tol: crate::tools::TOL_PX };
    let (first, last) = (points[0], points[points.len() - 1]);
    s.pointer(ev(PointerKind::Down, first)).unwrap();
    for p in &points[1..] {
        s.pointer(ev(PointerKind::Drag, *p)).unwrap();
    }
    s.pointer(ev(PointerKind::Up, last)).unwrap();
}

fn box_of(s: &mut Session, id: u64) -> [f64; 5] {
    let b = s.execute("shape.inspect", &json!({"id": id})).unwrap()["box"].clone();
    let f = |k: &str| b[k].as_f64().unwrap_or(0.0);
    [f("x"), f("y"), f("w"), f("h"), f("rot")]
}

/// Plan stage 8 item 1 (WPS W-B3–W-B5; case regressions/issue-hand-d2-snap-not-sticky): on a
/// corner handle Cmd grows the shape about its centre, Alt lets it follow the pointer exactly with
/// the corner across still anchored, Shift keeps its proportions.
#[test]
fn resize_modifiers_follow_wps() {
    let cmd = Mods { cmd: true, ..Mods::default() };
    let alt = Mods { alt: true, ..Mods::default() };
    let shift = Mods { shift: true, ..Mods::default() };
    let (mut s, ids) = stage8(&[[300.0, 200.0, 120.0, 60.0]]);
    let id = ids[0];
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    gesture(&mut s, cmd, &[(420.0, 260.0), (440.0, 280.0), (460.0, 300.0)]);
    let b = box_of(&mut s, id);
    assert!((b[0] + b[2] / 2.0 - 360.0).abs() < 1e-6 && (b[1] + b[3] / 2.0 - 230.0).abs() < 1e-6, "Cmd keeps the centre: {b:?}");
    assert!(b[2] > 190.0 && b[3] > 130.0, "and grows both sides: {b:?}");
    undo(&mut s);
    gesture(&mut s, alt, &[(420.0, 260.0), (430.0, 265.0), (439.0, 271.0)]);
    assert_eq!(box_of(&mut s, id)[..4], [300.0, 200.0, 139.0, 71.0], "Alt: the corner follows the pointer, the opposite one stays");
    undo(&mut s);
    gesture(&mut s, shift, &[(420.0, 260.0), (450.0, 270.0), (480.0, 275.0)]);
    let b = box_of(&mut s, id);
    assert!((b[2] / b[3] - 2.0).abs() < 1e-9 && b[0] == 300.0 && b[1] == 200.0, "Shift keeps the proportions: {b:?}");
    undo(&mut s);
    // No modifier: the dragged corner lands on the grid (6 pt), the opposite corner stays.
    gesture(&mut s, Mods::default(), &[(420.0, 260.0), (430.0, 265.0), (439.0, 271.0)]);
    let b = box_of(&mut s, id);
    assert_eq!(b[..4], [300.0, 200.0, 138.0, 70.0], "the grid pulls only the dragged edges: {b:?}");
}

/// Plan stage 8 items 1–2 (walkthrough D2; WPS W-A2, W-A7, W-A8, W-C3; case
/// regressions/issue-hand-d2-snap-not-sticky): with snap to grid off a drag follows the pointer to
/// the point, a 4 px drag moves; near another shape's centre line it snaps there and draws the
/// guide; Cmd is a plain move that snaps; Alt drags a copy that lands where the pointer puts it.
#[test]
fn a_drag_follows_the_pointer_and_snaps_only_near_a_guide() {
    let (mut s, ids) = stage8(&[[100.0, 100.0, 80.0, 60.0], [400.0, 300.0, 100.0, 50.0]]);
    let (a, b) = (ids[0], ids[1]);
    s.prefs.snap_to_grid = false;
    gesture(&mut s, Mods::default(), &[(140.0, 130.0), (142.0, 130.0), (145.0, 130.0)]);
    assert_eq!(box_of(&mut s, a)[..2], [105.0, 100.0], "5 pt with nothing near: 5 pt");
    undo(&mut s);
    gesture(&mut s, Mods::default(), &[(140.0, 130.0), (144.0, 130.0)]);
    assert_eq!(box_of(&mut s, a)[0], 104.0, "a 4 px drag moves");
    undo(&mut s);
    s.execute("edit.deselectAll", &json!({})).unwrap();
    // A's centre 2 pt from B's: it snaps to it, with the guide drawn while the drag goes on.
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    s.pointer(ev(PointerKind::Down, 140.0, 130.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 300.0, 130.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 448.0, 130.0)).unwrap();
    assert!(s.tool.guides.iter().any(|g| g.vertical && g.pos == 450.0), "the centre line is drawn: {:?}", s.tool.guides);
    s.pointer(ev(PointerKind::Up, 448.0, 130.0)).unwrap();
    assert!(s.tool.guides.is_empty());
    assert_eq!(box_of(&mut s, a)[0], 410.0, "and the centres line up");
    undo(&mut s);
    // Snap to grid on: Cmd still snaps (to the grid here).
    s.prefs.snap_to_grid = true;
    s.execute("shape.move", &json!({"ids": [a], "dx": 1, "dy": 1})).unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    gesture(&mut s, Mods { cmd: true, ..Mods::default() }, &[(141.0, 131.0), (146.0, 131.0), (150.0, 131.0)]);
    assert_eq!(box_of(&mut s, a)[..2], [108.0, 102.0], "a Cmd drag lands on the grid");
    undo(&mut s);
    // Alt: a copy, at the pointer, off the grid; the original stays.
    s.execute("edit.deselectAll", &json!({})).unwrap();
    gesture(&mut s, Mods { alt: true, ..Mods::default() }, &[(141.0, 131.0), (145.0, 131.0), (148.0, 131.0)]);
    assert_eq!(box_of(&mut s, a)[..2], [101.0, 101.0], "the original stays");
    let copy = s.doc().unwrap().selection.shapes.clone();
    assert_eq!(copy.len(), 1);
    assert_ne!(u64::from(copy[0].0), a);
    assert_eq!(box_of(&mut s, u64::from(copy[0].0))[..2], [108.0, 101.0], "the copy lands at the pointer");
    let _ = b;
}

/// Plan stage 8 item 4 (walkthrough D5; WPS W-D1–W-D4; case regressions/issue-hand-d5-arrow-steps):
/// an arrow key moves one grid step with snap to grid on and 0.75 pt with it off; Shift and Alt
/// change nothing; Cmd moves 10 pt either way.
#[test]
fn arrow_steps_follow_the_grid() {
    let (mut s, ids) = stage8(&[[100.0, 100.0, 80.0, 60.0]]);
    let id = ids[0];
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    let steps = undo_steps(&s);
    let x = |s: &mut Session| box_of(s, id)[0];
    let step = |s: &mut Session, mods: Mods| {
        let before = x(s);
        s.key("ArrowRight", mods).unwrap();
        x(s) - before
    };
    assert_eq!(step(&mut s, Mods::default()), 6.0, "one grid step");
    assert_eq!(step(&mut s, Mods { shift: true, ..Mods::default() }), 6.0, "Shift: the same");
    assert_eq!(step(&mut s, Mods { alt: true, ..Mods::default() }), 6.0, "Alt: the same");
    assert_eq!(step(&mut s, Mods { cmd: true, ..Mods::default() }), 10.0, "Cmd: 10 pt");
    s.prefs.snap_to_grid = false;
    assert_eq!(step(&mut s, Mods::default()), 0.75, "grid off: 0.75 pt");
    assert_eq!(step(&mut s, Mods { alt: true, ..Mods::default() }), 0.75, "Alt: the same");
    assert_eq!(step(&mut s, Mods { cmd: true, ..Mods::default() }), 10.0, "Cmd: 10 pt");
    assert_eq!(undo_steps(&s), steps + 1, "the nudges of one selection are one step");
}

/// Plan stage 8 item 5 (WPS W-I1, W-I3, W-I4): paste and Duplicate offset by 10 pt; Duplicate
/// again on copies the person moved repeats that move.
#[test]
fn paste_and_duplicate_offset_10_and_duplicate_repeats_the_move() {
    let (mut s, ids) = stage8(&[[100.0, 100.0, 80.0, 60.0]]);
    let id = ids[0];
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.execute("edit.copy", &json!({})).unwrap();
    let pasted = |s: &mut Session| s.doc().unwrap().selection.shapes[0].0 as u64;
    s.execute("edit.paste", &json!({})).unwrap();
    let p1 = pasted(&mut s);
    assert_eq!(box_of(&mut s, p1)[..2], [110.0, 110.0]);
    s.execute("edit.paste", &json!({})).unwrap();
    let p2 = pasted(&mut s);
    assert_eq!(box_of(&mut s, p2)[..2], [120.0, 120.0]);
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.execute("edit.duplicate", &json!({})).unwrap();
    let d1 = pasted(&mut s);
    assert_eq!(box_of(&mut s, d1)[..2], [110.0, 110.0], "Cmd+D: 10 pt");
    s.execute("shape.move", &json!({"dx": 30, "dy": 0})).unwrap();
    s.execute("edit.duplicate", &json!({})).unwrap();
    let d2 = pasted(&mut s);
    assert_eq!(box_of(&mut s, d2)[..2], [180.0, 120.0], "again on the moved copy: the copy's offset (40, 10)");
    s.execute("edit.duplicate", &json!({})).unwrap();
    let d3 = pasted(&mut s);
    assert_eq!(box_of(&mut s, d3)[..2], [220.0, 130.0], "and again");
    // Another selection: the plain offset.
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.execute("edit.duplicate", &json!({})).unwrap();
    let d4 = pasted(&mut s);
    assert_eq!(box_of(&mut s, d4)[..2], [110.0, 110.0]);
}

/// Plan stage 8 items 1 and 6 (WPS W-B6, W-E8): a free rotation lands on a whole degree, Shift on
/// 15°; a shape drawn with Cmd grows from its centre, with Shift and Cmd a square about the press.
#[test]
fn rotation_is_whole_degrees_and_cmd_draws_from_the_centre() {
    let (mut s, ids) = stage8(&[[300.0, 200.0, 200.0, 100.0]]);
    let id = ids[0];
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    let (_, rotate) = crate::tools::handles(&kition_slide_geom::Xfrm::new(300.0, 200.0, 200.0, 100.0), 1.0);
    let centre = (400.0, 250.0);
    let r = (rotate.y - centre.1).abs();
    let at = |deg: f64| {
        let a = (deg - 90.0).to_radians();
        (centre.0 + r * a.cos(), centre.1 + r * a.sin())
    };
    gesture(&mut s, Mods::default(), &[(rotate.x, rotate.y), at(10.0), at(20.3)]);
    assert_eq!(box_of(&mut s, id)[4], 20.0, "a whole degree");
    undo(&mut s);
    gesture(&mut s, Mods { shift: true, ..Mods::default() }, &[(rotate.x, rotate.y), at(10.0), at(20.3)]);
    assert_eq!(box_of(&mut s, id)[4], 15.0, "Shift: 15° steps");
    s.execute("edit.deselectAll", &json!({})).unwrap();
    s.set_tool(ToolKind::Shape { preset: "ellipse".into() });
    let cmd = Mods { cmd: true, ..Mods::default() };
    gesture(&mut s, cmd, &[(600.0, 400.0), (620.0, 410.0), (640.0, 420.0)]);
    let drawn = s.doc().unwrap().selection.shapes[0].0 as u64;
    assert_eq!(box_of(&mut s, drawn)[..4], [560.0, 380.0, 80.0, 40.0], "Cmd: from the centre");
    s.set_tool(ToolKind::Shape { preset: "ellipse".into() });
    gesture(&mut s, Mods { shift: true, ..cmd }, &[(600.0, 400.0), (620.0, 410.0), (640.0, 420.0)]);
    let drawn = s.doc().unwrap().selection.shapes[0].0 as u64;
    assert_eq!(box_of(&mut s, drawn)[..4], [560.0, 360.0, 80.0, 80.0], "Shift+Cmd: a square about the press");
}

/// Plan stage 8 item 7 (WPS W-C2): a shape dragged to within reach of the spacing of the two before
/// it lands at that spacing, and the equal gaps are marked while the drag goes on; between two
/// shapes it centres in the room between them.
#[test]
fn a_drag_snaps_to_an_equal_spacing_and_marks_the_gaps() {
    let (mut s, ids) = stage8(&[[100.0, 100.0, 60.0, 60.0], [200.0, 100.0, 60.0, 60.0], [330.0, 100.0, 60.0, 60.0]]);
    s.prefs.snap_to_grid = false;
    let c = ids[2];
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    s.pointer(ev(PointerKind::Down, 360.0, 110.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 340.0, 110.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 332.0, 110.0)).unwrap();
    let marks: Vec<_> = s.tool.guides.iter().filter(|g| !g.vertical).map(|g| (g.from, g.to)).collect();
    assert!(marks.contains(&(160.0, 200.0)) && marks.contains(&(260.0, 300.0)), "both 40 pt gaps are marked: {:?}", s.tool.guides);
    s.pointer(ev(PointerKind::Up, 332.0, 110.0)).unwrap();
    assert_eq!(box_of(&mut s, c)[0], 300.0, "the third lands 40 pt after the second, as the second after the first");
    undo(&mut s);
    // Between the first two shapes' neighbours: a shape 2 pt off the middle of the room centres in it.
    s.execute("shape.move", &json!({"ids": [c], "x": 600, "y": 100})).unwrap();
    s.execute("shape.move", &json!({"ids": [ids[1]], "x": 400, "y": 100})).unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    // Room from 160 to 400 (240 pt) for 60 pt: 90 pt each side, so x 250.
    s.pointer(ev(PointerKind::Down, 610.0, 110.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 400.0, 110.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 262.0, 110.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 262.0, 110.0)).unwrap();
    assert_eq!(box_of(&mut s, c)[0], 250.0, "centred between its neighbours");
}

/// Plan stage 9 item 4 (WPS W-E1, W-E2, W-E4; case regressions/issue-hand-e1-text-box-width): a
/// click of the Text Box tool makes a 320 pt box at the press, one line high and wrapping; a drag
/// makes it as wide as the drag; the tool goes back to Select after one box, and Escape puts it down.
#[test]
fn the_text_box_tool_makes_320_pt_boxes_and_drags_set_the_width() {
    let (mut s, _) = stage8(&[]);
    s.set_tool(ToolKind::TextBox);
    gesture(&mut s, Mods::default(), &[(100.0, 100.0)]);
    let id = s.doc().unwrap().selection.text.as_ref().unwrap().shape.0 as u64;
    let b = box_of(&mut s, id);
    assert_eq!(b[..3], [100.0, 100.0, 320.0], "320 pt wide, its top left at the click");
    assert!(b[3] > 10.0 && b[3] < 40.0, "one line high ({})", b[3]);
    assert!(matches!(s.tool.kind, ToolKind::Select), "one box, then Select");
    s.type_text("A line long enough to go past the right edge of a box three hundred and twenty points wide").unwrap();
    let wrapped = box_of(&mut s, id);
    assert_eq!(wrapped[2], 320.0, "the width stays");
    assert!(wrapped[3] > b[3] * 1.5, "the text wraps and the box grows down ({} → {})", b[3], wrapped[3]);
    s.key("Escape", Mods::default()).unwrap();
    s.key("Escape", Mods::default()).unwrap();
    s.set_tool(ToolKind::TextBox);
    gesture(&mut s, Mods::default(), &[(100.0, 300.0), (180.0, 310.0), (230.0, 320.0)]);
    let dragged = s.doc().unwrap().selection.text.as_ref().unwrap().shape.0 as u64;
    assert_eq!(box_of(&mut s, dragged)[..3], [100.0, 300.0, 130.0], "the drag's width");
    s.key("Escape", Mods::default()).unwrap();
    s.key("Escape", Mods::default()).unwrap();
    // Escape with the tool chosen and nothing made puts it down.
    s.set_tool(ToolKind::TextBox);
    s.key("Escape", Mods::default()).unwrap();
    assert!(matches!(s.tool.kind, ToolKind::Select));
}

/// Plan stage 9 item 4 (WPS W-E5; case regressions/issue-hand-e5-double-click-text-box): a
/// double-click on the empty slide makes a 320 pt text box there to type in; left empty it goes.
#[test]
fn a_double_click_on_the_empty_slide_makes_a_text_box() {
    let (mut s, _) = stage8(&[[600.0, 100.0, 100.0, 100.0]]);
    let n = s.doc().unwrap().shapes().len();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    for kind in [PointerKind::Down, PointerKind::Up, PointerKind::Down, PointerKind::Up, PointerKind::DoubleClick] {
        s.pointer(ev(kind, 200.0, 250.0)).unwrap();
    }
    let id = s.doc().unwrap().selection.text.as_ref().expect("typing in a new box").shape.0 as u64;
    assert_eq!(box_of(&mut s, id)[..3], [200.0, 250.0, 320.0]);
    assert_eq!(s.doc().unwrap().shapes().len(), n + 1);
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), n, "left empty, it goes");
    // On a shape a double-click still edits its text; off the slide it makes nothing.
    for kind in [PointerKind::Down, PointerKind::Up, PointerKind::DoubleClick] {
        s.pointer(ev(kind, 980.0, 250.0)).unwrap();
    }
    assert_eq!(s.doc().unwrap().shapes().len(), n, "nothing off the slide");
}

/// Plan stage 9 item 3 (WPS W-E10): a shape tool locked for drawing stays chosen after each shape;
/// Escape puts it down; choosing a tool again unlocks it.
#[test]
fn a_locked_shape_tool_draws_one_shape_after_another_until_escape() {
    let (mut s, _) = stage8(&[]);
    let n = s.doc().unwrap().shapes().len();
    s.set_tool(ToolKind::Shape { preset: "heart".into() });
    s.tool.sticky = true;
    gesture(&mut s, Mods::default(), &[(100.0, 100.0), (150.0, 150.0), (200.0, 180.0)]);
    gesture(&mut s, Mods::default(), &[(300.0, 100.0)]);
    assert_eq!(s.doc().unwrap().shapes().len(), n + 2, "two hearts");
    assert!(matches!(&s.tool.kind, ToolKind::Shape { preset } if preset == "heart"), "the tool stays");
    s.key("Escape", Mods::default()).unwrap();
    assert!(matches!(s.tool.kind, ToolKind::Select), "Escape puts it down");
    s.set_tool(ToolKind::Shape { preset: "heart".into() });
    assert!(!s.tool.sticky, "chosen again, it is not locked");
    gesture(&mut s, Mods::default(), &[(500.0, 100.0)]);
    assert!(matches!(s.tool.kind, ToolKind::Select));
}

/// Plan stage 9 item 5 (walkthrough F7; case regressions/issue-hand-f7-picture-placeholder): a
/// picture put in an empty content placeholder fits inside it whole, in its middle, keeping its
/// proportions; the placeholder becomes the picture (its prompt goes).
#[test]
fn a_picture_in_a_content_placeholder_fits_and_keeps_its_proportions() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "titleAndContent"})).unwrap();
    let content = s.doc().unwrap().shapes().iter().find(|sh| sh.ph_type() == Some(model::PhType::Obj)).unwrap().id;
    s.execute("edit.select", &json!({"ids": [content.0]})).unwrap();
    // Two pixels by one.
    let wide = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGNgYPgPRAAFAgH/wSuWnwAAAABJRU5ErkJggg==";
    let r = s.execute("image.insert", &json!({"data": wide, "name": "wide.png"})).unwrap();
    assert_eq!(r["id"].as_u64(), Some(u64::from(content.0)), "the placeholder takes it");
    let b = box_of(&mut s, u64::from(content.0));
    assert!((b[2] / b[3] - 2.0).abs() < 0.01, "two to one ({} × {})", b[2], b[3]);
    assert_eq!([b[0], b[1], b[2], b[3]], [137.0, 144.0, 686.0, 343.0], "as high as the placeholder, in its middle");
    let sh = s.doc().unwrap().shape(content).unwrap();
    assert!(matches!(&sh.kind, model::ShapeKind::Picture { fill } if fill.crop == [0.0; 4]), "whole, not cropped");
    assert!(sh.text.is_none(), "no text, so no prompt");
}

/// A shape's frame on the slide (a group member's carried out of its group).
fn on_slide(s: &Session, id: u64) -> kition_slide_geom::Xfrm {
    let st = s.doc().unwrap();
    let sh = st.shape(model::ShapeId(id as u32)).unwrap();
    cmd::slide_xfrm_of(&st.doc, &st.selection, sh)
}

fn selected(s: &Session) -> Vec<u64> {
    s.doc().unwrap().selection.shapes.iter().map(|id| u64::from(id.0)).collect()
}

/// Plan stage 10 item 1 (walkthrough C9; WPS W-A12; case regressions/issue-hand-c9-group-child): a
/// click selects the group; a click on a member of the selected group selects that member, which a
/// drag then moves alone in one undo step (the group grows to hold it); a drag on the selected
/// group moves the whole group; Escape goes back to the group, then to nothing.
#[test]
fn a_click_on_a_member_of_the_selected_group_selects_that_member() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [a, b]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    let click = |s: &mut Session, x: f64, y: f64| {
        s.pointer(ev(PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(PointerKind::Up, x, y)).unwrap();
    };
    let drag = |s: &mut Session, (x, y): (f64, f64), (dx, dy): (f64, f64)| {
        s.pointer(ev(PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(PointerKind::Drag, x + dx / 2.0, y + dy / 2.0)).unwrap();
        s.pointer(ev(PointerKind::Drag, x + dx, y + dy)).unwrap();
        s.pointer(ev(PointerKind::Up, x + dx, y + dy)).unwrap();
    };
    click(&mut s, 150.0, 125.0);
    assert_eq!(selected(&s), vec![g], "the first click selects the group");
    click(&mut s, 150.0, 125.0);
    assert_eq!(selected(&s), vec![a], "a click on its member selects the member");
    click(&mut s, 350.0, 225.0);
    assert_eq!(selected(&s), vec![b], "and a click on another member that one");
    // The member has its own handles: its corner resizes it, not the group.
    let steps = s.doc().unwrap().history.undo.len();
    drag(&mut s, (400.0, 250.0), (20.0, 10.0));
    assert_eq!(on_slide(&s, b).w, 120.0, "the member's corner handle resizes the member");
    assert_eq!(on_slide(&s, a).w, 100.0, "and nothing else");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), steps, "one undo step");
    // A drag moves the member alone; the group grows to hold it.
    drag(&mut s, (350.0, 225.0), (40.0, 20.0));
    let moved = on_slide(&s, b);
    assert_eq!((moved.x, moved.y), (340.0, 220.0), "the member moves");
    assert_eq!((on_slide(&s, a).x, on_slide(&s, a).y), (100.0, 100.0), "the other member stays");
    let gb = on_slide(&s, g);
    assert_eq!((gb.x, gb.y, gb.w, gb.h), (100.0, 100.0, 340.0, 170.0), "the group holds both");
    assert_eq!(selected(&s), vec![b], "the member stays selected");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((on_slide(&s, b).x, on_slide(&s, b).y), (300.0, 200.0), "one undo puts it back");
    assert_eq!(s.doc().unwrap().history.undo.len(), steps);
    // Escape: back to the group, then nothing.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(selected(&s), vec![g], "Escape goes back to the group");
    // A drag on the selected group moves all of it.
    drag(&mut s, (150.0, 125.0), (10.0, 10.0));
    assert_eq!(selected(&s), vec![g], "a drag keeps the group selected");
    assert_eq!((on_slide(&s, a).x, on_slide(&s, b).x), (110.0, 310.0), "and moves all of it");
    s.key("Escape", Mods::default()).unwrap();
    assert!(selected(&s).is_empty(), "then Escape selects nothing");
}

/// Plan stage 10 item 1 (walkthrough C9): in a group that scales its child space, a member is
/// pressed, moved and resized where it is drawn on the slide.
#[test]
fn a_member_of_a_scaled_group_moves_where_it_is_drawn() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [a, b]})).unwrap()["id"].as_u64().unwrap();
    // Twice as large: a is drawn at (100, 100) 200 × 100, b at (500, 300) 200 × 100.
    s.execute("shape.setBounds", &json!({"id": g, "x": 100, "y": 100, "w": 600, "h": 300})).unwrap();
    let drawn = on_slide(&s, b);
    assert_eq!((drawn.x, drawn.y, drawn.w, drawn.h), (500.0, 300.0, 200.0, 100.0));
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    for kind in [PointerKind::Down, PointerKind::Up, PointerKind::Down, PointerKind::Up] {
        s.pointer(ev(kind, 600.0, 350.0)).unwrap();
    }
    assert_eq!(selected(&s), vec![b], "two clicks where b is drawn select b");
    s.pointer(ev(PointerKind::Down, 600.0, 350.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 610.0, 355.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 620.0, 360.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 620.0, 360.0)).unwrap();
    let moved = on_slide(&s, b);
    assert!((moved.x - 520.0).abs() < 1e-6 && (moved.y - 310.0).abs() < 1e-6, "moved as far as the pointer on the slide: {moved:?}");
    assert!((moved.w - 200.0).abs() < 1e-6, "at its drawn size: {moved:?}");
    // Its own corner handle, where it is drawn.
    s.pointer(ev(PointerKind::Down, 720.0, 410.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 730.0, 420.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 740.0, 430.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 740.0, 430.0)).unwrap();
    let sized = on_slide(&s, b);
    assert!((sized.w - 220.0).abs() < 1e-6 && (sized.h - 120.0).abs() < 1e-6, "its corner follows the pointer: {sized:?}");
    assert!((sized.x - 520.0).abs() < 1e-6, "the opposite corner stays: {sized:?}");
}

/// Plan stage 10 item 1 (walkthrough C9; R-E5): a double-click on a member's words edits that
/// member's text with the word selected, whether or not the group was selected; Escape leaves the
/// text with the member selected, then goes back to the group.
#[test]
fn a_double_click_on_a_members_words_edits_its_text() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 300, 100], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [500, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [a, b]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).unwrap();
    let ev = |kind| PointerEvent { kind, x: 250.0, y: 150.0, mods: Mods::default(), tol: crate::tools::TOL_PX };
    for kind in [PointerKind::Down, PointerKind::Up, PointerKind::Down, PointerKind::Up, PointerKind::DoubleClick] {
        s.pointer(ev(kind)).unwrap();
    }
    let t = s.doc().unwrap().selection.text.clone().expect("editing");
    assert_eq!(u64::from(t.shape.0), a, "the member's text is edited");
    assert!(t.is_range(), "with the word selected: {t:?}");
    s.type_text("Hi").unwrap();
    assert_eq!(s.execute("text.get", &json!({"id": a})).unwrap()["text"], "Hi");
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(selected(&s), vec![a], "Escape leaves the text, the member selected");
    s.key("Escape", Mods::default()).unwrap();
    assert_eq!(selected(&s), vec![g], "then the group");
    // Straight from nothing selected: the double-click event alone reaches the member's text.
    s.execute("edit.deselectAll", &json!({})).unwrap();
    s.pointer(ev(PointerKind::DoubleClick)).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().map(|t| u64::from(t.shape.0)), Some(a));
    let _ = b;
}

/// Plan stage 10 item 2 (walkthrough R8; R-D17): dragging a handle of one of several selected
/// objects scales every one by the same ratio, each about its own opposite handle (its centre with
/// Cmd), in one undo step.
#[test]
fn dragging_a_handle_of_several_selected_scales_them_all() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 200, 60, 120]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let steps = s.doc().unwrap().history.undo.len();
    let gesture = |s: &mut Session, mods: Mods, from: (f64, f64), to: (f64, f64)| {
        let ev = |kind, (x, y): (f64, f64)| PointerEvent { kind, x, y, mods, tol: crate::tools::TOL_PX };
        s.pointer(ev(PointerKind::Down, from)).unwrap();
        s.pointer(ev(PointerKind::Drag, ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0))).unwrap();
        s.pointer(ev(PointerKind::Drag, to)).unwrap();
        s.pointer(ev(PointerKind::Up, to)).unwrap();
    };
    // a's SE corner by (50, 25): a grows 1.5 ×; b too, its top left staying.
    gesture(&mut s, Mods::default(), (200.0, 150.0), (250.0, 175.0));
    let (na, nb) = (on_slide(&s, a), on_slide(&s, b));
    assert_eq!((na.x, na.y, na.w, na.h), (100.0, 100.0, 150.0, 75.0));
    assert_eq!((nb.x, nb.y, nb.w, nb.h), (400.0, 200.0, 90.0, 180.0), "the other scales by the same ratio");
    assert_eq!(selected(&s), vec![a, b], "both stay selected");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), steps, "one undo step");
    let nb = on_slide(&s, b);
    assert_eq!((nb.x, nb.y, nb.w, nb.h), (400.0, 200.0, 60.0, 120.0), "undone for both");
    // The W edge by -50 (a 1.5 × wider): b keeps its right edge; Cmd keeps each centre.
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    gesture(&mut s, Mods::default(), (100.0, 125.0), (50.0, 125.0));
    let nb = on_slide(&s, b);
    assert_eq!((nb.x, nb.w, nb.h), (370.0, 90.0, 120.0), "an edge scales one way, about the other edge");
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    gesture(&mut s, Mods { cmd: true, ..Default::default() }, (200.0, 150.0), (250.0, 175.0));
    let nb = on_slide(&s, b);
    assert!((nb.x + nb.w / 2.0 - 430.0).abs() < 1e-6 && (nb.y + nb.h / 2.0 - 260.0).abs() < 1e-6, "Cmd: about each centre ({nb:?})");
    assert!((nb.w - 120.0).abs() < 1e-6 && (nb.h - 240.0).abs() < 1e-6, "twice the size, as a is ({nb:?})");
}

/// Plan stage 10 item 3 (R-C25): a locked object's handles are grey and take no press.
#[test]
fn a_locked_objects_handles_take_no_press() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.lock", &json!({"ids": [a], "locked": true})).unwrap();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    assert_eq!(s.hit_test(kition_slide_geom::Point::new(200.0, 150.0), crate::tools::TOL_PX), None, "its corner handle is not hit");
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: crate::tools::TOL_PX };
    s.pointer(ev(PointerKind::Down, 200.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 230.0, 170.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 230.0, 170.0)).unwrap();
    let x = on_slide(&s, a);
    assert_eq!((x.w, x.h), (100.0, 50.0), "a drag from it changes nothing");
}

/// Plan stage 10 item 1: an Alt drag on a selected member leaves it and drags a copy of it, made in
/// the same group, in one undo step.
#[test]
fn an_alt_drag_on_a_member_copies_it_into_its_group() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [a, b]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    let steps = s.doc().unwrap().history.undo.len();
    let alt = Mods { alt: true, ..Default::default() };
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: alt, tol: crate::tools::TOL_PX };
    s.pointer(ev(PointerKind::Down, 150.0, 125.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 160.0, 130.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 170.0, 225.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 170.0, 225.0)).unwrap();
    let copy = selected(&s);
    assert_eq!(copy.len(), 1, "the copy is selected");
    assert_ne!(copy[0], a);
    let holding = cmd::groups_holding(s.doc().unwrap().shapes(), model::ShapeId(copy[0] as u32));
    assert_eq!(holding, vec![model::ShapeId(g as u32)], "in the same group");
    let moved = on_slide(&s, copy[0]);
    assert_eq!((moved.x, moved.y), (120.0, 200.0), "where the pointer took it");
    assert_eq!((on_slide(&s, a).x, on_slide(&s, a).y), (100.0, 100.0), "the member stays");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), steps, "one undo step");
    assert!(s.doc().unwrap().shape(model::ShapeId(copy[0] as u32)).is_none(), "undone, the copy goes");
    let _ = b;
}
