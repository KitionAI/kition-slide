//! Historical membership for content-based media reuse through a numeric shape address.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::session::model::Exchange;
use kition_kit::{CoveredAsk, HistoricalBlobs, KitDocument};
use kition_slide_kit::SlideDocument;
use serde_json::json;

struct NoBytes;
impl HistoricalBlobs for NoBytes {
    fn read(&self, _: &str, _: usize, _: usize) -> Result<Vec<u8>, String> {
        Err("no retained bytes".into())
    }
}

#[test]
fn a_media_item_that_only_matches_now_is_absent_from_the_historical_match_set() {
    // The same contributor-original one-pixel fixture used by the independent picture probes.
    let red = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";
    let mut doc = SlideDocument::create("History.pptx", 960, 540).unwrap();
    let shape = doc.execute("shape.insert", json!({"preset":"rect", "rect":[10,10,50,50]})).unwrap()["id"].clone();
    doc.execute("image.insert", json!({"name":"red.png", "data":red, "rect":[100,10,50,50]})).unwrap();
    let state = Exchange::new().replica(&mut doc).unwrap().state_json().unwrap();
    let media = state["objects"].as_object().unwrap().iter().find(|(_, object)| object["$type"] == "Media").unwrap().0.clone();
    let asked = CoveredAsk {
        id: json!(media),
        before: serde_json::from_value(
            json!({"blob":{"hash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "mime":"application/octet-stream", "size":0}}),
        )
        .unwrap(),
        ..Default::default()
    };
    let answer = doc.covered_at("shape.fill", &json!({"ids":[shape], "picture":red}), &[asked], &NoBytes).unwrap().unwrap();
    let item = answer.covered.iter().find(|item| item.id == media).unwrap();
    assert_eq!(item.covered, Some(false), "different sizes prove the old media did not match");
    assert!(!answer.under.contains(&json!(media)), "the historical set must exclude a newly matching media item: {answer:?}");
}
