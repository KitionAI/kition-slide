//! C8-slide-6: object-model creation describes the same objects its file stores.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::kdm::reread;
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn state(doc: &mut SlideDocument) -> Value {
    Exchange::new().replica(doc).unwrap().state_json().unwrap()
}

#[test]
fn creates_assign_stored_ids_and_defaults_before_the_receipt() {
    for name in ["Deck.kslide", "Deck.pptx"] {
        for ty in ["Slide", "Shape"] {
            let mut doc = SlideDocument::create(name, 960, 540).unwrap();
            doc = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
            let mut exchange = Exchange::new();
            let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap()["toolId"].as_str().unwrap().to_owned();
            let before = state(&mut doc);
            let version = exchange.replica(&mut doc).unwrap().version();
            let (place, fields) = if ty == "Slide" {
                ("slide-file.slides".to_owned(), json!({}))
            } else {
                (format!("{first}.shapes"), json!({"kind": "shape", "name": "Overview"}))
            };
            exchange.batch(&mut doc, &json!({"ops": [{"op": "create", "type": ty, "in": place, "at": 0, "fields": fields}]}), false).unwrap();
            let memory = state(&mut doc);
            let saved = doc.save(Some(name)).unwrap();
            let mut again = SlideDocument::open(name, &saved).unwrap();
            let schema = doc.model_schema();
            let difference = reread::diff_paired(&schema, &memory, &state(&mut again)).to_json();
            for list in ["created", "changed", "moved", "deleted"] {
                assert_eq!(difference[list], json!([]), "{name} {ty}: {difference}");
            }
            let receipt = exchange.replica(&mut doc).unwrap().changes_since(&version).unwrap().to_json();
            let disk = reread::diff_paired(&schema, &before, &state(&mut again)).to_json();
            for list in ["created", "changed", "moved", "deleted"] {
                assert_eq!(receipt[list], disk[list], "{name} {ty} {list}");
            }
        }
    }
}

#[test]
fn internal_autofit_fields_cannot_be_sampled() {
    let doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let schema = doc.model_schema();
    let props = schema.field("TextBody", "props").unwrap();
    assert_eq!(props.sample, Some(false));
}

#[test]
fn sampled_shape_creations_default_to_an_ordinary_shape() {
    for name in ["Deck.kslide", "Deck.pptx"] {
        let mut doc = SlideDocument::create(name, 960, 540).unwrap();
        doc = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
        let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap()["toolId"].clone();
        let mut exchange = Exchange::new();
        exchange
            .batch(
                &mut doc,
                &json!({"ops": [{
                    "op": "create", "type": "Shape", "in": format!("{}.shapes", first.as_str().unwrap()),
                    "fields": {"name": "Overview", "xfrm": {"x": 10, "y": 10, "w": 100, "h": 50}}
                }]}),
                false,
            )
            .unwrap();
        let memory = state(&mut doc);
        let mut again = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
        let diff = reread::diff_paired(&doc.model_schema(), &memory, &state(&mut again)).to_json();
        for list in ["created", "changed", "moved", "deleted"] {
            assert_eq!(diff[list], json!([]), "{name}: {diff}");
        }
        let part = again.execute("slide.inspect", json!({"index": 0})).unwrap();
        assert!(part["shapes"].as_array().unwrap().iter().any(|s| s["name"] == "Overview" && s["kind"] == "shape"));
    }
}

#[test]
fn cross_slide_moves_have_the_saved_identity_in_their_transaction() {
    let mut failures = Vec::new();
    for name in ["Deck.kslide", "Deck.pptx"] {
        let mut doc = SlideDocument::create(name, 960, 540).unwrap();
        doc.execute("textBox.insert", json!({"text": "Moved", "rect": [10, 10, 100, 50]})).unwrap();
        doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        doc = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
        let first = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
        let second = doc.execute("slide.inspect", json!({"index": 1})).unwrap();
        let index = first["shapes"].as_array().unwrap().iter().position(|shape| shape["text"] == "Moved").unwrap();
        let old = first["shapes"][index]["toolId"].clone();
        let before = state(&mut doc);
        let mut exchange = Exchange::new();
        let version = exchange.replica(&mut doc).unwrap().version();
        exchange
            .batch(
                &mut doc,
                &json!({"ops": [
                    {"op": "list.remove", "target": first["toolId"], "field": "shapes", "index": index},
                    {"op": "list.insert", "target": second["toolId"], "field": "shapes", "item": old, "index": 0}
                ]}),
                false,
            )
            .unwrap();
        let memory = state(&mut doc);
        let saved = doc.save(Some(name)).unwrap();
        let mut again = SlideDocument::open(name, &saved).unwrap();
        let disk = state(&mut again);
        let schema = doc.model_schema();
        let extra = reread::diff_paired(&schema, &memory, &disk).to_json();
        for list in ["created", "changed", "moved", "deleted"] {
            if extra[list] != json!([]) {
                failures.push(format!("{name}: own move reread {list}: {}", extra[list]));
            }
        }
        let receipt = exchange.replica(&mut doc).unwrap().changes_since(&version).unwrap().to_json();
        let actual = reread::diff_paired(&schema, &before, &disk).to_json();
        for list in ["created", "changed", "moved", "deleted"] {
            if receipt[list] != actual[list] {
                failures.push(format!("{name}: move receipt {list}: {} != {}", receipt[list], actual[list]));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn command_limits_are_machine_readable() {
    let doc = SlideDocument::create("Bounds.pptx", 960, 540).unwrap();
    for (id, field, minimum, maximum) in [
        ("format.fontSize", "size", 1.0, 4000.0),
        ("table.insert", "rows", 1.0, 75.0),
        ("animation.timing", "duration", 0.0, 600_000.0),
        ("image.adjust", "saturation", 0.0, 4.0),
    ] {
        let commands = doc.commands();
        let schema = commands.iter().find(|c| c.id == id).unwrap().schema.as_ref().unwrap();
        assert_eq!(schema[field]["minimum"].as_f64(), Some(minimum), "{id}.{field}");
        assert_eq!(schema[field]["maximum"].as_f64(), Some(maximum), "{id}.{field}");
    }
}

#[test]
fn nested_command_limits_are_machine_readable() {
    let doc = SlideDocument::create("Bounds.pptx", 960, 540).unwrap();
    let commands = doc.commands();
    let effects = commands.iter().find(|c| c.id == "shape.effects").unwrap().schema.as_ref().unwrap();
    assert_eq!(effects["shadow"]["oneOf"][1]["properties"]["blur"]["maximum"], json!(200));
    assert_eq!(effects["glow"]["oneOf"][0]["maximum"], json!(150));
    assert_eq!(effects["glow"]["oneOf"][1]["properties"]["radius"]["maximum"], json!(150));
    assert_eq!(effects["softEdges"]["maximum"], json!(100));
    let timing = commands.iter().find(|c| c.id == "transition.timing").unwrap().schema.as_ref().unwrap();
    assert_eq!(timing["after"]["oneOf"][0]["maximum"], json!(86400000));
}

#[test]
fn json_member_limits_are_validated_before_writing() {
    let mut doc = SlideDocument::create("Bounds.pptx", 960, 540).unwrap();
    let id = doc.execute("textBox.insert", json!({"text": "Bounded", "rect": [0, 0, 100, 50]})).unwrap()["id"].clone();
    let part = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
    let shape = part["shapes"].as_array().unwrap().iter().find(|shape| shape["id"] == id).unwrap()["toolId"].clone();
    let mut exchange = Exchange::new();
    let before = state(&mut doc);
    for (field, value) in [("xfrm.w", -1.0), ("xfrm.x", 2147483648.0)] {
        let error = exchange.batch(&mut doc, &json!({"ops": [{"op": "set", "target": shape, "field": field, "value": value}]}), false);
        assert!(error.is_err(), "{field} accepted {value}");
        assert_eq!(state(&mut doc), before, "invalid member set is atomic");
    }
}

#[test]
fn layout_names_and_numeric_limits_describe_the_resolver() {
    let doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let commands = doc.commands();
    for id in ["slide.new", "slide.layout"] {
        let command = commands.iter().find(|command| command.id == id).unwrap();
        assert!(command.name_params.contains(&("layout", "props.name")));
        assert!(command.object_types.contains(&("layout", &["Layout"][..])));
    }
    for (id, field, minimum, maximum) in
        [("format.fontSize", "size", 1.0, 4000.0), ("table.columnWidth", "width", 4.0, 4000.0), ("animation.timing", "duration", 0.0, 600_000.0)]
    {
        let schema = commands.iter().find(|command| command.id == id).unwrap().schema.as_ref().unwrap();
        assert_eq!(schema[field]["minimum"].as_f64(), Some(minimum));
        assert_eq!(schema[field]["maximum"].as_f64(), Some(maximum));
    }
    let schema = doc.model_schema();
    for (ty, field) in [("Col", "width"), ("Row", "height")] {
        let def = schema.field(ty, field).unwrap();
        assert_eq!(def.minimum.as_ref().and_then(serde_json::Number::as_f64), Some(0.0));
        assert_eq!(def.maximum.as_ref().and_then(serde_json::Number::as_f64), Some(kition_slide_geom::stored::extent(f64::MAX)));
    }
}

#[test]
fn unused_font_face_metadata_survives_saving() {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let bytes = doc.save(Some("Deck.kslide")).unwrap();
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.kslide", &bytes).unwrap();
    file.font_faces.push(kition_slide_model::FontFace {
        slot: "latin".into(),
        typeface: "Unused original face".into(),
        panose: Some("02000000000000000000".into()),
        ..Default::default()
    });
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "kslide").unwrap();
    doc = SlideDocument::open("Deck.kslide", &bytes).unwrap();
    let saved = doc.save(Some("Deck.pptx")).unwrap();
    let again = kition_slide_engine::cmd::file::open_slide_file("Deck.pptx", &saved).unwrap();
    assert_eq!(again.font_faces, file.font_faces);
}

#[test]
fn an_unrelated_text_edit_keeps_a_connectors_imported_route() {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    let box_id = doc.execute("shape.insert", json!({"preset": "rect", "rect": [100, 100, 120, 80], "text": "Box"})).unwrap()["id"].clone();
    let connector = doc.execute("shape.connect", json!({"from": box_id, "to": box_id})).unwrap()["id"].clone();
    let mut file = kition_slide_engine::cmd::file::open_slide_file("Deck.kslide", &doc.save(Some("Deck.kslide")).unwrap()).unwrap();
    let slide = std::sync::Arc::make_mut(&mut file.slides[0]);
    let line = slide.shapes.iter_mut().find(|s| json!(s.id.0) == connector).unwrap();
    line.xfrm.as_mut().unwrap().x += 30.0;
    let imported = line.clone();
    let bytes = kition_slide_engine::cmd::file::save_bytes(&file, "kslide").unwrap();
    let mut doc = SlideDocument::open("Deck.kslide", &bytes).unwrap();
    doc.execute("text.set", json!({"id": box_id, "text": "Other text"})).unwrap();
    let after = kition_slide_engine::cmd::file::open_slide_file("Deck.kslide", &doc.save(Some("Deck.kslide")).unwrap()).unwrap();
    assert_eq!(after.slides[0].shapes.iter().find(|s| s.id == imported.id), Some(&imported));
    doc.execute("shape.move", json!({"ids": [box_id], "dx": 20})).unwrap();
    let moved = kition_slide_engine::cmd::file::open_slide_file("Deck.kslide", &doc.save(Some("Deck.kslide")).unwrap()).unwrap();
    assert_ne!(moved.slides[0].shapes.iter().find(|s| s.id == imported.id).unwrap().xfrm, imported.xfrm);
}

#[test]
fn diagram_fingerprints_follow_image_content_across_local_id_changes() {
    use kition_slide_model::{MediaId, Shape, ShapeKind, SlideFile, shapes_fingerprint, style::PictureFill};
    let mut file = SlideFile::default();
    let first = file.add_media("image.png", "image/png", vec![1, 2, 3]);
    let mut shape = Shape { kind: ShapeKind::Picture { fill: PictureFill { media: first, ..Default::default() } }, ..Default::default() };
    let fingerprint = shapes_fingerprint(std::slice::from_ref(&shape), &file.media);
    file.media[0].id = MediaId(40);
    if let ShapeKind::Picture { fill } = &mut shape.kind {
        fill.media = MediaId(40);
    }
    assert_eq!(shapes_fingerprint(std::slice::from_ref(&shape), &file.media), fingerprint);
    file.media[0].data = std::sync::Arc::new(vec![4, 5, 6]);
    assert_ne!(shapes_fingerprint(std::slice::from_ref(&shape), &file.media), fingerprint);
}

#[test]
fn comparing_unordered_media_keeps_each_items_bytes() {
    let mut file = kition_slide_model::SlideFile::default();
    file.add_media("z.png", "image/png", vec![9]);
    file.add_media("a.png", "image/png", vec![1]);
    let normalized = kdm_model::normalize(&file);
    for item in normalized.media {
        let original = file.media.iter().find(|original| original.name == item.name).unwrap();
        assert_eq!(item.data, original.data, "{}", item.name);
    }
}
