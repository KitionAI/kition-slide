//! Sections and custom shows are objects with one entry per member slide (`src/members.rs`): a
//! custom show may list a slide twice, every replica reads the same members in the same order,
//! and the repair after a merged draft (data model 6.6) drops a member naming a slide the document
//! deleted meanwhile, and keeps a slide the draft and the document moved to different sections in
//! the first of them.

mod common;

use common::{SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, draft, edit, first, merge, peer, slide_file, sync};
use kition_slide_model::{CustomShow, Section, SlideFile, SlideId};

fn ids(p: &SlideFile) -> Vec<SlideId> {
    p.slides.iter().map(|s| s.id).collect()
}

/// The common slide file with sections One (its first two slides) and Two (the rest), and a
/// custom show of its first slide.
fn sectioned() -> SlideFile {
    let mut p = slide_file();
    let all = ids(&p);
    p.sections = vec![
        Section { identity: kition_slide_model::identity::fresh(), name: "One".into(), slides: all[..2].to_vec() },
        Section { identity: kition_slide_model::identity::fresh(), name: "Two".into(), slides: all[2..].to_vec() },
    ];
    p.custom_shows = vec![CustomShow { name: "Short".into(), slides: vec![all[0]] }];
    p
}

fn pair() -> (SharedBlobs, common::Session, common::Session) {
    let blobs = SharedBlobs::default();
    let a = first(&sectioned(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &common::Session, b: &common::Session, blobs: &SharedBlobs) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(canonical(&a.model().slide_file), canonical(&b.model().slide_file), "same view");
    for s in [a, b] {
        let (fresh, _) = kdm_model::read_document(s.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
        assert_eq!(canonical(&fresh), canonical(&s.model().slide_file), "the view is the replica's");
        assert_no_dangling(&s.model().slide_file);
        assert_replica_clean(s.replica());
    }
}

#[test]
fn sections_and_custom_shows_are_objects_with_one_entry_per_member() {
    let (_, a, _) = pair();
    let replica = a.replica();
    let root = kition_kit::kdm::ObjectId::from_string("slide-file");
    let sections = replica.list(&root, "sections").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(sections, a.model().ids().sections());
    assert_eq!(sections.len(), 2);
    let p = &a.model().slide_file;
    for (object, view) in sections.iter().zip(&p.sections) {
        assert_eq!(replica.get(object, "name").unwrap_or_default(), view.name.as_str());
        assert_eq!(replica.get(object, "identity").unwrap_or_default(), view.identity.as_str());
        let members = replica.map_entries(object, "slides").unwrap_or_else(|error| panic!("{error}"));
        let objects: Vec<_> =
            view.slides.iter().map(|s| a.model().ids().slides.object(*s).unwrap_or_else(|| panic!("slide")).as_str().to_owned()).collect();
        let mut keys: Vec<_> = members.iter().map(|(key, _)| key.clone()).collect();
        keys.sort();
        let mut expected = objects.clone();
        expected.sort();
        assert_eq!(keys, expected, "one entry per member slide, keyed by its object");
    }
    assert!(replica.schema().field("SlideFile", "sections").is_ok_and(|def| def.kind.is_container()));
    assert_eq!(replica.list(&root, "customShows").unwrap_or_default(), a.model().ids().custom_shows());
}

#[test]
fn members_added_to_one_custom_show_in_turn_are_all_kept_and_a_slide_may_repeat() {
    let (blobs, mut a, mut b) = pair();
    let all = ids(&a.model().slide_file);
    edit(&mut a, &blobs, |p| p.custom_shows[0].slides.extend([all[1], all[0]]));
    sync(&mut a, &mut b);
    edit(&mut b, &blobs, |p| p.custom_shows[0].slides.push(all[2]));
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let show = &a.model().slide_file.custom_shows[0].slides;
    assert_eq!(show.len(), 4, "{show:?}");
    assert_eq!(show[0], all[0]);
    assert_eq!(show.iter().filter(|s| **s == all[0]).count(), 2, "a slide listed twice stays twice");
    assert!(show.contains(&all[1]) && show.contains(&all[2]));
}

fn section<'a>(p: &'a SlideFile, name: &str) -> &'a Section {
    p.sections.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no section {name}: {:?}", p.sections))
}

/// The document's view is what its replica reads, and nothing names a deleted slide.
fn assert_clean(s: &common::Session, blobs: &SharedBlobs) {
    let (fresh, _) = kdm_model::read_document(s.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(&s.model().slide_file), "the view is the replica's");
    assert_no_dangling(&s.model().slide_file);
    assert_replica_clean(s.replica());
}

#[test]
fn a_slide_deleted_meanwhile_leaves_the_custom_show_a_draft_added_it_to() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&sectioned(), &blobs);
    let all = ids(&doc.model().slide_file);
    // The draft renames the first section and adds the second slide to the custom show; the
    // document deletes that slide meanwhile.
    let changes = draft(&doc, &blobs, |p| {
        p.sections[0].name = "First".into();
        p.custom_shows[0].slides.push(all[1]);
    });
    edit(&mut doc, &blobs, |p| {
        p.slides.retain(|s| s.id != all[1]);
        for section in &mut p.sections {
            section.slides.retain(|s| *s != all[1]);
        }
    });
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    assert_eq!(section(p, "First").slides, vec![all[0]]);
    assert_eq!(p.custom_shows[0].slides, vec![all[0]], "the deleted slide's member is dropped");
}

#[test]
fn a_slide_the_draft_and_the_document_move_to_different_sections_ends_in_the_first() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&sectioned(), &blobs);
    let all = ids(&doc.model().slide_file);
    let changes = draft(&doc, &blobs, |p| {
        p.sections[1].slides.retain(|s| *s != all[2]);
        p.sections.push(Section { identity: kition_slide_model::identity::fresh(), name: "Three".into(), slides: vec![all[2]] });
    });
    edit(&mut doc, &blobs, |p| {
        p.sections[1].slides.retain(|s| *s != all[2]);
        p.sections[0].slides.push(all[2]);
    });
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    assert_eq!(p.sections.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["One", "Two", "Three"]);
    let holding: Vec<&str> = p.sections.iter().filter(|s| s.slides.contains(&all[2])).map(|s| s.name.as_str()).collect();
    assert_eq!(holding, ["One"], "the first section holding it keeps it");
    assert_eq!(section(p, "One").slides, vec![all[0], all[1], all[2]]);
}

#[test]
fn a_draft_moving_a_slide_deleted_meanwhile_into_a_section_leaves_no_member() {
    let blobs = SharedBlobs::default();
    let mut doc = first(&sectioned(), &blobs);
    let all = ids(&doc.model().slide_file);
    let changes = draft(&doc, &blobs, |p| {
        p.sections[1].slides.retain(|s| *s != all[2]);
        p.sections[0].slides.push(all[2]);
    });
    edit(&mut doc, &blobs, |p| {
        p.slides.retain(|s| s.id != all[2]);
        for section in &mut p.sections {
            section.slides.retain(|s| *s != all[2]);
        }
    });
    merge(&mut doc, &changes);
    assert_clean(&doc, &blobs);
    let p = &doc.model().slide_file;
    assert_eq!(section(p, "One").slides, vec![all[0], all[1]], "the drafted member naming the deleted slide is dropped");
    // The file has four slides (its title slide and three more): Two keeps the fourth.
    assert_eq!(all.len(), 4);
    assert_eq!(section(p, "Two").slides, vec![all[3]]);
}
