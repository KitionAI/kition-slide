//! Integer bounds on nullable members, nested paths and map entries (kition-sdk 0.1.0: a bounded
//! member that may be null declares `nullable: true`). The sweep also found doc.revision, body.columns, bullet.startAt, table cell spans,
//! placeholder/connector indexes, media timing/dimensions/bookmarks, chart style/axis/series
//! indexes, Comment.props.kept.tz, and RGBA channels in nested JSON or maps. Their Rust decoders reject fractions,
//! but the pinned schema cannot declare the bounds at those locations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::kdm::Schema;
use serde_json::json;

#[test]
fn nullable_integer_members_keep_null_and_refuse_fractions() {
    let schema = Schema::parse(
        &json!({
            "kdm": 1, "kit": "kit.slide", "root": "Root", "schemaVersion": 1,
            "types": {"Root": {"fields": {"props": {"kind": "json", "members": {
                "paragraph": {"minimum": 0, "maximum": 4294967295_u64, "multipleOf": 1, "nullable": true}
            }}}}}
        })
        .to_string(),
    )
    .unwrap();
    assert!(schema.check_value("Root", "props", &json!({"paragraph": null})).is_ok(), "a bounded Option<u32> must retain its null value");
    assert!(schema.check_value("Root", "props", &json!({"paragraph": 0.5})).is_err(), "a nullable bound still refuses a fraction");
}

#[test]
fn nested_integer_members() {
    let schema = Schema::parse(
        &json!({
            "kdm": 1, "kit": "kit.slide", "root": "Root", "schemaVersion": 1,
            "types": {"Root": {"fields": {"props": {"kind": "json", "members": {
                "doc.revision": {"minimum": 0, "maximum": 4294967295_u64, "multipleOf": 1}
            }}}}}
        })
        .to_string(),
    )
    .unwrap();
    assert!(schema.check_value("Root", "props", &json!({"doc": {"revision": 0.5}})).is_err(), "nested u32 bounds must validate their nested value");
}

#[test]
fn comment_timezone_member() {
    let schema = Schema::parse(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    assert!(
        schema.check_value("Comment", "props", &json!({"kept": {"tz": 0.5}})).is_err(),
        "Comment.props.kept.tz is a nested Option<i32>, not a top-level props.tz"
    );
}

#[test]
fn integer_map_entries() {
    let schema = Schema::parse(
        &json!({
            "kdm": 1, "kit": "kit.slide", "root": "Root", "schemaVersion": 1,
            "types": {"Root": {"fields": {"parts": {"kind": "map", "members": {
                "style": {"minimum": 0, "maximum": 4294967295_u64, "multipleOf": 1}
            }}}}}
        })
        .to_string(),
    );
    assert!(schema.is_ok(), "chart map entries need numeric declarations: {schema:?}");
}
