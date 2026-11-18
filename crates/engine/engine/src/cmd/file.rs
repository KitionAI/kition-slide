//! File menu: new, open, save, export, close, properties.

use kition_slide_model::{SlideFile, defaults};
use serde_json::{Value, json};

use super::*;
use crate::{DocState, EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "file.new", "New slide", ["File"], Some("Cmd+N"), "{theme?: name, size?: [w,h] pt, blank?: bool}", always, new),
        cmd!(noundo "file.open", "Open…", ["File"], Some("Cmd+O"), "{path}", always, open),
        cmd!(noundo "file.openBytes", "Open Data", [], None, "{name, data: base64}", always, open_bytes),
        cmd!(noundo "file.save", "Save", ["File"], Some("Cmd+S"), "{path?, format?: kslide|pptx}", has_doc, save),
        cmd!(noundo "file.saveAs", "Save As…", ["File"], Some("Cmd+Shift+S"), "{path, format?}", has_doc, save_as),
        cmd!(noundo "file.saveTemplate", "Save as Template…", ["File"], None, "{path, format?}", has_doc, save_as),
        cmd!(query "file.saveBytes", "Save to Data", [], None, "{format?: kslide|pptx} → {data: base64}", has_doc, save_bytes_cmd),
        cmd!(noundo "file.export", "Export…", ["File"], None, "{path, format: png|jpeg|pptx|kslide|outline|pdf, slide?: index, all?: bool, scale?: px per pt; pdf: layout?: slides|notes|handouts, perPage?: 1|2|3|4|6|9, dpi?, slides?: [index], includeHidden?, textLayer?, frame?}", has_doc, export),
        cmd!(query "file.exportBytes", "Export to Data", [], None, "{format?: pdf|png|jpeg|jpg (default pdf), slide?: index (png/jpeg; default the current one), all?: bool (png/jpeg: every slide), scale?: px per pt (png/jpeg, default 2); pdf: layout?: slides|notes|handouts, perPage?: 1|2|3|4|6|9, dpi?, slides?: [index], includeHidden?, textLayer?, frame?} → {data: base64 (the first file), format, bytes, files?: [{slide, data}] (png/jpeg with all)}", has_doc, export_bytes),
        cmd!(query "file.render", "Render Slide", [], None, "{target?: {slide?, slideId?}, slide?: index, scale?, maxSize?, edit?: bool} → {png: base64, width, height, slide, slideId, scale}", has_doc, render),
        cmd!(noundo "file.close", "Close", ["File"], Some("Cmd+W"), "{}", has_doc, close),
        cmd!(
            "file.properties",
            "Properties",
            ["File"],
            None,
            "{title?, subject?, author?, keywords?, comments?, category?, company?} → properties",
            has_doc,
            properties
        ),
        cmd!(noundo "file.recovery.save", "Save AutoRecover Information", [], None, "{} → {written}", has_doc, recovery_save),
        cmd!(query "file.recovery.list", "Recovered slide files", [], None, "{} → [{uid, path, title, saved}]", always, recovery_list),
        cmd!(noundo "file.recovery.open", "Open Recovered slide files", [], None, "{} → {opened: [document index]}", always, recovery_open),
        cmd!(noundo "file.recovery.discard", "Discard Recovered slide files", [], None, "{uid?: one entry, else all}", always, recovery_discard),
        cmd!(noundo "file.revert", "Revert", [], None, "{}", has_doc, revert),
        cmd!(noundo "window.next", "Next Window", ["Window"], Some("Cmd+`"), "{index?}", has_doc, next_window),
    ]
}

fn theme_named(name: &str) -> Option<kition_slide_model::Theme> {
    kition_slide_model::theme::builtin_themes().into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

fn new(s: &mut Session, p: &Value) -> Result<Value> {
    let theme = str_param(p, "theme").and_then(theme_named).unwrap_or_default();
    let size = p
        .get("size")
        .and_then(Value::as_array)
        .and_then(|a| Some(kition_slide_geom::Size::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?)))
        .filter(|s| s.width >= 1.0 && s.height >= 1.0 && s.width < 5000.0 && s.height < 5000.0)
        .unwrap_or(defaults::WIDE);
    let mut doc = defaults::blank_slide_file(size, theme, !bool_or(p, "blank", false));
    as_stored(&mut doc);
    let title = s.next_untitled();
    let i = s.add_document(DocState::new(doc, None, title));
    Ok(json!({"document": i}))
}

/// Recognise and read slide bytes (native, PPTX).
pub fn open_slide_file(name: &str, bytes: &[u8]) -> Result<SlideFile> {
    if kition_slide_format::sniff(bytes) {
        let mut p = kition_slide_format::load(bytes).map_err(|e| EngineError::Other(e.to_string()))?;
        as_stored(&mut p);
        return Ok(p);
    }
    if kition_slide_pptx::sniff(bytes) {
        let mut p = kition_slide_pptx::import(bytes).map_err(|e| EngineError::Other(e.to_string()))?;
        kition_slide_format::repair(&mut p);
        as_stored(&mut p);
        return Ok(p);
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".txt") || lower.ends_with(".md") {
        let text = String::from_utf8_lossy(bytes);
        let mut p = defaults::blank_slide_file(defaults::WIDE, Default::default(), false);
        kition_slide_format::outline_to_slides(&mut p, &text);
        if p.slides.is_empty() {
            p = defaults::new_slide_file(None);
        }
        return Ok(p);
    }
    Err(EngineError::Other(format!("{name}: not a slide file Kition Slide can read")))
}

/// An opened deck as its file stores it (`stored`): written and read again, it reads the same.
/// The receipts of its shrink-to-fit results, which no file stores, are recomputed.
pub(crate) fn as_stored(p: &mut SlideFile) {
    crate::stored::settle(p, None);
    crate::autofit::receipts(p);
}

pub fn add_opened(s: &mut Session, name: &str, path: Option<String>, p: SlideFile) -> usize {
    let title = std::path::Path::new(name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    s.add_document(DocState::new(p, path, title))
}

#[cfg(not(target_arch = "wasm32"))]
fn open(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.open", "missing `path`"))?;
    crate::disk::need(s, "file.open")?;
    let bytes = std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let doc = open_slide_file(path, &bytes)?;
    let native = kition_slide_format::sniff(&bytes) || kition_slide_pptx::sniff(&bytes);
    let i = add_opened(s, path, native.then(|| path.to_string()), doc);
    Ok(json!({"document": i, "slides": s.doc()?.doc.slides.len()}))
}
#[cfg(target_arch = "wasm32")]
fn open(_s: &mut Session, _p: &Value) -> Result<Value> {
    Err(EngineError::Other("use file.openBytes in the browser".into()))
}

fn open_bytes(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("Untitled slide");
    let data = str_param(p, "data").and_then(base64_decode).ok_or_else(|| bad("file.openBytes", "missing or invalid base64 `data`"))?;
    let doc = open_slide_file(name, &data)?;
    let i = add_opened(s, name, None, doc);
    Ok(json!({"document": i}))
}

/// Encode the slide in `format` (`kslide`, `pptx`, `outline`).
pub fn save_bytes(doc: &SlideFile, format: &str) -> Result<Vec<u8>> {
    match format {
        "pptx" | "potx" | "ppsx" => {
            // A commenter can receive concurrent text without permission to persist its fit.
            // Materialize derived scales only in this export projection, never in the replica.
            let mut exported = doc.clone();
            crate::autofit::repair(&mut exported);
            exported.keep_own_slide_ids();
            kition_slide_pptx::export(&exported).map_err(|e| EngineError::Other(e.to_string()))
        }
        "outline" | "txt" => Ok(kition_slide_format::slides_to_outline(doc).into_bytes()),
        "pdf" => kition_slide_pdf::export(doc, &Default::default()).map_err(|e| EngineError::Other(e.to_string())),
        _ => kition_slide_format::save(doc).map_err(|e| EngineError::Other(e.to_string())),
    }
}

/// PDF options from command params: `{layout: slides|notes|handouts, perPage, dpi, slides: [i],
/// includeHidden, textLayer, frame}`.
pub fn pdf_options(p: &Value) -> kition_slide_pdf::PdfOptions {
    let mut o = kition_slide_pdf::PdfOptions { layout: kition_slide_pdf::PageLayout::Slides, ..Default::default() };
    o.layout = match str_param(p, "layout").unwrap_or("slides") {
        "notes" => kition_slide_pdf::PageLayout::Notes,
        "handouts" => kition_slide_pdf::PageLayout::Handouts { per_page: usize_param(p, "perPage").unwrap_or(6).min(9) as u8 },
        _ => kition_slide_pdf::PageLayout::Slides,
    };
    o.dpi = f64_or(p, "dpi", o.dpi).clamp(36.0, 600.0);
    o.slides = p.get("slides").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_u64().map(|x| x as usize)).collect());
    o.include_hidden = bool_or(p, "includeHidden", false);
    o.text_layer = bool_or(p, "textLayer", true);
    o.frame_slides = bool_or(p, "frame", true);
    o
}

/// PDF bytes for `doc` with [`pdf_options`] from `p`.
pub fn pdf_bytes(doc: &SlideFile, p: &Value) -> Result<Vec<u8>> {
    kition_slide_pdf::export(doc, &pdf_options(p)).map_err(|e| EngineError::Other(e.to_string()))
}

pub fn format_for_path(path: &str) -> &'static str {
    let l = path.to_ascii_lowercase();
    if l.ends_with(".pptx") || l.ends_with(".potx") || l.ends_with(".ppsx") {
        "pptx"
    } else if l.ends_with(".png") {
        "png"
    } else if l.ends_with(".jpg") || l.ends_with(".jpeg") {
        "jpeg"
    } else if l.ends_with(".pdf") {
        "pdf"
    } else if l.ends_with(".txt") || l.ends_with(".md") {
        "outline"
    } else {
        "kslide"
    }
}

fn mark_saved(s: &mut Session, path: Option<String>) -> Result<()> {
    let st = s.doc_mut()?;
    st.saved_doc = st.doc.clone();
    if path.is_some() {
        st.path = path;
    }
    st.revision += 1;
    let uid = st.uid;
    if let Some(dir) = &s.recovery_dir {
        crate::recovery::discard(dir, uid);
    }
    Ok(())
}

/// Writes `bytes` to `path` for `command`; never when a host owns every file (`crate::disk`).
#[cfg(not(target_arch = "wasm32"))]
fn write_file(s: &Session, command: &str, path: &str, bytes: &[u8]) -> Result<()> {
    crate::disk::need(s, command)?;
    // Write to a temporary sibling then rename, so a failed save never truncates the old file.
    let tmp = format!("{path}.saving");
    std::fs::write(&tmp, bytes).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}
#[cfg(target_arch = "wasm32")]
fn write_file(_s: &Session, _command: &str, _path: &str, _bytes: &[u8]) -> Result<()> {
    Err(EngineError::Other("saving to a path is not available in the browser".into()))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").map(String::from).or_else(|| s.active().and_then(|d| d.path.clone()));
    let Some(path) = path else {
        s.ui_requests.push(crate::UiRequest::PickFile { purpose: "saveAs".into() });
        return Ok(json!({"needsPath": true}));
    };
    let format = str_param(p, "format").unwrap_or(format_for_path(&path));
    let bytes = save_bytes(&s.doc()?.doc, format)?;
    write_file(s, "file.save", &path, &bytes)?;
    mark_saved(s, Some(path.clone()))?;
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
    if str_param(p, "path").is_none() {
        s.ui_requests.push(crate::UiRequest::PickFile { purpose: "saveAs".into() });
        return Ok(json!({"needsPath": true}));
    }
    save(s, p)
}

fn save_bytes_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let format = str_param(p, "format").unwrap_or("kslide");
    let bytes = save_bytes(&s.doc()?.doc, format)?;
    Ok(json!({"data": base64_encode(&bytes), "bytes": bytes.len()}))
}

/// The date `file.render` shows in date fields: fixed, so a render answers the same bytes whenever
/// it runs (2000-01-01).
pub const RENDER_DATE: (i32, u32, u32) = (2000, 1, 1);
/// The largest `scale` `file.render` takes, in pixels per point.
pub const RENDER_MAX_SCALE: f64 = 16.0;
/// The sides `maxSize` may cap a render at, in pixels.
pub const RENDER_MAX_SIZE: std::ops::RangeInclusive<u64> = 16..=8192;

/// The slide `file.render` draws, by index from 0: `target.slide` (from 1) or `target.slideId`,
/// else `slide` (from 0), else the first slide; never the current one, so the picture depends on
/// the call alone.
fn render_index(doc: &SlideFile, p: &Value) -> Result<usize> {
    const C: &str = "file.render";
    let count = doc.slides.len();
    if count == 0 {
        return Err(bad(C, "the deck has no slides to draw"));
    }
    let target = p.get("target").filter(|t| !t.is_null());
    let target = match target {
        None => None,
        Some(Value::Object(view)) => {
            if let Some(other) = view.keys().find(|key| !matches!(key.as_str(), "slide" | "slideId")) {
                return Err(bad(C, format!("`target.{other}` is not a view it draws: `target` takes `slide` (from 1) or `slideId`")));
            }
            if view.len() > 1 {
                return Err(bad(C, "`target` names the slide twice: give `slide` (from 1) or `slideId`"));
            }
            view.iter().next().map(|(key, value)| (key.clone(), value.clone()))
        }
        Some(other) => return Err(bad(C, format!("`target` is {other}: an object, `{{slide: n}}` (from 1) or `{{slideId: id}}`"))),
    };
    if target.is_some() && p.get("slide").is_some_and(|v| !v.is_null()) {
        return Err(bad(C, "the slide is named twice: give `target` or `slide`, not both"));
    }
    let numbered = |n: Option<u64>, shown: &Value| {
        n.and_then(|n| usize::try_from(n).ok()).filter(|n| (1..=count).contains(n)).map(|n| n - 1).ok_or_else(|| {
            bad(C, format!("`target.slide` is {shown}: the deck has {count} slide{}, counted from 1", if count == 1 { "" } else { "s" }))
        })
    };
    match target {
        Some((key, value)) if key == "slide" => numbered(value.as_u64(), &value),
        Some((_, value)) => value
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .and_then(|id| doc.slide_index(kition_slide_model::SlideId(id)))
            .ok_or_else(|| bad(C, format!("no slide has id {value}: slide ids are the `id`s slide_inspect lists for the slides"))),
        None => match p.get("slide").filter(|v| !v.is_null()) {
            None => Ok(0),
            Some(v) => v
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| *i < count)
                .ok_or_else(|| bad(C, format!("`slide` is {v}: an index from 0 to {}", count - 1))),
        },
    }
}

/// One slide as a PNG, a pure function of the slide file and the call: the slide `render_index`
/// picks, at `scale` pixels per point (default 1, at most [`RENDER_MAX_SCALE`]), brought down so
/// neither side exceeds `maxSize` pixels, with date fields showing [`RENDER_DATE`]. The current
/// slide, the selection and the clock change nothing in it.
fn render(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "file.render";
    let st = s.doc()?;
    let doc = &st.doc;
    let index = render_index(doc, p)?;
    let mut scale = match p.get("scale").filter(|v| !v.is_null()) {
        None => 1.0,
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite() && *x > 0.0 && *x <= RENDER_MAX_SCALE)
            .ok_or_else(|| bad(C, format!("`scale` is {v}: pixels per point, more than 0 and at most {RENDER_MAX_SCALE}")))?,
    };
    if let Some(v) = p.get("maxSize").filter(|v| !v.is_null()) {
        let max = v.as_u64().filter(|m| RENDER_MAX_SIZE.contains(m)).ok_or_else(|| {
            bad(C, format!("`maxSize` is {v}: the longest side in pixels, {} to {}", RENDER_MAX_SIZE.start(), RENDER_MAX_SIZE.end()))
        })?;
        let longest = doc.slide_size.width.max(doc.slide_size.height).max(1.0);
        scale = scale.min(max as f64 / longest);
    }
    let (y, m, d) = RENDER_DATE;
    let img = kition_slide_render::render_slide(
        doc,
        index,
        &kition_slide_render::RenderOpts {
            scale,
            edit: bool_or(p, "edit", false),
            now: Some(kition_slide_text::datetime::DateTime::new(y, m, d, 0, 0, 0)),
            ..Default::default()
        },
    );
    let slide_id = doc.slides.get(index).map(|slide| slide.id);
    Ok(
        json!({"png": base64_encode(&img.to_png()), "width": img.width, "height": img.height, "slide": index + 1, "slideId": slide_id, "scale": scale}),
    )
}

/// The slides `p` picks as pictures in `format` (png, jpeg or jpg): slide `slide`, every slide with
/// `all`, else the current one, at `scale` pixels per point (default 2). Answers each slide's index
/// and bytes.
fn slide_pictures(st: &DocState, p: &Value, format: &str, command: &str) -> Result<Vec<(usize, Vec<u8>)>> {
    let scale = f64_or(p, "scale", 2.0);
    let slides: Vec<usize> = match usize_param(p, "slide") {
        Some(i) => vec![i],
        None if bool_or(p, "all", false) => (0..st.doc.slides.len()).collect(),
        None => vec![st.selection.slide],
    };
    slides
        .into_iter()
        .map(|i| {
            if i >= st.doc.slides.len() {
                return Err(bad(command, format!("no slide {i}")));
            }
            let img = kition_slide_render::render_slide(
                &st.doc,
                i,
                &kition_slide_render::RenderOpts { scale: scale.clamp(0.01, 16.0), ..Default::default() },
            );
            Ok((i, if format == "png" { img.to_png() } else { img.to_jpeg(92) }))
        })
        .collect()
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.export", "missing `path`"))?.to_string();
    let format = str_param(p, "format").unwrap_or(format_for_path(&path)).to_string();
    let st = s.doc()?;
    match format.as_str() {
        "png" | "jpeg" | "jpg" => {
            let pictures = slide_pictures(st, p, &format, "file.export")?;
            let mut written = vec![];
            for (k, (_, bytes)) in pictures.iter().enumerate() {
                let out = if pictures.len() == 1 {
                    path.clone()
                } else {
                    let pth = std::path::Path::new(&path);
                    let stem = pth.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Slide".into());
                    let ext = pth.extension().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| format.clone());
                    pth.with_file_name(format!("{stem}{}.{ext}", k + 1)).to_string_lossy().to_string()
                };
                write_file(s, "file.export", &out, bytes)?;
                written.push(out);
            }
            Ok(json!({"files": written}))
        }
        "pdf" => {
            let bytes = pdf_bytes(&st.doc, p)?;
            write_file(s, "file.export", &path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len()}))
        }
        other => {
            let bytes = save_bytes(&st.doc, other)?;
            write_file(s, "file.export", &path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len()}))
        }
    }
}

/// `file.export` without a file: the PDF or the slides' pictures as base64, for a host to write
/// (an editor in a browser, an agent's export). `data` is the first file; with `all`, `files` lists
/// every slide's.
fn export_bytes(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "file.exportBytes";
    let format = str_param(p, "format").unwrap_or("pdf").to_ascii_lowercase();
    let st = s.doc()?;
    match format.as_str() {
        "png" | "jpeg" | "jpg" => {
            let pictures = slide_pictures(st, p, &format, C)?;
            let Some((_, first)) = pictures.first() else { return Err(bad(C, "no slide to export")) };
            let mut out = json!({"data": base64_encode(first), "format": format, "bytes": first.len()});
            if bool_or(p, "all", false) && usize_param(p, "slide").is_none() {
                out["files"] = pictures.iter().map(|(slide, bytes)| json!({"slide": slide, "data": base64_encode(bytes)})).collect();
            }
            Ok(out)
        }
        "pdf" => {
            let bytes = pdf_bytes(&st.doc, p)?;
            Ok(json!({"data": base64_encode(&bytes), "format": "pdf", "bytes": bytes.len()}))
        }
        other => Err(bad(C, format!("cannot export `{other}`: the formats are pdf, png, and jpeg"))),
    }
}

fn close(s: &mut Session, _p: &Value) -> Result<Value> {
    let Some(i) = s.active_index() else { return Ok(json!({"closed": false})) };
    s.close_document(i);
    Ok(json!({"closed": true}))
}

fn properties(s: &mut Session, p: &Value) -> Result<Value> {
    let fields = ["title", "subject", "author", "keywords", "comments", "category", "company"];
    if fields.iter().any(|f| p.get(*f).is_some()) {
        s.edit(|doc, _| {
            let pr = &mut doc.props;
            for f in fields {
                if let Some(v) = str_param(p, f) {
                    let slot = match f {
                        "title" => &mut pr.title,
                        "subject" => &mut pr.subject,
                        "author" => &mut pr.author,
                        "keywords" => &mut pr.keywords,
                        "comments" => &mut pr.comments,
                        "category" => &mut pr.category,
                        _ => &mut pr.company,
                    };
                    *slot = v.to_string();
                }
            }
            Ok(())
        })?;
    }
    Ok(serde_json::to_value(&s.doc()?.doc.props).unwrap_or_default())
}

fn revert(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc_mut()?;
    st.doc = st.saved_doc.clone();
    st.history = crate::History { limit: st.history.limit, pushed: st.history.pushed, ..Default::default() };
    st.revision += 1;
    ok()
}

fn next_window(s: &mut Session, p: &Value) -> Result<Value> {
    let n = s.documents().len();
    if n == 0 {
        return ok();
    }
    let i = usize_param(p, "index").unwrap_or_else(|| (s.active_index().unwrap_or(0) + 1) % n);
    s.set_active(i.min(n - 1));
    ok()
}

fn recovery_dir(s: &Session) -> Result<std::path::PathBuf> {
    crate::disk::need(s, "file.recovery")?;
    s.recovery_dir.clone().ok_or_else(|| EngineError::Other("no recovery folder is set".into()))
}

fn recovery_save(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(json!({"written": crate::recovery::save(s, &dir)?}))
}

fn recovery_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(Value::Array(
        crate::recovery::list(&dir)
            .into_iter()
            .map(|(uid, mut m)| {
                if let Some(o) = m.as_object_mut() {
                    o.insert("uid".into(), json!(uid));
                }
                m
            })
            .collect(),
    ))
}

fn recovery_open(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(json!({"opened": crate::recovery::open(s, &dir)?}))
}

fn recovery_discard(s: &mut Session, p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    match p.get("uid").and_then(Value::as_u64) {
        Some(uid) => crate::recovery::discard(&dir, uid),
        None => {
            for (uid, _) in crate::recovery::list(&dir) {
                crate::recovery::discard(&dir, uid);
            }
        }
    }
    ok()
}
