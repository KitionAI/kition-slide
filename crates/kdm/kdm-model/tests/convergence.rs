//! Seeded random multi-session convergence (S7): ~200 random model-level edits on two or three
//! sessions with random exchanges, among them edits inside a text body (typing, deleting, bold and
//! italic ranges, paragraph splits and merges, in shapes, table cells and notes: plan K7); after a final exchange every replica has the same state hash
//! and the same (id-blind) view, and no view holds a dangling reference.

mod common;

use std::sync::Arc;

use common::{Session, SharedBlobs, assert_no_dangling, assert_replica_clean, canonical, edit, first, peer, slide_file, sync_all};
use kition_slide_model::style::PictureFill;
use kition_slide_model::text::{Run, RunProps, TextBody};
use kition_slide_model::{Animation, Shape, ShapeId, ShapeKind, SlideFile, Table, Xfrm, defaults};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

fn new_shape(p: &mut SlideFile, rng: &mut Rng) -> Shape {
    let id = ShapeId(p.alloc_id());
    Shape {
        id,
        name: format!("Shape {id}"),
        xfrm: Some(Xfrm::new(rng.below(800) as f64, rng.below(400) as f64, 80.0, 40.0)),
        text: Some(TextBody::from_text(&format!("text {}", rng.below(1000)))),
        ..Default::default()
    }
}

fn top_level_ids(slide: &kition_slide_model::Slide) -> Vec<ShapeId> {
    slide.shapes.iter().map(|s| s.id).collect()
}

/// Removes animations, triggers, connector ends and comments naming shapes `slide` no longer holds,
/// as the engine's own commands do.
fn tidy(slide: &mut kition_slide_model::Slide) {
    let mut live = std::collections::HashSet::new();
    kition_slide_model::walk(&slide.shapes, &mut |s, _| {
        live.insert(s.id);
    });
    slide.animations.retain(|a| live.contains(&a.shape));
    for a in &mut slide.animations {
        if a.trigger.is_some_and(|t| !live.contains(&t)) {
            a.trigger = None;
        }
    }
    fn fix(shapes: &mut [Shape], live: &std::collections::HashSet<ShapeId>) {
        for s in shapes {
            match &mut s.kind {
                ShapeKind::Connector { start, end } => {
                    for side in [start, end] {
                        if side.is_some_and(|(id, _)| !live.contains(&id)) {
                            *side = None;
                        }
                    }
                }
                ShapeKind::Group { children, .. } => fix(children, live),
                _ => {}
            }
        }
    }
    fix(&mut slide.shapes, &live);
}

/// One edit inside `body`: typing, deleting, a bold or italic range, a split or a merge.
fn edit_text(body: &mut TextBody, rng: &mut Rng) {
    if body.paragraphs.is_empty() {
        body.paragraphs.push(Default::default());
    }
    let pi = rng.below(body.paragraphs.len());
    let words = ["a", "bc", " Hello ", "world", "!"];
    let word = words[rng.below(words.len())];
    let op = rng.below(5);
    if op == 4 && body.paragraphs.len() > 1 && pi + 1 < body.paragraphs.len() {
        // Merge with the next paragraph (Backspace at its start).
        let next = body.paragraphs.remove(pi + 1);
        body.paragraphs[pi].runs.extend(next.runs);
        body.paragraphs[pi].normalize();
        return;
    }
    // The paragraph as characters with their properties.
    let p = &mut body.paragraphs[pi];
    let mut chars: Vec<(char, RunProps)> = p.runs.iter().flat_map(|r| r.text.chars().map(|c| (c, r.props.clone()))).collect();
    let at = rng.below(chars.len() + 1);
    match op {
        0 | 1 => {
            let props = chars.get(at.saturating_sub(1)).map(|c| c.1.clone()).unwrap_or_default();
            for (k, c) in word.chars().enumerate() {
                chars.insert(at + k, (c, props.clone()));
            }
        }
        2 => {
            let end = (at + 1 + rng.below(4)).min(chars.len());
            chars.drain(at.min(end)..end);
        }
        3 => {
            let end = (at + 1 + rng.below(6)).min(chars.len());
            let bold = rng.chance(50);
            for c in &mut chars[at.min(end)..end] {
                if bold {
                    c.1.bold = Some(true);
                } else {
                    c.1.italic = Some(true);
                }
            }
        }
        _ => {
            // Split here (Enter).
            let tail: Vec<(char, RunProps)> = chars.split_off(at);
            let mut second = p.clone();
            second.runs = tail.into_iter().map(|(c, props)| Run::with(c.to_string(), props)).collect();
            second.normalize();
            p.runs = chars.into_iter().map(|(c, props)| Run::with(c.to_string(), props)).collect();
            p.normalize();
            body.paragraphs.insert(pi + 1, second);
            return;
        }
    }
    p.runs = chars.into_iter().map(|(c, props)| Run::with(c.to_string(), props)).collect();
    p.normalize();
}

fn random_edit(p: &mut SlideFile, rng: &mut Rng) {
    let slide_count = p.slides.len();
    match rng.below(22) {
        0 => {
            let layouts: Vec<_> = p.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.id)).collect();
            let layout = layouts[rng.below(layouts.len())];
            let slide = defaults::new_slide(p, layout);
            let at = rng.below(slide_count + 1);
            p.slides.insert(at, Arc::new(slide));
        }
        1 if slide_count > 1 => {
            let gone = p.slides.remove(rng.below(slide_count)).id;
            for section in &mut p.sections {
                section.slides.retain(|s| *s != gone);
            }
        }
        2 if slide_count > 1 => {
            let moved = p.slides.remove(rng.below(slide_count));
            let to = rng.below(slide_count);
            p.slides.insert(to, moved);
        }
        _ if slide_count == 0 => {}
        3 | 4 => {
            let i = rng.below(slide_count);
            let shape = new_shape(p, rng);
            let s = Arc::make_mut(&mut p.slides[i]);
            let at = rng.below(s.shapes.len() + 1);
            s.shapes.insert(at, shape);
        }
        5 => {
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            if !s.shapes.is_empty() {
                s.shapes.remove(rng.below(s.shapes.len()));
                tidy(s);
            }
        }
        6 => {
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            if s.shapes.len() > 1 {
                let moved = s.shapes.remove(rng.below(s.shapes.len()));
                let to = rng.below(s.shapes.len() + 1);
                s.shapes.insert(to, moved);
            }
        }
        7 => {
            // Group two top-level shapes.
            let i = rng.below(slide_count);
            let id = ShapeId(p.alloc_id());
            let s = Arc::make_mut(&mut p.slides[i]);
            if s.shapes.len() >= 2 {
                let first = rng.below(s.shapes.len() - 1);
                let children: Vec<Shape> = s.shapes.drain(first..first + 2).collect();
                let group = Shape {
                    id,
                    name: format!("Group {id}"),
                    xfrm: Some(Xfrm::new(0.0, 0.0, 100.0, 100.0)),
                    kind: ShapeKind::Group { children, child: Xfrm::new(0.0, 0.0, 100.0, 100.0) },
                    ..Default::default()
                };
                s.shapes.insert(first, group);
            }
        }
        8 => {
            // Ungroup the first group.
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            if let Some(at) = s.shapes.iter().position(Shape::is_group) {
                let group = s.shapes.remove(at);
                if let ShapeKind::Group { children, .. } = group.kind {
                    for (k, child) in children.into_iter().enumerate() {
                        s.shapes.insert(at + k, child);
                    }
                }
                tidy(s);
            }
        }
        9 | 10 => {
            // Edit some shape's text, at any depth.
            let word = format!("edit {}", rng.below(10_000));
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            let mut all = Vec::new();
            kition_slide_model::walk(&s.shapes, &mut |sh, _| all.push(sh.id));
            if !all.is_empty() {
                let id = all[rng.below(all.len())];
                if let Some(sh) = s.shape_mut(id)
                    && !matches!(sh.kind, ShapeKind::Group { .. } | ShapeKind::Table(_))
                {
                    sh.text = Some(TextBody::from_text(&word));
                }
            }
        }
        11 => {
            // Insert a table, or change one: row/col insert/delete, cell edit.
            let i = rng.below(slide_count);
            let id = ShapeId(p.alloc_id());
            let op = rng.below(5);
            let (r, c, word) = (rng.below(5), rng.below(5), format!("cell {}", rng.below(1000)));
            let s = Arc::make_mut(&mut p.slides[i]);
            match s.shapes.iter_mut().find_map(|sh| if let ShapeKind::Table(t) = &mut sh.kind { Some(t) } else { None }) {
                None => s.shapes.push(Shape {
                    id,
                    name: format!("Table {id}"),
                    xfrm: Some(Xfrm::new(10.0, 10.0, 300.0, 90.0)),
                    kind: ShapeKind::Table(Table::new(3, 3, 300.0, 30.0)),
                    ..Default::default()
                }),
                Some(t) => match op {
                    0 => t.insert_row(r.min(t.n_rows())),
                    1 => t.insert_col(c.min(t.n_cols())),
                    2 => {
                        t.delete_row(r % t.n_rows().max(1));
                    }
                    3 => {
                        t.delete_col(c % t.n_cols().max(1));
                    }
                    _ => {
                        let (rows, cols) = (t.n_rows(), t.n_cols());
                        if let Some(cell) = t.cell_mut(r % rows.max(1), c % cols.max(1)) {
                            cell.text = TextBody::from_text(&word);
                        }
                    }
                },
            }
        }
        12 => {
            // Apply a layout.
            let layouts: Vec<_> = p.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.id)).collect();
            let layout = layouts[rng.below(layouts.len())];
            Arc::make_mut(&mut p.slides[rng.below(slide_count)]).layout = layout;
        }
        13 => {
            // Change the theme.
            let master = Arc::make_mut(&mut p.masters[0]);
            master.theme.name = format!("Theme {}", rng.below(100));
        }
        14 => {
            // Add media and a picture of it.
            let bytes: Vec<u8> = (0..16).map(|_| rng.below(256) as u8).collect();
            let media = p.add_media("image.png", "image/png", bytes);
            let mut picture = new_shape(p, rng);
            picture.text = None;
            picture.kind = ShapeKind::Picture { fill: PictureFill { media, ..Default::default() } };
            let i = rng.below(slide_count);
            Arc::make_mut(&mut p.slides[i]).shapes.push(picture);
        }
        15 => {
            // Animate a shape, sometimes triggered by another.
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            let ids = top_level_ids(s);
            if !ids.is_empty() {
                let shape = ids[rng.below(ids.len())];
                let trigger = if rng.chance(30) { Some(ids[rng.below(ids.len())]) } else { None };
                let at = rng.below(s.animations.len() + 1);
                s.animations.insert(at, Animation { shape, trigger, duration_ms: 100 * (1 + rng.below(9) as u32), ..Default::default() });
            }
        }
        16 => {
            // Connect two shapes.
            let i = rng.below(slide_count);
            let id = ShapeId(p.alloc_id());
            let s = Arc::make_mut(&mut p.slides[i]);
            let ids = top_level_ids(s);
            if ids.len() >= 2 {
                let (from, to) = (ids[rng.below(ids.len())], ids[rng.below(ids.len())]);
                s.shapes.push(Shape {
                    id,
                    name: format!("Connector {id}"),
                    xfrm: Some(Xfrm::new(0.0, 0.0, 10.0, 10.0)),
                    kind: ShapeKind::Connector { start: Some((from, 0)), end: Some((to, 2)) },
                    ..Default::default()
                });
            }
        }
        18..=20 => {
            // Inside a text body: some shape's (at any depth), a table cell's, or the notes'.
            let s = Arc::make_mut(&mut p.slides[rng.below(slide_count)]);
            match rng.below(4) {
                0 => edit_text(&mut s.notes, rng),
                1 => {
                    let cell = s.shapes.iter_mut().find_map(|sh| if let ShapeKind::Table(t) = &mut sh.kind { Some(t) } else { None }).and_then(|t| {
                        let (rows, cols) = (t.n_rows(), t.n_cols());
                        let (r, c) = (rng.below(rows), rng.below(cols));
                        t.cell_mut(r, c)
                    });
                    if let Some(cell) = cell {
                        edit_text(&mut cell.text, rng);
                    }
                }
                _ => {
                    let mut all = Vec::new();
                    kition_slide_model::walk(&s.shapes, &mut |sh, _| {
                        if sh.text.is_some() {
                            all.push(sh.id);
                        }
                    });
                    if !all.is_empty() {
                        let id = all[rng.below(all.len())];
                        if let Some(text) = s.shape_mut(id).and_then(|sh| sh.text.as_mut()) {
                            edit_text(text, rng);
                        }
                    }
                }
            }
        }
        _ => {
            // Sections over the current slides.
            let ids: Vec<_> = p.slides.iter().map(|s| s.id).collect();
            let split = rng.below(ids.len() + 1);
            p.sections = vec![
                kition_slide_model::Section { identity: kition_slide_model::identity::fresh(), name: "One".into(), slides: ids[..split].to_vec() },
                kition_slide_model::Section { identity: kition_slide_model::identity::fresh(), name: "Two".into(), slides: ids[split..].to_vec() },
            ];
        }
    }
}

/// The view is what a fresh read of the replica gives: the view patched from what an import or
/// undo touched (`src/patch.rs`) never drifts from a full read.
fn assert_view_fresh(session: &Session, blobs: &SharedBlobs, what: &str) {
    let (fresh, _) = kdm_model::read_document(session.replica(), blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&session.model().slide_file), canonical(&fresh), "{what}: the patched view matches a full read");
}

fn run(seed: u64, sessions: usize, steps: usize) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let blobs = SharedBlobs::default();
    let a = first(&slide_file(), &blobs);
    let mut all: Vec<Session> = vec![a];
    for k in 1..sessions {
        let next = peer(&all[0], 10 + k as u64, &blobs);
        all.push(next);
    }
    for _ in 0..steps {
        let who = rng.below(all.len());
        let sub = rng.next();
        edit(&mut all[who], &blobs, |p| random_edit(p, &mut Rng(sub | 1)));
        if rng.chance(25) {
            let (from, to) = (rng.below(all.len()), rng.below(all.len()));
            if from != to {
                let bytes = all[from].export_changes(&all[to].version()).unwrap_or_else(|error| panic!("export: {error}"));
                all[to].import_changes(&bytes).unwrap_or_else(|error| panic!("seed {seed}: import: {error}"));
                assert_view_fresh(&all[to], &blobs, &format!("seed {seed}: import"));
            }
        }
        if rng.chance(3) {
            let who = rng.below(all.len());
            all[who].undo().unwrap_or_else(|error| panic!("seed {seed}: undo: {error}"));
            assert_view_fresh(&all[who], &blobs, &format!("seed {seed}: undo"));
        }
    }
    sync_all(&mut all);
    let hash = all[0].state_hash().unwrap_or_else(|error| panic!("hash: {error}"));
    let view = canonical(&all[0].model().slide_file);
    for (k, session) in all.iter().enumerate() {
        assert_eq!(session.state_hash().unwrap_or_else(|error| panic!("hash: {error}")), hash, "seed {seed}: session {k} state hash");
        assert_eq!(canonical(&session.model().slide_file), view, "seed {seed}: session {k} view");
        assert_no_dangling(&session.model().slide_file);
        assert_replica_clean(session.replica());
        // The view is what a fresh read of the replica gives.
        let (fresh, _) = kdm_model::read_document(session.replica(), &blobs).unwrap_or_else(|error| panic!("read: {error}"));
        assert_eq!(canonical(&fresh), view, "seed {seed}: session {k} view matches its replica");
    }
}

#[test]
fn two_sessions_converge() {
    for seed in [1, 2, 3] {
        run(seed, 2, 200);
    }
}

#[test]
fn three_sessions_converge() {
    for seed in [11, 12] {
        run(seed, 3, 200);
    }
}
