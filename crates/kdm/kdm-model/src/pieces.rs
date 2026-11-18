//! What a paragraph's `text` field holds beyond its own characters: split anchors, so text typed
//! at the end of a paragraph while someone else splits it lands at the end of the second half, and
//! merge anchors, so text typed into a paragraph while someone else merges it into the one before
//! stays in it.
//!
//! **Split anchors.** A paragraph split keeps the first half's object (when that half is the larger
//! one, see [`crate::text::align_paragraphs`]): the moved text is deleted from its field and copied
//! into a new paragraph. Characters another person types concurrently after the split point were
//! placed next to the deleted ones, so on their own they would surface at the end of the first
//! half. The split therefore first inserts a **split anchor** at the split point, a `U+FFFC` whose
//! `inline` mark is `{"splitTo": <new paragraph's object>}`, and only then deletes the moved text:
//! every character typed concurrently anywhere after the split point sits after the anchor, while
//! text typed later at the end of the first half goes before it.
//!
//! **Merge anchors.** A paragraph merge (Backspace at a paragraph's start, Delete at its end) is
//! the split's mirror: it keeps both objects. The second paragraph stays listed in the body with
//! its field untouched, and the first gets a **merge anchor** where its text ends, a `U+FFFC`
//! whose `inline` mark is `{"mergeFrom": <second paragraph's object>}`. A listed paragraph a merge
//! anchor names is **absorbed**: it is not a paragraph of its own but shows, with everything it
//! shows, at the anchor. Text typed concurrently anywhere into the second paragraph (its start, its
//! middle, its end) is in the second paragraph's field, so it shows in the merged paragraph where
//! it was typed. Undoing the merge removes the anchor and the second paragraph is its own again.
//!
//! So a field is **segments** between anchors. The segments before the first split anchor are the
//! paragraph's own text (region 0); each split anchor's region (its **tail**) runs to the next
//! split anchor. [`layout`] shows each tail at the end of the paragraph its anchor names when that
//! paragraph is shown in the same body, else (it was deleted) at the end of the anchor's own
//! paragraph, and each absorbed paragraph at the merge anchor naming it (the first such anchor in
//! the order paragraphs are placed; one naming a paragraph already placed, or not listed, shows
//! nothing). A view paragraph is thus a list of [`Piece`]s (segments of fields); [`assign`] maps an
//! edit of the whole view paragraph back onto them, so a write puts every character into the field
//! it lives in.
//!
//! A build that does not know anchors reads one as an empty inline run (an `inline` value that is
//! not a run reads as an empty one), a tail as text at the end of the first half, and an absorbed
//! paragraph as the paragraph it was before the merge: nothing is lost.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ObjectId, TextRun};
use kition_kit::session::diff_text;
use serde_json::{Map, Value};

use crate::text::{INLINE, INLINE_MARK};

/// The key of a split anchor's `inline` mark value.
pub(crate) const SPLIT_TO: &str = "splitTo";
/// The key of a merge anchor's `inline` mark value.
pub(crate) const MERGE_FROM: &str = "mergeFrom";

/// A paragraph object's field as last read or written: what the next write diffs from.
pub(crate) type Truth = Arc<Vec<TextRun>>;

/// An anchor in a paragraph's field and the paragraph it names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Anchor {
    Split(ObjectId),
    Merge(ObjectId),
}

impl Anchor {
    /// The anchor as the one character of its field.
    pub fn run(&self) -> TextRun {
        let (key, target) = match self {
            Self::Split(target) => (SPLIT_TO, target),
            Self::Merge(target) => (MERGE_FROM, target),
        };
        let mut value = Map::new();
        value.insert(key.into(), Value::String(target.as_str().to_owned()));
        let mut attributes = std::collections::BTreeMap::new();
        attributes.insert(INLINE_MARK.to_owned(), Value::Object(value));
        TextRun { insert: INLINE.to_string(), attributes }
    }

    pub fn target(&self) -> &ObjectId {
        match self {
            Self::Split(target) | Self::Merge(target) => target,
        }
    }
}

/// One part of a view paragraph: segment `seg` of a paragraph object's field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Piece {
    pub object: ObjectId,
    pub seg: usize,
}

impl Piece {
    pub fn own(object: &ObjectId) -> Self {
        Self { object: object.clone(), seg: 0 }
    }
}

/// A paragraph field split at its anchors: `segments[k]` is followed by `anchors[k]`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Composition {
    pub segments: Vec<Vec<TextRun>>,
    pub anchors: Vec<Anchor>,
}

impl Default for Composition {
    fn default() -> Self {
        Self { segments: vec![Vec::new()], anchors: Vec::new() }
    }
}

impl Composition {
    /// The field again: each segment, then the anchor after it.
    pub fn runs(&self) -> Vec<TextRun> {
        let mut out = Vec::new();
        for (k, segment) in self.segments.iter().enumerate() {
            out.extend(segment.iter().cloned());
            if let Some(anchor) = self.anchors.get(k) {
                out.push(anchor.run());
            }
        }
        canon(out)
    }

    /// The runs of one segment of this field.
    pub fn segment(&self, seg: usize) -> &[TextRun] {
        self.segments.get(seg).map_or(&[][..], Vec::as_slice)
    }

    pub fn segment_mut(&mut self, seg: usize) -> Option<&mut Vec<TextRun>> {
        self.segments.get_mut(seg)
    }

    /// The region of segment `seg`: how many split anchors come before it.
    pub fn region(&self, seg: usize) -> usize {
        self.anchors.iter().take(seg).filter(|a| matches!(a, Anchor::Split(_))).count()
    }

    /// The field position where segment `seg` starts.
    pub fn start_of(&self, seg: usize) -> usize {
        self.segments.iter().take(seg).map(|s| char_len(s)).sum::<usize>() + seg.min(self.anchors.len())
    }

    /// Puts `anchor` at character `offset` of segment `seg`, splitting it.
    pub fn insert_anchor_at(&mut self, seg: usize, offset: usize, anchor: Anchor) {
        let Some(segment) = self.segments.get_mut(seg) else { return };
        let len = char_len(segment);
        let right = slice(segment, offset.min(len), len);
        *segment = slice(segment, 0, offset.min(len));
        self.anchors.insert(seg, anchor);
        self.segments.insert(seg + 1, right);
    }

    /// Puts `anchor` right after segment `seg` (an empty segment follows it).
    pub fn insert_anchor(&mut self, seg: usize, anchor: Anchor) {
        let seg = seg.min(self.anchors.len());
        self.anchors.insert(seg, anchor);
        self.segments.insert(seg + 1, Vec::new());
    }

    /// Drops anchor `k`, joining the segments around it.
    pub fn remove_anchor(&mut self, k: usize) {
        if k >= self.anchors.len() {
            return;
        }
        self.anchors.remove(k);
        let next = self.segments.remove(k + 1);
        if let Some(segment) = self.segments.get_mut(k) {
            segment.extend(next);
        }
    }
}

/// The anchor a run's `inline` mark makes it, if it is one.
fn anchor_of(run: &TextRun) -> Option<Anchor> {
    let value = run.attributes.get(INLINE_MARK)?;
    if let Some(target) = value.get(SPLIT_TO).and_then(Value::as_str) {
        return Some(Anchor::Split(ObjectId::from_string(target)));
    }
    value.get(MERGE_FROM).and_then(Value::as_str).map(|target| Anchor::Merge(ObjectId::from_string(target)))
}

/// Splits a field's runs at its anchors.
pub(crate) fn compose(runs: &[TextRun]) -> Composition {
    let mut c = Composition::default();
    for run in runs {
        let Some(anchor) = anchor_of(run) else {
            push(&mut c, run.clone());
            continue;
        };
        // Each anchor character is an anchor; anything else in the run (no writer makes it) is
        // text, without the mark.
        let mut text = String::new();
        for ch in run.insert.chars() {
            if ch == INLINE {
                if !text.is_empty() {
                    push(&mut c, plain_part(run, std::mem::take(&mut text)));
                }
                c.anchors.push(anchor.clone());
                c.segments.push(Vec::new());
            } else {
                text.push(ch);
            }
        }
        if !text.is_empty() {
            push(&mut c, plain_part(run, text));
        }
    }
    c
}

fn plain_part(run: &TextRun, insert: String) -> TextRun {
    let mut attributes = run.attributes.clone();
    attributes.remove(INLINE_MARK);
    TextRun { insert, attributes }
}

fn push(c: &mut Composition, run: TextRun) {
    if let Some(segment) = c.segments.last_mut() {
        segment.push(run);
    }
}

/// Runs without empty ones, mark values in the form the replica stores them (whole floats as
/// integers), adjacent runs with the same marks joined: the form a field's delta takes, so two run
/// lists of one text compare equal, and a write sets only marks whose value really changed.
pub(crate) fn canon(runs: Vec<TextRun>) -> Vec<TextRun> {
    let mut out: Vec<TextRun> = Vec::with_capacity(runs.len());
    for mut run in runs {
        if run.insert.is_empty() {
            continue;
        }
        for value in run.attributes.values_mut() {
            *value = canonical(std::mem::take(value));
        }
        match out.last_mut() {
            Some(last) if last.attributes == run.attributes => last.insert.push_str(&run.insert),
            _ => out.push(run),
        }
    }
    out
}

/// `value` as the replica stores it: whole floats as integers (object keys are sorted already).
fn canonical(value: Value) -> Value {
    const SAFE: f64 = 9_007_199_254_740_992.0;
    match value {
        Value::Number(number) if !number.is_i64() && !number.is_u64() => match number.as_f64() {
            #[allow(clippy::cast_possible_truncation)]
            Some(float) if float.fract() == 0.0 && float.abs() <= SAFE => Value::from(float as i64),
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(fields) => Value::Object(fields.into_iter().map(|(key, value)| (key, canonical(value))).collect()),
        other => other,
    }
}

pub(crate) fn char_len(runs: &[TextRun]) -> usize {
    runs.iter().map(|r| r.insert.chars().count()).sum()
}

pub(crate) fn text_of(runs: &[TextRun]) -> String {
    runs.iter().map(|r| r.insert.as_str()).collect()
}

/// The runs covering characters `start..end` of `runs`.
pub(crate) fn slice(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let len = run.insert.chars().count();
        let (from, to) = (start.max(at), end.min(at + len));
        if from < to {
            let insert: String = run.insert.chars().skip(from - at).take(to - from).collect();
            out.push(TextRun { insert, attributes: run.attributes.clone() });
        }
        at += len;
    }
    out
}

/// How one body shows its paragraphs (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    /// The view's paragraphs, in list order: the listed paragraphs not absorbed.
    pub roots: Vec<ObjectId>,
    /// The pieces of each view paragraph that shows more than its own field's first segment.
    pub pieces: HashMap<ObjectId, Vec<Piece>>,
    /// Each absorbed paragraph and the view paragraph showing it, in the order they are placed.
    pub absorbed: Vec<(ObjectId, ObjectId)>,
}

/// The pieces of every paragraph of one body (`listed`, each field composed in `fields`; see the
/// module docs): each view paragraph shows its own region, the paragraphs absorbed at its merge
/// anchors where they stand, then the tails of split anchors naming it (by their paragraphs in the
/// order paragraphs are placed), and its own tails whose target is not shown in this body.
pub(crate) fn layout(listed: &[ObjectId], fields: &HashMap<ObjectId, Composition>) -> Layout {
    let is_listed: HashSet<&ObjectId> = listed.iter().filter(|p| fields.contains_key(*p)).collect();
    // Listed paragraphs a merge anchor of another listed paragraph names.
    let mut named: HashSet<&ObjectId> = HashSet::new();
    for source in &is_listed {
        for anchor in &fields[*source].anchors {
            if let Anchor::Merge(target) = anchor
                && target != *source
                && is_listed.contains(target)
            {
                named.insert(target);
            }
        }
    }
    let mut placer = Placer { fields, named: &named, placed: HashMap::new(), order: Vec::new(), include: HashMap::new() };
    let mut root_set: HashSet<&ObjectId> = HashSet::new();
    for source in listed.iter().filter(|p| is_listed.contains(p)) {
        if !named.contains(source) && !placer.placed.contains_key(source) {
            root_set.insert(source);
            placer.place(source, source, 0);
        }
    }
    // Paragraphs absorbed only in a cycle of merge anchors: the first in the list is shown.
    for source in listed.iter().filter(|p| is_listed.contains(p)) {
        if !placer.placed.contains_key(source) {
            root_set.insert(source);
            placer.place(source, source, 0);
        }
    }
    let Placer { placed, order, include, .. } = placer;
    // The view paragraph showing each split tail: its target's, when the target is shown.
    let mut held: HashMap<&ObjectId, Vec<(&ObjectId, usize)>> = HashMap::new();
    for source in &order {
        let c = &fields[*source];
        let mut region = 0;
        for anchor in &c.anchors {
            if let Anchor::Split(target) = anchor {
                region += 1;
                let holder = if target != *source && placed.contains_key(target) { target } else { *source };
                held.entry(holder).or_default().push((*source, region));
            }
        }
    }
    let expander = Expander { fields, include: &include, held: &held };
    let mut done: HashSet<(&ObjectId, usize)> = HashSet::new();
    let roots: Vec<ObjectId> = listed.iter().filter(|p| root_set.contains(p)).cloned().collect();
    let mut views: HashMap<&ObjectId, Vec<Piece>> = HashMap::new();
    for root in &roots {
        let mut out = Vec::new();
        expander.expand(root, &mut out, &mut done, 0);
        views.insert(root, out);
    }
    // Safety: a region no expansion reached shows at the end of its paragraph's view paragraph.
    for source in &order {
        let c = &fields[*source];
        let regions = c.region(c.segments.len().saturating_sub(1));
        for region in 0..=regions {
            if !done.contains(&(*source, region))
                && let Some(view) = placed.get(*source).and_then(|root| views.get_mut(*root))
            {
                done.insert((*source, region));
                view.extend(
                    c.segments.iter().enumerate().filter(|(k, _)| c.region(*k) == region).map(|(k, _)| Piece { object: (*source).clone(), seg: k }),
                );
            }
        }
    }
    let mut pieces = HashMap::new();
    for root in &roots {
        if let Some(view) = views.remove(root)
            && view != [Piece::own(root)]
        {
            pieces.insert(root.clone(), view);
        }
    }
    let absorbed = order.iter().filter(|p| !root_set.contains(*p)).map(|p| ((*p).clone(), placed[*p].clone())).collect();
    Layout { roots, pieces, absorbed }
}

/// Places paragraphs: each view paragraph, then the paragraphs absorbed at its merge anchors.
struct Placer<'a> {
    fields: &'a HashMap<ObjectId, Composition>,
    named: &'a HashSet<&'a ObjectId>,
    /// Each placed paragraph's view paragraph.
    placed: HashMap<&'a ObjectId, &'a ObjectId>,
    order: Vec<&'a ObjectId>,
    /// The paragraph each merge anchor (paragraph, anchor index) shows.
    include: HashMap<(&'a ObjectId, usize), &'a ObjectId>,
}

impl<'a> Placer<'a> {
    fn place(&mut self, object: &'a ObjectId, root: &'a ObjectId, depth: usize) {
        self.placed.insert(object, root);
        self.order.push(object);
        if depth > MAX_DEPTH {
            return;
        }
        let Some(c) = self.fields.get(object) else { return };
        for (k, anchor) in c.anchors.iter().enumerate() {
            if let Anchor::Merge(target) = anchor
                && let Some(target) = self.named.get(target).copied()
                && !self.placed.contains_key(target)
            {
                self.include.insert((object, k), target);
                self.place(target, root, depth + 1);
            }
        }
    }
}

const MAX_DEPTH: usize = 256;

struct Expander<'a, 'b> {
    fields: &'a HashMap<ObjectId, Composition>,
    include: &'b HashMap<(&'a ObjectId, usize), &'a ObjectId>,
    held: &'b HashMap<&'a ObjectId, Vec<(&'a ObjectId, usize)>>,
}

impl<'a> Expander<'a, '_> {
    /// A paragraph's view: its own region, then the tails it holds.
    fn expand(&self, object: &'a ObjectId, out: &mut Vec<Piece>, done: &mut HashSet<(&'a ObjectId, usize)>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        self.region(object, 0, out, done, depth);
        for &(source, region) in self.held.get(object).map_or(&[][..], Vec::as_slice) {
            self.region(source, region, out, done, depth);
        }
    }

    fn region(&self, object: &'a ObjectId, region: usize, out: &mut Vec<Piece>, done: &mut HashSet<(&'a ObjectId, usize)>, depth: usize) {
        if !done.insert((object, region)) {
            return;
        }
        let Some(c) = self.fields.get(object) else { return };
        let mut at = 0;
        for k in 0..c.segments.len() {
            if at == region {
                out.push(Piece { object: object.clone(), seg: k });
                if let Some(target) = self.include.get(&(object, k)) {
                    self.expand(target, out, done, depth + 1);
                }
            }
            if matches!(c.anchors.get(k), Some(Anchor::Split(_))) {
                at += 1;
                if at > region {
                    break;
                }
            }
        }
    }
}

/// A view paragraph's runs from its pieces.
pub(crate) fn shown(pieces: &[Piece], fields: &HashMap<ObjectId, Composition>) -> Vec<TextRun> {
    let mut out = Vec::new();
    for piece in pieces {
        if let Some(c) = fields.get(&piece.object) {
            out.extend(c.segment(piece.seg).iter().cloned());
        }
    }
    canon(out)
}

/// Splits `new` (a view paragraph's new runs) into its pieces, whose old lengths are `lens` and
/// whose old text is `old`: kept characters stay in their piece, inserted ones join the piece of
/// the character before them (the first piece at the start). A piece in `gone` (its field is
/// being deleted) gives its characters to the piece before it. Answers each piece's new runs.
pub(crate) fn assign(old: &str, lens: &[usize], gone: &[bool], new: &[TextRun]) -> Vec<Vec<TextRun>> {
    let new_text = text_of(new);
    // The piece of every old character.
    let mut owner_old: Vec<usize> = Vec::with_capacity(old.chars().count());
    for (k, &len) in lens.iter().enumerate() {
        // A gone piece's characters belong to the closest piece before it that stays.
        let owner = (0..=k).rev().find(|&i| !gone.get(i).copied().unwrap_or(false)).unwrap_or(0);
        owner_old.extend(std::iter::repeat_n(owner, len));
    }
    let mut owner_new: Vec<usize> = Vec::with_capacity(new_text.chars().count());
    let mut at = 0;
    for edit in diff_text(old, &new_text) {
        owner_new.extend(owner_old.get(at..edit.index).unwrap_or(&[]).iter().copied());
        let last = owner_new.last().copied().unwrap_or(0);
        owner_new.extend(std::iter::repeat_n(last, edit.insert.chars().count()));
        at = edit.index + edit.delete;
    }
    owner_new.extend(owner_old.get(at..).unwrap_or(&[]).iter().copied());
    let mut out = vec![Vec::new(); lens.len().max(1)];
    let mut start = 0;
    for (k, slot) in out.iter_mut().enumerate() {
        let count = owner_new.iter().filter(|&&o| o == k).count();
        *slot = slice(new, start, start + count);
        start += count;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> TextRun {
        TextRun { insert: s.into(), attributes: Default::default() }
    }

    fn split(target: &ObjectId) -> TextRun {
        Anchor::Split(target.clone()).run()
    }

    fn merge(target: &ObjectId) -> TextRun {
        Anchor::Merge(target.clone()).run()
    }

    #[test]
    fn a_field_composes_at_its_anchors_and_back() {
        let (n, m) = (ObjectId::from_string("n"), ObjectId::from_string("m"));
        let runs = canon(vec![run("Hello "), split(&n), run("!"), merge(&m), run("?")]);
        let c = compose(&runs);
        assert_eq!(c.segments.len(), 3);
        assert_eq!(text_of(c.segment(0)), "Hello ");
        assert_eq!(c.anchors, [Anchor::Split(n), Anchor::Merge(m)]);
        assert_eq!(text_of(c.segment(1)), "!");
        assert_eq!(text_of(c.segment(2)), "?");
        assert_eq!(c.runs(), runs);
        assert_eq!(c.start_of(1), 7);
        assert_eq!(c.start_of(2), 9);
        let mut d = c.clone();
        d.insert_anchor(0, Anchor::Merge(ObjectId::from_string("x")));
        assert_eq!(d.segments.len(), 4);
        d.remove_anchor(0);
        assert_eq!(d, c);
    }

    #[test]
    fn a_tail_shows_at_the_end_of_its_target() {
        let (o, n) = (ObjectId::from_string("o"), ObjectId::from_string("n"));
        let mut fields = HashMap::new();
        fields.insert(o.clone(), compose(&[run("Hello "), split(&n), run("!")]));
        fields.insert(n.clone(), compose(&[run("world")]));
        let l = layout(&[o.clone(), n.clone()], &fields);
        assert_eq!(l.roots, [o.clone(), n.clone()]);
        assert!(!l.pieces.contains_key(&o));
        assert_eq!(text_of(&shown(&l.pieces[&n], &fields)), "world!");
        // Its target gone, the tail stays where it is.
        let l = layout(std::slice::from_ref(&o), &fields);
        assert_eq!(text_of(&shown(&l.pieces[&o], &fields)), "Hello !");
    }

    #[test]
    fn an_absorbed_paragraph_shows_at_its_merge_anchor() {
        let (a, b, c) = (ObjectId::from_string("a"), ObjectId::from_string("b"), ObjectId::from_string("c"));
        let mut fields = HashMap::new();
        fields.insert(a.clone(), compose(&[run("Hello "), merge(&b), run("!")]));
        fields.insert(b.clone(), compose(&[run("world"), split(&c), run(" and more")]));
        fields.insert(c.clone(), compose(&[run("Next")]));
        let l = layout(&[a.clone(), b.clone(), c.clone()], &fields);
        assert_eq!(l.roots, [a.clone(), c.clone()]);
        assert_eq!(l.absorbed, [(b.clone(), a.clone())]);
        assert_eq!(text_of(&shown(&l.pieces[&a], &fields)), "Hello world!");
        assert_eq!(text_of(&shown(&l.pieces[&c], &fields)), "Next and more");
        // Not listed: the anchor shows nothing, and the tail stays with its paragraph.
        let l = layout(&[a.clone(), c.clone()], &fields);
        assert_eq!(text_of(&shown(&l.pieces[&a], &fields)), "Hello !");
        // Two anchors naming one paragraph show it once; a cycle shows its first paragraph.
        fields.insert(c.clone(), compose(&[run("Next"), merge(&b)]));
        let l = layout(&[a.clone(), b.clone(), c.clone()], &fields);
        assert_eq!(text_of(&shown(&l.pieces[&a], &fields)), "Hello world!");
        assert_eq!(text_of(&shown(&l.pieces[&c], &fields)), "Next and more");
        fields.insert(b.clone(), compose(&[run("world"), merge(&a)]));
        let l = layout(&[a.clone(), b.clone()], &fields);
        assert_eq!(l.roots, std::slice::from_ref(&a));
        assert_eq!(text_of(&shown(&l.pieces[&a], &fields)), "Hello world!");
    }

    #[test]
    fn an_edit_goes_to_the_piece_it_touches() {
        let new = [run("world, and more!")];
        // "world" (own) + "!" (a tail): ", and more" typed before the "!" joins "world".
        let parts = assign("world!", &[5, 1], &[false, false], &new);
        assert_eq!(text_of(&parts[0]), "world, and more");
        assert_eq!(text_of(&parts[1]), "!");
        // Typed after the "!": the tail's.
        let parts = assign("world!", &[5, 1], &[false, false], &[run("world!?")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("world", "!?"));
        // A gone piece's characters go to the piece before it.
        let parts = assign("world!", &[5, 1], &[false, true], &[run("world!")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("world!", ""));
        // Typing at the start of an empty first piece stays in it.
        let parts = assign("!", &[0, 1], &[false, false], &[run("x!")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("x", "!"));
    }
}
