//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on slides (KHP
//! A21): pictures and media the slide refers to but does not hold or cannot show, text that does not
//! fit its box, fonts drawn with a substitute, placeholders left empty, shapes off the slide, and
//! pictures without alt text, slides the renderer fails on, and effects too large to draw. Shapes
//! are named as `shape:<id>` (the ids the engine's commands take in `ids`) and, when the slide has a
//! replica, by their object id (`model.describe`); slides count from 1.
//!
//! Every slide is drawn once, as the show draws it at 1:1 ([`kition_slide_render::probe_slide`]).
//! Natively a renderer panic is caught and reported as `slide.render-failed`; a WebAssembly build
//! cannot catch one (it aborts the worker), which is why the renderer leaves out effects whose
//! extents it cannot draw instead (`slide.effects-dropped`) rather than reaching that panic.
//!
//! Codes: `slide.missing-media`, `slide.unreadable-picture`, `slide.render-failed`,
//! `slide.text-overflow`, `slide.effects-dropped`, `slide.font-substituted`,
//! `slide.empty-placeholder`, `slide.off-slide`, `slide.missing-alt-text`.

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::diagnostics::{Diagnostic, Severity, Target};
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::AutoFit;
use kition_slide_model::{Background, Fill, MediaId, Shape, ShapeId, ShapeKind, SlideFile};
use serde_json::json;

/// One code the check reports: its severity and a one-sentence summary for the catalogue.
pub struct Entry {
    pub code: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

const fn entry(code: &'static str, severity: Severity, summary: &'static str) -> Entry {
    Entry { code, severity, summary }
}

/// Every code [`check`] reports that a file can carry, errors first: the catalogue shipped as
/// `agent/checks.json` (KHP A38), with one conformance fixture per code
/// (`conformance/check-<name>.kslide`). `slide.render-failed` is reported too but has no entry:
/// it needs a renderer panic, which no file is known to cause ([`UNCATALOGUED`]).
pub const CATALOGUE: &[Entry] = &[
    entry("slide.missing-media", Severity::Error, "A picture or media clip names bytes the slide file does not hold, so it shows nothing."),
    entry("slide.unreadable-picture", Severity::Error, "A picture's bytes are not a format the slide can show, so it draws nothing."),
    entry("slide.text-overflow", Severity::Warning, "Text does not fit its box, so it spills over the box's edge."),
    entry("slide.effects-dropped", Severity::Warning, "A shape's effects are too large to draw, so it is shown without them."),
    entry(
        "slide.font-substituted",
        Severity::Warning,
        "A run's font is not available here, so its text is drawn with a substitute and may wrap differently.",
    ),
    entry("slide.off-slide", Severity::Warning, "A shape lies entirely outside the slide, so the show never displays it."),
    entry("slide.empty-placeholder", Severity::Info, "A placeholder holds no text: the editor shows its prompt and the show shows nothing."),
    entry("slide.missing-alt-text", Severity::Info, "A picture has no alt text for screen readers."),
];

/// Codes [`check`] reports that no file is known to produce: a renderer panic.
pub const UNCATALOGUED: &[&str] = &["slide.render-failed"];

/// The catalogue as `agent/checks.json` holds it: code, severity, summary, and its fixture.
pub fn catalogue_json() -> serde_json::Value {
    serde_json::Value::Array(
        CATALOGUE
            .iter()
            .map(|entry| {
                json!({
                    "code": entry.code,
                    "severity": entry.severity,
                    "summary": entry.summary,
                    "fixture": format!("check-{}.kslide", entry.code.trim_start_matches("slide.")),
                })
            })
            .collect(),
    )
}

/// Everything the slide file's slides show that is worth fixing, errors first. Targets use the
/// same stored tool ids before replica initialization and after saving and reopening.
pub fn check(pres: &SlideFile) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let held: BTreeMap<MediaId, &kition_slide_model::MediaItem> = pres.media.iter().map(|item| (item.id, item)).collect();
    let (slide_w, slide_h) = (pres.slide_size.width, pres.slide_size.height);
    for (index, slide) in pres.slides.iter().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let object = |id: ShapeId| slide.shape(id).filter(|shape| !shape.identity.is_empty()).map(kition_slide_engine::cmd::inspect::shape_tool_id);
        let on = |shape: &Shape| Target {
            slide: Some(number),
            path: Some(format!("shape:{}", shape.id.0)),
            object_id: object(shape.id),
            ..Target::default()
        };
        if let Some(Background::Fill { fill: Fill::Picture(picture) }) = &slide.background {
            media(&held, picture.media, Target { slide: Some(number), path: Some("background".into()), ..Target::default() }, &mut found);
        }
        let ctx = Ctx::for_slide(pres, slide);
        kition_slide_model::walk(&slide.shapes, &mut |shape, depth| {
            for id in media_of(shape) {
                media(&held, id, on(shape), &mut found);
            }
            if matches!(shape.kind, ShapeKind::Picture { .. }) && shape.descr.trim().is_empty() && !shape.decorative {
                found.push(
                    Diagnostic::new(
                        "slide.missing-alt-text",
                        Severity::Info,
                        format!("Picture \"{}\" has no alt text for screen readers.", shape.name),
                    )
                    .on(on(shape)),
                );
            }
            // Group children are placed in their group's own coordinates: only top-level shapes
            // are measured against the slide and laid out.
            if depth > 0 || shape.hidden {
                return;
            }
            let Some(ctx) = &ctx else { return };
            let x = shape.xfrm.unwrap_or_else(|| resolve::xfrm(ctx, shape));
            if x.w > 0.0 && x.h > 0.0 && (x.x + x.w <= 0.0 || x.y + x.h <= 0.0 || x.x >= slide_w || x.y >= slide_h) {
                found.push(
                    Diagnostic::new(
                        "slide.off-slide",
                        Severity::Warning,
                        format!("\"{}\" lies entirely outside the slide, so the show never displays it.", shape.name),
                    )
                    .on(on(shape))
                    .fix(
                        "shape.move",
                        Some(json!({
                            "ids": [shape.id.0],
                            "onSlide": number,
                            "x": x.x.clamp(0.0, (slide_w - x.w).max(0.0)),
                            "y": x.y.clamp(0.0, (slide_h - x.h).max(0.0)),
                        })),
                    ),
                );
            }
            if let Some(ph) = &shape.ph
                && !ph.kind.is_footer_kind()
                && shape.text.as_ref().is_none_or(|body| body.text().trim().is_empty())
                && matches!(shape.kind, ShapeKind::Shape)
            {
                found.push(
                    Diagnostic::new(
                        "slide.empty-placeholder",
                        Severity::Info,
                        format!("Placeholder \"{}\" is empty: the editor shows its prompt, the show shows nothing.", shape.name),
                    )
                    .on(on(shape))
                    .fix("edit.delete", Some(json!({ "ids": [shape.id.0], "onSlide": number }))),
                );
            }
            if overflows(ctx, shape, x) {
                found.push(
                    Diagnostic::new("slide.text-overflow", Severity::Warning, format!("The text of \"{}\" does not fit its box.", shape.name))
                        .on(on(shape))
                        .fix("format.autofit", Some(json!({ "ids": [shape.id.0], "onSlide": number, "mode": "shrink" }))),
                );
            }
        });
        rendered(pres, index, number, &object, &mut found);
    }
    let db = kition_slide_fonts::FontDb::global();
    for family in requested_fonts(pres).into_iter().filter(|family| !db.has_family(family)) {
        let used = db.face(&family, "Regular").family.clone();
        // A metric-compatible stand-in (Carlito for Calibri, Arimo for Arial…) keeps every line
        // where it was: a note. Any other is a warning: the text reflows here and differently in
        // each app without the font (an agent took the note for nothing to do, khp-bench S03).
        let compatible = METRIC_COMPATIBLE.iter().any(|(asked, stand_in)| family.eq_ignore_ascii_case(asked) && used.eq_ignore_ascii_case(stand_in));
        let (severity, advice) = if compatible {
            (Severity::Info, "with the same metrics, so lines stay where they are")
        } else {
            (
                Severity::Warning,
                "so its lines may wrap differently, and apps without it substitute their own font; use an installed family for the same look everywhere",
            )
        };
        found.push(Diagnostic::new(
            "slide.font-substituted",
            severity,
            format!("The font \"{family}\" is not available here; its text is drawn with \"{used}\", {advice}."),
        ));
    }
    found.sort_by_key(|diagnostic| diagnostic.severity);
    found
}

/// What drawing slide `index` as the show does met: a renderer failure, or shapes drawn without
/// their effects because those reach past what the rasterizer can draw.
fn rendered(pres: &SlideFile, index: usize, number: u32, object: &dyn Fn(ShapeId) -> Option<String>, found: &mut Vec<Diagnostic>) {
    let probe = kition_slide_render::probe_slide(pres, index, &kition_slide_render::RenderOpts::default());
    if probe.panicked {
        found.push(
            Diagnostic::new("slide.render-failed", Severity::Error, format!("Slide {number} could not be drawn: the show displays it blank."))
                .on(Target { slide: Some(number), ..Target::default() }),
        );
    }
    let Some(slide) = pres.slides.get(index) else { return };
    for id in probe.effects_skipped {
        let mut name = None;
        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
            if shape.id == id {
                name = Some(shape.name.clone());
            }
        });
        let (target, what) = match name {
            Some(name) => (
                Target { slide: Some(number), path: Some(format!("shape:{}", id.0)), object_id: object(id), ..Target::default() },
                format!("\"{name}\""),
            ),
            None => (Target { slide: Some(number), ..Target::default() }, "A shape of the slide's layout or master".to_owned()),
        };
        found.push(
            Diagnostic::new(
                "slide.effects-dropped",
                Severity::Warning,
                format!("{what} has shadow, glow, soft edge or reflection effects too large to draw; it is shown without them."),
            )
            .on(target),
        );
    }
}

/// The media a shape shows itself (its picture, video, audio, poster, preview, or picture fill).
fn media_of(shape: &Shape) -> Vec<MediaId> {
    let mut ids = Vec::new();
    match &shape.kind {
        ShapeKind::Picture { fill } => ids.push(fill.media),
        ShapeKind::Media(clip) => ids.extend([Some(clip.media), clip.poster].into_iter().flatten()),
        ShapeKind::Opaque { preview: Some(preview), .. } => ids.push(*preview),
        _ => {}
    }
    if let Some(Fill::Picture(picture)) = &shape.fill {
        ids.push(picture.media);
    }
    ids
}

/// A missing media item, or a picture whose bytes are not a format the engine draws.
fn media(held: &BTreeMap<MediaId, &kition_slide_model::MediaItem>, id: MediaId, target: Target, found: &mut Vec<Diagnostic>) {
    match held.get(&id) {
        None => found.push(
            Diagnostic::new("slide.missing-media", Severity::Error, "The slide refers to a picture or media file it does not hold.").on(target),
        ),
        Some(item) if item.link.is_none() && item.data.is_empty() => {
            found.push(
                Diagnostic::new("slide.missing-media", Severity::Error, format!("\"{}\" is empty: the slide holds no bytes for it.", item.name))
                    .on(target),
            );
        }
        Some(item) if item.link.is_none() && item.content_type.starts_with("image/") && !picture_bytes(&item.data) => {
            found.push(
                Diagnostic::new(
                    "slide.unreadable-picture",
                    Severity::Error,
                    format!("\"{}\" is not a picture format the slide can show.", item.name),
                )
                .on(target),
            );
        }
        Some(_) => {}
    }
}

/// Whether `bytes` start like a picture format the renderer draws (PNG, JPEG, GIF, BMP, TIFF,
/// WebP, WMF, EMF, SVG).
fn picture_bytes(bytes: &[u8]) -> bool {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    let svg = || {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        head.contains("<svg")
    };
    starts(b"\x89PNG\r\n\x1a\n")
        || starts(b"\xff\xd8\xff")
        || starts(b"GIF8")
        || starts(b"BM")
        || starts(b"II*\0")
        || starts(b"MM\0*")
        || (starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
        || starts(b"\xd7\xcd\xc6\x9a")
        || starts(b"\x01\0\x09\0")
        || (starts(b"\x01\0\0\0") && bytes.get(40..44) == Some(b" EMF"))
        || svg()
}

/// Whether the shape's text overflows its box as drawn: text that resizes its shape never does,
/// and shrink-on-overflow text only when it cannot shrink enough.
fn overflows(ctx: &Ctx, shape: &Shape, x: kition_slide_geom::Xfrm) -> bool {
    let Some(body) = &shape.text else { return false };
    if body.text().trim().is_empty() || !matches!(shape.kind, ShapeKind::Shape) {
        return false;
    }
    if resolve::body(ctx, shape).autofit == Some(AutoFit::Shape) {
        return false;
    }
    let geo = kition_slide_render::shape_geometry(shape, x.w, x.h);
    let opts = kition_slide_text::Opts {
        rect: geo.text_rect,
        fields: &kition_slide_text::NoFields,
        prompt_color: None,
        no_shrink: false,
        empty_bullets: false,
    };
    kition_slide_text::layout(ctx, shape, body, &opts).overflow
}

/// Families and the bundled stand-ins drawn with their metrics (Kition Fonts, the Chrome OS core
/// fonts): text set in either breaks its lines in the same places.
const METRIC_COMPATIBLE: &[(&str, &str)] = &[
    ("Calibri", "Carlito"),
    ("Cambria", "Caladea"),
    ("Arial", "Arimo"),
    ("Helvetica", "Arimo"),
    ("Times New Roman", "Tinos"),
    ("Times", "Tinos"),
    ("Courier New", "Cousine"),
];

/// The font families the slide's themes and text ask for (theme slots resolved to their families).
fn requested_fonts(pres: &SlideFile) -> BTreeSet<String> {
    let mut families = BTreeSet::new();
    let mut theme = BTreeMap::new();
    for master in &pres.masters {
        let fonts = &master.theme.fonts;
        for (slot, family) in [("+mj-lt", &fonts.major.latin), ("+mn-lt", &fonts.minor.latin)] {
            if !family.is_empty() {
                families.insert(family.clone());
                theme.entry(slot).or_insert_with(|| family.clone());
            }
        }
    }
    for slide in &pres.slides {
        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
            for run in shape.text.iter().flat_map(|body| &body.paragraphs).flat_map(|paragraph| &paragraph.runs) {
                if let Some(name) = &run.props.font {
                    let family = theme.get(name.as_str()).cloned().unwrap_or_else(|| name.clone());
                    if !family.is_empty() && !family.starts_with('+') {
                        families.insert(family);
                    }
                }
            }
        });
    }
    families
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kition_slide_geom::Xfrm;
    use kition_slide_model::style::{PictureFill, PictureMode};
    use kition_slide_model::{MediaItem, PhType, Placeholder, ShapeId};

    use super::*;

    fn check_slide_file(pres: &SlideFile) -> Vec<Diagnostic> {
        check(pres)
    }

    fn codes(found: &[Diagnostic]) -> Vec<&str> {
        found.iter().map(|diagnostic| diagnostic.code.as_str()).collect()
    }

    fn picture(id: u32, media: u32) -> Shape {
        Shape {
            id: ShapeId(id),
            name: format!("Picture {id}"),
            xfrm: Some(Xfrm { x: 10.0, y: 10.0, w: 100.0, h: 100.0, ..Xfrm::default() }),
            kind: ShapeKind::Picture {
                fill: PictureFill {
                    media: MediaId(media),
                    crop: [0.0; 4],
                    mode: PictureMode::default(),
                    alpha: None,
                    adjust: Default::default(),
                    ..Default::default()
                },
            },
            ..Shape::default()
        }
    }

    fn slide_file(shapes: Vec<Shape>, media: Vec<MediaItem>) -> SlideFile {
        let mut pres = SlideFile::default();
        let mut slide = (*pres.slides.first().cloned().unwrap_or_default()).clone();
        slide.shapes = shapes;
        pres.slides = vec![Arc::new(slide)];
        pres.media = media;
        pres
    }

    #[test]
    fn a_new_slide_file_has_nothing_worse_than_notes() {
        let found = check_slide_file(&SlideFile::default());
        assert!(found.iter().all(|diagnostic| diagnostic.severity == Severity::Info), "{found:?}");
        assert!(kition_kit::diagnostics::check_diagnostics(&found).is_ok());
    }

    #[test]
    fn finds_missing_and_unreadable_pictures_and_missing_alt_text() {
        let png = MediaItem {
            id: MediaId(7),
            name: "ok.png".into(),
            content_type: "image/png".into(),
            data: Arc::new(b"\x89PNG\r\n\x1a\n....".to_vec()),
            link: None,
        };
        let junk = MediaItem {
            id: MediaId(8),
            name: "junk.png".into(),
            content_type: "image/png".into(),
            data: Arc::new(b"not a picture".to_vec()),
            link: None,
        };
        let mut described = picture(3, 7);
        described.descr = "A chart".into();
        let found = check_slide_file(&slide_file(vec![picture(2, 99), described, picture(4, 8)], vec![png, junk]));
        let errors: Vec<_> = found.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect();
        assert_eq!(codes(&found)[..2], ["slide.missing-media", "slide.unreadable-picture"], "errors first: {found:?}");
        assert_eq!(errors[0].target.slide, Some(1));
        assert_eq!(errors[0].target.path.as_deref(), Some("shape:2"));
        let alt: Vec<_> =
            found.iter().filter(|diagnostic| diagnostic.code == "slide.missing-alt-text").map(|diagnostic| diagnostic.target.path.clone()).collect();
        assert_eq!(alt, [Some("shape:2".to_owned()), Some("shape:4".to_owned())]);
        assert!(kition_kit::diagnostics::check_diagnostics(&found).is_ok());
    }

    #[test]
    fn finds_shapes_off_the_slide_and_empty_placeholders() {
        let mut off = picture(5, 7);
        off.descr = "x".into();
        off.xfrm = Some(Xfrm { x: 2000.0, y: 10.0, w: 50.0, h: 50.0, ..Xfrm::default() });
        let empty = Shape {
            id: ShapeId(6),
            name: "Title 1".into(),
            xfrm: Some(Xfrm { x: 10.0, y: 10.0, w: 300.0, h: 60.0, ..Xfrm::default() }),
            ph: Some(Placeholder { kind: PhType::Title, ..Placeholder::default() }),
            ..Shape::default()
        };
        let png = MediaItem {
            id: MediaId(7),
            name: "ok.png".into(),
            content_type: "image/png".into(),
            data: Arc::new(b"\x89PNG\r\n\x1a\n".to_vec()),
            link: None,
        };
        let found = check_slide_file(&slide_file(vec![off, empty], vec![png]));
        assert!(codes(&found).contains(&"slide.off-slide"), "{found:?}");
        let placeholder = found.iter().find(|diagnostic| diagnostic.code == "slide.empty-placeholder").map(|diagnostic| diagnostic.fix.clone());
        assert_eq!(placeholder.flatten().map(|fix| fix.command), Some("edit.delete".to_owned()));
    }

    #[test]
    fn recognises_picture_formats_by_their_bytes() {
        assert!(picture_bytes(b"\xff\xd8\xff\xe0rest"));
        assert!(picture_bytes(b"<?xml version=\"1.0\"?><svg xmlns=\"\"/>"));
        assert!(!picture_bytes(b"PK\x03\x04"));
    }

    #[test]
    fn reports_shapes_drawn_without_effects_too_large_to_draw() {
        use kition_slide_model::Effects;
        use kition_slide_model::style::Glow;
        let huge = Shape {
            id: ShapeId(7),
            name: "Halo".into(),
            xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 100.0)),
            effects: Some(Effects {
                glow: Some(Glow { color: kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::rgb(255, 0, 0)), radius: 1.0e6 }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let small = Shape {
            id: ShapeId(8),
            name: "Ring".into(),
            xfrm: Some(Xfrm::new(400.0, 100.0, 100.0, 100.0)),
            effects: Some(Effects { soft_edge: Some(4.0), ..Default::default() }),
            ..Default::default()
        };
        let found = check_slide_file(&slide_file(vec![huge, small], vec![]));
        let dropped: Vec<_> = found.iter().filter(|diagnostic| diagnostic.code == "slide.effects-dropped").collect();
        assert_eq!(dropped.len(), 1, "{found:?}");
        assert_eq!(dropped[0].target, Target { slide: Some(1), path: Some("shape:7".into()), ..Target::default() });
        assert!(!codes(&found).contains(&"slide.render-failed"));
    }
}
