//! Text bodies in the paragraph form: two replicas typing into one text box, one table cell or
//! one slide's notes keep both texts; bold and italic over overlapping ranges both apply; typing
//! at a bold run's end extends it; a paragraph split or merge (and a merge's undo) merges with
//! concurrent typing.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_replica_clean, canonical, edit, first, peer, person, shape_named, shape_object, slide_file, sync, table_mut};
use kdm_model::{BodyOwner, read_document};
use kition_slide_model::text::{Paragraph, Run, RunProps, TextBody};
use kition_slide_model::{ShapeKind, SlideFile};

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
