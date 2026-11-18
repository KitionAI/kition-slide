//! What a saved PPTX keeps beyond the drawn model (phase 7 fidelity) replicates: tag lists on the
//! slide file, masters, layouts, slides and shapes, the slide file's font faces, an object's carried parts
//! (`Opaque.rels`) and a custom geometry's source. All of it lives in `props` registers that
//! already exist (no schema change), so a 0.4.x peer, whose model lacks these keys, still reads
//! every register it knows.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, canonical, edit, first, new_replica, peer, slide_file, sync};
use kdm_model::read_document;
use kition_slide_model::{CustomPath, FontFace, Geom, KeptRel, ShapeKind, SlideFile};
use serde::Deserialize;
use serde_json::Value;

fn tags(k: &str) -> Vec<(String, String)> {
    vec![(format!("{k}_TAG"), format!("{k} value"))]
}

fn rich() -> SlideFile {
    let mut p = slide_file();
    p.tags = vec![tags("DECK")];
    p.font_faces = vec![FontFace {
        slot: "latin".into(),
        typeface: "Test Sans".into(),
        panose: Some("020B0604020202020204".into()),
        charset: Some("0".into()),
        pitch_family: Some("34".into()),
    }];
    let ole = p.add_media("oleObject1.bin", "application/vnd.openxmlformats-officedocument.oleObject", b"synthetic".to_vec());
    let m = Arc::make_mut(&mut p.masters[0]);
    m.tags = vec![tags("MASTER")];
    m.layouts[0].tags = vec![tags("LAYOUT")];
    let s = Arc::make_mut(&mut p.slides[0]);
    s.tags = vec![tags("SLIDE")];
    let a = s.shapes.iter_mut().find(|x| x.name == "A").unwrap_or_else(|| panic!("shape A"));
    a.tags = vec![tags("SHAPE")];
    a.geom = Geom::Custom {
        paths: vec![CustomPath { w: 10.0, h: 10.0, d: "M 0 0 L 10 0 L 5 10 Z".into(), ..Default::default() }],
        source: Some("<a:custGeom xmlns:a=\"urn:a\"><a:gdLst><a:gd name=\"g\" fmla=\"val 5\"/></a:gdLst></a:custGeom>".into()),
    };
    let b = s.shapes.iter_mut().find(|x| x.name == "B").unwrap_or_else(|| panic!("shape B"));
    b.text = None;
    b.kind = ShapeKind::Opaque {
        xml: "<p:graphicFrame/>".into(),
        preview: None,
        preview_adjust: None,
        label: "Object".into(),
        rels: vec![KeptRel { id: "rId1".into(), rel_type: "urn:rel/oleObject".into(), media: ole, rels: vec![] }],
    };
    p
}

#[test]
fn kept_parts_read_back_from_a_replica() {
    let p = rich();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&p, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&read), canonical(&p));
    let b = read.slides[0].shapes.iter().find(|x| x.name == "B").unwrap_or_else(|| panic!("shape B"));
    let ShapeKind::Opaque { rels, .. } = &b.kind else { panic!("not kept") };
    assert_eq!(read.media(rels[0].media).map(|m| m.data.as_slice()), Some(&b"synthetic"[..]), "the carried part names its media item");
}

#[test]
fn kept_parts_replicate_to_a_peer() {
    let blobs = SharedBlobs::default();
    let mut a = first(&slide_file(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    let r = rich();
    edit(&mut a, &blobs, |p| {
        p.tags = r.tags.clone();
        p.font_faces = r.font_faces.clone();
        Arc::make_mut(&mut p.slides[0]).tags = r.slides[0].tags.clone();
        let m = Arc::make_mut(&mut p.masters[0]);
        m.tags = r.masters[0].tags.clone();
        m.layouts[0].tags = r.masters[0].layouts[0].tags.clone();
    });
    sync(&mut a, &mut b);
    let q = &b.model().slide_file;
    assert_eq!((&q.tags, &q.font_faces, &q.slides[0].tags), (&r.tags, &r.font_faces, &r.slides[0].tags));
    assert_eq!((&q.masters[0].tags, &q.masters[0].layouts[0].tags), (&r.masters[0].tags, &r.masters[0].layouts[0].tags));
}

/// The registers as a 0.4.x reader declares them (no `tags`, `fontFaces`, `rels` or `source`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldRootProps {
    first_slide_number: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldSlideProps {
    name: String,
    show_master_shapes: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum OldGeom {
    Preset {},
    Custom { paths: Vec<CustomPath> },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum OldKind {
    Opaque {
        xml: String,
        label: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldShapeProps {
    geom: OldGeom,
    kind: OldKind,
}

#[test]
fn a_0_4_reader_reads_registers_with_the_new_keys() {
    let p = rich();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&p, &blobs);
    let root = kition_kit::kdm::ObjectId::from_string("slide-file");
    let props = replica.get(&root, "props").unwrap_or_else(|error| panic!("props: {error}"));
    assert!(props.get("tags").is_some() && props.get("fontFaces").is_some(), "{props}");
    let old: OldRootProps = serde_json::from_value(props).unwrap_or_else(|error| panic!("old root props: {error}"));
    assert_eq!(old.first_slide_number, p.first_slide_number);

    let slide = ids.slides.object(p.slides[0].id).cloned().unwrap_or_else(|| panic!("slide"));
    let props = replica.get(&slide, "props").unwrap_or_else(|error| panic!("props: {error}"));
    let old: OldSlideProps = serde_json::from_value(props).unwrap_or_else(|error| panic!("old slide props: {error}"));
    assert_eq!((old.name.as_str(), old.show_master_shapes), (p.slides[0].name.as_str(), p.slides[0].show_master_shapes));

    for name in ["A", "B"] {
        let id = p.slides[0].shapes.iter().find(|x| x.name == name).map(|x| x.id).unwrap_or_else(|| panic!("shape {name}"));
        let object = ids.shape(&slide, id).cloned().unwrap_or_else(|| panic!("shape object"));
        // A shape's members are its `look` map's entries since schema 5, with the same keys.
        let props = Value::Object(replica.map_entries(&object, "look").unwrap_or_else(|error| panic!("look: {error}")).into_iter().collect());
        let old: OldShapeProps = serde_json::from_value(props).unwrap_or_else(|error| panic!("old shape props of {name}: {error}"));
        match (name, old.geom, old.kind) {
            ("A", OldGeom::Custom { paths }, _) => assert_eq!(paths[0].d, "M 0 0 L 10 0 L 5 10 Z"),
            ("B", OldGeom::Preset {}, OldKind::Opaque { xml, label }) => assert_eq!((xml.as_str(), label.as_str()), ("<p:graphicFrame/>", "Object")),
            (name, ..) => panic!("unexpected registers for {name}"),
        }
    }
}
