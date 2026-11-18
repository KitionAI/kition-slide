//! Chart parts (`c:chartSpace`) → [`Chart`]. The original XML is kept in `Chart.raw`.

use kition_slide_model::chart::{Axis, AxisKind, Chart, ChartFormat, ChartText, ChartType, DataLabels, DataTable, ManualLayout, PointMarker, Series};
use kition_slide_model::style::{Dash, Fill, LineCap, LineJoin, PatternFill};

use crate::xml::El;

/// Read fills that need no relationships; image fills are resolved in `picture_fills`.
fn simple_fill(sp: Option<&El>) -> Option<Fill> {
    let sp = sp?;
    for e in sp.elements() {
        match e.local() {
            "noFill" => return Some(Fill::None),
            "solidFill" => return super::dml::color(e).map(|color| Fill::Solid { color }),
            "gradFill" => return Some(Fill::Gradient(super::dml::gradient(e))),
            "pattFill" => {
                let c = |n: &str| e.child(n).and_then(super::dml::color);
                return Some(Fill::Pattern(PatternFill {
                    preset: e.attr("prst").unwrap_or("pct5").to_string(),
                    fg: c("fgClr").unwrap_or_else(|| kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::BLACK)),
                    bg: c("bgClr").unwrap_or_else(|| kition_slide_model::ColorRef::rgb(kition_slide_color::Rgba::WHITE)),
                }));
            }
            _ => {}
        }
    }
    None
}

fn simple_line(sp: Option<&El>) -> Option<kition_slide_model::Line> {
    let ln = sp?.child("ln")?;
    let mut l = kition_slide_model::Line { width: super::dml::pt(ln, "w"), ..Default::default() };
    l.fill = simple_fill(Some(ln));
    l.compound = ln.attr("cmpd").map(kition_slide_model::style::Compound::from_xml);
    l.cap = ln.attr("cap").map(|c| match c {
        "rnd" => LineCap::Round,
        "sq" => LineCap::Square,
        _ => LineCap::Flat,
    });
    for e in ln.elements() {
        match e.local() {
            "prstDash" => l.dash = Some(Dash::Preset { name: e.attr("val").unwrap_or("solid").to_string() }),
            "round" => l.join = Some(LineJoin::Round),
            "bevel" => l.join = Some(LineJoin::Bevel),
            "miter" => l.join = Some(LineJoin::Miter),
            "headEnd" | "tailEnd" => {
                let end = kition_slide_model::style::LineEnd {
                    kind: e.attr("type").unwrap_or("none").to_string(),
                    w: e.attr("w").unwrap_or("med").to_string(),
                    len: e.attr("len").unwrap_or("med").to_string(),
                };
                if e.is("headEnd") { l.head = Some(end) } else { l.tail = Some(end) }
            }
            _ => {}
        }
    }
    Some(l)
}

fn err_bars(e: &El) -> kition_slide_model::chart::ErrorBars {
    kition_slide_model::chart::ErrorBars {
        dir: e.child_val("errDir").map(String::from),
        kind: e.child_val("errBarType").unwrap_or("both").to_string(),
        val_type: e.child_val("errValType").unwrap_or("fixedVal").to_string(),
        val: e.child("val").and_then(|v| v.f64("val")).filter(|v| v.is_finite()).unwrap_or(0.0),
        no_end_cap: flag(e, "noEndCap").unwrap_or(false),
        line: simple_line(e.child("spPr")),
        plus: e.child("plus").map(numbers).unwrap_or_default(),
        minus: e.child("minus").map(numbers).unwrap_or_default(),
    }
}

fn trendline(t: &El) -> kition_slide_model::chart::Trendline {
    let num = |n: &str| t.child(n).and_then(|v| v.f64("val")).filter(|v| v.is_finite());
    kition_slide_model::chart::Trendline {
        kind: t.child_val("trendlineType").unwrap_or("linear").to_string(),
        order: t.child("order").and_then(|v| v.u32("val")),
        period: t.child("period").and_then(|v| v.u32("val")),
        forward: num("forward").unwrap_or(0.0),
        backward: num("backward").unwrap_or(0.0),
        intercept: num("intercept"),
        line: simple_line(t.child("spPr")),
        name: t.child("name").map(|n| n.text()).filter(|n| !n.is_empty()),
    }
}

/// Text properties: the body's rotation and the first paragraph's default run properties
/// (`c:txPr`), or a rich text's first run.
/// The outer shadow of an element's shape properties (`a:effectLst/a:outerShdw`); shadows
/// scaled to nothing are dropped.
fn shadow_of(sp: Option<&El>) -> Option<kition_slide_model::style::Shadow> {
    let fx = sp?.child("effectLst")?;
    super::dml::effects(fx).outer_shadow.filter(|s| s.sx.abs() > 1e-6 && s.sy.abs() > 1e-6)
}

fn text_props(t: Option<&El>) -> ChartText {
    let mut out = ChartText::default();
    let Some(t) = t else { return out };
    if let Some(b) = t.child("bodyPr") {
        out.rot = b.i64("rot").filter(|r| *r != -60_000_000).map(|r| r as f64 / 60_000.0);
        out.vertical = b.attr("vert").is_some_and(|v| v != "horz");
    }
    let p = t.child("p");
    let rpr = p.and_then(|p| p.path(&["pPr", "defRPr"]));
    let run = p.and_then(|p| p.children_named("r").next()).and_then(|r| r.child("rPr"));
    for r in [rpr, run].into_iter().flatten() {
        for (tag, font) in [("latin", &mut out.font), ("ea", &mut out.font_ea), ("cs", &mut out.font_cs)] {
            if let Some(face) = r.child(tag).and_then(|f| f.attr("typeface")).filter(|f| !f.is_empty()) {
                *font = Some(face.to_owned());
            }
        }
        if let Some(sz) = r.f64("sz").filter(|v| v.is_finite() && *v > 0.0) {
            out.size = Some((sz / 100.0).clamp(1.0, 400.0));
        }
        if let Some(spacing) = r.f64("spc").filter(|s| s.is_finite()) {
            out.spacing = Some((spacing / 100.0).clamp(-4000.0, 4000.0));
        }
        if let Some(b) = r.bool("b") {
            out.bold = Some(b);
        }
        if let Some(i) = r.bool("i") {
            out.italic = Some(i);
        }
        if let Some(c) = r.child("solidFill").and_then(super::dml::color) {
            out.color = Some(c);
        }
        if let Some(list) = r.child("effectLst") {
            let effects = super::dml::effects(list);
            out.shadow = effects.outer_shadow;
            out.glow = effects.glow;
            out.effects_explicit = true;
        }
    }
    out
}

fn manual_layout(l: Option<&El>) -> Option<ManualLayout> {
    let m = l?.child("manualLayout")?;
    let f = |n: &str| m.child_val(n).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(-10.0, 10.0));
    let out = ManualLayout {
        inner: m.child_val("layoutTarget") == Some("inner"),
        x: f("x"),
        y: f("y"),
        w: f("w"),
        h: f("h"),
        x_edge: m.child_val("xMode") == Some("edge"),
        y_edge: m.child_val("yMode") == Some("edge"),
    };
    (out.x.is_some() || out.y.is_some() || out.w.is_some() || out.h.is_some()).then_some(out)
}

fn flag(e: &El, n: &str) -> Option<bool> {
    e.child(n).map(|v| v.attr("val").is_none_or(|v| v == "1" || v == "true"))
}

/// Standard label layout positions the box; Office's label extension additionally sizes it.
/// A point-level extension ignores x/y (MS-ODRAWXML 2.6.1.17).
fn label_layout(d: &El, group: bool) -> Option<ManualLayout> {
    let mut layout = manual_layout(d.child("layout")).and_then(|mut l| {
        // Standard data-label w/h are ignored; c15:layout provides their supported sizes.
        l.w = None;
        l.h = None;
        (l.x.is_some() || l.y.is_some()).then_some(l)
    });
    for ext in d
        .child("extLst")
        .into_iter()
        .flat_map(|e| e.children_named("ext"))
        .filter(|e| e.attr("uri").is_some_and(|uri| uri.eq_ignore_ascii_case("{CE6537A1-D6FC-4f65-9D91-7224C49458BB}")))
    {
        let Some(extra) = manual_layout(ext.child("layout")) else { continue };
        if extra.w.is_none() && extra.h.is_none() && (!group || (extra.x.is_none() && extra.y.is_none())) {
            continue;
        }
        let out = layout.get_or_insert_with(ManualLayout::default);
        out.w = extra.w.or(out.w);
        out.h = extra.h.or(out.h);
        if group {
            if extra.x.is_some() {
                out.x = extra.x;
                out.x_edge = false;
            }
            if extra.y.is_some() {
                out.y = extra.y;
                out.y_edge = false;
            }
        }
    }
    layout
}

fn data_labels(d: &El) -> DataLabels {
    data_labels_with(d, true)
}

fn data_labels_own(d: &El) -> DataLabels {
    data_labels_with(d, false)
}

fn data_labels_with(d: &El, points: bool) -> DataLabels {
    let f = |n: &str| flag(d, n).unwrap_or(false);
    let mut out = DataLabels {
        value: f("showVal"),
        category: f("showCatName"),
        series: f("showSerName"),
        percent: f("showPercent"),
        bubble_size: f("showBubbleSize"),
        legend_key: f("showLegendKey"),
        pos: d.child_val("dLblPos").map(String::from),
        layout: label_layout(d, points),
        num_fmt: d.child("numFmt").filter(|n| n.bool("sourceLinked") != Some(true)).and_then(|n| n.attr("formatCode")).map(String::from),
        separator: d.child("separator").map(|s| s.text()),
        text: text_props(d.child("txPr")),
        fill: simple_fill(d.child("spPr")),
        line: simple_line(d.child("spPr")),
        deleted: vec![],
        points: vec![],
    };
    if !points {
        return out;
    }
    if flag(d, "delete") == Some(true) {
        out.value = false;
        out.category = false;
        out.series = false;
        out.percent = false;
        out.bubble_size = false;
    }
    for l in d.children_named("dLbl").take(10_000) {
        let Some(i) = l.child("idx").and_then(|i| i.u32("val")) else { continue };
        if flag(l, "delete") == Some(true) {
            out.deleted.push(i as usize);
        } else if l.child("showVal").is_some()
            || l.child("separator").is_some()
            || l.child("spPr").is_some()
            || l.child("txPr").is_some()
            || label_layout(l, false).is_some()
        {
            let mut p = data_labels_own(l);
            // What the point's label leaves out comes from the series' labels.
            // Keep the existing non-effect style fallback independent of new effects.
            let shadow = p.text.shadow.take();
            let glow = p.text.glow.take();
            let effects_explicit = std::mem::take(&mut p.text.effects_explicit);
            let spacing = p.text.spacing.take();
            if p.text == ChartText::default() {
                p.text = out.text.clone();
            }
            if effects_explicit || shadow.is_some() || glow.is_some() {
                p.text.shadow = shadow;
                p.text.glow = glow;
                p.text.effects_explicit = effects_explicit;
            }
            p.text.spacing = spacing.or(out.text.spacing);
            p.text.inherit_effects(&out.text);
            if p.pos.is_none() {
                p.pos = out.pos.clone();
            }
            if p.separator.is_none() {
                p.separator = out.separator.clone();
            }
            if p.fill.is_none() {
                p.fill = out.fill.clone();
            }
            if p.line.is_none() {
                p.line = out.line.clone();
            }
            // An explicit source-linked format clears the custom parent format.
            if l.child("numFmt").is_none() {
                p.num_fmt = out.num_fmt.clone();
            }
            for (name, field, inherited) in [
                ("showVal", &mut p.value, out.value),
                ("showCatName", &mut p.category, out.category),
                ("showSerName", &mut p.series, out.series),
                ("showPercent", &mut p.percent, out.percent),
                ("showBubbleSize", &mut p.bubble_size, out.bubble_size),
                ("showLegendKey", &mut p.legend_key, out.legend_key),
            ] {
                if l.child(name).is_none() {
                    *field = inherited;
                }
            }
            out.points.push((i as usize, p));
        }
    }
    out
}

fn axis(a: &El) -> Axis {
    let num = |e: Option<&El>, n: &str| e.and_then(|e| e.child_val(n)).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let sc = a.child("scaling");
    Axis {
        id: a.child("axId").and_then(|i| i.u32("val")).unwrap_or(0),
        kind: match a.local() {
            "valAx" => AxisKind::Value,
            "dateAx" => AxisKind::Date,
            "serAx" => AxisKind::Series,
            _ => AxisKind::Category,
        },
        pos: a.child_val("axPos").unwrap_or("b").to_string(),
        deleted: flag(a, "delete").unwrap_or(false),
        reversed: sc.and_then(|s| s.child_val("orientation")) == Some("maxMin"),
        min: num(sc, "min"),
        max: num(sc, "max"),
        major_unit: num(Some(a), "majorUnit").filter(|v| *v > 0.0),
        minor_unit: num(Some(a), "minorUnit").filter(|v| *v > 0.0),
        log_base: num(sc, "logBase").filter(|v| *v >= 2.0),
        num_fmt: a.child("numFmt").and_then(|n| n.attr("formatCode")).map(String::from),
        major_tick: a.child_val("majorTickMark").unwrap_or("cross").to_string(),
        minor_tick: a.child_val("minorTickMark").unwrap_or("cross").to_string(),
        label_pos: a.child_val("tickLblPos").unwrap_or("nextTo").to_string(),
        line: simple_line(a.child("spPr")),
        major_grid: a.child("majorGridlines").map(|g| simple_line(g.child("spPr")).unwrap_or_default()),
        minor_grid: a.child("minorGridlines").map(|g| simple_line(g.child("spPr")).unwrap_or_default()),
        text: text_props(a.child("txPr")),
        title: a.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into())),
        title_text: a.child("title").map(|t| text_props(t.path(&["tx", "rich"]).or_else(|| t.child("txPr")))).unwrap_or_default(),
        cross_ax: a.child("crossAx").and_then(|i| i.u32("val")).unwrap_or(0),
        crosses: a.child_val("crosses").map(String::from),
        crosses_at: num(Some(a), "crossesAt"),
        between: a.child_val("crossBetween").map(|v| v == "between"),
        label_skip: a.child("tickLblSkip").and_then(|v| v.u32("val")).filter(|v| *v > 0),
        label_offset: num(Some(a), "lblOffset"),
    }
}

/// Formatting of the chart (layout, axes, text, borders): see [`ChartFormat`].
fn chart_format(root: &El, ch: &El, pa: &El) -> ChartFormat {
    let mut f = ChartFormat {
        rounded: flag(root, "roundedCorners").unwrap_or(false),
        chart_line: simple_line(root.child("spPr")),
        plot_line: simple_line(pa.child("spPr")),
        chart_shadow: shadow_of(root.child("spPr")),
        plot_shadow: shadow_of(pa.child("spPr")),
        text: text_props(root.child("txPr")),
        plot_layout: manual_layout(pa.child("layout")),
        blanks_as_zero: ch.child_val("dispBlanksAs") == Some("zero"),
        ..Default::default()
    };
    if let Some(t) = ch.child("title") {
        f.title_text = text_props(t.path(&["tx", "rich"]).or_else(|| t.child("txPr")));
        f.title_layout = manual_layout(t.child("layout"));
        f.title_overlay = flag(t, "overlay").unwrap_or(false);
        f.title_fill = simple_fill(t.child("spPr"));
        f.title_line = simple_line(t.child("spPr"));
    }
    if let Some(l) = ch.child("legend") {
        f.legend_text = text_props(l.child("txPr"));
        f.legend_layout = manual_layout(l.child("layout"));
        f.legend_overlay = flag(l, "overlay").unwrap_or(false);
        f.legend_fill = simple_fill(l.child("spPr"));
        f.legend_line = simple_line(l.child("spPr"));
        for e in l.children_named("legendEntry") {
            if flag(e, "delete") == Some(true)
                && let Some(i) = e.child("idx").and_then(|i| i.u32("val"))
            {
                f.legend_deleted.push(i as usize);
            }
        }
    }
    for a in pa.elements().filter(|a| matches!(a.local(), "valAx" | "catAx" | "dateAx" | "serAx")).take(16) {
        f.axes.push(axis(a));
    }
    if let Some(v) = ch.child("view3D") {
        let num = |n: &str, d: f64| v.child(n).and_then(|x| x.f64("val")).filter(|x| x.is_finite()).unwrap_or(d);
        f.view3d = Some((num("rotX", 15.0).clamp(-90.0, 90.0), num("rotY", 20.0).rem_euclid(360.0), num("depthPercent", 100.0).clamp(20.0, 2000.0)));
    }
    for g in pa.elements().filter(|g| g.local().ends_with("Chart")) {
        if g.local().contains("3D") {
            f.three_d = true;
        }
        if f.labels.is_none()
            && let Some(d) = g.child("dLbls")
        {
            f.labels = Some(data_labels(d));
        }
        if let Some(l) = g.child("dropLines") {
            f.drop_lines = Some(simple_line(l.child("spPr")).unwrap_or_default());
        }
        if let Some(l) = g.child("hiLowLines") {
            f.hi_low_lines = Some(simple_line(l.child("spPr")).unwrap_or_default());
        }
        if let Some(l) = g.child("serLines") {
            f.ser_lines = Some(simple_line(l.child("spPr")).unwrap_or_default());
        }
        if let Some(u) = g.child("upDownBars") {
            let gap = u.child("gapWidth").and_then(|v| v.f64("val")).unwrap_or(150.0) / 100.0;
            f.up_down_bars = Some((
                gap.clamp(0.0, 5.0),
                u.child("upBars").and_then(|b| simple_fill(b.child("spPr"))),
                u.child("downBars").and_then(|b| simple_fill(b.child("spPr"))),
            ));
            f.up_down_lines =
                (u.child("upBars").and_then(|b| simple_line(b.child("spPr"))), u.child("downBars").and_then(|b| simple_line(b.child("spPr"))));
        }
        if let Some(a) = g.child("firstSliceAng").and_then(|v| v.f64("val")) {
            f.first_slice_angle = a.clamp(0.0, 360.0);
        }
        if let Some(s) = g.child_val("scatterStyle") {
            f.scatter_style = Some(s.to_string());
        }
        if let Some(s) = g.child_val("radarStyle") {
            f.radar_style = Some(s.to_string());
        }
        if g.is("ofPieChart") {
            f.of_pie = Some(g.child_val("ofPieType").unwrap_or("pie").to_string());
            f.second_pie_size = g.child("secondPieSize").and_then(|v| v.f64("val")).map(|v| v.clamp(5.0, 200.0));
            f.split_pos = g.child("splitPos").and_then(|v| v.f64("val"));
            f.split_type = g.child_val("splitType").map(String::from);
        }
        if let Some(b) = g.child("bubbleScale").and_then(|v| v.f64("val")) {
            f.bubble_scale = Some(b.clamp(0.0, 300.0));
        }
    }
    if let Some(t) = pa.child("dTable") {
        f.data_table = Some(DataTable {
            horizontal: flag(t, "showHorzBorder").unwrap_or(false),
            vertical: flag(t, "showVertBorder").unwrap_or(false),
            outline: flag(t, "showOutline").unwrap_or(false),
            keys: flag(t, "showKeys").unwrap_or(false),
            line: simple_line(t.child("spPr")),
            text: text_props(t.child("txPr")),
        });
    }
    f
}

fn rich_text(t: &El) -> Option<String> {
    let tx = t.child("tx")?;
    if let Some(r) = tx.child("rich") {
        let s = r.children_named("p").map(|p| p.text()).collect::<Vec<_>>().join("\n");
        return Some(s);
    }
    tx.find("v").map(|v| v.text())
}

/// String values from `c:cat`/`c:tx` (str or num refs, caches or literals).
fn strings(e: &El) -> Vec<String> {
    let cache = e.find("strCache").or_else(|| e.find("numCache")).or_else(|| e.find("strLit")).or_else(|| e.find("numLit"));
    if let Some(c) = cache {
        let n = c.child("ptCount").and_then(|p| p.u32("val")).unwrap_or(0).min(100_000) as usize;
        let mut out = vec![String::new(); n];
        for pt in c.children_named("pt").take(100_000) {
            let i = pt.u32("idx").unwrap_or(0) as usize;
            let v = pt.child("v").map(|v| v.text()).unwrap_or_default();
            if i >= out.len() {
                if i >= 100_000 {
                    continue;
                }
                out.resize(i + 1, String::new());
            }
            if let Some(s) = out.get_mut(i) {
                *s = v;
            }
        }
        return out;
    }
    if let Some(ml) = e.find("multiLvlStrCache") {
        // Use the innermost level.
        if let Some(l) = ml.child("lvl") {
            return l.children_named("pt").map(|p| p.child("v").map(|v| v.text()).unwrap_or_default()).collect();
        }
    }
    if let Some(v) = e.child("v") {
        return vec![v.text()];
    }
    vec![]
}

fn numbers(e: &El) -> Vec<Option<f64>> {
    strings(e).into_iter().map(|s| s.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// Restore marker pictures when comparing a kept chart against its current model.
pub(crate) fn kept_marker_pictures(root: &El, chart: &mut Chart, rels: &[kition_slide_model::KeptRel]) {
    let picture = |marker: Option<&El>| {
        let bf = marker?.path(&["spPr", "blipFill"])?;
        let blip = bf.child("blip")?;
        let id = blip.attr("r:embed").or_else(|| blip.attr("r:link"))?;
        let media = rels.iter().find(|r| r.id == id && r.rel_type.ends_with("/image"))?.media;
        Some(Fill::Picture(super::dml::blip_fill_with_media(bf, media)))
    };
    let Some(pa) = root.path(&["chart", "plotArea"]) else { return };
    let series = pa.elements().filter(|g| g.local().ends_with("Chart")).flat_map(|g| g.children_named("ser").take(1000));
    for (element, series) in series.zip(chart.series.iter_mut()) {
        if let Some(fill) = picture(element.child("marker")) {
            series.fmt.marker_fill = Some(fill);
        }
        for point in element.children_named("dPt") {
            let Some(index) = point.child("idx").and_then(|index| index.u32("val")) else {
                continue;
            };
            if let Some(fill) = picture(point.child("marker"))
                && let Some((_, marker)) = series.fmt.point_markers.iter_mut().find(|(i, _)| *i == index as usize)
            {
                marker.fill = Some(fill);
            }
        }
    }
}

/// Resolve chart, series, point, and marker picture fills using the chart relationships.
pub(crate) fn picture_fills(imp: &mut super::Imp, name: &str, bytes: &[u8], chart: &mut Chart) {
    let Ok(doc) = crate::xml::parse(bytes) else { return };
    let part = super::Part::new(imp.pkg, name, &doc);
    let mut root = doc.root.clone();
    super::resolve_mc(&mut root, 0);
    let blip = |imp: &mut super::Imp, sp: Option<&El>| -> Option<Fill> {
        let bf = sp?.child("blipFill")?;
        super::dml::blip_fill(imp, &part, bf).map(Fill::Picture)
    };
    if let Some(f) = blip(imp, root.child("spPr")) {
        chart.chart_fill = Some(f);
    }
    if let Some(f) = blip(imp, root.path(&["chart", "plotArea", "spPr"])) {
        chart.plot_fill = Some(f);
    }
    // Series and points, in the order `chart_from` reads them.
    let Some(pa) = root.path(&["chart", "plotArea"]) else { return };
    let sers = pa.elements().filter(|g| g.local().ends_with("Chart")).flat_map(|g| g.children_named("ser").take(1000));
    for (ser, s) in sers.zip(chart.series.iter_mut()) {
        if let Some(f) = blip(imp, ser.child("spPr")) {
            s.fill = Some(f);
        }
        if let Some(f) = blip(imp, ser.path(&["marker", "spPr"])) {
            s.fmt.marker_fill = Some(f);
        }
        for dp in ser.children_named("dPt") {
            let Some(i) = dp.child("idx").and_then(|i| i.u32("val")) else { continue };
            if let Some(f) = blip(imp, dp.child("spPr")) {
                s.point_fills.retain(|(k, _)| *k != i as usize);
                s.point_fills.push((i as usize, f));
            }
            if let Some(fill) = blip(imp, dp.path(&["marker", "spPr"]))
                && let Some((_, marker)) = s.fmt.point_markers.iter_mut().find(|(index, _)| *index == i as usize)
            {
                marker.fill = Some(fill);
            }
        }
    }
}

/// The colour scheme of a chart's theme override part (`a:themeOverride/a:clrScheme`).
pub fn theme_override_scheme(bytes: &[u8]) -> Option<kition_slide_color::ColorScheme> {
    use kition_slide_color::{Rgba, SchemeSlot};
    use kition_slide_model::ColorBase;
    let doc = crate::xml::parse(bytes).ok()?;
    let cs = doc.root.child("clrScheme")?;
    let mut scheme = kition_slide_color::ColorScheme { name: cs.attr("name").unwrap_or("").to_string(), colors: [Rgba::BLACK; 12] };
    let mut any = false;
    for slot in SchemeSlot::THEME {
        let c = cs.child(slot.xml_name()).and_then(super::dml::color)?;
        let rgb = match c.base {
            ColorBase::Rgb { rgb } => rgb,
            ColorBase::System { last, .. } => last,
            ColorBase::Preset { name } => kition_slide_color::preset(&name).unwrap_or(Rgba::BLACK),
            ColorBase::Scheme { .. } => return None,
        };
        scheme.set(slot, rgb);
        any = true;
    }
    any.then_some(scheme)
}

/// Parse a chart part (`None` for something that isn't a chart we can read). `external`
/// renumbers the relationships the kept part names (its embedded workbook, `c:externalData`);
/// when it answers `None` for one, the workbook reference is dropped.
pub fn read_chart_part(bytes: &[u8], external: &mut dyn FnMut(&str) -> Option<String>) -> Option<Chart> {
    let doc = crate::xml::parse(bytes).ok()?;
    let mut keep = doc.root.clone();
    let mut root = doc.root;
    super::resolve_mc(&mut root, 0);
    if !root.is("chartSpace") {
        return None;
    }
    let mut c = chart_from(&root)?;
    // Kept as the export writes it back (without the parts it cannot carry), so a saved file
    // reads back with the same text.
    keep.remove_all(&["userShapes"]);
    let mut mapped = keep.clone();
    if super::map_rel_refs(&mut mapped, external) {
        keep = mapped;
    } else {
        keep.remove_all(&["externalData"]);
    }
    let mut w = crate::xml::W::new();
    w.raw(&keep.to_xml());
    c.raw = Some(String::from_utf8_lossy(&w.finish()).into_owned());
    Some(c)
}

pub(crate) fn chart_from(root: &El) -> Option<Chart> {
    let ch = root.child("chart")?;
    let pa = ch.child("plotArea")?;
    let mut c = Chart { categories: vec![], series: vec![], title: None, legend: None, gridlines: false, ..Default::default() };
    c.style = root.child_val("style").and_then(|v| v.parse().ok()).unwrap_or(2);
    if let Some(t) = ch.child("title") {
        c.title = Some(rich_text(t).unwrap_or_else(|| "Chart Title".into()));
    }
    if ch.child("autoTitleDeleted").and_then(|a| a.bool("val")) == Some(true) {
        c.title = None;
    }
    c.legend = ch.child("legend").map(|l| l.child_val("legendPos").unwrap_or("r").to_string());
    c.chart_fill = simple_fill(root.child("spPr"));
    c.plot_fill = simple_fill(pa.child("spPr"));

    let mut kinds: Vec<ChartType> = vec![];
    let mut cat_format = None;
    for g in pa.elements() {
        let name = g.local();
        if !name.ends_with("Chart") {
            continue;
        }
        let grouping = g.child_val("grouping").unwrap_or("clustered");
        let kind = match name {
            "barChart" | "bar3DChart" => {
                let bar = g.child_val("barDir") == Some("bar");
                match (bar, grouping) {
                    (false, "stacked") => ChartType::StackedColumn,
                    (false, "percentStacked") => ChartType::PercentColumn,
                    (false, _) => ChartType::Column,
                    (true, "stacked") => ChartType::StackedBar,
                    (true, "percentStacked") => ChartType::PercentBar,
                    (true, _) => ChartType::Bar,
                }
            }
            "lineChart" | "line3DChart" => {
                let markers = g.child_val("marker").is_some_and(|v| v == "1" || v == "true")
                    && g.children_named("ser").all(|s| s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) != Some("none"));
                match grouping {
                    "stacked" | "percentStacked" => ChartType::StackedLine,
                    _ if markers => ChartType::LineMarkers,
                    _ => ChartType::Line,
                }
            }
            "areaChart" | "area3DChart" => {
                if matches!(grouping, "stacked" | "percentStacked") {
                    ChartType::StackedArea
                } else {
                    ChartType::Area
                }
            }
            "pieChart" | "pie3DChart" | "ofPieChart" => ChartType::Pie,
            "doughnutChart" => ChartType::Doughnut,
            "scatterChart" => ChartType::Scatter,
            "bubbleChart" => ChartType::Bubble,
            "radarChart" => {
                if g.child_val("radarStyle") == Some("filled") {
                    ChartType::FilledRadar
                } else {
                    ChartType::Radar
                }
            }
            "stockChart" => ChartType::Stock,
            "surfaceChart" | "surface3DChart" => ChartType::Surface,
            _ => continue,
        };
        if let Some(v) = g.child_val("varyColors") {
            c.vary_colors = v == "1" || v == "true";
        }
        if let Some(v) = g.child_val("gapWidth").and_then(|v| v.parse::<f64>().ok()) {
            c.gap_width = (v / 100.0).clamp(0.0, 5.0);
        }
        if let Some(v) = g.child_val("overlap").and_then(|v| v.parse::<f64>().ok()) {
            c.overlap = (v / 100.0).clamp(-1.0, 1.0);
        }
        if let Some(v) = g.child_val("holeSize").and_then(|v| v.parse::<f64>().ok()) {
            c.hole_size = (v / 100.0).clamp(0.0, 0.9);
        }
        if g.path(&["dLbls", "showVal"]).and_then(|v| v.bool("val")) == Some(true) {
            c.data_labels = true;
        }
        kinds.push(kind);
        for s in g.children_named("ser").take(1000) {
            let mut ser = Series {
                name: s.child("tx").map(strings).and_then(|v| v.into_iter().next()).unwrap_or_default(),
                fill: simple_fill(s.child("spPr")),
                line: simple_line(s.child("spPr")),
                // The series flag takes precedence over the scatter group's default.
                smooth: s
                    .child("smooth")
                    .and_then(|v| v.bool("val"))
                    .unwrap_or_else(|| kind == ChartType::Scatter && g.child_val("scatterStyle").is_some_and(|v| v.starts_with("smooth"))),
                marker: s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")).map(String::from),
                kind: Some(kind),
                ..Default::default()
            };
            if let Some(d) = s.child("dLbls").and_then(|d| d.child("showVal")).and_then(|v| v.bool("val")) {
                ser.data_labels = Some(d);
            }
            ser.fmt.idx = s.child("idx").and_then(|i| i.u32("val"));
            ser.fmt.axes = g.children_named("axId").filter_map(|a| a.u32("val")).collect();
            ser.fmt.labels = s.child("dLbls").map(data_labels);
            ser.fmt.shadow = shadow_of(s.child("spPr"));
            ser.fmt.err_bars = s.children_named("errBars").take(4).map(err_bars).collect();
            ser.fmt.trendlines = s.children_named("trendline").take(8).map(trendline).collect();
            ser.fmt.invert_if_negative = flag(s, "invertIfNegative").unwrap_or(false);
            ser.fmt.explosion = s.child("explosion").and_then(|v| v.f64("val")).map(|v| (v / 100.0).clamp(0.0, 4.0)).unwrap_or(0.0);
            if let Some(m) = s.child("marker") {
                ser.fmt.marker_size = m.child("size").and_then(|v| v.f64("val")).filter(|v| v.is_finite()).map(|v| v.clamp(2.0, 72.0));
                ser.fmt.marker_fill = simple_fill(m.child("spPr"));
                ser.fmt.marker_line = simple_line(m.child("spPr"));
                ser.fmt.marker_effects = m.path(&["spPr", "effectLst"]).map(super::dml::effects);
            }
            for dp in s.children_named("dPt") {
                let Some(i) = dp.child("idx").and_then(|i| i.u32("val")) else { continue };
                if let Some(f) = simple_fill(dp.child("spPr")) {
                    ser.point_fills.push((i as usize, f));
                }
                if let Some(marker) = dp.child("marker")
                    && ser.fmt.point_markers.len() < 1000
                {
                    ser.fmt.point_markers.push((
                        i as usize,
                        PointMarker {
                            symbol: marker.child_val("symbol").map(String::from),
                            size: marker.child("size").and_then(|v| v.f64("val")).filter(|v| v.is_finite()).map(|v| v.clamp(2.0, 72.0)),
                            fill: simple_fill(marker.child("spPr")),
                            line: simple_line(marker.child("spPr")),
                            effects: marker.path(&["spPr", "effectLst"]).map(super::dml::effects),
                        },
                    ));
                }
                if let Some(l) = simple_line(dp.child("spPr")) {
                    ser.fmt.point_lines.push((i as usize, l));
                }
                if let Some(sh) = shadow_of(dp.child("spPr")) {
                    ser.fmt.point_shadows.push((i as usize, sh));
                }
                if let Some(e) = dp.child("explosion").and_then(|v| v.f64("val")) {
                    ser.fmt.point_explosion.push((i as usize, (e / 100.0).clamp(0.0, 4.0)));
                }
            }
            if let Some(cat) = s.child("cat").or_else(|| s.child("xVal")) {
                let cats = strings(cat);
                if matches!(kind, ChartType::Scatter | ChartType::Bubble) {
                    ser.x_values = cats.iter().map(|v| v.trim().parse::<f64>().ok()).collect();
                }
                if c.categories.is_empty() {
                    c.categories = cats;
                    cat_format =
                        cat.find("numCache").and_then(|n| n.child("formatCode")).map(|f| f.text()).filter(|f| !f.is_empty() && f != "General");
                }
            }
            if let Some(v) = s.child("val").or_else(|| s.child("yVal")) {
                ser.values = numbers(v);
                ser.fmt.num_fmt = v.find("formatCode").map(|f| f.text()).filter(|f| !f.is_empty() && f != "General");
            }
            if let Some(b) = s.child("bubbleSize") {
                ser.sizes = numbers(b);
            }
            c.series.push(ser);
        }
    }
    let first = *kinds.first()?;
    c.kind = if kinds.iter().any(|k| *k != first) { ChartType::Combo } else { first };
    if c.kind != ChartType::Combo {
        for s in &mut c.series {
            s.kind = None;
        }
    }
    // Axes.
    let vertical = |a: &&El| matches!(a.child_val("axPos"), Some("l" | "r"));
    let val_ax = pa.children_named("valAx").find(vertical).or_else(|| pa.children_named("valAx").last());
    let cat_ax = pa.child("catAx").or_else(|| pa.child("dateAx")).or_else(|| pa.children_named("valAx").find(|a| !vertical(a)));
    if let Some(v) = val_ax {
        c.gridlines = v.child("majorGridlines").is_some();
        let sc = v.child("scaling");
        c.y_min = sc.and_then(|s| s.child_val("min")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.y_max = sc.and_then(|s| s.child_val("max")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.axis_titles.1 = v.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    if let Some(a) = cat_ax {
        c.axis_titles.0 = a.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    // A title without text of its own names the only series, as the reference renders show it.
    if let Some(t) = ch.child("title")
        && rich_text(t).is_none()
        && c.title.is_some()
        && let [only] = c.series.as_slice()
        && !only.name.is_empty()
    {
        c.title = Some(only.name.clone());
    }
    c.fmt = chart_format(root, ch, pa);
    c.fmt.cat_format = cat_format;
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str =
        r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;

    fn read(body: &str) -> Chart {
        let xml = format!(r#"<c:chartSpace {NS}><c:chart>{body}</c:chart></c:chartSpace>"#);
        read_chart_part(xml.as_bytes(), &mut |_| None).expect("chart")
    }

    #[test]
    fn chart_tracking_reads_signed_values_zero_overrides_and_label_inheritance() {
        for (value, expected) in [("120", Some(1.2)), ("-75", Some(-0.75)), ("0", Some(0.0)), ("NaN", None), ("inf", None)] {
            let xml = format!(r#"<c:txPr {NS}><a:p><a:pPr><a:defRPr spc="{value}"/></a:pPr></a:p></c:txPr>"#);
            let doc = crate::xml::parse(xml.as_bytes()).unwrap();
            assert_eq!(text_props(Some(&doc.root)).spacing, expected);
        }
        let chart = read(
            r#"<c:title><c:tx><c:rich><a:p><a:pPr><a:defRPr spc="150"/></a:pPr>
            <a:r><a:rPr spc="0"/><a:t>Title</a:t></a:r></a:p></c:rich></c:tx></c:title>
            <c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:dLbls>
            <c:dLbl><c:idx val="0"/><c:txPr><a:p><a:pPr><a:defRPr sz="1800"/></a:pPr></a:p></c:txPr></c:dLbl>
            <c:dLbl><c:idx val="1"/><c:txPr><a:p><a:pPr><a:defRPr spc="0"/></a:pPr></a:p></c:txPr></c:dLbl>
            <c:txPr><a:p><a:pPr><a:defRPr sz="1200" b="1" spc="125"/></a:pPr></a:p></c:txPr>
            <c:showVal val="1"/></c:dLbls></c:ser></c:barChart></c:plotArea>"#,
        );
        assert_eq!(chart.fmt.title_text.spacing, Some(0.0));
        let labels = chart.series[0].fmt.labels.as_ref().unwrap();
        assert_eq!(labels.point(0).unwrap().text.spacing, Some(1.25));
        assert_eq!(labels.point(0).unwrap().text.size, Some(18.0));
        let zero = &labels.point(1).unwrap().text;
        assert_eq!(zero.spacing, Some(0.0));
        assert_eq!(zero.size, Some(12.0));
        assert_eq!(zero.bold, Some(true));
    }

    #[test]
    fn separator_only_point_labels_override_empty_or_custom_and_inherit_other_fields() {
        let c = read(
            r#"<c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:dLbls>
            <c:dLbl><c:idx val="0"/><c:separator>; </c:separator></c:dLbl>
            <c:dLbl><c:idx val="1"/><c:separator/></c:dLbl>
            <c:dLbl><c:idx val="2"/><c:showVal val="0"/></c:dLbl>
            <c:dLbl><c:idx val="3"/></c:dLbl>
            <c:dLbl><c:idx val="4"/><c:delete val="1"/><c:separator>ignored</c:separator></c:dLbl>
            <c:numFmt formatCode="0.00" sourceLinked="0"/>
            <c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1200" b="1"/></a:pPr></a:p></c:txPr>
            <c:dLblPos val="inEnd"/><c:showVal val="1"/><c:showCatName val="1"/>
            <c:separator> / </c:separator></c:dLbls></c:ser></c:barChart></c:plotArea>"#,
        );
        let labels = c.series[0].fmt.labels.as_ref().unwrap();
        assert_eq!(labels.points.len(), 3);
        for (index, separator) in [(0, "; "), (1, "")] {
            let point = labels.point(index).unwrap();
            assert_eq!(point.separator.as_deref(), Some(separator));
            assert!(point.value && point.category);
            assert_eq!(point.num_fmt.as_deref(), Some("0.00"));
            assert_eq!(point.pos.as_deref(), Some("inEnd"));
            assert_eq!(point.text.size, Some(12.0));
            assert_eq!(point.text.bold, Some(true));
        }
        let inherited = labels.point(2).unwrap();
        assert!(!inherited.value && inherited.category);
        assert_eq!(inherited.separator.as_deref(), Some(" / "));
        assert_eq!(labels.point(3).unwrap().separator.as_deref(), Some(" / "));
        assert!(labels.point(4).is_none(), "a deleted label stays deleted even with a separator");
    }

    #[test]
    fn chart_run_effect_lists_replace_defaults_and_point_labels_inherit_only_when_undefined() {
        let c = read(
            r#"<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr>
            <a:effectLst><a:glow rad="25400"><a:srgbClr val="FF0000"/></a:glow></a:effectLst>
            </a:defRPr></a:pPr><a:r><a:rPr><a:effectLst/></a:rPr><a:t>Title</a:t></a:r></a:p></c:rich></c:tx></c:title>
            <c:plotArea><c:barChart><c:dLbls>
            <c:dLbl><c:idx val="0"/><c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1400"/></a:pPr></a:p></c:txPr></c:dLbl>
            <c:dLbl><c:idx val="1"/><c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr><a:effectLst/></a:defRPr></a:pPr></a:p></c:txPr></c:dLbl>
            <c:dLbl><c:idx val="2"/><c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr><a:effectLst>
            <a:glow rad="12700"><a:srgbClr val="00FF00"/></a:glow></a:effectLst></a:defRPr></a:pPr></a:p></c:txPr></c:dLbl>
            <c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1000"><a:effectLst>
            <a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0">
            <a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw>
            </a:effectLst></a:defRPr></a:pPr></a:p></c:txPr><c:showVal val="1"/>
            </c:dLbls></c:barChart></c:plotArea>"#,
        );
        assert!(c.fmt.title_text.effects_explicit && c.fmt.title_text.glow.is_none());
        let labels = c.fmt.labels.as_ref().unwrap();
        let shadow = labels.text.shadow.as_ref().unwrap();
        assert_eq!((shadow.blur, shadow.dist, shadow.dir), (4.0, 3.0, 45.0));
        let partial = &labels.point(0).unwrap().text;
        assert_eq!(partial.size, Some(14.0));
        assert_eq!(partial.shadow, labels.text.shadow);
        let empty = &labels.point(1).unwrap().text;
        assert!(empty.effects_explicit && empty.shadow.is_none() && empty.glow.is_none());
        assert_eq!(empty.size, Some(10.0));
        let glow = &labels.point(2).unwrap().text;
        assert!(glow.effects_explicit && glow.shadow.is_none());
        assert_eq!(glow.size, Some(10.0));
        assert_eq!(glow.glow.as_ref().unwrap().radius, 1.0);
    }

    #[test]
    fn data_label_layout_separates_standard_position_and_extension_size() {
        let c = read(
            r#"<c:plotArea><c:barChart><c:ser><c:idx val="0"/>
            <c:dLbls><c:dLbl><c:idx val="0"/><c:layout><c:manualLayout>
            <c:xMode val="edge"/><c:x val="0.2"/><c:y val="-0.1"/><c:w val="0.9"/>
            </c:manualLayout></c:layout><c:extLst>
            <c:ext uri="unrelated"><f:layout><c:manualLayout><c:w val="0.8"/></c:manualLayout></f:layout></c:ext>
            <c:ext uri="{CE6537A1-D6FC-4f65-9D91-7224C49458BB}"><c15:layout><c:manualLayout>
            <c:x val="0.7"/><c:y val="0.6"/><c:w val="0.12"/><c:h val="0.08"/>
            </c:manualLayout></c15:layout></c:ext></c:extLst></c:dLbl><c:showVal val="1"/></c:dLbls>
            </c:ser></c:barChart></c:plotArea>"#,
        );
        let point = c.series[0].fmt.labels.as_ref().unwrap().point(0).unwrap();
        assert!(point.value, "layout-only override inherits the series' value flag");
        let layout = point.layout.as_ref().unwrap();
        assert_eq!((layout.x, layout.y, layout.w, layout.h), (Some(0.2), Some(-0.1), Some(0.12), Some(0.08)));
        assert!(layout.x_edge);
        assert!(!layout.y_edge);
    }

    #[test]
    fn layout_only_labels_inherit_formats_and_flags_individually() {
        let c = read(
            r#"<c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:dLbls>
            <c:dLbl><c:idx val="0"/><c:layout><c:manualLayout><c:x val="0.1"/></c:manualLayout></c:layout>
            <c:showCatName val="1"/><c:showPercent val="0"/></c:dLbl>
            <c:dLbl><c:idx val="1"/><c:layout><c:manualLayout><c:y val="0.1"/></c:manualLayout></c:layout>
            <c:numFmt formatCode="General" sourceLinked="1"/><c:showVal val="0"/><c:showLegendKey val="0"/></c:dLbl>
            <c:dLbl><c:idx val="2"/><c:layout><c:manualLayout><c:x val="0.2"/></c:manualLayout></c:layout>
            <c:numFmt formatCode="0.000" sourceLinked="0"/></c:dLbl>
            <c:numFmt formatCode="0.00" sourceLinked="0"/><c:showVal val="1"/><c:showCatName val="0"/>
            <c:showSerName val="1"/><c:showPercent val="1"/><c:showBubbleSize val="1"/><c:showLegendKey val="1"/>
            </c:dLbls></c:ser></c:barChart></c:plotArea>"#,
        );
        let labels = c.series[0].fmt.labels.as_ref().unwrap();
        let point = labels.point(0).unwrap();
        assert_eq!(point.num_fmt.as_deref(), Some("0.00"));
        assert!(point.value && point.category && point.series && point.bubble_size && point.legend_key);
        assert!(!point.percent, "explicit false overrides the parent's true flag");
        let linked = labels.point(1).unwrap();
        assert!(linked.num_fmt.is_none(), "sourceLinked clears the parent's custom format");
        assert!(!linked.value && !linked.category && !linked.legend_key);
        assert!(linked.series && linked.percent && linked.bubble_size);
        assert_eq!(labels.point(2).unwrap().num_fmt.as_deref(), Some("0.000"));
    }

    #[test]
    fn unrelated_label_extensions_and_standard_sizes_do_not_create_overrides() {
        let c = read(
            r#"<c:plotArea><c:barChart><c:ser><c:idx val="0"/>
            <c:dLbls><c:dLbl><c:idx val="0"/><c:layout><c:manualLayout><c:w val="0.4"/></c:manualLayout></c:layout>
            <c:extLst><c:ext uri="unrelated"><f:layout><c:manualLayout><c:h val="0.3"/></c:manualLayout></f:layout></c:ext></c:extLst>
            </c:dLbl><c:showVal val="1"/></c:dLbls></c:ser></c:barChart></c:plotArea>"#,
        );
        let labels = c.series[0].fmt.labels.as_ref().unwrap();
        assert!(labels.points.is_empty());
        assert!(labels.point(0).unwrap().layout.is_none());
    }

    #[test]
    fn chart_title_keeps_script_typefaces_and_run_overrides() {
        let c = read(
            r#"<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1600">
            <a:latin typeface="Example Latin"/><a:ea typeface="Example East Asian"/><a:cs typeface="Example Complex"/>
            </a:defRPr></a:pPr><a:r><a:rPr><a:latin typeface="Example Override"/></a:rPr><a:t>Title</a:t></a:r>
            </a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea>"#,
        );
        let text = &c.fmt.title_text;
        assert_eq!(text.font.as_deref(), Some("Example Override"));
        assert_eq!(text.font_ea.as_deref(), Some("Example East Asian"));
        assert_eq!(text.font_cs.as_deref(), Some("Example Complex"));
        assert_eq!(text.size, Some(16.0));
    }

    #[test]
    fn reads_layouts_axes_labels_and_series_formatting() {
        let c = read(
            r#"<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1600" b="1"/></a:pPr><a:r><a:t>T</a:t></a:r></a:p></c:rich></c:tx>
            <c:layout><c:manualLayout><c:xMode val="edge"/><c:yMode val="edge"/><c:x val="0.1"/><c:y val="0.05"/></c:manualLayout></c:layout><c:overlay val="0"/></c:title>
            <c:plotArea><c:layout><c:manualLayout><c:layoutTarget val="inner"/><c:xMode val="edge"/><c:yMode val="edge"/><c:x val="0.1"/><c:y val="0.2"/><c:w val="0.8"/><c:h val="0.6"/></c:manualLayout></c:layout>
            <c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="3"/><c:order val="0"/><c:tx><c:v>S</c:v></c:tx>
            <c:marker><c:symbol val="star"/><c:size val="9"/></c:marker>
            <c:dLbls><c:dLbl><c:idx val="1"/><c:delete val="1"/></c:dLbl><c:numFmt formatCode="0.0" sourceLinked="0"/><c:dLblPos val="t"/><c:showVal val="1"/><c:showCatName val="1"/><c:separator>; </c:separator></c:dLbls>
            <c:trendline><c:trendlineType val="poly"/><c:order val="3"/></c:trendline>
            <c:errBars><c:errDir val="y"/><c:errBarType val="plus"/><c:errValType val="percentage"/><c:noEndCap val="1"/><c:val val="10"/></c:errBars>
            <c:val><c:numLit><c:formatCode>0%</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>0.5</c:v></c:pt><c:pt idx="1"><c:v>0.25</c:v></c:pt></c:numLit></c:val>
            <c:smooth val="1"/></c:ser><c:upDownBars><c:gapWidth val="50"/><c:upBars/><c:downBars/></c:upDownBars><c:axId val="1"/><c:axId val="2"/></c:lineChart>
            <c:catAx><c:axId val="1"/><c:scaling><c:orientation val="maxMin"/></c:scaling><c:delete val="1"/><c:axPos val="b"/><c:majorTickMark val="out"/><c:tickLblPos val="low"/><c:crossAx val="2"/></c:catAx>
            <c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/><c:max val="2"/><c:min val="-1"/></c:scaling><c:delete val="0"/><c:axPos val="l"/>
            <c:majorGridlines/><c:numFmt formatCode="0.00" sourceLinked="0"/><c:majorTickMark val="none"/><c:txPr><a:bodyPr rot="-2700000"/><a:p><a:pPr><a:defRPr sz="800"/></a:pPr></a:p></c:txPr>
            <c:crossAx val="1"/><c:crosses val="max"/><c:crossBetween val="midCat"/><c:majorUnit val="0.5"/></c:valAx>
            <c:dTable><c:showHorzBorder val="1"/><c:showKeys val="1"/></c:dTable></c:plotArea>
            <c:legend><c:legendPos val="t"/><c:legendEntry><c:idx val="0"/><c:delete val="1"/></c:legendEntry><c:overlay val="1"/></c:legend><c:dispBlanksAs val="zero"/>"#,
        );
        let f = &c.fmt;
        assert_eq!(f.title_text.size, Some(16.0));
        assert_eq!(f.title_text.bold, Some(true));
        let tl = f.title_layout.as_ref().expect("title layout");
        assert_eq!((tl.x, tl.y, tl.x_edge), (Some(0.1), Some(0.05), true));
        let pl = f.plot_layout.as_ref().expect("plot layout");
        assert!(pl.inner && pl.w == Some(0.8) && pl.h == Some(0.6));
        assert!(f.legend_overlay && f.legend_deleted == vec![0] && f.blanks_as_zero);
        assert_eq!(f.up_down_bars.as_ref().map(|u| u.0), Some(0.5));
        let dt = f.data_table.as_ref().expect("data table");
        assert!(dt.horizontal && dt.keys && !dt.vertical);
        let cat = f.axis(1).expect("cat axis");
        assert!(cat.deleted && cat.reversed && cat.major_tick == "out" && cat.label_pos == "low" && cat.kind == AxisKind::Category);
        let val = f.axis(2).expect("value axis");
        assert_eq!((val.min, val.max, val.major_unit), (Some(-1.0), Some(2.0), Some(0.5)));
        assert_eq!(val.num_fmt.as_deref(), Some("0.00"));
        assert_eq!((val.text.size, val.text.rot), (Some(8.0), Some(-45.0)));
        assert_eq!((val.crosses.as_deref(), val.between), (Some("max"), Some(false)));
        assert!(val.major_grid.is_some());
        let s = &c.series[0];
        assert_eq!((s.fmt.idx, s.marker.as_deref(), s.fmt.marker_size, s.smooth), (Some(3), Some("star"), Some(9.0), true));
        assert_eq!(s.fmt.num_fmt.as_deref(), Some("0%"));
        assert_eq!(s.fmt.axes, vec![1, 2]);
        let l = s.fmt.labels.as_ref().expect("labels");
        assert!(l.value && l.category && l.deleted == vec![1] && l.point(1).is_none() && l.point(0).is_some());
        assert_eq!((l.pos.as_deref(), l.num_fmt.as_deref(), l.separator.as_deref()), (Some("t"), Some("0.0"), Some("; ")));
        assert_eq!((s.fmt.trendlines[0].kind.as_str(), s.fmt.trendlines[0].order), ("poly", Some(3)));
        let eb = &s.fmt.err_bars[0];
        assert_eq!((eb.kind.as_str(), eb.val_type.as_str(), eb.val, eb.no_end_cap), ("plus", "percentage", 10.0, true));
    }

    #[test]
    fn a_series_smooth_flag_wins_over_the_scatter_style() {
        let ser = |smooth: &str| {
            format!(
                r#"<c:ser><c:idx val="0"/><c:order val="0"/><c:xVal><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>1</c:v></c:pt></c:numLit></c:xVal>
                <c:yVal><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>2</c:v></c:pt></c:numLit></c:yVal>{smooth}</c:ser>"#
            )
        };
        let chart = |smooth: &str| {
            read(&format!(r#"<c:plotArea><c:scatterChart><c:scatterStyle val="smoothMarker"/>{}</c:scatterChart></c:plotArea>"#, ser(smooth)))
        };
        assert!(!chart(r#"<c:smooth val="0"/>"#).series[0].smooth);
        assert!(chart("").series[0].smooth);
    }

    #[test]
    fn reads_a_theme_override_scheme() {
        let xml = r#"<a:themeOverride xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:clrScheme name="x">
            <a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="111111"/></a:dk2><a:lt2><a:srgbClr val="EEEEEE"/></a:lt2>
            <a:accent1><a:srgbClr val="FFB143"/></a:accent1><a:accent2><a:srgbClr val="FE7B48"/></a:accent2><a:accent3><a:srgbClr val="5FB6FF"/></a:accent3>
            <a:accent4><a:srgbClr val="3787FF"/></a:accent4><a:accent5><a:srgbClr val="06BB9A"/></a:accent5><a:accent6><a:srgbClr val="8E7EF1"/></a:accent6>
            <a:hlink><a:srgbClr val="304FFE"/></a:hlink><a:folHlink><a:srgbClr val="00A3F4"/></a:folHlink></a:clrScheme></a:themeOverride>"#;
        let s = theme_override_scheme(xml.as_bytes()).expect("scheme");
        assert_eq!(s.get(kition_slide_color::SchemeSlot::Accent1), kition_slide_color::Rgba::rgb(0xFF, 0xB1, 0x43));
        assert_eq!(s.get(kition_slide_color::SchemeSlot::Lt1), kition_slide_color::Rgba::WHITE);
        // An incomplete scheme is not used.
        assert!(theme_override_scheme(b"<a:themeOverride xmlns:a=\"x\"><a:clrScheme/></a:themeOverride>").is_none());
        assert!(theme_override_scheme(b"not xml").is_none());
    }
}
