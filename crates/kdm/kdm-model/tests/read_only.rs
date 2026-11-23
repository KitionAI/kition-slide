//! Content Kition Slide keeps verbatim but does not model is read-only through the object model
//! (Kit Standard 2.0, OM8): a shape of kind `opaque` and the unmodelled `rawExt` XML of the slide,
//! masters, and slides. Reads still answer them, the kit's own transactions still write them, and
//! the marks change nothing stored, so a reader of the schema without them loads the document.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, new_replica, slide_file};
use kdm_model::{SlideModel, read_document};
use kition_kit::kdm::batch::{self, BatchErrorCode, ReplicaTarget};
use kition_kit::kdm::{ActorId, ObjectId, Replica, Schema};
use kition_kit::session::KitModel;
use kition_slide_model::{ShapeKind, SlideFile};
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../model/schema.json");

/// A slide whose shape `B` is kept as an opaque object, and whose slide carries unmodelled XML.
fn kept_slide_file() -> SlideFile {
    let mut p = slide_file();
    p.raw_ext = Some("<p:extLst/>".into());
    let s = Arc::make_mut(&mut p.slides[0]);
    let b = s.shapes.iter_mut().find(|x| x.name == "B").unwrap_or_else(|| panic!("shape B"));
    b.text = None;
    b.kind = ShapeKind::Opaque { xml: "<p:graphicFrame/>".into(), preview: None, preview_adjust: None, label: "Object".into(), rels: Vec::new() };
    p
}

fn shape_named(replica: &Replica, name: &str) -> ObjectId {
    replica
        .objects_of_type("Shape")
        .into_iter()
        .find(|id| replica.get(id, "name").ok() == Some(json!(name)))
        .unwrap_or_else(|| panic!("shape {name}"))
}

fn refused(replica: &mut Replica, ops: Value) -> BatchErrorCode {
    let before = replica.state_hash().unwrap_or_else(|error| panic!("hash: {error}"));
    let error = batch::run(&mut ReplicaTarget(replica), &json!({ "ops": ops })).expect_err("the batch is refused");
    assert_eq!(replica.state_hash().unwrap_or_else(|error| panic!("hash: {error}")), before, "a refused batch changed the document");
    error.code
}

#[test]
fn opaque_shapes_and_kept_xml_are_read_only_through_the_object_model() {
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(&kept_slide_file(), &blobs);
    let opaque = shape_named(&replica, "B");
    let drawn = shape_named(&replica, "A");
    let slide = replica.objects_of_type("Slide").into_iter().next().unwrap_or_else(|| panic!("a slide"));

    // Reads answer kept content.
    let read = batch::run(
        &mut ReplicaTarget(&mut replica),
        &json!({ "load": [{ "target": opaque.to_string() }, { "target": "root", "fields": ["rawExt"] }] }),
    )
    .unwrap_or_else(|error| panic!("load: {error}"));
    assert_eq!(read["loaded"][0]["fields"]["kind"], "opaque");
    assert_eq!(read["loaded"][1]["fields"]["rawExt"], "<p:extLst/>");

    // Writes to it fail with read-only and change nothing.
    for ops in [
        json!([{ "op": "set", "target": opaque.to_string(), "field": "name", "value": "Renamed" }]),
        json!([{ "op": "set", "target": opaque.to_string(), "field": "xfrm", "value": { "x": 0, "y": 0, "w": 10, "h": 10 } }]),
        json!([{ "op": "delete", "target": opaque.to_string() }]),
        json!([{ "op": "set", "target": "root", "field": "rawExt", "value": "<p:extLst><x/></p:extLst>" }]),
        json!([{ "op": "set", "target": slide.to_string(), "field": "rawExt", "value": null }]),
        json!([{ "op": "set", "target": drawn.to_string(), "field": "kind", "value": "opaque" }]),
        json!([{ "op": "create", "type": "Shape", "in": format!("{slide}.shapes"), "fields": { "kind": "opaque", "name": "Fake" } }]),
    ] {
        assert_eq!(refused(&mut replica, ops.clone()), BatchErrorCode::ReadOnly, "{ops}");
    }

    // What the kit models stays writable, and placing the kept shape is the slide's to do.
    batch::run(
        &mut ReplicaTarget(&mut replica),
        &json!({ "ops": [
            { "op": "set", "target": drawn.to_string(), "field": "name", "value": "A2" },
            { "op": "list.move", "target": slide.to_string(), "field": "shapes", "from": 0, "to": 1 },
        ] }),
    )
    .unwrap_or_else(|error| panic!("writable edits: {error}"));
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    let kept = read.slides[0].shapes.iter().find(|x| x.name == "B").unwrap_or_else(|| panic!("B kept"));
    assert!(matches!(&kept.kind, ShapeKind::Opaque { xml, .. } if xml == "<p:graphicFrame/>"));
    assert_eq!(read.raw_ext.as_deref(), Some("<p:extLst/>"));
}

#[test]
fn the_schema_marks_opaque_shapes_and_kept_xml() {
    let schema = SlideModel::schema();
    let shape = schema.type_def("Shape").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(shape.read_only_when.as_ref().map(|when| (when.field.as_str(), when.values.clone())), Some(("kind", vec!["opaque".to_owned()])));
    for ty in ["SlideFile", "Master", "Slide"] {
        assert!(schema.field(ty, "rawExt").unwrap_or_else(|error| panic!("{error}")).read_only, "{ty}.rawExt");
    }
}

#[test]
fn a_reader_of_the_schema_without_the_marks_loads_the_document() {
    // The schema as it was before the marks: same version, so documents move both ways.
    let mut plain: Value = serde_json::from_str(SCHEMA).unwrap_or_else(|error| panic!("{error}"));
    let types = plain["types"].as_object_mut().unwrap_or_else(|| panic!("types"));
    for def in types.values_mut() {
        let def = def.as_object_mut().unwrap_or_else(|| panic!("type"));
        def.remove("readOnlyWhen");
        // A tool id (OM9) names a read-only field: the schema before the marks had neither.
        def.remove("toolId");
        for field in def["fields"].as_object_mut().unwrap_or_else(|| panic!("fields")).values_mut() {
            field.as_object_mut().unwrap_or_else(|| panic!("field")).remove("readOnly");
        }
    }
    let plain = Arc::new(Schema::parse(&plain.to_string()).unwrap_or_else(|error| panic!("{error}")));
    assert_eq!(plain.schema_version, SlideModel::schema().schema_version);
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&kept_slide_file(), &blobs);
    let snapshot = replica.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let actor = ActorId::from_u64(9).unwrap_or_else(|error| panic!("{error}"));
    let old = Replica::load(plain, &snapshot, actor).unwrap_or_else(|error| panic!("the previous schema loads: {error}"));
    assert_eq!(old.state_json().ok(), replica.state_json().ok());
    let back = Replica::load(SlideModel::schema(), &old.snapshot().unwrap_or_else(|error| panic!("{error}")), actor)
        .unwrap_or_else(|error| panic!("{error}"));
    let (read, _) = read_document(&back, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(read.raw_ext.as_deref(), Some("<p:extLst/>"));
}
