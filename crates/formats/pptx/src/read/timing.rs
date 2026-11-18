//! Slide transitions (`p:transition`) and animations (`p:timing`).

use kition_slide_model::Shape;
use kition_slide_model::anim::{AnimClass, AnimStart, Animation, KeptTiming, TextBuild, TimingTarget, Transition};

use super::shapes::IdCtx;
use super::{Imp, Part, dml};
use crate::maps;
use crate::xml::El;

fn ms_attr(e: &El, name: &str) -> Option<u32> {
    let v = e.attr(name)?.trim();
    if v == "indefinite" {
        return None;
    }
    v.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 86_400_000.0) as u32)
}

pub fn transition(imp: &mut Imp, part: &Part, t: &El) -> Option<Transition> {
    let effect = t.elements().find(|e| !matches!(e.local(), "sndAc" | "extLst"));
    let mut tr = Transition { advance_on_click: t.bool("advClick").unwrap_or(true), advance_after_ms: ms_attr(t, "advTm"), ..Default::default() };
    match effect.and_then(maps::transition_from_xml) {
        Some((k, o)) => {
            tr.kind = k;
            tr.option = o;
        }
        None => match effect {
            Some(e) => {
                tr.kind = e.local().to_string();
                tr.raw = Some(part.keep(t));
            }
            None => tr.kind = "none".into(),
        },
    }
    let default_dur = kition_slide_model::anim::TRANSITIONS.iter().find(|x| x.0 == tr.kind).map(|x| x.3).unwrap_or(1000);
    tr.duration_ms = ms_attr(t, "dur").unwrap_or(match t.attr("spd") {
        Some("fast") => 500,
        Some("med") => 750,
        Some("slow") => default_dur.max(1000),
        _ => default_dur,
    });
    if let Some(snd) = t.path(&["sndAc", "stSnd", "snd"]) {
        tr.sound = dml::media_ref(imp, part, snd);
    }
    if tr.kind == "none" && tr.advance_on_click && tr.advance_after_ms.is_none() && tr.sound.is_none() {
        return None;
    }
    Some(tr)
}

struct Found<'a> {
    ctn: &'a El,
    trigger: Option<u32>,
}

fn collect<'a>(e: &'a El, trigger: Option<u32>, out: &mut Vec<Found<'a>>, depth: usize) {
    if depth > 64 || out.len() > 10_000 {
        return;
    }
    for c in e.elements() {
        if c.is("cTn") && c.has_attr("presetClass") {
            out.push(Found { ctn: c, trigger });
            continue;
        }
        let mut trig = trigger;
        if c.is("seq")
            && let Some(ctn) = c.child("cTn")
            && ctn.attr("nodeType") == Some("interactiveSeq")
        {
            trig = ctn.path(&["stCondLst", "cond", "tgtEl", "spTgt"]).and_then(|s| s.u32("spid"));
        }
        collect(c, trig, out, depth + 1);
    }
}

fn max_dur(e: &El, depth: usize) -> Option<u32> {
    if depth > 64 {
        return None;
    }
    let mut best: Option<u32> = None;
    for c in e.elements() {
        if c.is("cTn")
            && let Some(d) = ms_attr(c, "dur")
            && d > 1
        {
            best = Some(best.map_or(d, |b: u32| b.max(d)));
        }
        if let Some(d) = max_dur(c, depth + 1) {
            best = Some(best.map_or(d, |b| b.max(d)));
        }
    }
    best
}

/// Flatten the timing tree into the Animation Pane list.
pub fn animations(ctx: &IdCtx, timing: &El, _shapes: &[Shape]) -> Vec<Animation> {
    let Some(tn) = timing.child("tnLst") else { return vec![] };
    let mut found = vec![];
    collect(tn, None, &mut found, 0);
    // Paragraph builds (`bldP build="p"`) by (spid, grpId).
    let by_para: Vec<(u32, String)> = timing
        .child("bldLst")
        .map(|b| {
            b.children_named("bldP")
                .filter(|p| p.attr("build") == Some("p"))
                .filter_map(|p| Some((p.u32("spid")?, p.attr("grpId").unwrap_or("0").to_string())))
                .collect()
        })
        .unwrap_or_default();
    let mut out: Vec<(Animation, u32, String)> = vec![];
    for f in found {
        let c = f.ctn;
        let class = AnimClass::from_xml(c.attr("presetClass").unwrap_or("entr"));
        let preset = c.u32("presetID").unwrap_or(0);
        let sub = c.u32("presetSubtype").unwrap_or(0);
        let Some(tgt) = c.find("spTgt") else { continue };
        let Some(file_id) = tgt.u32("spid") else { continue };
        let Some(shape) = ctx.lookup(file_id) else { continue };
        let paragraph = tgt.path(&["txEl", "pRg"]).and_then(|p| p.u32("st"));
        let effect = maps::effect_for_preset(class, preset).unwrap_or_else(|| maps::fallback_effect(class));
        let start = match c.attr("nodeType") {
            Some("clickEffect") => AnimStart::OnClick,
            Some("afterEffect") => AnimStart::AfterPrevious,
            _ => AnimStart::WithPrevious,
        };
        let mut a = Animation {
            shape,
            class,
            effect: effect.to_string(),
            option: maps::option_from_subtype(effect, sub),
            start,
            duration_ms: max_dur(c, 0).unwrap_or(0),
            delay_ms: c.path(&["stCondLst", "cond"]).and_then(|d| ms_attr(d, "delay")).unwrap_or(0),
            paragraph,
            trigger: f.trigger.and_then(|t| ctx.lookup(t)),
            preset_id: Some(preset),
            preset_subtype: Some(sub),
            auto_reverse: c.bool("autoRev").unwrap_or(false),
            smooth_start: c.f64("accel").map(|v| v / 100_000.0).unwrap_or(0.0),
            smooth_end: c.f64("decel").map(|v| v / 100_000.0).unwrap_or(0.0),
            ..Default::default()
        };
        a.repeat = match c.attr("repeatCount") {
            Some("indefinite") => u32::MAX,
            Some(v) => v.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| (v / 1000.0).round().clamp(1.0, 10_000.0) as u32).unwrap_or(1),
            None => 1,
        };
        if let Some(m) = c.find("animMotion") {
            a.path = m.attr("path").map(String::from);
        }
        if let Some(r) = c.find("animRot") {
            a.amount = r.f64("by").map(|v| v / 60_000.0);
            if a.effect == "spin" && a.amount.is_some_and(|v| v < 0.0) {
                a.option = "counterClockwise".into();
            }
        } else if let Some(s) = c.find("animScale").and_then(|s| s.child("by")) {
            a.amount = s.f64("x").map(|v| v / 100_000.0);
        }
        if let Some(clr) = c.find("animClr").and_then(|x| x.child("to")) {
            a.color = dml::color(clr);
        }
        let grp = c.attr("grpId").unwrap_or("0").to_string();
        // Collapse a paragraph-by-paragraph build into one animation.
        if paragraph.is_some() && by_para.iter().any(|(s, g)| *s == file_id && *g == grp) {
            if out.last().is_some_and(|(l, lf, lg)| *lf == file_id && *lg == grp && l.text_build == TextBuild::ByParagraph) {
                continue;
            }
            a.text_build = TextBuild::ByParagraph;
            a.paragraph = None;
        }
        out.push((a, file_id, grp));
    }
    out.into_iter().map(|(a, _, _)| a).collect()
}

/// Most bytes of a timing tree kept whole.
const MAX_KEPT_TIMING: usize = 4 << 20;

/// Prefixes a kept timing tree may use outside extension lists and alternate content (anything
/// else would need declaring ignorable where it is written).
const KNOWN_PREFIXES: &[&str] = &["p", "a", "r", "mc", "xml", "xmlns", ""];

/// The slide's timing tree kept whole (see [`KeptTiming`]), when every shape it names was read and
/// it uses no relationships (sounds) or unknown markup outside extensions.
pub fn kept(part: &Part, ctx: &IdCtx, timing: &El, anims: &[Animation]) -> Option<KeptTiming> {
    if super::has_rel_refs(timing) || !plain(timing, 0) {
        return None;
    }
    let mut spids = Vec::new();
    collect_spids(timing, &mut spids, 0);
    let mut targets: Vec<TimingTarget> = Vec::new();
    for spid in spids {
        if !targets.iter().any(|t| t.spid == spid) {
            targets.push(TimingTarget { spid, shape: ctx.lookup(spid)? });
        }
    }
    let digest = kition_slide_model::anim::timing_digest(anims, &targets)?;
    let xml = part.keep(timing);
    (xml.len() <= MAX_KEPT_TIMING).then_some(KeptTiming { xml, targets, digest })
}

/// No prefixes but known ones outside `p:extLst` and `mc:AlternateContent`.
fn plain(e: &El, depth: usize) -> bool {
    if depth > crate::xml::MAX_DEPTH {
        return false;
    }
    if e.is("extLst") || e.is("AlternateContent") {
        return true;
    }
    let known = |name: &str| KNOWN_PREFIXES.contains(&name.split_once(':').map_or("", |(p, _)| p));
    known(&e.name) && e.attrs.iter().all(|(k, _)| known(k)) && e.elements().all(|c| plain(c, depth + 1))
}

/// Every `spid` attribute value in `e`, in document order.
pub(crate) fn collect_spids(e: &El, out: &mut Vec<u32>, depth: usize) {
    if depth > crate::xml::MAX_DEPTH || out.len() > 100_000 {
        return;
    }
    if let Some(v) = e.attrs.iter().find(|(k, _)| k == "spid").and_then(|(_, v)| v.trim().parse::<u32>().ok()) {
        out.push(v);
    }
    for c in e.elements() {
        collect_spids(c, out, depth + 1);
    }
}
