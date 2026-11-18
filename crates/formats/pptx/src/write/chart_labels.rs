//! Authored data-label formatting and manual layout in regenerated chart parts.

use kition_slide_model::chart::{ChartText, DataLabels, ManualLayout};

use super::super::{Exp, Out, dml};
use crate::xml::{A, W};

fn position(w: &mut W, l: &ManualLayout) {
    if l.x.is_none() && l.y.is_none() {
        return;
    }
    w.open0("c:layout");
    w.open0("c:manualLayout");
    for (tag, edge, value) in [("c:xMode", l.x_edge, l.x), ("c:yMode", l.y_edge, l.y)] {
        if value.is_some_and(f64::is_finite) {
            w.val(tag, if edge { "edge" } else { "factor" });
        }
    }
    for (tag, value) in [("c:x", l.x), ("c:y", l.y)] {
        if let Some(value) = value.filter(|v| v.is_finite()) {
            w.val(tag, value);
        }
    }
    w.close("c:manualLayout");
    w.close("c:layout");
}

fn extended_layout(w: &mut W, l: &ManualLayout, group: bool) {
    if l.w.is_none() && l.h.is_none() && (!group || (l.x.is_none() && l.y.is_none())) {
        return;
    }
    w.open0("c:extLst");
    w.open("c:ext", A::new().a("uri", "{CE6537A1-D6FC-4f65-9D91-7224C49458BB}"));
    w.open("c15:layout", A::new().a("xmlns:c15", "http://schemas.microsoft.com/office/drawing/2012/chart"));
    w.open0("c:manualLayout");
    if group {
        for (tag, value) in [("c:x", l.x), ("c:y", l.y)] {
            if let Some(value) = value.filter(|v| v.is_finite()) {
                w.val(tag, value);
            }
        }
    }
    for (tag, value) in [("c:w", l.w), ("c:h", l.h)] {
        if let Some(value) = value.filter(|v| v.is_finite() && (0.0..=1.0).contains(v)) {
            w.val(tag, value);
        }
    }
    w.close("c:manualLayout");
    w.close("c15:layout");
    w.close("c:ext");
    w.close("c:extLst");
}

pub(super) fn text(w: &mut W, t: &ChartText) {
    if *t == ChartText::default() {
        return;
    }
    w.open0("c:txPr");
    w.empty(
        "a:bodyPr",
        A::new()
            .o("rot", t.rot.filter(|r| r.is_finite()).map(|r| (r.clamp(-360.0, 360.0) * 60_000.0).round() as i64))
            .o("vert", t.vertical.then_some("vert")),
    );
    w.empty0("a:lstStyle");
    w.open0("a:p");
    paragraph(w, t);
    w.close("a:p");
    w.close("c:txPr");
}

fn properties(w: &mut W, x: &mut Exp, o: &mut Out, l: &DataLabels, group: bool, source_linked: bool) {
    if let Some(layout) = &l.layout
        && !group
    {
        position(w, layout);
    }
    if let Some(code) = &l.num_fmt {
        w.empty("c:numFmt", A::new().a("formatCode", code).a("sourceLinked", 0));
    } else if source_linked {
        // Point formats are resolved during import; None over a custom parent is a reset.
        w.empty("c:numFmt", A::new().a("formatCode", "General").a("sourceLinked", 1));
    }
    if l.fill.is_some() || l.line.is_some() {
        w.open0("c:spPr");
        if let Some(fill) = &l.fill {
            dml::fill(w, x, o, fill);
        }
        if let Some(line) = &l.line {
            dml::line(w, x, o, "a:ln", line);
        }
        w.close("c:spPr");
    }
    text(w, &l.text);
    if let Some(pos) = &l.pos {
        w.val("c:dLblPos", pos);
    }
    for (tag, value) in [
        ("c:showLegendKey", l.legend_key),
        ("c:showVal", l.value),
        ("c:showCatName", l.category),
        ("c:showSerName", l.series),
        ("c:showPercent", l.percent),
        ("c:showBubbleSize", l.bubble_size),
    ] {
        w.val(tag, u8::from(value));
    }
    if let Some(separator) = &l.separator {
        w.elt("c:separator", separator);
    }
    if let Some(layout) = &l.layout {
        extended_layout(w, layout, group);
    }
}

pub(super) fn write(w: &mut W, x: &mut Exp, o: &mut Out, labels: &DataLabels) {
    w.open0("c:dLbls");
    let deleted: std::collections::HashSet<usize> = labels.deleted.iter().take(10_000).copied().collect();
    for index in labels.deleted.iter().take(10_000) {
        w.open0("c:dLbl");
        w.val("c:idx", index);
        w.val("c:delete", 1);
        w.close("c:dLbl");
    }
    for (index, label) in labels.points.iter().take(10_000).filter(|(i, _)| !deleted.contains(i)) {
        w.open0("c:dLbl");
        w.val("c:idx", index);
        properties(w, x, o, label, false, label.num_fmt.is_none() && labels.num_fmt.is_some());
        w.close("c:dLbl");
    }
    properties(w, x, o, labels, true, false);
    w.close("c:dLbls");
}

/// Shared paragraph defaults for rich titles and chart text containers.
pub(super) fn paragraph(w: &mut W, t: &ChartText) {
    w.open0("a:pPr");
    w.open(
        "a:defRPr",
        A::new()
            .o("sz", t.size.filter(|s| s.is_finite()).map(|s| (s.clamp(1.0, 400.0) * 100.0).round() as u32))
            .o("spc", t.spacing.filter(|s| s.is_finite()).map(|s| (s.clamp(-4000.0, 4000.0) * 100.0).round() as i32))
            .o("b", t.bold.map(u8::from))
            .o("i", t.italic.map(u8::from)),
    );
    if let Some(c) = &t.color {
        dml::solid(w, c);
    }
    if t.effects_explicit || t.shadow.is_some() || t.glow.is_some() {
        dml::effects(w, &kition_slide_model::style::Effects { outer_shadow: t.shadow.clone(), glow: t.glow.clone(), ..Default::default() });
    }
    for (tag, face) in [("a:latin", &t.font), ("a:ea", &t.font_ea), ("a:cs", &t.font_cs)] {
        if let Some(face) = face {
            w.empty(tag, A::new().a("typeface", face));
        }
    }
    w.close("a:defRPr");
    w.close("a:pPr");
}
