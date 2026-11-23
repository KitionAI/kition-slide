//! Sections and custom shows are objects whose members merge per slide (`src/members.rs`): two
//! people changing different members, or one renaming while the other moves slides, both keep
//! their change, and every replica ends with the same sections in the same order.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, edit, first, peer, slide_file, sync};
use kition_slide_model::{CustomShow, Section, SlideFile, SlideId, defaults};

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

fn section<'a>(p: &'a SlideFile, name: &str) -> &'a Section {
    p.sections.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no section {name}: {:?}", p.sections))
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
fn a_rename_and_a_move_of_slides_in_one_section_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    let all = ids(&a.model().slide_file);
    // A moves the second slide from One into Two; B renames Two and adds a new slide to it.
    edit(&mut a, &blobs, |p| {
        p.sections[0].slides.retain(|s| *s != all[1]);
        p.sections[1].slides.insert(0, all[1]);
    });
    edit(&mut b, &blobs, |p| {
        let layout = p.masters[0].layouts[0].id;
        let slide = defaults::new_slide(p, layout);
        let id = slide.id;
        p.slides.push(Arc::new(slide));
        p.sections[1].name = "Second".into();
        p.sections[1].slides.push(id);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let p = &a.model().slide_file;
    let added = p.slides.last().map(|s| s.id).unwrap_or_else(|| panic!("a slide"));
    assert_eq!(p.sections.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["One", "Second"]);
    assert_eq!(section(p, "One").slides, vec![all[0]]);
    let second = &section(p, "Second").slides;
    assert_eq!(second.first(), Some(&all[1]), "A's move, at the place A gave it");
    assert!(all[2..].iter().chain([&added]).all(|s| second.contains(s)), "B's new slide and the others: {second:?}");
    assert_eq!(second.len(), all.len(), "the first slide stays in One");
}

#[test]
fn slides_added_to_one_section_at_once_are_all_kept() {
    let (blobs, mut a, mut b) = pair();
    let add = |name: &'static str| {
        move |p: &mut SlideFile| {
            let layout = p.masters[0].layouts[0].id;
            let mut slide = defaults::new_slide(p, layout);
            slide.name = name.into();
            let id = slide.id;
            p.slides.push(Arc::new(slide));
            p.sections[0].slides.push(id);
        }
    };
    edit(&mut a, &blobs, add("by A"));
    edit(&mut b, &blobs, add("by B"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let p = &a.model().slide_file;
    let named = |name: &str| p.slides.iter().find(|s| s.name == name).map(|s| s.id).unwrap_or_else(|| panic!("{name}"));
    let one = &section(p, "One").slides;
    assert!(one.contains(&named("by A")) && one.contains(&named("by B")), "both added slides are in One: {one:?}");
    assert_eq!(one.len(), 4);
}

#[test]
fn a_slide_two_people_move_to_different_sections_at_once_ends_in_one() {
    let (blobs, mut a, mut b) = pair();
    let all = ids(&a.model().slide_file);
    edit(&mut a, &blobs, |p| {
        p.sections[1].slides.retain(|s| *s != all[2]);
        p.sections[0].slides.push(all[2]);
    });
    edit(&mut b, &blobs, |p| {
        p.sections[1].slides.retain(|s| *s != all[2]);
        p.sections.push(Section { identity: kition_slide_model::identity::fresh(), name: "Three".into(), slides: vec![all[2]] });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let p = &a.model().slide_file;
    let holding: Vec<&str> = p.sections.iter().filter(|s| s.slides.contains(&all[2])).map(|s| s.name.as_str()).collect();
    assert_eq!(holding, ["One"], "the first section holding it keeps it");
}

#[test]
fn members_added_to_one_custom_show_at_once_are_all_kept_and_a_slide_may_repeat() {
    let (blobs, mut a, mut b) = pair();
    let all = ids(&a.model().slide_file);
    edit(&mut a, &blobs, |p| p.custom_shows[0].slides.extend([all[1], all[0]]));
    edit(&mut b, &blobs, |p| p.custom_shows[0].slides.push(all[2]));
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let show = &a.model().slide_file.custom_shows[0].slides;
    assert_eq!(show.len(), 4, "{show:?}");
    assert_eq!(show[0], all[0]);
    assert_eq!(show.iter().filter(|s| **s == all[0]).count(), 2, "a slide listed twice stays twice");
    assert!(show.contains(&all[1]) && show.contains(&all[2]));
}

#[test]
fn deleting_a_slide_takes_it_out_of_a_section_another_person_renamed() {
    let (blobs, mut a, mut b) = pair();
    let all = ids(&a.model().slide_file);
    edit(&mut a, &blobs, |p| {
        p.slides.retain(|s| s.id != all[0]);
        for section in &mut p.sections {
            section.slides.retain(|s| *s != all[0]);
        }
        p.custom_shows[0].slides.retain(|s| *s != all[0]);
    });
    edit(&mut b, &blobs, |p| {
        p.sections[0].name = "First".into();
        p.custom_shows[0].slides.push(all[1]);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b, &blobs);
    let p = &a.model().slide_file;
    assert_eq!(section(p, "First").slides, vec![all[1]]);
    assert_eq!(p.custom_shows[0].slides, vec![all[1]]);
}
