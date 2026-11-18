//! PPTX import and export: Office Open XML PresentationML / DrawingML, written from the public
//! ECMA-376 standard.
//!
//! [`import`] reads a package leniently (unknown parts are skipped, malformed parts are logged and
//! ignored, every size and depth is bounded) into the Kition Slide model; [`export`] writes a valid
//! package that PowerPoint opens without repair.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod autofit_ext;
mod comments_ext;
mod custgeom;
mod diagram_ids;
mod font_faces_ext;
mod layout_id;
mod maps;
mod member_ids;
mod opc;
mod png;
pub(crate) mod read;
pub mod stored;
mod tables;
mod write;
mod xml;

use kition_slide_model::SlideFile;

/// Extension URI of the PowerPoint 2010 section list in `presentation.xml`.
pub(crate) const SECTION_EXT_URI: &str = "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";

#[derive(Debug, thiserror::Error)]
pub enum PptxError {
    #[error("not a PPTX file: {0}")]
    NotPptx(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("write failed: {0}")]
    Write(String),
    #[error("malformed XML: {0}")]
    Xml(String),
}

/// Does this look like an OOXML package (zip with `[Content_Types].xml`)?
///
/// The part is looked up in the zip's central directory, not in the leading bytes: writers may
/// store it anywhere in the archive (recent PowerPoint builds put it last, megabytes in).
pub fn sniff(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"PK") {
        return false;
    }
    zip::ZipArchive::new(std::io::Cursor::new(bytes)).map(|z| z.index_for_name("[Content_Types].xml").is_some()).unwrap_or(false)
}

/// Read a `.pptx` / `.potx` / `.ppsx` package.
pub fn import(bytes: &[u8]) -> Result<SlideFile, PptxError> {
    read::import(bytes)
}

/// Write a `.pptx` package.
pub fn export(p: &SlideFile) -> Result<Vec<u8>, PptxError> {
    write::export(p)
}

/// The notes master a file written from `p` reads back with when `p` has notes but no notes
/// master of its own (the writer then writes a default one), so a model can hold, before it is
/// saved, what its saved file opens as (task card C8-slide-5). Written and read with one slide
/// of `p` and its masters, without its media; `None` when that fails.
pub fn default_notes_master(p: &SlideFile) -> Option<kition_slide_model::Master> {
    let mut probe = p.clone();
    let mut slide = (**probe.slides.first()?).clone();
    slide.shapes.clear();
    slide.animations.clear();
    slide.timing = None;
    slide.comments.clear();
    slide.notes = kition_slide_model::text::TextBody::from_text("-");
    probe.slides = vec![std::sync::Arc::new(slide)];
    probe.notes_master = None;
    probe.handout_master = None;
    probe.sections.clear();
    probe.custom_shows.clear();
    probe.media.clear();
    probe.embedded_fonts.clear();
    probe.embedded_font_rels.clear();
    let back = import(&export(&probe).ok()?).ok()?;
    back.notes_master.map(|master| (*master).clone())
}

/// Content type for a file name's extension.
pub(crate) fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "avi" => "video/x-msvideo",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "fntdata" => "application/x-fontdata",
        _ => "application/octet-stream",
    }
}
