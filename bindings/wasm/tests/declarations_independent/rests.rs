//! C8-slide-read-3: what each write call rests on, read from its handler and the engine operation
//! behind it, compared with what the guard holds for the call (the command's declarations and
//! `under`).
//!
//! Each row is one code path of a command on [`super::fixture::fixture`], with the editor state the
//! path reads (`part`: the current slide or the master/layout in Slide Master view; `selection`:
//! the selected shapes or slides, the text being edited, table cells) and, for each object the
//! write rests on, the fields held (`*` every field, `-` none: only that it is there, or a
//! comma-separated list of field paths, `look.fill`, `props.name`, `~shapeCount`) and the positions
//! held (`*` where it is matters, `-` it does not, or `address`). What the guard holds for a call
//! is computed as `kition-kits`' guard does ([`held`]): the target parameter's ids on every field
//! (and every position unless the command is position-independent), the objects under a place the
//! call gives, then `under`'s premises in their place (the last premise of an object wins), then
//! every other string of the parameters that names an object, on every field.
//!
//! The rules the rows follow (what a write rests on):
//! - what decides what the call acts on: the object a parameter, the selection or the current
//!   slide names (its existence; its position when named by place or by an old positional
//!   address), the item an index names (an animation, a comment thread, a section);
//! - the values the call reads to compute what it writes: the fields read, the positions when it
//!   reads where the object is; another object used as an operand, an endpoint, a source, a
//!   layout, a placeholder it inherits from, the master's theme;
//! - the field it replaces whole (a lost update otherwise);
//! - not: what only refuses the call (a kind check), what only makes a new name or number unique
//!   unless the result keeps it, what only shapes the receipt.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::{KitCommand, KitDocument};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

#[path = "rests_a.rs"]
mod rests_a;
#[path = "rests_b.rs"]
mod rests_b;
#[path = "rests_c.rs"]
mod rests_c;
#[path = "rests_d.rs"]
mod rests_d;
#[path = "rests_e.rs"]
mod rests_e;
#[path = "rests_f.rs"]
mod rests_f;

/// Editor-state facts added after the earlier declaration comparison, retained as reviewed
/// evidence rather than blind readings. These are unions across the command's paths, not claims
/// that every call reads every state.
///
/// Reviewed: explicit-id drawing/shape delegates, fresh freeform/smart graphic insertion, and
/// design.fonts' master lookup have no selection facts (the view is Part). recovery-discard has
/// no Part fact: recovery files do not use the editor's current part. The sampled
/// table Part facts are supported by with_table -> edit_shapes -> current_shapes; its selection
/// fallbacks remain separate facts.
// C8-slide-read-4: all 31 originally unsampled facts checked. Eight erroneous facts already
// removed by review remain removed: drawing.chart/delete/reconnect/text/ungroup Selection
// (cmd/drawing.rs:291-349,433-448), insert.smartGraphic Selection (cmd/insert.rs:938-963),
// shape.delete Selection (cmd/shape.rs:1012-1027), recovery.discard Part (cmd/file.rs:494-504).
// The 23 surviving unsampled facts are RIGHT; evidence is attached below (shared table helper).
pub const SCANNED: &[(&str, &str)] = &[
    // C8-slide-read-4 RIGHT: cmd/drawing.rs:361-380 resolves the edited part; required ids replace selection.
    ("drawing.arrange", "part"),
    // C8-slide-read-4 RIGHT: cmd/drawing.rs:291-349 resolves the edited part; required ids replace selection.
    ("drawing.chart", "part"),
    // C8-slide-read-4 RIGHT: cmd/drawing.rs:301-321 resolves the edited part; explicit delegate ids replace selection.
    ("drawing.delete", "part"),
    ("drawing.format", "part"),
    ("drawing.group", "part"),
    ("drawing.reconnect", "part"),
    // C8-slide-read-4 RIGHT: cmd/drawing.rs:291-354 resolves the edited part; required ids replace selection.
    ("drawing.text", "part"),
    ("drawing.transform", "part"),
    // C8-slide-read-4 RIGHT: cmd/drawing.rs:433-448 resolves required ids in the edited part.
    ("drawing.ungroup", "part"),
    // C8-slide-read-4 RIGHT: cmd/edit.rs:42-84 stores/restores/follows both selection and part.
    ("edit.redo", "part"),
    ("edit.redo", "selection"),
    // C8-slide-read-4 RIGHT: cmd/edit.rs:42-84 stores/restores/follows both selection and part.
    ("edit.undo", "part"),
    ("edit.undo", "selection"),
    // C8-slide-read-4 RIGHT: cmd/insert.rs:601-616 inserts into text or delegates header/footer on the current part.
    ("insert.dateTime", "part"),
    ("insert.hyperlink", "part"),
    // C8-slide-read-4 RIGHT: cmd/insert.rs:601-611 inserts into text or delegates header/footer on the current part.
    ("insert.slideNumber", "part"),
    // C8-slide-read-4 RIGHT: cmd/slide.rs:557-567 clamps the current slide when removing slides.
    ("section.remove", "part"),
    ("shape.delete", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.border", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.cellFill", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.cellFormat", "part"),
    ("table.clearCells", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.columnWidth", "part"),
    ("table.deleteColumn", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.deleteRow", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.distributeColumns", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:106-117 falls back to text/cells/shapes when id is absent.
    ("table.distributeColumns", "selection"),
    ("table.distributeRows", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.insertColumnLeft", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.insertColumnRight", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.insertRowAbove", "part"),
    ("table.insertRowBelow", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.merge", "part"),
    ("table.options", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:106-117 falls back to text/cells/shapes when id is absent.
    ("table.options", "selection"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.rowHeight", "part"),
    // C8-slide-read-4 RIGHT: cmd/table.rs:105-155 resolves/mutates the table in the edited part.
    ("table.split", "part"),
    ("table.style", "part"),
];

/// One code path of a command: steps run on the fixture first (`setup`, a JSON list of
/// `[command, params]`, roles in place of ids), its parameters (roles in place of ids), the editor
/// state it reads (sorted, comma-separated) and what a write with `basis` must hold of each object
/// (role, fields, positions). `blind: false` marks a row pinned from the declaration after review.
pub struct Rest {
    pub command: &'static str,
    pub setup: &'static str,
    pub params: &'static str,
    pub editor: &'static str,
    pub holds: &'static [(&'static str, &'static str, &'static str)],
    pub blind: bool,
}

pub const fn r(
    command: &'static str,
    params: &'static str,
    editor: &'static str,
    holds: &'static [(&'static str, &'static str, &'static str)],
) -> Rest {
    Rest { command, setup: "", params, editor, holds, blind: true }
}

pub const fn rs(
    setup: &'static str,
    command: &'static str,
    params: &'static str,
    editor: &'static str,
    holds: &'static [(&'static str, &'static str, &'static str)],
) -> Rest {
    Rest { command, setup, params, editor, holds, blind: true }
}

/// A row pinned from the declaration after review (the reading could not say).
pub const fn pinned(mut row: Rest) -> Rest {
    row.blind = false;
    row
}

pub fn rows() -> Vec<&'static Rest> {
    rests_a::ROWS.iter().chain(rests_b::ROWS).chain(rests_c::ROWS).chain(rests_d::ROWS).chain(rests_e::ROWS).chain(rests_f::ROWS).collect()
}

fn with_roles(value: &Value, roles: &[(&str, String)]) -> Value {
    match value {
        Value::String(text) => roles.iter().find(|(role, _)| role == text).map_or_else(|| value.clone(), |(_, id)| json!(id)),
        Value::Array(items) => Value::Array(items.iter().map(|item| with_roles(item, roles)).collect()),
        Value::Object(fields) => Value::Object(fields.iter().map(|(key, item)| (key.clone(), with_roles(item, roles))).collect()),
        _ => value.clone(),
    }
}

/// A premise's fields or positions as the rows spell them.
fn spelled(set: Option<&Vec<String>>) -> String {
    let Some(items) = set else { return "*".to_owned() };
    let set: BTreeSet<&String> = items.iter().collect();
    if set.is_empty() { "-".to_owned() } else { set.into_iter().cloned().collect::<Vec<_>>().join(",") }
}

fn normal(text: &str) -> String {
    if text == "*" || text == "-" {
        return text.to_owned();
    }
    let set: BTreeSet<&str> = text.split(',').map(str::trim).filter(|item| !item.is_empty()).collect();
    if set.is_empty() { "-".to_owned() } else { set.into_iter().collect::<Vec<_>>().join(",") }
}

/// The strings of a parameter value that may be ids, one level deep, as the guard reads them.
fn mentioned(value: &Value, out: &mut BTreeSet<String>) {
    let mut one = |value: &Value| {
        if let Some(text) = value.as_str().filter(|text| !text.is_empty() && !text.starts_with('$') && *text != "root") {
            out.insert(text.to_owned());
        }
    };
    match value {
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Object(object) => object.values().for_each(&mut one),
                    other => one(other),
                }
            }
        }
        Value::Object(object) => object.values().for_each(&mut one),
        other => one(other),
    }
}

/// What a write with `basis` of `command` with `params` holds, as the guard computes it: the editor
/// state the command declares, and for each object its fields and positions.
pub fn held(
    doc: &SlideDocument,
    command: &KitCommand,
    params: &Value,
    known: &BTreeMap<String, String>,
) -> (String, BTreeMap<String, (String, String)>) {
    let mut editor: Vec<&str> = command.reads.iter().map(|state| state.as_str()).collect();
    editor.sort_unstable();
    let name = |id: &str| known.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let mut ids: BTreeMap<String, (String, String)> = BTreeMap::new();
    let target = command.targets;
    let given = |value: &Value| -> Vec<Value> {
        match value {
            Value::Array(items) => items.clone(),
            Value::Null => Vec::new(),
            other => vec![other.clone()],
        }
    };
    if let Some(target) = target {
        for value in params.get(target).map(given).unwrap_or_default() {
            if let Some(id) = value.as_str().filter(|id| known.contains_key(*id)) {
                ids.insert(name(id), ("*".to_owned(), if command.position_independent { "-" } else { "*" }.to_owned()));
            }
        }
    }
    let placed = command.places.iter().any(|place| params.get(*place).is_some_and(|value| !value.is_null()));
    let under = doc.under(&command.id, params).unwrap_or_else(|error| panic!("{} {params}: {error}", command.id));
    if let Some(under) = &under {
        if placed {
            for object in &under.objects {
                if let Some(id) = object.as_str() {
                    ids.entry(name(id)).or_insert_with(|| ("*".to_owned(), "*".to_owned()));
                }
            }
        }
        let mut projected = Vec::new();
        for premise in &under.premises {
            let id = premise.id.as_str().map(str::to_owned).unwrap_or_else(|| premise.id.to_string());
            let label = name(&id);
            if let Some(owner) = label.strip_prefix("@body(").and_then(|label| label.strip_suffix(')')) {
                assert_eq!(premise.positions, Some(Vec::new()), "an inherited body hold reads content only");
                let fields = premise.fields.as_ref().unwrap();
                let entry = ids.entry(owner.into()).or_insert_with(|| ("-".into(), "-".into()));
                if entry.0 != "*" {
                    for field in fields {
                        let path = format!("body.{field}");
                        entry.0 = if entry.0 == "-" { path } else { format!("{},{path}", entry.0) };
                    }
                }
            } else {
                let mut plain = premise.fields.clone();
                if let Some(fields) = &mut plain {
                    fields.retain(|field| {
                        let path = format!("@path:{id}.{field}");
                        if let Some((alias, role)) = known
                            .iter()
                            .filter(|(alias, _)| alias.starts_with("@path:") && (path == **alias || path.starts_with(&format!("{alias}."))))
                            .max_by_key(|(alias, _)| alias.len())
                        {
                            let rest = path.strip_prefix(alias).unwrap().trim_start_matches('.');
                            // Stored Shape.identity is the identity/existence fact of this role.
                            projected.push((
                                role.clone(),
                                if rest == "identity" {
                                    "-".to_owned()
                                } else if rest.is_empty() {
                                    "*".into()
                                } else {
                                    rest.into()
                                },
                            ));
                            false
                        } else {
                            true
                        }
                    });
                }
                // An id inside a path (a section's member entry `slides.slide:262`) by its name.
                if let Some(fields) = &mut plain {
                    for field in fields.iter_mut() {
                        *field =
                            field.split('.').map(|part| known.get(part).cloned().unwrap_or_else(|| part.to_owned())).collect::<Vec<_>>().join(".");
                    }
                }
                ids.insert(label, (spelled(plain.as_ref()), spelled(premise.positions.as_ref())));
            }
        }
        for (role, field) in projected {
            let entry = ids.entry(role).or_insert_with(|| ("-".into(), "-".into()));
            if entry.0 != "*" && field != "-" {
                entry.0 = if field == "*" || entry.0 == "-" { field } else { format!("{},{field}", entry.0) };
            }
        }
        // An ordered list hold is a membership/order fact, not every member's content.
        for order in &under.orders {
            let parent = order.parent.as_str().unwrap();
            let entry = ids.entry(name(parent)).or_insert_with(|| ("-".into(), "-".into()));
            if entry.0 != "*" {
                let fact = format!("@order({})", order.field);
                entry.0 = if entry.0 == "-" { fact } else { format!("{},{fact}", entry.0) };
            }
        }
    }
    let mut strings = BTreeSet::new();
    for (key, value) in params.as_object().into_iter().flatten() {
        if Some(key.as_str()) != target && !command.places.contains(&key.as_str()) {
            mentioned(value, &mut strings);
        }
    }
    for text in strings {
        if known.contains_key(&text) {
            ids.entry(name(&text)).or_insert_with(|| ("*".to_owned(), "*".to_owned()));
        }
    }
    (editor.join(","), ids)
}

/// Whether the fields or positions `held` cover `needed`: `*` every one, `-` none, else paths, a
/// held path covering itself and every path below it.
fn covers(held: &str, needed: &str) -> bool {
    if held == "*" || needed == "-" {
        return true;
    }
    if needed == "*" || held == "-" {
        return false;
    }
    needed.split(',').all(|need| held.split(',').any(|have| need == have || need.starts_with(&format!("{have}."))))
}

/// How a call's holds differ from its reading: (holds less somewhere, holds more somewhere).
fn ways(holds: &BTreeMap<String, (String, String)>, expected: &BTreeMap<String, (String, String)>) -> (Vec<String>, Vec<String>) {
    let none = ("-".to_owned(), "-".to_owned());
    let mut less = Vec::new();
    let mut more = Vec::new();
    for (object, need) in expected {
        let have = holds.get(object).unwrap_or(&none);
        if !holds.contains_key(object) || !covers(&have.0, &need.0) || !covers(&have.1, &need.1) {
            less.push(format!("{object} needs {need:?}, holds {}", holds.get(object).map_or("nothing".to_owned(), |h| format!("{h:?}"))));
        }
    }
    for (object, have) in holds {
        let need = expected.get(object).unwrap_or(&none);
        if !covers(&need.0, &have.0) || !covers(&need.1, &have.1) || (!expected.contains_key(object) && have != &none) {
            more.push(format!("{object} holds {have:?}, needs {}", expected.get(object).map_or("nothing".to_owned(), |n| format!("{n:?}"))));
        }
    }
    (less, more)
}

/// The fixture after `setup`, its roles (by role and label) and every object's readable name.
fn world(setup: &str, command: &str) -> (SlideDocument, Vec<(String, String)>, BTreeMap<String, String>) {
    let (mut doc, named) = super::fixture::fixture();
    let mut roles: Vec<(String, String)> = named.iter().map(|(role, id)| ((*role).to_owned(), id.clone())).collect();
    let initial = super::fixture::labels(&mut doc);
    for (id, label) in &initial {
        if label.starts_with('$') {
            roles.push((label.clone(), id.clone()));
        }
    }
    if !setup.is_empty() {
        let view: Vec<(&str, String)> = roles.iter().map(|(role, id)| (role.as_str(), id.clone())).collect();
        let steps: Value = with_roles(&serde_json::from_str(setup).unwrap_or_else(|e| panic!("{command} setup: {e}")), &view);
        for step in steps.as_array().into_iter().flatten() {
            let name = step[0].as_str().unwrap_or_default();
            doc.execute(name, step[1].clone()).unwrap_or_else(|e| panic!("{command} setup {name}: {e}"));
        }
    }
    let mut known: BTreeMap<String, String> = super::fixture::labels(&mut doc).into_iter().collect();
    for (role, id) in &roles {
        known.insert(id.clone(), role.clone());
    }
    (doc, roles, known)
}

/// The comparison, as lines, and the commands differing, holding less and holding more.
pub fn compare() -> (Vec<String>, BTreeSet<&'static str>, BTreeSet<&'static str>, BTreeSet<&'static str>, usize) {
    let base = world("", "");
    let commands = base.0.commands();
    let writes: BTreeSet<&str> = commands.iter().filter(|c| c.mutates).map(|c| c.id.as_str()).collect();
    let mut lines = Vec::new();
    let (mut differing, mut less, mut more) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut editors: BTreeMap<&'static str, (BTreeSet<&str>, String)> = BTreeMap::new();
    for row in rows() {
        let Some(command) = commands.iter().find(|c| c.id == row.command && c.mutates) else {
            lines.push(format!("{}: a reading of no write command", row.command));
            differing.insert(row.command);
            continue;
        };
        let fresh;
        let (doc, roles, known) = if row.setup.is_empty() {
            (&base.0, &base.1, &base.2)
        } else {
            fresh = world(row.setup, row.command);
            (&fresh.0, &fresh.1, &fresh.2)
        };
        let view: Vec<(&str, String)> = roles.iter().map(|(role, id)| (role.as_str(), id.clone())).collect();
        let params = with_roles(&serde_json::from_str(row.params).unwrap_or_else(|e| panic!("{} {}: {e}", row.command, row.params)), &view);
        let (editor, holds) = held(doc, command, &params, known);
        let entry = editors.entry(row.command).or_insert_with(|| (BTreeSet::new(), editor));
        entry.0.extend(row.editor.split(',').map(str::trim).filter(|state| !state.is_empty()));
        let mut expected: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (object, fields, at) in row.holds {
            expected.insert((*object).to_owned(), (normal(fields), normal(at)));
        }
        let holds: BTreeMap<String, (String, String)> = holds.into_iter().map(|(k, (f, p))| (k, (normal(&f), normal(&p)))).collect();
        let (fewer, wider) = ways(&holds, &expected);
        if !fewer.is_empty() || !wider.is_empty() {
            differing.insert(row.command);
            if !fewer.is_empty() {
                less.insert(row.command);
            }
            if !wider.is_empty() {
                more.insert(row.command);
            }
            lines.push(format!(
                "{} {}{}",
                row.command,
                row.params,
                fewer.iter().map(|l| format!("\n  LESS {l}")).chain(wider.iter().map(|m| format!("\n  MORE {m}"))).collect::<String>()
            ));
        }
    }
    for (command, state) in SCANNED {
        editors.get_mut(command).unwrap_or_else(|| panic!("{command}: no row")).0.insert(state);
    }
    for (command, (read, declared)) in &editors {
        let declared: BTreeSet<&str> = declared.split(',').filter(|state| !state.is_empty()).collect();
        if read != &declared {
            differing.insert(command);
            if !read.is_subset(&declared) {
                less.insert(command);
            }
            if !declared.is_subset(read) {
                more.insert(command);
            }
            lines.push(format!("{command}: EDITOR declares {declared:?}, its reading {read:?}"));
        }
    }
    let read: BTreeSet<&str> = rows().iter().map(|row| row.command).collect();
    for missing in writes.difference(&read) {
        lines.push(format!("{missing}: a write command with no reading"));
    }
    (lines, differing, less, more, writes.len())
}
