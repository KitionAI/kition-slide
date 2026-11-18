//! Shape trees: `p:sp`, `p:pic`, `p:grpSp`, `p:cxnSp`, `p:graphicFrame` (tables, charts, smart graphics,
//! OLE and other objects).

mod ink;

use std::collections::{HashMap, HashSet};

use kition_slide_color::SchemeSlot;
use kition_slide_geom::{Xfrm, preset};
use kition_slide_model::style::{ColorRef, Effects, Fill};
use kition_slide_model::table::{Cell, Row, Table};
use kition_slide_model::text::{Anchor, TextDir};
use kition_slide_model::{Geom, MediaClip, PhType, Placeholder, Shape, ShapeId, ShapeKind, ShapeStyle};

use super::{Imp, Part, dml};
use crate::xml::El;

/// File shape ids at or above this are reassigned.
pub const MAX_KEPT_ID: u32 = 1 << 30;
const MAX_TREE_DEPTH: usize = 32;
const MAX_SHAPES: usize = 20_000;

/// Shape id bookkeeping for one part.
#[derive(Default)]
pub struct IdCtx {
    pub used: HashSet<u32>,
    /// File id → model id.
    pub map: HashMap<u32, ShapeId>,
    /// Always allocate new ids (shapes pulled in from other parts).
    pub fresh: bool,
    pub count: usize,
}

impl IdCtx {
    fn assign(&mut self, imp: &mut Imp, file_id: Option<u32>) -> ShapeId {
        if !self.fresh
            && let Some(i) = file_id
            && (2..MAX_KEPT_ID).contains(&i)
            && !self.used.contains(&i)
        {
            self.used.insert(i);
            self.map.insert(i, ShapeId(i));
            return ShapeId(i);
        }
        let n = imp.alloc();
        self.used.insert(n);
        if let Some(i) = file_id {
            self.map.entry(i).or_insert(ShapeId(n));
        }
        ShapeId(n)
    }
    pub fn lookup(&self, file_id: u32) -> Option<ShapeId> {
        self.map.get(&file_id).copied()
    }
}

/// Read the children of an `spTree` / `grpSp`.
pub fn sp_tree(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, tree: &El, depth: usize) -> Vec<Shape> {
    let mut out = vec![];
    if depth > MAX_TREE_DEPTH {
        log::warn!("pptx: group nesting too deep in {}", part.name);
        return out;
    }
    for e in tree.elements() {
        if ctx.count >= MAX_SHAPES {
            log::warn!("pptx: too many shapes in {}", part.name);
            break;
        }
        let s = match e.local() {
            "sp" => Some(sp(imp, part, ctx, e)),
            "pic" => Some(pic(imp, part, ctx, e)),
            "grpSp" => Some(grp(imp, part, ctx, e, depth)),
            "cxnSp" => Some(cxn(imp, part, ctx, e)),
            "graphicFrame" => Some(frame(imp, part, ctx, e, depth)),
            "contentPart" => ink::read(imp, part, ctx, e),
            "AlternateContent" => {
                let choice = e.children_named("Choice").find(|ch| super::supported_mc_choice(ch));
                // A choice is atomic: never partly import it and discard its fallback.
                let content = choice.and_then(|ch| {
                    let mut elements = ch.elements();
                    let first = elements.next()?;
                    (first.is("contentPart") && elements.next().is_none()).then_some(first)
                });
                if let Some(s) = content.and_then(|cp| ink::read(imp, part, ctx, cp)) {
                    Some(s)
                } else {
                    if let Some(fallback) = e.child("Fallback") {
                        out.extend(sp_tree(imp, part, ctx, fallback, depth + 1));
                    }
                    None
                }
            }
            _ => None,
        };
        if let Some(s) = s {
            ctx.count += 1;
            out.push(s);
        }
    }
    if depth == 0 {
        remap_connectors(&mut out, ctx, 0);
    }
    out
}

fn remap_connectors(shapes: &mut [Shape], ctx: &IdCtx, depth: usize) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    for s in shapes.iter_mut() {
        match &mut s.kind {
            ShapeKind::Connector { start, end } => {
                for e in [start, end] {
                    *e = e.and_then(|(id, site)| ctx.lookup(id.0).map(|m| (m, site)));
                }
            }
            ShapeKind::Group { children, .. } => remap_connectors(children, ctx, depth + 1),
            _ => {}
        }
    }
}

/// Common non-visual properties (`p:nvSpPr`, `p:nvPicPr`…).
fn base(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, nv: Option<&El>) -> Shape {
    let c = nv.and_then(|n| n.child("cNvPr"));
    let mut s = Shape { id: ctx.assign(imp, c.and_then(|c| c.u32("id"))), ..Default::default() };
    if let Some(c) = c {
        s.identity = c.find("creationId").and_then(|e| e.attr("id")).and_then(kition_slide_model::identity::canonical).unwrap_or_default();
        s.name = c.attr("name").unwrap_or("").to_string();
        s.descr = c.attr("descr").unwrap_or("").to_string();
        s.title = c.attr("title").unwrap_or("").to_string();
        s.hidden = c.bool("hidden").unwrap_or(false);
        s.click = c.child("hlinkClick").and_then(|h| dml::hyperlink(imp, part, h));
        s.hover = c.child("hlinkHover").and_then(|h| dml::hyperlink(imp, part, h));
        s.decorative = c.find("decorative").and_then(|d| d.bool("val")).unwrap_or(false);
    }
    if let Some(nv) = nv {
        for e in nv.elements() {
            match e.local() {
                "cNvSpPr" => {
                    s.text_box = e.bool("txBox").unwrap_or(false);
                    s.locked = e.child("spLocks").is_some_and(|l| l.bool("noMove") == Some(true) && l.bool("noResize") == Some(true));
                }
                "cNvPicPr" | "cNvGrpSpPr" | "cNvCxnSpPr" | "cNvGraphicFramePr" | "cNvContentPartPr" => {
                    s.locked = e.elements().next().is_some_and(|l| l.bool("noMove") == Some(true) && l.bool("noResize") == Some(true));
                }
                "nvPr" => {
                    s.tags = super::tags(imp, part, e.child("custDataLst"));
                    if let Some(ph) = e.child("ph") {
                        s.ph = Some(Placeholder {
                            kind: ph.attr("type").map(PhType::from_xml).unwrap_or(PhType::Obj),
                            idx: ph.u32("idx").unwrap_or(0),
                            vertical: ph.attr("orient") == Some("vert"),
                            size: ph.attr("sz").map(String::from),
                            has_custom_prompt: ph.bool("hasCustomPrompt").unwrap_or(false),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    s
}

fn preset_geom(pg: &El) -> Geom {
    let name = pg.attr("prst").unwrap_or("rect").to_string();
    let defaults: Vec<f64> = preset::info(&name).map(|i| i.defaults.to_vec()).unwrap_or_default();
    let mut adj = defaults;
    let mut any = false;
    if let Some(av) = pg.child("avLst") {
        for gd in av.children_named("gd").take(16) {
            let n = gd.attr("name").unwrap_or("");
            let idx = if n == "adj" {
                0
            } else if let Some(k) = n.strip_prefix("adj").and_then(|k| k.parse::<usize>().ok()).filter(|k| (1..=16).contains(k)) {
                k - 1
            } else {
                continue;
            };
            let Some(v) =
                gd.attr("fmla").and_then(|f| f.trim().strip_prefix("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
            else {
                continue;
            };
            if adj.len() <= idx {
                adj.resize(idx + 1, 0.0);
            }
            if let Some(slot) = adj.get_mut(idx) {
                *slot = v;
                any = true;
            }
        }
    }
    if !any {
        adj.clear();
    }
    Geom::Preset { name, adj }
}

/// `spPr` → transform, geometry, fill, line, effects.
fn sp_pr(imp: &mut Imp, part: &Part, s: &mut Shape, pr: &El) {
    if let Some(x) = pr.child("xfrm") {
        s.xfrm = Some(dml::xfrm(x).0);
    }
    if let Some(pg) = pr.child("prstGeom") {
        s.geom = preset_geom(pg);
    } else if let Some(cg) = pr.child("custGeom") {
        let (w, h) =
            s.xfrm.map(|x| (kition_slide_geom::pt_to_emu(x.w) as f64, kition_slide_geom::pt_to_emu(x.h) as f64)).unwrap_or((21600.0, 21600.0));
        let paths = crate::custgeom::read_cust_geom(cg, w, h);
        if !paths.is_empty() {
            let source = crate::custgeom::has_more_than_paths(cg).then(|| part.keep(cg));
            s.geom = Geom::Custom { paths, source };
        }
    }
    s.fill = dml::fill_of(imp, part, pr);
    if let Some(ln) = pr.child("ln") {
        s.line = Some(dml::line(imp, part, ln));
    }
    if let Some(fx) = pr.child("effectLst") {
        s.effects = Some(dml::effects(fx));
    }
    let raw: String = pr.elements().filter(|c| c.is("scene3d") || c.is("sp3d")).map(|c| part.keep(c)).collect();
    if !raw.is_empty() {
        let fx = s.effects.get_or_insert_with(Effects::default);
        fx.bevel = pr.path(&["sp3d", "bevelT"]).map(|b| b.attr("prst").unwrap_or("circle").to_string());
        fx.raw3d = Some(raw);
    }
}

fn style(st: &El) -> ShapeStyle {
    let r = |n: &str| {
        st.child(n)
            .map(|e| (e.u32("idx").unwrap_or(0).min(1003), dml::color(e).unwrap_or(ColorRef::scheme(SchemeSlot::Accent1))))
            .unwrap_or((0, ColorRef::scheme(SchemeSlot::Accent1)))
    };
    let font = st.child("fontRef");
    ShapeStyle {
        line_ref: r("lnRef"),
        fill_ref: r("fillRef"),
        effect_ref: r("effectRef"),
        font_ref: (font.and_then(|f| f.attr("idx")).unwrap_or("minor").to_string(), font.and_then(dml::color)),
    }
}

fn sp(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, e: &El) -> Shape {
    let mut s = base(imp, part, ctx, e.child("nvSpPr"));
    if let Some(pr) = e.child("spPr") {
        sp_pr(imp, part, &mut s, pr);
    }
    if e.bool("useBgFill") == Some(true) {
        s.fill = Some(Fill::Background);
    }
    s.style = e.child("style").map(style);
    s.text = e.child("txBody").map(|t| {
        let mut body = dml::text_body(imp, part, t);
        // A shrink result a file gives is a saved one, unless Kition's own extension said
        // otherwise (`autofit_ext`).
        let flagged = t.child("bodyPr").is_some_and(crate::autofit_ext::flags_saved);
        if matches!(body.body.autofit, Some(kition_slide_model::text::AutoFit::Shrink { .. })) && !flagged {
            body.body.autofit_saved = Some(true);
        }
        body
    });
    // A shape's own text without `bodyPr/@anchor` is top-anchored (the schema default, as
    // PowerPoint shows it); only placeholders inherit an anchor. Kition's default for shapes it
    // creates (middle) must not apply to them.
    if s.ph.is_none()
        && let Some(t) = s.text.as_mut()
    {
        t.body.anchor.get_or_insert(Anchor::Top);
    }
    s
}

fn ms(v: Option<&str>) -> u32 {
    v.and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, u32::MAX as f64) as u32).unwrap_or(0)
}

fn pic(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, e: &El) -> Shape {
    let mut s = base(imp, part, ctx, e.child("nvPicPr"));
    if let Some(pr) = e.child("spPr") {
        sp_pr(imp, part, &mut s, pr);
    }
    s.style = e.child("style").map(style);
    let pf = e.child("blipFill").and_then(|b| dml::blip_fill(imp, part, b));
    let nvpr = e.path(&["nvPicPr", "nvPr"]);
    let vf = nvpr.and_then(|n| n.child("videoFile"));
    let af = nvpr.and_then(|n| n.child("audioFile"));
    let p14 = nvpr.and_then(|n| n.child("extLst")).and_then(|x| x.find("media"));
    if vf.is_some() || af.is_some() || p14.is_some() {
        let media = p14.and_then(|m| dml::media_ref(imp, part, m)).or_else(|| vf.or(af).and_then(|f| dml::media_ref(imp, part, f)));
        if let Some(media) = media {
            let mut clip =
                MediaClip { media, video: vf.is_some() || af.is_none(), poster: pf.as_ref().map(|p| p.media), volume: 1.0, ..Default::default() };
            if let Some(m) = p14 {
                if let Some(t) = m.child("trim") {
                    clip.trim_start_ms = ms(t.attr("st"));
                    clip.trim_end_ms = ms(t.attr("end"));
                }
                if let Some(f) = m.child("fade") {
                    clip.fade_in_ms = ms(f.attr("in"));
                    clip.fade_out_ms = ms(f.attr("out"));
                }
                if let Some(l) = m.child("bmkLst") {
                    clip.bookmarks =
                        l.children_named("bmk").take(1000).map(|b| (b.attr("name").unwrap_or("").to_string(), ms(b.attr("time")))).collect();
                }
            }
            s.kind = ShapeKind::Media(clip);
            return s;
        }
    }
    match pf {
        Some(fill) => s.kind = ShapeKind::Picture { fill },
        None => log::warn!("pptx: picture {} in {} has no readable image", s.name, part.name),
    }
    s
}

fn grp(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, e: &El, depth: usize) -> Shape {
    let mut s = base(imp, part, ctx, e.child("nvGrpSpPr"));
    let mut child = None;
    if let Some(pr) = e.child("grpSpPr") {
        if let Some(x) = pr.child("xfrm") {
            let (xf, ch) = dml::xfrm(x);
            s.xfrm = Some(xf);
            child = ch;
        }
        s.fill = dml::fill_of(imp, part, pr);
        if let Some(fx) = pr.child("effectLst") {
            s.effects = Some(dml::effects(fx));
        }
    }
    let children = sp_tree(imp, part, ctx, e, depth + 1);
    let child = child.or_else(|| s.xfrm.map(|x| Xfrm::new(x.x, x.y, x.w, x.h))).unwrap_or_default();
    s.kind = ShapeKind::Group { children, child };
    as_ink(s)
}

/// A group read back as the slide's ink layer when it is one the writer made: named `Ink`, its
/// children each an `Ink N` freeform of one open path, without fill, in the group's own space.
/// Each child becomes a stroke through its path's points (in slide points; pressure 0.5, which
/// the file does not keep), with its outline's colour and width; a half-transparent outline is a
/// highlighter's.
fn as_ink(s: Shape) -> Shape {
    let ShapeKind::Group { children, child } = &s.kind else { return s };
    let same_space = s.xfrm.is_some_and(|x| {
        (x.x - child.x).abs() < 0.01 && (x.y - child.y).abs() < 0.01 && (x.w - child.w).abs() < 0.01 && (x.h - child.h).abs() < 0.01
    });
    if s.name != "Ink" || children.is_empty() || !same_space {
        return s;
    }
    let mut strokes = Vec::with_capacity(children.len());
    for c in children {
        let (Some(xf), Geom::Custom { paths, .. }) = (c.xfrm, &c.geom) else { return s };
        let [p] = paths.as_slice() else { return s };
        if !c.name.starts_with("Ink ") || !matches!(c.fill, Some(Fill::None)) || p.d.contains('Z') || p.d.contains('z') {
            return s;
        }
        let (sx, sy) = (if p.w > 0.0 { xf.w / p.w } else { 1.0 }, if p.h > 0.0 { xf.h / p.h } else { 1.0 });
        let nums: Vec<f64> = p.d.split(|ch: char| ch.is_whitespace() || ch == ',').filter_map(|t| t.parse::<f64>().ok()).collect();
        let points: Vec<(f64, f64, f32)> = nums.as_chunks::<2>().0.iter().map(|[px, py]| (xf.x + px * sx, xf.y + py * sy, 0.5)).collect();
        let Some(line) = &c.line else { return s };
        let (color, alpha) = match &line.fill {
            Some(Fill::Solid { color }) => (
                match &color.base {
                    kition_slide_model::style::ColorBase::Rgb { rgb } => *rgb,
                    _ => kition_slide_color::Rgba::BLACK,
                },
                color.alpha(),
            ),
            _ => (kition_slide_color::Rgba::BLACK, 1.0),
        };
        strokes.push(kition_slide_model::InkStroke { points, color, width: line.width.unwrap_or(1.0), highlighter: alpha <= 0.6 });
    }
    Shape { kind: ShapeKind::Ink { strokes }, ..s }
}

fn cxn(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, e: &El) -> Shape {
    let nv = e.child("nvCxnSpPr");
    let mut s = base(imp, part, ctx, nv);
    if let Some(pr) = e.child("spPr") {
        sp_pr(imp, part, &mut s, pr);
    }
    s.style = e.child("style").map(style);
    let c = nv.and_then(|n| n.child("cNvCxnSpPr"));
    let end = |n: &str| c.and_then(|c| c.child(n)).and_then(|x| Some((ShapeId(x.u32("id")?), x.u32("idx").unwrap_or(0))));
    s.kind = ShapeKind::Connector { start: end("stCxn"), end: end("endCxn") };
    s
}

/// A chart part, carrying the parts its relationships name: its style and colour style parts, and
/// its embedded workbook while the kept part names it.
fn chart_part(imp: &mut Imp, name: &str) -> Option<kition_slide_model::Chart> {
    let pkg = imp.pkg;
    let bytes = pkg.get(name)?;
    let rels = pkg.rels(name);
    // A carried part must stand alone (no relationships of its own).
    let alone = |r: &crate::opc::Rel| !r.external && pkg.rels(&r.target).items.is_empty() && pkg.get(&r.target).is_some();
    let mut kept: Vec<kition_slide_model::KeptRel> = vec![];
    let mut chart = super::read_chart_part(bytes, &mut |id| {
        let r = rels.get(id).filter(|r| (r.ty.ends_with("/package") || r.ty.ends_with("/image")) && alone(r))?;
        let media = imp.media_for(&r.target)?;
        let n = format!("rId{}", kept.len() + 1);
        kept.push(kition_slide_model::KeptRel { id: n.clone(), rel_type: r.ty.clone(), media, rels: vec![] });
        Some(n)
    })?;
    for r in rels.items.iter().filter(|r| r.ty.ends_with("/chartStyle") || r.ty.ends_with("/chartColorStyle")) {
        if kept.iter().any(|k| k.rel_type == r.ty) || !alone(r) {
            continue;
        }
        if let Some(media) = imp.media_for(&r.target) {
            kept.push(kition_slide_model::KeptRel { id: format!("rId{}", kept.len() + 1), rel_type: r.ty.clone(), media, rels: vec![] });
        }
    }
    chart.rels = kept;
    // The colours a theme override gives the chart (drawn with them; not written back).
    let theme_override = rels.items.iter().find(|r| !r.external && r.ty.ends_with("/themeOverride"));
    chart.fmt.scheme = theme_override.and_then(|r| pkg.get(&r.target)).and_then(super::theme_override_scheme);
    super::picture_fills(imp, name, bytes, &mut chart);
    Some(chart)
}

fn frame(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, e: &El, depth: usize) -> Shape {
    let mut s = base(imp, part, ctx, e.child("nvGraphicFramePr"));
    if let Some(x) = e.child("xfrm") {
        s.xfrm = Some(dml::xfrm(x).0);
    }
    let gd = e.path(&["graphic", "graphicData"]);
    let uri = gd.and_then(|g| g.attr("uri")).unwrap_or("");
    if let Some(gd) = gd {
        if let Some(tbl) = gd.child("tbl") {
            s.kind = ShapeKind::Table(table(imp, part, tbl));
            return s;
        }
        if let Some(c) = gd.child("chart")
            && let Some(rel) = part.rels.get(c.attr("r:id").unwrap_or(""))
            && let Some(chart) = chart_part(imp, &rel.target)
        {
            s.kind = ShapeKind::Chart(Box::new(chart));
            return s;
        }
        if uri.ends_with("/diagram")
            && let Some(g) = smart_graphic(imp, part, e, gd, &s, depth)
        {
            return g;
        }
    }
    // Something we don't model: keep its XML and show a preview picture when it has one.
    let (preview, preview_adjust) = {
        let mut pics = vec![];
        e.find_all("blip", &mut pics);
        pics.into_iter()
            .find_map(|b| {
                let media = dml::media_ref(imp, part, b)?;
                let adjust = dml::blip_adjust(b);
                Some((Some(media), (adjust != Default::default()).then_some(adjust)))
            })
            .unwrap_or_default()
    };
    let label = if let Some(o) = gd.and_then(|g| g.find("oleObj")) {
        o.attr("progId").or(o.attr("name")).unwrap_or("Object").to_string()
    } else if uri.ends_with("/diagram") {
        "Smart graphic".to_string()
    } else if uri.contains("chart") {
        "Chart".to_string()
    } else {
        uri.rsplit('/').next().unwrap_or("Object").to_string()
    };
    // The parts its relationships name travel with it (an OLE object's embedding and picture).
    let (xml, rels) = match super::carry_rels(imp, part, e) {
        Some((el, rels)) => (part.keep(&el), rels),
        None => (part.keep(e), vec![]),
    };
    s.kind = ShapeKind::Opaque { xml, preview, preview_adjust, label, rels };
    s
}

/// A smart graphic (`dgm`): read the pre-drawn shapes of its drawing part as a group.
fn smart_graphic(imp: &mut Imp, part: &Part, e: &El, gd: &El, frame: &Shape, depth: usize) -> Option<Shape> {
    let rel_ids = gd.child("relIds")?;
    let dm = part.rels.get(rel_ids.attr("r:dm").unwrap_or(""))?;
    let data = imp.pkg.xml(&dm.target)?;
    let drawing_ref = data.root.find("dataModelExt").and_then(|x| x.attr("relId")).map(String::from)?;
    let drawing_rel = part.rels.get(&drawing_ref).map(|r| r.target.clone())?;
    let mut d = imp.pkg.xml(&drawing_rel)?;
    super::resolve_mc(&mut d.root, 0);
    let dpart = Part::new(imp.pkg, &drawing_rel, &d);
    let tree = d.root.child("spTree")?;
    let mut sub = IdCtx { fresh: true, ..Default::default() };
    let mut children = sp_tree(imp, &dpart, &mut sub, tree, depth + 1);
    if children.is_empty() {
        return None;
    }
    // Empty placeholder nodes show their prompt from the data model, by the shapes' `modelId`.
    let prompts = kition_office_core::smartart::prompts(&data.root);
    if !prompts.is_empty() {
        node_prompts(tree, &mut children, &prompts, 0);
    }
    let mut g = frame.clone();
    let x = frame.xfrm.unwrap_or_default();
    // The diagram itself travels with the group, written back while its shapes are as read.
    g.diagram = kept_diagram(imp, part, e, &drawing_ref, &children);
    g.kind = ShapeKind::Group { children, child: Xfrm::new(0.0, 0.0, x.w, x.h) };
    Some(g)
}

/// Give the shapes read from drawing tree `tree` (in its element order, groups within) the
/// prompts of the data model's placeholder nodes they show (see [`kition_slide_model::Shape::prompts`]).
fn node_prompts(tree: &El, shapes: &mut [Shape], prompts: &std::collections::HashMap<&str, Vec<Option<&str>>>, depth: usize) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    let read: Vec<&El> = tree.elements().filter(|e| matches!(e.local(), "sp" | "pic" | "grpSp" | "cxnSp" | "graphicFrame")).collect();
    // Shapes and their elements pair up only when every element was read.
    if read.len() != shapes.len() {
        return;
    }
    for (e, shape) in read.into_iter().zip(shapes.iter_mut()) {
        if let ShapeKind::Group { children, .. } = &mut shape.kind {
            node_prompts(e, children.as_mut_slice(), prompts, depth + 1);
        } else if let Some(slots) = e.attr("modelId").and_then(|id| prompts.get(id)) {
            shape.prompts = slots.iter().map(|p| p.map(|p| p.trim().to_string()).filter(|p| !p.is_empty())).collect();
        }
    }
}

/// The smart graphic frame `e` kept with its parts (see [`kition_slide_model::KeptDiagram`]).
fn kept_diagram(imp: &mut Imp, part: &Part, e: &El, drawing_ref: &str, children: &[Shape]) -> Option<Box<kition_slide_model::KeptDiagram>> {
    let (el, parts) = super::carry_rels(imp, part, e)?;
    let r = part.rels.get(drawing_ref)?;
    if r.external {
        return None;
    }
    let target = r.target.clone();
    let (media, rels) = super::kept_part(imp, &target)?;
    Some(Box::new(kition_slide_model::KeptDiagram {
        frame: part.keep(&el),
        parts,
        drawing: Some(kition_slide_model::KeptRel { id: String::new(), rel_type: r.ty.clone(), media, rels }),
        drawing_ref: drawing_ref.to_string(),
        fingerprint: kition_slide_model::shapes_fingerprint(children, &imp.media),
    }))
}

fn table(imp: &mut Imp, part: &Part, tbl: &El) -> Table {
    let mut t = Table { style: "none".into(), first_row: false, band_row: false, ..Default::default() };
    if let Some(pr) = tbl.child("tblPr") {
        t.first_row = pr.bool("firstRow").unwrap_or(false);
        t.first_col = pr.bool("firstCol").unwrap_or(false);
        t.last_row = pr.bool("lastRow").unwrap_or(false);
        t.last_col = pr.bool("lastCol").unwrap_or(false);
        t.band_row = pr.bool("bandRow").unwrap_or(false);
        t.band_col = pr.bool("bandCol").unwrap_or(false);
        if let Some(id) = pr.child("tableStyleId").map(|e| e.text()) {
            t.style = crate::tables::style_from_guid(id.trim());
        }
    }
    t.cols = tbl
        .child("tblGrid")
        .map(|g| g.children_named("gridCol").take(1000).map(|c| dml::pt(c, "w").unwrap_or(72.0).clamp(0.0, 20_000.0)).collect())
        .unwrap_or_default();
    for tr in tbl.children_named("tr").take(1000) {
        let mut row = Row { identity: String::new(), height: dml::pt(tr, "h").unwrap_or(0.0).clamp(0.0, 20_000.0), cells: vec![] };
        for tc in tr.children_named("tc").take(1000) {
            row.cells.push(cell(imp, part, tc));
        }
        t.rows.push(row);
    }
    if t.cols.is_empty() {
        let n = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        t.cols = vec![72.0; n];
    }
    t.normalize();
    crate::member_ids::read(tbl, &mut t);
    t
}

fn cell(imp: &mut Imp, part: &Part, tc: &El) -> Cell {
    let mut c = Cell {
        grid_span: tc.u32("gridSpan").unwrap_or(1).clamp(1, 1000),
        row_span: tc.u32("rowSpan").unwrap_or(1).clamp(1, 1000),
        h_merge: tc.bool("hMerge").unwrap_or(false),
        v_merge: tc.bool("vMerge").unwrap_or(false),
        ..Default::default()
    };
    if let Some(tx) = tc.child("txBody") {
        c.text = dml::text_body(imp, part, tx);
    }
    if let Some(pr) = tc.child("tcPr") {
        if ["marL", "marR", "marT", "marB"].iter().any(|n| pr.has_attr(n)) {
            c.margins = Some([
                dml::pt(pr, "marL").unwrap_or(7.2),
                dml::pt(pr, "marR").unwrap_or(7.2),
                dml::pt(pr, "marT").unwrap_or(3.6),
                dml::pt(pr, "marB").unwrap_or(3.6),
            ]);
        }
        c.anchor = pr.attr("anchor").and_then(Anchor::from_xml);
        c.vertical = pr.attr("vert").and_then(TextDir::from_xml);
        for (i, n) in ["lnL", "lnR", "lnT", "lnB"].iter().enumerate() {
            if let Some(ln) = pr.child(n)
                && let Some(b) = c.borders.get_mut(i)
            {
                *b = Some(dml::line(imp, part, ln));
            }
        }
        c.diag_down = pr.child("lnTlToBr").map(|l| dml::line(imp, part, l));
        c.diag_up = pr.child("lnBlToTr").map(|l| dml::line(imp, part, l));
        c.fill = dml::fill_of(imp, part, pr);
    }
    c
}
