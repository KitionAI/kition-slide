//! PPTX import: package → [`SlideFile`].

mod chart;
pub(crate) mod dml;
pub(crate) mod shapes;
mod theme;
pub(crate) mod timing;

pub(crate) use chart::{chart_from as chart_from_el, kept_marker_pictures, picture_fills, read_chart_part, theme_override_scheme};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kition_slide_geom::{Size, emu_to_pt};
use kition_slide_model::text::TextBody;
use kition_slide_model::{
    Background, Comment, CustomShow, HeaderFooter, Layout, LayoutId, LayoutType, Master, MasterId, MediaId, MediaItem, PhType, Properties, Section,
    ShapeId, ShowSettings, Slide, SlideFile, SlideId, defaults,
};

use crate::PptxError;
use crate::opc::{self, Package, Rels};
use crate::xml::{Doc, El, Node};

/// Import state shared by all parts.
pub struct Imp<'a> {
    pub pkg: &'a Package,
    pub media: Vec<MediaItem>,
    pub media_by_part: HashMap<String, MediaId>,
    pub next: u32,
    pub slide_by_part: HashMap<String, SlideId>,
    pub custom_show_names: HashMap<String, String>,
    /// How often each (slot, typeface) came with each set of face attributes.
    pub faces: HashMap<(String, String), HashMap<FaceAttrs, usize>>,
}

/// A typeface's PANOSE number, character set and pitch and family, as a file gives them.
pub type FaceAttrs = (Option<String>, Option<String>, Option<String>);

/// Most distinct typefaces whose face attributes are kept.
const MAX_FACES: usize = 4096;

impl Imp<'_> {
    pub fn alloc(&mut self) -> u32 {
        self.next = self.next.saturating_add(1);
        self.next
    }
    /// Note the face attributes of a typeface named by `e` (`a:latin`, `a:buFont`…).
    pub fn note_face(&mut self, slot: &str, e: &El) {
        let Some(typeface) = e.attr("typeface").filter(|t| !t.is_empty()) else { return };
        let attrs: FaceAttrs = (e.attr("panose").map(String::from), e.attr("charset").map(String::from), e.attr("pitchFamily").map(String::from));
        let key = (slot.to_string(), typeface.to_string());
        if self.faces.len() >= MAX_FACES && !self.faces.contains_key(&key) {
            return;
        }
        let seen = self.faces.entry(key).or_default();
        if seen.len() < 16 {
            *seen.entry(attrs).or_default() += 1;
        }
    }
    /// Media item for an internal part, read once.
    pub fn media_for(&mut self, part: &str) -> Option<MediaId> {
        let key = opc::norm(part);
        if let Some(m) = self.media_by_part.get(&key) {
            return Some(*m);
        }
        let mut data = self.pkg.get(part)?.to_vec();
        let name = opc::file_of(self.pkg.original_name(part).unwrap_or(part)).to_string();
        let content_type = self.pkg.content_type(part).filter(|c| !c.is_empty()).unwrap_or_else(|| crate::mime_for(&name).to_string());
        // The pixel the writer gives an image with no bytes, under a name that is no PNG's: the
        // image had none (`write::media_part`).
        if !content_type.contains("png") && data == crate::png::transparent_1x1() {
            data.clear();
        }
        let id = MediaId(self.alloc());
        self.media.push(MediaItem { id, name, content_type, data: Arc::new(data), link: None });
        self.media_by_part.insert(key, id);
        Some(id)
    }
    /// Media item for an external link.
    pub fn media_link(&mut self, url: &str) -> MediaId {
        if let Some(m) = self.media.iter().find(|m| m.link.as_deref() == Some(url)) {
            return m.id;
        }
        let id = MediaId(self.alloc());
        let name = url.rsplit('/').next().unwrap_or(url).to_string();
        let content_type = crate::mime_for(&name).to_string();
        self.media.push(MediaItem { id, name, content_type, data: Arc::new(vec![]), link: Some(url.to_string()) });
        id
    }
}

/// One part being read: its name, relationships and root namespace declarations.
pub struct Part {
    pub name: String,
    pub rels: Rels,
    pub ns: Vec<(String, String)>,
}

impl Part {
    pub fn new(pkg: &Package, name: &str, doc: &Doc) -> Self {
        Part { name: name.to_string(), rels: pkg.rels(name), ns: doc.ns_decls() }
    }
    /// Serialize an element with the namespace declarations it needs from the part root.
    pub fn keep(&self, el: &El) -> String {
        keep_xml(el, &self.ns)
    }
}

/// Serialize `el`, declaring every prefix it uses that the source root declared.
pub fn keep_xml(el: &El, ns: &[(String, String)]) -> String {
    let mut used = HashSet::new();
    fn rec(e: &El, used: &mut HashSet<String>, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        if let Some((p, _)) = e.name.split_once(':') {
            used.insert(p.to_string());
        }
        for (k, _) in &e.attrs {
            if let Some((p, _)) = k.split_once(':')
                && p != "xmlns"
                && p != "xml"
            {
                used.insert(p.to_string());
            }
        }
        for c in e.elements() {
            rec(c, used, depth + 1);
        }
    }
    rec(el, &mut used, 0);
    let mut el = el.clone();
    for (k, v) in ns {
        let p = k.strip_prefix("xmlns:").unwrap_or("");
        if !p.is_empty() && used.contains(p) && !el.attrs.iter().any(|(a, _)| a == k) {
            el.attrs.push((k.clone(), v.clone()));
        }
    }
    // A declaration inside that repeats one of the root's says nothing: dropped, so XML kept,
    // written and read again is kept as it was (C8-slide-5: a transition read from its own file
    // gained a repeated `xmlns:p` on each read).
    fn drop_repeats(e: &mut El, declared: &[(String, String)], depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        for child in e.elements_mut() {
            child.attrs.retain(|attr| !(attr.0.starts_with("xmlns") && declared.contains(attr)));
            drop_repeats(child, declared, depth + 1);
        }
    }
    let declared: Vec<(String, String)> = el.attrs.iter().filter(|(k, _)| k.starts_with("xmlns")).cloned().collect();
    drop_repeats(&mut el, &declared, 0);
    el.to_xml()
}

/// Namespace prefixes whose `mc:Choice` content we understand.
const SUPPORTED_MC: [&str; 9] = ["p14", "p15", "p159", "a14", "a16", "p1510", "a15", "c14", "p188"];

pub(super) fn supported_mc_choice(ch: &El) -> bool {
    ch.attr("Requires").unwrap_or("").split_whitespace().all(|r| SUPPORTED_MC.contains(&r))
}

/// Resolve ordinary alternatives; defer ink choices until the linked part is validated.
pub fn resolve_mc(el: &mut El, depth: usize) {
    if depth > crate::xml::MAX_DEPTH {
        return;
    }
    if el.children.iter().any(|c| matches!(c, Node::El(e) if e.is("AlternateContent"))) {
        let mut out = Vec::with_capacity(el.children.len());
        for c in std::mem::take(&mut el.children) {
            match c {
                Node::El(mut ac) if ac.is("AlternateContent") => {
                    let choice = ac.children_named("Choice").find(|ch| supported_mc_choice(ch));
                    if choice.is_some_and(|ch| ch.elements().any(|e| e.is("contentPart"))) {
                        let inherited = ac.attrs.clone();
                        for branch in ac.elements_mut() {
                            let ns: Vec<_> = branch.attrs.iter().chain(inherited.iter()).filter(|(k, _)| k.starts_with("xmlns")).cloned().collect();
                            for child in branch.elements_mut() {
                                for (key, value) in &ns {
                                    if !child.attrs.iter().any(|(k, _)| k == key) {
                                        child.attrs.push((key.clone(), value.clone()));
                                    }
                                }
                            }
                        }
                        out.push(Node::El(ac));
                        continue;
                    }
                    let branch = choice.or_else(|| ac.child("Fallback"));
                    if let Some(b) = branch {
                        for n in &b.children {
                            if let Node::El(e) = n {
                                let mut e = e.clone();
                                // Carry namespace declarations from the branch onto its content.
                                for (k, v) in b.attrs.iter().chain(ac.attrs.iter()) {
                                    if k.starts_with("xmlns") && !e.attrs.iter().any(|(a, _)| a == k) {
                                        e.attrs.push((k.clone(), v.clone()));
                                    }
                                }
                                out.push(Node::El(e));
                            }
                        }
                    }
                }
                other => out.push(other),
            }
        }
        el.children = out;
    }
    for c in el.elements_mut() {
        resolve_mc(c, depth + 1);
    }
}

fn load(pkg: &Package, name: &str) -> Option<Doc> {
    let mut d = pkg.xml(name)?;
    resolve_mc(&mut d.root, 0);
    Some(d)
}

/// Every shape id (`cNvPr/@id`) under `el` that the reader keeps.
fn cnvpr_ids(el: &El, ids: &mut std::collections::HashSet<u32>) {
    let mut v = vec![];
    el.find_all("cNvPr", &mut v);
    ids.extend(v.into_iter().filter_map(|c| c.u32("id")).filter(|id| *id < shapes::MAX_KEPT_ID));
}

fn max_cnvpr(el: &El, m: &mut u32) {
    let mut v = vec![];
    el.find_all("cNvPr", &mut v);
    for c in v {
        if let Some(id) = c.u32("id")
            && id < shapes::MAX_KEPT_ID
        {
            *m = (*m).max(id);
        }
    }
}

pub fn import(bytes: &[u8]) -> Result<SlideFile, PptxError> {
    let pkg = Package::open(bytes)?;
    let root_rels = pkg.rels("");
    let pres_path = root_rels.first_of("officeDocument").map(|r| r.target.clone()).unwrap_or_else(|| "ppt/presentation.xml".into());
    let pres_doc = load(&pkg, &pres_path).ok_or_else(|| PptxError::NotPptx(format!("missing or unreadable {pres_path}")))?;
    let pres_el = &pres_doc.root;
    if !pres_el.is("presentation") {
        return Err(PptxError::NotPptx(format!("{pres_path} is not a slide part")));
    }
    let pres_part = Part::new(&pkg, &pres_path, &pres_doc);
    let authors = read_authors(&pkg, &pres_part);

    // Masters, layouts and slides to read.
    let master_paths: Vec<String> = pres_el
        .child("sldMasterIdLst")
        .map(|l| l.children_named("sldMasterId").filter_map(|m| pres_part.rels.target(m.attr("r:id").unwrap_or(""))).map(String::from).collect())
        .unwrap_or_default();
    let master_paths: Vec<String> =
        if master_paths.is_empty() { pres_part.rels.all_of("slideMaster").map(|r| r.target.clone()).collect() } else { master_paths };
    let slide_entries: Vec<(u32, String)> = pres_el
        .child("sldIdLst")
        .map(|l| {
            l.children_named("sldId")
                .take(10_000)
                .filter_map(|s| Some((s.u32("id").unwrap_or(0), pres_part.rels.target(s.attr("r:id").unwrap_or(""))?.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let mut docs: HashMap<String, Doc> = HashMap::new();
    let mut max_id = 0u32;
    let mut shape_ids = std::collections::HashSet::new();
    let mut all_parts: Vec<String> = master_paths.clone();
    for m in &master_paths {
        all_parts.extend(pkg.rels(m).all_of("slideLayout").map(|r| r.target.clone()));
    }
    all_parts.extend(slide_entries.iter().map(|(_, p)| p.clone()));
    for p in &all_parts {
        let key = opc::norm(p);
        if docs.contains_key(&key) {
            continue;
        }
        if let Some(d) = load(&pkg, p) {
            max_cnvpr(&d.root, &mut max_id);
            cnvpr_ids(&d.root, &mut shape_ids);
            let reserved = crate::diagram_ids::reserved(&d.root);
            max_id = reserved.iter().copied().max().unwrap_or(0).max(max_id);
            shape_ids.extend(reserved);
            docs.insert(key, d);
        }
    }

    let mut imp = Imp {
        pkg: &pkg,
        media: vec![],
        media_by_part: HashMap::new(),
        next: max_id.max(1),
        slide_by_part: HashMap::new(),
        custom_show_names: HashMap::new(),
        faces: HashMap::new(),
    };
    // Slide ids first so hyperlinks anywhere can point at slides. A slide keeps its file id
    // (`p:sldId/@id`) when it is valid, unique and no shape's id, so an id read from one copy of
    // the file names the same slide after the file is changed elsewhere and read again; otherwise
    // it gets a fresh id. Fresh ids start above every kept one.
    let mut kept_ids = std::collections::HashSet::new();
    let kept: Vec<Option<u32>> = slide_entries
        .iter()
        .map(|(file_id, path)| {
            (docs.contains_key(&opc::norm(path))
                && (256..=2_147_483_647).contains(file_id)
                && !shape_ids.contains(file_id)
                && kept_ids.insert(*file_id))
            .then_some(*file_id)
        })
        .collect();
    imp.next = imp.next.max(kept.iter().flatten().copied().max().unwrap_or(0));
    let mut slide_ids: Vec<(u32, String, SlideId)> = vec![];
    for ((file_id, path), kept) in slide_entries.iter().zip(kept) {
        if !docs.contains_key(&opc::norm(path)) {
            log::warn!("pptx: slide {path} is missing or unreadable; skipped");
            continue;
        }
        let id = SlideId(kept.unwrap_or_else(|| imp.alloc()));
        imp.slide_by_part.insert(opc::norm(path), id);
        slide_ids.push((*file_id, path.clone(), id));
    }
    if let Some(l) = pres_el.child("custShowLst") {
        for cs in l.children_named("custShow") {
            if let (Some(id), Some(name)) = (cs.attr("id"), cs.attr("name")) {
                imp.custom_show_names.insert(id.to_string(), name.to_string());
            }
        }
    }

    let mut p = defaults::blank_slide_file(defaults::WIDE, Default::default(), false);
    p.masters.clear();
    p.media.clear();
    p.default_text_style = Default::default();

    if let Some(sz) = pres_el.child("sldSz") {
        let w = sz.i64("cx").map(emu_to_pt).unwrap_or(960.0);
        let h = sz.i64("cy").map(emu_to_pt).unwrap_or(540.0);
        p.slide_size = Size::new(sane_len(w, 960.0), sane_len(h, 540.0));
    }
    if let Some(sz) = pres_el.child("notesSz") {
        let w = sz.i64("cx").map(emu_to_pt).unwrap_or(540.0);
        let h = sz.i64("cy").map(emu_to_pt).unwrap_or(720.0);
        p.notes_size = Size::new(sane_len(w, 540.0), sane_len(h, 720.0));
    }
    p.first_slide_number = pres_el.u32("firstSlideNum").unwrap_or(1).min(9999);
    p.tags = tags(&imp, &pres_part, pres_el.child("custDataLst"));
    if let Some(dts) = pres_el.child("defaultTextStyle") {
        p.default_text_style = dml::list_style(&mut imp, &pres_part, dts);
    }
    // Embedded fonts: the list as it is, with the font parts it names.
    if let Some(list) = pres_el.child("embeddedFontLst")
        && let Some((el, rels)) = carry_rels(&mut imp, &pres_part, list)
    {
        p.embedded_font_list = Some(pres_part.keep(&el));
        p.embedded_font_rels = rels;
    }

    // Masters and layouts.
    let mut layout_by_part: HashMap<String, LayoutId> = HashMap::new();
    for mp in &master_paths {
        let Some(doc) = docs.get(&opc::norm(mp)) else {
            log::warn!("pptx: master {mp} missing; skipped");
            continue;
        };
        let master = read_master(&mut imp, mp, doc, &docs, &mut layout_by_part);
        p.masters.push(Arc::new(master));
    }
    if p.masters.is_empty() {
        log::warn!("pptx: no readable slide master; using a default one");
        let (m, _) = defaults::build_master(p.slide_size, Default::default(), imp.next);
        imp.next = imp.next.saturating_add(200);
        p.masters.push(Arc::new(m));
    }
    let fallback_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();

    // Slides.
    let mut file_slide_ids: HashMap<u32, SlideId> = HashMap::new();
    for (file_id, path, id) in &slide_ids {
        let Some(doc) = docs.get(&opc::norm(path)) else { continue };
        let part = Part::new(&pkg, path, doc);
        let layout = part.rels.first_of("slideLayout").and_then(|r| layout_by_part.get(&opc::norm(&r.target)).copied()).unwrap_or(fallback_layout);
        let mut slide = read_slide(&mut imp, &part, doc, *id, layout);
        slide.file_id = Some(*file_id);
        // Notes.
        if let Some(n) = part.rels.first_of("notesSlide")
            && let Some(nd) = load(&pkg, &n.target)
        {
            let np = Part::new(&pkg, &n.target, &nd);
            slide.notes = read_notes(&mut imp, &np, &nd.root);
            // A notes page without notes keeps its own placeholders (written back while the
            // notes stay empty), when it names no part besides its master and slide.
            if slide.notes.is_empty()
                && nd.root.is("notes")
                && !has_rel_refs(&nd.root)
                && np.rels.items.iter().all(|r| !r.external && matches!(r.kind.as_str(), "notesMaster" | "slide"))
            {
                let kept = np.keep(&nd.root);
                if kept.len() <= MAX_NOTES_PAGE {
                    slide.notes_page = Some(kept);
                }
            }
        }
        // Comments.
        for c in part.rels.all_of("comments") {
            if let Some(cd) = pkg.xml(&c.target) {
                let mut read = read_comments(&authors, &cd);
                // A modern comment names its shape by the shape's file id; keep it if the slide has it.
                let mut ids = HashSet::new();
                kition_slide_model::walk(&slide.shapes, &mut |s, _| {
                    ids.insert(s.id);
                });
                for comment in &mut read {
                    if comment.shape.is_some_and(|id| !ids.contains(&id)) {
                        comment.shape = None;
                    }
                }
                slide.comments.extend(read);
            }
        }
        file_slide_ids.insert(*file_id, *id);
        p.slides.push(Arc::new(slide));
    }

    // Sections.
    if let Some(ext) = pres_el.child("extLst") {
        if let Some(sl) = ext.find("sectionLst") {
            for s in sl.children_named("section").take(10_000) {
                let slides = s
                    .child("sldIdLst")
                    .map(|l| l.children_named("sldId").filter_map(|x| x.u32("id").and_then(|i| file_slide_ids.get(&i).copied())).collect())
                    .unwrap_or_default();
                p.sections.push(Section {
                    identity: s.attr("id").and_then(kition_slide_model::identity::canonical).unwrap_or_default(),
                    name: s.attr("name").unwrap_or("").to_string(),
                    slides,
                });
            }
        }
        let kept: String = ext
            .children_named("ext")
            .filter(|e| !matches!(e.attr("uri"), Some(crate::SECTION_EXT_URI | crate::font_faces_ext::URI)) && !has_rel_refs(e))
            .map(|e| pres_part.keep(e))
            .collect();
        if !kept.is_empty() {
            p.raw_ext = Some(kept);
        }
    }
    // Custom shows.
    if let Some(l) = pres_el.child("custShowLst") {
        for cs in l.children_named("custShow") {
            let slides = cs
                .child("sldLst")
                .map(|l| {
                    l.children_named("sld")
                        .filter_map(|s| pres_part.rels.target(s.attr("r:id").unwrap_or("")))
                        .filter_map(|t| imp.slide_by_part.get(&opc::norm(t)).copied())
                        .collect()
                })
                .unwrap_or_default();
            p.custom_shows.push(CustomShow { name: cs.attr("name").unwrap_or("").to_string(), slides });
        }
    }
    // Show settings, and the slide and view properties kept as written (without the
    // children that name other parts).
    if let Some(r) = pres_part.rels.first_of("presProps")
        && let Some(d) = pkg.xml(&r.target)
    {
        if let Some(sp) = d.root.child("showPr") {
            p.show = read_show(sp, &imp.custom_show_names);
        }
        p.pres_props = kept_props(&d, "presentationPr", &["showPr"]);
    }
    if let Some(r) = pres_part.rels.first_of("viewProps")
        && let Some(d) = pkg.xml(&r.target)
    {
        p.view_props = kept_props(&d, "viewPr", &[]);
    }
    // Notes and handout masters.
    if let Some(r) = pres_part.rels.first_of("notesMaster") {
        p.notes_master = read_aux_master(&mut imp, &pkg, &r.target, "notesMaster", "Notes Master").map(Arc::new);
    }
    if let Some(r) = pres_part.rels.first_of("handoutMaster") {
        p.handout_master = read_aux_master(&mut imp, &pkg, &r.target, "handoutMaster", "Handout Master").map(Arc::new);
    }
    // Document properties.
    if let Some(r) = root_rels.first_of("core-properties")
        && let Some(d) = pkg.xml(&r.target)
    {
        read_core(&d.root, &mut p.props);
    }
    if let Some(r) = root_rels.first_of("extended-properties")
        && let Some(d) = pkg.xml(&r.target)
    {
        p.props.company = d.root.child("Company").map(|c| c.text()).unwrap_or_default();
    }
    // Custom document properties, verbatim (they relate to no other part).
    if let Some(r) = root_rels.first_of("custom-properties")
        && let Some(b) = pkg.get(&r.target)
        && pkg.xml(&r.target).is_some_and(|d| d.root.is("Properties"))
        && let Ok(text) = std::str::from_utf8(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b))
        && text.len() <= MAX_CUSTOM_PROPS
    {
        p.props.custom_xml = Some(text.to_string());
    }

    p.header_footer = header_footer(&p);
    p.media = std::mem::take(&mut imp.media);
    p.font_faces = font_faces(&imp.faces);
    crate::font_faces_ext::read(pres_el, &mut p.font_faces);
    p.next_id = imp.next;
    p.fix_next_id();
    Ok(p)
}

/// A notes or handout master: its theme, background, shapes, colour map, notes text style and
/// header and footer switches.
fn read_aux_master(imp: &mut Imp, pkg: &Package, target: &str, root_name: &str, name: &str) -> Option<Master> {
    let d = load(pkg, target)?;
    if !d.root.is(root_name) {
        log::warn!("pptx: {target} is not a {root_name}; skipped");
        return None;
    }
    let part = Part::new(pkg, target, &d);
    let mut ctx = shapes::IdCtx::default();
    // Nothing names a notes or handout master by id: one minted on each read (after every other
    // id) would differ between two reads of one deck whenever the deck's other ids did.
    let mut m = Master { id: MasterId(0), name: name.into(), ..Default::default() };
    if let Some(t) = part.rels.first_of("theme")
        && let Some(td) = pkg.xml(&t.target)
    {
        let tp = Part::new(pkg, &t.target, &td);
        m.theme = theme::read_theme(imp, &tp, &td.root);
    }
    if let Some(csld) = d.root.child("cSld") {
        m.background = background(imp, &part, csld);
        if let Some(tree) = csld.child("spTree") {
            m.shapes = shapes::sp_tree(imp, &part, &mut ctx, tree, 0);
        }
    }
    if let Some(cm) = d.root.child("clrMap") {
        m.color_map = color_map(cm);
    }
    if let Some(s) = d.root.child("notesStyle") {
        m.body_style = dml::list_style(imp, &part, s);
    }
    m.hf = d.root.child("hf").map(|hf| part.keep(hf));
    Some(m)
}

/// Each typeface's most frequent face attributes, when they say anything (sorted, so a file
/// always reads the same).
fn font_faces(faces: &HashMap<(String, String), HashMap<FaceAttrs, usize>>) -> Vec<kition_slide_model::FontFace> {
    let mut out: Vec<kition_slide_model::FontFace> = faces
        .iter()
        .filter_map(|((slot, typeface), seen)| {
            let ((panose, charset, pitch_family), _) = seen.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
            (panose.is_some() || charset.is_some() || pitch_family.is_some()).then(|| kition_slide_model::FontFace {
                slot: slot.clone(),
                typeface: typeface.clone(),
                panose: panose.clone(),
                charset: charset.clone(),
                pitch_family: pitch_family.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.slot, &a.typeface).cmp(&(&b.slot, &b.typeface)));
    out
}

fn sane_len(v: f64, dflt: f64) -> f64 {
    if v.is_finite() && (1.0..=20_000.0).contains(&v) { v } else { dflt }
}

/// A properties part's root kept as written, without the children `skip` names and those naming
/// other parts; `None` when nothing is left to keep.
fn kept_props(d: &Doc, root: &str, skip: &[&str]) -> Option<String> {
    if !d.root.is(root) {
        return None;
    }
    let mut el = d.root.clone();
    el.children.retain(|c| match c {
        Node::El(e) => !skip.contains(&e.local()) && !has_rel_refs(e),
        _ => false,
    });
    if el.elements().next().is_none() && el.attrs.iter().all(|(k, _)| k == "xmlns" || k.starts_with("xmlns:")) {
        return None;
    }
    let kept = keep_xml(&el, &d.ns_decls());
    (kept.len() <= MAX_NOTES_PAGE).then_some(kept)
}

/// Largest notes page or properties part kept as XML (bytes).
const MAX_NOTES_PAGE: usize = 1 << 20;

/// Is this attribute a relationship reference (`r:id`, `r:embed`, `r:link`…)?
pub fn is_rel_ref(key: &str, value: &str) -> bool {
    !key.starts_with("xmlns")
        && key.contains(':')
        && matches!(key.rsplit(':').next(), Some("id" | "embed" | "link" | "dm" | "lo" | "qs" | "cs" | "pict" | "href"))
        && !value.is_empty()
}

/// Does the element (or a descendant) reference a relationship?
pub fn has_rel_refs(el: &El) -> bool {
    fn rec(e: &El, depth: usize) -> bool {
        if depth > crate::xml::MAX_DEPTH {
            return false;
        }
        e.attrs.iter().any(|(k, v)| is_rel_ref(k, v)) || e.elements().any(|c| rec(c, depth + 1))
    }
    rec(el, 0)
}

/// Calls `f` on every relationship reference of `el` and its descendants; `f` returns the new
/// value, or `None` to stop (the walk then returns `false`).
pub fn map_rel_refs(el: &mut El, f: &mut dyn FnMut(&str) -> Option<String>) -> bool {
    fn rec(e: &mut El, f: &mut dyn FnMut(&str) -> Option<String>, depth: usize) -> bool {
        if depth > crate::xml::MAX_DEPTH {
            return false;
        }
        for (k, v) in e.attrs.iter_mut() {
            if is_rel_ref(k, v) {
                match f(v) {
                    Some(n) => *v = n,
                    None => return false,
                }
            }
        }
        e.elements_mut().all(|c| rec(c, f, depth + 1))
    }
    rec(el, f, 0)
}

/// Most relationships one kept element may carry.
const MAX_KEPT_RELS: usize = 64;

/// A part carried as a media item, with its own relationships when their targets have none of
/// their own (a diagram data part's pictures). `None` for a part deeper than that (a slide).
pub fn kept_part(imp: &mut Imp, target: &str) -> Option<(kition_slide_model::MediaId, Vec<kition_slide_model::KeptRel>)> {
    let own = imp.pkg.rels(target);
    if own.items.len() > MAX_KEPT_RELS {
        return None;
    }
    let mut rels = vec![];
    for r in &own.items {
        let media = if r.external {
            imp.media_link(&r.target)
        } else {
            if !imp.pkg.rels(&r.target).items.is_empty() {
                return None;
            }
            imp.media_for(&r.target)?
        };
        rels.push(kition_slide_model::KeptRel { id: r.id.clone(), rel_type: r.ty.clone(), media, rels: vec![] });
    }
    Some((imp.media_for(target)?, rels))
}

/// `el` with its relationship references renumbered `rId1`, `rId2`… in document order, and the
/// relationships they name with their targets carried as media items (see [`kept_part`]).
/// `None` when a reference names no relationship, or a part that cannot travel alone.
pub fn carry_rels(imp: &mut Imp, part: &Part, el: &El) -> Option<(El, Vec<kition_slide_model::KeptRel>)> {
    let mut el = el.clone();
    let mut rels: Vec<kition_slide_model::KeptRel> = vec![];
    let mut by_file: HashMap<String, String> = HashMap::new();
    let ok = map_rel_refs(&mut el, &mut |id| {
        if let Some(n) = by_file.get(id) {
            return Some(n.clone());
        }
        if rels.len() >= MAX_KEPT_RELS {
            return None;
        }
        let r = part.rels.get(id)?;
        let (media, own) = if r.external {
            (imp.media_link(&r.target), vec![])
        } else {
            let t = r.target.clone();
            kept_part(imp, &t)?
        };
        let n = format!("rId{}", rels.len() + 1);
        rels.push(kition_slide_model::KeptRel { id: n.clone(), rel_type: r.ty.clone(), media, rels: own });
        by_file.insert(id.to_string(), n.clone());
        Some(n)
    });
    ok.then_some((el, rels))
}

/// Most tag lists and tags read for one owner / list.
const MAX_TAG_LISTS: usize = 256;
const MAX_TAGS: usize = 10_000;

/// The tag lists a `p:custDataLst` names (`p:tags r:id` → `ppt/tags/tag#.xml`).
pub fn tags(imp: &Imp, part: &Part, list: Option<&El>) -> Vec<kition_slide_model::TagList> {
    let Some(list) = list else { return vec![] };
    list.children_named("tags")
        .take(MAX_TAG_LISTS)
        .filter_map(|t| part.rels.get(t.attr("r:id").unwrap_or("")))
        .filter(|r| !r.external)
        .filter_map(|r| imp.pkg.xml(&r.target))
        .filter(|d| d.root.is("tagLst"))
        .map(|d| {
            d.root
                .children_named("tag")
                .take(MAX_TAGS)
                .map(|t| (t.attr("name").unwrap_or("").to_string(), t.attr("val").unwrap_or("").to_string()))
                .collect()
        })
        .collect()
}

pub fn color_map(cm: &El) -> Vec<(String, String)> {
    cm.attrs.iter().filter(|(k, _)| !k.starts_with("xmlns")).map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn background(imp: &mut Imp, part: &Part, csld: &El) -> Option<Background> {
    let bg = csld.child("bg")?;
    if let Some(pr) = bg.child("bgPr") {
        return Some(Background::Fill { fill: dml::fill_of(imp, part, pr).unwrap_or(kition_slide_model::Fill::None) });
    }
    let r = bg.child("bgRef")?;
    Some(Background::Ref {
        idx: r.u32("idx").unwrap_or(1001),
        color: dml::color(r).unwrap_or(kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Bg1)),
    })
}

fn read_master(imp: &mut Imp, path: &str, doc: &Doc, docs: &HashMap<String, Doc>, layout_by_part: &mut HashMap<String, LayoutId>) -> Master {
    let pkg = imp.pkg;
    let part = Part::new(pkg, path, doc);
    let root = &doc.root;
    let mut m = Master { id: MasterId(0), preserve: root.bool("preserve").unwrap_or(false), ..Default::default() };
    if let Some(t) = part.rels.first_of("theme")
        && let Some(td) = pkg.xml(&t.target)
    {
        let tp = Part::new(pkg, &t.target, &td);
        m.theme = theme::read_theme(imp, &tp, &td.root);
    }
    m.id = MasterId(imp.alloc());
    if let Some(cm) = root.child("clrMap") {
        m.color_map = color_map(cm);
    }
    if let Some(csld) = root.child("cSld") {
        m.name = csld.attr("name").unwrap_or("").to_string();
        m.background = background(imp, &part, csld);
        if let Some(tree) = csld.child("spTree") {
            let mut ctx = shapes::IdCtx::default();
            m.shapes = shapes::sp_tree(imp, &part, &mut ctx, tree, 0);
        }
        m.tags = tags(imp, &part, csld.child("custDataLst"));
    }
    if m.name.is_empty() {
        m.name = m.theme.name.clone();
    }
    if let Some(ts) = root.child("txStyles") {
        if let Some(s) = ts.child("titleStyle") {
            m.title_style = dml::list_style(imp, &part, s);
        }
        if let Some(s) = ts.child("bodyStyle") {
            m.body_style = dml::list_style(imp, &part, s);
        }
        if let Some(s) = ts.child("otherStyle") {
            m.other_style = dml::list_style(imp, &part, s);
        }
    }
    // Layouts in list order, then any other related layouts.
    let mut lpaths: Vec<String> = root
        .child("sldLayoutIdLst")
        .map(|l| l.children_named("sldLayoutId").filter_map(|x| part.rels.target(x.attr("r:id").unwrap_or(""))).map(String::from).collect())
        .unwrap_or_default();
    for r in part.rels.all_of("slideLayout") {
        if !lpaths.iter().any(|x| opc::norm(x) == opc::norm(&r.target)) {
            lpaths.push(r.target.clone());
        }
    }
    for lp in lpaths.iter().take(1000) {
        let key = opc::norm(lp);
        if layout_by_part.contains_key(&key) {
            continue;
        }
        let Some(ld) = docs.get(&key) else { continue };
        let lpart = Part::new(pkg, lp, ld);
        let l = read_layout(imp, &lpart, ld);
        layout_by_part.insert(key, l.id);
        m.layouts.push(l);
    }
    if m.layouts.is_empty() {
        // A master must have a layout for slides to use.
        imp.next = imp.next.saturating_add(1);
        m.layouts.push(Layout {
            id: LayoutId(imp.next),
            name: "Blank".into(),
            kind: LayoutType::Blank,
            show_master_shapes: true,
            ..Default::default()
        });
    }
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        m.raw_ext = Some(ext.elements().map(|e| part.keep(e)).collect());
    }
    m.hf = root.child("hf").map(|hf| part.keep(hf));
    m
}

fn read_layout(imp: &mut Imp, part: &Part, doc: &Doc) -> Layout {
    let root = &doc.root;
    let mut l = Layout {
        id: LayoutId(imp.alloc()),
        identity: crate::layout_id::read(root),
        kind: LayoutType::from_xml(root.attr("type").unwrap_or("cust")),
        show_master_shapes: root.bool("showMasterSp").unwrap_or(true),
        preserve: root.bool("preserve").unwrap_or(false),
        user_drawn: root.bool("userDrawn").unwrap_or(false),
        ..Default::default()
    };
    if let Some(csld) = root.child("cSld") {
        l.name = csld.attr("name").unwrap_or("").to_string();
        l.background = background(imp, part, csld);
        if let Some(tree) = csld.child("spTree") {
            let mut ctx = shapes::IdCtx::default();
            l.shapes = shapes::sp_tree(imp, part, &mut ctx, tree, 0);
        }
        l.tags = tags(imp, part, csld.child("custDataLst"));
    }
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        let raw: String = ext.elements().filter(|ext| ext.attr("uri") != Some(crate::layout_id::URI)).map(|e| part.keep(e)).collect();
        l.raw_ext = (!raw.is_empty()).then_some(raw);
    }
    l.hf = root.child("hf").map(|hf| part.keep(hf));
    l
}

fn read_slide(imp: &mut Imp, part: &Part, doc: &Doc, id: SlideId, layout: LayoutId) -> Slide {
    let root = &doc.root;
    let mut s = Slide {
        id,
        layout,
        hidden: root.bool("show") == Some(false),
        show_master_shapes: root.bool("showMasterSp").unwrap_or(true),
        ..Default::default()
    };
    let mut ctx = shapes::IdCtx::default();
    if let Some(csld) = root.child("cSld") {
        s.name = csld.attr("name").unwrap_or("").to_string();
        s.background = background(imp, part, csld);
        if let Some(tree) = csld.child("spTree") {
            s.shapes = shapes::sp_tree(imp, part, &mut ctx, tree, 0);
        }
        s.tags = tags(imp, part, csld.child("custDataLst"));
    }
    if let Some(t) = root.child("transition") {
        s.transition = timing::transition(imp, part, t);
    }
    if let Some(t) = root.child("timing") {
        s.animations = timing::animations(&ctx, t, &s.shapes);
        s.timing = timing::kept(part, &ctx, t, &s.animations);
    }
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        let raw: String = ext.elements().filter(|e| e.attr("uri") != Some(crate::diagram_ids::URI)).map(|e| part.keep(e)).collect();
        s.raw_ext = (!raw.is_empty()).then_some(raw);
    }
    crate::diagram_ids::read(root, &mut s, &imp.media);
    s
}

fn read_notes(imp: &mut Imp, part: &Part, root: &El) -> TextBody {
    let Some(tree) = root.path(&["cSld", "spTree"]) else { return TextBody::default() };
    for sp in tree.children_named("sp") {
        let ph = sp.path(&["nvSpPr", "nvPr", "ph"]);
        let is_body = ph.is_some_and(|ph| matches!(ph.attr("type"), Some("body") | None) && ph.attr("type") != Some("sldImg"));
        if is_body && let Some(tx) = sp.child("txBody") {
            let mut t = dml::text_body(imp, part, tx);
            // Notes keep only content: drop body formatting from the notes master placeholder.
            t.list_style = Default::default();
            if t.is_empty() {
                return TextBody::default();
            }
            return t;
        }
    }
    TextBody::default()
}

/// Comment authors by id: legacy `commentAuthors.xml` (numeric ids) and modern `authors.xml`
/// (GUIDs): name, initials, user id, identity provider.
type Authors = HashMap<String, [String; 4]>;

fn read_authors(pkg: &Package, pres: &Part) -> Authors {
    let mut authors = Authors::new();
    for kind in ["commentAuthors", "authors"] {
        if let Some(r) = pres.rels.first_of(kind)
            && let Some(d) = pkg.xml(&r.target)
        {
            for a in d.root.elements().take(100_000) {
                if let Some(id) = a.attr("id") {
                    let text = |name: &str| a.attr(name).unwrap_or("").to_string();
                    authors.insert(id.to_string(), [text("name"), text("initials"), text("userId"), text("providerId")]);
                }
            }
        }
    }
    authors
}

/// A comment part's comments, replies under the comment they answer: a legacy `p:cmLst` (replies
/// thread through `p15:threadingInfo`) or a modern `p188:cmLst` (replies in each comment's
/// `replyLst`).
fn read_comments(authors: &Authors, doc: &Doc) -> Vec<Comment> {
    if let Some(comments) = crate::comments_ext::read(&doc.root, authors) {
        return comments;
    }
    let mut root = doc.root.clone();
    crate::comments_ext::strip(&mut root);
    let root = &root;
    let ns = doc.ns_decls();
    let modern = root.prefix() == "p188" || root.elements().any(|e| e.has_attr("created") || e.child("txBody").is_some());
    if modern { read_modern_comments(authors, root, &ns) } else { read_legacy_comments(authors, root, &ns) }
}

fn author_of(authors: &Authors, id: Option<&str>) -> (String, String, String, String) {
    let [name, initials, user, provider] = id.and_then(|a| authors.get(a).cloned()).unwrap_or_default();
    (name, initials, user, provider)
}

fn read_legacy_comments(authors: &Authors, root: &El, ns: &[(String, String)]) -> Vec<Comment> {
    // (comment, its author id and idx, the author id and idx it answers)
    let mut flat: Vec<(Comment, Option<(String, String)>, Option<(String, String)>)> = Vec::new();
    for e in root.children_named("cm").take(10_000) {
        let (author, initials, ..) = author_of(authors, e.attr("authorId"));
        let pos = e.child("pos");
        let mut parent = None;
        let mut kept = kition_slide_model::CommentKept::default();
        if let Some(list) = e.child("extLst") {
            let mut rest = list.clone();
            rest.children.retain(|node| match node {
                Node::El(ext) => match ext.child("threadingInfo") {
                    Some(info) => {
                        kept.tz = info.i32("timeZoneBias");
                        parent =
                            info.child("parentCm").map(|p| (p.attr("authorId").unwrap_or("").to_string(), p.attr("idx").unwrap_or("").to_string()));
                        false
                    }
                    None => true,
                },
                Node::Text(_) => false,
            });
            let others: String = rest.elements().map(|ext| crate::read::keep_xml(ext, ns)).collect();
            if !others.is_empty() {
                kept.ext_xml = Some(others);
            }
        }
        let comment = Comment {
            author,
            initials,
            text: e.child("text").map(El::text).unwrap_or_default(),
            date: e.attr("dt").unwrap_or("").to_string(),
            x: pos.and_then(|p| p.f64("x")).unwrap_or(0.0) / 8.0,
            y: pos.and_then(|p| p.f64("y")).unwrap_or(0.0) / 8.0,
            kept: (kept != kition_slide_model::CommentKept::default()).then(|| Box::new(kept)),
            ..Default::default()
        };
        let me = e.attr("authorId").zip(e.attr("idx")).map(|(a, i)| (a.to_string(), i.to_string()));
        flat.push((comment, me, parent));
    }
    // A reply goes under the thread of the comment it answers (one level: a reply to a reply
    // answers the same thread), when that comment came before it; otherwise it stays a comment.
    let mut out: Vec<Comment> = Vec::new();
    let mut thread_of: HashMap<(String, String), usize> = HashMap::new();
    for (comment, me, parent) in flat {
        let thread = parent.as_ref().and_then(|p| thread_of.get(p).copied());
        let at = match thread.and_then(|t| out.get_mut(t).map(|c| (t, c))) {
            Some((t, top)) => {
                top.replies.push(comment);
                t
            }
            None => {
                out.push(comment);
                out.len() - 1
            }
        };
        if let Some(me) = me {
            thread_of.entry(me).or_insert(at);
        }
    }
    out
}

/// A modern comment's or reply's text, and its `txBody` as read.
fn modern_body(e: &El, ns: &[(String, String)]) -> (String, Option<String>) {
    match e.child("txBody") {
        Some(body) => {
            let text = body.children_named("p").map(El::text).collect::<Vec<_>>().join("\n");
            (text, Some(crate::read::keep_xml(body, ns)))
        }
        None => (e.child("text").map(El::text).unwrap_or_default(), None),
    }
}

fn read_modern_comment(authors: &Authors, e: &El, ns: &[(String, String)]) -> Comment {
    let author_guid = e.attr("authorId").unwrap_or("").to_string();
    let (author, initials, user_id, provider_id) = author_of(authors, e.attr("authorId"));
    let (text, body_xml) = modern_body(e, ns);
    let status = e.attr("status").unwrap_or("");
    let anchor = e.elements().find(|c| !matches!(c.local(), "pos" | "replyLst" | "txBody" | "extLst"));
    let shape = anchor.and_then(|a| a.find("spMk")).and_then(|m| m.u32("id")).map(ShapeId);
    let kept = kition_slide_model::CommentKept {
        modern: true,
        guid: e.attr("id").unwrap_or("").to_string(),
        author_guid,
        user_id,
        provider_id,
        status: if matches!(status, "" | "active" | "resolved") { String::new() } else { status.to_string() },
        // The slide marker's `sldId` is the file's numbering of its slides: the writer names the
        // slide as it writes it, so it is not kept.
        anchor_xml: anchor.map(|a| {
            let mut a = a.clone();
            drop_slide_ids(&mut a, 0);
            crate::read::keep_xml(&a, ns)
        }),
        body_text: body_xml.is_some().then(|| text.clone()),
        body_xml,
        ext_xml: e.child("extLst").map(|x| crate::read::keep_xml(x, ns)),
        tz: None,
    };
    let pos = e.child("pos");
    Comment {
        author,
        initials,
        text,
        date: e.attr("created").or(e.attr("dt")).unwrap_or("").to_string(),
        x: pos.and_then(|p| p.i64("x")).map_or(0.0, emu_to_pt),
        y: pos.and_then(|p| p.i64("y")).map_or(0.0, emu_to_pt),
        resolved: status == "resolved",
        shape,
        kept: Some(Box::new(kept)),
        ..Default::default()
    }
}

fn drop_slide_ids(e: &mut El, depth: usize) {
    if depth > 16 {
        return;
    }
    if e.local() == "sldMk" {
        e.attrs.retain(|(k, _)| k != "sldId");
    }
    for c in e.elements_mut() {
        drop_slide_ids(c, depth + 1);
    }
}

fn read_modern_comments(authors: &Authors, root: &El, ns: &[(String, String)]) -> Vec<Comment> {
    root.children_named("cm")
        .take(10_000)
        .map(|e| {
            let mut comment = read_modern_comment(authors, e, ns);
            comment.replies = e
                .child("replyLst")
                .map(|list| list.children_named("reply").take(1000).map(|r| read_modern_comment(authors, r, ns)).collect())
                .unwrap_or_default();
            comment
        })
        .collect()
}

fn read_show(sp: &El, shows: &HashMap<String, String>) -> ShowSettings {
    let mut s = ShowSettings {
        loop_until_esc: sp.bool("loop").unwrap_or(false),
        without_narration: sp.bool("showNarration") == Some(false),
        without_animation: sp.bool("showAnimation") == Some(false),
        use_timings: sp.bool("useTimings").unwrap_or(true),
        ..Default::default()
    };
    if sp.child("browse").is_some() {
        s.show_type = "browsed".into();
    } else if sp.child("kiosk").is_some() {
        s.show_type = "kiosk".into();
    }
    if let Some(r) = sp.child("sldRg") {
        s.range = Some((r.u32("st").unwrap_or(1).max(1), r.u32("end").unwrap_or(1).max(1)));
    }
    if let Some(c) = sp.child("custShow") {
        let id = c.attr("id").unwrap_or("");
        s.custom_show = Some(shows.get(id).cloned().unwrap_or_else(|| id.to_string()));
    }
    if let Some(c) = sp.child("penClr").and_then(dml::color)
        && let kition_slide_model::ColorBase::Rgb { rgb } = c.base
    {
        s.pen_color = rgb;
    }
    s
}

/// Most bytes of custom document properties kept.
const MAX_CUSTOM_PROPS: usize = 4 << 20;

fn read_core(root: &El, p: &mut Properties) {
    for e in root.elements() {
        let t = e.text();
        match e.local() {
            "title" => p.title = t,
            "subject" => p.subject = t,
            "creator" => p.author = t,
            "keywords" => p.keywords = t,
            "description" => p.comments = t,
            "category" => p.category = t,
            "lastModifiedBy" => p.last_modified_by = t,
            "revision" => p.revision = t.trim().parse().unwrap_or(0),
            "created" => p.created = t,
            "modified" => p.modified = t,
            _ => {}
        }
    }
}

/// Which footer placeholders the slides show.
fn header_footer(p: &SlideFile) -> HeaderFooter {
    let mut hf = HeaderFooter::default();
    for s in &p.slides {
        for sh in &s.shapes {
            match sh.ph_type() {
                Some(PhType::SlideNum) => hf.slide_number = true,
                Some(PhType::Date) => {
                    hf.date = true;
                    if let Some(t) = &sh.text
                        && !t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| matches!(r.kind, kition_slide_model::text::RunKind::Field { .. }))
                        && hf.date_text.is_empty()
                    {
                        hf.date_text = t.text();
                    }
                }
                Some(PhType::Footer) => {
                    hf.footer = true;
                    if hf.footer_text.is_empty()
                        && let Some(t) = &sh.text
                    {
                        hf.footer_text = t.text();
                    }
                }
                _ => {}
            }
        }
    }
    hf
}
