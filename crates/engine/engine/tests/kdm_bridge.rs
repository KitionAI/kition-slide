//! The command bridge to each slide's KDM replica (Kition phase 3d, Task 2): every
//! mutating command (or pointer gesture) replicates as one transaction that reproduces the slide in
//! a peer, undo after a remote edit keeps the remote edit, a drag is one undo step, selection-only
//! changes write nothing, the history limit never loses an edit, a remote deletion of the selected
//! slide moves the selection on, and two sessions running random commands converge with the
//! engine's slide equal to its own replica after every step.

use kition_slide_engine::model::{ShapeKind, SlideFile};
use kition_slide_engine::{MemoryBlobs, Mods, Origin, PointerEvent, PointerKind, Session, Version};
use serde_json::{Value, json};

/// A session with a three-slide slide file: a title slide, a slide with two shapes, a slide with a table.
fn new_session() -> Session {
    let mut s = Session::with_new();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 80], "text": "A"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [300, 100, 100, 80], "text": "B"})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("insert.table", &json!({"rows": 2, "cols": 2, "rect": [100.0, 100.0, 300.0, 80.0]})).unwrap();
    s.execute("slide.go", &json!({"index": 1})).unwrap();
    s
}

/// A second session holding a replica of `a`'s active slide.
fn peer(a: &mut Session) -> Session {
    let mut b = Session::new();
    b.prefs.snap_to_grid = false;
    b.prefs.smart_guides = false;
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    b
}

/// Sends `from`'s changes (and the blobs they need) that `to` lacks.
fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap();
    let blobs = from.blobs_since(&since).unwrap();
    to.add_blobs(&blobs).unwrap();
    let bytes = from.export_changes(&since).unwrap();
    to.import_changes(&bytes).unwrap();
}

fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version().unwrap() == b.version().unwrap() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn doc(s: &Session) -> &SlideFile {
    &s.active().unwrap().doc
}

fn canonical(p: &SlideFile) -> Value {
    serde_json::to_value(kdm_model::normalize(p)).unwrap()
}

/// `a` and `b` hold the same replicated state and the same slide (ids aside).
fn assert_same(a: &mut Session, b: &mut Session, what: &str) {
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "{what}: replicas differ");
    assert!(canonical(doc(a)) == canonical(doc(b)), "{what}: slides differ:\n{:#?}", differences(&canonical(doc(a)), &canonical(doc(b))));
}

/// The slide as read back fresh from the session's replica, without flushing anything.
fn replica_doc(s: &Session) -> SlideFile {
    let kdm = s.kdm().expect("an edit made the replica");
    kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0
}

/// The engine's slide is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let (mine, held) = (canonical(doc(s)), canonical(&replica_doc(s)));
    assert!(mine == held, "{what}: the slide is not what its replica holds:\n{:#?}", differences(&mine, &held));
}

/// The replica's own undo steps (one per undoable transaction).
fn replica_steps(s: &Session) -> usize {
    s.kdm().map_or(0, |k| k.steps().0)
}

fn last_steps(s: &Session) -> u32 {
    s.active().unwrap().history.undo.last().unwrap().steps
}

fn ev(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
    PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 }
}

fn shape_ids(s: &Session) -> Vec<u64> {
    s.active().unwrap().shapes().iter().map(|x| x.id.0 as u64).collect()
}

fn png() -> String {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    kition_slide_engine::cmd::base64_encode(&bytes)
}

fn first_table(s: &Session) -> u64 {
    doc(s).slides[2].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Table(_))).unwrap().id.0 as u64
}

#[test]
fn one_command_one_transaction_reproduces_in_a_peer() {
    let cases: Vec<(&str, fn(&mut Session))> = vec![
        ("text typing", |s| {
            let id = shape_ids(s)[0];
            s.execute("text.edit", &json!({"id": id})).unwrap();
            s.type_text("x").unwrap();
        }),
        ("shape move (drag)", |s| {
            s.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
            s.pointer(ev(PointerKind::Drag, 160.0, 150.0)).unwrap();
            s.pointer(ev(PointerKind::Drag, 170.0, 160.0)).unwrap();
            s.pointer(ev(PointerKind::Up, 170.0, 160.0)).unwrap();
        }),
        ("table row insert", |s| {
            let id = first_table(s);
            s.execute("slide.go", &json!({"index": 2})).unwrap();
            s.execute("table.insertRowBelow", &json!({"id": id, "row": 0})).unwrap();
        }),
        ("slide reorder", |s| {
            s.execute("slide.move", &json!({"from": 2, "to": 0})).unwrap();
        }),
        ("layout apply", |s| {
            s.execute("slide.layout", &json!({"layout": "twoContent"})).unwrap();
        }),
        ("theme change", |s| {
            s.execute("design.theme", &json!({"name": "Ember"})).unwrap();
        }),
        ("paste with an image", |s| {
            s.execute("insert.picture", &json!({"data": png(), "name": "red.png", "rect": [10, 10, 40, 40]})).unwrap();
            s.execute("edit.copy", &json!({})).unwrap();
            s.execute("slide.go", &json!({"index": 0})).unwrap();
            s.execute("edit.paste", &json!({})).unwrap();
        }),
        ("group", |s| {
            s.execute("edit.select", &json!({"ids": shape_ids(s)})).unwrap();
            s.execute("arrange.group", &json!({})).unwrap();
        }),
        ("ungroup", |s| {
            s.execute("edit.select", &json!({"ids": shape_ids(s)})).unwrap();
            s.execute("arrange.group", &json!({})).unwrap();
            s.execute("arrange.ungroup", &json!({})).unwrap();
        }),
        ("fit_text_box", |s| {
            s.execute("insert.textBox", &json!({"rect": [50, 300, 120, 20], "text": "short"})).unwrap();
            s.type_text(" and then a much longer line of text that wraps over several lines").unwrap();
        }),
    ];
    for (name, run) in cases {
        let mut a = new_session();
        let mut b = peer(&mut a);
        let steps = replica_steps(&a);
        let entries = a.active().unwrap().history.undo.len();
        run(&mut a);
        let commands = a.active().unwrap().history.undo.len() - entries;
        assert!(commands >= 1, "{name}: an undo step");
        assert_eq!(last_steps(&a), 1, "{name}: exactly one transaction for the last step");
        assert_eq!(replica_steps(&a) - steps, commands, "{name}: one transaction per step, nothing else");
        assert_holds(&a, name);
        send(&mut a, &mut b);
        assert_same(&mut a, &mut b, name);
    }
}

#[test]
fn a_drag_is_one_undo_step() {
    let mut s = new_session();
    let before = canonical(doc(&s));
    let entries = s.active().unwrap().history.undo.len();
    s.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
    for i in 1..6 {
        s.pointer(ev(PointerKind::Drag, 150.0 + 10.0 * i as f64, 140.0)).unwrap();
    }
    s.pointer(ev(PointerKind::Up, 200.0, 140.0)).unwrap();
    assert_eq!(s.active().unwrap().history.undo.len(), entries + 1, "one history step");
    assert_eq!(last_steps(&s), 1, "one replica step");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(canonical(doc(&s)) == before, "one undo takes the whole drag back");
    assert_holds(&s, "after the undo");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.active().unwrap().shapes()[0].xfrm.unwrap().x, 150.0);
}

#[test]
fn a_remote_edit_in_the_middle_of_a_drag_keeps_both() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    a.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
    a.pointer(ev(PointerKind::Drag, 170.0, 140.0)).unwrap();
    b.execute("slide.rename", &json!({"name": "Remote", "index": 1})).unwrap();
    send(&mut b, &mut a);
    a.pointer(ev(PointerKind::Drag, 190.0, 140.0)).unwrap();
    a.pointer(ev(PointerKind::Up, 190.0, 140.0)).unwrap();
    assert_holds(&a, "after the drag");
    assert_eq!(a.active().unwrap().shapes()[0].xfrm.unwrap().x, 140.0);
    a.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(a.active().unwrap().shapes()[0].xfrm.unwrap().x, 100.0, "the whole drag undoes");
    assert_eq!(doc(&a).slides[1].name, "Remote", "the remote edit stays");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
}

#[test]
fn undo_back_to_the_saved_slide_file_is_clean() {
    let mut s = new_session();
    let st = s.active_mut().unwrap();
    st.saved_doc = st.doc.clone();
    s.execute("slide.rename", &json!({"name": "Dirty"})).unwrap();
    assert!(s.active().unwrap().is_dirty());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.active().unwrap().is_dirty(), "the replica's undo result is the saved slide");
}

#[test]
fn undo_after_a_remote_edit_keeps_it() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    a.execute("shape.move", &json!({"ids": [shape_ids(&a)[0]], "dx": 20, "dy": 0})).unwrap();
    b.execute("slide.go", &json!({"index": 2})).unwrap();
    b.execute("table.insertRowBelow", &json!({"id": first_table(&b), "row": 0})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
    assert!(a.execute("edit.undo", &json!({})).is_ok());
    assert_eq!(a.active().unwrap().shapes()[0].xfrm.unwrap().x, 100.0, "the local move is undone");
    let rows = |s: &Session| doc(s).slides[2].shapes.iter().find_map(|x| if let ShapeKind::Table(t) = &x.kind { Some(t.rows.len()) } else { None });
    assert_eq!(rows(&a), Some(3), "the remote row insert survives the local undo");
    assert_holds(&a, "after the undo");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the undo");
}

#[test]
fn selection_only_changes_write_nothing() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let version = a.version().unwrap();
    a.execute("edit.select", &json!({"ids": shape_ids(&a)})).unwrap();
    a.execute("slide.go", &json!({"index": 0})).unwrap();
    a.execute("edit.selectAll", &json!({})).unwrap();
    a.pointer(ev(PointerKind::Down, 700.0, 500.0)).unwrap();
    a.pointer(ev(PointerKind::Up, 700.0, 500.0)).unwrap();
    assert_eq!(a.version().unwrap(), version, "no transaction");
    assert_eq!(b.active().unwrap().selection.slide, 0, "the peer's own selection is untouched");
    send(&mut a, &mut b);
    assert_same(&mut a, &mut b, "nothing to send");
}

#[test]
fn every_edit_replicates_at_the_history_limit() {
    let mut s = new_session();
    s.active_mut().unwrap().history.limit = 3;
    for i in 0..8 {
        s.execute("slide.rename", &json!({"name": format!("N{i}")})).unwrap();
        assert_eq!(replica_doc(&s).slides[1].name, format!("N{i}"), "edit {i} replicated");
        assert_eq!(last_steps(&s), 1, "edit {i}: one replica step");
    }
    assert_eq!(s.kdm().unwrap().steps().0, 3, "the replica keeps only the steps the history can take");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(doc(&s).slides[1].name, "N6", "undo reverts the last edit only");
    assert_holds(&s, "after the undo");
    for _ in 0..5 {
        let _ = s.execute("edit.undo", &json!({}));
    }
    assert_eq!(doc(&s).slides[1].name, "N4", "undo stops where the history does");
    assert_holds(&s, "at the bottom of the history");
}

#[test]
fn a_remote_deletion_of_the_selected_slide_fixes_the_selection() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    a.execute("slide.go", &json!({"index": 2})).unwrap();
    a.execute("edit.select", &json!({"ids": [first_table(&a)]})).unwrap();
    b.execute("slide.delete", &json!({"index": 2})).unwrap();
    b.execute("slide.delete", &json!({"index": 1})).unwrap();
    send(&mut b, &mut a);
    let st = a.active().unwrap();
    assert_eq!(doc(&a).slides.len(), 1);
    assert_eq!(st.selection.slide, 0, "the selection moved to a slide that exists");
    assert!(st.selection.shapes.is_empty(), "the deleted table is no longer selected");
    // A slide before the current one deleted remotely: the selection follows the same slide.
    let mut a = new_session();
    let mut b = peer(&mut a);
    a.execute("slide.go", &json!({"index": 2})).unwrap();
    let current = doc(&a).slides[2].id;
    b.execute("slide.delete", &json!({"index": 0})).unwrap();
    send(&mut b, &mut a);
    assert_eq!(doc(&a).slides[a.active().unwrap().selection.slide].id, current, "still on the same slide");
    a.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 20, 20]})).unwrap();
    assert_holds(&a, "an edit after the import");
}

/// KitModel 1.1 M12a and engine contract 2 E14a: an agent's edits are the agent's undo steps. The
/// person's undo leaves them; the agent's (or `scope: agent`) takes them back.
#[test]
fn agent_edits_record_their_origin_and_undo_on_this_replica() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("slide.rename", &json!({"name": "Agent"}), &agent).unwrap();
    assert_eq!(s.origin(), kition_slide_engine::kdm_bridge::local_person(), "the origin is restored after the call");
    assert_eq!(doc(&s).slides[1].name, "Agent");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(doc(&s).slides[1].name, "Agent", "the person's undo keeps the agent's step");
    s.execute_as("edit.undo", &json!({}), &agent).unwrap();
    assert_ne!(doc(&s).slides[1].name, "Agent", "the agent's undo takes it back");
    s.execute_as("edit.redo", &json!({}), &agent).unwrap();
    assert_eq!(doc(&s).slides[1].name, "Agent", "and the agent's redo brings it back");
    s.execute("edit.undo", &json!({"scope": "agent"})).unwrap();
    assert_ne!(doc(&s).slides[1].name, "Agent", "`scope: agent` names the agent's steps");
    assert!(s.execute("edit.undo", &json!({"scope": "robot"})).is_err());
}

/// An agent's undo takes back its own last step though the person edited after it: the person's
/// later edit stays (the replica keeps it, as it keeps another replica's).
#[test]
fn an_agents_undo_keeps_the_persons_later_edit() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("slide.rename", &json!({"name": "By the agent"}), &agent).unwrap();
    s.execute("slide.go", &json!({"index": 0})).unwrap();
    s.execute("slide.rename", &json!({"name": "By the person"})).unwrap();
    s.execute_as("edit.undo", &json!({}), &agent).unwrap();
    let names: Vec<String> = doc(&s).slides.iter().map(|slide| slide.name.clone()).collect();
    assert!(!names.iter().any(|name| name == "By the agent"), "{names:?}");
    assert!(names.iter().any(|name| name == "By the person"), "the person's later edit stays: {names:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    let names: Vec<String> = doc(&s).slides.iter().map(|slide| slide.name.clone()).collect();
    assert!(!names.iter().any(|name| name == "By the person"), "the person's own undo takes theirs: {names:?}");
    assert_holds(&s, "the replica and the document agree after both undos");
}

/// A new step clears only its own scope's redo steps, as the replica does.
#[test]
fn a_new_agent_step_keeps_the_persons_redo() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute("slide.rename", &json!({"name": "Person"})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute_as("slide.go", &json!({"index": 0}), &agent).unwrap();
    s.execute_as("slide.rename", &json!({"name": "Agent"}), &agent).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    let names: Vec<String> = doc(&s).slides.iter().map(|slide| slide.name.clone()).collect();
    assert!(names.iter().any(|name| name == "Person") && names.iter().any(|name| name == "Agent"), "{names:?}");
}

#[test]
fn a_file_newer_than_its_stored_replica_wins_on_open() {
    let mut a = new_session();
    let snapshot = a.snapshot().unwrap();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    let doc_id = a.doc_id().unwrap();
    a.execute("slide.rename", &json!({"name": "Newer"})).unwrap();
    let file = a.execute("file.saveBytes", &json!({})).unwrap();
    let file = kition_slide_engine::cmd::base64_decode(file["data"].as_str().unwrap()).unwrap();

    let mut b = Session::new();
    b.open_with_replica(&file, &snapshot, blobs.clone()).unwrap();
    assert_eq!(doc(&b).slides[1].name, "Newer", "the file's content");
    assert!(canonical(doc(&b)) == canonical(doc(&a)), "the whole file");
    assert_holds(&b, "recorded onto the replica");
    assert_eq!(b.doc_id().unwrap(), doc_id, "the replica's own identity");
    assert!(!b.active().unwrap().is_dirty(), "opening is clean");
    assert!(b.active().unwrap().history.undo.is_empty(), "opening is not an undo step");

    let mut c = Session::new();
    c.open_with_replica(&[], &snapshot, blobs).unwrap();
    send(&mut b, &mut c);
    assert_same(&mut b, &mut c, "a peer of the older replica gets the file's content");
}

#[test]
fn passthroughs_answer_before_any_edit() {
    let mut s = Session::with_new();
    assert!(s.doc_id().is_none(), "no replica before anything asks for one");
    let version = s.version().unwrap();
    assert!(s.doc_id().is_some());
    assert_eq!(s.kdm_resets(), 0);
    assert!(s.export_changes(&Version::empty()).unwrap().len() > 1);
    assert_eq!(s.version().unwrap(), version, "asking writes nothing");
    let blobs: MemoryBlobs = s.blobs().unwrap();
    assert!(blobs.0.is_empty(), "a new slide has no media");
}

/// The text of slide 1's first shape.
fn first_text(s: &Session) -> String {
    doc(s).slides[1].shapes[0].text.as_ref().map(|t| t.text()).unwrap_or_default()
}

/// Two people typing into the same text box: one at its
/// end, one at its start; both texts survive, and each one's undo takes back only their own.
#[test]
fn two_people_typing_into_one_text_box_keep_both_texts() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    b.execute("slide.go", &json!({"index": 1})).unwrap();
    let (ia, ib) = (shape_ids(&a)[0], shape_ids(&b)[0]);
    a.execute("text.edit", &json!({"id": ia})).unwrap();
    a.type_text(" from a").unwrap();
    b.execute("text.edit", &json!({"id": ib})).unwrap();
    b.execute("text.move", &json!({"to": "start"})).unwrap();
    b.type_text("b: ").unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "typed");
    assert_eq!(first_text(&a), "b: A from a");
    assert_holds(&a, "a after the exchange");
    assert_holds(&b, "b after the exchange");
    a.execute("text.exit", &json!({})).unwrap();
    a.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(first_text(&a), "b: A", "a's undo keeps b's typing");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the undo");
}

/// Enter in the middle of a paragraph while the other person types into it, then bold on part of
/// it while the other italicizes an overlapping part.
#[test]
fn a_split_and_formatting_merge_with_concurrent_typing() {
    let mut a = new_session();
    let id = shape_ids(&a)[0];
    a.execute("text.set", &json!({"id": id, "text": "Hello world"})).unwrap();
    let mut b = peer(&mut a);
    b.execute("slide.go", &json!({"index": 1})).unwrap();
    let ib = shape_ids(&b)[0];
    a.execute("text.edit", &json!({"id": id, "at": [0, 6]})).unwrap();
    a.type_text("\n").unwrap();
    b.execute("text.edit", &json!({"id": ib})).unwrap();
    b.type_text("!").unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "split and typed");
    let body = doc(&a).slides[1].shapes[0].text.clone().unwrap();
    assert_eq!(body.paragraphs.len(), 2, "{:?}", body.text());
    assert_eq!(body.paragraphs[0].text(), "Hello ", "{:?}", body.text());
    assert_eq!(body.paragraphs[1].text(), "world!", "b's typing at the end follows the second half: {:?}", body.text());
    assert_holds(&a, "a");
    assert_holds(&b, "b");

    a.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 5]})).unwrap();
    a.execute("format.bold", &json!({"on": true})).unwrap();
    b.execute("text.select", &json!({"anchor": [0, 2], "caret": [0, 6]})).unwrap();
    b.execute("format.italic", &json!({"on": true})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "formatted");
    let body = doc(&a).slides[1].shapes[0].text.clone().unwrap();
    let runs = &body.paragraphs[0].runs;
    let bold: String = runs.iter().filter(|r| r.props.bold == Some(true)).map(|r| r.text.as_str()).collect();
    let italic: String = runs.iter().filter(|r| r.props.italic == Some(true)).map(|r| r.text.as_str()).collect();
    assert_eq!(bold, "Hello", "{runs:?}");
    assert_eq!(italic, "llo ", "{runs:?}");
    assert_holds(&a, "a");
}

/// Typing into the same table cell, and into the same slide's notes, keeps both texts.
#[test]
fn two_people_typing_into_one_cell_and_one_slides_notes_keep_both() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    for s in [&mut a, &mut b] {
        s.execute("slide.go", &json!({"index": 2})).unwrap();
    }
    let (ta, tb) = (first_table(&a), first_table(&b));
    a.execute("text.edit", &json!({"id": ta, "cell": [0, 0]})).unwrap();
    a.type_text("one").unwrap();
    b.execute("text.edit", &json!({"id": tb, "cell": [0, 0]})).unwrap();
    b.type_text("two").unwrap();
    a.execute("text.edit", &json!({"notes": true})).unwrap();
    a.type_text("notes a").unwrap();
    b.execute("text.edit", &json!({"notes": true})).unwrap();
    b.type_text("notes b").unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "typed");
    let cell =
        doc(&a).slides[2].shapes.iter().find_map(|x| if let ShapeKind::Table(t) = &x.kind { Some(t.rows[0].cells[0].text.text()) } else { None });
    let cell = cell.unwrap();
    assert!(cell == "onetwo" || cell == "twoone", "{cell}");
    let notes = doc(&a).slides[2].notes.text();
    assert!(notes.contains("notes a") && notes.contains("notes b"), "{notes}");
    assert_holds(&a, "a");
    assert_holds(&b, "b");
}

/// A tiny deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// One random command: typing, drags, table rows, slide moves, layouts, themes, paste with an
/// image, group/ungroup, text boxes that fit their text, deletions, undo and redo.
fn random_command(s: &mut Session, rng: &mut Rng, picture: &str) -> String {
    let slides = doc(s).slides.len() as u64;
    let ids = shape_ids(s);
    let pick = |rng: &mut Rng| ids.get(rng.below(ids.len() as u64) as usize).copied();
    let themes = kition_slide_engine::model::theme::builtin_themes();
    let (id, params): (&str, Value) = match rng.below(20) {
        0 => ("slide.go", json!({"index": rng.below(slides)})),
        1 => match pick(rng) {
            Some(id) => {
                let _ = s.execute("text.edit", &json!({"id": id}));
                // In-text edits (K7): at the end, at the start, a new paragraph, or bold over the
                // first characters.
                let r = match rng.below(4) {
                    0 => {
                        let _ = s.execute("text.move", &json!({"to": "start"}));
                        s.type_text("<")
                    }
                    1 => {
                        let _ = s.execute("text.move", &json!({"to": "left"}));
                        s.type_text("\n")
                    }
                    2 => {
                        let _ = s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 1]}));
                        s.execute("format.bold", &json!({"on": true}))
                    }
                    _ => s.type_text(["a", "bc ", "Hello world. "][rng.below(3) as usize]),
                };
                return format!("type into {id} -> {}", r.is_ok());
            }
            None => ("slide.new", json!({"layout": "blank"})),
        },
        2 => {
            let Some(x) = s.active().unwrap().shapes().first().and_then(|x| x.xfrm) else { return "no shape to drag".into() };
            let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
            let (dx, dy) = (rng.below(60) as f64 - 30.0, rng.below(60) as f64 - 30.0);
            s.pointer(ev(PointerKind::Down, cx, cy)).unwrap();
            s.pointer(ev(PointerKind::Drag, cx + dx / 2.0, cy + dy / 2.0)).unwrap();
            s.pointer(ev(PointerKind::Drag, cx + dx, cy + dy)).unwrap();
            s.pointer(ev(PointerKind::Up, cx + dx, cy + dy)).unwrap();
            return format!("drag by {dx},{dy}");
        }
        3 => {
            let table = s.active().unwrap().shapes().iter().find(|x| matches!(x.kind, ShapeKind::Table(_))).map(|x| x.id.0);
            match table {
                Some(id) => ("table.insertRowBelow", json!({"id": id, "row": 0})),
                None => ("insert.table", json!({"rows": 2, "cols": 2, "rect": [50.0, 300.0, 200.0, 60.0]})),
            }
        }
        4 => ("slide.move", json!({"from": rng.below(slides), "to": rng.below(slides)})),
        5 => {
            let layout = ["twoContent", "blank", "titleAndContent"][rng.below(3) as usize];
            ("slide.layout", json!({ "layout": layout }))
        }
        6 => {
            let name = themes[rng.below(themes.len() as u64) as usize].name.clone();
            ("design.theme", json!({ "name": name }))
        }
        7 => {
            let _ = s.execute("insert.picture", &json!({"data": picture, "name": "p.png", "rect": [rng.below(400), 20, 30, 30]}));
            let _ = s.execute("edit.copy", &json!({}));
            let _ = s.execute("slide.go", &json!({"index": rng.below(slides)}));
            ("edit.paste", json!({}))
        }
        8 => {
            let _ = s.execute("edit.select", &json!({"ids": ids}));
            ("arrange.group", json!({}))
        }
        9 => match pick(rng) {
            Some(id) => {
                let _ = s.execute("edit.select", &json!({"ids": [id]}));
                ("arrange.ungroup", json!({}))
            }
            None => ("edit.undo", json!({})),
        },
        10 => ("insert.textBox", json!({"rect": [rng.below(500), rng.below(300), 80, 20], "text": "Fit me to a few more words"})),
        11 => ("shape.insert", json!({"preset": "rect", "rect": [rng.below(500), rng.below(300), 60, 40], "text": "S"})),
        12 => match pick(rng) {
            Some(id) => ("edit.delete", json!({"ids": [id]})),
            None => ("slide.new", json!({})),
        },
        13 if slides > 1 => ("slide.delete", json!({"index": rng.below(slides)})),
        13 => ("slide.new", json!({})),
        14 => ("slide.rename", json!({"name": format!("S{}", rng.below(100))})),
        15 | 16 => ("edit.redo", json!({})),
        _ => ("edit.undo", json!({})),
    };
    let r = s.execute(id, &params);
    format!("{id} {params} -> {}", if r.is_ok() { "ok" } else { "refused" })
}

/// Two sessions run ~250 random commands between them with a history limit small enough to wrap,
/// exchanging changes every few commands. After every command the slide is exactly what its own
/// replica holds; after every exchange both hold the same replicated state and the same slide.
#[test]
fn random_commands_keep_slide_file_and_replica_equal_and_converge() {
    let picture = png();
    for seed in [3u64, 11, 2026] {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut a = new_session();
        a.active_mut().unwrap().history.limit = 4;
        let mut b = peer(&mut a);
        b.active_mut().unwrap().history.limit = 4;
        let mut log: Vec<String> = Vec::new();
        for step in 0..63 {
            for (name, s) in [("a", &mut a), ("b", &mut b)] {
                let what = random_command(s, &mut rng, &picture);
                log.push(format!("{name}: {what}"));
                let context = || format!("seed {seed}, step {step}, {name}; last commands:\n{}", log[log.len().saturating_sub(6)..].join("\n"));
                assert_eq!(s.kdm_resets(), 0, "{}", context());
                let (mine, held) = (canonical(doc(s)), canonical(&replica_doc(s)));
                assert!(mine == held, "the slide is not what its replica holds: {}\n{:#?}", context(), differences(&mine, &held));
                let st = s.active().unwrap();
                assert!(st.selection.slide < st.doc.slides.len().max(1), "selection out of range: {}", context());
            }
            if step % 4 == 3 {
                sync(&mut a, &mut b);
                assert_same(&mut a, &mut b, &format!("seed {seed}, step {step}"));
            }
        }
        sync(&mut a, &mut b);
        assert_same(&mut a, &mut b, &format!("seed {seed}, final"));
        let mut c = peer(&mut a);
        assert_same(&mut a, &mut c, &format!("seed {seed}: a fresh peer matches"));
    }
}

/// The first few paths at which two JSON values differ (for a readable failure).
fn differences(a: &Value, b: &Value) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if out.len() >= 5 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), format!("{path}.{key}"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, format!("{path}[{i}]"), out);
                }
            }
            _ => out.push(format!("{path}: {} vs {}", short(a), short(b))),
        }
    }
    fn short(v: &Value) -> String {
        let s = v.to_string();
        if s.len() > 300 { format!("{}…", s.chars().take(300).collect::<String>()) } else { s }
    }
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out);
    out
}

// --- Comment threads (Kition Kit Standard 2.0 rule M23) ------------------------------------------

/// The objects (with their types) that importing `bytes` into a replica of `base` changes: the
/// containers the change's operations write.
fn touched_by(base: &[u8], bytes: &[u8]) -> Vec<(String, String)> {
    use kdm_model::kition_kit::kdm::{ActorId, Replica};
    use kdm_model::kition_kit::session::KitModel;
    let schema = <kdm_model::SlideModel as KitModel>::schema();
    let mut replica = Replica::load(schema.clone(), base, ActorId::from_u64(99).unwrap()).unwrap();
    let before = Replica::load(schema, base, ActorId::from_u64(98).unwrap()).unwrap();
    let min = replica.min_schema_version();
    let changed = replica.import_changes(bytes).unwrap();
    assert_eq!(replica.min_schema_version(), min, "the change raises no schema version");
    changed
        .into_iter()
        .map(|object| {
            (replica.object_type(&object).or_else(|_| before.object_type(&object)).unwrap_or_else(|_| "?".into()), object.as_str().to_owned())
        })
        .collect()
}

fn threads(s: &mut Session) -> Vec<Value> {
    s.execute("comment.threads", &json!({})).unwrap().as_array().cloned().unwrap()
}

#[test]
fn comment_threads_open_reply_resolve_delete_and_replicate() {
    let mut a = new_session();
    let shape = shape_ids(&a)[0];
    a.execute("edit.select", &json!({"ids": [shape]})).unwrap();
    let opened =
        a.execute("comment.add", &json!({"text": "Is this right?", "author": "user:ann", "name": "Ann", "created": "2026-10-19T10:00:00Z"})).unwrap();
    let thread = opened["thread"].as_str().unwrap().to_owned();
    assert_holds(&a, "after comment.add");
    let mut b = peer(&mut a);
    let replied = b.execute("comment.reply", &json!({"thread": thread, "text": "Yes.", "author": "user:bob", "name": "Bob"})).unwrap();
    let reply = replied["comment"].as_str().unwrap().to_owned();
    a.execute("comment.resolve", &json!({"thread": thread, "resolved": true})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after a reply and a resolve");
    let listed = threads(&mut a);
    assert_eq!(listed.len(), 1);
    let t = &listed[0];
    assert_eq!(t["id"], json!(thread));
    assert_eq!(t["resolved"], json!(true));
    assert_eq!(t["slide"], json!(1));
    assert_eq!(t["anchor"]["shape"], json!(shape));
    assert!(t["anchor"]["box"]["width"].as_f64().unwrap() > 0.0);
    let people: Vec<(Value, Value, Value)> =
        t["comments"].as_array().unwrap().iter().map(|c| (c["author"].clone(), c["name"].clone(), c["body"].clone())).collect();
    assert_eq!(people, vec![(json!("user:ann"), json!("Ann"), json!("Is this right?")), (json!("user:bob"), json!("Bob"), json!("Yes."))]);
    assert!(t["comments"][1]["created"].as_str().unwrap().ends_with('Z'), "a reply without `created` is stamped now");

    // Deleting the thread's first comment leaves its reply opening it.
    let head = t["comments"][0]["id"].as_str().unwrap().to_owned();
    b.execute("comment.delete", &json!({"thread": thread, "comment": head})).unwrap();
    sync(&mut a, &mut b);
    let t = threads(&mut a).remove(0);
    assert_eq!((t["comments"].as_array().unwrap().len(), t["comments"][0]["id"].clone()), (1, json!(reply)));
    assert_holds(&a, "after deleting the first comment");
    b.execute("comment.delete", &json!({"thread": thread, "comment": reply})).unwrap();
    sync(&mut a, &mut b);
    assert!(threads(&mut a).is_empty());
    assert_same(&mut a, &mut b, "after deleting the thread");
    // Undo is per person: B's deletes come back on B.
    b.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(threads(&mut b).len(), 1);
    assert_holds(&b, "after undo");
}

#[test]
fn a_commenter_comments_but_cannot_edit_and_writes_only_comment_objects() {
    let mut a = new_session();
    a.execute("comment.add", &json!({"text": "Ann's thread", "author": "user:ann", "name": "Ann"})).unwrap();
    let mut b = peer(&mut a);
    b.set_comment_only(Some("user:bob".into()));
    let before = doc(&b).clone();
    for (command, params) in [
        ("shape.insert", json!({"preset": "rect", "rect": [10, 10, 50, 50]})),
        ("slide.new", json!({})),
        ("slide.delete", json!({"index": 0})),
        ("comment.delete", json!({"all": true})),
    ] {
        let refused = b.execute(command, &params).unwrap_err().to_string();
        assert!(refused.contains("only comment") || refused.contains("name the comment"), "{command}: {refused}");
    }
    assert!(canonical(doc(&b)) == canonical(&before), "nothing changed");
    let base = b.snapshot().unwrap();
    let since = b.version().unwrap();
    let thread = threads(&mut b)[0]["id"].as_str().unwrap().to_owned();
    // Whatever author the call names, a commenter writes as themselves.
    b.execute("comment.reply", &json!({"thread": thread, "text": "From Bob", "author": "user:mallory", "name": "Bob"})).unwrap();
    b.execute("comment.add", &json!({"text": "Bob's own thread", "name": "Bob"})).unwrap();
    b.execute("comment.resolve", &json!({"thread": thread})).unwrap();
    let ann = threads(&mut b)[0]["comments"][0]["id"].as_str().unwrap().to_owned();
    assert!(b.execute("comment.delete", &json!({"thread": thread, "comment": ann})).is_err(), "only its author deletes a comment");
    let touched = touched_by(&base, &b.export_changes(&since).unwrap());
    assert!(!touched.is_empty());
    for (ty, object) in &touched {
        assert!(ty == "CommentThread" || ty == "Comment", "a commenter's change touched {ty} {object}");
    }
    let listed = threads(&mut b);
    let mut authors: Vec<String> =
        listed.iter().flat_map(|t| t["comments"].as_array().unwrap().iter().map(|c| c["author"].as_str().unwrap().to_owned())).collect();
    authors.sort();
    assert_eq!(authors, vec!["user:ann", "user:bob", "user:bob"]);
    // Their own steps undo.
    b.execute("edit.undo", &json!({})).unwrap();
    assert!(threads(&mut b)[0]["resolved"] == json!(false));
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "the commenter's comments reach the editor");
}

#[test]
fn comments_copied_with_a_slide_are_their_own_threads() {
    let mut a = new_session();
    a.execute("comment.add", &json!({"text": "Copy me", "author": "user:ann", "name": "Ann"})).unwrap();
    a.execute("slide.duplicate", &json!({})).unwrap();
    let listed = threads(&mut a);
    let ids: std::collections::HashSet<&str> = listed.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(listed.len(), 2, "the copy carries the comment");
    assert_eq!(ids.len(), 2, "each copy has its own id");
    assert_eq!(a.kdm().unwrap().replica().objects_of_type("CommentThread").len(), 2);
    assert_holds(&a, "after duplicating a slide with a comment");
    let mut b = peer(&mut a);
    assert_same(&mut a, &mut b, "a peer of it");
}

#[test]
fn table_format_undo_keeps_remote_cell_text_and_remote_merge_retargets_caret() {
    let mut a = new_session();
    a.execute("slide.go", &json!({"index": 2})).unwrap();
    a.execute("text.edit", &json!({"id": first_table(&a), "cell": [0, 1]})).unwrap();
    let mut b = peer(&mut a);
    b.execute("slide.go", &json!({"index": 2})).unwrap();
    b.execute("edit.select", &json!({"ids": [first_table(&b)]})).unwrap();
    a.execute("table.cellFormat", &json!({"anchor": "bottom"})).unwrap();
    b.execute("text.edit", &json!({"cell": [1, 1]})).unwrap();
    b.execute("text.insert", &json!({"text": "Remote content"})).unwrap();
    sync(&mut a, &mut b);
    a.execute("edit.undo", &json!({})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "undo table formatting keeps remote content");
    let ShapeKind::Table(table) = &doc(&a).slides[2].shapes[0].kind else { panic!("table") };
    assert_eq!(table.cell(1, 1).unwrap().text.text(), "Remote content");
    assert_eq!(table.cell(0, 1).unwrap().anchor, None);
    b.execute("table.merge", &json!({"from": [0, 0], "to": [0, 1]})).unwrap();
    sync(&mut a, &mut b);
    assert_eq!(a.active().unwrap().selection.text.as_ref().unwrap().cell, Some((0, 0)));
    assert_same(&mut a, &mut b, "remote merge keeps both replicas aligned");
}

/// `KitDrawing 1.0` DR14: concurrent drawing commands on one shape.
#[test]
fn concurrent_drawing_commands_on_one_shape_keep_both_sides() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    b.execute("slide.go", &json!({"index": 1})).unwrap();
    // KitDrawing ids are the shapes' replica objects: the same on both peers (DR2).
    let id = a.execute("drawing.describe", &json!({})).unwrap()["objects"][0]["id"].as_str().unwrap().to_string();
    let bid = b.execute("drawing.describe", &json!({})).unwrap()["objects"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(id, bid, "one id on every replica");
    assert!(id.parse::<u32>().is_err(), "a replica object id, not a local number: {id}");
    // Text against fill, and a move against an outline.
    a.execute("drawing.text", &json!({"id": id, "text": "Typed by A"})).unwrap();
    b.execute("drawing.format", &json!({"ids": [bid], "fill": {"color": "#00FF00"}})).unwrap();
    a.execute("drawing.transform", &json!({"ids": [id], "dx": 40})).unwrap();
    b.execute("drawing.format", &json!({"ids": [bid], "stroke": {"color": "#FF0000", "width": 4}})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after concurrent drawing commands");
    let d = a.execute("drawing.describe", &json!({"ids": [id]})).unwrap();
    let o = &d["objects"][0];
    assert_eq!(o["text"]["paragraphs"][0]["runs"][0]["text"], "Typed by A");
    assert_eq!(o["fill"]["color"]["rgb"], "#00FF00");
    assert_eq!(o["frame"]["x"], 140.0);
    assert_eq!(o["stroke"]["width"], 4.0);
    // Undo on A after B's edits reverts only A's last change.
    a.execute("edit.undo", &json!({})).unwrap();
    let d = a.execute("drawing.describe", &json!({"ids": [id]})).unwrap();
    assert_eq!(d["objects"][0]["frame"]["x"], 100.0);
    assert_eq!(d["objects"][0]["stroke"]["width"], 4.0);
    // Deleting against editing: the shape is deleted.
    b.execute("drawing.delete", &json!({"ids": [bid]})).unwrap();
    a.execute("drawing.format", &json!({"ids": [id], "fill": {"none": true}})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after a delete against an edit");
    assert_eq!(a.execute("drawing.describe", &json!({})).unwrap()["objects"].as_array().unwrap().len(), 1, "the shape is deleted");
}
