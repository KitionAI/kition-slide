#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use kition_slide_engine::Session;
use kition_slide_model::{PhType, Placeholder, ShapeId, ShapeKind, find_shape_mut};
use serde_json::json;

fn insert(session: &mut Session, x: f64) -> ShapeId {
    ShapeId(session.execute("shape.insert", &json!({"preset": "rect", "rect": [x, 40, 100, 60]})).unwrap()["id"].as_u64().unwrap() as u32)
}

fn fixture() -> (Session, ShapeId) {
    let mut session = Session::with_new();
    let a = insert(&mut session, 100.0);
    let b = insert(&mut session, 200.0);
    let group = session.execute("arrange.group", &json!({"ids": [a.0, b.0]})).unwrap()["id"].as_u64().unwrap();
    session.execute("shape.setBounds", &json!({"ids": [group], "x": 400, "w": 400})).unwrap();
    (session, a)
}

fn horizontal_box(session: &Session, id: ShapeId) -> (f64, f64) {
    let state = session.doc().unwrap();
    for shape in state.shapes() {
        if shape.id == id {
            let x = shape.xfrm.unwrap();
            return (x.x, x.w);
        }
        if let ShapeKind::Group { children, child } = &shape.kind
            && let Some(member) = children.iter().find(|shape| shape.id == id)
        {
            let outer = shape.xfrm.unwrap();
            let x = member.xfrm.unwrap();
            return (outer.x + (x.x - child.x) * outer.w / child.w, x.w * outer.w / child.w);
        }
    }
    panic!("missing shape {id}")
}

#[test]
fn grouped_member_alignment_uses_slide_coordinates_for_its_reference() {
    let (mut session, child) = fixture();
    let other = insert(&mut session, 500.0);
    session.execute("arrange.align", &json!({"ids": [child.0, other.0], "edge": "left", "to": "selection"})).unwrap();
    assert_eq!(horizontal_box(&session, child).0, 400.0);
    assert_eq!(horizontal_box(&session, other).0, 400.0);
}

#[test]
fn grouped_member_distribution_uses_slide_coordinates_for_its_reference() {
    let (mut session, child) = fixture();
    let middle = insert(&mut session, 500.0);
    let last = insert(&mut session, 900.0);
    session.execute("arrange.distribute", &json!({"ids": [child.0, middle.0, last.0], "dir": "horizontal", "to": "selection"})).unwrap();
    let (a, aw) = horizontal_box(&session, child);
    let (b, bw) = horizontal_box(&session, middle);
    let (c, _) = horizontal_box(&session, last);
    assert!(((b - a - aw) - (c - b - bw)).abs() < 1e-6, "unequal world-space gaps: {a}/{aw}, {b}/{bw}, {c}");
}

#[test]
fn inserting_a_picture_replaces_a_selected_grouped_placeholder() {
    let (mut session, child) = fixture();
    {
        let state = session.doc_mut().unwrap();
        let slide = Arc::make_mut(&mut Arc::make_mut(&mut state.doc).slides[0]);
        let placeholder = find_shape_mut(&mut slide.shapes, child).unwrap();
        placeholder.ph = Some(Placeholder { kind: PhType::Picture, ..Default::default() });
    }
    session.execute("edit.select", &json!({"ids": [child.0]})).unwrap();
    let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 100, 60, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let answer =
        session.execute("insert.picture", &json!({"data": kition_slide_engine::cmd::base64_encode(png.get_ref()), "name": "original.png"})).unwrap();
    assert_eq!(answer["id"], child.0);
    assert!(matches!(session.doc().unwrap().shape(child).unwrap().kind, ShapeKind::Picture { .. }));
}
