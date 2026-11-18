//! The view patched from what an import touched (Kit Standard 2.0, rule E11; `src/patch.rs`): the
//! slides a change did not touch keep their very `Arc`s (so the engine's per-slide caches stay
//! warm), and the view is the one a full read of the replica gives.

mod common;

use std::sync::Arc;

use common::{Session, SharedBlobs, canonical, edit, first, peer, send, shape, slide_file};
use kition_slide_model::ShapeKind;
use kition_slide_model::style::PictureFill;
use kition_slide_model::{Slide, SlideFile, defaults};

fn assert_fresh(session: &Session, blobs: &SharedBlobs) {
    let (fresh, _) = kdm_model::read_document(session.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&session.model().slide_file), canonical(&fresh), "the patched view is what a full read gives");
}

fn slides(session: &Session) -> Vec<Arc<Slide>> {
    session.model().slide_file.slides.clone()
}

fn pair() -> (Session, Session, SharedBlobs) {
    let blobs = SharedBlobs::default();
    let a = first(&slide_file(), &blobs);
    let b = peer(&a, 2, &blobs);
    (a, b, blobs)
}

#[test]
fn a_shape_edit_reads_only_its_slide_again() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    edit(&mut a, &blobs, |p| {
        let s0 = Arc::make_mut(&mut p.slides[0]);
        if let Some(x) = s0.shapes[0].xfrm.as_mut() {
            x.x += 25.0;
        }
        s0.shapes[1].name = "Renamed".into();
    });
    send(&a, &mut b);
    let after = slides(&b);
    assert!(!Arc::ptr_eq(&before[0], &after[0]), "the edited slide is read again");
    assert!(Arc::ptr_eq(&before[1], &after[1]) && Arc::ptr_eq(&before[2], &after[2]), "the other slides are kept");
    assert_eq!(after[0].shapes[1].name, "Renamed");
    assert_eq!(after[0].id, before[0].id, "the slide keeps its local id");
    assert_eq!(after[0].shapes[0].id, before[0].shapes[0].id, "its shapes keep theirs");
    assert_fresh(&b, &blobs);
    // The patched ids still write: b's own edit of the same slide reaches a.
    edit(&mut b, &blobs, |p| Arc::make_mut(&mut p.slides[0]).shapes[0].name = "From b".into());
    send(&b, &mut a);
    assert_eq!(a.model().slide_file.slides[0].shapes[0].name, "From b");
    assert_fresh(&a, &blobs);
}

#[test]
fn typing_and_a_new_shape_read_only_their_slide_again() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    edit(&mut a, &blobs, |p| {
        let new = shape(p, "New", 400.0);
        let s2 = Arc::make_mut(&mut p.slides[2]);
        s2.shapes.push(new);
        if let Some(text) = Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut() {
            *text = kition_slide_model::text::TextBody::from_text("Typed\nSplit");
        }
    });
    send(&a, &mut b);
    let after = slides(&b);
    assert!(Arc::ptr_eq(&before[1], &after[1]), "the untouched slide is kept");
    assert!(after[2].shapes.iter().any(|s| s.name == "New"));
    assert_fresh(&b, &blobs);
}

#[test]
fn slides_added_and_moved_keep_the_other_slides() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    edit(&mut a, &blobs, |p: &mut SlideFile| {
        let layout = p.masters[0].layouts[0].id;
        let slide = defaults::new_slide(p, layout);
        p.slides.insert(1, Arc::new(slide));
        p.slides.swap(2, 3);
    });
    send(&a, &mut b);
    let after = slides(&b);
    assert_eq!(after.len(), before.len() + 1);
    assert!(Arc::ptr_eq(&before[0], &after[0]), "slide 0 is kept");
    assert!(Arc::ptr_eq(&before[2], &after[2]), "the moved slides are kept");
    assert!(Arc::ptr_eq(&before[1], &after[3]));
    assert_fresh(&b, &blobs);
}

#[test]
fn a_deleted_shape_reads_only_its_slide_again() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    edit(&mut a, &blobs, |p| {
        let s1 = Arc::make_mut(&mut p.slides[1]);
        s1.shapes.clear();
    });
    send(&a, &mut b);
    let after = slides(&b);
    assert!(after[1].shapes.is_empty());
    assert!(Arc::ptr_eq(&before[0], &after[0]) && Arc::ptr_eq(&before[2], &after[2]), "the other slides are kept");
    assert_fresh(&b, &blobs);
}

#[test]
fn what_a_patch_cannot_see_reads_the_whole_view() {
    let (mut a, mut b, blobs) = pair();
    // A deleted slide (others may link to it), then a master's change.
    edit(&mut a, &blobs, |p| {
        p.slides.remove(1);
    });
    send(&a, &mut b);
    assert_eq!(slides(&b).len(), slides(&a).len());
    assert_fresh(&b, &blobs);
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.masters[0]).name = "Renamed master".into());
    send(&a, &mut b);
    assert_eq!(b.model().slide_file.masters[0].name, "Renamed master");
    assert_fresh(&b, &blobs);
}

/// Adds a picture of new media bytes to slide `index`.
fn add_picture(p: &mut SlideFile, index: usize, bytes: Vec<u8>) {
    let media = p.add_media("added.png", "image/png", bytes);
    let mut picture = shape(p, "Added picture", 20.0);
    picture.text = None;
    picture.kind = ShapeKind::Picture { fill: PictureFill { media, ..Default::default() } };
    Arc::make_mut(&mut p.slides[index]).shapes.push(picture);
}

#[test]
fn a_new_picture_reads_only_its_slide_and_its_media() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    let media_before = b.model().slide_file.media.len();
    edit(&mut a, &blobs, |p| add_picture(p, 1, vec![0x89, b'P', b'N', b'G', 9, 9, 9]));
    send(&a, &mut b);
    let after = slides(&b);
    assert!(Arc::ptr_eq(&before[0], &after[0]) && Arc::ptr_eq(&before[2], &after[2]), "the other slides are kept");
    assert_eq!(b.model().slide_file.media.len(), media_before + 1);
    let picture = after[1].shapes.iter().find(|s| s.name == "Added picture").unwrap_or_else(|| panic!("the picture arrived"));
    let ShapeKind::Picture { fill } = &picture.kind else { panic!("a picture") };
    assert_eq!(b.model().slide_file.media(fill.media).map(|m| m.data.as_slice()), Some([0x89, b'P', b'N', b'G', 9, 9, 9].as_slice()));
    assert_fresh(&b, &blobs);
}

#[test]
fn a_deleted_slide_keeps_the_other_slides() {
    let (mut a, mut b, blobs) = pair();
    let before = slides(&b);
    edit(&mut a, &blobs, |p| {
        p.slides.remove(1);
    });
    send(&a, &mut b);
    let after = slides(&b);
    assert_eq!(after.len(), before.len() - 1);
    assert!(
        Arc::ptr_eq(&before[0], &after[0]) && Arc::ptr_eq(&before[2], &after[1]) && Arc::ptr_eq(&before[3], &after[2]),
        "the other slides are kept"
    );
    assert_fresh(&b, &blobs);
    // The patched ids still write.
    edit(&mut b, &blobs, |p| Arc::make_mut(&mut p.slides[1]).name = "After the deletion".into());
    send(&b, &mut a);
    assert_eq!(a.model().slide_file.slides[1].name, "After the deletion");
    assert_fresh(&a, &blobs);
}

#[test]
fn a_deleted_slide_another_links_to_reads_the_whole_view() {
    let (mut a, mut b, blobs) = pair();
    edit(&mut a, &blobs, |p| {
        let target = p.slides[1].id;
        let link = kition_slide_model::text::Hyperlink {
            action: kition_slide_model::text::Action::Slide { slide: target },
            tooltip: String::new(),
            highlight_click: false,
        };
        Arc::make_mut(&mut p.slides[0]).shapes[0].click = Some(link);
    });
    send(&a, &mut b);
    edit(&mut a, &blobs, |p| {
        p.slides.remove(1);
    });
    send(&a, &mut b);
    assert_fresh(&b, &blobs);
}

/// A slide file of `n` slides, each with a few shapes and text.
fn big_slide_file(n: usize) -> SlideFile {
    let mut p = slide_file();
    let layout = p.masters[0].layouts[0].id;
    while p.slides.len() < n {
        let mut slide = defaults::new_slide(&mut p, layout);
        for k in 0..6 {
            let s = shape(&mut p, &format!("Shape {k}"), 40.0 * k as f64);
            slide.shapes.push(s);
        }
        p.slides.push(Arc::new(slide));
    }
    p
}

/// `cargo test --release -p kdm-model --test patch -- --ignored --nocapture`: what applying a
/// peer's media insert and slide deletion costs on a 120-slide slide file (median of 20).
#[test]
#[ignore = "a measurement; run in release builds"]
fn media_insert_and_slide_deletion_apply_times() {
    let blobs = SharedBlobs::default();
    let mut a = first(&big_slide_file(120), &blobs);
    let mut b = peer(&a, 2, &blobs);
    let mut timed = |label: &str, f: &dyn Fn(&mut SlideFile, usize)| {
        let mut times = Vec::new();
        for round in 0..20 {
            edit(&mut a, &blobs, |p| f(p, round));
            let bytes = a.export_changes(&b.version()).unwrap_or_else(|error| panic!("export: {error}"));
            let start = std::time::Instant::now();
            b.import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!("kit.slide 120 slides: {label} applies in p50 {:.2} ms, max {:.2} ms", times[times.len() / 2], times[times.len() - 1]);
    };
    timed("a media insert", &|p, round| add_picture(p, 5 + round, vec![0x89, b'P', b'N', b'G', round as u8, 1, 2]));
    timed("a slide deletion", &|p, round| {
        p.slides.remove(40 + round);
    });
    assert_fresh(&b, &blobs);
}
