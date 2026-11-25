//! Kition Slide's adapter for the kit suite's performance mode (`kition-convergence`'s `perf`): the
//! slide driven as a host drives it, through `SlideDocument`'s commands and its contract 2
//! members (`open_with_replica`, `export_changes`, `import_changes`, `snapshot`, blobs).

#![allow(dead_code)]

use kition_convergence::{Blobs, KitAdapter, Rng, Script};
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_slide_kit::SlideDocument;
use serde_json::{Value, json};

pub const NAME: &str = "Slide.kslide";

/// Two IME results and emoji, by code point (CJK ideographs "middle" and "text", a grinning face).
const CJK_MIDDLE: char = '\u{4E2D}';
const CJK_TEXT: char = '\u{6587}';
const EMOJI_GRIN: char = '\u{1F600}';

pub struct SlideKit;

/// One replica as a host holds it: the engine, and the host's blob store for the document. The
/// engine keeps only the blobs its state references (and those handed to it until its next
/// import), so a host hands its store to the engine before every import, as a replay of a local
/// change log or a change that arrives before what it depends on needs blobs the engine dropped.
pub struct Host {
    pub doc: SlideDocument,
    store: Blobs,
}

impl std::ops::Deref for Host {
    type Target = SlideDocument;
    fn deref(&self) -> &SlideDocument {
        &self.doc
    }
}

impl std::ops::DerefMut for Host {
    fn deref_mut(&mut self) -> &mut SlideDocument {
        &mut self.doc
    }
}

const PRESETS: [&str; 3] = ["rect", "ellipse", "star5"];

fn run(doc: &mut SlideDocument, command: &str, params: Value) -> Result<Value, String> {
    doc.execute(command, params).map_err(|error| format!("{command}: {error}"))
}

/// Runs a command whose failure only means it did not apply to this state.
fn attempt(doc: &mut SlideDocument, command: &str, params: Value) -> bool {
    doc.execute(command, params).is_ok()
}

fn slides(doc: &mut SlideDocument) -> Result<Vec<Value>, String> {
    Ok(run(doc, "file.inspect", json!({}))?["slides"].as_array().cloned().unwrap_or_default())
}

/// The shapes of slide `index`, after making it the current slide (commands act on it).
fn shapes(doc: &mut SlideDocument, index: usize) -> Result<Vec<Value>, String> {
    run(doc, "text.exit", json!({})).ok();
    run(doc, "slide.go", json!({ "index": index }))?;
    Ok(run(doc, "slide.inspect", json!({ "index": index }))?["shapes"].as_array().cloned().unwrap_or_default())
}

/// The probe text frame: the first shape of the first slide (its title).
fn text_probe(doc: &mut SlideDocument) -> Result<Option<Value>, String> {
    if slides(doc)?.is_empty() {
        return Ok(None);
    }
    Ok(shapes(doc, 0)?.first().map(|shape| shape["id"].clone()))
}

/// The probe object: the last shape of the first slide that is not a placeholder.
fn probe(doc: &mut SlideDocument) -> Result<Option<Value>, String> {
    if slides(doc)?.is_empty() {
        return Ok(None);
    }
    Ok(shapes(doc, 0)?.iter().rev().find(|shape| shape["kind"] != "placeholder").map(|shape| shape["id"].clone()))
}

fn paragraphs(doc: &mut SlideDocument, id: &Value) -> Result<Vec<String>, String> {
    let got = run(doc, "text.get", json!({ "id": id }))?;
    let text = got["text"].as_str().unwrap_or_default();
    Ok(text.split('\n').map(str::to_owned).collect())
}

/// `[paragraph, character]` of character `at` in the paragraphs joined by `\n`.
fn position(paragraphs: &[String], at: usize) -> [usize; 2] {
    let mut rest = at;
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let len = paragraph.chars().count();
        if rest <= len {
            return [index, rest];
        }
        rest -= len + 1;
    }
    let last = paragraphs.len().saturating_sub(1);
    [last, paragraphs.last().map_or(0, |p| p.chars().count())]
}

/// Types `text` at character `at` of the probe text.
fn type_at(doc: &mut SlideDocument, at: usize, text: &str) -> Result<bool, String> {
    let Some(id) = text_probe(doc)? else { return Ok(false) };
    let paragraphs = paragraphs(doc, &id)?;
    run(doc, "text.edit", json!({ "id": id, "at": position(&paragraphs, at) }))?;
    run(doc, "text.insert", json!({ "text": text }))?;
    run(doc, "text.exit", json!({}))?;
    Ok(true)
}

fn delete_at(doc: &mut SlideDocument, at: usize, len: usize) -> Result<bool, String> {
    let Some(id) = text_probe(doc)? else { return Ok(false) };
    let paragraphs = paragraphs(doc, &id)?;
    run(doc, "text.edit", json!({ "id": id, "at": position(&paragraphs, at) }))?;
    run(doc, "text.select", json!({ "anchor": position(&paragraphs, at), "caret": position(&paragraphs, at + len) }))?;
    run(doc, "text.delete", json!({}))?;
    run(doc, "text.exit", json!({}))?;
    Ok(true)
}

/// A small valid PNG (stored deflate) whose colour derives from `seed`.
pub fn png(seed: u32) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let body = [kind, data].concat();
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    let (w, h) = (4u32, 4u32);
    let [r, g, b, _] = seed.wrapping_mul(2_654_435_761).to_be_bytes();
    // Each row: filter type 0, then the pixels.
    let row: Vec<u8> = std::iter::once(0).chain((0..w).flat_map(|_| [r, g, b])).collect();
    let raw: Vec<u8> = (0..h).flat_map(|_| row.iter().copied()).collect();
    let (mut a, mut bsum) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        bsum = (bsum + a) % 65_521;
    }
    let len = raw.len() as u16;
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&len.to_le_bytes());
    zlib.extend_from_slice(&(!len).to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&((bsum << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `(64-char hex sha256, u64 LE length, bytes) * N`, as `blobs` and `add_blobs` move them.
fn encode_blobs(blobs: &Blobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in blobs {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`].
fn decode_blobs(bytes: &[u8]) -> Result<Blobs, String> {
    let mut out = Blobs::new();
    let mut at = 0;
    while at < bytes.len() {
        let hash = std::str::from_utf8(bytes.get(at..at + 64).ok_or("truncated blob hash")?).map_err(|e| e.to_string())?.to_owned();
        let len = u64::from_le_bytes(bytes.get(at + 64..at + 72).ok_or("truncated blob length")?.try_into().map_err(|_| "length")?) as usize;
        let data = bytes.get(at + 72..at + 72 + len).ok_or("truncated blob data")?.to_vec();
        out.insert(hash, data);
        at += 72 + len;
    }
    Ok(out)
}

impl KitAdapter for SlideKit {
    type Doc = Host;

    fn name(&self) -> &str {
        "kit.slide"
    }

    fn schema(&self) -> &str {
        include_str!("../../../../crates/kdm/kdm-model/model/schema.json")
    }

    fn create(&self) -> Result<Host, String> {
        let mut doc = SlideDocument::create(NAME, 0, 0)?;
        let title = text_probe(&mut doc)?.ok_or("a new slide has no title")?;
        run(&mut doc, "text.set", json!({ "id": title, "text": "Hello world\nSecond line" }))?;
        run(&mut doc, "shape.insert", json!({ "preset": "rect", "rect": [100, 300, 120, 80], "text": "box" }))?;
        run(&mut doc, "shape.insert", json!({ "preset": "ellipse", "rect": [300, 300, 80, 80] }))?;
        run(&mut doc, "slide.new", json!({ "layout": "titleAndContent", "title": "Agenda", "body": "One\nTwo" }))?;
        run(&mut doc, "slide.new", json!({ "layout": "blank" }))?;
        run(&mut doc, "shape.insert", json!({ "preset": "star5", "rect": [50, 50, 60, 60] }))?;
        Ok(Host { doc, store: Blobs::new() })
    }

    fn join(&self, snapshot: &[u8], blobs: &Blobs) -> Result<Host, String> {
        let encoded = encode_blobs(blobs);
        let mut doc = SlideDocument::open_with_replica(NAME, &[], snapshot, &encoded)?;
        // `open_with_replica` falls back to a plain open; make sure the replica is the one given.
        if !doc.has_replica() {
            return Err("the snapshot did not open as a replica".into());
        }
        doc.version()?;
        Ok(Host { doc, store: blobs.clone() })
    }

    fn version(&self, doc: &mut Host) -> Result<Vec<u8>, String> {
        doc.version()
    }

    fn export_changes(&self, doc: &mut Host, since: &[u8]) -> Result<Vec<u8>, String> {
        doc.export_changes(since)
    }

    fn import_changes(&self, doc: &mut Host, bytes: &[u8]) -> Result<(), String> {
        let own = decode_blobs(&doc.doc.blobs()?)?;
        doc.store.extend(own);
        let store = encode_blobs(&doc.store);
        doc.doc.add_blobs(&store)?;
        doc.doc.import_changes(bytes).map(|_| ())
    }

    fn snapshot(&self, doc: &mut Host) -> Result<Vec<u8>, String> {
        doc.snapshot()
    }

    fn state(&self, doc: &mut Host) -> Result<Value, String> {
        let snapshot = doc.snapshot()?;
        let replica = Replica::load(kdm_model::SlideModel::schema(), &snapshot, ActorId::random()).map_err(|e| e.to_string())?;
        replica.state_json().map_err(|e| e.to_string())
    }

    fn undo(&self, doc: &mut Host) -> Result<bool, String> {
        run(doc, "text.exit", json!({})).ok();
        doc.undo().map(|(done, _)| done)
    }

    fn redo(&self, doc: &mut Host) -> Result<bool, String> {
        run(doc, "text.exit", json!({})).ok();
        doc.redo().map(|(done, _)| done)
    }

    fn random_edit(&self, doc: &mut Host, rng: &mut Rng) -> Result<Option<String>, String> {
        let count = slides(doc)?.len();
        if count == 0 {
            run(doc, "slide.new", json!({}))?;
            return Ok(Some("slide.new".into()));
        }
        let slide = rng.below(count);
        let list = shapes(doc, slide)?;
        let shape = rng.pick(&list).map(|shape| shape["id"].clone());
        let value = rng.below(1000) as u32;
        let (label, done) = match (rng.below(22), shape) {
            (0, _) => ("slide.new", attempt(doc, "slide.new", json!({ "layout": "titleAndContent", "title": format!("S{value}") }))),
            (1, _) => ("slide.duplicate", attempt(doc, "slide.duplicate", json!({ "index": slide }))),
            (2, _) if count > 1 => ("slide.delete", attempt(doc, "slide.delete", json!({ "index": slide }))),
            (3, _) => ("slide.move", attempt(doc, "slide.move", json!({ "from": slide, "to": rng.below(count) }))),
            (4 | 5, _) => (
                "shape.insert",
                attempt(doc, "shape.insert", json!({ "preset": PRESETS[rng.below(3)], "rect": [rng.below(800), rng.below(450), 60, 40] })),
            ),
            (6 | 7, Some(id)) => ("shape.move", attempt(doc, "shape.move", json!({ "x": rng.below(800), "y": rng.below(450), "ids": [id] }))),
            (8, Some(id)) => {
                ("shape.resize", attempt(doc, "shape.resize", json!({ "w": 20 + rng.below(200), "h": 20 + rng.below(100), "ids": [id] })))
            }
            (9 | 10, Some(id)) => ("shape.fill", attempt(doc, "shape.fill", json!({ "color": format!("#{:06x}", value * 16_000), "ids": [id] }))),
            (11, Some(id)) => ("edit.delete", attempt(doc, "edit.delete", json!({ "ids": [id] }))),
            (12, Some(id)) => ("edit.duplicate", attempt(doc, "edit.duplicate", json!({ "ids": [id] }))),
            (13, _) if list.len() >= 2 => {
                let ids: Vec<Value> = list.iter().take(2).map(|shape| shape["id"].clone()).collect();
                ("arrange.group", attempt(doc, "arrange.group", json!({ "ids": ids })))
            }
            (14, Some(id)) => ("arrange.ungroup", attempt(doc, "arrange.ungroup", json!({ "ids": [id] }))),
            (15, Some(id)) => ("arrange.bringToFront", attempt(doc, "arrange.bringToFront", json!({ "ids": [id] }))),
            (16..=18, Some(id)) => {
                let text = match rng.below(3) {
                    0 => format!("t{value}"),
                    1 => [CJK_MIDDLE, CJK_TEXT].iter().collect(),
                    _ => format!("{EMOJI_GRIN} "),
                };
                let typed = attempt(doc, "text.edit", json!({ "id": id, "end": true }))
                    && attempt(doc, "text.insert", json!({ "text": if rng.chance(15) { format!("{text}\n") } else { text } }));
                run(doc, "text.exit", json!({})).ok();
                ("text.insert", typed)
            }
            (19, _) => ("slide.notes", attempt(doc, "slide.notes", json!({ "text": format!("note {value}"), "index": slide }))),
            (20, _) => (
                "image.insert",
                attempt(doc, "image.insert", json!({ "data": base64(&png(value)), "name": format!("p{value}.png"), "rect": [10, 10, 40, 40] })),
            ),
            (21, Some(id)) => ("text.set", attempt(doc, "text.set", json!({ "id": id, "text": format!("set {value}\nline") }))),
            _ => ("none", false),
        };
        Ok(done.then(|| label.to_owned()))
    }

    fn script(&self, doc: &mut Host, step: &Script) -> Result<bool, String> {
        match step {
            Script::SetField { slot, value } => {
                let id = probe(doc)?.ok_or("no probe shape")?;
                if *slot == 0 {
                    run(doc, "shape.move", json!({ "x": 10 + value % 700, "y": 20, "ids": [id] }))?;
                } else {
                    run(doc, "shape.fill", json!({ "color": format!("#{:06x}", (value * 7919) % 0xFF_FFFF), "ids": [id] }))?;
                }
            }
            Script::Create => {
                shapes(doc, 0)?;
                run(doc, "shape.insert", json!({ "preset": "rect", "rect": [500, 100, 50, 50] }))?;
            }
            Script::Delete => {
                let id = probe(doc)?.ok_or("no probe shape")?;
                run(doc, "edit.delete", json!({ "ids": [id] }))?;
            }
            Script::Duplicate => {
                let id = probe(doc)?.ok_or("no probe shape")?;
                run(doc, "edit.duplicate", json!({ "ids": [id] }))?;
            }
            Script::Reorder { which } => {
                let count = slides(doc)?.len();
                if count < 2 {
                    return Ok(true);
                }
                run(doc, "slide.move", json!({ "from": usize::from(*which) % count, "to": count - 1 }))?;
            }
            Script::InsertText { at, text } => return type_at(doc, *at, text),
            Script::DeleteText { at, len } => return delete_at(doc, *at, *len),
            Script::SplitParagraph { at } => return type_at(doc, *at, "\n"),
            Script::MergeParagraph { at } => return delete_at(doc, *at, 1),
            Script::InsertBlob { seed } => {
                shapes(doc, 0)?;
                run(doc, "image.insert", json!({ "data": base64(&png(*seed)), "name": format!("p{seed}.png"), "rect": [10, 10, 40, 40] }))?;
            }
            // The schema has no tree: groups are a list of children.
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn text(&self, doc: &mut Host) -> Result<Option<String>, String> {
        let Some(id) = text_probe(doc)? else { return Ok(None) };
        Ok(Some(paragraphs(doc, &id)?.join("\n")))
    }

    fn check_view(&self, doc: &mut Host) -> Result<(), String> {
        let saved = run(doc, "file.saveBytes", json!({ "format": "kslide" }))?;
        let bytes = kition_convergence_b64_decode(saved["data"].as_str().ok_or("file.saveBytes gave no data")?)?;
        let slide_file = kdm_model::decode(NAME, &bytes).map_err(|e| e.to_string())?;
        let snapshot = doc.snapshot()?;
        let blobs = decode_blobs(&doc.blobs()?)?;
        let mut memory = MemoryBlobs::default();
        for (hash, bytes) in blobs {
            memory.0.insert(hash, std::sync::Arc::from(bytes.as_slice()));
        }
        let session = kdm_model::SlideSession::open_snapshot(&snapshot, memory).map_err(|e| e.to_string())?;
        if session.holds(&slide_file).map_err(|e| e.to_string())? { Ok(()) } else { Err("the engine's slide differs from its replica".into()) }
    }

    fn blobs(&self, doc: &mut Host) -> Result<Blobs, String> {
        decode_blobs(&doc.blobs()?)
    }

    fn add_blobs(&self, doc: &mut Host, blobs: &Blobs) -> Result<(), String> {
        doc.store.extend(blobs.clone());
        doc.doc.add_blobs(&encode_blobs(blobs))
    }

    fn export_file(&self, doc: &mut Host) -> Result<Option<Vec<u8>>, String> {
        doc.save(Some("Slide.pptx")).map(Some)
    }

    fn keystroke(&self, doc: &mut Host, _rng: &mut Rng) -> Result<(), String> {
        // Random edits can leave the first slide without shapes: give it a text box once.
        let id = match text_probe(doc)? {
            Some(id) => id,
            None => {
                if slides(doc)?.is_empty() {
                    run(doc, "slide.new", json!({}))?;
                }
                shapes(doc, 0)?;
                run(doc, "textBox.insert", json!({ "rect": [100, 100, 300, 80], "text": "typing" }))?;
                text_probe(doc)?.ok_or("no text box")?
            }
        };
        run(doc, "text.edit", json!({ "id": id, "end": true }))?;
        run(doc, "text.insert", json!({ "text": "a" }))?;
        run(doc, "text.exit", json!({}))?;
        // The repaint of the visible page: an editor shows the current slide alone (`setSingle`,
        // web/src/slides.tsx), and kit-editor renders the canvas a pane shows, here at half scale.
        // (All the slides stacked is an agent's or overview's canvas, not the page typed on.)
        doc.set_single(true);
        let (w, h) = (doc.width() as i32, doc.height() as i32);
        doc.render_area(0, 0, w, h, (w / 2) as u32, (h / 2) as u32).map(|_| ())
    }
}

/// Standard base64 (with padding), as `file.saveBytes` answers.
fn kition_convergence_b64_decode(text: &str) -> Result<Vec<u8>, String> {
    let value = |c: u8| -> Result<u32, String> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("not base64: {c}")),
        } as u32)
    };
    let mut out = Vec::new();
    for chunk in text.trim_end_matches('=').as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Ok(out)
}
