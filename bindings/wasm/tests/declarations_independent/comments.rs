//! Blind positional projections of review.rs::find_thread (lines 118-136).
//! These compare resolution only, not the complete field/editor-state read set.
use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

struct Projection {
    command: &'static str,
    params: Value,
    thread: Value,
    ordinal: Option<String>,
}

fn projections() -> (SlideDocument, Vec<Projection>) {
    let mut doc = SlideDocument::create("Comment-read.pptx", 960, 540).unwrap();
    // A valid comment index must also be a valid but DIFFERENT slide index.
    // This makes accidental interpretation as a slide index observable.
    let mut threads = Vec::new();
    for slide in 0..3 {
        if slide > 0 {
            doc.execute("slide.new", json!({"layout": "blank"})).unwrap();
        }
        for index in 0..2 {
            let added = doc.execute("comment.add", json!({"slide": slide, "text": format!("{slide}:{index}"), "name": "Reader"})).unwrap();
            threads.push(added["thread"].clone());
        }
    }
    doc.execute("slide.go", json!({"index": 2})).unwrap();
    let slides = doc.execute("document.inspect", json!({})).unwrap()["slides"].clone();
    let derived = doc.derived(&[json!("slide-file")], &["commentOrder".into()]).unwrap();
    let expected_order = json!({
        (format!("slide:{}", slides[0]["id"])): [threads[0], threads[1]],
        (format!("slide:{}", slides[1]["id"])): [threads[2], threads[3]],
        (format!("slide:{}", slides[2]["id"])): [threads[4], threads[5]]
    });
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0].id, json!("slide-file"));
    assert_eq!(derived[0].values.get("commentOrder"), Some(&expected_order));
    let ordinal = |slide: usize, index: usize| format!("~commentOrder.slide:{}.{}", slides[slide]["id"].as_u64().unwrap(), index);
    let mut rows = Vec::new();
    for command in ["comment.reply", "comment.delete", "comment.resolve"] {
        // review.rs:122-125 searches the whole deck for a stable thread id.
        rows.push(Projection { command, params: json!({"thread": threads[1]}), thread: threads[1].clone(), ordinal: None });
        // review.rs:126-130 interprets BOTH numbers in @slide:index positionally.
        rows.push(Projection { command, params: json!({"thread": "@1:0"}), thread: threads[2].clone(), ordinal: Some(ordinal(1, 0)) });
        // review.rs:134-136 uses selection.slide, never slides[index].
        rows.push(Projection { command, params: json!({"index": 1}), thread: threads[5].clone(), ordinal: Some(ordinal(2, 1)) });
    }
    (doc, rows)
}

#[test]
fn comment_resolution_matches_blind_handler_projections() {
    let (doc, rows) = projections();
    let mut differing = std::collections::BTreeSet::new();
    let mut less = std::collections::BTreeSet::new();
    let mut more = std::collections::BTreeSet::new();
    for row in &rows {
        let under = doc.under(row.command, &row.params).unwrap().unwrap_or_default();
        // An ordinal reads the identity at that position, even when its new target
        // was untouched by the change that shifted it (review.rs:126-136).
        let holds_ordinal = row.ordinal.as_ref().is_none_or(|field| {
            under
                .premises
                .iter()
                .any(|premise| premise.id == json!("slide-file") && premise.fields.as_ref().is_some_and(|fields| fields.contains(field)))
        });
        let actual = &under.objects;
        // @1:0 reads slide 1's position; index uses the current part instead.
        let expected_windows = if row.params["thread"] == json!("@1:0") {
            vec![kition_kit::Within { parent: json!("slide-file"), field: "slides".into(), from: 1, to: 2, ..Default::default() }]
        } else {
            Vec::new()
        };
        let holds_window = under.within == expected_windows;
        let expected = if row.params.get("index").is_some() || row.params["thread"].as_str().is_some_and(|id| id.starts_with('@')) {
            vec![row.thread.clone()]
        } else {
            Vec::new()
        };
        if *actual != expected || !holds_ordinal || !holds_window {
            differing.insert(row.command);
            if expected.iter().any(|id| !actual.contains(id))
                || !holds_ordinal
                || expected_windows.iter().any(|window| !under.within.contains(window))
            {
                less.insert(row.command);
            }
            if actual.iter().any(|id| !expected.contains(id)) || under.within.iter().any(|window| !expected_windows.contains(window)) {
                more.insert(row.command);
            }
            eprintln!(
                "{} {}: expected resolved {expected:?}, got {actual:?}; ordinal held {holds_ordinal}; window held {holds_window}",
                row.command, row.params
            );
        }
    }
    let both = less.intersection(&more).count();
    eprintln!(
        "comment resolution: {} projections / 3 measured commands (193 writes overall); {} differing; less {}, more {}, both {both}",
        rows.len(),
        differing.len(),
        less.len(),
        more.len()
    );
    // C8-slide-read-2 baseline. These ceilings may only decrease. The 190 other
    // writes and the fields/editor-state of these three are NOT measured here.
    const CEILINGS: [usize; 4] = [0, 0, 0, 0];
    for (actual, ceiling) in [differing.len(), less.len(), more.len(), both].into_iter().zip(CEILINGS) {
        assert!(actual <= ceiling, "comment resolution regressed: {differing:?}; less {less:?}; more {more:?}");
    }
    assert_eq!(rows.len(), 9);
    assert_eq!(doc.commands().iter().filter(|command| command.mutates).count(), 193);
}
