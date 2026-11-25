//! Sections and custom shows: each a `Section` or `CustomShow` object, listed in the slide file's
//! `sections` or `customShows`, whose `slides` map holds one entry per member slide, so a draft
//! adding, removing or moving slides of a section (or show) the document changed meanwhile merges
//! per slide (data model 6.6, rule D4), and a rename never touches the members.
//!
//! An entry's key is the member slide's object id; a custom show that lists one slide again keys
//! its `n`-th listing `<object id>#<n>`. Its value is the member's place in the list (a number):
//! members read in order of place, then key, so two slides added at one place (by a draft and by
//! the document) both stay, in the same order everywhere.
//!
//! The repair after a merge ([`repair`]) drops each entry naming a slide that is gone, and, as a
//! slide belongs to one section at most, an entry of a section naming a slide an earlier section
//! (in the slide file's order) holds: a draft and the document moving one slide to two sections
//! leave it in the first (`tests/members.rs`). The reader reads the same.

use std::collections::{HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};
use kition_slide_model::SlideId;
use serde_json::{Map, Value, json};

use crate::ids::Ids;
use crate::refs::Locals;
use crate::{KdmResult, root_id};

/// The object type of a section.
pub(crate) const SECTION: &str = "Section";
/// The object type of a custom show.
pub(crate) const SHOW: &str = "CustomShow";
/// The members field of both.
pub(crate) const MEMBERS: &str = "slides";
/// What separates a slide's object id from its listing number in a repeated member's key.
const REPEAT: char = '#';

/// The entries a `slides` map holds for `slides` (see the module docs). A slide the view holds no
/// object for is left out.
pub(crate) fn entries(slides: &[SlideId], ids: &Ids) -> Map<String, Value> {
    let mut seen: HashMap<&ObjectId, usize> = HashMap::new();
    let mut out = Map::new();
    for (at, slide) in slides.iter().enumerate() {
        let Some(object) = ids.slides.object(*slide) else { continue };
        let n = seen.entry(object).or_default();
        *n += 1;
        let key = if *n == 1 { object.as_str().to_owned() } else { format!("{}{REPEAT}{n}", object.as_str()) };
        out.insert(key, json!(at));
    }
    out
}

/// The slide (its object) a member key names.
pub(crate) fn slide_of(key: &str) -> ObjectId {
    ObjectId::from_string(key.split_once(REPEAT).map_or(key, |(object, _)| object))
}

/// Makes `object`'s `slides` map hold exactly `wanted`, writing only the entries that differ.
/// Answers whether it wrote.
pub(crate) fn write(tx: &mut Tx<'_>, object: &ObjectId, wanted: &Map<String, Value>) -> KdmResult<bool> {
    let have: Map<String, Value> = tx.map_entries(object, MEMBERS)?.into_iter().collect();
    let mut wrote = false;
    for (key, at) in wanted {
        if have.get(key) != Some(at) {
            tx.map_set(object, MEMBERS, key, at.clone())?;
            wrote = true;
        }
    }
    for key in have.keys().filter(|key| !wanted.contains_key(*key)) {
        tx.map_delete(object, MEMBERS, key)?;
        wrote = true;
    }
    Ok(wrote)
}

/// The slides a `slides` map lists, in order (by place, then key), each one the reader holds.
pub(crate) fn read(entries: Vec<(String, Value)>, locals: &Locals) -> Vec<SlideId> {
    let mut members: Vec<(f64, String, SlideId)> = entries
        .into_iter()
        .filter_map(|(key, at)| {
            let slide = locals.slides.get(&slide_of(&key)).copied()?;
            Some((at.as_f64().filter(|at| at.is_finite()).unwrap_or(0.0), key, slide))
        })
        .collect();
    members.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    members.into_iter().map(|(_, _, slide)| slide).collect()
}

/// The repair after a merge (see the module docs). Answers whether it wrote.
pub(crate) fn repair(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = false;
    let mut placed: HashSet<ObjectId> = HashSet::new();
    let mut listed = HashSet::new();
    let sections: Vec<ObjectId> = tx.list_items(&root, "sections")?.into_iter().filter(|s| tx.exists(s) && listed.insert(s.clone())).collect();
    for section in &sections {
        for (key, _) in tx.map_entries(section, MEMBERS)? {
            let slide = slide_of(&key);
            if !tx.exists(&slide) || !placed.insert(slide) {
                tx.map_delete(section, MEMBERS, &key)?;
                wrote = true;
            }
        }
    }
    for show in tx.objects_of_type(SHOW) {
        for (key, _) in tx.map_entries(&show, MEMBERS)? {
            if !tx.exists(&slide_of(&key)) {
                tx.map_delete(&show, MEMBERS, &key)?;
                wrote = true;
            }
        }
    }
    Ok(wrote)
}
