//! C8-land-9s: reproduce kit-env receipt/save differences without changing its SDK.
//! Run from the repository root: cargo run -p kition-slide-kit --example env_receipts
use kition_kit::KitDocument;
use kition_kit::kdm::reread;
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

fn core(value: &Value) -> Value {
    json!({ "created": value["created"], "changed": value["changed"], "moved": value["moved"], "deleted": value["deleted"] })
}

/// Use the same public object loads as kit-env, so shapes are keyed by tool id.
fn snapshot(exchange: &mut Exchange, doc: &mut SlideDocument) -> Result<Value, Box<dyn std::error::Error>> {
    let state = exchange.replica(doc)?.state_json()?;
    let loads: Vec<Value> = state["objects"].as_object().ok_or("no objects")?.keys().map(|id| json!({ "target": id })).collect();
    let answer = exchange.batch(doc, &json!({ "load": loads }), true)?;
    let mut objects = serde_json::Map::new();
    for object in answer["loaded"].as_array().ok_or("no loaded objects")? {
        let mut fields = object["fields"].as_object().ok_or("no fields")?.clone();
        fields.insert("$type".to_owned(), object["type"].clone());
        objects.insert(object["id"].as_str().ok_or("no object id")?.to_owned(), Value::Object(fields));
    }
    Ok(json!({ "objects": objects }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "conformance/check-empty-placeholder.kslide".to_owned());
    let bytes = std::fs::read(&path)?;
    let mut doc = SlideDocument::open(&path, &bytes)?;
    let count = doc.inspect()?["slides"].as_array().ok_or("no slides")?.len();
    let mut scenarios = Vec::new();
    let mut parts = Vec::new();
    for index in 0..count {
        parts.push(doc.execute("slide.inspect", json!({ "index": index }))?);
    }
    scenarios.push((
        "create slide".to_owned(),
        json!([{
            "op": "create", "type": "Slide", "in": "slide-file.slides", "at": 0, "fields": {}
        }]),
    ));
    for part in &parts {
        scenarios.push((
            format!("create shape in {}", part["toolId"]),
            json!([{
                "op": "create", "type": "Shape", "in": format!("{}.shapes", part["toolId"].as_str().ok_or("no slide id")?),
                "at": 0, "fields": { "kind": "shape", "name": "Overview" }
            }]),
        ));
        let shapes = part["shapes"].as_array().ok_or("no shapes")?;
        for (index, shape) in shapes.iter().enumerate() {
            scenarios.push((format!("delete {}", shape["toolId"]), json!([{ "op": "delete", "target": shape["toolId"] }])));
            if shapes.len() > 1 {
                scenarios.push((
                    format!("reorder {}", shape["toolId"]),
                    json!([{
                        "op": "list.move", "target": part["toolId"], "field": "shapes", "from": index, "to": (index + 1) % shapes.len()
                    }]),
                ));
            }
            for to in parts.iter().filter(|to| to["toolId"] != part["toolId"]) {
                scenarios.push((
                    format!("move {} to {}", shape["toolId"], to["toolId"]),
                    json!([
                        { "op": "list.remove", "target": part["toolId"], "field": "shapes", "index": index },
                        { "op": "list.insert", "target": to["toolId"], "field": "shapes", "item": shape["toolId"], "index": 0 }
                    ]),
                ));
            }
        }
    }
    for (name, ops) in scenarios {
        let mut doc = SlideDocument::open(&path, &bytes)?;
        let schema = doc.model_schema();
        let mut exchange = Exchange::new();
        let before = snapshot(&mut exchange, &mut doc)?;
        let version = exchange.replica(&mut doc)?.version();
        let batch = json!({ "origin": { "kind": "agent", "principal": "probe" }, "ops": ops });
        if let Err(error) = exchange.batch(&mut doc, &batch, false) {
            println!("{name}: refused: {error}");
            continue;
        }
        let saved = doc.save(Some(&path))?;
        let receipt = core(&exchange.replica(&mut doc)?.changes_since(&version)?.to_json());
        let mut again = SlideDocument::open(&path, &saved)?;
        let after = snapshot(&mut Exchange::new(), &mut again)?;
        let disk = core(&reread::diff_paired(&schema, &before, &after).to_json());
        println!("{}", json!({ "scenario": name, "ops": ops, "equal": receipt == disk, "receipt": receipt, "saved": disk }));
    }
    Ok(())
}
