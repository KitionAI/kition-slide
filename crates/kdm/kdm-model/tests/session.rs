//! `SlideSession`: the engine bridge's replica (phase 3d Task 2): `Send`, record/undo/redo and
//! the exchange with a peer, media blobs shipped through `blobs_since`, and garbage-collected once
//! no reachable undo step needs them.

mod common;

use std::sync::Arc;

use common::{canonical, person, slide_file};
use kdm_model::SlideSession;
use kition_kit::kdm::Version;
use kition_kit::session::MemoryBlobs;
use kition_slide_model::SlideFile;

fn send(from: &SlideSession, to: &mut SlideSession) {
    let since = to.version();
    to.add_blobs(&MemoryBlobs(from.blobs_since(&since).into_iter().collect())).unwrap();
    to.import_changes(&from.export_changes(&since).unwrap()).unwrap();
}

fn edited(s: &SlideSession, f: impl FnOnce(&mut SlideFile)) -> SlideFile {
    let mut p = s.slide_file().clone();
    f(&mut p);
    p
}

fn picture_count(p: &SlideFile) -> usize {
    p.media.iter().filter(|m| !m.data.is_empty()).count()
}

#[test]
fn a_session_is_send() {
    fn send_able<T: Send>() {}
    send_able::<SlideSession>();
}

#[test]
fn record_undo_redo_and_a_peer_agree() {
    let mut a = SlideSession::create(&slide_file()).unwrap();
    let snapshot = a.snapshot().unwrap();
    let mut b = SlideSession::open_snapshot(&snapshot, MemoryBlobs(a.blobs_since(&Version::empty()).into_iter().collect())).unwrap();
    assert_eq!(canonical(a.slide_file()), canonical(b.slide_file()));

    let after = edited(&a, |p| Arc::make_mut(&mut p.slides[0]).name = "Renamed".into());
    assert_eq!(a.record(&person("a"), &after).unwrap(), 1, "one undo step");
    assert_eq!(a.record(&person("a"), &after).unwrap(), 0, "nothing changed, no step");
    send(&a, &mut b);
    assert_eq!(b.slide_file().slides[0].name, "Renamed");
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());

    assert!(a.undo().unwrap().0);
    assert_ne!(a.slide_file().slides[0].name, "Renamed");
    assert!(a.redo().unwrap().0);
    assert_eq!(a.slide_file().slides[0].name, "Renamed");
    assert!(!a.redo().unwrap().0, "nothing more to redo");
}

#[test]
fn a_new_picture_ships_its_blob_and_only_that() {
    let mut a = SlideSession::create(&slide_file()).unwrap();
    let mut b = SlideSession::open_snapshot(&a.snapshot().unwrap(), MemoryBlobs(a.blobs_since(&Version::empty()).into_iter().collect())).unwrap();
    let since = b.version();
    let after = edited(&a, |p| {
        p.add_media("image2.png", "image/png", vec![0x89, b'P', b'N', b'G', 9, 9, 9]);
    });
    a.record(&person("a"), &after).unwrap();
    let blobs = a.blobs_since(&since);
    assert_eq!(blobs.len(), 1, "only the new picture's bytes");
    send(&a, &mut b);
    assert_eq!(picture_count(b.slide_file()), 2);
    assert!(a.blobs_since(&a.version()).is_empty(), "an up-to-date peer needs nothing");
}

#[test]
fn a_picture_whose_blob_arrives_after_its_changes_shows_once_it_does() {
    let mut a = SlideSession::create(&slide_file()).unwrap();
    let mut b = SlideSession::open_snapshot(&a.snapshot().unwrap(), MemoryBlobs(a.blobs_since(&Version::empty()).into_iter().collect())).unwrap();
    let since = b.version();
    let after = edited(&a, |p| {
        p.add_media("image2.png", "image/png", vec![0x89, b'P', b'N', b'G', 7, 7, 7]);
    });
    a.record(&person("a"), &after).unwrap();
    // The changes first: they import, and the picture shows empty meanwhile.
    b.import_changes(&a.export_changes(&since).unwrap()).unwrap();
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert!(b.view_loaded());
    assert_eq!(picture_count(b.slide_file()), 1);
    // Then the blob: the view reloads with it.
    assert!(b.add_blobs(&MemoryBlobs(a.blobs_since(&since).into_iter().collect())).unwrap());
    assert_eq!(picture_count(b.slide_file()), 2);
    assert_eq!(canonical(a.slide_file()), canonical(b.slide_file()));
}

#[test]
fn a_deleted_pictures_blob_lives_while_its_undo_step_does() {
    let mut a = SlideSession::create(&slide_file()).unwrap();
    assert_eq!(a.blobs().0.len(), 1);
    let after = edited(&a, |p| {
        Arc::make_mut(&mut p.slides[2]).shapes.clear();
        p.media.clear();
    });
    a.record(&person("a"), &after).unwrap();
    assert_eq!(a.blobs().0.len(), 1, "the undo step can bring it back");
    assert!(a.undo().unwrap().0);
    assert_eq!(picture_count(a.slide_file()), 1, "undo brought the picture back");
    assert!(a.redo().unwrap().0);
    a.retain_steps(0, 0);
    assert!(a.blobs().0.is_empty(), "no reachable step needs it any more");
    assert!(!a.can_undo());
}
