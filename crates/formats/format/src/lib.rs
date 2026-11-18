//! The native `.kslide` format: a zip holding `slide.json` (the model, minus media
//! bytes), `media/<id>` files with the embedded media, and `mimetype`. Readers ignore unknown
//! fields, so newer files open in older builds with what those builds understand.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use kition_slide_model::text::{Paragraph, TextBody};
use kition_slide_model::{LayoutType, SlideFile, defaults};

pub const MIMETYPE: &str = "application/vnd.kition.slide+zip";
pub const EXTENSION: &str = "kslide";
/// Format version written into the manifest.
pub const VERSION: u32 = 2;
/// The format id, model entry, and model field version 1 files used; they still open.
const V1_FORMAT: &str = "kitionslide";
const V1_ENTRY: &str = "presentation.json";
/// Largest entry we will inflate (guards against zip bombs).
const MAX_ENTRY: u64 = 2 << 30;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not a Kition Slide file: {0}")]
    NotOurs(String),
    #[error("damaged file: {0}")]
    Damaged(String),
    #[error("write failed: {0}")]
    Write(String),
}

pub type Result<T> = std::result::Result<T, FormatError>;

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    generator: String,
    #[serde(alias = "presentation")]
    slide: SlideFile,
}

/// Serialize a slide.
pub fn save(p: &SlideFile) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let w = |e: zip::result::ZipError| FormatError::Write(e.to_string());
        z.start_file("mimetype", stored).map_err(w)?;
        z.write_all(MIMETYPE.as_bytes()).map_err(|e| FormatError::Write(e.to_string()))?;
        let m = Manifest {
            format: "kslide".into(),
            version: VERSION,
            generator: format!("Kition Slide {}", env!("CARGO_PKG_VERSION")),
            slide: p.clone(),
        };
        let json = serde_json::to_vec(&m).map_err(|e| FormatError::Write(e.to_string()))?;
        z.start_file("slide.json", deflate).map_err(w)?;
        z.write_all(&json).map_err(|e| FormatError::Write(e.to_string()))?;
        for item in &p.media {
            if item.link.is_some() && item.data.is_empty() {
                continue;
            }
            // Already-compressed media is stored, not deflated again.
            let opts = if is_compressed(&item.content_type) { stored } else { deflate };
            z.start_file(format!("media/{}", item.id.0), opts).map_err(w)?;
            z.write_all(&item.data).map_err(|e| FormatError::Write(e.to_string()))?;
        }
        z.finish().map_err(w)?;
    }
    Ok(buf.into_inner())
}

fn is_compressed(ct: &str) -> bool {
    ct.starts_with("image/jpeg")
        || ct.starts_with("image/png")
        || ct.starts_with("video/")
        || ct.starts_with("audio/mpeg")
        || ct.starts_with("audio/mp4")
        || ct.contains("zip")
}

/// Does this look like a `.kslide` file?
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK") && bytes.windows(MIMETYPE.len()).take(200).any(|w| w == MIMETYPE.as_bytes())
}

fn read_entry(z: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let f = z.by_name(name).map_err(|e| FormatError::Damaged(format!("{name}: {e}")))?;
    if f.size() > MAX_ENTRY {
        return Err(FormatError::Damaged(format!("{name} is too large")));
    }
    let mut out = Vec::with_capacity(f.size().min(64 << 20) as usize);
    f.take(MAX_ENTRY).read_to_end(&mut out).map_err(|e| FormatError::Damaged(format!("{name}: {e}")))?;
    Ok(out)
}

/// Read a slide.
pub fn load(bytes: &[u8]) -> Result<SlideFile> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| FormatError::NotOurs(e.to_string()))?;
    let json =
        read_entry(&mut z, "slide.json").or_else(|_| read_entry(&mut z, V1_ENTRY)).map_err(|_| FormatError::NotOurs("no slide.json".into()))?;
    let m: Manifest = serde_json::from_slice(&json).map_err(|e| FormatError::Damaged(e.to_string()))?;
    if m.format != "kslide" && m.format != V1_FORMAT {
        return Err(FormatError::NotOurs(m.format));
    }
    let mut p = m.slide;
    for item in &mut p.media {
        if let Ok(data) = read_entry(&mut z, &format!("media/{}", item.id.0)) {
            item.data = Arc::new(data);
        }
    }
    repair(&mut p);
    Ok(p)
}

/// Make a loaded slide file safe to edit: a master exists, every slide's layout exists,
/// ids are unique, tables are rectangular.
pub fn repair(p: &mut SlideFile) {
    if p.masters.is_empty() {
        let fresh = defaults::blank_slide_file(p.slide_size, Default::default(), false);
        p.masters = fresh.masters;
    }
    p.fix_next_id();
    let first_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for i in 0..p.slides.len() {
        let needs_layout = p.slides.get(i).is_some_and(|s| p.layout(s.layout).is_none());
        let dup = p.slides.get(i).is_some_and(|s| !seen.insert(s.id));
        if needs_layout || dup {
            let new_id = if dup { Some(kition_slide_model::SlideId(p.alloc_id())) } else { None };
            if let Some(s) = p.slides.get_mut(i) {
                let s = Arc::make_mut(s);
                if needs_layout {
                    s.layout = first_layout;
                }
                if let Some(id) = new_id {
                    s.id = id;
                }
            }
        }
    }
    if !(p.slide_size.width.is_finite() && p.slide_size.width >= 1.0 && p.slide_size.height.is_finite() && p.slide_size.height >= 1.0) {
        p.slide_size = defaults::WIDE;
    }
    p.slide_size.width = p.slide_size.width.min(4000.0);
    p.slide_size.height = p.slide_size.height.min(4000.0);
}

/// Outline text: one slide per unindented line; tab-indented lines become body bullets at their
/// level (Insert ▸ Slides from Outline, View ▸ Outline copy).
pub fn outline_to_slides(p: &mut SlideFile, text: &str) -> usize {
    let Some(lay) = defaults::layout_of_kind(p, LayoutType::TitleAndContent) else { return 0 };
    let mut made = 0;
    let mut cur: Option<kition_slide_model::Slide> = None;
    let flush = |p: &mut SlideFile, s: Option<kition_slide_model::Slide>, made: &mut usize| {
        if let Some(s) = s {
            p.slides.push(Arc::new(s));
            *made += 1;
        }
    };
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let tabs = raw.chars().take_while(|c| *c == '\t').count();
        let spaces = raw.chars().take_while(|c| *c == ' ').count() / 2;
        let level = tabs.max(spaces);
        let line = raw.trim().trim_start_matches(['-', '*', '•']).trim();
        if level == 0 {
            flush(p, cur.take(), &mut made);
            let mut s = defaults::new_slide(p, lay);
            if let Some(t) = s.shapes.first_mut() {
                t.text = Some(TextBody::from_text(line));
            }
            cur = Some(s);
        } else if let Some(s) = cur.as_mut()
            && let Some(body) = s.shapes.get_mut(1)
        {
            let tb = body.text.get_or_insert_with(TextBody::default);
            let mut para = Paragraph::new(line);
            para.level = (level - 1).min(8) as u8;
            tb.paragraphs.push(para);
        }
    }
    flush(p, cur, &mut made);
    made
}

/// The slide as outline text (titles unindented, body paragraphs tab-indented by level).
pub fn slides_to_outline(p: &SlideFile) -> String {
    let mut out = String::new();
    for s in &p.slides {
        out.push_str(&s.title());
        out.push('\n');
        for sh in &s.shapes {
            if sh.ph_type().is_some_and(|k| k.is_title()) {
                continue;
            }
            if let Some(t) = &sh.text {
                for para in &t.paragraphs {
                    if para.is_empty() {
                        continue;
                    }
                    for _ in 0..=para.level {
                        out.push('\t');
                    }
                    out.push_str(&para.text().replace('\u{b}', " "));
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_media() {
        let mut p = SlideFile::default();
        let id = p.add_media("a.png", "image/png", vec![1, 2, 3, 4]);
        let bytes = save(&p).unwrap();
        assert!(sniff(&bytes));
        let q = load(&bytes).unwrap();
        assert_eq!(q.slides.len(), p.slides.len());
        assert_eq!(q.media(id).map(|m| m.data.as_slice()), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(serde_json::to_value(&q.slides).unwrap(), serde_json::to_value(&p.slides).unwrap());
    }

    #[test]
    fn version_1_files_still_open() {
        let mut p = SlideFile::default();
        let id = p.add_media("a.png", "image/png", vec![1, 2, 3, 4]);
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("mimetype", opts).unwrap();
            z.write_all(MIMETYPE.as_bytes()).unwrap();
            let manifest = serde_json::json!({ "format": V1_FORMAT, "version": 1, "generator": "Kition Slide", "presentation": p });
            z.start_file(V1_ENTRY, opts).unwrap();
            z.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
            z.start_file(format!("media/{}", id.0), opts).unwrap();
            z.write_all(&[1, 2, 3, 4]).unwrap();
            z.finish().unwrap();
        }
        let bytes = buf.into_inner();
        assert!(sniff(&bytes));
        let q = load(&bytes).unwrap();
        assert_eq!(q.slides.len(), p.slides.len());
        assert_eq!(q.media(id).map(|m| m.data.as_slice()), Some(&[1u8, 2, 3, 4][..]));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(load(b"").is_err());
        assert!(load(b"PK\x03\x04garbage").is_err());
        assert!(load(&[0u8; 1000]).is_err());
        let mut b = save(&SlideFile::default()).unwrap();
        let n = b.len();
        b.truncate(n / 2);
        assert!(load(&b).is_err());
    }

    #[test]
    fn repair_fixes_missing_layout_and_size() {
        let mut p = SlideFile::default();
        Arc::make_mut(&mut p.slides[0]).layout = kition_slide_model::LayoutId(99999);
        p.slide_size.width = f64::NAN;
        repair(&mut p);
        assert!(p.validate().is_empty());
        assert_eq!(p.slide_size.width, 960.0);
    }

    #[test]
    fn outline_roundtrip() {
        let mut p = defaults::blank_slide_file(defaults::WIDE, Default::default(), false);
        let n = outline_to_slides(&mut p, "Intro\n\tPoint one\n\t\tSub point\nSecond\n\tMore");
        assert_eq!(n, 2);
        let o = slides_to_outline(&p);
        assert!(o.contains("Intro\n\tPoint one\n\t\tSub point\nSecond"), "{o}");
    }

    #[test]
    fn kept_text_3d_properties_survive_native_save_and_load() {
        let mut p = SlideFile::default();
        let scene = r#"<a:scene3d xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:camera prst="obliqueBottomRight"/><a:lightRig rig="flat" dir="t"/></a:scene3d>"#;
        for text in [r#"<a:sp3d extrusionH="254000"/>"#, r#"<a:flatTx z="0"/>"#, "<a:sp3d/>"] {
            let body = &mut Arc::make_mut(&mut p.slides[0]).shapes[0].text.as_mut().unwrap().body;
            body.scene3d_xml = Some(scene.into());
            body.text3d_xml = Some(text.into());
            let q = load(&save(&p).unwrap()).unwrap();
            assert_eq!(q.slides[0].shapes[0].text.as_ref().unwrap().body, p.slides[0].shapes[0].text.as_ref().unwrap().body);
        }
        let value = serde_json::to_value(kition_slide_model::text::BodyProps::default()).unwrap();
        assert!(value.get("scene3dXml").is_none());
        assert!(value.get("text3dXml").is_none());
    }
}
