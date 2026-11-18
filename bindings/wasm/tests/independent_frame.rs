//! C8-slide-read: an absolute drawing frame follows the resolved target address.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

#[test]
fn absolute_drawing_frame_survives_numeric_tool_and_scoped_addresses_and_save() {
    for name in ["Frame.pptx", "Frame.kslide"] {
        for spelling in ["numeric", "tool", "scoped"] {
            let mut doc = SlideDocument::create(name, 960, 540).unwrap();
            let inserted = doc.execute("shape.insert", json!({"preset": "rect", "rect": [10, 20, 30, 40]})).unwrap();
            let slide = doc.execute("slide.inspect", json!({"index": 0})).unwrap();
            let shape = slide["shapes"].as_array().unwrap().iter().find(|shape| shape["id"] == inserted["id"]).unwrap();
            let id = shape["toolId"].clone();
            let address = match spelling {
                "numeric" => shape["id"].clone(),
                "tool" => id.clone(),
                _ => Value::from(format!("shape:{}/{}", slide["id"], shape["id"])),
            };
            doc.execute("drawing.transform", json!({"id": address, "onSlide": 1, "frame": {"x": 100, "y": 120, "w": 130, "h": 140}})).unwrap();
            let frame = doc.execute("drawing.describe", json!({"ids": [id]})).unwrap()["objects"][0]["frame"].clone();
            assert_eq!(frame["x"], 100.0, "{name} {spelling}: {frame}");
            assert_eq!(frame["y"], 120.0);
            assert_eq!(frame["w"], 130.0);
            assert_eq!(frame["h"], 140.0);
            let mut reopened = SlideDocument::open(name, &doc.save(Some(name)).unwrap()).unwrap();
            let saved = reopened.execute("drawing.describe", json!({"ids": [id], "onSlide": 1})).unwrap()["objects"][0]["frame"].clone();
            assert_eq!(saved, frame, "{name} {spelling}");
        }
    }
}
