//! DrawingML effect lists replace inherited lists, including explicitly empty lists.

use kition_slide_color::Rgba;
use kition_slide_model::style::{ColorRef, Glow, Reflection, Shadow};
use kition_slide_model::text::RunProps;

fn parent() -> RunProps {
    RunProps {
        shadow: Some(Shadow {
            color: ColorRef::rgb(Rgba::BLACK),
            blur: 3.0,
            dist: 2.0,
            dir: 45.0,
            inner: false,
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            align: "ctr".into(),
            rotate_with_shape: true,
        }),
        glow: Some(Glow { color: ColorRef::rgb(Rgba::BLACK), radius: 4.0 }),
        reflection: Some(Reflection { dist: 4.0, ..Default::default() }),
        effects_explicit: true,
        ..Default::default()
    }
}

#[test]
fn explicit_empty_effects_survive_multiple_inheritance_levels() {
    let mut layout = RunProps { effects_explicit: true, ..Default::default() };
    layout.inherit(&parent());
    let mut shape = RunProps::default();
    shape.inherit(&layout);
    shape.inherit(&parent());
    let mut run = RunProps::default();
    run.inherit(&shape);
    run.inherit(&parent());
    assert!(run.effects_explicit);
    assert!(run.shadow.is_none());
    assert!(run.glow.is_none());
    assert!(run.reflection.is_none());
}

#[test]
fn absent_effects_inherit_the_complete_parent_list() {
    let mut run = RunProps::default();
    run.inherit(&parent());
    assert_eq!(run, parent());
}

#[test]
fn glow_only_and_legacy_effects_do_not_merge_parent_shadows() {
    for explicit in [false, true] {
        let mut run = RunProps { glow: parent().glow, effects_explicit: explicit, ..Default::default() };
        run.inherit(&parent());
        assert!(run.shadow.is_none());
        assert!(run.reflection.is_none());
        assert_eq!(run.glow, parent().glow);
        let mut descendant = RunProps::default();
        descendant.inherit(&run);
        descendant.inherit(&parent());
        assert!(descendant.shadow.is_none());
    }
}

#[test]
fn effect_presence_is_backward_compatible_and_serializes_only_when_set() {
    let old: RunProps = serde_json::from_str("{}").unwrap();
    assert!(!old.effects_explicit);
    assert!(serde_json::to_value(&old).unwrap().get("effectsExplicit").is_none());
    let empty = RunProps { effects_explicit: true, ..Default::default() };
    let value = serde_json::to_value(&empty).unwrap();
    assert_eq!(value["effectsExplicit"], true);
    assert_eq!(serde_json::from_value::<RunProps>(value).unwrap(), empty);
}

#[test]
fn reflection_only_effects_replace_the_parent_list() {
    for explicit in [false, true] {
        let mut run = RunProps { reflection: parent().reflection, effects_explicit: explicit, ..Default::default() };
        run.inherit(&parent());
        assert!(run.shadow.is_none());
        assert!(run.glow.is_none());
        assert_eq!(run.reflection, parent().reflection);
        let mut child = RunProps::default();
        child.inherit(&run);
        child.inherit(&parent());
        assert_eq!(child, run);
    }
}

#[test]
fn legacy_reflection_native_defaults_preserve_original_placement() {
    let legacy = r#"{"blur":0,"startAlpha":1,"endAlpha":0,"endPos":1,"dist":0}"#;
    let reflection: Reflection = serde_json::from_str(legacy).unwrap();
    assert_eq!(reflection, Reflection::default());
    assert_eq!(reflection.sy, -1.0);
    assert_eq!(reflection.dir, 90.0);
    assert_eq!(reflection.fade_dir, 90.0);
    assert_eq!(reflection.align, "bl");
    assert!(!reflection.rotate_with_shape);
    let absent: RunProps = serde_json::from_str("{}").unwrap();
    assert!(absent.reflection.is_none());
    assert!(serde_json::to_value(absent).unwrap().get("reflection").is_none());
    let authored = RunProps {
        reflection: Some(Reflection {
            start_pos: 0.15,
            sx: 0.75,
            sy: -0.6,
            dir: 35.0,
            fade_dir: 100.0,
            kx: -12.0,
            ky: 8.0,
            align: "tr".into(),
            rotate_with_shape: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let json = serde_json::to_value(&authored).unwrap();
    assert_eq!(json["reflection"]["startPos"], 0.15);
    assert_eq!(json["reflection"]["fadeDir"], 100.0);
    assert_eq!(serde_json::from_value::<RunProps>(json).unwrap(), authored);
}
