//! Commands that act on shapes take their ids as numbers or strings, and an id that names no
//! shape is an error: it used to be dropped, and the command then edited the selection.
use kition_slide_engine::Session;
use serde_json::json;

fn width(s: &Session, id: u64) -> f64 {
    let st = s.doc().unwrap();
    let shape = st.shape(kition_slide_model::ShapeId(id as u32)).unwrap();
    shape.xfrm.unwrap().w
}

#[test]
fn a_shape_id_may_be_a_string() {
    let mut s = Session::with_new();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 40]})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 100, 100, 40]})).unwrap();
    s.execute("shape.resize", &json!({"ids": [a.to_string()], "w": 300})).unwrap();
    assert_eq!(width(&s, a), 300.0);
}

#[test]
fn an_unknown_id_is_an_error_and_the_selection_is_untouched() {
    let mut s = Session::with_new();
    let selected = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 40]})).unwrap()["id"].as_u64().unwrap();
    for ids in [json!(["01a1858a-fb85-7718-887d-f448dcf6a50d"]), json!([9999]), json!(["no such"])] {
        let error = s.execute("shape.resize", &json!({"ids": ids, "w": 300})).unwrap_err().to_string();
        assert!(error.contains("no shape"), "{error}");
    }
    let error = s.execute("shape.resize", &json!({"id": "9999", "w": 300})).unwrap_err().to_string();
    assert!(error.contains("no shape `9999`"), "{error}");
    assert_eq!(width(&s, selected), 100.0, "the selected shape kept its size");
    s.execute("shape.resize", &json!({"w": 250})).unwrap();
    assert_eq!(width(&s, selected), 250.0, "without ids the selection is still the target");
}

#[test]
fn going_to_a_slide_past_the_end_is_an_error() {
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({})).unwrap();
    let error = s.execute("slide.go", &json!({"index": 4})).unwrap_err().to_string();
    assert!(error.contains("no slide at index 4: the deck has 2 slides"), "{error}");
    assert_eq!(s.execute("slide.go", &json!({"index": 1})).unwrap()["index"], 1);
}

#[test]
fn an_east_asian_font_is_the_runs_east_asian_font_too() {
    // With only <a:latin>, PowerPoint and LibreOffice draw Han characters in the theme's East
    // Asian font and the digits in the chosen one.
    let mut s = Session::with_new();
    let id =
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 300, 60], "text": "2026 年第三季度"})).unwrap()["id"].as_u64().unwrap();
    s.execute("format.fontName", &json!({"family": "PingFang SC"})).unwrap();
    let props = |s: &Session| {
        let st = s.doc().unwrap();
        let sh = st.shape(kition_slide_model::ShapeId(id as u32)).unwrap();
        let run = sh.text.as_ref().unwrap().paragraphs[0].runs[0].props.clone();
        (run.font, run.font_ea)
    };
    assert_eq!(props(&s), (Some("PingFang SC".into()), Some("PingFang SC".into())));
    s.execute("format.fontName", &json!({"family": "Inter"})).unwrap();
    assert_eq!(props(&s), (Some("Inter".into()), Some("PingFang SC".into())), "a Latin family leaves the East Asian font");
}

#[test]
fn theme_fonts_in_an_east_asian_family_are_the_east_asian_fonts_too() {
    let mut s = Session::with_new();
    let before = s.doc().unwrap().doc.masters[0].theme.fonts.clone();
    s.execute("theme.fonts", &json!({"major": "Microsoft YaHei", "minor": "Microsoft YaHei"})).unwrap();
    let fonts = s.doc().unwrap().doc.masters[0].theme.fonts.clone();
    assert_eq!((fonts.minor.latin.as_str(), fonts.minor.ea.as_str()), ("Microsoft YaHei", "Microsoft YaHei"));
    s.execute("theme.fonts", &json!({"major": "Inter", "minor": "Inter"})).unwrap();
    let fonts = s.doc().unwrap().doc.masters[0].theme.fonts.clone();
    assert_eq!((fonts.minor.latin.as_str(), fonts.minor.ea.as_str()), ("Inter", "Microsoft YaHei"), "a Latin family keeps the East Asian font");
    assert_eq!(fonts.minor.cs, before.minor.cs, "the complex-script font stays");
}

#[test]
fn a_font_this_machine_lacks_is_said() {
    let mut s = Session::with_new();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 300, 60], "text": "Hello"})).unwrap();
    let answer = s.execute("format.fontName", &json!({"family": "No Such Family 123"})).unwrap();
    assert_eq!(answer["available"], false, "{answer}");
    assert!(answer["note"].as_str().unwrap().contains("is not installed here"), "{answer}");
    let answer = s.execute("format.fontName", &json!({"family": "Inter"})).unwrap();
    assert!(answer.get("available").is_none(), "a bundled family needs no note: {answer}");
    let answer = s.execute("theme.fonts", &json!({"major": "No Such Family 123", "minor": "Inter"})).unwrap();
    assert_eq!(answer["fonts"].as_array().unwrap().len(), 1, "{answer}");
}

#[test]
fn formatting_takes_the_same_ids_as_shape_commands() {
    // format.* select the named shapes through edit.select, which read numbers only.
    let mut s = Session::with_new();
    let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 300, 60], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselectAll", &json!({})).ok();
    s.execute("format.fontSize", &json!({"ids": [id.to_string()], "size": 30})).unwrap();
    let st = s.doc().unwrap();
    let size = st.shape(kition_slide_model::ShapeId(id as u32)).unwrap().text.as_ref().unwrap().paragraphs[0].runs[0].props.size;
    assert_eq!(size, Some(30.0));
    let error = s.execute("format.fontSize", &json!({"ids": ["9999"], "size": 30})).unwrap_err().to_string();
    assert!(error.contains("go to their slide first"), "{error}");
}

#[test]
fn a_kitdrawing_run_font_this_machine_lacks_is_said() {
    let mut s = Session::with_new();
    let answer = s
        .execute(
            "drawing.insert",
            &json!({"kind": "textBox", "frame": {"x": 10, "y": 10, "w": 300, "h": 60},
        "paragraphs": [{"runs": [{"text": "Hi", "font": "No Such Family 123"}]}]}),
        )
        .unwrap();
    assert_eq!(answer["fonts"][0]["available"], false, "{answer}");
    assert!(answer.get("id").is_some(), "the id is still answered: {answer}");
}

#[test]
fn shape_delete_deletes_the_named_shapes_only() {
    let mut s = Session::with_new();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 40]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 100, 100, 40]})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("shape.delete", &json!({})).unwrap_err().to_string().contains("name the shapes"));
    s.execute("shape.delete", &json!({"ids": [a.to_string()]})).unwrap();
    let st = s.doc().unwrap();
    assert!(st.shape(kition_slide_model::ShapeId(a as u32)).is_none());
    assert!(st.shape(kition_slide_model::ShapeId(b as u32)).is_some(), "the selected shape stays");
}

#[test]
fn a_font_family_given_as_a_scheme_name_says_how_to_set_it() {
    let mut s = Session::with_new();
    let error = s.execute("theme.fonts", &json!({"name": "Arial"})).unwrap_err().to_string();
    assert!(error.contains("pass `major` and `minor`"), "{error}");
}
