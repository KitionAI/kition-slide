//! Chart parts: a `c:chartSpace` with cached categories and values (no embedded workbook).

use kition_slide_model::chart::{AxisKind, Chart, ChartText, ChartType, Series};
use kition_slide_model::style::Fill;

use super::{Exp, Out};
use crate::opc::{NS_A, NS_C, NS_R};
use crate::xml::{A, W};

#[path = "chart_labels.rs"]
mod labels;

/// Column letter for a 0-based index (A, B, … Z, AA…).
fn col(i: usize) -> String {
    let mut n = i + 1;
    let mut s = String::new();
    while n > 0 && s.len() < 4 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

fn simple_fill(w: &mut W, f: &Fill) {
    match f {
        Fill::None => w.empty0("a:noFill"),
        Fill::Solid { color } => super::dml::solid(w, color),
        Fill::Gradient(_) | Fill::Picture(_) | Fill::Pattern(_) | Fill::Group | Fill::Background => {
            if let Fill::Gradient(g) = f
                && let Some(s) = g.stops.first()
            {
                super::dml::solid(w, &s.color);
            }
        }
    }
}

fn sp_pr(w: &mut W, fill: Option<&Fill>, line: Option<&kition_slide_model::Line>) {
    if fill.is_none() && line.is_none() {
        return;
    }
    w.open0("c:spPr");
    if let Some(f) = fill {
        simple_fill(w, f);
    }
    if let Some(l) = line {
        w.open("a:ln", A::new().o("w", l.width.map(super::dml::emu_pos)));
        if let Some(f) = &l.fill {
            simple_fill(w, f);
        }
        w.close("a:ln");
    }
    w.close("c:spPr");
}

fn rich(w: &mut W, text: &str, style: &ChartText) {
    w.open0("c:tx");
    w.open0("c:rich");
    w.empty0("a:bodyPr");
    w.empty0("a:lstStyle");
    for line in text.split('\n') {
        w.open0("a:p");
        if *style != ChartText::default() {
            labels::paragraph(w, style);
        }
        w.open0("a:r");
        w.elt("a:t", line);
        w.close("a:r");
        w.close("a:p");
    }
    w.close("c:rich");
    w.close("c:tx");
}

fn title(w: &mut W, text: &str, style: &ChartText) {
    w.open0("c:title");
    rich(w, text, style);
    w.val("c:overlay", 0);
    w.close("c:title");
}

fn str_cache(w: &mut W, f: &str, vals: &[String]) {
    w.open0("c:strRef");
    w.elt("c:f", f);
    w.open0("c:strCache");
    w.val("c:ptCount", vals.len());
    for (i, v) in vals.iter().enumerate() {
        w.open("c:pt", A::new().a("idx", i));
        w.elt("c:v", v);
        w.close("c:pt");
    }
    w.close("c:strCache");
    w.close("c:strRef");
}

fn num_cache(w: &mut W, f: &str, vals: &[Option<f64>]) {
    w.open0("c:numRef");
    w.elt("c:f", f);
    w.open0("c:numCache");
    w.elt("c:formatCode", "General");
    w.val("c:ptCount", vals.len());
    for (i, v) in vals.iter().enumerate() {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            w.open("c:pt", A::new().a("idx", i));
            w.elt("c:v", &fmt_num(v));
            w.close("c:pt");
        }
    }
    w.close("c:numCache");
    w.close("c:numRef");
}

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    Radar,
    Bubble,
}

fn group_of(k: ChartType) -> Group {
    match k {
        ChartType::Line | ChartType::LineMarkers | ChartType::StackedLine | ChartType::Stock => Group::Line,
        ChartType::Area | ChartType::StackedArea | ChartType::Surface => Group::Area,
        ChartType::Pie => Group::Pie,
        ChartType::Doughnut | ChartType::Sunburst => Group::Doughnut,
        ChartType::Scatter => Group::Scatter,
        ChartType::Bubble => Group::Bubble,
        ChartType::Radar | ChartType::FilledRadar => Group::Radar,
        _ => Group::Bar,
    }
}

#[allow(clippy::too_many_arguments)]
fn marker(
    w: &mut W,
    x: &mut Exp,
    o: &mut Out,
    symbol: Option<&str>,
    size: Option<f64>,
    fill: Option<&Fill>,
    line: Option<&kition_slide_model::Line>,
    effects: Option<&kition_slide_model::style::Effects>,
) {
    if symbol.is_none() && size.is_none() && fill.is_none() && line.is_none() && effects.is_none() {
        return;
    }
    w.open0("c:marker");
    if let Some(symbol) = symbol.filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric())) {
        w.val("c:symbol", symbol);
    }
    if let Some(size) = size.filter(|s| s.is_finite()) {
        w.val("c:size", size.clamp(2.0, 72.0).round() as u32);
    }
    if fill.is_some() || line.is_some() || effects.is_some() {
        w.open0("c:spPr");
        if let Some(fill) = fill {
            super::dml::fill(w, x, o, fill);
        }
        if let Some(line) = line {
            super::dml::line(w, x, o, "a:ln", line);
        }
        if let Some(effects) = effects {
            super::dml::effects(w, effects);
        }
        w.close("c:spPr");
    }
    w.close("c:marker");
}

fn series(w: &mut W, x: &mut Exp, o: &mut Out, c: &Chart, s: &Series, i: usize, kind: ChartType) {
    let g = group_of(kind);
    w.open0("c:ser");
    w.val("c:idx", i);
    w.val("c:order", i);
    w.open0("c:tx");
    str_cache(w, &format!("Sheet1!${}$1", col(i + 1)), std::slice::from_ref(&s.name));
    w.close("c:tx");
    sp_pr(w, s.fill.as_ref(), s.line.as_ref());
    if g == Group::Bar {
        w.val("c:invertIfNegative", 0);
    }
    if matches!(g, Group::Line | Group::Scatter | Group::Radar) {
        let sym = match (&s.marker, kind) {
            (Some(m), _) if !m.is_empty() => Some(m.as_str()),
            (_, ChartType::LineMarkers) => None,
            (_, ChartType::Scatter) => None,
            _ => Some("none"),
        };
        marker(w, x, o, sym, s.fmt.marker_size, s.fmt.marker_fill.as_ref(), s.fmt.marker_line.as_ref(), s.fmt.marker_effects.as_ref());
        for (index, point) in s.fmt.point_markers.iter().take(1000) {
            w.open0("c:dPt");
            w.val("c:idx", index);
            marker(w, x, o, point.symbol.as_deref(), point.size, point.fill.as_ref(), point.line.as_ref(), point.effects.as_ref());
            w.close("c:dPt");
        }
    }
    if g == Group::Pie || g == Group::Doughnut || g == Group::Bar {
        for (idx, f) in s.point_fills.iter().take(1000) {
            w.open0("c:dPt");
            w.val("c:idx", idx);
            if g == Group::Bar {
                w.val("c:invertIfNegative", 0);
            }
            if g != Group::Bar {
                w.val("c:bubble3D", 0);
            }
            sp_pr(w, Some(f), None);
            w.close("c:dPt");
        }
    }
    if let Some(labels) = &s.fmt.labels {
        labels::write(w, x, o, labels);
    } else if let Some(d) = s.data_labels {
        data_labels(w, d);
    }
    let n = c.categories.len().max(s.values.len());
    let last = n + 1;
    if matches!(g, Group::Scatter | Group::Bubble) {
        let xs: Vec<Option<f64>> = if s.x_values.is_empty() { (1..=s.values.len()).map(|v| Some(v as f64)).collect() } else { s.x_values.clone() };
        w.open0("c:xVal");
        num_cache(w, &format!("Sheet1!$A$2:$A${last}"), &xs);
        w.close("c:xVal");
        w.open0("c:yVal");
        num_cache(w, &format!("Sheet1!${0}$2:${0}${last}", col(i + 1)), &s.values);
        w.close("c:yVal");
        if g == Group::Bubble {
            let sizes: Vec<Option<f64>> = if s.sizes.is_empty() { s.values.iter().map(|_| Some(1.0)).collect() } else { s.sizes.clone() };
            w.open0("c:bubbleSize");
            num_cache(w, &format!("Sheet1!${0}$2:${0}${last}", col(i + 2)), &sizes);
            w.close("c:bubbleSize");
            w.val("c:bubble3D", 0);
        } else {
            w.val("c:smooth", if s.smooth { 1 } else { 0 });
        }
    } else {
        w.open0("c:cat");
        str_cache(w, &format!("Sheet1!$A$2:$A${last}"), &c.categories);
        w.close("c:cat");
        w.open0("c:val");
        num_cache(w, &format!("Sheet1!${0}$2:${0}${last}", col(i + 1)), &s.values);
        w.close("c:val");
        if g == Group::Line {
            w.val("c:smooth", if s.smooth { 1 } else { 0 });
        }
    }
    w.close("c:ser");
}

fn data_labels(w: &mut W, show: bool) {
    w.open0("c:dLbls");
    for (n, v) in [
        ("c:showLegendKey", false),
        ("c:showVal", show),
        ("c:showCatName", false),
        ("c:showSerName", false),
        ("c:showPercent", false),
        ("c:showBubbleSize", false),
    ] {
        w.val(n, if v { 1 } else { 0 });
    }
    w.close("c:dLbls");
}

const AX_CAT: u32 = 50_001;
const AX_VAL: u32 = 50_002;

fn axes(w: &mut W, c: &Chart, g: Group) {
    let horizontal = c.kind.horizontal() && g == Group::Bar;
    let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
    let main_axes = c.main_axes();
    let cat = main_axes
        .and_then(|(id, _)| c.fmt.axis(id))
        .or_else(|| c.fmt.axes.iter().find(|a| matches!(a.kind, AxisKind::Category | AxisKind::Date)))
        .or_else(|| c.fmt.axes.iter().find(|a| a.kind == AxisKind::Value && matches!(a.pos.as_str(), "b" | "t")));
    let val = main_axes
        .and_then(|(_, id)| c.fmt.axis(id))
        .or_else(|| cat.and_then(|cat| c.fmt.axes.iter().find(|a| a.id == cat.cross_ax && a.kind == AxisKind::Value)))
        .or_else(|| c.fmt.axes.iter().find(|a| a.kind == AxisKind::Value && Some(a.id) != cat.map(|cat| cat.id)));
    if g == Group::Scatter || g == Group::Bubble {
        w.open0("c:valAx");
        w.val("c:axId", AX_CAT);
        w.open0("c:scaling");
        w.val("c:orientation", "minMax");
        w.close("c:scaling");
        w.val("c:delete", 0);
        w.val("c:axPos", "b");
        if let Some(t) = &c.axis_titles.0 {
            title(w, t, cat.map(|a| &a.title_text).unwrap_or(&ChartText::default()));
        }
        w.empty("c:numFmt", A::new().a("formatCode", "General").a("sourceLinked", 1));
        w.val("c:majorTickMark", "out");
        w.val("c:minorTickMark", "none");
        w.val("c:tickLblPos", "nextTo");
        if let Some(axis) = cat {
            labels::text(w, &axis.text);
        }
        w.val("c:crossAx", AX_VAL);
        w.val("c:crosses", "autoZero");
        w.val("c:crossBetween", "midCat");
        w.close("c:valAx");
    } else {
        w.open0("c:catAx");
        w.val("c:axId", AX_CAT);
        w.open0("c:scaling");
        w.val("c:orientation", "minMax");
        w.close("c:scaling");
        w.val("c:delete", 0);
        w.val("c:axPos", cat_pos);
        if let Some(t) = &c.axis_titles.0 {
            title(w, t, cat.map(|a| &a.title_text).unwrap_or(&ChartText::default()));
        }
        w.empty("c:numFmt", A::new().a("formatCode", "General").a("sourceLinked", 1));
        w.val("c:majorTickMark", "none");
        w.val("c:minorTickMark", "none");
        w.val("c:tickLblPos", "nextTo");
        if let Some(axis) = cat {
            labels::text(w, &axis.text);
        }
        w.val("c:crossAx", AX_VAL);
        w.val("c:crosses", "autoZero");
        w.val("c:auto", 1);
        w.val("c:lblAlgn", "ctr");
        let offset = cat
            .and_then(|axis| axis.label_offset)
            .filter(|value| value.is_finite() && (0.0..=1000.0).contains(value) && value.fract() == 0.0)
            .unwrap_or(100.0);
        w.val("c:lblOffset", fmt_num(offset));
        w.val("c:noMultiLvlLbl", 0);
        w.close("c:catAx");
    }
    w.open0("c:valAx");
    w.val("c:axId", AX_VAL);
    w.open0("c:scaling");
    w.val("c:orientation", "minMax");
    if let Some(m) = c.y_max.filter(|v| v.is_finite()) {
        w.val("c:max", fmt_num(m));
    }
    if let Some(m) = c.y_min.filter(|v| v.is_finite()) {
        w.val("c:min", fmt_num(m));
    }
    w.close("c:scaling");
    w.val("c:delete", 0);
    w.val("c:axPos", val_pos);
    if c.gridlines {
        w.empty0("c:majorGridlines");
    }
    if let Some(t) = &c.axis_titles.1 {
        title(w, t, val.map(|a| &a.title_text).unwrap_or(&ChartText::default()));
    }
    w.empty("c:numFmt", A::new().a("formatCode", if c.kind.percent() { "0%" } else { "General" }).a("sourceLinked", 1));
    w.val("c:majorTickMark", "none");
    w.val("c:minorTickMark", "none");
    w.val("c:tickLblPos", "nextTo");
    if let Some(axis) = val {
        labels::text(w, &axis.text);
    }
    w.val("c:crossAx", AX_CAT);
    w.val("c:crosses", "autoZero");
    w.val("c:crossBetween", if g == Group::Area || g == Group::Scatter { "midCat" } else { "between" });
    w.close("c:valAx");
}

fn plot_group(w: &mut W, x: &mut Exp, o: &mut Out, c: &Chart, kind: ChartType, list: &[(usize, &Series)]) {
    let g = group_of(kind);
    let grouping = if kind.percent() {
        "percentStacked"
    } else if kind.stacked() {
        "stacked"
    } else if matches!(g, Group::Bar) {
        "clustered"
    } else {
        "standard"
    };
    let vary = if c.vary_colors { 1 } else { 0 };
    let tag = match g {
        Group::Bar => "c:barChart",
        Group::Line => "c:lineChart",
        Group::Area => "c:areaChart",
        Group::Pie => "c:pieChart",
        Group::Doughnut => "c:doughnutChart",
        Group::Scatter => "c:scatterChart",
        Group::Radar => "c:radarChart",
        Group::Bubble => "c:bubbleChart",
    };
    w.open0(tag);
    match g {
        Group::Bar => {
            w.val("c:barDir", if kind.horizontal() { "bar" } else { "col" });
            w.val("c:grouping", grouping);
        }
        Group::Line | Group::Area => w.val("c:grouping", grouping),
        Group::Scatter => w.val("c:scatterStyle", "lineMarker"),
        Group::Radar => w.val("c:radarStyle", if kind == ChartType::FilledRadar { "filled" } else { "marker" }),
        _ => {}
    }
    w.val("c:varyColors", vary);
    for (i, s) in list {
        series(w, x, o, c, s, *i, kind);
    }
    if let Some(labels) = &c.fmt.labels {
        labels::write(w, x, o, labels);
    } else if c.data_labels {
        data_labels(w, true);
    }
    match g {
        Group::Bar => {
            w.val("c:gapWidth", (c.gap_width.clamp(0.0, 5.0) * 100.0).round() as i64);
            let ov = if kind.stacked() { 100 } else { (c.overlap.clamp(-1.0, 1.0) * 100.0).round() as i64 };
            if ov != 0 {
                w.val("c:overlap", ov);
            }
        }
        Group::Line => w.val("c:marker", 1),
        Group::Pie => w.val("c:firstSliceAng", 0),
        Group::Doughnut => {
            w.val("c:firstSliceAng", 0);
            w.val("c:holeSize", (c.hole_size.clamp(0.1, 0.9) * 100.0).round() as i64);
        }
        _ => {}
    }
    if !matches!(g, Group::Pie | Group::Doughnut) {
        w.val("c:axId", AX_CAT);
        w.val("c:axId", AX_VAL);
    }
    w.close(tag);
}

/// The chart part XML.
///
/// Kept references and newly written picture markers are bound to the chart part.
pub fn chart_xml(c: &Chart, x: &mut Exp, o: &mut Out) -> Vec<u8> {
    // Reuse the kept part when the chart is unchanged since it was read.
    if let Some(raw) = &c.raw
        && let Ok(doc) = crate::xml::parse(raw.as_bytes())
        && doc.root.is("chartSpace")
    {
        let mut root = doc.root.clone();
        crate::read::resolve_mc(&mut root, 0);
        if let Some(mut back) = crate::read::chart_from_el(&root) {
            crate::read::kept_marker_pictures(&root, &mut back, &c.rels);
            let mut me = c.clone();
            me.raw = None;
            me.rels = vec![];
            // Read from the chart's theme override part, not from the chart part.
            me.fmt.scheme = None;
            // Picture fills are read with the part's relationships, not from its XML alone.
            let picture = |f: &Option<kition_slide_model::style::Fill>| matches!(f, Some(kition_slide_model::style::Fill::Picture(_)));
            for f in [&mut me.chart_fill, &mut me.plot_fill] {
                if picture(f) {
                    *f = None;
                }
            }
            for s in &mut me.series {
                if picture(&s.fill) {
                    s.fill = None;
                }
                s.point_fills.retain(|(_, f)| !matches!(f, kition_slide_model::style::Fill::Picture(_)));
            }
            if back == me {
                let mut keep = doc.root;
                keep.remove_all(&["userShapes"]);
                if !super::shapes::relink(x, o, &mut keep, &c.rels) {
                    keep.remove_all(&["externalData"]);
                }
                let mut w = W::new();
                w.raw(&keep.to_xml());
                return w.finish();
            }
            // Edited since it was read: the kept part patched to the chart as it is now, so what
            // the model does not hold survives, when the patch reads back as the chart.
            if let Some(patched) = kition_slide_model::chart_spec::patched_part(raw, &back, &me)
                && let Ok(pdoc) = crate::xml::parse(patched.as_bytes())
                && pdoc.root.is("chartSpace")
            {
                let mut proot = pdoc.root.clone();
                crate::read::resolve_mc(&mut proot, 0);
                if let Some(mut again) = crate::read::chart_from_el(&proot) {
                    crate::read::kept_marker_pictures(&proot, &mut again, &c.rels);
                    if again == me {
                        let mut keep = pdoc.root;
                        keep.remove_all(&["userShapes"]);
                        if !super::shapes::relink(x, o, &mut keep, &c.rels) {
                            keep.remove_all(&["externalData"]);
                        }
                        let mut w = W::new();
                        w.raw(&keep.to_xml());
                        return w.finish();
                    }
                }
            }
        }
    }
    let mut w = W::new();
    w.open("c:chartSpace", A::new().a("xmlns:c", NS_C).a("xmlns:a", NS_A).a("xmlns:r", NS_R));
    w.val("c:date1904", 0);
    w.val("c:roundedCorners", 0);
    w.val("c:style", c.style.clamp(1, 48));
    w.open0("c:chart");
    match &c.title {
        Some(t) => {
            title(&mut w, t, &c.fmt.title_text);
            w.val("c:autoTitleDeleted", 0);
        }
        None => w.val("c:autoTitleDeleted", 1),
    }
    w.open0("c:plotArea");
    w.empty0("c:layout");
    let indexed: Vec<(usize, &Series)> = c.series.iter().enumerate().collect();
    let main = match c.kind {
        ChartType::Combo => c.series.first().and_then(|s| s.kind).unwrap_or(ChartType::Column),
        k => k,
    };
    if c.kind == ChartType::Combo {
        // One plot group per series type, sharing the axes.
        let mut kinds: Vec<ChartType> = vec![];
        for s in &c.series {
            let k = s.kind.unwrap_or(main);
            let k = if matches!(group_of(k), Group::Pie | Group::Doughnut | Group::Scatter | Group::Bubble) { ChartType::Column } else { k };
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        for k in kinds {
            let list: Vec<(usize, &Series)> = indexed
                .iter()
                .filter(|(_, s)| {
                    let sk = s.kind.unwrap_or(main);
                    let sk =
                        if matches!(group_of(sk), Group::Pie | Group::Doughnut | Group::Scatter | Group::Bubble) { ChartType::Column } else { sk };
                    sk == k
                })
                .copied()
                .collect();
            plot_group(&mut w, x, o, c, k, &list);
        }
        axes(&mut w, c, Group::Bar);
    } else {
        if !matches!(
            c.kind,
            ChartType::Column
                | ChartType::StackedColumn
                | ChartType::PercentColumn
                | ChartType::Bar
                | ChartType::StackedBar
                | ChartType::PercentBar
                | ChartType::Line
                | ChartType::LineMarkers
                | ChartType::StackedLine
                | ChartType::Area
                | ChartType::StackedArea
                | ChartType::Pie
                | ChartType::Doughnut
                | ChartType::Scatter
                | ChartType::Bubble
                | ChartType::Radar
                | ChartType::FilledRadar
        ) {
            log::warn!("pptx: {} chart written as a column chart", c.kind.label());
        }
        plot_group(&mut w, x, o, c, main, &indexed);
        let g = group_of(main);
        if !matches!(g, Group::Pie | Group::Doughnut) {
            axes(&mut w, c, g);
        }
    }
    if let Some(f) = &c.plot_fill {
        sp_pr(&mut w, Some(f), None);
    }
    w.close("c:plotArea");
    if let Some(pos) = &c.legend {
        let p = match pos.as_str() {
            "b" | "t" | "l" | "r" | "tr" => pos.as_str(),
            _ => "r",
        };
        w.open0("c:legend");
        w.val("c:legendPos", p);
        w.val("c:overlay", 0);
        labels::text(&mut w, &c.fmt.legend_text);
        w.close("c:legend");
    }
    w.val("c:plotVisOnly", 1);
    w.val("c:dispBlanksAs", "gap");
    w.close("c:chart");
    if let Some(f) = &c.chart_fill {
        sp_pr(&mut w, Some(f), None);
    }
    labels::text(&mut w, &c.fmt.text);
    w.close("c:chartSpace");
    w.finish()
}
