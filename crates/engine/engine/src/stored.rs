//! The document in memory is the document its saved file opens as (task card C8-slide-5).
//!
//! The agents' premise guard compares the document a kit holds with the document the kit reads
//! when another program changed its file: any difference between what the kit holds and what its
//! own saved file opens as reads there as a change by someone else, and a write on such an object
//! is refused. So a value enters the model as the file stores it, at the moment it enters:
//!
//! - Numbers the file counts in whole units are quantised to them, one function per kind of
//!   quantity (`kition_slide_geom::stored`): lengths in EMU (a shape's frame, a group's child
//!   frame, a table's column widths and row heights, text insets and column spacing), rotations in
//!   1/60,000 degree within 0..360, shrink-to-fit scales in 1/100,000.
//! - What the file writes explicitly is explicit in the model: the anchor of a shape's own text
//!   (written middle, read top for a text box), the id a slide is written with (its own id: a
//!   copy does not carry its source's), and the notes master a file with notes gets.
//! - Whether a body's autofit result is a saved one, where a reader would infer otherwise (a result
//!   Kition cleared), is written into the text body's extension list under Kition's own URI
//!   (`kition_slide_pptx`'s `autofit_ext`). (A shrink result with no flag at all, which a refit
//!   never leaves, reads back as a saved one.)
//! - A derived value the file does not store is recomputed identically on load: the receipt of the
//!   inputs a shrink-to-fit result was computed from (`autofit::receipts`).
//!
//! [`settle`] applies the rule to what a command changed (the slides, masters and layouts not
//! shared with the document before it) and to a whole document when it is opened.

use std::collections::HashSet;
use std::sync::Arc;

use kition_slide_geom::{Xfrm, stored};
use kition_slide_model::text::{Anchor, AutoFit, BodyProps, Paragraph, TextBody};
use kition_slide_model::{Shape, ShapeKind, Slide, SlideFile};

/// Settles `doc` to what its file stores: every slide, master and layout `old` does not share
/// (all of them when there is no `old`). Answers whether anything changed.
pub(crate) fn settle(doc: &mut SlideFile, old: Option<&SlideFile>) -> bool {
    let kept: HashSet<*const Slide> = old.map(|old| old.slides.iter().map(Arc::as_ptr).collect()).unwrap_or_default();
    let mut changed = file_ids(doc, old.is_none());
    // On open, identities are derived (the same file, the same ids); an edit's new shapes draw
    // theirs, as the replica draws object ids.
    changed |= match old {
        None => kition_slide_model::identity::settle(doc),
        Some(old) => {
            kition_slide_model::identity::settle_with(doc, &mut || Some(kdm_model::kition_kit::kdm::ObjectId::new().as_str().to_owned()), Some(old))
        }
    };
    for slide in &mut doc.slides {
        if kept.contains(&Arc::as_ptr(slide)) {
            continue;
        }
        changed |= in_place(slide, |slide| settle_slide(slide, &doc.media));
    }
    let masters: HashSet<*const kition_slide_model::Master> = old.map(|old| old.masters.iter().map(Arc::as_ptr).collect()).unwrap_or_default();
    for master in &mut doc.masters {
        if masters.contains(&Arc::as_ptr(master)) {
            continue;
        }
        changed |= in_place(master, |master| {
            let mut changed = shapes(&mut master.shapes, &doc.media);
            for layout in &mut master.layouts {
                changed |= shapes(&mut layout.shapes, &doc.media);
            }
            changed
        });
    }
    changed |= notes_master(doc);
    changed |= kition_slide_pptx::stored::settle_diagrams(doc);
    changed
}

/// Applies `settle` to the value behind `arc`, cloning it only when it changes something.
fn in_place<T: Clone>(arc: &mut Arc<T>, settle: impl Fn(&mut T) -> bool) -> bool {
    if let Some(unique) = Arc::get_mut(arc) {
        return settle(unique);
    }
    let mut copy = (**arc).clone();
    if settle(&mut copy) {
        *arc = Arc::new(copy);
        true
    } else {
        false
    }
}

/// The id each slide is written with (`p:sldId/@id`), which it reads back with as its id: its own.
/// A slide made since the file was read has none yet, a copy carries its source's (the writer
/// would give the first of the two in slide order the source's id, the other a new one), and a
/// file's id the reader did not keep (a repeat, a shape's id, one out of range) is not the slide's;
/// each gets its own. When the deck is opened (`all`), every slide does. Not on every write: a
/// slide's id here is this replica's, and a peer that numbered it otherwise must not write its own
/// back and forth.
fn file_ids(doc: &mut SlideFile, all: bool) -> bool {
    // The ids a slide holds as its own: a copy of such a slide is the repeat, wherever it is.
    let owned: HashSet<u32> = doc.slides.iter().filter(|slide| slide.file_id == Some(slide.id.0)).map(|slide| slide.id.0).collect();
    let mut seen = HashSet::new();
    let mut changed = false;
    for slide in &mut doc.slides {
        let own = Some(slide.id.0);
        if slide.file_id == own {
            continue;
        }
        let repeat = slide.file_id.is_some_and(|id| owned.contains(&id) || !seen.insert(id));
        if all || slide.file_id.is_none() || repeat {
            Arc::make_mut(slide).file_id = own;
            changed = true;
        }
    }
    changed
}

/// [`settle`] for a document read from its replica after changes another replica made (an
/// import): the slides' file ids first, as every replica settles them alike.
pub(crate) fn settle_merged(doc: &mut SlideFile, old: &SlideFile) -> bool {
    merged_file_ids(doc) | settle(doc, Some(old))
}

/// The file ids of a merged deck, a function of the deck. A draft and the document that each added
/// a slide gave both slides the same id (each its next number), and this replica's numbers for the
/// two slides are its own: giving the repeat this replica's number for it, as [`file_ids`] does for
/// a local copy, would depend on the replica. So a repeat takes what the writer gives it: the first
/// slide in slide order keeps the id, each other one the next id above every slide's. (A slide with
/// none is [`file_ids`]'s.) Tested by `kition-slide-kit`'s `tests/concurrent_slides.rs`.
fn merged_file_ids(doc: &mut SlideFile) -> bool {
    const MAX: u32 = 2_147_483_647;
    let mut next = doc.slides.iter().filter_map(|slide| slide.file_id).max().map_or(256, |max| max.saturating_add(1)).max(256);
    // The ids `file_ids` gives the slides with none (their own) are taken too.
    let taken: HashSet<u32> = doc.slides.iter().filter(|slide| slide.file_id.is_none()).map(|slide| slide.id.0).collect();
    let mut used = HashSet::new();
    let mut changed = false;
    for slide in &mut doc.slides {
        let Some(id) = slide.file_id else { continue };
        if used.insert(id) {
            continue;
        }
        // Ends: `used` and `taken` hold at most one id per slide.
        while next > MAX || used.contains(&next) || taken.contains(&next) {
            next = if next > MAX { 256 } else { next + 1 };
        }
        used.insert(next);
        Arc::make_mut(slide).file_id = Some(next);
        changed = true;
    }
    changed
}

fn settle_slide(slide: &mut Slide, media: &[kition_slide_model::MediaItem]) -> bool {
    shapes(&mut slide.shapes, media) | kition_slide_pptx::stored::settle_slide(slide)
}

fn shapes(shapes: &mut [Shape], media: &[kition_slide_model::MediaItem]) -> bool {
    let mut changed = false;
    for shape in shapes {
        changed |= settle_shape(shape, media);
    }
    changed
}

/// Sets `value` to what `stored` makes of it; answers whether that changed it.
fn set(value: &mut f64, stored: impl Fn(f64) -> f64) -> bool {
    let to = stored(*value);
    // `!=` on purpose: -0.0 and 0.0 read alike, and NaN is never stored (the writer writes 0).
    if *value != to || value.is_nan() {
        *value = to;
        true
    } else {
        false
    }
}

fn set_opt(value: &mut Option<f64>, settle: impl Fn(f64) -> f64) -> bool {
    match value {
        Some(v) => set(v, settle),
        None => false,
    }
}

/// A shape's frame: a position, a size that is never negative, a rotation in 0..360.
fn frame(x: &mut Xfrm) -> bool {
    set(&mut x.x, stored::length)
        | set(&mut x.y, stored::length)
        | set(&mut x.w, stored::extent)
        | set(&mut x.h, stored::extent)
        | set(&mut x.rot, stored::angle)
}

/// A group's child space: no rotation of its own.
fn child_frame(x: &mut Xfrm) -> bool {
    set(&mut x.x, stored::length) | set(&mut x.y, stored::length) | set(&mut x.w, stored::extent) | set(&mut x.h, stored::extent)
}

fn settle_shape(shape: &mut Shape, media: &[kition_slide_model::MediaItem]) -> bool {
    let diagram_valid = shape.diagram.as_ref().map(|diagram| diagram.fingerprint == kition_slide_model::shapes_fingerprint(shape.children(), media));
    let mut changed = shape.xfrm.as_mut().is_some_and(frame);
    match &mut shape.kind {
        ShapeKind::Group { children, child } => {
            changed |= child_frame(child);
            changed |= shapes(children, media);
        }
        ShapeKind::Table(table) => {
            for width in &mut table.cols {
                changed |= set(width, stored::extent);
            }
            for row in &mut table.rows {
                changed |= set(&mut row.height, stored::extent);
                for cell in &mut row.cells {
                    if let Some(margins) = cell.margins.as_mut() {
                        for margin in margins {
                            changed |= set(margin, stored::length);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    match diagram_valid {
        Some(true) => {
            let fingerprint = kition_slide_model::shapes_fingerprint(shape.children(), media);
            if let Some(diagram) = shape.diagram.as_mut()
                && diagram.fingerprint != fingerprint
            {
                diagram.fingerprint = fingerprint;
                changed = true;
            }
        }
        Some(false) => {
            shape.diagram = None;
            changed = true;
        }
        None => {}
    }
    // A shape's own text without an anchor: the writer writes middle for an autoshape (Kition's
    // default for it) and nothing for a text box, which the reader reads as top.
    let own = shape.ph.is_none() && matches!(shape.kind, ShapeKind::Shape);
    let text_box = shape.text_box;
    if let Some(body) = shape.text.as_mut() {
        changed |= text(body);
        if own && body.body.anchor.is_none() {
            body.body.anchor = Some(if text_box { Anchor::Top } else { Anchor::Middle });
            changed = true;
        }
    }
    changed
}

/// A shape's text: its body properties, and at least one paragraph (a file's text body has one).
fn text(body: &mut TextBody) -> bool {
    let empty = body.paragraphs.is_empty();
    if empty {
        body.paragraphs.push(Paragraph::default());
    }
    body_props(&mut body.body) | empty
}

/// Insets and column spacing in EMU, a text rotation in 1/60,000 degree, a shrink scale in
/// 1/100,000 (read back clamped as the reader clamps it).
fn body_props(b: &mut BodyProps) -> bool {
    let mut changed = set_opt(&mut b.inset_l, stored::length)
        | set_opt(&mut b.inset_t, stored::length)
        | set_opt(&mut b.inset_r, stored::length)
        | set_opt(&mut b.inset_b, stored::length)
        | set_opt(&mut b.col_spacing, stored::extent)
        | set_opt(&mut b.rot, body_rotation);
    if let Some(AutoFit::Shrink { font_scale, line_reduction }) = &mut b.autofit {
        changed |= set(font_scale, |v| stored::fraction(v).clamp(0.01, 1.0));
        changed |= set(line_reduction, |v| stored::fraction(v).clamp(0.0, 1.0));
    }
    changed
}

/// A text body's own rotation (`bodyPr/@rot`): whole 1/60,000 degree, within ±360, not wrapped.
fn body_rotation(deg: f64) -> f64 {
    if !deg.is_finite() {
        return 0.0;
    }
    (deg * 60_000.0).round().clamp(-21_600_000.0, 21_600_000.0) / 60_000.0
}

/// A deck with notes is written with a notes master (the file's, else a default one): the model
/// holds the one its file will read back with.
fn notes_master(doc: &mut SlideFile) -> bool {
    if doc.notes_master.is_some() || !doc.slides.iter().any(|slide| !slide.notes.is_empty() || slide.notes_page.is_some()) {
        return false;
    }
    match kition_slide_pptx::default_notes_master(doc) {
        Some(master) => {
            doc.notes_master = Some(Arc::new(master));
            true
        }
        None => false,
    }
}
