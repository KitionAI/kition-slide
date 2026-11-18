//! The deck the dependency rows run on (C8-slide-read-3), by role. See [`fixture`].

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

pub const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";

fn run(doc: &mut SlideDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn shapes(doc: &mut SlideDocument, index: usize) -> Vec<Value> {
    run(doc, "slide.inspect", json!({ "index": index }))["shapes"].as_array().cloned().unwrap_or_default()
}

/// The deck, and its objects by role (role, object id as the guard names it).
///
/// - `$S1` (index 0): the Title Slide layout; title `$S1Title` "Deck", subtitle `$S1Sub`
///   "Subtitle". Section "Intro" starts here.
/// - `$S2` (index 1): Title and Content; title placeholder `$Title` "Results", content
///   placeholder `$Body` "One\nTwo"; rectangle `$Rect` [100,300,120,80] "Box"; ellipse `$Oval`
///   [400,300,100,100]; connector `$Line` glued from `$Rect` to `$Oval`; group `$Group` of
///   rectangles `$GA` [600,100,80,60] and `$GB` [700,100,80,60]; table `$Table` 2×2 (cells "a" "b"
///   / "c" "d"); column chart `$Chart`; text box `$Text` "Note"; picture `$Pic` (media `$Media:red.png`);
///   video `$Video` (media `$Media:clip.mp4`, its poster `$Media:clip.mp4-poster.png`). Animations: 0 fade on `$Rect`, 1 fly on `$Oval`. Transition
///   fade 600 ms. Notes "Speak". A comment thread `$Thread` "Check" on `$Rect`. Section "Main"
///   starts here.
/// - `$S3` (index 2): Blank; star `$Star` [300,200,100,100].
/// - `$Master`: the slide master. Layouts, layouts' and the master's shapes and media are named by
///   [`labels`]: `$Layout:Title and Content`, `$Layout:Title and Content/Title 1`,
///   `$Master/Title 1`, `$Media:red.png`.
/// - `root`: the deck itself (`slide-file`): slideSize, sections, show, customShows ("Short":
///   slides 0 and 2), headerFooter, defaultTextStyle.
///
/// Editor state: the current slide is `$S2`, the selection `$Rect`; no text is being edited.
pub fn fixture() -> (SlideDocument, Vec<(&'static str, String)>) {
    let mut doc = SlideDocument::create("Reading.kslide", 960, 540).unwrap();
    let mut roles: Vec<(&'static str, String)> = Vec::new();
    // Slide 1: the title slide the new deck has.
    let s1 = shapes(&mut doc, 0);
    run(&mut doc, "text.set", json!({ "id": s1[0]["toolId"], "text": "Deck" }));
    run(&mut doc, "text.set", json!({ "id": s1[1]["toolId"], "text": "Subtitle" }));
    roles.push(("$S1Title", s1[0]["toolId"].as_str().unwrap().to_owned()));
    roles.push(("$S1Sub", s1[1]["toolId"].as_str().unwrap().to_owned()));
    // Slide 2.
    run(&mut doc, "slide.new", json!({ "layout": "titleAndContent", "title": "Results", "body": "One\nTwo" }));
    run(&mut doc, "slide.go", json!({ "index": 1 }));
    let base = shapes(&mut doc, 1);
    roles.push(("$Title", base[0]["toolId"].as_str().unwrap().to_owned()));
    roles.push(("$Body", base[1]["toolId"].as_str().unwrap().to_owned()));
    let made = |doc: &mut SlideDocument, answer: Value| -> String {
        let id = answer["id"].as_u64().unwrap_or_else(|| panic!("no id in {answer}"));
        shapes(doc, 1).iter().find(|shape| shape["id"] == id).unwrap()["toolId"].as_str().unwrap().to_owned()
    };
    let a = run(&mut doc, "shape.insert", json!({ "preset": "rect", "rect": [100, 300, 120, 80], "text": "Box" }));
    let rect = made(&mut doc, a);
    let a = run(&mut doc, "shape.insert", json!({ "preset": "ellipse", "rect": [400, 300, 100, 100] }));
    let oval = made(&mut doc, a);
    let a = run(&mut doc, "shape.connect", json!({ "from": rect, "to": oval }));
    let line = made(&mut doc, a);
    let a = run(&mut doc, "shape.insert", json!({ "preset": "rect", "rect": [600, 100, 80, 60] }));
    let ga = made(&mut doc, a);
    let a = run(&mut doc, "shape.insert", json!({ "preset": "rect", "rect": [700, 100, 80, 60] }));
    let gb = made(&mut doc, a);
    let a = run(&mut doc, "arrange.group", json!({ "ids": [ga, gb] }));
    let group = match a["id"].as_u64() {
        Some(_) => made(&mut doc, a),
        None => shapes(&mut doc, 1).iter().find(|shape| shape["kind"] == "group").unwrap()["toolId"].as_str().unwrap().to_owned(),
    };
    let a = run(&mut doc, "insert.table", json!({ "rows": 2, "cols": 2, "rect": [100, 120, 300, 100], "data": [["a", "b"], ["c", "d"]] }));
    let table = made(&mut doc, a);
    let a = run(
        &mut doc,
        "insert.chart",
        json!({ "type": "column", "rect": [500, 200, 200, 150], "categories": ["Q1", "Q2"], "series": [{ "name": "Sales", "values": [1, 2] }] }),
    );
    let chart = made(&mut doc, a);
    let a = run(&mut doc, "insert.textBox", json!({ "rect": [100, 450, 200, 40], "text": "Note" }));
    let text = made(&mut doc, a);
    let a = run(&mut doc, "insert.picture", json!({ "name": "red.png", "data": RED, "rect": [800, 300, 50, 50] }));
    let pic = made(&mut doc, a);
    let video = include_bytes!("../../../../crates/media/media/tests/fixtures/clip-h264.mp4");
    let a = run(
        &mut doc,
        "insert.video",
        json!({ "name": "clip.mp4", "data": kition_slide_engine::cmd::base64_encode(video), "rect": [800, 400, 120, 80] }),
    );
    let clip = made(&mut doc, a);
    run(&mut doc, "animation.add", json!({ "effect": "fade", "ids": [rect] }));
    run(&mut doc, "animation.add", json!({ "effect": "fly", "ids": [oval] }));
    run(&mut doc, "transition.set", json!({ "index": 1, "kind": "fade", "duration": 600 }));
    run(&mut doc, "slide.notes", json!({ "index": 1, "text": "Speak" }));
    let thread = run(&mut doc, "comment.add", json!({ "slide": 1, "id": rect, "text": "Check", "author": "colleague" }));
    for (role, id) in [
        ("$Rect", &rect),
        ("$Oval", &oval),
        ("$Line", &line),
        ("$Group", &group),
        ("$GA", &ga),
        ("$GB", &gb),
        ("$Table", &table),
        ("$Chart", &chart),
        ("$Text", &text),
        ("$Pic", &pic),
        ("$Video", &clip),
    ] {
        roles.push((role, id.clone()));
    }
    roles.push(("$Thread", thread["thread"].as_str().unwrap_or_else(|| panic!("no thread in {thread}")).to_owned()));
    // Slide 3.
    run(&mut doc, "slide.new", json!({ "layout": "blank", "at": 2 }));
    run(&mut doc, "slide.go", json!({ "index": 2 }));
    let a = run(&mut doc, "shape.insert", json!({ "preset": "star5", "rect": [300, 200, 100, 100] }));
    let star_number = a["id"].as_u64().unwrap();
    let star = shapes(&mut doc, 2).iter().find(|shape| shape["id"] == star_number).unwrap()["toolId"].as_str().unwrap().to_owned();
    roles.push(("$Star", star));
    run(&mut doc, "section.add", json!({ "name": "Intro", "at": 0 }));
    run(&mut doc, "section.add", json!({ "name": "Main", "at": 1 }));
    run(&mut doc, "show.customShow", json!({ "name": "Short", "slides": [0, 2] }));
    // Slides and the objects without tool ids, by the replica's ids.
    let state = Exchange::new().replica(&mut doc).unwrap().state_json().unwrap();
    let objects = state["objects"].as_object().unwrap().clone();
    let slides: Vec<Value> = run(&mut doc, "document.inspect", json!({}))["slides"].as_array().cloned().unwrap_or_default();
    for (role, slide) in ["$S1", "$S2", "$S3"].into_iter().zip(&slides) {
        roles.push((role, slide["toolId"].as_str().map(str::to_owned).unwrap_or_else(|| format!("slide:{}", slide["id"]))));
    }
    let find = |ty: &str, test: &dyn Fn(&Value) -> bool| -> Option<String> {
        objects.iter().find(|(_, object)| object["$type"] == ty && test(object)).map(|(id, _)| id.clone())
    };
    if let Some(master) = find("Master", &|_| true) {
        roles.push(("$Master", master));
    }
    roles.push(("root", "slide-file".to_owned()));
    run(&mut doc, "slide.go", json!({ "index": 1 }));
    run(&mut doc, "edit.select", json!({ "ids": [rect] }));
    (doc, roles)
}

/// A readable name for every object of the replica the roles leave out: `$Layout:<name>`, a
/// layout's or master's shape `$Layout:<name>/<shape>` or `$Master/<shape>`, `$Media:<name>`, and
/// otherwise `<type>:<id>`.
pub fn labels(doc: &mut SlideDocument) -> Vec<(String, String)> {
    let state = Exchange::new().replica(doc).unwrap().state_json().unwrap();
    let objects = state["objects"].as_object().unwrap();
    let mut out = Vec::new();
    for (id, object) in objects {
        let ty = object["$type"].as_str().unwrap_or_default();
        match ty {
            "Layout" => {
                let name = object["props"]["name"].as_str().unwrap_or_default();
                out.push((id.clone(), format!("$Layout:{name}")));
                let public = format!("layout:{}", object["identity"].as_str().unwrap());
                out.push((public.clone(), format!("$Layout:{name}")));
                for (at, shape) in object["shapes"].as_array().into_iter().flatten().filter_map(Value::as_str).enumerate() {
                    let label = format!("$Layout:{name}/{}", objects[shape]["name"].as_str().unwrap_or_default());
                    out.push((shape.to_owned(), label.clone()));
                    // Source-evidenced alias: KDM Layout.shapes[at] is this exact Shape UUID.
                    // Used only to compare equivalent guard facts on a public holder.
                    out.push((format!("@path:{public}.shapes.{at}"), label));
                }
            }
            "Master" => {
                for shape in object["shapes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    out.push((shape.to_owned(), format!("$Master/{}", objects[shape]["name"].as_str().unwrap_or_default())));
                }
            }
            "Media" => out.push((id.clone(), format!("$Media:{}", object["props"]["name"].as_str().unwrap_or_default()))),
            _ => {}
        }
    }
    for (id, object) in objects {
        if !out.iter().any(|(known, _)| known == id) {
            out.push((id.clone(), format!("{}:{id}", object["$type"].as_str().unwrap_or_default())));
        }
    }
    // Canonical paths for directly held inherited body registers (the same facts as
    // `<placeholder>.body.props`, independent of the raw TextBody UUID).
    let bodies: Vec<_> = objects
        .iter()
        .filter_map(|(id, object)| {
            let (_, owner) = out.iter().find(|(known, label)| known == id && (label.starts_with("$Layout:") || label.starts_with("$Master/")))?;
            Some((object["body"].as_str()?.to_owned(), format!("@body({owner})")))
        })
        .collect();
    out.extend(bodies);
    out
}
