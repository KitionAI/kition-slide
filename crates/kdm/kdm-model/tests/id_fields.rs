//! Every engine id stored as data must be written as an object id and translated back
//! (`kdm_model`'s `refs` module and its explicit cases). This lists every model field typed with an
//! id, so a new one fails here until the bridge handles it.

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../engine/model/src/lib.rs")),
    ("anim.rs", include_str!("../../../engine/model/src/anim.rs")),
    ("chart.rs", include_str!("../../../engine/model/src/chart.rs")),
    ("style.rs", include_str!("../../../engine/model/src/style.rs")),
    ("table.rs", include_str!("../../../engine/model/src/table.rs")),
    ("text.rs", include_str!("../../../engine/model/src/text.rs")),
    ("theme.rs", include_str!("../../../engine/model/src/theme.rs")),
];

/// `file: field` for every field whose type names an id, and how the bridge handles it.
const HANDLED: &[&str] = &[
    // Definitions: each is an object's own identity, minted by the reader (never replicated).
    "lib.rs: id: ShapeId",
    "lib.rs: id: SlideId",
    "lib.rs: id: LayoutId",
    "lib.rs: id: MasterId",
    "lib.rs: id: MediaId",
    // Slide.layout: the `layout` ref field.
    "lib.rs: layout: LayoutId",
    // refs::walk, key `media` / `poster` / `preview` / `sound`.
    "lib.rs: media: MediaId",
    "lib.rs: poster: Option<MediaId>",
    "lib.rs: preview: Option<MediaId>",
    "style.rs: media: MediaId",
    "text.rs: media: crate::MediaId",
    "anim.rs: sound: Option<crate::MediaId>",
    // refs::walk, `{"kind": "slide", "slide": id}`.
    "text.rs: Slide { slide: crate::SlideId }",
    // refs::walk, `{"type": "connector", "start"/"end": [id, idx]}`.
    "lib.rs: start: Option<(ShapeId, u32)>",
    "lib.rs: end: Option<(ShapeId, u32)>",
    // Explicit: animations, comments, sections, custom shows, embedded fonts.
    "anim.rs: shape: ShapeId",
    "anim.rs: trigger: Option<ShapeId>",
    "lib.rs: shape: Option<ShapeId>",
    "lib.rs: slides: Vec<SlideId>",
    "lib.rs: embedded_fonts: Vec<(String, MediaId)>",
];

#[test]
fn every_id_field_is_handled() {
    let mut found = Vec::new();
    for (file, source) in SOURCES {
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("//") || line.starts_with("pub fn") || line.starts_with("fn ") || line.contains("=>") || line.contains(" = ") {
                continue;
            }
            let line = line.trim_start_matches("pub ").trim_end_matches(',');
            let names_id = ["ShapeId", "SlideId", "LayoutId", "MasterId", "MediaId"].iter().any(|id| line.contains(id));
            if names_id
                && line.contains(": ")
                && !line.starts_with("id_type!")
                && !line.starts_with("use ")
                && !line.contains("ShapeId(")
                && !line.contains("SlideId(")
                && !line.contains("LayoutId(")
            {
                found.push(format!("{file}: {line}"));
            }
        }
    }
    found.sort();
    found.dedup();
    let unhandled: Vec<_> = found.iter().filter(|f| !HANDLED.contains(&f.as_str())).collect();
    assert!(unhandled.is_empty(), "model fields holding ids that kdm-model does not translate: {unhandled:#?}");
    let stale: Vec<_> = HANDLED.iter().filter(|h| !found.iter().any(|f| f == *h)).collect();
    assert!(stale.is_empty(), "listed as handled but no longer in the model (update this list): {stale:#?}");
}
