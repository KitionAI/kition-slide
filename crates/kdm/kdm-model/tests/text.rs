//! Text bodies in the paragraph form: two replicas
//! typing into one text box, one table cell or one slide's notes keep both texts; bold and italic
//! over overlapping ranges both apply; typing at a bold run's end extends it; a paragraph split or
//! merge (and a merge's undo) merges with concurrent typing; a 0.3.x replica (text bodies as
//! registers) reads, migrates on the first write as a `system` transaction and stays converged; a
//! 0.3.x build refuses a new replica.

mod common;

use std::sync::Arc;

use common::{
    SharedBlobs, assert_replica_clean, canonical, edit, first, new_replica, peer, person, shape_named, shape_object, slide_file, sync, table_mut,
};
use kdm_model::{BodyOwner, COMMENTS_SCHEMA_VERSION, Ids, MIGRATED_SCHEMA_VERSION, SlideModel, SlideSession, read_document, write_document};
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_slide_model::text::{Paragraph, Run, RunProps, TextBody};
use kition_slide_model::{ShapeKind, SlideFile};
use serde_json::Value;

fn body_mut<'a>(p: &'a mut SlideFile, slide: usize, name: &str) -> &'a mut TextBody {
    let s = Arc::make_mut(&mut p.slides[slide]);
    s.shapes.iter_mut().find(|s| s.name == name).and_then(|s| s.text.as_mut()).unwrap_or_else(|| panic!("no text in {name}"))
}

fn text_of(p: &SlideFile, slide: usize, name: &str) -> String {
    shape_named(p, slide, name).text.as_ref().map(TextBody::text).unwrap_or_default()
}

/// Sets the whole text of `paragraph` as one plain run.
fn set_plain(body: &mut TextBody, paragraph: usize, text: &str) {
    body.paragraphs[paragraph].runs = vec![Run::new(text)];
}

/// Each character of a paragraph with whether it is bold and italic.
fn styles(p: &Paragraph) -> Vec<(char, bool, bool)> {
    p.runs.iter().flat_map(|r| r.text.chars().map(|c| (c, r.props.bold == Some(true), r.props.italic == Some(true)))).collect()
}

/// `p` with `range` (characters of its only paragraph) carrying `f`'s run properties.
fn format(body: &mut TextBody, range: std::ops::Range<usize>, f: impl Fn(&mut RunProps)) {
    let p = &mut body.paragraphs[0];
    let mut runs = Vec::new();
    let mut at = 0;
    for run in std::mem::take(&mut p.runs) {
        for ch in run.text.chars() {
            let mut props = run.props.clone();
            if range.contains(&at) {
                f(&mut props);
            }
            runs.push(Run::with(ch.to_string(), props));
            at += 1;
        }
    }
    p.runs = runs;
    p.normalize();
}

fn pair_with(p: &SlideFile) -> (SharedBlobs, common::Session, common::Session) {
    let blobs = SharedBlobs::default();
    let a = first(p, &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &common::Session, b: &common::Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(canonical(&a.model().slide_file), canonical(&b.model().slide_file), "same view");
    assert_replica_clean(a.replica());
}

fn hello_slide_file() -> SlideFile {
    let mut p = slide_file();
    *body_mut(&mut p, 0, "A") = TextBody::from_text("Hello world");
    p
}

#[test]
fn concurrent_typing_at_different_positions_of_one_text_box_keeps_both() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    let paragraph_object = |s: &common::Session| {
        let shape = shape_object(s, 0, "A");
        s.model().ids().body(&BodyOwner::Shape(shape)).unwrap_or_else(|| panic!("a body")).paragraphs[0].clone()
    };
    let before = paragraph_object(&a);
    edit(&mut a, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello big world"));
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello world!"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text_of(&a.model().slide_file, 0, "A"), "Hello big world!");
    assert_eq!(paragraph_object(&a), before, "typing keeps the paragraph's object");
}

#[test]
fn concurrent_typing_at_one_position_keeps_both() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    edit(&mut a, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello, world"));
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello there world"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let text = text_of(&a.model().slide_file, 0, "A");
    assert!(text == "Hello, there world" || text == "Hello there, world", "{text}");
}

#[test]
fn bold_and_italic_over_overlapping_ranges_both_apply() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    edit(&mut a, &blobs, |p| format(body_mut(p, 0, "A"), 0..8, |r| r.bold = Some(true)));
    edit(&mut b, &blobs, |p| format(body_mut(p, 0, "A"), 3..11, |r| r.italic = Some(true)));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    let chars = styles(&shape_named(p, 0, "A").text.as_ref().unwrap_or_else(|| panic!("text")).paragraphs[0]);
    assert_eq!(chars.len(), 11);
    for (i, (_, bold, italic)) in chars.iter().enumerate() {
        assert_eq!(*bold, i < 8, "bold at {i}: {chars:?}");
        assert_eq!(*italic, i >= 3, "italic at {i}: {chars:?}");
    }
}

#[test]
fn typing_at_a_bold_runs_end_extends_it() {
    let mut slide_file = slide_file();
    *body_mut(&mut slide_file, 0, "A") = TextBody::from_text("Bold");
    let (blobs, mut a, mut b) = pair_with(&slide_file);
    edit(&mut a, &blobs, |p| format(body_mut(p, 0, "A"), 0..4, |r| r.bold = Some(true)));
    sync(&mut a, &mut b);
    // A minimal client typing right after the bold range, with no marks of its own: `bold`
    // expands `after`.
    let shape = shape_object(&b, 0, "A");
    let paragraph = b.model().ids().body(&BodyOwner::Shape(shape)).unwrap_or_else(|| panic!("a body")).paragraphs[0].clone();
    // `execute`, not `record`: the write comes from the replica, so the view must take it too.
    b.execute(&person("b"), |_, tx| Ok(tx.text_insert(&paragraph, "text", 4, "er")?)).unwrap_or_else(|error| panic!("{error}"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let chars = styles(&shape_named(&a.model().slide_file, 0, "A").text.as_ref().unwrap_or_else(|| panic!("text")).paragraphs[0]);
    assert_eq!(chars.iter().map(|c| c.0).collect::<String>(), "Bolder");
    assert!(chars.iter().all(|c| c.1), "every character is bold: {chars:?}");
}

#[test]
fn a_paragraph_split_merges_with_concurrent_typing() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    // A presses Enter after "Hello ", B types at the start and at the end.
    edit(&mut a, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        let mut second = body.paragraphs[0].clone();
        second.runs = vec![Run::new("world")];
        set_plain(body, 0, "Hello ");
        body.paragraphs.insert(1, second);
    });
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Oh Hello world!"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    let body = shape_named(p, 0, "A").text.as_ref().unwrap_or_else(|| panic!("text"));
    assert_eq!(body.paragraphs.len(), 2, "{:?}", body.text());
    assert_eq!(body.paragraphs[0].text(), "Oh Hello ", "typed at the start: the first half's");
    assert_eq!(body.paragraphs[1].text(), "world!", "typed at the end: the second half's");
    assert_fresh(&a, &blobs);
    assert_fresh(&b, &blobs);
}

fn split_hello(p: &mut SlideFile) {
    let body = body_mut(p, 0, "A");
    let mut second = body.paragraphs[0].clone();
    second.runs = vec![Run::new("world")];
    set_plain(body, 0, "Hello ");
    body.paragraphs.insert(1, second);
}

fn paragraphs(s: &common::Session) -> Vec<String> {
    shape_named(&s.model().slide_file, 0, "A").text.as_ref().map(|b| b.paragraphs.iter().map(Paragraph::text).collect()).unwrap_or_default()
}

/// The view is what a fresh read of the replica gives.
fn assert_fresh(s: &common::Session, blobs: &SharedBlobs) {
    let (fresh, _) = read_document(s.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(&s.model().slide_file), "the view is the replica's");
}

#[test]
fn text_typed_in_the_moved_half_while_a_paragraph_splits_follows_the_second_half() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    edit(&mut a, &blobs, split_hello);
    // B, not yet seeing the split, types inside "world" and after it.
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello wor-ld and more"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let shown = paragraphs(&a);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert_eq!(shown[0], "Hello ", "nothing of the second half in the first: {shown:?}");
    assert!(shown[1].starts_with("world") || shown[1].starts_with("wor"), "{shown:?}");
    for typed in ["-", " and more"] {
        assert!(shown[1].contains(typed), "{typed:?} follows the second half: {shown:?}");
    }
    assert_fresh(&a, &blobs);
}

#[test]
fn text_typed_after_seeing_a_split_stays_where_it_was_typed() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    edit(&mut a, &blobs, split_hello);
    sync(&mut a, &mut b);
    // Both halves typed at their ends, after the split arrived.
    edit(&mut b, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        set_plain(body, 0, "Hello there");
        set_plain(body, 1, "world!");
    });
    edit(&mut a, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello, "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let shown = paragraphs(&a);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert!(shown[0] == "Hello, there" || shown[0] == "Hello there, " || shown[0].contains("there"), "{shown:?}");
    assert_eq!(shown[1], "world!");
    assert_fresh(&a, &blobs);
}

#[test]
fn a_tail_moves_with_its_paragraph_when_it_is_edited_merged_or_deleted() {
    let (blobs, mut a, mut b) = pair_with(&hello_slide_file());
    edit(&mut a, &blobs, split_hello);
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello world!"));
    sync(&mut a, &mut b);
    assert_eq!(paragraphs(&a), ["Hello ", "world!"]);
    // Typing at the end of the second half, around the text the first half's field holds.
    edit(&mut a, &blobs, |p| set_plain(body_mut(p, 0, "A"), 1, "big world!?"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&b), ["Hello ", "big world!?"]);
    assert_fresh(&b, &blobs);
    // Joining the halves (Backspace at the second's start) keeps every character once.
    edit(&mut b, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        let second = body.paragraphs.remove(1);
        body.paragraphs[0].runs.extend(second.runs);
        body.paragraphs[0].normalize();
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&a), ["Hello big world!?"]);
    assert_fresh(&a, &blobs);
    // Split again, the other side typing at the end, then the second half deleted: its tail goes too.
    edit(&mut a, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        let mut second = body.paragraphs[0].clone();
        second.runs = vec![Run::new("world!?")];
        set_plain(body, 0, "Hello big ");
        body.paragraphs.insert(1, second);
    });
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, "Hello big world!? Yes."));
    sync(&mut a, &mut b);
    assert_eq!(paragraphs(&a), ["Hello big ", "world!? Yes."]);
    edit(&mut b, &blobs, |p| {
        body_mut(p, 0, "A").paragraphs.remove(1);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&a), ["Hello big "]);
    assert_fresh(&a, &blobs);
}

fn two_paragraph_slide_file() -> SlideFile {
    let mut p = slide_file();
    let body = body_mut(&mut p, 0, "A");
    *body = TextBody::from_text("Hello world");
    let mut second = body.paragraphs[0].clone();
    second.runs = vec![Run::new("Second line")];
    body.paragraphs.push(second);
    p
}

/// Backspace at the start of the second paragraph.
fn merge_first_two(p: &mut SlideFile) {
    let body = body_mut(p, 0, "A");
    let second = body.paragraphs.remove(1);
    body.paragraphs[0].runs.extend(second.runs);
    body.paragraphs[0].normalize();
}

/// The paragraphs the body object lists, each with its field's text (what a 0.4.x build shows).
fn listed_paragraphs(s: &common::Session) -> Vec<String> {
    let shape = shape_object(s, 0, "A");
    let body = s.model().ids().body(&BodyOwner::Shape(shape)).unwrap_or_else(|| panic!("no body")).body.clone();
    let replica = s.replica();
    replica
        .list(&body, "paragraphs")
        .unwrap_or_default()
        .iter()
        .map(|p| replica.text(p, "text").unwrap_or_default().replace('\u{fffc}', ""))
        .collect()
}

#[test]
fn a_paragraph_merge_keeps_text_typed_concurrently_into_the_second_paragraph() {
    for (at, merged) in [(0, "Hello worldRRSecond line"), (3, "Hello worldSecRRond line"), (11, "Hello worldSecond lineRR")] {
        let (blobs, mut a, mut b) = pair_with(&two_paragraph_slide_file());
        edit(&mut a, &blobs, merge_first_two);
        assert_eq!(paragraphs(&a), ["Hello worldSecond line"]);
        // B, not yet seeing the merge, types at the start, in the middle or at the end of the second.
        let typed: String = "Second line".chars().take(at).chain("RR".chars()).chain("Second line".chars().skip(at)).collect();
        edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 1, &typed));
        sync(&mut a, &mut b);
        assert_converged(&a, &b);
        assert_eq!(paragraphs(&a), [merged], "typed at {at}");
        assert_fresh(&a, &blobs);
        assert_fresh(&b, &blobs);
        // A build that does not know merge anchors still lists both paragraphs: nothing is lost.
        assert_eq!(listed_paragraphs(&a), ["Hello world", typed.as_str()]);
        // Editing the merged paragraph afterwards writes into the fields its text lives in.
        edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 0, &format!("{merged}!")));
        sync(&mut a, &mut b);
        assert_converged(&a, &b);
        assert_eq!(paragraphs(&a), [format!("{merged}!")]);
        assert_fresh(&a, &blobs);
    }
}

#[test]
fn a_paragraph_merge_and_a_concurrent_split_of_the_second_paragraph_keep_every_character() {
    // The split keeps the second paragraph's object for its first half.
    let (blobs, mut a, mut b) = pair_with(&two_paragraph_slide_file());
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        let mut third = body.paragraphs[1].clone();
        third.runs = vec![Run::new("line")];
        set_plain(body, 1, "Second ");
        body.paragraphs.insert(2, third);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&a), ["Hello worldSecond ", "line"]);
    assert_fresh(&a, &blobs);
    assert_fresh(&b, &blobs);
    // The split keeps it for its second half: the halves may trade places, nothing is lost.
    let (blobs, mut a, mut b) = pair_with(&two_paragraph_slide_file());
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |p| {
        let body = body_mut(p, 0, "A");
        let mut first_half = body.paragraphs[1].clone();
        first_half.runs = vec![Run::new("Se")];
        set_plain(body, 1, "cond line");
        body.paragraphs.insert(1, first_half);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let shown = paragraphs(&a);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert!(shown[0].starts_with("Hello world"), "{shown:?}");
    let mut chars: Vec<char> = shown.concat().chars().collect();
    let mut expected: Vec<char> = "Hello worldSecond line".chars().collect();
    chars.sort_unstable();
    expected.sort_unstable();
    assert_eq!(chars, expected, "every character once: {shown:?}");
    assert_fresh(&a, &blobs);
    assert_fresh(&b, &blobs);
}

#[test]
fn undoing_a_paragraph_merge_keeps_text_typed_concurrently_into_the_second_paragraph() {
    let (blobs, mut a, mut b) = pair_with(&two_paragraph_slide_file());
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |p| set_plain(body_mut(p, 0, "A"), 1, "RRSecond line"));
    sync(&mut a, &mut b);
    assert_eq!(paragraphs(&a), ["Hello worldRRSecond line"]);
    let (undone, _) = a.undo().unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    assert_eq!(paragraphs(&a), ["Hello world", "RRSecond line"]);
    assert_fresh(&a, &blobs);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&b), ["Hello world", "RRSecond line"]);
    assert_fresh(&b, &blobs);
    // And redone.
    let (redone, _) = a.redo().unwrap_or_else(|error| panic!("redo: {error}"));
    assert!(redone);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(paragraphs(&b), ["Hello worldRRSecond line"]);
    assert_fresh(&a, &blobs);
    assert_fresh(&b, &blobs);
}

#[test]
fn typing_writes_only_the_marks_that_changed() {
    // Sizes are floats in the view and whole numbers in the replica: a write compares them as the
    // replica stores them, so typing re-marks nothing and a concurrent size change survives it.
    let mut slide_file = hello_slide_file();
    body_mut(&mut slide_file, 0, "A").paragraphs[0].runs = vec![Run::with("Hello world", RunProps { size: Some(18.0), ..Default::default() })];
    let (blobs, mut a, mut b) = pair_with(&slide_file);
    for k in 0..6 {
        // Later changes on A's side: A's write is the later one at every character.
        edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).name = format!("edit {k}"));
    }
    edit(&mut b, &blobs, |p| format(body_mut(p, 0, "A"), 0..5, |r| r.size = Some(24.0)));
    edit(&mut a, &blobs, |p| {
        let p = &mut body_mut(p, 0, "A").paragraphs[0];
        p.runs[0].text.push('!');
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &shape_named(&a.model().slide_file, 0, "A").text.as_ref().unwrap_or_else(|| panic!("text")).paragraphs[0];
    let sizes: Vec<(String, Option<f64>)> = p.runs.iter().map(|r| (r.text.clone(), r.props.size)).collect();
    assert_eq!(sizes, [("Hello".to_owned(), Some(24.0)), (" world!".to_owned(), Some(18.0))]);
}

#[test]
fn adjacent_runs_with_equal_formatting_read_the_same_on_every_replica_after_merges() {
    // Runs split where their formatting is equal (as files hold them), edited concurrently by
    // character, by run boundary and by formatting: every replica's view is the same, and is what
    // a fresh read of its replica gives.
    for seed in 1..=60u64 {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move || {
            rng ^= rng >> 12;
            rng ^= rng << 25;
            rng ^= rng >> 27;
            rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let mut slide_file = hello_slide_file();
        body_mut(&mut slide_file, 0, "A").paragraphs[0].runs = ["ab", "cd", "ef", "gh", "ij"].into_iter().map(Run::new).collect();
        let (blobs, mut a, mut b) = pair_with(&slide_file);
        for _ in 0..30 {
            let (op, at, who) = (next() % 4, next() as usize, next() % 2);
            let s = if who == 0 { &mut a } else { &mut b };
            edit(s, &blobs, |p| {
                let para = &mut body_mut(p, 0, "A").paragraphs[0];
                // Characters with the index of the run that holds them.
                let mut chars: Vec<(char, RunProps, usize)> = Vec::new();
                for (k, run) in para.runs.iter().enumerate() {
                    chars.extend(run.text.chars().map(|c| (c, run.props.clone(), k)));
                }
                let at = at % (chars.len() + 1);
                match op {
                    0 => {
                        let (props, k) = chars.get(at.saturating_sub(1)).map(|c| (c.1.clone(), c.2)).unwrap_or_default();
                        chars.insert(at, ('x', props, k));
                    }
                    1 if at < chars.len() => {
                        chars.remove(at);
                    }
                    2 if at < chars.len() => {
                        chars[at].1.bold = if chars[at].1.bold.is_some() { None } else { Some(true) };
                        chars[at].2 += 1000;
                    }
                    _ => chars.iter_mut().skip(at).for_each(|c| c.2 += 1),
                }
                let mut runs: Vec<Run> = Vec::new();
                let mut last = None;
                for (c, props, k) in chars {
                    match runs.last_mut() {
                        Some(run) if last == Some(k) && run.props == props => run.text.push(c),
                        _ => runs.push(Run::with(c.to_string(), props)),
                    }
                    last = Some(k);
                }
                para.runs = runs;
            });
            if next() % 5 == 0 {
                sync(&mut a, &mut b);
            }
        }
        sync(&mut a, &mut b);
        assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "seed {seed}");
        assert_eq!(canonical(&a.model().slide_file), canonical(&b.model().slide_file), "seed {seed}: same runs on both replicas");
        assert_fresh(&a, &blobs);
        assert_fresh(&b, &blobs);
    }
}

#[test]
fn concurrent_typing_in_one_table_cell_keeps_both() {
    let (blobs, mut a, mut b) = pair_with(&slide_file());
    let set = |text: &'static str| {
        move |p: &mut SlideFile| {
            if let Some(t) = table_mut(p) {
                t.rows[0].cells[0].text.paragraphs[0].runs = vec![Run::new(text)];
            }
        }
    };
    edit(&mut a, &blobs, set("A-r0c0"));
    edit(&mut b, &blobs, set("r0c0-B"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = &a.model().slide_file;
    let ShapeKind::Table(t) = &shape_named(p, 1, "Table").kind else { panic!("a table") };
    assert_eq!(t.rows[0].cells[0].text.text(), "A-r0c0-B");
    assert_eq!(t.rows[0].cells[1].text.text(), "r0c1");
}

#[test]
fn concurrent_typing_in_one_slides_notes_keeps_both() {
    let mut p = slide_file();
    Arc::make_mut(&mut p.slides[0]).notes = TextBody::from_text("Notes");
    let (blobs, mut a, mut b) = pair_with(&p);
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).notes.paragraphs[0].runs = vec![Run::new("My Notes")]);
    edit(&mut b, &blobs, |p| Arc::make_mut(&mut p.slides[0]).notes.paragraphs[0].runs = vec![Run::new("Notes here")]);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.model().slide_file.slides[0].notes.text(), "My Notes here");
}

// --- 0.3.x replicas (K6) ----------------------------------------------------------------------

fn schema_version(version: u32) -> Arc<Schema> {
    let current = format!("\"schemaVersion\": {}", SlideModel::schema().schema_version);
    let json = include_str!("../model/schema.json").replacen(&current, &format!("\"schemaVersion\": {version}"), 1);
    Arc::new(Schema::parse(&json).unwrap_or_else(|error| panic!("schema: {error}")))
}

/// Rewrites a replica's text bodies into the 0.3.x register form: a shape's `text`, a slide's
/// `notes`, a cell's inline `text` (the slide holds no ids inside its text).
fn to_legacy(replica: &mut Replica, blobs: &SharedBlobs) {
    let (p, ids) = read_document(replica, blobs).unwrap_or_else(|error| panic!("read: {error}"));
    replica
        .transact(&Origin::system("legacy"), |tx| {
            for slide in &p.slides {
                let Some(slide_object) = ids.slides.object(slide.id).cloned() else { continue };
                if let Some(body) = ids.body(&BodyOwner::Notes(slide_object.clone())) {
                    tx.set(&slide_object, "notesBody", Value::Null)?;
                    tx.delete(&body.body)?;
                }
                tx.set(&slide_object, "notes", json(&slide.notes))?;
                let mut shapes = Vec::new();
                kition_slide_model::walk(&slide.shapes, &mut |s, _| shapes.push(s.clone()));
                for shape in shapes {
                    let Some(object) = ids.shape(&slide_object, shape.id).cloned() else { continue };
                    if let Some(body) = ids.body(&BodyOwner::Shape(object.clone())) {
                        tx.set(&object, "body", Value::Null)?;
                        tx.delete(&body.body)?;
                        tx.set(&object, "text", json(&shape.text))?;
                    }
                    if let (ShapeKind::Table(t), Some(table)) = (&shape.kind, ids.table(&object)) {
                        for (row, row_object) in t.rows.iter().zip(&table.rows) {
                            for (cell, col_object) in row.cells.iter().zip(&table.cols) {
                                let key = format!("{}:{}", row_object.as_str(), col_object.as_str());
                                let mut v = tx.map_get(&table.table, "cells", &key)?;
                                if let Some(map) = v.as_object_mut()
                                    && let Some(Value::String(body)) = map.remove("body")
                                {
                                    tx.delete(&ObjectId::from_string(body))?;
                                    map.insert("text".into(), json(&cell.text));
                                }
                                tx.map_set(&table.table, "cells", &key, v)?;
                            }
                        }
                    }
                }
            }
            Ok(())
        })
        .unwrap_or_else(|error| panic!("legacy: {error}"));
}

fn json(v: &impl serde::Serialize) -> Value {
    serde_json::to_value(v).unwrap_or_default()
}

fn legacy_slide_file() -> SlideFile {
    let mut p = hello_slide_file();
    Arc::make_mut(&mut p.slides[0]).notes = TextBody::from_text("Speaker notes");
    p
}

/// A snapshot of a replica holding `p` in the 0.3.x form, and its blobs.
/// A 0.3.x document: stamped with schema version 1, its text bodies in the register form.
fn legacy_snapshot(p: &SlideFile) -> (Vec<u8>, MemoryBlobs) {
    let blobs = SharedBlobs::default();
    let (mut replica, ()) = Replica::create_with(schema_version(1), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
        write_document(tx, p, &mut Ids::default(), &mut blobs.clone()).map(|_| ())
    })
    .unwrap_or_else(|error| panic!("create: {error}"));
    to_legacy(&mut replica, &blobs);
    let map = blobs.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    (replica.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}")), MemoryBlobs(map))
}

/// How many text bodies `replica` still holds in the 0.3.x register form.
fn legacy_bodies(replica: &Replica) -> usize {
    let shapes = replica.objects_of_type("Shape").into_iter().filter(|s| !replica.get(s, "text").unwrap_or_default().is_null()).count();
    let notes = replica.objects_of_type("Slide").into_iter().filter(|s| !replica.get(s, "notes").unwrap_or_default().is_null()).count();
    let cells: usize = replica
        .objects_of_type("Table")
        .into_iter()
        .map(|t| replica.map_entries(&t, "cells").unwrap_or_default().into_iter().filter(|(_, c)| c.get("text").is_some()).count())
        .sum();
    shapes + notes + cells
}

#[test]
fn a_legacy_replica_reads_and_migrates_on_the_first_write() {
    let p = legacy_slide_file();
    let (snapshot, blobs) = legacy_snapshot(&p);
    let mut s = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("open: {error}"));
    assert_eq!(canonical(s.slide_file()), canonical(&p), "a 0.3.x replica reads as it was");
    assert!(s.model().ids().needs_migration());
    let legacy = legacy_bodies(s.replica());
    assert!(legacy > 2, "{legacy}");

    // A write that changes no text raises the schema version and migrates no text body.
    let mut after = s.slide_file().clone();
    Arc::make_mut(&mut after.slides[0]).name = "Renamed".into();
    assert_eq!(s.record(&person("a"), &after).unwrap_or_else(|error| panic!("record: {error}")), 1, "the migration is no undo step");
    // The text migration raises it to 2, the comments' (in the same `system` transaction) to 3.
    const { assert!(MIGRATED_SCHEMA_VERSION < COMMENTS_SCHEMA_VERSION) };
    assert_eq!(s.replica().min_schema_version(), COMMENTS_SCHEMA_VERSION);
    assert_eq!(legacy_bodies(s.replica()), legacy, "untouched text bodies stay registers");
    assert_replica_clean(s.replica());
    let (fresh, _) = read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(&after), "the replica holds the same slide");

    // A write that changes a text body migrates that one, in a `system` transaction.
    let mut typed = s.slide_file().clone();
    set_plain(body_mut(&mut typed, 0, "A"), 0, "Hello big world");
    Arc::make_mut(&mut typed.slides[0]).notes = TextBody::from_text("Speaker notes, edited");
    assert_eq!(s.record(&person("a"), &typed).unwrap_or_else(|error| panic!("record: {error}")), 1, "the migration is no undo step");
    assert_eq!(legacy_bodies(s.replica()), legacy - 2, "the shape's text and the notes moved to bodies");
    let shape = shape_object_of(&s, 0, "A");
    assert!(s.replica().get(&shape, "text").unwrap_or_default().is_null());
    assert!(s.model().ids().body(&BodyOwner::Shape(shape)).is_some());
    let (fresh, _) = read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(&typed));

    // Undo takes back the edits, not the migrations.
    assert!(s.undo().unwrap_or_else(|error| panic!("undo: {error}")).0);
    assert_eq!(text_of(s.slide_file(), 0, "A"), "Hello world");
    assert_eq!(s.slide_file().slides[0].notes.text(), "Speaker notes");
    assert_eq!(legacy_bodies(s.replica()), legacy - 2, "the migrated bodies stay");
    assert!(s.undo().unwrap_or_else(|error| panic!("undo: {error}")).0);
    assert_eq!(s.slide_file().slides[0].name, p.slides[0].name, "undo takes back the rename");
    assert!(!s.can_undo(), "the migrations are not undoable");
}

fn shape_object_of(s: &SlideSession, slide: usize, name: &str) -> ObjectId {
    let p = s.slide_file();
    let slide_object = s.model().ids().slides.object(p.slides[slide].id).unwrap_or_else(|| panic!("slide object")).clone();
    let id = shape_named(p, slide, name).id;
    s.model().ids().shape(&slide_object, id).unwrap_or_else(|| panic!("shape object")).clone()
}

#[test]
fn a_migrated_replica_is_refused_by_a_0_3_reader() {
    let p = legacy_slide_file();
    let (snapshot, blobs) = legacy_snapshot(&p);
    let actor = ActorId::from_u64(9).unwrap_or_else(|_| panic!("actor"));
    let mut old =
        Replica::load(schema_version(1), &snapshot, actor).unwrap_or_else(|error| panic!("a 0.3.x reader opens the 0.3.x replica: {error}"));
    assert_eq!(old.min_schema_version(), 1);

    let mut s = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("open: {error}"));
    assert_eq!(s.replica().min_schema_version(), 1, "reading does not raise the version");
    let since = s.version();
    let mut after = s.slide_file().clone();
    Arc::make_mut(&mut after.slides[0]).name = "Renamed".into();
    s.record(&person("a"), &after).unwrap_or_else(|error| panic!("record: {error}"));
    assert_eq!(s.replica().min_schema_version(), COMMENTS_SCHEMA_VERSION, "the migration raises the minimum schema version");

    let migrated = s.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    assert!(
        matches!(Replica::load(schema_version(1), &migrated, actor), Err(KdmError::NewerSchema { found, reads: 1 }) if found == u64::from(COMMENTS_SCHEMA_VERSION)),
        "a 0.3.x reader refuses to load the migrated replica"
    );
    let changes = s.export_changes(&since).unwrap_or_else(|error| panic!("export: {error}"));
    let before = old.version();
    assert!(
        matches!(old.import_changes(&changes), Err(KdmError::NewerSchema { found, reads: 1 }) if found == u64::from(COMMENTS_SCHEMA_VERSION)),
        "a 0.3.x reader refuses the migration's changes"
    );
    assert_eq!(old.version(), before, "the refused import changes nothing");
    assert_eq!(old.min_schema_version(), 1);
    assert!(SlideSession::open_snapshot(&migrated, s.blobs().clone()).is_ok(), "this build reads the migrated replica");
}

fn exchange(a: &mut SlideSession, b: &mut SlideSession) {
    for _ in 0..4 {
        let since = b.version();
        b.import_changes(&a.export_changes(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
        let since = a.version();
        a.import_changes(&b.export_changes(&since).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    }
    assert_eq!(a.version(), b.version(), "the replicas converged");
}

fn assert_same_sessions(a: &SlideSession, b: &SlideSession) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    assert_eq!(canonical(a.slide_file()), canonical(b.slide_file()));
    for s in [a, b] {
        let (fresh, _) = read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
        assert_eq!(canonical(&fresh), canonical(s.slide_file()), "the view is the replica's");
        assert_replica_clean(s.replica());
        assert_eq!(s.replica().objects_of_type("TextBody").len(), s.model().ids().body_count(), "no body is left over");
    }
}

/// A 0.3.x slide with a two-paragraph text box (shape `B`) and a table, opened twice.
fn two_legacy_sessions() -> (SlideSession, SlideSession, SlideFile) {
    let mut p = legacy_slide_file();
    *body_mut(&mut p, 0, "B") = TextBody::from_text("Title\nSubtitle");
    let (snapshot, blobs) = legacy_snapshot(&p);
    let a = SlideSession::open_snapshot(&snapshot, blobs.clone()).unwrap_or_else(|error| panic!("open: {error}"));
    let b = SlideSession::open_snapshot(&snapshot, blobs).unwrap_or_else(|error| panic!("open: {error}"));
    (a, b, p)
}

fn record_with(s: &mut SlideSession, who: &str, f: impl FnOnce(&mut SlideFile)) {
    let mut after = s.slide_file().clone();
    f(&mut after);
    s.record(&person(who), &after).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn two_peers_migrating_at_once_keep_both_sides_edits() {
    let (mut a, mut b, _) = two_legacy_sessions();
    // Offline: A edits text box A and B's first line, B edits B's second line and the notes.
    record_with(&mut a, "a", |p| {
        set_plain(body_mut(p, 0, "A"), 0, "Hello A world");
        set_plain(body_mut(p, 0, "B"), 0, "Title by A");
    });
    record_with(&mut b, "b", |p| {
        set_plain(body_mut(p, 0, "B"), 1, "Subtitle by B");
        Arc::make_mut(&mut p.slides[0]).notes = TextBody::from_text("Speaker notes by B");
    });
    exchange(&mut a, &mut b);
    assert_same_sessions(&a, &b);
    let view = a.slide_file();
    assert_eq!(text_of(view, 0, "A"), "Hello A world", "an edit only one side made survives");
    assert_eq!(text_of(view, 0, "B"), "Title by A\nSubtitle by B", "each side's line, once");
    assert_eq!(view.slides[0].notes.text(), "Speaker notes by B");
}

#[test]
fn a_paragraph_both_peers_changed_while_migrating_is_kept_twice() {
    let (mut a, mut b, _) = two_legacy_sessions();
    record_with(&mut a, "a", |p| set_plain(body_mut(p, 0, "B"), 0, "Title from a"));
    record_with(&mut b, "b", |p| set_plain(body_mut(p, 0, "B"), 0, "Title from b"));
    exchange(&mut a, &mut b);
    assert_same_sessions(&a, &b);
    let text = text_of(a.slide_file(), 0, "B");
    assert!(text == "Title from a\nTitle from b\nSubtitle" || text == "Title from b\nTitle from a\nSubtitle", "both versions: {text:?}");
    // Typing goes on in either version after the merge.
    record_with(&mut b, "b", |p| set_plain(body_mut(p, 0, "B"), 2, "Subtitle!"));
    exchange(&mut a, &mut b);
    assert!(text_of(a.slide_file(), 0, "B").ends_with("Subtitle!"));
}

#[test]
fn concurrent_migrations_of_table_cells_and_new_paragraphs_merge() {
    let (mut a, mut b, _) = two_legacy_sessions();
    let set_cell = |text: &'static str| {
        move |p: &mut SlideFile| {
            if let Some(t) = table_mut(p) {
                t.rows[0].cells[0].text.paragraphs[0].runs = vec![Run::new(text)];
            }
        }
    };
    record_with(&mut a, "a", set_cell("r0c0 by A"));
    record_with(&mut b, "b", |p| {
        // B adds a paragraph after the title and deletes the subtitle.
        let body = body_mut(p, 0, "B");
        body.paragraphs.truncate(1);
        let mut added = body.paragraphs[0].clone();
        added.runs = vec![Run::new("Added by B")];
        body.paragraphs.push(added);
    });
    record_with(&mut a, "a", |p| set_plain(body_mut(p, 0, "B"), 0, "Title by A"));
    exchange(&mut a, &mut b);
    assert_same_sessions(&a, &b);
    let view = a.slide_file();
    assert_eq!(text_of(view, 0, "B"), "Title by A\nAdded by B", "B's added paragraph after the title, B's deletion kept");
    let ShapeKind::Table(t) = &shape_named(view, 1, "Table").kind else { panic!("a table") };
    assert_eq!(t.rows[0].cells[0].text.text(), "r0c0 by A");
}

#[test]
fn a_replica_merged_without_a_session_is_repaired_when_opened() {
    let (mut a, mut b, _) = two_legacy_sessions();
    record_with(&mut a, "a", |p| set_plain(body_mut(p, 0, "B"), 0, "Title by A"));
    record_with(&mut b, "b", |p| set_plain(body_mut(p, 0, "B"), 1, "Subtitle by B"));
    // A store merges both replicas' changes, no kit session running.
    let mut stored = Replica::load(SlideModel::schema(), &a.snapshot().unwrap_or_else(|error| panic!("{error}")), ActorId::random())
        .unwrap_or_else(|error| panic!("{error}"));
    stored.import_changes(&b.export_changes(&stored.version()).unwrap_or_else(|error| panic!("{error}"))).unwrap_or_else(|error| panic!("{error}"));
    let mut blobs = a.blobs().clone();
    blobs.0.extend(b.blobs().0.clone());
    let c =
        SlideSession::open_snapshot(&stored.snapshot().unwrap_or_else(|error| panic!("{error}")), blobs).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text_of(c.slide_file(), 0, "B"), "Title by A\nSubtitle by B");
    let (fresh, _) = read_document(c.replica(), c.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&fresh), canonical(c.slide_file()), "the repair is in the replica");
}

#[test]
fn two_peers_repairing_the_same_merge_at_once_converge() {
    let (mut a, mut b, _) = two_legacy_sessions();
    record_with(&mut a, "a", |p| set_plain(body_mut(p, 0, "B"), 0, "Title by A"));
    record_with(&mut b, "b", |p| set_plain(body_mut(p, 0, "B"), 1, "Subtitle by B"));
    // Each imports the other's migration before seeing the other's repair.
    let (to_b, to_a) = (
        a.export_changes(&b.version()).unwrap_or_else(|error| panic!("{error}")),
        b.export_changes(&a.version()).unwrap_or_else(|error| panic!("{error}")),
    );
    a.import_changes(&to_a).unwrap_or_else(|error| panic!("{error}"));
    b.import_changes(&to_b).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text_of(a.slide_file(), 0, "B"), "Title by A\nSubtitle by B");
    assert_eq!(text_of(b.slide_file(), 0, "B"), "Title by A\nSubtitle by B");
    exchange(&mut a, &mut b);
    assert_same_sessions(&a, &b);
    assert_eq!(text_of(a.slide_file(), 0, "B"), "Title by A\nSubtitle by B");
}

#[test]
fn a_0_3_build_refuses_a_new_replica_and_this_one_reads_an_old_one() {
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&slide_file(), &blobs);
    let snapshot = replica.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let actor = ActorId::from_u64(9).unwrap_or_else(|_| panic!("actor"));
    assert!(Replica::load(schema_version(1), &snapshot, actor).is_err(), "schema version 1 refuses a version 2 replica");

    let old = Replica::create(schema_version(1), DocId::new(), actor).unwrap_or_else(|error| panic!("{error}"));
    let bytes = old.snapshot().unwrap_or_else(|error| panic!("{error}"));
    assert!(Replica::load(SlideModel::schema(), &bytes, actor).is_ok(), "schema version 2 reads a version 1 replica");
}
