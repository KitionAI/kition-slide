//! Picture thresholds and optional object-preview corrections preserve native compatibility.

use kition_slide_model::ShapeKind;
use kition_slide_model::style::PictureAdjust;

#[test]
fn older_native_values_have_no_bilevel_or_preview_correction() {
    let adjust: PictureAdjust = serde_json::from_str("{}").unwrap();
    assert_eq!(adjust, PictureAdjust::default());
    assert!(serde_json::to_value(&adjust).unwrap().get("biLevel").is_none());
    let opaque: ShapeKind = serde_json::from_str(r#"{"type":"opaque","xml":"<p:graphicFrame/>","preview":7,"label":"Object"}"#).unwrap();
    assert!(matches!(&opaque, ShapeKind::Opaque { preview_adjust: None, .. }));
    assert!(serde_json::to_value(&opaque).unwrap().get("previewAdjust").is_none());
}

#[test]
fn native_threshold_zero_is_distinct_from_an_absent_effect() {
    for threshold in [0.0, 0.25, 1.0] {
        let adjust = PictureAdjust { bi_level: Some(threshold), ..Default::default() };
        let opaque = ShapeKind::Opaque {
            xml: "<p:graphicFrame/>".into(),
            preview: Some(kition_slide_model::MediaId(7)),
            preview_adjust: Some(adjust.clone()),
            label: "Object".into(),
            rels: vec![],
        };
        let value = serde_json::to_value(&opaque).unwrap();
        assert_eq!(value["previewAdjust"]["biLevel"], threshold);
        assert_eq!(serde_json::from_value::<ShapeKind>(value).unwrap(), opaque);
        assert_ne!(adjust, PictureAdjust::default());
    }
}
