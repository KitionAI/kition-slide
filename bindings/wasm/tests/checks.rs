//! `slide_check` against its catalogue (`agent/checks.json`, KHP A38): the catalogue file is the
//! check's own (`check::CATALOGUE`), errors first, one sentence each; every fixture in
//! `conformance/` reports its code and only catalogued codes; `basic.kslide` is clean; checking
//! never changes the slides; and every `fix` the check offers clears its own code. The fixtures
//! are written by the crate's `conformance` test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_kit::diagnostics::{Severity, check_diagnostics};
use kition_slide_kit::SlideDocument;
use kition_slide_kit::check::{CATALOGUE, UNCATALOGUED, catalogue_json};
use serde_json::Value;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn open(name: &str) -> SlideDocument {
    let bytes = std::fs::read(repo().join("conformance").join(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    SlideDocument::open(name, &bytes).unwrap()
}

fn fixture(code: &str) -> String {
    format!("check-{}.kslide", code.trim_start_matches("slide."))
}

#[test]
fn the_catalogue_file_is_the_checks_own() {
    let file: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    assert_eq!(file, catalogue_json(), "regenerate with KITION_WRITE_CONFORMANCE=1 cargo test -p kition-slide-kit conformance");
    let mut previous = Severity::Error;
    for entry in CATALOGUE {
        assert!(kition_kit::diagnostics::valid_code(entry.code) && entry.code.starts_with("slide."), "{}", entry.code);
        assert!(entry.severity >= previous, "errors first: {}", entry.code);
        previous = entry.severity;
        assert!(entry.summary.ends_with('.') && !entry.summary.contains('\n'), "{}: one sentence", entry.code);
        assert!(!UNCATALOGUED.contains(&entry.code), "{}", entry.code);
    }
}

#[test]
fn every_fixture_reports_its_code_and_only_catalogued_codes() {
    let known: Vec<&str> = CATALOGUE.iter().map(|entry| entry.code).collect();
    for entry in CATALOGUE {
        let name = fixture(entry.code);
        let slides = open(&name);
        let found = slides.check();
        check_diagnostics(&found).unwrap();
        assert!(found.iter().any(|diagnostic| diagnostic.code == entry.code), "{name}: {found:?}");
        for diagnostic in &found {
            assert!(known.contains(&diagnostic.code.as_str()), "{name}: {} is not catalogued", diagnostic.code);
        }
    }
}

#[test]
fn basic_is_clean_and_checking_changes_nothing() {
    let slides = open("basic.kslide");
    let before = slides.inspect().unwrap();
    assert_eq!(slides.check(), Vec::new());
    assert_eq!(slides.inspect().unwrap(), before);
    assert!(!slides.dirty());
}

/// Every fix a fixture's diagnostics carry runs as an engine command and takes its code away.
#[test]
fn every_fix_clears_its_code() {
    for entry in CATALOGUE {
        let name = fixture(entry.code);
        let mut slides = open(&name);
        let fixes: Vec<_> =
            slides.check().into_iter().filter(|diagnostic| diagnostic.code == entry.code).filter_map(|diagnostic| diagnostic.fix).collect();
        if fixes.is_empty() {
            continue;
        }
        for fix in fixes {
            let params = fix.params.unwrap_or(Value::Object(serde_json::Map::new()));
            slides.execute(&fix.command, params).unwrap_or_else(|error| panic!("{name}: {}: {error}", fix.command));
        }
        let left: Vec<_> = slides.check().into_iter().filter(|diagnostic| diagnostic.code == entry.code).collect();
        assert!(left.is_empty(), "{name}: after its fixes: {left:?}");
    }
}
