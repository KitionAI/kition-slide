//! An Appear entrance (preset 1) keeps its delay and its start (on click, with previous, after
//! previous, and on a click on another shape) across saving as .pptx and opening again, written
//! where PresentationML keeps them: the delay on the effect's own `p:cTn`
//! (`p:stCondLst/p:cond delay`), the start as that `p:cTn`'s `nodeType` (and a trigger as an
//! `interactiveSeq`). Appear's only behaviour in the file is a `p:set` of `style.visibility` whose
//! `p:cTn` has `dur="1"`: the file keeps no duration for it. The engine takes a value into the
//! model as the file stores it, when it enters (`crates/engine/engine/src/stored.rs`, which settles
//! a slide's animations through `kition_slide_pptx::stored`), so a duration given to an Appear is
//! none (0) at once, not only after saving and reopening (case regressions/issue-appear-timing).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Read;

use kition_slide_engine::Session;
use kition_slide_model::anim::{AnimClass, AnimStart, Animation};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A new deck with four text boxes animated by Appear through the animation commands.
fn appear_deck() -> (Session, Vec<Value>) {
    let mut s = Session::new();
    run(&mut s, "file.new", json!({}));
    let ids: Vec<Value> = (0..4)
        .map(|i| {
            run(&mut s, "textBox.insert", json!({ "rect": [40.0 + 200.0 * f64::from(i), 100.0, 150.0, 60.0], "text": format!("Box {i}") }))["id"]
                .clone()
        })
        .collect();
    run(&mut s, "animation.add", json!({ "ids": [ids[0]], "effect": "appear", "start": "onClick", "delay": 750 }));
    run(&mut s, "animation.add", json!({ "ids": [ids[1]], "effect": "appear", "start": "withPrevious", "delay": 1250 }));
    run(&mut s, "animation.add", json!({ "ids": [ids[2]], "effect": "appear", "start": "afterPrevious", "delay": 500 }));
    run(&mut s, "animation.add", json!({ "ids": [ids[3]], "effect": "appear", "start": "onClick", "delay": 300 }));
    run(&mut s, "animation.timing", json!({ "index": 3, "trigger": ids[0] }));
    (s, ids)
}

fn animations(s: &Session) -> Vec<Animation> {
    let st = s.doc().unwrap();
    st.doc.slides[0].animations.clone()
}

fn slide_xml(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut text = String::new();
    z.by_name("ppt/slides/slide1.xml").expect("slide1.xml").read_to_string(&mut text).expect("read");
    text
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    tag[at..].split('"').next()
}

/// Each Appear effect node in document order: its `nodeType`, its own start delay, and the body of
/// its `p:par` (its behaviours).
fn appear_nodes(xml: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<p:cTn ") {
        let after = &rest[at..];
        let end = after.find('>').expect("tag end");
        let tag = &after[..end];
        if attr(tag, "presetID") == Some("1") && attr(tag, "presetClass") == Some("entr") {
            let body = &after[end + 1..];
            let par = &body[..body.find("</p:par>").expect("effect par end")];
            assert!(par.starts_with("<p:stCondLst><p:cond "), "the effect's first child is its start condition: {par}");
            let cond = &par[..par.find("/>").expect("cond end")];
            out.push((attr(tag, "nodeType").unwrap_or("").to_string(), attr(cond, "delay").unwrap_or("").to_string(), par.to_string()));
        }
        rest = &after[end..];
    }
    out
}

#[test]
fn appear_keeps_its_delay_and_start_in_pptx() {
    let (s, _) = appear_deck();
    let before = animations(&s);
    let set: Vec<_> = before.iter().map(|a| (a.effect.as_str(), a.start, a.delay_ms, a.trigger.is_some())).collect();
    assert_eq!(
        set,
        vec![
            ("appear", AnimStart::OnClick, 750, false),
            ("appear", AnimStart::WithPrevious, 1250, false),
            ("appear", AnimStart::AfterPrevious, 500, false),
            ("appear", AnimStart::OnClick, 300, true),
        ],
        "the commands set the timing"
    );
    let bytes = kition_slide_pptx::export(&s.doc().unwrap().doc).expect("export");

    // Written where PresentationML keeps them.
    let xml = slide_xml(&bytes);
    let nodes = appear_nodes(&xml);
    let written: Vec<(&str, &str)> = nodes.iter().map(|(node, delay, _)| (node.as_str(), delay.as_str())).collect();
    assert_eq!(written, vec![("clickEffect", "750"), ("withEffect", "1250"), ("afterEffect", "500"), ("clickEffect", "300")], "{xml}");
    for (_, _, par) in &nodes {
        assert!(par.contains("<p:set>") && par.contains("style.visibility") && par.contains("dur=\"1\""), "Appear sets visibility at once: {par}");
        assert!(!par.contains("p:animEffect") && !par.contains("<p:anim "), "Appear has no timed behaviour: {par}");
    }
    assert!(xml.contains("nodeType=\"interactiveSeq\""), "the triggered Appear is an interactive sequence: {xml}");

    // Read back the same.
    let reopened = kition_slide_pptx::import(&bytes).expect("import");
    let after = &reopened.slides[0].animations;
    assert_eq!(after.len(), before.len(), "{after:?}");
    for (x, y) in before.iter().zip(after.iter()) {
        assert_eq!((&x.effect, x.class, x.start, x.delay_ms, x.shape, x.trigger), (&y.effect, y.class, y.start, y.delay_ms, y.shape, y.trigger));
        assert_eq!(y.class, AnimClass::Entrance);
        assert_eq!((x.duration_ms, y.duration_ms), (0, 0), "Appear has no duration");
    }
}

#[test]
fn a_duration_given_to_appear_is_not_stored_in_pptx() {
    let (mut s, _) = appear_deck();
    run(&mut s, "animation.timing", json!({ "index": 0, "duration": 2000 }));
    let before = animations(&s);
    assert_eq!(
        (before[0].effect.as_str(), before[0].delay_ms, before[0].duration_ms),
        ("appear", 750, 0),
        "the duration enters the model as the file stores it: none"
    );
    let bytes = kition_slide_pptx::export(&s.doc().unwrap().doc).expect("export");
    let nodes = appear_nodes(&slide_xml(&bytes));
    assert!(!nodes[0].2.contains("dur=\"2000\""), "the file has no place for Appear's duration: {}", nodes[0].2);
    let reopened = kition_slide_pptx::import(&bytes).expect("import");
    let first = &reopened.slides[0].animations[0];
    assert_eq!((first.effect.as_str(), first.delay_ms, first.duration_ms), ("appear", 750, 0), "delay kept, duration not stored");
}
