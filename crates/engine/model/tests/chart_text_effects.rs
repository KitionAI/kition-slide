//! Chart text uses the same whole-list effect inheritance as DrawingML runs.

use kition_slide_color::Rgba;
use kition_slide_model::chart::ChartText;
use kition_slide_model::style::{ColorRef, Glow};

#[test]
fn explicit_empty_and_legacy_nonempty_lists_block_ancestors() {
    let parent = ChartText { glow: Some(Glow { color: ColorRef::rgb(Rgba::BLACK), radius: 4.0 }), ..Default::default() };
    let mut empty = ChartText { effects_explicit: true, ..Default::default() };
    empty.inherit_effects(&parent);
    assert!(empty.glow.is_none());
    let mut descendant = ChartText::default();
    descendant.inherit_effects(&empty);
    descendant.inherit_effects(&parent);
    assert!(descendant.effects_explicit && descendant.glow.is_none());
    let mut legacy = parent.clone();
    legacy.inherit_effects(&empty);
    assert_eq!(legacy, parent);
    let mut inherited = ChartText::default();
    inherited.inherit_effects(&parent);
    assert_eq!(inherited, parent);
}

#[test]
fn old_chart_text_defaults_remain_serialization_compatible() {
    let old: ChartText = serde_json::from_str("{}").unwrap();
    let json = serde_json::to_value(&old).unwrap();
    for field in ["shadow", "glow", "effectsExplicit"] {
        assert!(json.get(field).is_none());
    }
    let empty = ChartText { effects_explicit: true, ..Default::default() };
    let json = serde_json::to_value(&empty).unwrap();
    assert_eq!(json["effectsExplicit"], true);
    assert_eq!(serde_json::from_value::<ChartText>(json).unwrap(), empty);
}
