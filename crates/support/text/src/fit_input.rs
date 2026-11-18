//! Stable receipts for saved autofit results. No replica identity or font-file state enters
//! the receipt: peers keep the winning saved result when its semantic inputs still match.

use kition_slide_geom::{Affine, Rect};
use kition_slide_model::Shape;
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::{Bullet, RunProps, TextBody, TextDir};
use sha2::{Digest, Sha256};

fn metrics(mut props: RunProps) -> RunProps {
    props.link = None;
    props.fill = None;
    props.outline = None;
    props.highlight = None;
    props.shadow = None;
    props.glow = None;
    props.effects_explicit = false;
    props.underline = None;
    props.underline_color = None;
    props.strike = None;
    props.no_proof = None;
    props.alt_lang = None;
    props.dirty = None;
    props.smt_clean = None;
    props
}

fn coordinate(value: f64) -> serde_json::Value {
    if value.is_nan() {
        return "nan".into();
    }
    if !value.is_finite() {
        return if value.is_sign_positive() { "+infinity" } else { "-infinity" }.into();
    }
    // Geometry can use transcendental functions whose final bits vary by target. A millionth
    // of a point is far below layout/raster precision. Avoid overflow for hostile large values.
    let rounded = if value.abs() < f64::MAX / 1e6 { (value * 1e6).round() / 1e6 } else { value };
    serde_json::json!(if rounded == 0.0 { 0.0 } else { rounded })
}

/// SHA-256 of versioned, canonical layout inputs, independent of local ids and run splitting.
/// JSON object keys are sorted; typed fields and arrays fix their ordering across targets.
/// The complete text participates, so no prefix/truncation can hide a concurrent insertion.
pub fn autofit_input(ctx: &Ctx, shape: &Shape, body: &TextBody, rect: Rect) -> String {
    autofit_input_with_transform(ctx, shape, body, rect, resolve::xfrm(ctx, shape).affine())
}

/// A receipt with the actual text transform. Only East Asian vertical layout depends on its
/// orientation; unrelated text receipts remain byte-for-byte compatible.
pub fn autofit_input_with_transform(ctx: &Ctx, shape: &Shape, body: &TextBody, rect: Rect, transform: Affine) -> String {
    let mut bp = resolve::body(ctx, shape);
    let orientation = (bp.vert == Some(TextDir::EaVertical)).then(|| super::upright_east_asian(ctx, shape, &bp, transform));
    bp.autofit = None;
    bp.autofit_saved = None;
    bp.autofit_input = None;
    // Kept 3-D properties affect painting, not the flat text layout measured here.
    bp.scene3d_xml = None;
    bp.text3d_xml = None;
    let paragraphs: Vec<_> = body
        .paragraphs
        .iter()
        .map(|para| {
            let mut normalized = para.clone();
            normalized.props = resolve::para(ctx, shape, para);
            normalized.props.bullet_color = None;
            if let Some(Bullet::Picture { media }) = &mut normalized.props.bullet {
                // Picture bullets occupy a font-sized square, independent of the media id.
                *media = kition_slide_model::MediaId(0);
            }
            normalized.end_props = metrics(resolve::run(ctx, shape, para, &para.end_props));
            for run in &mut normalized.runs {
                run.props = metrics(resolve::run(ctx, shape, para, &run.props));
            }
            normalized.normalize();
            normalized
        })
        .collect();
    let mut value = serde_json::json!({
        "version": 1,
        "rect": [coordinate(rect.x0), coordinate(rect.y0), coordinate(rect.x1), coordinate(rect.y1)],
        "body": bp,
        "paragraphs": paragraphs,
        "fonts": [ &ctx.master.theme.fonts.major, &ctx.master.theme.fonts.minor ],
        "textBox": shape.text_box,
    });
    if let Some(upright) = orientation {
        value["eastAsianUpright"] = upright.into();
    }
    // Value serialization cannot fail: it contains only JSON values, not arbitrary map keys.
    let digest = Sha256::digest(value.to_string().as_bytes());
    format!("v1:{digest:x}")
}
