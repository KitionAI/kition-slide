//! TEMPORARY (deleted in the next commit): rewrites every committed `.kslide` with today's writer
//! (keeping its ids) and prints them as base64, and prints how a session opened with its stored
//! replica differs from one that opened the file alone.

use kition_slide_engine::{Session, Version};
use serde_json::{Value, json};

const FILES: &[&str] = &[
    "cases/slide.agent.act/next-steps/input/deck.kslide",
    "cases/slide.agent.context/two-slides-one-run/input/deck.kslide",
    "cases/slide.agent.draft/closing-slide/input/deck.kslide",
    "cases/slide.agent.observe/echo-title/input/deck.kslide",
    "cases/slide.agent.recover/text-overflow/input/overflow.kslide",
    "conformance/basic.kslide",
    "conformance/check-effects-dropped.kslide",
    "conformance/check-empty-placeholder.kslide",
    "conformance/check-font-substituted.kslide",
    "conformance/check-missing-alt-text.kslide",
    "conformance/check-missing-media.kslide",
    "conformance/check-off-slide.kslide",
    "conformance/check-text-overflow.kslide",
    "conformance/check-unreadable-picture.kslide",
    "web/public/sample/Sample.kslide",
];

fn open(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.openBytes", &json!({"name": "Deck.kslide", "data": kition_slide_engine::cmd::base64_encode(bytes)})).unwrap();
    s
}

fn saved(s: &mut Session) -> Vec<u8> {
    let file = s.execute("file.saveBytes", &json!({})).unwrap();
    kition_slide_engine::cmd::base64_decode(file["data"].as_str().unwrap()).unwrap()
}

fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
    if out.len() >= 150 || a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                walk(x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), format!("{path}.{key}"), out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                walk(p, q, format!("{path}[{i}]"), out);
            }
        }
        _ => {
            let short = |v: &Value| v.to_string().chars().take(200).collect::<String>();
            out.push(format!("{path}: {} vs {}", short(a), short(b)))
        }
    }
}

#[test]
fn tmp_regenerate_fixtures_and_diagnose_the_replica_path() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../");
    let mut report = String::new();
    for path in FILES {
        let bytes = std::fs::read(format!("{root}{path}")).unwrap();
        let mut s = open(&bytes);
        let fresh = saved(&mut s);
        let again = saved(&mut open(&fresh));
        report.push_str(&format!("REGEN {path} stable={} {}\n", again == fresh, kition_slide_engine::cmd::base64_encode(&fresh)));

        // The replica path, for this file as rewritten.
        let mut one = open(&fresh);
        let snapshot = one.snapshot().unwrap();
        let blobs = one.blobs_since(&Version::empty()).unwrap();
        let mut kept = Session::new();
        kept.open_with_replica(&fresh, &snapshot, blobs).unwrap();
        let replica_save = saved(&mut kept);
        let mut out = Vec::new();
        walk(
            &serde_json::to_value(&one.active().unwrap().doc).unwrap(),
            &serde_json::to_value(&kept.active().unwrap().doc).unwrap(),
            String::new(),
            &mut out,
        );
        report.push_str(&format!("REPLICA {path} same_bytes={} differences={}\n", replica_save == fresh, out.len()));
        for line in out.iter().take(if path.ends_with("basic.kslide") || path.ends_with("Sample.kslide") { 150 } else { 12 }) {
            report.push_str(&format!("  DIFF {line}\n"));
        }
    }
    println!("{report}");
    panic!("temporary: see the report above");
}
