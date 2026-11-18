//! PPTX export: [`SlideFile`] → package.

mod chart;
pub(crate) mod dml;
pub(crate) mod shapes;
mod theme;
pub(crate) mod timing;

use std::collections::HashMap;
use std::sync::Arc;

use kition_slide_geom::Xfrm;
use kition_slide_model::text::{Paragraph, TextBody};
use kition_slide_model::{
    Background, Comment, Layout, LayoutId, Master, MediaId, PhType, Placeholder, Shape, ShapeId, Slide, SlideFile, SlideId, defaults,
};

use crate::PptxError;
use crate::opc::{self, NS_A, NS_P, NS_P14, NS_R, PackageOut, RelsOut};
use crate::xml::{A, W};

const CT_PRES: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const CT_LAYOUT: &str = "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
const CT_MASTER: &str = "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
const CT_NOTES_MASTER: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml";
const CT_HANDOUT_MASTER: &str = "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml";
const CT_NOTES: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";
const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
const CT_PRES_PROPS: &str = "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";
const CT_VIEW_PROPS: &str = "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";
const CT_TABLE_STYLES: &str = "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml";
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CT_COMMENTS: &str = "application/vnd.openxmlformats-officedocument.presentationml.comments+xml";
const CT_AUTHORS: &str = "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml";
const CT_CUSTOM: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";
const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const CT_APP: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";
const CT_TAGS: &str = "application/vnd.openxmlformats-officedocument.presentationml.tags+xml";

/// Export state.
pub struct Exp<'a> {
    pub p: &'a SlideFile,
    pub pkg: PackageOut,
    media_parts: HashMap<MediaId, String>,
    n_image: u32,
    n_media: u32,
    n_chart: u32,
    n_embed: u32,
    n_tags: u32,
    n_font: u32,
    n_diagram: u32,
    /// Kept parts whose own relationships are written already.
    rels_written: std::collections::HashSet<String>,
    pub slide_parts: HashMap<SlideId, String>,
    pub table_styles: Vec<String>,
    placeholder: Option<String>,
    /// The slide's font faces by (slot, typeface).
    faces: HashMap<(&'a str, &'a str), &'a kition_slide_model::FontFace>,
    /// Each slide's `p:sldId/@id`, in slide order (see [`slide_ids`]).
    sld_ids: Vec<u32>,
}

/// The `p:sldId` ids: each slide's id from its file when it is valid (256 up to 2^31 − 1) and not
/// taken by an earlier slide, else the next free id above all of them.
fn slide_ids(p: &SlideFile) -> Vec<u32> {
    const MAX: u32 = 2_147_483_647;
    let valid = |id: u32| (256..=MAX).contains(&id);
    let mut next = p.slides.iter().filter_map(|s| s.file_id).filter(|&i| valid(i)).max().map_or(256, |m| m.saturating_add(1));
    let mut used = std::collections::HashSet::new();
    p.slides
        .iter()
        .map(|s| {
            let id = match s.file_id.filter(|&i| valid(i) && !used.contains(&i)) {
                Some(id) => id,
                None => {
                    while next > MAX || used.contains(&next) {
                        next = if next > MAX { 256 } else { next + 1 };
                    }
                    next
                }
            };
            used.insert(id);
            id
        })
        .collect()
}

/// A part being written: its name and relationships.
pub struct Out {
    pub name: String,
    pub rels: RelsOut,
}

impl Out {
    fn new(name: &str) -> Self {
        Out { name: name.to_string(), rels: RelsOut::default() }
    }
}

fn ext_for(content_type: &str, name: &str, data: &[u8]) -> String {
    let by_ct = match content_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "image/x-emf" | "image/emf" => "emf",
        "image/x-wmf" | "image/wmf" => "wmf",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/x-ms-wmv" => "wmv",
        "video/x-msvideo" => "avi",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/x-ms-wma" => "wma",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        _ => "",
    };
    if !by_ct.is_empty() {
        return by_ct.to_string();
    }
    if data.starts_with(b"\x89PNG") {
        return "png".into();
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        return "jpeg".into();
    }
    if data.starts_with(b"GIF8") {
        return "gif".into();
    }
    let e = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()) { e } else { "bin".into() }
}

/// The part name for a media item named `name` in folder `dir`: its own name when that is a plain
/// file name with extension `ext` that no part has yet, so the file read again names the item as
/// it was (task card C8-slide-5); else `numbered(n)` for the next `n` of `counter` that no part has.
fn part_name(pkg: &PackageOut, dir: &str, name: &str, ext: &str, counter: &mut u32, numbered: impl Fn(u32) -> String) -> String {
    let plain = !name.is_empty()
        && name.len() <= 96
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
        && name.rsplit_once('.').is_some_and(|(stem, e)| !stem.is_empty() && e.eq_ignore_ascii_case(ext));
    if plain {
        let own = format!("{dir}/{name}");
        if !pkg.has_part(&own) {
            return own;
        }
    }
    loop {
        *counter += 1;
        let candidate = numbered(*counter);
        if !pkg.has_part(&candidate) || *counter == u32::MAX {
            return candidate;
        }
    }
}

impl Exp<'_> {
    fn media_part(&mut self, id: MediaId) -> Option<String> {
        if let Some(p) = self.media_parts.get(&id) {
            return Some(p.clone());
        }
        let m = self.p.media(id)?;
        if m.data.is_empty() {
            // An image the file had no bytes for: a transparent pixel under the item's own name,
            // which reads back as an image with no bytes (`read::media_for`), so the deck read again
            // holds the item as it was (C8-slide-5) and PowerPoint shows nothing in its place.
            if m.link.is_some() || !m.content_type.starts_with("image/") {
                return None;
            }
            let ext = ext_for(&m.content_type, &m.name, &m.data);
            let name = part_name(&self.pkg, "ppt/media", &m.name, &ext, &mut self.n_image, |n| format!("ppt/media/image{n}.{ext}"));
            self.pkg.binary(&name, &m.content_type, crate::png::transparent_1x1());
            self.media_parts.insert(id, name.clone());
            return Some(name);
        }
        let ext = ext_for(&m.content_type, &m.name, &m.data);
        let ct = if m.content_type.is_empty() || !m.content_type.contains('/') {
            crate::mime_for(&format!("x.{ext}")).to_string()
        } else {
            m.content_type.clone()
        };
        let name = if ct.starts_with("image/") {
            part_name(&self.pkg, "ppt/media", &m.name, &ext, &mut self.n_image, |n| format!("ppt/media/image{n}.{ext}"))
        } else {
            part_name(&self.pkg, "ppt/media", &m.name, &ext, &mut self.n_media, |n| format!("ppt/media/media{n}.{ext}"))
        };
        self.pkg.binary(&name, &ct, m.data.as_ref().clone());
        self.media_parts.insert(id, name.clone());
        Some(name)
    }
    /// A relationship from `o` to a media item (embedded part, or external link).
    pub fn media_rel(&mut self, o: &mut Out, id: MediaId, kind: &str) -> Option<String> {
        let m = self.p.media(id)?;
        if let Some(link) = m.link.clone().filter(|_| m.data.is_empty()) {
            return Some(o.rels.add(kind, &link, true));
        }
        let part = self.media_part(id)?;
        Some(o.rels.add(kind, &opc::relative(&o.name, &part), false))
    }
    pub fn media_rel_new(&mut self, o: &mut Out, id: MediaId, kind: &str) -> Option<String> {
        let m = self.p.media(id)?;
        if m.data.is_empty() {
            return None;
        }
        let part = self.media_part(id)?;
        Some(o.rels.add(kind, &opc::relative(&o.name, &part), false))
    }
    /// The face attributes of a typeface in a slot (`latin`, `buFont`…), if the slide knows them.
    pub fn font_face(&self, slot: &str, typeface: &str) -> Option<&kition_slide_model::FontFace> {
        self.faces.get(&(slot, typeface)).copied()
    }
    /// A tiny transparent PNG for pictures whose image is missing.
    pub fn placeholder_rel(&mut self, o: &mut Out) -> String {
        let part = match &self.placeholder {
            Some(p) => p.clone(),
            None => {
                let name = part_name(&self.pkg, "ppt/media", "", "png", &mut self.n_image, |n| format!("ppt/media/image{n}.png"));
                self.pkg.binary(&name, "image/png", crate::png::transparent_1x1());
                self.placeholder = Some(name.clone());
                name
            }
        };
        o.rels.add("image", &opc::relative(&o.name, &part), false)
    }
    /// A relationship from `o` that kept XML names, to the part carried for it (written once per
    /// media item: images under `ppt/media`, tag lists under `ppt/tags`, anything else under
    /// `ppt/embeddings`), or to its external link.
    pub fn kept_rel(&mut self, o: &mut Out, r: &kition_slide_model::KeptRel) -> Option<String> {
        let ty = r.rel_type.as_str();
        if !ty.starts_with("http") || ty.ends_with("/slide") || ty.ends_with("/slideLayout") {
            return None;
        }
        let m = self.p.media(r.media)?;
        if m.data.is_empty() {
            let link = m.link.clone()?;
            return Some(o.rels.add(ty, &link, true));
        }
        // Images (an SVG original, an HD Photo layer) go to ppt/media as Office writes them.
        let image = ty.ends_with("/image") || ty.ends_with("/hdphoto");
        let part = if image { self.media_part(r.media)? } else { self.embedded_part(r.media, ty)? };
        self.part_rels(&part, &r.rels);
        Some(o.rels.add(ty, &opc::relative(&o.name, &part), false))
    }
    /// A kept part's own relationships (`KeptRel::rels`), by the ids its bytes use, written once.
    fn part_rels(&mut self, part: &str, rels: &[kition_slide_model::KeptRel]) {
        if rels.is_empty() || !self.rels_written.insert(part.to_string()) {
            return;
        }
        let mut out = RelsOut::default();
        for r in rels {
            let Some(m) = self.p.media(r.media) else { continue };
            if m.data.is_empty() {
                if let Some(link) = m.link.clone() {
                    out.items.push((r.id.clone(), r.rel_type.clone(), link, true));
                }
                continue;
            }
            let ty = r.rel_type.as_str();
            let target = if ty.ends_with("/image") || ty.ends_with("/hdphoto") { self.media_part(r.media) } else { self.embedded_part(r.media, ty) };
            if let Some(target) = target {
                out.items.push((r.id.clone(), r.rel_type.clone(), opc::relative(part, &target), false));
            }
        }
        self.pkg.rels(part, &out);
    }
    /// [`Self::kept_rel`] for a diagram data part, whose bytes name its drawing by the
    /// relationship id `old` of the slide it was read from: written naming it `new`, its id here.
    pub fn kept_diagram_data(&mut self, o: &mut Out, r: &kition_slide_model::KeptRel, old: &str, new: &str) -> Option<String> {
        let m = self.p.media(r.media)?;
        if let Some(name) = self.media_parts.get(&m.id) {
            return Some(o.rels.add(&r.rel_type, &opc::relative(&o.name, name), false));
        }
        let text = std::str::from_utf8(&m.data).ok()?;
        let data = text.replacen(&format!("relId=\"{old}\""), &format!("relId=\"{new}\""), 1).into_bytes();
        let ct = if m.content_type.contains('/') {
            m.content_type.clone()
        } else {
            "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml".to_string()
        };
        let name = part_name(&self.pkg, "ppt/diagrams", &m.name, "xml", &mut self.n_diagram, |n| format!("ppt/diagrams/data{n}.xml"));
        self.pkg.binary(&name, &ct, data);
        self.media_parts.insert(m.id, name.clone());
        self.part_rels(&name, &r.rels);
        Some(o.rels.add(&r.rel_type, &opc::relative(&o.name, &name), false))
    }
    fn embedded_part(&mut self, id: MediaId, ty: &str) -> Option<String> {
        if let Some(p) = self.media_parts.get(&id) {
            return Some(p.clone());
        }
        let m = self.p.media(id)?;
        let ext = ext_for(&m.content_type, &m.name, &m.data);
        let ct = if m.content_type.contains('/') { m.content_type.clone() } else { crate::mime_for(&format!("x.{ext}")).to_string() };
        let name = if ty.contains("/diagram") {
            // Diagram parts (data, layout, quickStyle, colors, drawing) where Office keeps them.
            let stem: String = m.name.chars().take_while(|c| c.is_ascii_alphabetic()).take(32).collect();
            let stem = if stem.is_empty() { "diagram" } else { stem.as_str() };
            let name = part_name(&self.pkg, "ppt/diagrams", &m.name, &ext, &mut self.n_diagram, |n| format!("ppt/diagrams/{stem}{n}.{ext}"));
            self.pkg.binary(&name, &ct, m.data.as_ref().clone());
            name
        } else if ty.ends_with("/font") {
            let name = part_name(&self.pkg, "ppt/fonts", &m.name, &ext, &mut self.n_font, |n| format!("ppt/fonts/font{n}.{ext}"));
            self.pkg.binary(&name, &ct, m.data.as_ref().clone());
            name
        } else if ty.ends_with("/tags") {
            self.n_tags += 1;
            let name = format!("ppt/tags/tag{}.xml", self.n_tags);
            self.pkg.xml_part(&name, &ct, m.data.as_ref().clone());
            name
        } else {
            let stem: String = m
                .name
                .rsplit_once('.')
                .map_or(m.name.as_str(), |(s, _)| s)
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(64)
                .collect();
            // Numbered afresh (`Workbook1` is written as `Workbook<n>`, not `Workbook1<n>`).
            let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit());
            let stem = if stem.is_empty() { "object" } else { stem };
            let name = part_name(&self.pkg, "ppt/embeddings", &m.name, &ext, &mut self.n_embed, |n| format!("ppt/embeddings/{stem}{n}.{ext}"));
            self.pkg.binary(&name, &ct, m.data.as_ref().clone());
            name
        };
        self.media_parts.insert(id, name.clone());
        Some(name)
    }
    /// A tag list part and a relationship from `o` to it.
    pub fn tags_rel(&mut self, o: &mut Out, tags: &kition_slide_model::TagList) -> String {
        self.n_tags += 1;
        let name = format!("ppt/tags/tag{}.xml", self.n_tags);
        let mut w = W::new();
        root_open(&mut w, "p:tagLst", A::new());
        for (k, v) in tags {
            w.empty("p:tag", A::new().a("name", k).a("val", v));
        }
        w.close("p:tagLst");
        self.pkg.xml_part(&name, CT_TAGS, w.finish());
        o.rels.add_new("tags", &opc::relative(&o.name, &name), false)
    }
    pub fn chart_rel(&mut self, o: &mut Out, c: &kition_slide_model::Chart) -> String {
        self.n_chart += 1;
        let n = self.n_chart;
        let name = format!("ppt/charts/chart{n}.xml");
        let mut co = Out::new(&name);
        let mut xml = chart::chart_xml(c, self, &mut co);
        // A chart without a kept part is written from the model: the formatting the writer leaves
        // out (axis scales and formats, label positions, series' lines and markers) is patched in.
        if c.raw.is_none()
            && let Ok(text) = std::str::from_utf8(&xml)
            && let Some(formatted) = kition_slide_model::chart_spec::formatted_part(text, c)
        {
            xml = formatted.into_bytes();
        }
        // A chart without a workbook of its data (one made here, or whose data were edited) gets
        // one, so PowerPoint's Edit Data opens them as cells.
        if let Ok(text) = std::str::from_utf8(&xml)
            && !kition_office_core::chart::codec::has_workbook(text)
            && let Ok(spec) = kition_office_core::chart::codec::read(text)
        {
            const PLACEHOLDER: &str = "rIdKitionData";
            if let Ok(bound) = kition_office_core::chart::codec::bind_workbook(text, PLACEHOLDER) {
                // Numbered as the other embedded parts are, so no name is taken twice.
                let book = loop {
                    self.n_embed += 1;
                    let name = format!("ppt/embeddings/Microsoft_Excel_Worksheet{}.xlsx", self.n_embed);
                    if !self.pkg.has_part(&name) {
                        break name;
                    }
                };
                let rid = co.rels.add(kition_office_core::chart::workbook::PACKAGE_REL, &opc::relative(&name, &book), false);
                self.pkg.binary(&book, kition_office_core::chart::workbook::XLSX_CONTENT_TYPE, kition_office_core::chart::workbook::workbook(&spec));
                xml = bound.replace(&format!(r#"r:id="{PLACEHOLDER}""#), &format!(r#"r:id="{rid}""#)).into_bytes();
            }
        }
        // Its style and colour style parts, one of each per chart as Office writes them.
        for r in &c.rels {
            let stem = if r.rel_type.ends_with("/chartStyle") {
                "style"
            } else if r.rel_type.ends_with("/chartColorStyle") {
                "colors"
            } else {
                continue;
            };
            let Some(m) = self.p.media(r.media).filter(|m| !m.data.is_empty()) else { continue };
            let part = if let Some(part) = self.media_parts.get(&m.id) {
                part.clone()
            } else {
                let part = part_name(&self.pkg, "ppt/charts", &m.name, "xml", &mut self.n_embed, |n| format!("ppt/charts/{stem}{n}.xml"));
                let ct = if m.content_type.contains('/') { m.content_type.clone() } else { crate::mime_for(&part).to_string() };
                self.pkg.xml_part(&part, &ct, m.data.as_ref().clone());
                self.media_parts.insert(m.id, part.clone());
                part
            };
            co.rels.add(&r.rel_type, &opc::relative(&name, &part), false);
        }
        self.pkg.xml_part(&name, CT_CHART, xml);
        self.pkg.rels(&name, &co.rels);
        o.rels.add_new("chart", &opc::relative(&o.name, &name), false)
    }
}

fn root_open(w: &mut W, tag: &str, extra: A) {
    let mut a = A::new().a("xmlns:a", NS_A).a("xmlns:r", NS_R).a("xmlns:p", NS_P);
    a.v.extend(extra.v);
    w.open(tag, a);
}

fn background(w: &mut W, x: &mut Exp, o: &mut Out, bg: &Background) {
    w.open0("p:bg");
    match bg {
        Background::Fill { fill } => {
            w.open0("p:bgPr");
            match fill {
                kition_slide_model::Fill::Background | kition_slide_model::Fill::Group => w.empty0("a:noFill"),
                f => dml::fill(w, x, o, f),
            }
            w.empty0("a:effectLst");
            w.close("p:bgPr");
        }
        Background::Ref { idx, color } => {
            let idx = if (1..=3).contains(idx) || (1001..=1003).contains(idx) { *idx } else { 1001 };
            w.open("p:bgRef", A::new().a("idx", idx));
            dml::color(w, color);
            w.close("p:bgRef");
        }
    }
    w.close("p:bg");
}

/// `p:custDataLst` naming a tag list part for each of `tags`.
pub fn cust_data(w: &mut W, x: &mut Exp, o: &mut Out, tags: &[kition_slide_model::TagList]) {
    if tags.is_empty() {
        return;
    }
    w.open0("p:custDataLst");
    for t in tags {
        let rid = x.tags_rel(o, t);
        w.empty("p:tags", A::new().a("r:id", rid));
    }
    w.close("p:custDataLst");
}

fn csld(
    w: &mut W,
    x: &mut Exp,
    o: &mut Out,
    name: &str,
    bg: Option<&Background>,
    list: &[Shape],
    tags: &[kition_slide_model::TagList],
) -> shapes::IdMap {
    w.open("p:cSld", A::new().o("name", (!name.is_empty()).then_some(name)));
    if let Some(bg) = bg {
        background(w, x, o, bg);
    }
    w.open0("p:spTree");
    shapes::tree_header(w);
    let mut ids = shapes::IdMap::build(list);
    {
        let mut cx = shapes::Ctx { ids: &mut ids, written: Default::default() };
        shapes::shapes(w, x, o, &mut cx, list, 0);
    }
    w.close("p:spTree");
    cust_data(w, x, o, tags);
    w.close("p:cSld");
    ids
}

const CLR_KEYS: [(&str, &str); 12] = [
    ("bg1", "lt1"),
    ("tx1", "dk1"),
    ("bg2", "lt2"),
    ("tx2", "dk2"),
    ("accent1", "accent1"),
    ("accent2", "accent2"),
    ("accent3", "accent3"),
    ("accent4", "accent4"),
    ("accent5", "accent5"),
    ("accent6", "accent6"),
    ("hlink", "hlink"),
    ("folHlink", "folHlink"),
];

fn clr_map(w: &mut W, map: &[(String, String)]) {
    let mut a = A::new();
    for (k, d) in CLR_KEYS {
        let v = map
            .iter()
            .find(|(mk, _)| mk == k)
            .map(|(_, v)| v.as_str())
            .filter(|v| kition_slide_color::SchemeSlot::from_xml(v).is_some_and(|s| kition_slide_color::SchemeSlot::THEME.contains(&s)))
            .unwrap_or(d);
        a = a.a(k, v);
    }
    w.empty("p:clrMap", a);
}

fn ext_list(w: &mut W, raw: Option<&str>) {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else { return };
    // Only keep well-formed extension elements.
    let Ok(d) = crate::xml::parse(format!("<x>{raw}</x>").as_bytes()) else { return };
    let exts: Vec<String> = d.root.elements().filter(|e| e.is("ext") && e.has_attr("uri")).map(|e| e.to_xml()).collect();
    if exts.is_empty() {
        return;
    }
    w.open0("p:extLst");
    for e in exts {
        w.raw(&e);
    }
    w.close("p:extLst");
}

/// Strip an `<p:extLst>` wrapper (masters/layouts keep the whole element).
fn ext_children(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let d = crate::xml::parse(raw.as_bytes()).ok()?;
    if d.root.is("extLst") {
        let ns = d.ns_decls();
        return Some(d.root.elements().map(|e| crate::read::keep_xml(e, &ns)).collect());
    }
    Some(raw.to_string())
}

fn notes_master_shapes(size: kition_slide_geom::Size) -> Vec<Shape> {
    let (w, h) = (size.width, size.height);
    let mk = |id: u32, name: &str, kind: PhType, idx: u32, x: f64, y: f64, ww: f64, hh: f64| Shape {
        id: ShapeId(id),
        name: name.into(),
        xfrm: Some(Xfrm::new(x, y, ww, hh)),
        ph: Some(Placeholder { kind, idx, size: (kind != PhType::SlideImage).then(|| "quarter".to_string()), ..Default::default() }),
        text: (kind != PhType::SlideImage).then(TextBody::default),
        ..Default::default()
    };
    let mut img = mk(4, "Slide Image Placeholder 3", PhType::SlideImage, 2, w * 0.125, h * 0.125, w * 0.75, h * 0.375);
    img.fill = Some(kition_slide_model::Fill::None);
    img.line = Some(kition_slide_model::Line::solid(kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::BLACK), 1.0));
    let mut body = mk(5, "Notes Placeholder 4", PhType::Body, 3, w * 0.1, h * 0.53, w * 0.8, h * 0.36);
    if let Some(t) = body.text.as_mut() {
        t.paragraphs = vec![Paragraph::new("Click to edit Master text styles")];
    }
    vec![
        mk(2, "Header Placeholder 1", PhType::Header, 0, 0.0, 0.0, w * 0.43, h * 0.05),
        mk(3, "Date Placeholder 2", PhType::Date, 1, w * 0.57, 0.0, w * 0.43, h * 0.05),
        img,
        body,
        mk(6, "Footer Placeholder 5", PhType::Footer, 4, 0.0, h * 0.95, w * 0.43, h * 0.05),
        mk(7, "Slide Number Placeholder 6", PhType::SlideNum, 5, w * 0.57, h * 0.95, w * 0.43, h * 0.05),
    ]
}

fn notes_style() -> kition_slide_model::text::ListStyle {
    let mut ls = kition_slide_model::text::ListStyle::default();
    for i in 0..9u8 {
        ls.set(
            i,
            kition_slide_model::text::LevelStyle {
                para: kition_slide_model::text::ParaProps {
                    margin_left: Some(36.0 * i as f64),
                    align: Some(kition_slide_model::text::Align::Left),
                    ..Default::default()
                },
                run: kition_slide_model::text::RunProps {
                    size: Some(12.0),
                    font: Some("+mn-lt".into()),
                    fill: Some(kition_slide_model::Fill::solid(kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Tx1))),
                    ..Default::default()
                },
            },
        );
    }
    ls
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b.iter().take(4).all(|c| c.is_ascii_digit())
        && b.get(4) == Some(&b'-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "-:.+TZ".contains(c))
}

/// A copy of the slide file with every slide pointing at a layout that exists.
fn sanitized(p: &SlideFile) -> std::borrow::Cow<'_, SlideFile> {
    let mut need = p.masters.is_empty() || p.masters.iter().any(|m| m.layouts.is_empty());
    need |= p.slides.iter().any(|s| p.layout(s.layout).is_none());
    if !need {
        return std::borrow::Cow::Borrowed(p);
    }
    let mut q = p.clone();
    if q.masters.is_empty() {
        let (m, next) = defaults::build_master(q.slide_size, Default::default(), q.next_id.max(1));
        q.next_id = next;
        q.masters.push(Arc::new(m));
    }
    for m in q.masters.iter_mut() {
        if m.layouts.is_empty() {
            let id = LayoutId(q.next_id.saturating_add(1));
            q.next_id = id.0;
            Arc::make_mut(m).layouts.push(Layout {
                id,
                name: "Blank".into(),
                kind: kition_slide_model::LayoutType::Blank,
                show_master_shapes: true,
                ..Default::default()
            });
        }
    }
    let first = q.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();
    let ok: Vec<LayoutId> = q.masters.iter().flat_map(|m| m.layouts.iter().map(|l| l.id)).collect();
    for s in q.slides.iter_mut() {
        if !ok.contains(&s.layout) {
            Arc::make_mut(s).layout = first;
        }
    }
    std::borrow::Cow::Owned(q)
}

pub fn export(p: &SlideFile) -> Result<Vec<u8>, PptxError> {
    let p = sanitized(p);
    let p: &SlideFile = &p;
    let mut x = Exp {
        p,
        pkg: PackageOut::default(),
        media_parts: HashMap::new(),
        n_image: 0,
        n_media: 0,
        n_chart: 0,
        n_embed: 0,
        n_tags: 0,
        n_font: 0,
        n_diagram: 0,
        rels_written: std::collections::HashSet::new(),
        slide_parts: HashMap::new(),
        table_styles: vec![],
        placeholder: None,
        faces: p.font_faces.iter().map(|f| ((f.slot.as_str(), f.typeface.as_str()), f)).collect(),
        sld_ids: slide_ids(p),
    };
    for (i, s) in p.slides.iter().enumerate() {
        x.slide_parts.insert(s.id, format!("ppt/slides/slide{}.xml", i + 1));
    }
    let mut pres = Out::new("ppt/presentation.xml");

    // Masters, themes and layouts.
    let mut master_rids = vec![];
    let mut layout_parts: HashMap<LayoutId, String> = HashMap::new();
    let mut n_layout = 0u32;
    let mut big_id: u64 = 2_147_483_648;
    for (mi, m) in p.masters.iter().enumerate() {
        let mname = format!("ppt/slideMasters/slideMaster{}.xml", mi + 1);
        let tname = format!("ppt/theme/theme{}.xml", mi + 1);
        let mut mo = Out::new(&mname);
        let mut lrids = vec![];
        for l in &m.layouts {
            n_layout += 1;
            let lname = format!("ppt/slideLayouts/slideLayout{n_layout}.xml");
            layout_parts.insert(l.id, lname.clone());
            let rid = mo.rels.add("slideLayout", &opc::relative(&mname, &lname), false);
            lrids.push(rid);
            let mut lo = Out::new(&lname);
            lo.rels.add("slideMaster", &opc::relative(&lname, &mname), false);
            let xml = layout_xml(&mut x, &mut lo, l);
            x.pkg.xml_part(&lname, CT_LAYOUT, xml);
            x.pkg.rels(&lname, &lo.rels);
        }
        mo.rels.add("theme", &opc::relative(&mname, &tname), false);
        let mut to = Out::new(&tname);
        let txml = theme::theme_xml(&mut x, &mut to, &m.theme);
        x.pkg.xml_part(&tname, CT_THEME, txml);
        x.pkg.rels(&tname, &to.rels);
        let master_id = big_id;
        big_id += 1;
        let mut layout_ids = vec![];
        for rid in lrids {
            layout_ids.push((big_id, rid));
            big_id += 1;
        }
        let xml = master_xml(&mut x, &mut mo, m, &layout_ids);
        x.pkg.xml_part(&mname, CT_MASTER, xml);
        x.pkg.rels(&mname, &mo.rels);
        master_rids.push((master_id, pres.rels.add("slideMaster", &opc::relative("ppt/presentation.xml", &mname), false)));
    }

    // Notes master when any slide has notes, or the file had one: its own shapes, background,
    // colour map and notes text style, else the defaults.
    let has_notes = p.notes_master.is_some() || p.slides.iter().any(|s| !s.notes.is_empty() || s.notes_page.is_some());
    let notes_master = "ppt/notesMasters/notesMaster1.xml";
    let mut notes_master_rid = None;
    if has_notes {
        let tname = format!("ppt/theme/theme{}.xml", p.masters.len() + 1);
        let mut no = Out::new(notes_master);
        no.rels.add("theme", &opc::relative(notes_master, &tname), false);
        let theme = p.notes_master.as_ref().map(|m| m.theme.clone()).or_else(|| p.masters.first().map(|m| m.theme.clone())).unwrap_or_default();
        let mut to = Out::new(&tname);
        let txml = theme::theme_xml(&mut x, &mut to, &theme);
        x.pkg.xml_part(&tname, CT_THEME, txml);
        x.pkg.rels(&tname, &to.rels);
        let mut w = W::new();
        root_open(&mut w, "p:notesMaster", A::new());
        let own = p.notes_master.as_deref();
        let bg = own
            .and_then(|m| m.background.clone())
            .unwrap_or(Background::Ref { idx: 1001, color: kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Bg1) });
        let shapes = own.map(|m| m.shapes.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| notes_master_shapes(p.notes_size));
        csld(&mut w, &mut x, &mut no, "", Some(&bg), &shapes, &[]);
        clr_map(&mut w, own.map(|m| m.color_map.as_slice()).unwrap_or_default());
        if let Some(hf) = own.and_then(|m| m.hf.as_deref()) {
            w.raw(hf);
        }
        let style = own.map(|m| m.body_style.clone()).filter(|s| !s.is_empty()).unwrap_or_else(notes_style);
        dml::list_style(&mut w, &mut x, &mut no, "p:notesStyle", &style);
        w.close("p:notesMaster");
        x.pkg.xml_part(notes_master, CT_NOTES_MASTER, w.finish());
        x.pkg.rels(notes_master, &no.rels);
        notes_master_rid = Some(pres.rels.add("notesMaster", &opc::relative("ppt/presentation.xml", notes_master), false));
    }

    // The file's handout master: its theme, background, shapes, colour map and switches.
    let mut handout_master_rid = None;
    if let Some(m) = p.handout_master.as_deref() {
        let name = "ppt/handoutMasters/handoutMaster1.xml";
        let tname = format!("ppt/theme/theme{}.xml", p.masters.len() + 1 + usize::from(has_notes));
        let mut ho = Out::new(name);
        ho.rels.add("theme", &opc::relative(name, &tname), false);
        let mut to = Out::new(&tname);
        let txml = theme::theme_xml(&mut x, &mut to, &m.theme);
        x.pkg.xml_part(&tname, CT_THEME, txml);
        x.pkg.rels(&tname, &to.rels);
        let mut w = W::new();
        root_open(&mut w, "p:handoutMaster", A::new());
        let bg = m
            .background
            .clone()
            .unwrap_or(Background::Ref { idx: 1001, color: kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Bg1) });
        csld(&mut w, &mut x, &mut ho, "", Some(&bg), &m.shapes, &[]);
        clr_map(&mut w, &m.color_map);
        if let Some(hf) = m.hf.as_deref() {
            w.raw(hf);
        }
        w.close("p:handoutMaster");
        x.pkg.xml_part(name, CT_HANDOUT_MASTER, w.finish());
        x.pkg.rels(name, &ho.rels);
        handout_master_rid = Some(pres.rels.add("handoutMaster", &opc::relative("ppt/presentation.xml", name), false));
    }

    // Comment authors. A slide any of whose comments came from a modern comment part, or whose
    // threads are resolved or anchored to shapes (which the legacy form cannot say), writes
    // modern comment parts.
    let mut authors: Vec<(String, String, u32)> = vec![];
    let modern = p
        .slides
        .iter()
        .any(|s| s.comments.iter().any(|c| c.shape.is_some() || c.resolved || c.is_modern() || c.replies.iter().any(Comment::is_modern)));
    let mut modern_authors: Vec<ModernAuthor> = vec![];
    let mut n_comments = 0u32;

    // Slides.
    let mut slide_rids = vec![];
    for (i, s) in p.slides.iter().enumerate() {
        let sname = format!("ppt/slides/slide{}.xml", i + 1);
        let mut so = Out::new(&sname);
        if let Some(lp) = layout_parts.get(&s.layout) {
            so.rels.add("slideLayout", &opc::relative(&sname, lp), false);
        }
        // The notes page: built for notes, or the empty page the file had, as it was.
        let kept_page = s.notes_page.as_deref().filter(|_| s.notes.is_empty());
        if !s.notes.is_empty() || kept_page.is_some() {
            let nname = format!("ppt/notesSlides/notesSlide{}.xml", i + 1);
            so.rels.add("notesSlide", &opc::relative(&sname, &nname), false);
            let mut no = Out::new(&nname);
            no.rels.add("notesMaster", &opc::relative(&nname, notes_master), false);
            no.rels.add("slide", &opc::relative(&nname, &sname), false);
            let xml = match kept_page {
                Some(page) => {
                    let mut w = W::new();
                    w.raw(page);
                    w.finish()
                }
                None => notes_xml(&mut x, &mut no, &s.notes),
            };
            x.pkg.xml_part(&nname, CT_NOTES, xml);
            x.pkg.rels(&nname, &no.rels);
        }
        if !s.comments.is_empty() {
            n_comments += 1;
            if modern {
                let sld_id = x.sld_ids[i] as usize;
                let cname = format!("ppt/comments/modernComment_{sld_id:X}_{:08X}.xml", fnv(&[&sname]) as u32);
                so.rels.add(RT_MODERN_COMMENTS, &opc::relative(&sname, &cname), false);
                let xml = modern_comments_xml(&s.comments, sld_id, i, &mut modern_authors);
                let state = modern_authors
                    .iter()
                    .map(|a| (a.guid.clone(), [a.name.clone(), a.initials.clone(), a.user_id.clone(), a.provider_id.clone()]))
                    .collect();
                x.pkg.xml_part(&cname, CT_MODERN_COMMENTS, crate::comments_ext::write(xml, &s.comments, &state)?);
            } else {
                let cname = format!("ppt/comments/comment{n_comments}.xml");
                so.rels.add("comments", &opc::relative(&sname, &cname), false);
                let xml = comments_xml(&s.comments, &mut authors);
                let state = authors
                    .iter()
                    .enumerate()
                    .map(|(i, (name, initials, _))| (i.to_string(), [name.clone(), initials.clone(), String::new(), String::new()]))
                    .collect();
                x.pkg.xml_part(&cname, CT_COMMENTS, crate::comments_ext::write(xml, &s.comments, &state)?);
            }
        }
        let xml = slide_xml(&mut x, &mut so, s);
        x.pkg.xml_part(&sname, CT_SLIDE, xml);
        x.pkg.rels(&sname, &so.rels);
        slide_rids.push(pres.rels.add("slide", &opc::relative("ppt/presentation.xml", &sname), false));
    }
    if !authors.is_empty() {
        let name = "ppt/commentAuthors.xml";
        let mut w = W::new();
        root_open(&mut w, "p:cmAuthorLst", A::new());
        for (i, (n, ini, last)) in authors.iter().enumerate() {
            w.empty("p:cmAuthor", A::new().a("id", i).a("name", n).a("initials", ini).a("lastIdx", last).a("clrIdx", i % 8));
        }
        w.close("p:cmAuthorLst");
        x.pkg.xml_part(name, CT_AUTHORS, w.finish());
        pres.rels.add("commentAuthors", "commentAuthors.xml", false);
    }
    if !modern_authors.is_empty() {
        let mut w = W::new();
        w.open("p188:authorLst", A::new().a("xmlns:a", NS_A).a("xmlns:r", NS_R).a("xmlns:p188", NS_P188));
        for a in &modern_authors {
            w.empty(
                "p188:author",
                A::new().a("id", &a.guid).a("name", &a.name).a("initials", &a.initials).a("userId", &a.user_id).a("providerId", &a.provider_id),
            );
        }
        w.close("p188:authorLst");
        x.pkg.xml_part("ppt/authors.xml", CT_MODERN_AUTHORS, w.finish());
        pres.rels.add(RT_MODERN_AUTHORS, "authors.xml", false);
    }

    // SlideFile-level parts.
    let theme1 = "theme/theme1.xml";
    pres.rels.add("theme", theme1, false);
    pres.rels.add("presProps", "presProps.xml", false);
    pres.rels.add("viewProps", "viewProps.xml", false);
    pres.rels.add("tableStyles", "tableStyles.xml", false);
    x.pkg.xml_part("ppt/presProps.xml", CT_PRES_PROPS, pres_props_xml(p));
    x.pkg.xml_part("ppt/viewProps.xml", CT_VIEW_PROPS, view_props_xml(p));
    let ts = crate::tables::table_styles_xml(&x.table_styles);
    x.pkg.xml_part("ppt/tableStyles.xml", CT_TABLE_STYLES, ts);
    let pxml = presentation_xml(&mut x, &mut pres, &master_rids, notes_master_rid.as_deref(), handout_master_rid.as_deref(), &slide_rids);
    x.pkg.xml_part("ppt/presentation.xml", CT_PRES, pxml);
    x.pkg.rels("ppt/presentation.xml", &pres.rels);

    // Document properties and package relationships.
    x.pkg.xml_part("docProps/core.xml", CT_CORE, core_xml(p));
    x.pkg.xml_part("docProps/app.xml", CT_APP, app_xml(p));
    let mut root = RelsOut::default();
    root.add("officeDocument", "ppt/presentation.xml", false);
    root.add(opc::RT_CORE, "docProps/core.xml", false);
    root.add("extended-properties", "docProps/app.xml", false);
    if let Some(custom) = p.props.custom_xml.as_deref().filter(|c| !c.is_empty()) {
        x.pkg.xml_part("docProps/custom.xml", CT_CUSTOM, custom.as_bytes().to_vec());
        root.add("custom-properties", "docProps/custom.xml", false);
    }
    x.pkg.rels("", &root);
    x.pkg.finish()
}

fn layout_xml(x: &mut Exp, o: &mut Out, l: &Layout) -> Vec<u8> {
    let mut w = W::new();
    root_open(
        &mut w,
        "p:sldLayout",
        A::new()
            .o("showMasterSp", (!l.show_master_shapes).then_some("0"))
            .a("type", l.kind.xml())
            .t("preserve", l.preserve)
            .t("userDrawn", l.user_drawn),
    );
    csld(&mut w, x, o, &l.name, l.background.as_ref(), &l.shapes, &l.tags);
    w.open0("p:clrMapOvr");
    w.empty0("a:masterClrMapping");
    w.close("p:clrMapOvr");
    if let Some(hf) = &l.hf {
        w.raw(hf);
    }
    let mut extensions = W::frag();
    crate::layout_id::write(&mut extensions, &l.identity);
    if let Some(raw) = ext_children(l.raw_ext.as_deref()) {
        extensions.raw(&raw);
    }
    ext_list(&mut w, Some(&extensions.s));
    w.close("p:sldLayout");
    w.finish()
}

fn master_xml(x: &mut Exp, o: &mut Out, m: &Master, layouts: &[(u64, String)]) -> Vec<u8> {
    let mut w = W::new();
    root_open(&mut w, "p:sldMaster", A::new().t("preserve", m.preserve));
    csld(&mut w, x, o, &m.name, m.background.as_ref(), &m.shapes, &m.tags);
    clr_map(&mut w, &m.color_map);
    w.open0("p:sldLayoutIdLst");
    for (id, rid) in layouts {
        w.empty("p:sldLayoutId", A::new().a("id", id).a("r:id", rid));
    }
    w.close("p:sldLayoutIdLst");
    if let Some(hf) = &m.hf {
        w.raw(hf);
    }
    w.open0("p:txStyles");
    let (dt, db, dother) = defaults::master_styles();
    let pick = |s: &kition_slide_model::text::ListStyle, d: kition_slide_model::text::ListStyle| if s.is_empty() { d } else { s.clone() };
    dml::list_style(&mut w, x, o, "p:titleStyle", &pick(&m.title_style, dt));
    dml::list_style(&mut w, x, o, "p:bodyStyle", &pick(&m.body_style, db));
    dml::list_style(&mut w, x, o, "p:otherStyle", &pick(&m.other_style, dother));
    w.close("p:txStyles");
    ext_list(&mut w, ext_children(m.raw_ext.as_deref()).as_deref());
    w.close("p:sldMaster");
    w.finish()
}

fn slide_xml(x: &mut Exp, o: &mut Out, s: &Slide) -> Vec<u8> {
    let mut w = W::new();
    root_open(&mut w, "p:sld", A::new().o("showMasterSp", (!s.show_master_shapes).then_some("0")).o("show", s.hidden.then_some("0")));
    let ids = csld(&mut w, x, o, &s.name, s.background.as_ref(), &s.shapes, &s.tags);
    w.open0("p:clrMapOvr");
    w.empty0("a:masterClrMapping");
    w.close("p:clrMapOvr");
    if let Some(t) = &s.transition {
        timing::transition(&mut w, x, o, t);
    }
    // The timing tree the slide was read with while its animations are unchanged; else built.
    if let Some(kept) = s.timing.as_ref().and_then(|k| timing::kept_xml(k, &s.animations, &ids)) {
        w.raw(&kept);
    } else if !s.animations.is_empty() {
        timing::timing(&mut w, &s.animations, &s.shapes, &ids);
    }
    let mut extensions = W::frag();
    crate::diagram_ids::write(&mut extensions, &s.shapes);
    if let Some(raw) = &s.raw_ext {
        extensions.raw(raw);
    }
    ext_list(&mut w, Some(&extensions.s));
    w.close("p:sld");
    w.finish()
}

fn notes_xml(x: &mut Exp, o: &mut Out, notes: &TextBody) -> Vec<u8> {
    let mut w = W::new();
    root_open(&mut w, "p:notes", A::new());
    let img = Shape {
        id: ShapeId(2),
        name: "Slide Image Placeholder 1".into(),
        ph: Some(Placeholder { kind: PhType::SlideImage, ..Default::default() }),
        ..Default::default()
    };
    let mut tb = notes.clone();
    tb.body = Default::default();
    let body = Shape {
        id: ShapeId(3),
        name: "Notes Placeholder 2".into(),
        ph: Some(Placeholder { kind: PhType::Body, idx: 1, ..Default::default() }),
        text: Some(tb),
        ..Default::default()
    };
    csld(&mut w, x, o, "", None, &[img, body], &[]);
    w.open0("p:clrMapOvr");
    w.empty0("a:masterClrMapping");
    w.close("p:clrMapOvr");
    w.close("p:notes");
    w.finish()
}

const CT_MODERN_COMMENTS: &str = "application/vnd.ms-powerpoint.comments+xml";
const CT_MODERN_AUTHORS: &str = "application/vnd.ms-powerpoint.authors+xml";
const RT_MODERN_COMMENTS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
const RT_MODERN_AUTHORS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";
const NS_P188: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";
const NS_P15: &str = "http://schemas.microsoft.com/office/powerpoint/2012/main";
const NS_PC: &str = "http://schemas.microsoft.com/office/powerpoint/2013/main/command";
const NS_AC: &str = "http://schemas.microsoft.com/office/drawing/2013/main/command";
/// The `p:ext` URI of a legacy comment's threading (`p15:threadingInfo`).
const EXT_THREADING: &str = "{C676402C-5697-4E1C-873F-D02D1690AC5C}";

/// FNV-1a over `parts` (with a separator), for names and ids derived from content.
fn fnv(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0xff)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

/// A `{GUID}` derived from `parts` (stable, so a saved file names the same comment the same way).
fn guid_of(parts: &[&str]) -> String {
    let a = fnv(parts);
    let b = fnv(&[&format!("{a:016x}"), "guid"]);
    let hex = format!("{a:016X}{b:016X}");
    let at = |from: usize, to: usize| hex.get(from..to).unwrap_or("0");
    format!("{{{}-{}-{}-{}-{}}}", at(0, 8), at(8, 12), at(12, 16), at(16, 20), at(20, 32))
}

fn comment_author(c: &Comment) -> String {
    if c.author.is_empty() { "Author".to_string() } else { c.author.clone() }
}

fn comment_initials(c: &Comment, author: &str) -> String {
    if c.initials.is_empty() { author.chars().filter(|c| c.is_alphabetic()).take(2).collect() } else { c.initials.clone() }
}

fn comments_xml(list: &[Comment], authors: &mut Vec<(String, String, u32)>) -> Vec<u8> {
    let mut w = W::new();
    root_open(&mut w, "p:cmLst", A::new());
    // Each comment, and the author index and idx of the comment its thread opened with (replies).
    let mut first: Option<(usize, u32)> = None;
    let mut flat: Vec<(&Comment, bool)> = vec![];
    for c in list.iter().take(10_000) {
        flat.push((c, false));
        for r in c.replies.iter().take(1000) {
            flat.push((r, true));
        }
    }
    for (c, reply) in flat {
        let author = comment_author(c);
        let ai = match authors.iter().position(|(n, _, _)| *n == author) {
            Some(i) => i,
            None => {
                authors.push((author.clone(), comment_initials(c, &author), 0));
                authors.len() - 1
            }
        };
        let idx = match authors.get_mut(ai) {
            Some(a) => {
                a.2 += 1;
                a.2
            }
            None => 1,
        };
        if !reply {
            first = Some((ai, idx));
        }
        let dt = if valid_date(&c.date) { c.date.clone() } else { "2024-01-01T00:00:00.000".into() };
        w.open("p:cm", A::new().a("authorId", ai).a("dt", dt).a("idx", idx));
        let pos = |v: f64| if v.is_finite() { (v * 8.0).round().clamp(0.0, 1.0e9) as i64 } else { 0 };
        w.empty("p:pos", A::new().a("x", pos(c.x)).a("y", pos(c.y)));
        w.elt("p:text", &c.text);
        let kept_ext = c.kept.as_ref().and_then(|k| k.ext_xml.as_deref());
        let parent = if reply { first } else { None };
        if parent.is_some() || kept_ext.is_some() {
            w.open0("p:extLst");
            if let Some((pa, pi)) = parent {
                w.open("p:ext", A::new().a("uri", EXT_THREADING));
                let tz = c.kept.as_ref().and_then(|k| k.tz);
                w.open("p15:threadingInfo", A::new().a("xmlns:p15", NS_P15).o("timeZoneBias", tz));
                w.empty("p15:parentCm", A::new().a("authorId", pa).a("idx", pi));
                w.close("p15:threadingInfo");
                w.close("p:ext");
            }
            if let Some(ext) = kept_ext {
                w.raw(ext);
            }
            w.close("p:extLst");
        }
        w.close("p:cm");
    }
    w.close("p:cmLst");
    w.finish()
}

/// A modern comment author: as read (its GUID kept), or one per name.
struct ModernAuthor {
    guid: String,
    name: String,
    initials: String,
    user_id: String,
    provider_id: String,
}

fn modern_author(c: &Comment, authors: &mut Vec<ModernAuthor>) -> String {
    let kept = c.kept.as_deref();
    let name = comment_author(c);
    let guid = kept.map(|k| k.author_guid.clone()).filter(|g| !g.is_empty()).unwrap_or_else(|| guid_of(&["author", &name]));
    if !authors.iter().any(|a| a.guid == guid) {
        authors.push(ModernAuthor {
            guid: guid.clone(),
            initials: comment_initials(c, &name),
            name,
            user_id: kept.map(|k| k.user_id.clone()).unwrap_or_default(),
            provider_id: kept.map(|k| k.provider_id.clone()).unwrap_or_default(),
        });
    }
    guid
}

/// A kept modern anchor with its slide marker naming the slide as written now.
fn modern_anchor(kept: &str, sld_id: usize) -> Option<String> {
    let mut doc = crate::xml::parse(kept.as_bytes()).ok()?;
    fn rec(e: &mut crate::xml::El, sld_id: usize, depth: usize) {
        if depth > 16 {
            return;
        }
        if e.local() == "sldMk" {
            e.set_attr("sldId", &sld_id.to_string());
        }
        for c in e.elements_mut() {
            rec(c, sld_id, depth + 1);
        }
    }
    rec(&mut doc.root, sld_id, 0);
    Some(doc.root.to_xml())
}

fn modern_body_xml(w: &mut W, c: &Comment) {
    if let Some(k) = c.kept.as_deref()
        && let (Some(xml), Some(text)) = (&k.body_xml, &k.body_text)
        && *text == c.text
    {
        w.raw(xml);
        return;
    }
    w.open0("p188:txBody");
    w.empty0("a:bodyPr");
    w.empty0("a:lstStyle");
    for line in c.text.split('\n') {
        w.open0("a:p");
        if !line.is_empty() {
            w.open0("a:r");
            w.elt("a:t", line);
            w.close("a:r");
        }
        w.close("a:p");
    }
    w.close("p188:txBody");
}

fn modern_attrs(c: &Comment, author: String, parts: &[&str]) -> A {
    let kept = c.kept.as_deref();
    let guid = kept.map(|k| k.guid.clone()).filter(|g| !g.is_empty()).unwrap_or_else(|| guid_of(parts));
    let created = if valid_date(&c.date) { c.date.clone() } else { "2024-01-01T00:00:00.000".into() };
    let status = match kept.map(|k| k.status.as_str()).filter(|s| !s.is_empty()) {
        Some(status) => Some(status.to_string()),
        None => c.resolved.then(|| "resolved".to_string()),
    };
    A::new().a("id", guid).a("authorId", author).a("created", created).o("status", status)
}

fn modern_comments_xml(list: &[Comment], sld_id: usize, slide: usize, authors: &mut Vec<ModernAuthor>) -> Vec<u8> {
    let mut w = W::new();
    w.open("p188:cmLst", A::new().a("xmlns:a", NS_A).a("xmlns:r", NS_R).a("xmlns:p188", NS_P188));
    let slide_key = slide.to_string();
    for (i, c) in list.iter().take(10_000).enumerate() {
        let author = modern_author(c, authors);
        let key = i.to_string();
        w.open("p188:cm", modern_attrs(c, author, &["cm", &slide_key, &key, &c.date, &c.author, &c.text]));
        // A kept anchor names a shape (`spMk`) exactly when the comment still has one.
        let kept_anchor = c.kept.as_deref().and_then(|k| k.anchor_xml.as_deref()).filter(|a| a.contains("spMk") == c.shape.is_some());
        match kept_anchor.and_then(|a| modern_anchor(a, sld_id)) {
            Some(anchor) => w.raw(&anchor),
            None => match c.shape {
                Some(shape) => {
                    w.open("ac:deMkLst", A::new().a("xmlns:ac", NS_AC));
                    w.empty("pc:docMk", A::new().a("xmlns:pc", NS_PC));
                    w.empty("pc:sldMk", A::new().a("xmlns:pc", NS_PC).a("cId", 0).a("sldId", sld_id));
                    w.empty("ac:spMk", A::new().a("id", shape.0));
                    w.close("ac:deMkLst");
                }
                None => {
                    w.open("pc:sldMkLst", A::new().a("xmlns:pc", NS_PC));
                    w.empty0("pc:docMk");
                    w.empty("pc:sldMk", A::new().a("cId", 0).a("sldId", sld_id));
                    w.close("pc:sldMkLst");
                }
            },
        }
        let emu = |v: f64| if v.is_finite() { (v * 12_700.0).round().clamp(0.0, 1.0e12) as i64 } else { 0 };
        w.empty("p188:pos", A::new().a("x", emu(c.x)).a("y", emu(c.y)));
        if !c.replies.is_empty() {
            w.open0("p188:replyLst");
            for (j, r) in c.replies.iter().take(1000).enumerate() {
                let author = modern_author(r, authors);
                let reply = j.to_string();
                w.open("p188:reply", modern_attrs(r, author, &["reply", &slide_key, &key, &reply, &r.date, &r.author, &r.text]));
                modern_body_xml(&mut w, r);
                if let Some(ext) = r.kept.as_deref().and_then(|k| k.ext_xml.as_deref()) {
                    w.raw(ext);
                }
                w.close("p188:reply");
            }
            w.close("p188:replyLst");
        }
        modern_body_xml(&mut w, c);
        if let Some(ext) = c.kept.as_deref().and_then(|k| k.ext_xml.as_deref()) {
            w.raw(ext);
        }
        w.close("p188:cm");
    }
    w.close("p188:cmLst");
    w.finish()
}

/// Sections with every slide in exactly one section, in slide order.
fn section_slides(p: &SlideFile) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = p.sections.iter().map(|s| (s.name.clone(), vec![])).collect();
    if out.is_empty() {
        return out;
    }
    let mut cur = 0usize;
    for (i, s) in p.slides.iter().enumerate() {
        if let Some(k) = p.sections.iter().position(|sec| sec.slides.contains(&s.id))
            && k >= cur
        {
            cur = k;
        }
        if let Some(sec) = out.get_mut(cur) {
            sec.1.push(i);
        }
    }
    out
}

fn presentation_xml(x: &mut Exp, o: &mut Out, masters: &[(u64, String)], notes: Option<&str>, handout: Option<&str>, slides: &[String]) -> Vec<u8> {
    let p = x.p;
    let mut w = W::new();
    let first = (p.first_slide_number != 1).then_some(p.first_slide_number.min(9999));
    // Kept embedded fonts, their references renumbered for this part.
    let fonts = p.embedded_font_list.as_deref().and_then(|xml| crate::xml::parse(xml.as_bytes()).ok()).and_then(|mut doc| {
        (doc.root.local() == "embeddedFontLst" && shapes::relink(x, o, &mut doc.root, &p.embedded_font_rels)).then(|| doc.root.to_xml())
    });
    root_open(
        &mut w,
        "p:presentation",
        A::new().a("saveSubsetFonts", 1).o("embedTrueTypeFonts", fonts.is_some().then_some(1)).o("firstSlideNum", first),
    );
    w.open0("p:sldMasterIdLst");
    for (id, rid) in masters {
        w.empty("p:sldMasterId", A::new().a("id", id).a("r:id", rid));
    }
    w.close("p:sldMasterIdLst");
    if let Some(n) = notes {
        w.open0("p:notesMasterIdLst");
        w.empty("p:notesMasterId", A::new().a("r:id", n));
        w.close("p:notesMasterIdLst");
    }
    if let Some(h) = handout {
        w.open0("p:handoutMasterIdLst");
        w.empty("p:handoutMasterId", A::new().a("r:id", h));
        w.close("p:handoutMasterIdLst");
    }
    if !slides.is_empty() {
        w.open0("p:sldIdLst");
        for (i, rid) in slides.iter().enumerate() {
            w.empty("p:sldId", A::new().a("id", x.sld_ids[i]).a("r:id", rid));
        }
        w.close("p:sldIdLst");
    }
    let clamp = |v: f64| dml::emu(v).clamp(914_400, 51_206_400);
    w.empty("p:sldSz", A::new().a("cx", clamp(p.slide_size.width)).a("cy", clamp(p.slide_size.height)));
    w.empty("p:notesSz", A::new().a("cx", dml::emu_pos(p.notes_size.width).max(1)).a("cy", dml::emu_pos(p.notes_size.height).max(1)));
    if let Some(fonts) = &fonts {
        w.raw(fonts);
    }
    if !p.custom_shows.is_empty() {
        w.open0("p:custShowLst");
        for (i, cs) in p.custom_shows.iter().enumerate() {
            w.open("p:custShow", A::new().a("name", &cs.name).a("id", i));
            w.open0("p:sldLst");
            for sid in &cs.slides {
                if let Some(part) = x.slide_parts.get(sid).cloned() {
                    let rid = o.rels.add("slide", &opc::relative("ppt/presentation.xml", &part), false);
                    w.empty("p:sld", A::new().a("r:id", rid));
                }
            }
            w.close("p:sldLst");
            w.close("p:custShow");
        }
        w.close("p:custShowLst");
    }
    cust_data(&mut w, x, o, &p.tags);
    let dts = if p.default_text_style.is_empty() { defaults::default_text_style() } else { p.default_text_style.clone() };
    dml::list_style(&mut w, x, o, "p:defaultTextStyle", &dts);
    let sections = section_slides(p);
    let raw = p.raw_ext.as_deref().filter(|r| !r.trim().is_empty());
    if !sections.is_empty() || raw.is_some() || !p.font_faces.is_empty() {
        let mut inner = W::frag();
        crate::font_faces_ext::write(&mut inner, &p.font_faces);
        if !sections.is_empty() {
            inner.open("p:ext", A::new().a("uri", crate::SECTION_EXT_URI));
            inner.open("p14:sectionLst", A::new().a("xmlns:p14", NS_P14));
            for (i, (name, list)) in sections.iter().enumerate() {
                inner.open(
                    "p14:section",
                    A::new().a("name", name).a(
                        "id",
                        p.sections
                            .get(i)
                            .and_then(|section| kition_slide_model::identity::guid(&section.identity))
                            .unwrap_or_else(|| crate::tables::guid(&format!("section:{i}:{name}"))),
                    ),
                );
                inner.open0("p14:sldIdLst");
                for si in list {
                    inner.empty("p14:sldId", A::new().a("id", x.sld_ids[*si]));
                }
                inner.close("p14:sldIdLst");
                inner.close("p14:section");
            }
            inner.close("p14:sectionLst");
            inner.close("p:ext");
        }
        if let Some(r) = raw {
            inner.raw(r);
        }
        ext_list(&mut w, Some(&inner.s));
    }
    w.close("p:presentation");
    w.finish()
}

fn pres_props_xml(p: &SlideFile) -> Vec<u8> {
    let mut show = W::frag();
    show_pr(&mut show, p);
    // The kept properties with the show settings in their place (after the print settings).
    if let Some(kept) = p.pres_props.as_deref()
        && let Ok(mut doc) = crate::xml::parse(kept.as_bytes())
        && doc.root.is("presentationPr")
        && let Ok(sp) = crate::xml::parse(show.s.as_bytes())
    {
        let at = doc
            .root
            .children
            .iter()
            .take_while(|c| matches!(c, crate::xml::Node::El(e) if matches!(e.local(), "htmlPubPr" | "webPr" | "prnPr")))
            .count();
        doc.root.children.insert(at, crate::xml::Node::El(sp.root));
        let mut w = W::new();
        w.raw(&doc.root.to_xml());
        return w.finish();
    }
    let mut w = W::new();
    root_open(&mut w, "p:presentationPr", A::new());
    w.raw(&show.s);
    w.close("p:presentationPr");
    w.finish()
}

fn show_pr(w: &mut W, p: &SlideFile) {
    let s = &p.show;
    w.open(
        "p:showPr",
        A::new()
            .t("loop", s.loop_until_esc)
            .a("showNarration", if s.without_narration { 0 } else { 1 })
            .o("showAnimation", s.without_animation.then_some("0"))
            .o("useTimings", (!s.use_timings).then_some("0")),
    );
    match s.show_type.as_str() {
        "browsed" => w.empty("p:browse", A::new()),
        "kiosk" => w.empty("p:kiosk", A::new()),
        _ => w.empty0("p:present"),
    }
    let custom = s.custom_show.as_ref().and_then(|n| p.custom_shows.iter().position(|c| c.name == *n));
    match (custom, s.range) {
        (Some(i), _) => w.empty("p:custShow", A::new().a("id", i)),
        (None, Some((a, b))) => w.empty("p:sldRg", A::new().a("st", a.max(1)).a("end", b.max(a).max(1))),
        _ => w.empty0("p:sldAll"),
    }
    w.open0("p:penClr");
    w.val("a:srgbClr", s.pen_color.hex());
    w.close("p:penClr");
    w.close("p:showPr");
}

fn view_props_xml(p: &SlideFile) -> Vec<u8> {
    let mut w = W::new();
    if let Some(kept) = p.view_props.as_deref()
        && crate::xml::parse(kept.as_bytes()).is_ok_and(|d| d.root.is("viewPr"))
    {
        w.raw(kept);
        return w.finish();
    }
    root_open(&mut w, "p:viewPr", A::new());
    w.open0("p:normalViewPr");
    w.empty("p:restoredLeft", A::new().a("sz", 15620));
    w.empty("p:restoredTop", A::new().a("sz", 94660));
    w.close("p:normalViewPr");
    w.empty("p:gridSpacing", A::new().a("cx", 76200).a("cy", 76200));
    w.close("p:viewPr");
    w.finish()
}

fn core_xml(p: &SlideFile) -> Vec<u8> {
    let pr = &p.props;
    let mut w = W::new();
    w.open(
        "cp:coreProperties",
        A::new()
            .a("xmlns:cp", "http://schemas.openxmlformats.org/package/2006/metadata/core-properties")
            .a("xmlns:dc", "http://purl.org/dc/elements/1.1/")
            .a("xmlns:dcterms", "http://purl.org/dc/terms/")
            .a("xmlns:dcmitype", "http://purl.org/dc/dcmitype/")
            .a("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    );
    for (tag, v) in [
        ("dc:title", &pr.title),
        ("dc:subject", &pr.subject),
        ("dc:creator", &pr.author),
        ("cp:keywords", &pr.keywords),
        ("dc:description", &pr.comments),
        ("cp:category", &pr.category),
        ("cp:lastModifiedBy", &pr.last_modified_by),
    ] {
        if !v.is_empty() {
            w.elt(tag, v);
        }
    }
    if pr.revision > 0 {
        w.elt("cp:revision", &pr.revision.to_string());
    }
    for (tag, v) in [("dcterms:created", &pr.created), ("dcterms:modified", &pr.modified)] {
        if valid_date(v) {
            w.open(tag, A::new().a("xsi:type", "dcterms:W3CDTF"));
            w.text(v);
            w.close(tag);
        }
    }
    w.close("cp:coreProperties");
    w.finish()
}

fn app_xml(p: &SlideFile) -> Vec<u8> {
    let stats = AppStats::of(p);
    let mut w = W::new();
    w.open(
        "Properties",
        A::new()
            .a("xmlns", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties")
            .a("xmlns:vt", "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"),
    );
    w.elt("Words", &stats.words.to_string());
    w.elt("Application", "Kition Slide");
    if let Some(f) = presentation_format(p.slide_size) {
        w.elt("PresentationFormat", f);
    }
    w.elt("Paragraphs", &stats.paragraphs.to_string());
    w.elt("Slides", &p.slides.len().to_string());
    w.elt("Notes", &p.slides.iter().filter(|s| !s.notes.is_empty()).count().to_string());
    w.elt("HiddenSlides", &p.slides.iter().filter(|s| s.hidden).count().to_string());
    w.elt("MMClips", &stats.clips.to_string());
    w.elt("ScaleCrop", "false");
    // What the slide is made of, as Office lists it in the file's properties.
    let groups: Vec<(&str, &[String])> =
        [("Fonts Used", stats.fonts.as_slice()), ("Theme", stats.themes.as_slice()), ("Slide Titles", stats.titles.as_slice())]
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .collect();
    w.open0("HeadingPairs");
    w.open("vt:vector", A::new().a("size", groups.len() * 2).a("baseType", "variant"));
    for (name, items) in &groups {
        w.open0("vt:variant");
        w.elt("vt:lpstr", name);
        w.close("vt:variant");
        w.open0("vt:variant");
        w.elt("vt:i4", &items.len().to_string());
        w.close("vt:variant");
    }
    w.close("vt:vector");
    w.close("HeadingPairs");
    w.open0("TitlesOfParts");
    w.open("vt:vector", A::new().a("size", groups.iter().map(|(_, v)| v.len()).sum::<usize>()).a("baseType", "lpstr"));
    for item in groups.iter().flat_map(|(_, v)| v.iter()) {
        w.elt("vt:lpstr", item);
    }
    w.close("vt:vector");
    w.close("TitlesOfParts");
    if !p.props.company.is_empty() {
        w.elt("Company", &p.props.company);
    }
    w.elt("LinksUpToDate", "false");
    w.elt("SharedDoc", "false");
    w.elt("HyperlinksChanged", "false");
    w.close("Properties");
    w.finish()
}

/// The statistics `docProps/app.xml` gives (computed from the slide, as Office does on save).
#[derive(Default)]
struct AppStats {
    words: usize,
    paragraphs: usize,
    clips: usize,
    fonts: Vec<String>,
    themes: Vec<String>,
    titles: Vec<String>,
}

impl AppStats {
    fn of(p: &SlideFile) -> Self {
        let mut st = AppStats::default();
        let theme = p.masters.first().map(|m| &m.theme);
        let font = |st: &mut AppStats, name: &str| {
            let name = match theme {
                Some(t) if name.starts_with('+') => t.font(name),
                _ => name.to_string(),
            };
            if !name.is_empty() && !name.starts_with('+') && !st.fonts.contains(&name) && st.fonts.len() < 1000 {
                st.fonts.push(name);
            }
        };
        if let Some(t) = theme {
            font(&mut st, &t.fonts.major.latin);
            font(&mut st, &t.fonts.minor.latin);
        }
        for s in &p.slides {
            kition_slide_model::walk(&s.shapes, &mut |sh, _| {
                if matches!(sh.kind, kition_slide_model::ShapeKind::Media(_)) {
                    st.clips += 1;
                }
                let Some(t) = &sh.text else { return };
                for para in &t.paragraphs {
                    let text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
                    if !text.trim().is_empty() {
                        st.paragraphs += 1;
                        st.words += text.split_whitespace().count();
                    }
                    for r in &para.runs {
                        if let Some(f) = r.props.font.as_deref() {
                            font(&mut st, f);
                        }
                    }
                }
            });
            st.titles.push(s.title());
        }
        for m in &p.masters {
            if !st.themes.contains(&m.theme.name) {
                st.themes.push(m.theme.name.clone());
            }
        }
        st
    }
}

/// The slide size's name in Office's list, for a size it names.
fn presentation_format(size: kition_slide_geom::Size) -> Option<&'static str> {
    let (w, h) = (dml::emu_pos(size.width), dml::emu_pos(size.height));
    let near = |a: i64, b: i64| (a - b).abs() <= 2000;
    [
        (12_192_000, 6_858_000, "Widescreen"),
        (9_144_000, 6_858_000, "On-screen Show (4:3)"),
        (9_144_000, 5_143_500, "On-screen Show (16:9)"),
        (9_144_000, 5_715_000, "On-screen Show (16:10)"),
    ]
    .into_iter()
    .find(|(cw, ch, _)| near(w, *cw) && near(h, *ch))
    .map(|(_, _, n)| n)
}
