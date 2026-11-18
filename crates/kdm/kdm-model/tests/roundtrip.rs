//! Round trips: a slide written to a replica reads back with the same content (ids aside), for the
//! web editor's sample, a slide exercising every replicated reference, and (when
//! `KITION_SLIDE_CORPUS=<dir>` is set) up to 40 `.pptx` files of a local corpus.

mod common;

use common::{SharedBlobs, canonical, new_replica, slide_file};
use kdm_model::{SlideModel, decode, read_document, write_changes};
use kition_kit::kdm::ActorId;
use kition_kit::session::{KitModel, KitSession};
use kition_slide_model::SlideFile;

/// `p` through `import`'s path (decode, write_document in a creating transaction) and back. Also
/// checks that diffing the read view against a deep copy of itself (no `Arc` shared, so every
/// field is compared) writes nothing.
fn round_trip(p: &SlideFile) -> SlideFile {
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(p, &blobs);
    let (read, mut ids) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    let mut copy = read.clone();
    copy.slides = copy.slides.iter().map(|s| std::sync::Arc::new((**s).clone())).collect();
    copy.masters = copy.masters.iter().map(|m| std::sync::Arc::new((**m).clone())).collect();
    let mut sink = blobs.clone();
    let wrote = replica
        .transact(&common::person("t"), |tx| write_changes(tx, &read, &copy, &mut ids, &mut sink))
        .unwrap_or_else(|error| panic!("write_changes: {error}"));
    assert!(!wrote, "an unchanged view wrote changes");
    read
}

fn assert_same(name: &str, original: &SlideFile, read: &SlideFile) {
    let (a, b) = (canonical(original), canonical(read));
    if a != b {
        let (a, b) = (a.to_string(), b.to_string());
        let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        let from = at.saturating_sub(200);
        panic!(
            "{name}: the round trip changed the slide near byte {at}:\n  wrote {}\n  read  {}",
            &a[from..(at + 200).min(a.len())],
            &b[from..(at + 200).min(b.len())]
        );
    }
    assert_eq!(original.media.len(), read.media.len(), "{name}: media count");
    for (x, y) in original.media.iter().zip(&read.media) {
        assert_eq!(x.data, y.data, "{name}: media {} bytes", x.name);
    }
}

#[test]
fn sample_round_trips() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../web/public/sample/Sample.kslide"))
        .unwrap_or_else(|error| panic!("sample: {error}"));
    let original = decode("Sample.kslide", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert!(!original.slides.is_empty());
    assert_same("Sample.kslide", &original, &round_trip(&original));
}

#[test]
fn slide_file_with_every_reference_round_trips() {
    let mut p = slide_file();
    let first = p.slides[0].id;
    let second = p.slides[1].id;
    p.sections =
        vec![kition_slide_model::Section { identity: kition_slide_model::identity::fresh(), name: "Intro".into(), slides: vec![first, second] }];
    p.custom_shows = vec![kition_slide_model::CustomShow { name: "Short".into(), slides: vec![second] }];
    let target = p.slides[0].shapes[0].id;
    let media = p.media[0].id;
    let slide = std::sync::Arc::make_mut(&mut p.slides[0]);
    slide.comments.push(kition_slide_model::Comment { author: "Ann".into(), text: "Look".into(), shape: Some(target), ..Default::default() });
    slide.transition = Some(kition_slide_model::Transition { kind: "fade".into(), sound: Some(media), ..Default::default() });
    slide.shapes[1].click = Some(kition_slide_model::text::Hyperlink {
        action: kition_slide_model::text::Action::Slide { slide: second },
        tooltip: String::new(),
        highlight_click: false,
    });
    p.embedded_fonts.push(("Family".into(), media));
    assert_same("slideFile", &p, &round_trip(&p));
}

#[test]
fn import_and_export_through_a_session() {
    let p = slide_file();
    let bytes = kition_slide_format::save(&p).unwrap_or_else(|error| panic!("save: {error}"));
    let session: KitSession<SlideModel> =
        KitSession::import("Slide.kslide", &bytes, Box::new(SharedBlobs::default()), ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor")))
            .unwrap_or_else(|error| panic!("import: {error}"));
    let exported = session.export("Slide.kslide").unwrap_or_else(|error| panic!("export: {error}"));
    let again = decode("Slide.kslide", &exported).unwrap_or_else(|error| panic!("decode: {error}"));
    assert_same("export", &decode("Slide.kslide", &bytes).unwrap_or_else(|error| panic!("decode: {error}")), &again);
    let pptx = session.export("Slide.pptx").unwrap_or_else(|error| panic!("pptx export: {error}"));
    assert!(kition_slide_pptx::sniff(&pptx));
}

#[test]
fn schema_parses() {
    let schema = SlideModel::schema();
    assert_eq!(schema.kit, "kit.slide");
}

/// `KITION_SLIDE_CORPUS=<dir>`: up to 40 `.pptx` files there, each imported, written, read back and
/// compared. Skipped when unset.
#[test]
fn pptx_corpus_round_trips() {
    let Ok(dir) = std::env::var("KITION_SLIDE_CORPUS") else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pptx")))
        .collect();
    files.sort();
    files.truncate(40);
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let Ok(original) = decode(&name, &bytes) else {
            eprintln!("{name}: not importable, skipped");
            continue;
        };
        assert_same(&name, &original, &round_trip(&original));
        checked += 1;
    }
    eprintln!("corpus: {checked} of {} files round-tripped", files.len());
}
