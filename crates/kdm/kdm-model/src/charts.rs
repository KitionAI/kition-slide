//! A chart as its own `Chart` object (schema version 4, `KitDrawing` DR14): a shape of kind
//! `chart` names it in its `chart` ref, and each of the chart's members (its type, title, legend,
//! categories and series, style, kept part…) is an entry of its `parts` map, so two people
//! changing different members of one chart at once both keep their change. The kept chart part
//! (`raw`) is never changed by an edit, only by a new file, so it never fights an edit.
//!
//! A schema 3 replica holds a chart in its shape's `props`: it reads as it is, and the first write
//! of the shape makes its `Chart` object (under an id derived from the shape's, so two replicas
//! doing so at once make the same object) and raises the minimum schema version, so a schema 3
//! build refuses the replica instead of writing charts it no longer reads.

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value};

use crate::KdmResult;

/// The minimum schema version once a chart is its own object.
pub const CHART_SCHEMA_VERSION: u32 = 4;

/// The object type.
pub(crate) const CHART: &str = "Chart";

/// A shape's serialized kind split into what stays in the shape's `props` (`{"type": "chart"}`) and
/// the chart's members, by name.
pub(crate) fn split(kind: &mut Value) -> Map<String, Value> {
    let Some(map) = kind.as_object_mut() else { return Map::new() };
    let mut parts = Map::new();
    let keys: Vec<String> = map.keys().filter(|k| k.as_str() != "type").cloned().collect();
    for k in keys {
        if let Some(v) = map.remove(&k) {
            parts.insert(k, v);
        }
    }
    parts
}

/// The id the chart of shape `shape` is made under: the same in every replica.
fn chart_id(shape: &ObjectId) -> ObjectId {
    let base: String = shape.as_str().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-')).take(120).collect();
    ObjectId::from_string(format!("{base}-chart"))
}

/// Makes shape `shape`'s chart hold `parts`, writing only the members that differ from what its
/// object holds (a new object when it has none). Answers whether it wrote.
pub(crate) fn write(tx: &mut Tx<'_>, shape: &ObjectId, parts: &Map<String, Value>) -> KdmResult<bool> {
    let stored = match tx.get(shape, "chart")? {
        Value::String(s) => Some(ObjectId::from_string(s)).filter(|o| tx.exists(o)),
        _ => None,
    };
    let Some(object) = stored else {
        let object = match tx.create_with_id(CHART, &chart_id(shape), Map::new()) {
            Ok(id) => id,
            Err(_) => tx.create(CHART, Map::new())?,
        };
        for (k, v) in parts {
            tx.map_set(&object, "parts", k, v.clone())?;
        }
        tx.set(shape, "chart", Value::String(object.as_str().to_owned()))?;
        tx.raise_schema_version(CHART_SCHEMA_VERSION)?;
        return Ok(true);
    };
    let have: Map<String, Value> = tx.map_entries(&object, "parts")?.into_iter().collect();
    let mut wrote = false;
    for (k, v) in parts {
        if have.get(k) != Some(v) {
            tx.map_set(&object, "parts", k, v.clone())?;
            wrote = true;
        }
    }
    for k in have.keys().filter(|k| !parts.contains_key(*k)) {
        tx.map_delete(&object, "parts", k)?;
        wrote = true;
    }
    Ok(wrote)
}

/// The members of shape `shape`'s chart object, or `None` when it has none (a schema 3 replica's
/// chart, in the shape's `props`).
pub(crate) fn read(replica: &kition_kit::kdm::Replica, shape: &ObjectId) -> KdmResult<Option<Map<String, Value>>> {
    let Value::String(object) = replica.get(shape, "chart")? else { return Ok(None) };
    let object = ObjectId::from_string(object);
    if !replica.exists(&object) {
        return Ok(None);
    }
    Ok(Some(replica.map_entries(&object, "parts")?.into_iter().collect()))
}
