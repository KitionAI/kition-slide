//! C8-slide-7: numeric declarations match the stored Rust integer types.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

#[test]
fn integer_fields_declare_their_storage_bounds() {
    let schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    for (ty, field, member, min, max) in [
        ("SlideFile", "props", "firstSlideNumber", 0_i64, i64::from(u32::MAX)),
        ("Slide", "props", "fileId", 256, 2_147_483_647),
        ("Slide", "slideId", "", 256, 2_147_483_647),
        ("Slide", "transition", "durationMs", 0, i64::from(u32::MAX)),
        ("Paragraph", "props", "level", 0, 8),
        ("Animation", "props", "durationMs", 0, i64::from(u32::MAX)),
        ("Animation", "props", "delayMs", 0, i64::from(u32::MAX)),
        ("Animation", "props", "repeat", 0, i64::from(u32::MAX)),
        ("Comment", "props", "kept.tz", i64::from(i32::MIN), i64::from(i32::MAX)),
    ] {
        let mut limits = &schema["types"][ty]["fields"][field];
        if !member.is_empty() {
            limits = &limits["members"][member];
        }
        assert_eq!(limits["multipleOf"], json!(1), "{ty}.{field}.{member}");
        assert_eq!(limits["minimum"], json!(min), "{ty}.{field}.{member}");
        assert_eq!(limits["maximum"], json!(max), "{ty}.{field}.{member}");
    }
}
