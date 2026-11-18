//! DrawingML reading: colours, fills, lines, effects, transforms and text.

use kition_slide_color::{ColorTransform, Rgba, SchemeSlot};
use kition_slide_geom::{Xfrm, emu_to_pt};
use kition_slide_model::style::{
    ColorBase, ColorRef, Compound, Dash, Effects, Fill, Glow, Gradient, GradientShape, GradientStop, Line, LineCap, LineEnd, LineJoin, PatternFill,
    PictureFill, PictureMode, Reflection, Shadow,
};
use kition_slide_model::text::{
    Action, Align, Anchor, AutoFit, BodyProps, Bullet, Caps, Hyperlink, LevelStyle, ListStyle, ParaProps, Paragraph, Run, RunKind, RunProps, Spacing,
    Strike, TabStop, TextBody, TextDir,
};

use super::{Imp, Part};
use crate::xml::El;

pub fn pt(e: &El, name: &str) -> Option<f64> {
    e.i64(name).map(emu_to_pt)
}

/// 1/100 000 → fraction.
pub fn pct(e: &El, name: &str) -> Option<f64> {
    e.f64(name).map(|v| v / 100_000.0)
}

/// 1/60 000 degree → degrees.
pub fn ang(e: &El, name: &str) -> Option<f64> {
    e.f64(name).map(|v| v / 60_000.0)
}

fn lin_to_srgb(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// A colour element (`srgbClr`, `schemeClr`, …) with its transforms.
pub fn color_el(c: &El) -> Option<ColorRef> {
    let base = match c.local() {
        "srgbClr" => ColorBase::Rgb { rgb: Rgba::from_hex(c.attr("val").unwrap_or("000000")).unwrap_or(Rgba::BLACK) },
        "schemeClr" => ColorBase::Scheme { slot: c.attr("val").and_then(SchemeSlot::from_xml).unwrap_or(SchemeSlot::Tx1) },
        "sysClr" => ColorBase::System {
            name: c.attr("val").unwrap_or("windowText").to_string(),
            last: c.attr("lastClr").and_then(Rgba::from_hex).unwrap_or(if c.attr("val") == Some("window") { Rgba::WHITE } else { Rgba::BLACK }),
        },
        "prstClr" => ColorBase::Preset { name: c.attr("val").unwrap_or("black").to_string() },
        "scrgbClr" => {
            let f = |n: &str| lin_to_srgb(c.f64(n).unwrap_or(0.0) / 100_000.0);
            ColorBase::Rgb { rgb: Rgba::from_f64(f("r"), f("g"), f("b"), 1.0) }
        }
        "hslClr" => {
            let h = c.f64("hue").unwrap_or(0.0) / 60_000.0;
            let s = c.f64("sat").unwrap_or(0.0) / 100_000.0;
            let l = c.f64("lum").unwrap_or(0.0) / 100_000.0;
            ColorBase::Rgb { rgb: Rgba::from_hsl(h, s, l, 255) }
        }
        _ => return None,
    };
    let mods = c.elements().take(32).filter_map(|m| ColorTransform::from_xml(m.local(), m.i32("val"))).collect();
    Some(ColorRef { base, mods })
}

/// The first colour child of a container (`solidFill`, `fgClr`, `lnRef`…).
pub fn color(container: &El) -> Option<ColorRef> {
    container.elements().find_map(color_el)
}

const FILL_NAMES: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

/// The fill child of `parent`, if any.
pub fn fill_of(imp: &mut Imp, part: &Part, parent: &El) -> Option<Fill> {
    parent.elements().find(|e| FILL_NAMES.contains(&e.local())).and_then(|f| fill_el(imp, part, f))
}

pub fn fill_el(imp: &mut Imp, part: &Part, f: &El) -> Option<Fill> {
    Some(match f.local() {
        "noFill" => Fill::None,
        "solidFill" => Fill::Solid { color: color(f).unwrap_or(ColorRef::rgb(Rgba::BLACK)) },
        "gradFill" => Fill::Gradient(gradient(f)),
        "blipFill" => match blip_fill(imp, part, f) {
            Some(p) => Fill::Picture(p),
            None => Fill::None,
        },
        "pattFill" => Fill::Pattern(PatternFill {
            preset: f.attr("prst").unwrap_or("pct5").to_string(),
            fg: f.child("fgClr").and_then(color).unwrap_or(ColorRef::rgb(Rgba::BLACK)),
            bg: f.child("bgClr").and_then(color).unwrap_or(ColorRef::rgb(Rgba::WHITE)),
        }),
        "grpFill" => Fill::Group,
        _ => return None,
    })
}

pub fn gradient(f: &El) -> Gradient {
    let mut stops: Vec<GradientStop> = f
        .child("gsLst")
        .map(|l| {
            l.children_named("gs")
                .take(256)
                .map(|g| GradientStop { pos: pct(g, "pos").unwrap_or(0.0).clamp(0.0, 1.0), color: color(g).unwrap_or(ColorRef::rgb(Rgba::BLACK)) })
                .collect()
        })
        .unwrap_or_default();
    stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let shape = if let Some(l) = f.child("lin") {
        GradientShape::Linear { angle: ang(l, "ang").unwrap_or(0.0), scaled: l.bool("scaled").unwrap_or(false) }
    } else if let Some(p) = f.child("path") {
        let r = p.child("fillToRect");
        let g = |n: &str| r.and_then(|r| pct(r, n)).unwrap_or(0.0);
        GradientShape::Path { path: p.attr("path").unwrap_or("circle").to_string(), focus: [g("l"), g("t"), g("r"), g("b")] }
    } else {
        GradientShape::Linear { angle: 90.0, scaled: false }
    };
    let tile_rect = f.child("tileRect").and_then(|r| {
        let g = |name| r.attr(name).map_or(Some(0.0), |_| pct(r, name));
        Some([g("l")?, g("t")?, g("r")?, g("b")?])
    });
    Gradient { stops, shape, tile_rect, rotate_with_shape: f.bool("rotWithShape").unwrap_or(true) }
}

/// `blipFill` (either `a:` or `p:`) → picture fill, registering the image as media.
pub fn blip_fill(imp: &mut Imp, part: &Part, bf: &El) -> Option<PictureFill> {
    let blip = bf.child("blip");
    let media = blip.and_then(|b| media_ref(imp, part, b));
    let mut pf = blip_fill_with_media(bf, media?);
    // The blip's extensions travel as kept XML with the parts they name (an SVG original, the
    // HD Photo layers of artistic effects); left out when one of those parts has parts of its own.
    if let Some(ext) = blip.and_then(|b| b.child("extLst"))
        && let Some((el, rels)) = super::carry_rels(imp, part, ext)
    {
        pf.ext = Some(part.keep(&el));
        pf.ext_rels = rels;
    }
    Some(pf)
}

/// Picture properties after the relationship has been resolved to a media item.
pub(crate) fn blip_fill_with_media(bf: &El, media: kition_slide_model::MediaId) -> PictureFill {
    let blip = bf.child("blip");
    let mut pf = PictureFill { media, rotate_with_shape: bf.bool("rotWithShape").unwrap_or(true), ..Default::default() };
    if let Some(b) = blip {
        pf.adjust = blip_adjust(b);
        for e in b.elements().filter(|e| e.local() == "alphaModFix") {
            pf.alpha = Some(pct(e, "amt").unwrap_or(1.0).clamp(0.0, 1.0));
        }
    }
    if let Some(r) = bf.child("srcRect") {
        pf.crop = [pct(r, "l").unwrap_or(0.0), pct(r, "t").unwrap_or(0.0), pct(r, "r").unwrap_or(0.0), pct(r, "b").unwrap_or(0.0)]
            .map(|v| v.clamp(-10.0, 1.0));
    }
    if let Some(t) = bf.child("tile") {
        pf.mode = PictureMode::Tile {
            tx: pt(t, "tx").unwrap_or(0.0),
            ty: pt(t, "ty").unwrap_or(0.0),
            sx: pct(t, "sx").unwrap_or(1.0),
            sy: pct(t, "sy").unwrap_or(1.0),
            flip: t.attr("flip").unwrap_or("none").to_string(),
            align: t.attr("algn").unwrap_or("tl").to_string(),
        };
    } else {
        let r = bf.path(&["stretch", "fillRect"]);
        let g = |n: &str| r.and_then(|r| pct(r, n)).unwrap_or(0.0).clamp(-10.0, 1.0);
        pf.mode = PictureMode::Stretch { fill_rect: [g("l"), g("t"), g("r"), g("b")] };
    }
    pf
}

/// Pixel corrections shared by pictures and preserved object previews.
pub(crate) fn blip_adjust(blip: &El) -> kition_slide_model::style::PictureAdjust {
    let mut adjust = kition_slide_model::style::PictureAdjust::default();
    for e in blip.elements() {
        match e.local() {
            "lum" => {
                adjust.brightness = pct(e, "bright").unwrap_or(0.0).clamp(-1.0, 1.0);
                adjust.contrast = pct(e, "contrast").unwrap_or(0.0).clamp(-1.0, 1.0);
            }
            "grayscl" => adjust.grayscale = true,
            "biLevel" => adjust.bi_level = pct(e, "thresh").filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0)),
            "duotone" => {
                let cs: Vec<ColorRef> = e.elements().filter_map(color_el).collect();
                if let [a, b] = cs.as_slice() {
                    adjust.duotone = Some((a.clone(), b.clone()));
                }
            }
            "clrChange" => {
                if let Some(ColorRef { base: ColorBase::Rgb { rgb }, .. }) = e.child("clrFrom").and_then(color) {
                    adjust.clear_color = Some(rgb);
                }
            }
            _ => {}
        }
    }
    adjust
}

/// Media for an element with `r:embed` (or `r:link`).
pub fn media_ref(imp: &mut Imp, part: &Part, b: &El) -> Option<kition_slide_model::MediaId> {
    if let Some(id) = b.attr("r:embed").filter(|s| !s.is_empty())
        && let Some(r) = part.rels.get(id)
    {
        if r.external {
            return Some(imp.media_link(&r.target));
        }
        let t = r.target.clone();
        if let Some(m) = imp.media_for(&t) {
            return Some(m);
        }
    }
    if let Some(id) = b.attr("r:link").filter(|s| !s.is_empty())
        && let Some(r) = part.rels.get(id)
    {
        if r.external {
            return Some(imp.media_link(&r.target));
        }
        let t = r.target.clone();
        return imp.media_for(&t);
    }
    None
}

pub fn line(imp: &mut Imp, part: &Part, ln: &El) -> Line {
    let mut l = Line { width: pt(ln, "w").map(|w| w.clamp(0.0, 2000.0)), ..Default::default() };
    l.cap = ln.attr("cap").map(|c| match c {
        "rnd" => LineCap::Round,
        "sq" => LineCap::Square,
        _ => LineCap::Flat,
    });
    l.compound = ln.attr("cmpd").map(Compound::from_xml);
    l.pen_align = ln.attr("algn").filter(|a| matches!(*a, "ctr" | "in")).map(str::to_string);
    l.fill = fill_of(imp, part, ln);
    for e in ln.elements() {
        match e.local() {
            "prstDash" => l.dash = Some(Dash::Preset { name: e.attr("val").unwrap_or("solid").to_string() }),
            "custDash" => {
                let pattern = e.children_named("ds").take(64).map(|d| (pct(d, "d").unwrap_or(1.0), pct(d, "sp").unwrap_or(1.0))).collect();
                l.dash = Some(Dash::Custom { pattern });
            }
            "round" => l.join = Some(LineJoin::Round),
            "bevel" => l.join = Some(LineJoin::Bevel),
            "miter" => {
                l.join = Some(LineJoin::Miter);
                l.miter_limit = pct(e, "lim").filter(|v| v.is_finite() && *v >= 0.0).map(|v| v.min(10_000.0));
            }
            "headEnd" => l.head = line_end(e),
            "tailEnd" => l.tail = line_end(e),
            _ => {}
        }
    }
    l
}

fn line_end(e: &El) -> Option<LineEnd> {
    let kind = e.attr("type").unwrap_or("none");
    Some(LineEnd { kind: kind.to_string(), w: e.attr("w").unwrap_or("med").to_string(), len: e.attr("len").unwrap_or("med").to_string() })
}

fn shadow(e: &El, inner: bool) -> Shadow {
    Shadow {
        color: color(e).unwrap_or(ColorRef::rgb(Rgba::BLACK)),
        blur: pt(e, "blurRad").unwrap_or(0.0).max(0.0),
        dist: pt(e, "dist").unwrap_or(0.0),
        dir: ang(e, "dir").unwrap_or(0.0),
        inner,
        sx: pct(e, "sx").unwrap_or(1.0),
        sy: pct(e, "sy").unwrap_or(1.0),
        kx: ang(e, "kx").unwrap_or(0.0),
        ky: ang(e, "ky").unwrap_or(0.0),
        align: e.attr("algn").unwrap_or(if inner { "" } else { "b" }).to_string(),
        rotate_with_shape: e.bool("rotWithShape").unwrap_or(true),
    }
}

/// `effectLst` → effects.
pub fn effects(el: &El) -> Effects {
    let mut fx = Effects::default();
    for e in el.elements() {
        match e.local() {
            "outerShdw" => fx.outer_shadow = Some(shadow(e, false)),
            "prstShdw" => {
                let mut s = shadow(e, false);
                s.align = "b".into();
                fx.outer_shadow = Some(s);
            }
            "innerShdw" => fx.inner_shadow = Some(shadow(e, true)),
            "glow" => fx.glow = Some(Glow { color: color(e).unwrap_or(ColorRef::rgb(Rgba::BLACK)), radius: pt(e, "rad").unwrap_or(0.0).max(0.0) }),
            "softEdge" => fx.soft_edge = Some(pt(e, "rad").unwrap_or(0.0).max(0.0)),
            "reflection" => {
                fx.reflection = Some(Reflection {
                    blur: pt(e, "blurRad").unwrap_or(0.0),
                    start_alpha: pct(e, "stA").unwrap_or(1.0),
                    start_pos: pct(e, "stPos").unwrap_or(0.0),
                    end_alpha: pct(e, "endA").unwrap_or(0.0),
                    end_pos: pct(e, "endPos").unwrap_or(1.0),
                    dist: pt(e, "dist").unwrap_or(0.0),
                    sy: pct(e, "sy").unwrap_or(1.0),
                    sx: pct(e, "sx").unwrap_or(1.0),
                    dir: ang(e, "dir").unwrap_or(0.0),
                    fade_dir: ang(e, "fadeDir").unwrap_or(90.0),
                    kx: ang(e, "kx").unwrap_or(0.0),
                    ky: ang(e, "ky").unwrap_or(0.0),
                    align: e.attr("algn").unwrap_or("b").to_string(),
                    rotate_with_shape: e.bool("rotWithShape").unwrap_or(true),
                })
            }
            _ => {}
        }
    }
    fx
}

/// `a:xfrm` → (box, child box for groups).
pub fn xfrm(x: &El) -> (Xfrm, Option<Xfrm>) {
    let off = x.child("off");
    let ext = x.child("ext");
    let g = |e: Option<&El>, n: &str| e.and_then(|e| pt(e, n)).unwrap_or(0.0);
    let mut xf = Xfrm::new(g(off, "x"), g(off, "y"), g(ext, "cx").max(0.0), g(ext, "cy").max(0.0));
    xf.rot = ang(x, "rot").unwrap_or(0.0);
    if !xf.rot.is_finite() {
        xf.rot = 0.0;
    }
    xf.flip_h = x.bool("flipH").unwrap_or(false);
    xf.flip_v = x.bool("flipV").unwrap_or(false);
    let child = match (x.child("chOff"), x.child("chExt")) {
        (Some(o), Some(e)) => Some(Xfrm::new(g(Some(o), "x"), g(Some(o), "y"), g(Some(e), "cx").max(0.0), g(Some(e), "cy").max(0.0))),
        _ => None,
    };
    (xf, child)
}

// ---------------------------------------------------------------------------------------------
// Text

pub fn body_pr(part: &Part, b: &El) -> BodyProps {
    let mut p = BodyProps {
        inset_l: pt(b, "lIns"),
        inset_t: pt(b, "tIns"),
        inset_r: pt(b, "rIns"),
        inset_b: pt(b, "bIns"),
        anchor: b.attr("anchor").and_then(Anchor::from_xml),
        anchor_ctr: b.bool("anchorCtr"),
        wrap: b.attr("wrap").map(|w| w != "none"),
        vert: b.attr("vert").and_then(TextDir::from_xml),
        rot: ang(b, "rot"),
        columns: b.u32("numCol").map(|n| n.clamp(1, 16)),
        col_spacing: pt(b, "spcCol"),
        rtl_col: b.bool("rtlCol"),
        upright: b.bool("upright"),
        compat_line_spacing: b.bool("compatLnSpc"),
        scene3d_xml: b.child("scene3d").map(|e| part.keep(e)),
        text3d_xml: b.elements().find(|e| e.is("sp3d") || e.is("flatTx")).map(|e| part.keep(e)),
        ..Default::default()
    };
    for e in b.elements() {
        match e.local() {
            "normAutofit" => {
                p.autofit = Some(AutoFit::Shrink {
                    font_scale: pct(e, "fontScale").unwrap_or(1.0).clamp(0.01, 1.0),
                    line_reduction: pct(e, "lnSpcReduction").unwrap_or(0.0).clamp(0.0, 1.0),
                })
            }
            "extLst" => crate::autofit_ext::read(e, &mut p),
            "spAutoFit" => p.autofit = Some(AutoFit::Shape),
            "noAutofit" => p.autofit = Some(AutoFit::None),
            "prstTxWarp" => {
                // Explicit no-shape is a local value that blocks an inherited warp.
                p.warp = e.attr("prst").map(String::from);
                p.warp_adjust = e
                    .child("avLst")
                    .map(|av| {
                        av.elements()
                            .filter(|gd| gd.local() == "gd")
                            .filter_map(|gd| {
                                let v = gd.attr("fmla")?.strip_prefix("val ")?.trim().parse::<f64>().ok().filter(|v| v.is_finite())?;
                                Some((gd.attr("name")?.to_string(), v))
                            })
                            .take(8)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            _ => {}
        }
    }
    p
}

fn spacing(e: &El) -> Option<Spacing> {
    if let Some(p) = e.child("spcPct") {
        return Some(Spacing::Pct(pct(p, "val").unwrap_or(1.0).clamp(0.0, 100.0)));
    }
    e.child("spcPts").map(|p| Spacing::Pts(p.f64("val").unwrap_or(0.0).clamp(0.0, 1_000_000.0) / 100.0))
}

/// `a:pPr` / `a:lvlNpPr` → (level, paragraph props, default run props).
pub fn ppr(imp: &mut Imp, part: &Part, p: &El) -> (Option<u8>, ParaProps, Option<RunProps>) {
    let mut pp = ParaProps {
        align: p.attr("algn").and_then(Align::from_xml),
        margin_left: pt(p, "marL"),
        margin_right: pt(p, "marR"),
        indent: pt(p, "indent"),
        rtl: p.bool("rtl"),
        font_align: p.attr("fontAlgn").map(String::from),
        east_asian_line_break: p.bool("eaLnBrk"),
        latin_line_break: p.bool("latinLnBrk"),
        hanging_punct: p.bool("hangingPunct"),
        default_tab: pt(p, "defTabSz"),
        ..Default::default()
    };
    let lvl = p.u32("lvl").map(|l| l.min(8) as u8);
    let mut def = None;
    for e in p.elements() {
        match e.local() {
            "lnSpc" => pp.line_spacing = spacing(e),
            "spcBef" => pp.space_before = spacing(e),
            "spcAft" => pp.space_after = spacing(e),
            "buNone" => pp.bullet = Some(Bullet::None),
            "buChar" => pp.bullet = Some(Bullet::Char { char: e.attr("char").unwrap_or("•").chars().take(4).collect() }),
            "buAutoNum" => {
                pp.bullet = Some(Bullet::AutoNum {
                    scheme: e.attr("type").unwrap_or("arabicPeriod").to_string(),
                    start_at: e.u32("startAt").unwrap_or(1).clamp(1, 32767),
                })
            }
            "buBlip" => {
                if let Some(m) = e.child("blip").and_then(|b| media_ref(imp, part, b)) {
                    pp.bullet = Some(Bullet::Picture { media: m });
                }
            }
            "buFont" => {
                imp.note_face("buFont", e);
                pp.bullet_font = e.attr("typeface").map(String::from);
            }
            "buClr" => pp.bullet_color = color(e),
            "buSzPct" => pp.bullet_size = pct(e, "val").map(|v| v.clamp(0.25, 4.0)),
            "tabLst" => {
                pp.tabs = Some(
                    e.children_named("tab")
                        .take(64)
                        .map(|t| TabStop { pos: pt(t, "pos").unwrap_or(0.0), align: t.attr("algn").unwrap_or("l").to_string() })
                        .collect(),
                )
            }
            "defRPr" => def = Some(rpr(imp, part, e)),
            _ => {}
        }
    }
    (lvl, pp, def)
}

pub fn hyperlink(imp: &Imp, part: &Part, h: &El) -> Option<Hyperlink> {
    let rid = h.attr("r:id").unwrap_or("");
    let rel = part.rels.get(rid);
    let action = h.attr("action").unwrap_or("");
    let a = if let Some(rest) = action.strip_prefix("ppaction://hlinkshowjump?jump=") {
        match rest {
            "nextslide" => Action::NextSlide,
            "previousslide" => Action::PreviousSlide,
            "firstslide" => Action::FirstSlide,
            "lastslide" => Action::LastSlide,
            "endshow" => Action::EndShow,
            "lastslideviewed" => Action::LastViewed,
            _ => return None,
        }
    } else if action.starts_with("ppaction://hlinksldjump") {
        let slide = rel.and_then(|r| imp.slide_by_part.get(&crate::opc::norm(&r.target)).copied())?;
        Action::Slide { slide }
    } else if let Some(q) = action.strip_prefix("ppaction://customshow?") {
        let id = q.split('&').find_map(|kv| kv.strip_prefix("id=")).unwrap_or("0");
        Action::CustomShow { name: imp.custom_show_names.get(id).cloned().unwrap_or_else(|| id.to_string()) }
    } else if action.starts_with("ppaction://program") {
        Action::Program { path: rel.map(|r| r.target.clone()).unwrap_or_default() }
    } else if action.starts_with("ppaction://media") {
        Action::PlayMedia
    } else if action.is_empty() || action.starts_with("ppaction://hlinkfile") {
        let r = rel?;
        if r.target.is_empty() {
            return None;
        }
        if !r.external
            && let Some(s) = imp.slide_by_part.get(&crate::opc::norm(&r.target))
        {
            Action::Slide { slide: *s }
        } else {
            Action::Url { url: r.target.clone() }
        }
    } else {
        return None;
    };
    Some(Hyperlink { action: a, tooltip: h.attr("tooltip").unwrap_or("").to_string(), highlight_click: h.bool("highlightClick").unwrap_or(false) })
}

pub fn rpr(imp: &mut Imp, part: &Part, r: &El) -> RunProps {
    let mut p = RunProps {
        lang: r.attr("lang").map(String::from),
        size: r.f64("sz").map(|s| (s / 100.0).clamp(1.0, 4000.0)),
        bold: r.bool("b"),
        italic: r.bool("i"),
        underline: r.attr("u").map(String::from),
        strike: r.attr("strike").map(|s| match s {
            "sngStrike" => Strike::Single,
            "dblStrike" => Strike::Double,
            _ => Strike::None,
        }),
        baseline: pct(r, "baseline").map(|b| b.clamp(-10.0, 10.0)),
        caps: r.attr("cap").map(|c| match c {
            "small" => Caps::Small,
            "all" => Caps::All,
            _ => Caps::None,
        }),
        spacing: r.f64("spc").map(|s| (s / 100.0).clamp(-4000.0, 4000.0)),
        kern: r.f64("kern").map(|k| (k / 100.0).clamp(0.0, 4000.0)),
        no_proof: r.bool("noProof"),
        alt_lang: r.attr("altLang").map(String::from),
        dirty: r.bool("dirty"),
        smt_clean: r.bool("smtClean"),
        ..Default::default()
    };
    for e in r.elements() {
        match e.local() {
            "ln" => p.outline = Some(line(imp, part, e)),
            n if FILL_NAMES.contains(&n) => p.fill = fill_el(imp, part, e),
            "effectLst" => {
                let fx = effects(e);
                p.effects_explicit = true;
                p.shadow = fx.outer_shadow;
                p.glow = fx.glow;
                p.reflection = fx.reflection;
            }
            "highlight" => p.highlight = color(e),
            "uFill" => p.underline_color = e.child("solidFill").and_then(color),
            "latin" | "ea" | "cs" | "sym" => {
                imp.note_face(e.local(), e);
                let f = e.attr("typeface").filter(|t| !t.is_empty()).map(String::from);
                match e.local() {
                    "latin" => p.font = f,
                    "ea" => p.font_ea = f,
                    "cs" => p.font_cs = f,
                    _ => p.font_sym = f,
                }
            }
            "hlinkClick" => p.link = hyperlink(imp, part, e),
            _ => {}
        }
    }
    p
}

/// `a:lstStyle`, `p:titleStyle`, `a:defaultTextStyle`… → list style.
pub fn list_style(imp: &mut Imp, part: &Part, ls: &El) -> ListStyle {
    let mut out = ListStyle::default();
    for e in ls.elements() {
        let n = e.local();
        if n == "defPPr" {
            let (_, para, run) = ppr(imp, part, e);
            out.default_level = Some(Box::new(LevelStyle { para, run: run.unwrap_or_default() }));
            continue;
        }
        let Some(l) = n.strip_prefix("lvl").and_then(|r| r.strip_suffix("pPr")).and_then(|d| d.parse::<u8>().ok()) else { continue };
        if !(1..=9).contains(&l) {
            continue;
        }
        let (_, para, run) = ppr(imp, part, e);
        out.set(l - 1, LevelStyle { para, run: run.unwrap_or_default() });
    }
    out
}

pub fn text_body(imp: &mut Imp, part: &Part, tx: &El) -> TextBody {
    let body = tx.child("bodyPr").map(|b| body_pr(part, b)).unwrap_or_default();
    let list_style = tx.child("lstStyle").map(|l| list_style(imp, part, l)).unwrap_or_default();
    let paragraphs = tx.children_named("p").take(100_000).map(|p| paragraph(imp, part, p)).collect();
    TextBody { body, list_style, paragraphs }
}

fn has_math_structure(e: &El, depth: usize) -> bool {
    if depth > crate::xml::MAX_DEPTH {
        return false;
    }
    matches!(
        e.local(),
        "acc"
            | "bar"
            | "borderBox"
            | "box"
            | "d"
            | "eqArr"
            | "f"
            | "func"
            | "groupChr"
            | "limLow"
            | "limUpp"
            | "m"
            | "nary"
            | "phant"
            | "rad"
            | "sPre"
            | "sSub"
            | "sSubSup"
            | "sSup"
    ) || e.elements().any(|c| has_math_structure(c, depth + 1))
}

pub fn paragraph(imp: &mut Imp, part: &Part, p: &El) -> Paragraph {
    let mut para = Paragraph::default();
    let mut default_run = None;
    for e in p.elements() {
        match e.local() {
            "pPr" => {
                let (lvl, pp, run) = ppr(imp, part, e);
                default_run = run;
                para.level = lvl.unwrap_or(0);
                para.props = pp;
            }
            "r" => {
                let props = e.child("rPr").map(|r| rpr(imp, part, r)).unwrap_or_default();
                let text: String = e.child("t").map(|t| t.text()).unwrap_or_default();
                // A vertical tab or line feed inside text is a soft break in some writers (and the
                // export writes both as `a:br`).
                for (i, piece) in text.split(['\u{b}', '\n']).enumerate() {
                    if i > 0 {
                        para.runs.push(Run { text: String::new(), props: props.clone(), kind: RunKind::Break });
                    }
                    if !piece.is_empty() {
                        para.runs.push(Run { text: piece.to_string(), props: props.clone(), kind: RunKind::Text });
                    }
                }
            }
            "br" => {
                let props = e.child("rPr").map(|r| rpr(imp, part, r)).unwrap_or_default();
                para.runs.push(Run { text: String::new(), props, kind: RunKind::Break });
            }
            "fld" => {
                let props = e.child("rPr").map(|r| rpr(imp, part, r)).unwrap_or_default();
                let text = e.child("t").map(|t| t.text()).unwrap_or_default();
                para.runs.push(Run { text, props, kind: RunKind::Field { field: e.attr("type").unwrap_or("").to_string() } });
            }
            "m" => {
                // Office Math (a14:m) — keep the OMML and its plain text.
                let mut text = String::new();
                let mut ts = vec![];
                e.find_all("t", &mut ts);
                for t in ts {
                    text.push_str(&t.text());
                }
                let omml = e.elements().map(|c| c.to_xml()).collect::<String>();
                if text.is_empty() && e.elements().any(|c| has_math_structure(c, 0)) {
                    // Empty-base equations still contain visible operators and delimiters. Give
                    // the inline object one model character for layout, caret movement and deletion;
                    // the original OMML stays unchanged and remains the export source of truth.
                    text.push('\u{fffc}');
                }
                // Math runs carry DrawingML character properties inside OMML, not on a14:m.
                let mut properties = vec![];
                e.find_all("rPr", &mut properties);
                let props = properties
                    .into_iter()
                    .find(|r| r.child("latin").is_some() || r.has_attr("sz") || r.has_attr("lang"))
                    .map(|r| rpr(imp, part, r))
                    .unwrap_or_default();
                para.runs.push(Run { text, props, kind: RunKind::Math { omml } });
            }
            "endParaRPr" => para.end_props = rpr(imp, part, e),
            _ => {}
        }
    }
    // The model stores explicit run properties, so materialize paragraph defaults before
    // resolving the lower-priority list/master styles. Include breaks, fields and the end
    // properties: an empty paragraph still contributes a line at its default font size.
    if let Some(defaults) = default_run {
        for run in &mut para.runs {
            run.props.inherit(&defaults);
        }
        para.end_props.inherit(&defaults);
    }
    para
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn gradient_tile_rectangle_preserves_signed_offsets_and_defaults_missing_edges() {
        for (tile, expected) in [
            ("", None),
            ("<a:tileRect/>", Some([0.0; 4])),
            ("<a:tileRect l=\"25000\" r=\"-100000\" b=\"-50000\"/>", Some([0.25, 0.0, -1.0, -0.5])),
            ("<a:tileRect l=\"NaN\"/>", None),
            ("<a:tileRect r=\"inf\"/>", None),
        ] {
            let xml = format!("<a:gradFill><a:path path=\"circle\"/>{tile}</a:gradFill>");
            let doc = parse(xml.as_bytes()).unwrap();
            assert_eq!(gradient(&doc.root).tile_rect, expected, "{tile}");
        }
    }

    #[test]
    fn legacy_native_gradient_defaults_to_no_tile_and_new_offsets_round_trip() {
        let json = r#"{"stops":[],"shape":{"kind":"linear","angle":45.0}}"#;
        let mut fill: Gradient = serde_json::from_str(json).unwrap();
        assert_eq!(fill.tile_rect, None);
        assert!(serde_json::to_value(&fill).unwrap().get("tileRect").is_none());
        fill.tile_rect = Some([-0.25, 0.0, -1.0, -0.5]);
        let encoded = serde_json::to_value(&fill).unwrap();
        assert_eq!(encoded["tileRect"], serde_json::json!([-0.25, 0.0, -1.0, -0.5]));
        assert_eq!(serde_json::from_value::<Gradient>(encoded).unwrap(), fill);
    }

    #[test]
    fn picture_fill_rotation_defaults_to_true_and_preserves_explicit_values() {
        assert!(PictureFill::default().rotate_with_shape);
        let old_json: PictureFill = serde_json::from_str(r#"{"media":7}"#).unwrap();
        assert!(old_json.rotate_with_shape);
        for (attribute, expected) in [("", true), ("rotWithShape=\"1\"", true), ("rotWithShape=\"0\"", false), ("rotWithShape=\"false\"", false)] {
            let xml = format!("<a:blipFill xmlns:a=\"x\" {attribute}><a:stretch><a:fillRect/></a:stretch></a:blipFill>");
            let doc = parse(xml.as_bytes()).unwrap();
            assert_eq!(blip_fill_with_media(&doc.root, kition_slide_model::MediaId(7)).rotate_with_shape, expected);
        }
    }

    #[test]
    fn picture_bilevel_preserves_zero_and_rejects_nonfinite_thresholds() {
        for (attribute, expected) in [
            ("", None),
            ("thresh=\"0\"", Some(0.0)),
            ("thresh=\"25000\"", Some(0.25)),
            ("thresh=\"100000\"", Some(1.0)),
            ("thresh=\"-5\"", Some(0.0)),
            ("thresh=\"200000\"", Some(1.0)),
            ("thresh=\"NaN\"", None),
            ("thresh=\"inf\"", None),
        ] {
            let xml = format!("<a:blipFill><a:blip><a:biLevel {attribute}/></a:blip></a:blipFill>");
            let doc = parse(xml.as_bytes()).unwrap();
            let fill = blip_fill_with_media(&doc.root, kition_slide_model::MediaId(7));
            assert_eq!(fill.adjust.bi_level, expected);
        }
    }

    #[test]
    fn colors_and_transforms() {
        let d = parse(
            br#"<a:solidFill xmlns:a="x"><a:schemeClr val="accent2"><a:lumMod val="75000"/><a:alpha val="50000"/></a:schemeClr></a:solidFill>"#,
        )
        .unwrap();
        let c = color(&d.root).unwrap();
        assert_eq!(c.base, ColorBase::Scheme { slot: SchemeSlot::Accent2 });
        assert_eq!(c.mods, vec![ColorTransform::LumMod(75000), ColorTransform::Alpha(50000)]);
        let d = parse(br#"<x><a:sysClr val="window" lastClr="FFFFFF"/></x>"#).unwrap();
        assert!(matches!(color(&d.root).unwrap().base, ColorBase::System { last: Rgba::WHITE, .. }));
        let d = parse(br#"<x><a:scrgbClr r="100000" g="0" b="0"/></x>"#).unwrap();
        assert_eq!(color(&d.root).unwrap().base, ColorBase::Rgb { rgb: Rgba::rgb(255, 0, 0) });
    }

    fn read_paragraph(xml: &str) -> Paragraph {
        let bytes = crate::export(&kition_slide_model::defaults::new_slide_file(None)).unwrap();
        let pkg = crate::opc::Package::open(&bytes).unwrap();
        let doc = parse(xml.as_bytes()).unwrap();
        let part = Part::new(&pkg, "ppt/slides/slide1.xml", &doc);
        let mut imp = Imp {
            pkg: &pkg,
            media: Vec::new(),
            media_by_part: Default::default(),
            next: 0,
            slide_by_part: Default::default(),
            custom_show_names: Default::default(),
            faces: Default::default(),
        };
        paragraph(&mut imp, &part, &doc.root)
    }

    #[test]
    fn paragraph_defaults_apply_to_text_breaks_fields_and_end_properties() {
        let p = read_paragraph(
            r#"<a:p xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                <a:pPr><a:defRPr sz="2800" b="1" lang="en-US"><a:latin typeface="Inter"/>
                    <a:solidFill><a:srgbClr val="5645D4"/></a:solidFill></a:defRPr></a:pPr>
                <a:r><a:t>First</a:t></a:r><a:br/>
                <a:fld id="synthetic-field" type="slidenum"><a:rPr b="0"/><a:t>1</a:t></a:fld>
                <a:r><a:rPr sz="1600" b="0"/><a:t>Override</a:t></a:r>
                <a:endParaRPr sz="2000"/>
            </a:p>"#,
        );
        assert_eq!(p.runs.len(), 4);
        for run in &p.runs {
            assert_eq!(run.props.font.as_deref(), Some("Inter"));
            assert_eq!(run.props.lang.as_deref(), Some("en-US"));
            assert_eq!(run.props.fill, Some(Fill::solid(ColorRef::rgb(Rgba::rgb(86, 69, 212)))));
        }
        assert_eq!(p.runs[0].props.size, Some(28.0));
        assert_eq!(p.runs[1].props.size, Some(28.0));
        assert_eq!(p.runs[2].props.size, Some(28.0));
        assert_eq!(p.runs[3].props.size, Some(16.0));
        assert_eq!(p.runs[0].props.bold, Some(true));
        assert_eq!(p.runs[2].props.bold, Some(false));
        assert_eq!(p.runs[3].props.bold, Some(false));
        assert_eq!(p.end_props.size, Some(20.0));
        assert_eq!(p.end_props.bold, Some(true));
    }

    #[test]
    fn math_keeps_authored_font_and_original_structure() {
        let p = read_paragraph(
            r#"<a:p><a14:m><m:oMath><m:r><m:rPr><m:sty m:val="i"/></m:rPr>
            <a:rPr sz="2400"><a:latin typeface="Inter"/></a:rPr><m:t>x</m:t></m:r>
            <m:sSup><m:e><m:r><m:t>y</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup>
            </m:oMath></a14:m></a:p>"#,
        );
        assert_eq!(p.runs[0].props.font.as_deref(), Some("Inter"));
        assert_eq!(p.runs[0].props.size, Some(24.0));
        assert_eq!(p.runs[0].text, "xy2");
        assert!(matches!(&p.runs[0].kind, RunKind::Math { omml } if omml.contains("m:sSup")));
    }

    #[test]
    fn empty_base_math_is_one_atomic_object_and_preserves_export_source() {
        let p =
            read_paragraph(r#"<a:p><a:r><a:t>A</a:t></a:r><a14:m><m:oMath><m:d><m:e/><m:e/></m:d></m:oMath></a14:m><a:r><a:t>B</a:t></a:r></a:p>"#);
        assert_eq!(p.char_len(), 3);
        assert_eq!(p.runs[1].text, "\u{fffc}");
        let RunKind::Math { omml } = &p.runs[1].kind else { panic!("expected a preserved equation") };
        assert_eq!(omml, "<m:oMath><m:d><m:e/><m:e/></m:d></m:oMath>");
        let body = TextBody { paragraphs: vec![p], ..Default::default() };
        let mut copied = TextBody::default();
        assert_eq!(kition_slide_model::edit::insert_body(&mut copied, (0, 0), &body), (0, 3));
        assert_eq!(copied.paragraphs[0].runs, body.paragraphs[0].runs);
        kition_slide_model::edit::delete(&mut copied, (0, 1), (0, 2));
        assert_eq!(copied.text(), "AB");
        assert!(copied.paragraphs[0].runs.iter().all(|r| r.kind == RunKind::Text));

        let mut slide_file = kition_slide_model::defaults::new_slide_file(None);
        std::sync::Arc::make_mut(&mut slide_file.slides[0]).shapes[0].text = Some(body.clone());
        let bytes = crate::export(&slide_file).unwrap();
        let pkg = crate::opc::Package::open(&bytes).unwrap();
        let xml = std::str::from_utf8(pkg.get("ppt/slides/slide1.xml").unwrap()).unwrap();
        let RunKind::Math { omml } = &body.paragraphs[0].runs[1].kind else { panic!("expected a preserved equation") };
        assert!(xml.contains(omml));
        assert!(!omml.contains('\u{fffc}'));
        let imported = crate::import(&bytes).unwrap();
        let equation = imported.slides[0]
            .shapes
            .iter()
            .filter_map(|s| s.text.as_ref())
            .flat_map(|b| &b.paragraphs)
            .flat_map(|p| &p.runs)
            .find(|r| matches!(r.kind, RunKind::Math { .. }))
            .unwrap();
        assert_eq!(equation.text, "\u{fffc}");
        assert_eq!(equation.kind, body.paragraphs[0].runs[1].kind);
    }

    #[test]
    fn empty_math_container_does_not_create_a_phantom_object() {
        for xml in ["<a:p><a14:m/></a:p>", "<a:p><a14:m><m:oMath/></a14:m></a:p>", "<a:p><a14:m><m:oMath><m:r><m:t/></m:r></m:oMath></a14:m></a:p>"] {
            let p = read_paragraph(xml);
            assert_eq!(p.char_len(), 0);
            assert!(p.runs[0].text.is_empty());
        }
        let p = read_paragraph("<a:p><a14:m><m:oMath><m:acc><m:e/></m:acc></m:oMath></a14:m></a:p>");
        assert_eq!(p.char_len(), 1);
        assert_eq!(p.runs[0].text, "\u{fffc}");
    }

    #[test]
    fn empty_paragraph_keeps_default_font_size() {
        let p = read_paragraph(r#"<a:p><a:pPr><a:defRPr sz="3200"/></a:pPr></a:p>"#);
        assert!(p.runs.is_empty());
        assert_eq!(p.end_props.size, Some(32.0));
    }

    #[test]
    fn body_props() {
        let d = parse(br#"<a:bodyPr lIns="91440" anchor="ctr" wrap="none" vert="vert270" numCol="2"><a:normAutofit fontScale="62500" lnSpcReduction="20000"/></a:bodyPr>"#).unwrap();
        let part = Part { name: "body.xml".into(), rels: Default::default(), ns: d.ns_decls() };
        let b = body_pr(&part, &d.root);
        assert_eq!(b.inset_l, Some(7.2));
        assert_eq!(b.anchor, Some(Anchor::Middle));
        assert_eq!(b.wrap, Some(false));
        assert_eq!(b.vert, Some(TextDir::Vertical270));
        assert_eq!(b.columns, Some(2));
        assert_eq!(b.autofit, Some(AutoFit::Shrink { font_scale: 0.625, line_reduction: 0.2 }));
        // Generic bodies (including table cells) do not acquire shape layout state.
        assert_eq!(b.autofit_saved, None);
    }
}
