//! A shape's members other than its frame, name, text and children as entries of its `look` map,
//! one per member written only where it changed, so two people changing two different ones of one
//! shape at once (its fill and its outline, a connector's end and its line) both keep theirs
//! (`KitDrawing 1.0` DR14). An ink layer's strokes are entries of their own, so strokes two people
//! draw on one slide at once both stay.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use serde_json::{Map, Value};

use crate::KdmResult;

/// The prefix of an ink layer's stroke entries in its `look`.
const STROKE: &str = "stroke:";

/// Writes `after`'s members into `object`'s `look`: those that differ from `before` (every one
/// for a new shape). An ink layer's strokes are entries of their own (see [`write_strokes`]).
pub(crate) fn write(tx: &mut Tx<'_>, object: &ObjectId, before: Option<&Value>, after: &Value) -> KdmResult<bool> {
    let before = before.map(|b| without_strokes(b).0);
    let (after, strokes) = without_strokes(after);
    let mut wrote = write_strokes(tx, object, strokes.as_deref().unwrap_or_default())?;
    let (was, now) = (before.as_ref().and_then(Value::as_object), after.as_object());
    for (k, v) in now.into_iter().flatten() {
        if was.and_then(|w| w.get(k)) != Some(v) {
            tx.map_set(object, "look", k, v.clone())?;
            wrote = true;
        }
    }
    for k in was.into_iter().flatten().map(|(k, _)| k).filter(|k| now.is_none_or(|n| !n.contains_key(*k))) {
        tx.map_delete(object, "look", k)?;
        wrote = true;
    }
    Ok(wrote)
}

/// `object`'s members: its `look` entries.
pub(crate) fn of(replica: &Replica, object: &ObjectId) -> KdmResult<Value> {
    let mut v: Map<String, Value> = Map::new();
    let mut strokes: Vec<(f64, String, Value)> = Vec::new();
    for (k, x) in replica.map_entries(object, "look").unwrap_or_default() {
        if k.starts_with(STROKE) {
            let n = x.get("n").and_then(Value::as_f64).unwrap_or(0.0);
            strokes.push((n, k, x.get("stroke").cloned().unwrap_or(Value::Null)));
        } else {
            v.insert(k, x);
        }
    }
    // An ink layer's strokes, in the order they were drawn (ties: by key).
    if let Some(kind) = v.get_mut("kind").and_then(Value::as_object_mut)
        && kind.get("type").and_then(Value::as_str) == Some("ink")
    {
        strokes.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        kind.insert("strokes".into(), Value::Array(strokes.into_iter().map(|(_, _, s)| s).filter(|s| !s.is_null()).collect()));
    }
    Ok(Value::Object(v))
}

/// `v` (a shape's members) without an ink layer's strokes, and those strokes.
fn without_strokes(v: &Value) -> (Value, Option<Vec<Value>>) {
    let mut v = v.clone();
    let strokes = v
        .get_mut("kind")
        .and_then(Value::as_object_mut)
        .filter(|k| k.get("type").and_then(Value::as_str) == Some("ink"))
        .and_then(|k| k.remove("strokes"))
        .map(|s| s.as_array().cloned().unwrap_or_default());
    (v, strokes)
}

/// Makes `object`'s stroke entries hold `strokes` (none for a shape that is not an ink layer):
/// each stroke one entry keyed by its content, so two people drawing on one slide at once both
/// keep their strokes, and an erased stroke's entry is deleted. An entry's `n` orders the strokes;
/// it is given once, past every entry there, and never rewritten (renumbering would bring an
/// erased stroke back when one person erased it while another renumbered it).
fn write_strokes(tx: &mut Tx<'_>, object: &ObjectId, strokes: &[Value]) -> KdmResult<bool> {
    let have: Map<String, Value> = tx.map_entries(object, "look").unwrap_or_default().into_iter().filter(|(k, _)| k.starts_with(STROKE)).collect();
    let mut next = have.values().filter_map(|x| x.get("n").and_then(Value::as_f64)).fold(0.0, f64::max) + 1.0;
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut keep = std::collections::HashSet::new();
    let mut wrote = false;
    for stroke in strokes {
        let hash = kition_kit::session::sha256_hex(stroke.to_string().as_bytes());
        let copy = seen.entry(hash.clone()).or_default();
        let key = format!("{STROKE}{}:{copy}", &hash[..16]);
        *copy += 1;
        if !have.contains_key(&key) {
            tx.map_set(object, "look", &key, serde_json::json!({ "n": next, "stroke": stroke }))?;
            next += 1.0;
            wrote = true;
        }
        keep.insert(key);
    }
    for key in have.keys().filter(|k| !keep.contains(*k)) {
        tx.map_delete(object, "look", key)?;
        wrote = true;
    }
    Ok(wrote)
}

/// One member of `object` in a transaction: its `look` entry.
pub(crate) fn member(tx: &Tx<'_>, object: &ObjectId, key: &str) -> KdmResult<Value> {
    tx.map_get(object, "look", key)
}

/// Sets one member of `object` in a transaction: its `look` entry.
pub(crate) fn set_member(tx: &mut Tx<'_>, object: &ObjectId, key: &str, v: Value) -> KdmResult<()> {
    tx.map_set(object, "look", key, v)
}
