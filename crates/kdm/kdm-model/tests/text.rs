//! Text bodies in the paragraph form: typing at a bold run's end extends it, on a replica that
//! took the bold from another.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_replica_clean, canonical, edit, first, peer, person, shape_named, shape_object, slide_file, sync};
use kdm_model::BodyOwner;
use kition_slide_model::SlideFile;
use kition_slide_model::text::{Paragraph, Run, RunProps, TextBody};

fn body_mut<'a>(p: &'a mut SlideFile, slide: usize, name: &str) -> &'a mut TextBody {
    let s = Arc::make_mut(&mut p.slides[slide]);
    s.shapes.iter_mut().find(|s| s.name == name).and_then(|s| s.text.as_mut()).unwrap_or_else(|| panic!("no text in {name}"))
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
