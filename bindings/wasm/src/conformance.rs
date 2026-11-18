//! The KHP conformance fixtures (`conformance/`) and the check catalogue (`agent/checks.json`,
//! from [`crate::check::CATALOGUE`]): `basic.kslide` (no diagnostics), `edit.json` (steps that
//! change it), one `check-<code>.kslide` per catalogue code, and `golden/` previews of `basic`.
//! Each fixture starts from a new slide file made with the engine's own commands; the broken
//! references a file can hold but the commands never make (a picture whose bytes are gone or are
//! not a picture, effects too large to draw) are made by editing the document, as a file damaged
//! elsewhere would be.
//!
//! `KITION_WRITE_CONFORMANCE=1 cargo test -p kition-slide-kit conformance` writes them; without
//! the variable the test only checks that every fixture it would write reports its code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_slide_engine::cmd::base64_decode;
use kition_slide_model::SlideFile;
use serde_json::{Value, json};

use crate::SlideDocument;
use crate::check::{CATALOGUE, catalogue_json};

/// A 1 × 1 PNG, for the pictures the fixtures insert.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(slides: &mut SlideDocument, command: &str, params: Value) -> Value {
    slides.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn id(answer: &Value) -> u64 {
    answer["id"].as_u64().or_else(|| answer["ids"][0].as_u64()).unwrap_or_else(|| panic!("an id in {answer}"))
}

fn file_mut(slides: &mut SlideDocument) -> &mut SlideFile {
    Arc::make_mut(&mut slides.session.doc_mut().expect("open").doc)
}

/// The slide file every fixture starts from: one slide with a title text box and a rectangle,
/// in a font this machine has.
fn basic() -> SlideDocument {
    let mut slides = SlideDocument::create("Deck.kslide", 960, 540).expect("slides");
    run(&mut slides, "edit.selectAll", json!({}));
    run(&mut slides, "edit.delete", json!({}));
    let title = id(&run(&mut slides, "insert.textBox", json!({ "rect": [60, 60, 600, 80], "text": "Quarterly review" })));
    let rect = id(&run(&mut slides, "shape.insert", json!({ "preset": "rect", "rect": [60, 180, 240, 140] })));
    run(&mut slides, "format.font", json!({ "ids": [title, rect], "family": "Inter" }));
    slides
}

fn edit() -> Value {
    json!({ "steps": [{ "command": "shape.insert", "params": { "preset": "ellipse", "rect": [360, 200, 120, 120] } }] })
}

fn picture(slides: &mut SlideDocument, alt: Option<&str>) -> u64 {
    let picture = id(&run(slides, "insert.picture", json!({ "data": PNG_BASE64, "name": "chart.png", "rect": [400, 60, 160, 120] })));
    if let Some(text) = alt {
        run(slides, "shape.altText", json!({ "ids": [picture], "text": text }));
    }
    picture
}

fn fixture(code: &str) -> SlideDocument {
    let mut slides = basic();
    match code {
        "slide.missing-media" => {
            picture(&mut slides, Some("A red square"));
            file_mut(&mut slides).media.clear();
        }
        "slide.unreadable-picture" => {
            picture(&mut slides, Some("A red square"));
            for item in &mut file_mut(&mut slides).media {
                item.data = Arc::new(b"not a picture".to_vec());
            }
        }
        "slide.text-overflow" => {
            let text = "This sentence is far too long for the small box it was typed into, so it runs past the bottom edge of its box. ".repeat(6);
            let tiny = id(&run(&mut slides, "insert.textBox", json!({ "rect": [60, 360, 160, 30], "text": text })));
            run(&mut slides, "format.font", json!({ "ids": [tiny], "family": "Inter" }));
            run(&mut slides, "format.autofit", json!({ "ids": [tiny], "mode": "none" }));
            // A text box grows to fit what is typed into it; a file made elsewhere keeps the small box.
            let file = file_mut(&mut slides);
            let slide = Arc::make_mut(&mut file.slides[0]);
            for shape in &mut slide.shapes {
                if u64::from(shape.id.0) == tiny {
                    shape.xfrm = shape.xfrm.map(|xfrm| kition_slide_geom::Xfrm { h: 30.0, ..xfrm });
                }
            }
        }
        "slide.effects-dropped" => {
            let halo = id(&run(&mut slides, "shape.insert", json!({ "preset": "rect", "rect": [500, 300, 120, 80] })));
            let file = file_mut(&mut slides);
            let slide = Arc::make_mut(&mut file.slides[0]);
            for shape in &mut slide.shapes {
                if u64::from(shape.id.0) == halo {
                    shape.effects = Some(kition_slide_model::Effects {
                        glow: Some(kition_slide_model::style::Glow {
                            color: kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::rgb(255, 0, 0)),
                            radius: 1.0e6,
                        }),
                        ..Default::default()
                    });
                }
            }
        }
        "slide.font-substituted" => {
            let note = id(&run(&mut slides, "insert.textBox", json!({ "rect": [60, 360, 400, 40], "text": "Speaker note" })));
            run(&mut slides, "format.font", json!({ "ids": [note], "family": "Nowhere Sans Pro" }));
        }
        "slide.off-slide" => {
            run(&mut slides, "shape.insert", json!({ "preset": "rect", "rect": [2000, 60, 80, 80] }));
        }
        "slide.empty-placeholder" => {
            run(&mut slides, "slide.new", json!({}));
        }
        "slide.missing-alt-text" => {
            // The editor names an inserted picture's alt text after its file; one made elsewhere
            // may carry none.
            let shown = picture(&mut slides, None);
            let file = file_mut(&mut slides);
            let slide = Arc::make_mut(&mut file.slides[0]);
            for shape in &mut slide.shapes {
                if u64::from(shape.id.0) == shown {
                    shape.descr.clear();
                }
            }
        }
        other => panic!("no fixture for {other}"),
    }
    slides
}

fn render(slides: &mut SlideDocument, scale: f64) -> Vec<u8> {
    let answer = run(slides, "file.render", json!({ "slide": 0, "scale": scale }));
    base64_decode(answer["png"].as_str().expect("png")).expect("base64")
}

fn codes(slides: &SlideDocument) -> Vec<String> {
    slides.check().into_iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn every_fixture_reports_its_code_and_basic_is_clean() {
    let write = std::env::var_os("KITION_WRITE_CONFORMANCE").is_some();
    let out = repo().join("conformance");
    let mut basic = basic();
    let reopened = SlideDocument::open("basic.kslide", &basic.save(None).expect("save")).expect("reopen");
    assert_eq!(codes(&reopened), Vec::<String>::new(), "basic.kslide is clean");
    if write {
        std::fs::create_dir_all(out.join("golden")).expect("conformance/golden");
        std::fs::create_dir_all(repo().join("agent")).expect("agent");
        std::fs::write(out.join("basic.kslide"), basic.save(None).expect("save")).expect("basic");
        std::fs::write(out.join("edit.json"), format!("{:#}\n", edit())).expect("edit.json");
        std::fs::write(out.join("golden/basic.png"), render(&mut basic, 1.0)).expect("golden");
        std::fs::write(out.join("golden/basic-scale-2.png"), render(&mut basic, 2.0)).expect("golden scale 2");
        let tolerance = json!({
            "maxChannelDelta": 2,
            "maxDifferentPixels": 2600,
            "note": "The engine's own renderer draws the preview; anti-aliased text edges may differ by a step between platforms, on at most 0.5% of the slide's pixels."
        });
        std::fs::write(out.join("golden/tolerance.json"), format!("{tolerance:#}\n")).expect("tolerance");
        std::fs::write(repo().join("agent/checks.json"), format!("{:#}\n", catalogue_json())).expect("checks.json");
    }
    for entry in CATALOGUE {
        let slides = fixture(entry.code);
        let bytes = slides.save(None).expect("save");
        let reopened = SlideDocument::open("fixture.kslide", &bytes).expect("reopen");
        assert!(codes(&reopened).iter().any(|code| code == entry.code), "{}: {:?}", entry.code, codes(&reopened));
        if write {
            std::fs::write(out.join(format!("check-{}.kslide", entry.code.trim_start_matches("slide."))), bytes).expect("fixture");
        }
    }
}
