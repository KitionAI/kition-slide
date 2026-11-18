//! C8-slide-pin: persisted layout identities make saved references executable.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};
use std::sync::Arc;

fn load(doc: &mut SlideDocument, id: &str, field: &str) -> Value {
    Exchange::default().batch(doc, &json!({"ops": [], "load": [{"target": id, "fields": [field]}]}), true).unwrap()["loaded"][0]["fields"][field]
        .clone()
}

#[test]
fn layout_ids_survive_part_reorder_rename_deletion_and_new_layouts() {
    let mut doc = SlideDocument::create("Layouts.pptx", 960, 540).unwrap();
    let bytes = doc.save(Some("Layouts.pptx")).unwrap();
    let mut model = kition_slide_engine::cmd::file::open_slide_file("Layouts.pptx", &bytes).unwrap();
    let master = Arc::make_mut(&mut model.masters[0]);
    let kept = master.layouts[0].identity.clone();
    let deleted = master.layouts.remove(master.layouts.len() - 1).identity;
    master.layouts[0].name = "Renamed".into();
    master.layouts.rotate_left(1);
    let changed = kition_slide_engine::cmd::file::save_bytes(&model, "pptx").unwrap();
    let mut reopened = SlideDocument::open("Layouts.pptx", &changed).unwrap();
    let id = format!("layout:{kept}");
    assert_eq!(load(&mut reopened, &id, "props")["name"], "Renamed");
    assert!(Exchange::default().batch(&mut reopened, &json!({"ops": [], "load": [{"target": format!("layout:{deleted}")}]}), true).is_err());
    reopened.execute("master.insertLayout", json!({"name": "New"})).unwrap();
    assert_eq!(load(&mut reopened, &id, "props")["name"], "Renamed");
    let again = reopened.save(Some("Layouts.pptx")).unwrap();
    let mut reopened = SlideDocument::open("Layouts.pptx", &again).unwrap();
    assert_eq!(load(&mut reopened, &id, "props")["name"], "Renamed");
    let slide = load(&mut reopened, "root", "slides")[0].as_str().unwrap().to_owned();
    assert_eq!(load(&mut reopened, &slide, "layout"), id);
    // Identity is immutable after creation.
    assert!(
        Exchange::default()
            .batch(&mut reopened, &json!({"ops": [{"op":"set", "target": id, "field":"identity", "value":"changed"}]}), false)
            .is_err()
    );
    // A history-bearing reread with a changed layout set/order is refused safely.
    doc.execute("slide.rename", json!({"index": 0, "name": "Own edit"})).unwrap();
    assert!(!doc.reconcile_reread(&reopened).unwrap());
}

#[test]
fn layout_metadata_repairs_deterministically_and_raw_creates_get_an_identity() {
    let mut doc = SlideDocument::create("Layouts.pptx", 960, 540).unwrap();
    let bytes = doc.save(Some("Layouts.pptx")).unwrap();
    let mut model = kition_slide_engine::cmd::file::open_slide_file("Layouts.pptx", &bytes).unwrap();
    let layouts = &mut Arc::make_mut(&mut model.masters[0]).layouts;
    layouts[0].identity.clear();
    layouts[1].identity = layouts[2].identity.clone();
    let mut other = model.clone();
    kition_slide_model::identity::settle(&mut model);
    kition_slide_model::identity::settle(&mut other);
    assert_eq!(model, other);
    let ids: std::collections::BTreeSet<_> = model.masters.iter().flat_map(|master| &master.layouts).map(|layout| &layout.identity).collect();
    assert_eq!(ids.len(), model.masters[0].layouts.len());
    let made = Exchange::default()
        .batch(&mut doc, &json!({"ops": [{"op":"create", "type":"Layout", "in":"root.masters.0.layouts", "fields":{"props":{"name":"Raw"}}}]}), false)
        .unwrap();
    let master = load(&mut doc, "root", "masters")[0].as_str().unwrap().to_owned();
    let layouts = load(&mut doc, &master, "layouts");
    let layout = layouts.as_array().unwrap().last().unwrap().as_str().unwrap();
    assert!(layout.starts_with("layout:"), "{made}");
    assert_eq!(load(&mut doc, layout, "props")["name"], "Raw");
}

#[test]
fn identityless_layouts_do_not_reuse_slot_ids_after_insertion() {
    let doc = SlideDocument::create("Layouts.pptx", 960, 540).unwrap();
    let mut before = kition_slide_engine::cmd::file::open_slide_file("Layouts.pptx", &doc.save(Some("Layouts.pptx")).unwrap()).unwrap();
    for master in &mut before.masters {
        for layout in &mut Arc::make_mut(master).layouts {
            layout.identity.clear();
        }
    }
    let mut after = before.clone();
    let master = Arc::make_mut(&mut after.masters[0]);
    let mut inserted = master.layouts[0].clone();
    inserted.id = kition_slide_model::LayoutId(50000);
    master.layouts.insert(0, inserted);
    // A format reader assigns local numbers in part order, reusing the old slot numbers.
    for (at, layout) in master.layouts.iter_mut().enumerate() {
        layout.id = before.masters[0].layouts.get(at).map_or(kition_slide_model::LayoutId(50001), |old| old.id);
    }
    kition_slide_model::identity::settle(&mut before);
    kition_slide_model::identity::settle(&mut after);
    let ids: std::collections::BTreeSet<_> = before.masters.iter().flat_map(|master| &master.layouts).map(|layout| &layout.identity).collect();
    assert!(after.masters.iter().flat_map(|master| &master.layouts).all(|layout| !ids.contains(&layout.identity)));
}

#[test]
fn native_slide_commands_take_the_persisted_layout_tool_id() {
    let mut doc = SlideDocument::create("Layouts.pptx", 960, 540).unwrap();
    doc.execute("master.insertLayout", json!({"name":"Owned layout"})).unwrap();
    let file = kition_slide_engine::cmd::file::open_slide_file("Layouts.pptx", &doc.save(Some("Layouts.pptx")).unwrap()).unwrap();
    let layout = file.masters.iter().flat_map(|master| &master.layouts).find(|layout| layout.name == "Owned layout").unwrap();
    let id = format!("layout:{}", layout.identity);
    doc.execute("slide.new", json!({"layout":id})).unwrap();
    assert_eq!(doc.execute("slide.inspect", json!({"index":1})).unwrap()["layout"], "Owned layout");
    doc.execute("slide.layout", json!({"index":0, "layout":id})).unwrap();
    assert_eq!(doc.execute("slide.inspect", json!({"index":0})).unwrap()["layout"], "Owned layout");
}
