//! Slide charts and the shared chart description (`KitDrawing` 1.1, `kition_office_core::chart`).
//!
//! A chart is described from Slide's model and a change written back by position, so a series
//! keeps its formatting. The chart part read from the file (`Chart::raw`) is patched to the new
//! description with the shared codec, so what Slide does not model survives the edit; the PPTX
//! writer reuses the patched part only when it reads back as the model, and writes the chart from
//! the model otherwise.

use kition_office_core::chart::{self as dm_chart, AxisSpec, ChartKind, ChartSpec, LegendPos, SeriesSpec, codec};
use kition_office_core::model as dm;
use kition_office_core::ops::DrawingError;
use kition_slide_color::{ColorTransform, SchemeSlot};

use crate::chart::{AxisKind, Chart, ChartType, DataLabels, Series};
use crate::style::{ColorBase, ColorRef, Fill, Line};

/// A Slide colour in the shared model's terms.
pub fn color_of(c: &ColorRef) -> dm::Color {
    let mut out = match &c.base {
        ColorBase::Rgb { rgb } => dm::Color::rgb(dm::Rgb(rgb.r, rgb.g, rgb.b)),
        ColorBase::Scheme { slot } if !matches!(slot, SchemeSlot::PhClr) => dm::Color::theme(slot.xml_name()),
        ColorBase::System { last, .. } => dm::Color::rgb(dm::Rgb(last.r, last.g, last.b)),
        // Preset names and the style placeholder colour have no theme slot: their value.
        _ => {
            let v = kition_slide_color::Rgba::BLACK;
            dm::Color::rgb(dm::Rgb(v.r, v.g, v.b))
        }
    };
    for m in &c.mods {
        match m {
            ColorTransform::Alpha(a) => out.alpha = Some(f64::from(*a) / 100_000.0),
            other => out.mods.push((other.xml_name().to_string(), transform_value(other))),
        }
    }
    out
}

fn transform_value(t: &ColorTransform) -> i32 {
    use ColorTransform::*;
    match t {
        Tint(v) | Shade(v) | LumMod(v) | LumOff(v) | SatMod(v) | SatOff(v) | HueMod(v) | HueOff(v) | Alpha(v) | AlphaMod(v) | AlphaOff(v)
        | RedMod(v) | GreenMod(v) | BlueMod(v) => *v,
        Comp | Inv | Gray => 0,
    }
}

/// A shared-model colour as a Slide colour.
pub fn color_ref(c: &dm::Color) -> Result<ColorRef, DrawingError> {
    let mut out = match &c.base {
        dm::ColorBase::Rgb { rgb } => ColorRef::rgb(kition_slide_color::Rgba::rgb(rgb.0, rgb.1, rgb.2)),
        dm::ColorBase::Theme { theme } => {
            ColorRef::scheme(SchemeSlot::from_xml(theme).ok_or_else(|| DrawingError::invalid(format!("unknown theme color `{theme}`")))?)
        }
    };
    for (name, value) in &c.mods {
        out.mods.push(
            ColorTransform::from_xml(name, Some(*value))
                .ok_or_else(|| DrawingError::unsupported("color.mods", format!("unknown color transform `{name}`")))?,
        );
    }
    if let Some(a) = c.alpha {
        out.mods.push(ColorTransform::Alpha((a.clamp(0.0, 1.0) * 100_000.0).round() as i32));
    }
    Ok(out)
}

/// The shared type of a Slide chart type; the types the description does not edit are named by
/// their DrawingML element (`c:` for the classic ones, the `cx:` layout for the 2016 ones).
pub fn kind_of(t: ChartType) -> ChartKind {
    match t {
        ChartType::Column => ChartKind::Column,
        ChartType::StackedColumn => ChartKind::StackedColumn,
        ChartType::PercentColumn => ChartKind::PercentColumn,
        ChartType::Bar => ChartKind::Bar,
        ChartType::StackedBar => ChartKind::StackedBar,
        ChartType::PercentBar => ChartKind::PercentBar,
        ChartType::Line => ChartKind::Line,
        ChartType::LineMarkers => ChartKind::LineMarkers,
        ChartType::StackedLine => ChartKind::StackedLine,
        ChartType::Area => ChartKind::Area,
        ChartType::StackedArea => ChartKind::StackedArea,
        ChartType::Pie => ChartKind::Pie,
        ChartType::Doughnut => ChartKind::Doughnut,
        ChartType::Scatter => ChartKind::Scatter,
        ChartType::Radar => ChartKind::Radar,
        ChartType::Bubble => ChartKind::Other("bubbleChart".into()),
        ChartType::FilledRadar => ChartKind::Other("radarChart".into()),
        ChartType::Stock => ChartKind::Other("stockChart".into()),
        ChartType::Surface => ChartKind::Other("surfaceChart".into()),
        ChartType::Histogram => ChartKind::Other("clusteredColumn".into()),
        ChartType::Pareto => ChartKind::Other("paretoLine".into()),
        ChartType::BoxWhisker => ChartKind::Other("boxWhisker".into()),
        ChartType::Waterfall => ChartKind::Other("waterfall".into()),
        ChartType::Funnel => ChartKind::Other("funnel".into()),
        ChartType::Treemap => ChartKind::Other("treemap".into()),
        ChartType::Sunburst => ChartKind::Other("sunburst".into()),
        // Described by its first series' type, with each series' own (DR15).
        ChartType::Combo => ChartKind::Column,
    }
}

/// The Slide type of a shared type the description edits.
fn type_of(k: &ChartKind) -> Option<ChartType> {
    Some(match k {
        ChartKind::Column => ChartType::Column,
        ChartKind::StackedColumn => ChartType::StackedColumn,
        ChartKind::PercentColumn => ChartType::PercentColumn,
        ChartKind::Bar => ChartType::Bar,
        ChartKind::StackedBar => ChartType::StackedBar,
        ChartKind::PercentBar => ChartType::PercentBar,
        ChartKind::Line => ChartType::Line,
        ChartKind::LineMarkers => ChartType::LineMarkers,
        ChartKind::StackedLine => ChartType::StackedLine,
        ChartKind::Area => ChartType::Area,
        ChartKind::StackedArea => ChartType::StackedArea,
        ChartKind::Pie => ChartType::Pie,
        ChartKind::Doughnut => ChartType::Doughnut,
        ChartKind::Scatter => ChartType::Scatter,
        ChartKind::Radar => ChartType::Radar,
        ChartKind::Other(_) => return None,
    })
}

/// The main type of a chart: a combination chart's is its first series' type.
fn main_type(c: &Chart) -> ChartType {
    match c.kind {
        ChartType::Combo => c.series.first().and_then(|s| s.kind).unwrap_or(ChartType::Column),
        k => k,
    }
}

fn number_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { v.to_string() }
}

/// A Slide chart, described.
pub fn spec_of(c: &Chart) -> ChartSpec {
    let main = main_type(c);
    let scatter = main == ChartType::Scatter;
    let mut categories = c.categories.clone();
    if scatter && let Some(first) = c.series.first().filter(|s| !s.x_values.is_empty()) {
        categories = first.x_values.iter().map(|v| v.map(number_text).unwrap_or_default()).collect();
    }
    let n = categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
    categories.extend((categories.len()..n).map(|i| (i + 1).to_string()));
    ChartSpec {
        kind: kind_of(main),
        title: c.title.clone(),
        series: c
            .series
            .iter()
            .map(|s| {
                let mut values = s.values.clone();
                values.resize(n, None);
                SeriesSpec {
                    name: s.name.clone(),
                    values,
                    color: match &s.fill {
                        Some(Fill::Solid { color }) => Some(color_of(color)),
                        _ => None,
                    },
                    kind: s.kind.filter(|k| c.kind == ChartType::Combo && *k != main).map(kind_of),
                    line_width: s.line.as_ref().and_then(|l| l.width),
                    // A line chart without markers is its type, not a series' choice.
                    marker: s.marker.clone().filter(|m| !(m == "none" && matches!(main, ChartType::Line | ChartType::StackedLine))),
                    smooth: s.smooth,
                    trendline: s.fmt.trendlines.first().map(trendline_spec).filter(|t| t.check().is_ok()),
                    // On other axes than the first series': the secondary ones.
                    secondary: !s.fmt.axes.is_empty() && c.series.first().is_some_and(|f| f.fmt.axes != s.fmt.axes),
                    error_bars: s.fmt.err_bars.iter().find(|e| value_bars(e)).map(error_bars_spec).filter(|e| e.check().is_ok()),
                }
            })
            .collect(),
        categories,
        legend: c.legend.as_deref().map(LegendPos::from_ooxml),
        data_labels: c.data_labels,
        gridlines: c.gridlines,
        x_title: c.axis_titles.0.clone(),
        y_title: c.axis_titles.1.clone(),
        x_axis: axis_spec(c, c.main_axes().map(|a| a.0), false),
        y_axis: axis_spec(c, c.main_axes().map(|a| a.1), true),
        data_label_position: c.fmt.labels.as_ref().and_then(|l| l.pos.clone()),
    }
}

/// A series' trendline, described (KitDrawing 1.1 DR16b).
fn trendline_spec(t: &crate::chart::Trendline) -> dm_chart::Trendline {
    dm_chart::Trendline {
        kind: t.kind.clone(),
        order: t.order,
        period: t.period,
        forward: (t.forward > 0.0).then_some(t.forward),
        backward: (t.backward > 0.0).then_some(t.backward),
    }
}

/// Error bars the description has: on the values, both ways, not custom amounts.
fn value_bars(e: &crate::chart::ErrorBars) -> bool {
    e.dir.as_deref().is_none_or(|d| d == "y") && (e.kind.is_empty() || e.kind == "both") && e.val_type != "cust"
}

fn error_bars_spec(e: &crate::chart::ErrorBars) -> dm_chart::ErrorBars {
    dm_chart::ErrorBars { kind: e.val_type.clone(), value: (e.val_type != "stdErr").then_some(e.val) }
}

/// Axis `id`'s scale and format; a chart without axes of its own has its value axis's range only.
fn axis_spec(c: &Chart, id: Option<u32>, value: bool) -> AxisSpec {
    match id.and_then(|i| c.fmt.axis(i)) {
        Some(a) => AxisSpec {
            min: a.min,
            max: a.max,
            major: a.major_unit.filter(|_| a.kind == AxisKind::Value),
            number_format: a.num_fmt.clone().filter(|f| !f.is_empty() && f != "General"),
            hidden: a.deleted,
        },
        None if value => AxisSpec { min: c.y_min, max: c.y_max, ..Default::default() },
        None => AxisSpec::default(),
    }
}

/// The light grey Office draws gridlines in.
fn grid_line() -> Line {
    Line::solid(ColorRef::rgb(kition_slide_color::Rgba::rgb(0xD9, 0xD9, 0xD9)), 0.75)
}

/// Writes description `spec` into `c`, changing only what differs from `before` (its description
/// when the change began), and keeps its kept part in step.
pub fn apply_spec(c: &mut Chart, before: &ChartSpec, spec: &ChartSpec) -> Result<(), DrawingError> {
    if spec == before {
        return Ok(());
    }
    if spec.kind != before.kind && matches!(before.kind, ChartKind::Other(_)) {
        return Err(DrawingError::unsupported("chart.kind", "this chart keeps its type"));
    }
    let main = if spec.kind != before.kind {
        type_of(&spec.kind).ok_or_else(|| DrawingError::unsupported("chart.kind", "this chart keeps its type"))?
    } else {
        main_type(c)
    };
    if spec.kind.is_pie() && spec.series.len() > 1 {
        return Err(DrawingError::invalid("a pie or doughnut chart shows one series"));
    }
    let combo = spec.series.iter().any(|s| s.kind.as_ref().is_some_and(|k| *k != spec.kind));
    let mut series_types = Vec::with_capacity(spec.series.len());
    for s in &spec.series {
        series_types.push(match &s.kind {
            Some(k) if combo => Some(type_of(k).ok_or_else(|| DrawingError::unsupported("chart.series.kind", "this series keeps its type"))?),
            _ => None,
        });
    }
    if spec.kind != before.kind
        || combo != (c.kind == ChartType::Combo)
        || spec.series.iter().map(|s| &s.kind).ne(before.series.iter().map(|s| &s.kind))
    {
        c.kind = if combo { ChartType::Combo } else { main };
        if spec.kind.is_pie() {
            c.vary_colors = true;
        } else if before.kind.is_pie() {
            c.vary_colors = false;
        }
    }
    c.title.clone_from(&spec.title);
    c.legend = spec.legend.map(|l| l.ooxml().to_string());
    c.data_labels = spec.data_labels;
    c.gridlines = spec.gridlines;
    c.axis_titles = (spec.x_title.clone(), spec.y_title.clone());
    // The axes the chart is drawn with take their scale, format, title and gridlines (a chart
    // without axes of its own gets them when its axes are formatted).
    let axes_changed = spec.x_axis != before.x_axis || spec.y_axis != before.y_axis;
    if axes_changed || c.main_axes().is_some() {
        let (x, y) = if axes_changed { c.ensure_axes() } else { c.main_axes().unwrap_or((1, 2)) };
        if let Some(a) = c.axis_mut(y) {
            if spec.y_axis != before.y_axis {
                (a.min, a.max, a.major_unit, a.num_fmt, a.deleted) =
                    (spec.y_axis.min, spec.y_axis.max, spec.y_axis.major, spec.y_axis.number_format.clone(), spec.y_axis.hidden);
            }
            if spec.y_title != before.y_title {
                a.title.clone_from(&spec.y_title);
            }
            if spec.gridlines != before.gridlines {
                a.major_grid = spec.gridlines.then(grid_line);
            }
        }
        if let Some(a) = c.axis_mut(x) {
            if spec.x_axis != before.x_axis {
                (a.min, a.max, a.num_fmt, a.deleted) = (spec.x_axis.min, spec.x_axis.max, spec.x_axis.number_format.clone(), spec.x_axis.hidden);
                if a.kind == AxisKind::Value {
                    a.major_unit = spec.x_axis.major;
                }
            }
            if spec.x_title != before.x_title {
                a.title.clone_from(&spec.x_title);
            }
        }
    }
    if spec.y_axis != before.y_axis {
        (c.y_min, c.y_max) = (spec.y_axis.min, spec.y_axis.max);
    }
    if spec.data_label_position != before.data_label_position {
        let labels = c.fmt.labels.get_or_insert_with(|| DataLabels { value: spec.data_labels, ..Default::default() });
        labels.pos.clone_from(&spec.data_label_position);
    }
    for (i, (want, out)) in spec.series.iter().zip(c.series.iter_mut()).enumerate() {
        let was = before.series.get(i);
        if was.map(|b| b.line_width) != Some(want.line_width) {
            match (want.line_width, out.line.as_mut()) {
                (w, Some(l)) => l.width = w,
                (Some(w), None) => out.line = Some(Line { width: Some(w), ..Default::default() }),
                (None, None) => {}
            }
        }
        if was.map(|b| &b.marker) != Some(&want.marker) {
            out.marker.clone_from(&want.marker);
        }
        if was.map(|b| b.smooth) != Some(want.smooth) {
            out.smooth = want.smooth;
        }
        // A trendline or error bars given anew replace the series' first trendline, or its
        // error bars on the values; others it has stay.
        if was.map(|b| &b.trendline) != Some(&want.trendline) {
            let kept = out.fmt.trendlines.first().cloned().unwrap_or_default();
            match &want.trendline {
                Some(t) => {
                    let line = crate::chart::Trendline {
                        kind: t.kind.clone(),
                        order: t.order,
                        period: t.period,
                        forward: t.forward.unwrap_or(0.0),
                        backward: t.backward.unwrap_or(0.0),
                        ..kept
                    };
                    match out.fmt.trendlines.first_mut() {
                        Some(first) => *first = line,
                        None => out.fmt.trendlines.push(line),
                    }
                }
                None => {
                    if !out.fmt.trendlines.is_empty() {
                        out.fmt.trendlines.remove(0);
                    }
                }
            }
        }
        if was.map(|b| &b.error_bars) != Some(&want.error_bars) {
            out.fmt.err_bars.retain(|e| !value_bars(e));
            if let Some(e) = &want.error_bars {
                out.fmt.err_bars.push(crate::chart::ErrorBars {
                    dir: (main == ChartType::Scatter).then(|| "y".to_string()),
                    kind: "both".into(),
                    val_type: e.kind.clone(),
                    val: e.value.unwrap_or(if e.kind == "percentage" { 5.0 } else { 1.0 }),
                    ..Default::default()
                });
            }
        }
    }
    // Series moved to or from the secondary axes: a hidden category axis and a value axis on the
    // right, made for the first and removed after the last.
    if spec.series.iter().map(|s| s.secondary).ne(before.series.iter().map(|s| s.secondary)) && !spec.kind.is_pie() {
        let (x, y) = c.ensure_axes();
        let primary = c.series.first().map(|f| f.fmt.axes.clone()).filter(|a| a.len() >= 2).unwrap_or_else(|| vec![x, y]);
        let existing = c.series.iter().map(|s| s.fmt.axes.clone()).find(|a| a.len() >= 2 && *a != primary);
        let wants = spec.series.iter().any(|s| s.secondary);
        let sec = match existing {
            Some(a) => a,
            None if wants => {
                let next = c.fmt.axes.iter().map(|a| a.id).max().unwrap_or(0) + 1;
                let (sc, sv) = (next, next + 1);
                let mut cat = c.default_axis(AxisKind::Category, "b", sc, sv);
                cat.deleted = true;
                let mut val = c.default_axis(AxisKind::Value, "r", sv, sc);
                val.crosses = Some("max".into());
                val.major_grid = None;
                c.fmt.axes.push(cat);
                c.fmt.axes.push(val);
                vec![sc, sv]
            }
            None => Vec::new(),
        };
        for (want, out) in spec.series.iter().zip(c.series.iter_mut()) {
            out.secondary_axis = want.secondary;
            out.fmt.axes = if want.secondary { sec.clone() } else { primary.clone() };
        }
        if !wants {
            c.fmt.axes.retain(|a| !sec.contains(&a.id));
        }
    }
    let scatter = main == ChartType::Scatter;
    if spec.categories != before.categories || spec.series != before.series {
        let x_values: Vec<Option<f64>> = spec.categories.iter().map(|t| t.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect();
        if !scatter || spec.categories != before.categories {
            c.categories.clone_from(&spec.categories);
        }
        let old = std::mem::take(&mut c.series);
        c.series = spec
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| -> Result<Series, DrawingError> {
                // A new series looks like the one before it, without its point overrides.
                let mut out = old.get(i).cloned().unwrap_or_else(|| Series { smooth: old.last().is_some_and(|l| l.smooth), ..Default::default() });
                out.name.clone_from(&s.name);
                out.values.clone_from(&s.values);
                if scatter {
                    out.x_values.clone_from(&x_values);
                }
                let had = before.series.get(i).and_then(|b| b.color.as_ref());
                if s.color.as_ref() != had {
                    out.fill = s.color.as_ref().map(color_ref).transpose()?.map(Fill::solid);
                }
                out.kind = series_types.get(i).copied().flatten().or(if combo { Some(main) } else { None });
                Ok(out)
            })
            .collect::<Result<_, _>>()?;
    } else {
        for (i, (s, out)) in spec.series.iter().zip(c.series.iter_mut()).enumerate() {
            let had = before.series.get(i).and_then(|b| b.color.as_ref());
            if s.color.as_ref() != had {
                out.fill = s.color.as_ref().map(color_ref).transpose()?.map(Fill::solid);
            }
            out.kind = series_types.get(i).copied().flatten().or(if combo { Some(main) } else { None });
        }
    }
    Ok(())
}

/// Kept part `raw` patched from what it says (`was`, the chart as Slide reads the part) to chart
/// `now`; `None` when the codec cannot read or patch it, or `now`'s type is one the description
/// does not edit.
pub fn patched_part(raw: &str, was: &Chart, now: &Chart) -> Option<String> {
    let (before, after) = (spec_of(was), spec_of(now));
    if matches!(after.kind, ChartKind::Other(_)) {
        return None;
    }
    let read = codec::read(raw).ok()?;
    codec::patch(raw, &carried(&read, &before, &after)).ok()
}

fn carried(read: &ChartSpec, before: &ChartSpec, spec: &ChartSpec) -> ChartSpec {
    let mut t = read.clone();
    macro_rules! carry {
        ($($f:ident),*) => { $( if spec.$f != before.$f { t.$f = spec.$f.clone(); } )* };
    }
    carry!(kind, title, legend, data_labels, gridlines, x_title, y_title, categories, x_axis, y_axis, data_label_position);
    if spec.series != before.series {
        t.series = spec
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (was, part) = (before.series.get(i), read.series.get(i));
                // A colour or type the change left alone stays as the part has it.
                let color = match (was, part) {
                    (Some(w), Some(p)) if w.color == s.color => p.color.clone(),
                    _ => s.color.clone(),
                };
                let kind = match (was, part) {
                    (Some(w), Some(p)) if w.kind == s.kind => p.kind.clone(),
                    _ => s.kind.clone(),
                };
                // Likewise its line, marker and smoothing.
                let same = |f: fn(&SeriesSpec) -> SeriesSpec| matches!((was, part), (Some(w), Some(_)) if f(w) == f(s));
                let look = |x: &SeriesSpec| SeriesSpec {
                    secondary: x.secondary,
                    line_width: x.line_width,
                    marker: x.marker.clone(),
                    smooth: x.smooth,
                    trendline: x.trendline.clone(),
                    error_bars: x.error_bars.clone(),
                    ..Default::default()
                };
                let from = if same(look) { part.unwrap_or(s) } else { s };
                SeriesSpec {
                    name: s.name.clone(),
                    values: s.values.clone(),
                    color,
                    kind,
                    line_width: from.line_width,
                    marker: from.marker.clone(),
                    smooth: from.smooth,
                    trendline: from.trendline.clone(),
                    error_bars: from.error_bars.clone(),
                    secondary: from.secondary,
                }
            })
            .collect();
    }
    let n = t.categories.len();
    for s in &mut t.series {
        s.values.resize(n, None);
    }
    t
}

/// Part `xml`, written from chart `c` (one without a kept part), with the formatting the writer
/// leaves out (axis scales and formats, label positions, series' lines and markers) patched in.
/// `None` when there is nothing to add or the codec cannot read it.
pub fn formatted_part(xml: &str, c: &Chart) -> Option<String> {
    let read = codec::read(xml).ok()?;
    let want = spec_of(c);
    if matches!(want.kind, ChartKind::Other(_)) || matches!(read.kind, ChartKind::Other(_)) {
        return None;
    }
    let mut t = read.clone();
    t.x_axis = want.x_axis;
    t.y_axis = want.y_axis;
    t.data_label_position = want.data_label_position;
    if t.series.len() == want.series.len() {
        for (a, b) in t.series.iter_mut().zip(&want.series) {
            a.line_width = b.line_width;
            a.marker.clone_from(&b.marker);
            a.smooth = b.smooth;
            a.trendline.clone_from(&b.trendline);
            a.error_bars.clone_from(&b.error_bars);
            a.secondary = b.secondary;
        }
    }
    if t == read {
        return None;
    }
    codec::patch(xml, &t).ok()
}

/// The chart a `drawing.insert` of kind `chart` makes from its description.
pub fn new_chart(spec: &ChartSpec) -> Result<Chart, DrawingError> {
    let kind = type_of(&spec.kind).ok_or_else(|| DrawingError::unsupported("chart.kind", "insert a chart of a type Slide makes"))?;
    let mut c = Chart::sample(kind);
    let before = spec_of(&c);
    apply_spec(&mut c, &before, spec)?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::model::{Color, Rgb};

    #[test]
    fn a_chart_reads_and_writes_back_through_the_description() {
        let mut c = Chart::sample(ChartType::Column);
        let before = spec_of(&c);
        assert_eq!((before.kind.clone(), before.series.len(), before.categories.len()), (ChartKind::Column, 3, 4));
        let mut spec = before.clone();
        spec.kind = ChartKind::StackedBar;
        spec.series.truncate(2);
        spec.series[1].color = Some(Color::rgb(Rgb(1, 2, 3)));
        spec.categories.push("Q5".into());
        for s in &mut spec.series {
            s.values.push(Some(9.0));
        }
        spec.legend = Some(LegendPos::Top);
        apply_spec(&mut c, &before, &spec).unwrap();
        assert_eq!(spec_of(&c), spec);
        assert_eq!(c.kind, ChartType::StackedBar);
    }

    #[test]
    fn a_combination_chart_keeps_its_series_types_and_other_types_keep_theirs() {
        let mut c = Chart::sample(ChartType::Column);
        let before = spec_of(&c);
        let mut spec = before.clone();
        spec.series[2].kind = Some(ChartKind::Line);
        apply_spec(&mut c, &before, &spec).unwrap();
        assert_eq!(c.kind, ChartType::Combo);
        assert_eq!(c.series[2].kind, Some(ChartType::Line));
        assert_eq!(spec_of(&c), spec);
        let mut w = Chart::sample(ChartType::Waterfall);
        let b = spec_of(&w);
        let mut s = b.clone();
        s.kind = ChartKind::Pie;
        assert_eq!(apply_spec(&mut w, &b, &s).unwrap_err().code, "unsupported");
    }

    #[test]
    fn a_series_moves_to_the_secondary_axis_and_back() {
        let mut c = Chart::sample(ChartType::Column);
        let spec = spec_of(&c);
        let raw = codec::write(&spec);
        c.raw = Some(raw.clone());
        let was = c.clone();
        let axes = c.fmt.axes.len();
        let mut next = spec.clone();
        next.series[1].kind = Some(ChartKind::Line);
        next.series[1].secondary = true;
        apply_spec(&mut c, &spec, &next).unwrap();
        assert_eq!(c.kind, ChartType::Combo);
        assert_ne!(c.series[1].fmt.axes, c.series[0].fmt.axes);
        assert_eq!(c.fmt.axes.len(), axes.max(2) + 2);
        assert!(c.fmt.axes.iter().any(|a| a.pos == "r" && a.crosses.as_deref() == Some("max")));
        let now = spec_of(&c);
        assert_eq!((now.series[1].secondary, now.series[1].kind.clone(), now.series[0].secondary), (true, Some(ChartKind::Line), false));
        let back = codec::read(&patched_part(&raw, &was, &c).unwrap()).unwrap();
        // In their order: the line series keeps its place between the columns.
        assert_eq!(back.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Series 1", "Series 2", "Series 3"]);
        assert!(back.series[1].secondary);
        let mut flat = now.clone();
        flat.series[1].secondary = false;
        apply_spec(&mut c, &now, &flat).unwrap();
        assert_eq!(c.series[1].fmt.axes, c.series[0].fmt.axes);
        assert_eq!(c.fmt.axes.len(), axes.max(2));
    }

    #[test]
    fn trendlines_and_error_bars_map_both_ways_and_patch_the_part() {
        let mut c = Chart::sample(ChartType::Line);
        c.series.truncate(1);
        let spec = spec_of(&c);
        let raw = codec::write(&spec);
        c.raw = Some(raw.clone());
        let was = c.clone();
        let mut next = spec.clone();
        next.series[0].trendline = Some(dm_chart::Trendline { kind: "exp".into(), forward: Some(2.0), ..Default::default() });
        next.series[0].error_bars = Some(dm_chart::ErrorBars { kind: "fixedVal".into(), value: Some(0.25) });
        apply_spec(&mut c, &spec, &next).unwrap();
        assert_eq!((c.series[0].fmt.trendlines[0].kind.as_str(), c.series[0].fmt.trendlines[0].forward), ("exp", 2.0));
        assert_eq!((c.series[0].fmt.err_bars[0].val_type.as_str(), c.series[0].fmt.err_bars[0].val), ("fixedVal", 0.25));
        assert_eq!(spec_of(&c), next);
        // Saving patches them into the kept part.
        let patched = patched_part(&raw, &was, &c).expect("the part patches");
        let back = codec::read(&patched).unwrap();
        assert_eq!(
            (back.series[0].trendline.clone(), back.series[0].error_bars.clone()),
            (next.series[0].trendline.clone(), next.series[0].error_bars.clone())
        );
        // Removed again.
        let mut none = next.clone();
        none.series[0].trendline = None;
        none.series[0].error_bars = None;
        apply_spec(&mut c, &next, &none).unwrap();
        assert!(c.series[0].fmt.trendlines.is_empty() && c.series[0].fmt.err_bars.is_empty());
    }

    #[test]
    fn an_edit_leaves_the_kept_part_and_saving_patches_it() {
        let mut c = Chart::sample(ChartType::Line);
        c.series.truncate(1);
        let spec = spec_of(&c);
        let raw = codec::write(&spec).replace("</c:chartSpace>", "<c:extLst><c:ext uri=\"{K}\"><kept/></c:ext></c:extLst></c:chartSpace>");
        c.raw = Some(raw.clone());
        let was = c.clone();
        let mut next = spec.clone();
        next.title = Some("Trend".into());
        apply_spec(&mut c, &spec, &next).unwrap();
        assert_eq!(c.raw.as_deref(), Some(raw.as_str()), "an edit does not touch the kept part");
        let patched = patched_part(&raw, &was, &c).expect("the part patches");
        assert!(patched.contains("<kept/>") && patched.contains("Trend"));
        assert!(patched_part("<c:chartSpace/>", &was, &c).is_none());
    }
}
