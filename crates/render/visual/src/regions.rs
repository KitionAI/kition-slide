//! Diagnostics that point a failing slide at a cause: which kind of object the differing pixels
//! fall on, which features the slide uses, and which requested font families are not available.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::style::Fill;
use kition_slide_model::text::RunKind;
use kition_slide_model::{Shape, ShapeKind, Slide, SlideFile};

/// The diagnostic kind of a top-level shape.
pub fn kind(s: &Shape) -> &'static str {
    match &s.kind {
        ShapeKind::Shape => {
            if s.text.as_ref().is_some_and(|t| t.paragraphs.iter().any(|p| p.runs.iter().any(|r| !r.text.trim().is_empty()))) {
                "text"
            } else if s.ph.is_some() {
                "empty-placeholder"
            } else {
                "shape"
            }
        }
        ShapeKind::Picture { .. } => "picture",
        ShapeKind::Group { .. } => "group",
        ShapeKind::Connector { .. } => "connector",
        ShapeKind::Table(_) => "table",
        ShapeKind::Chart(_) => "chart",
        ShapeKind::Media(_) => "media",
        ShapeKind::Ink { .. } => "ink",
        ShapeKind::Opaque { .. } => "object",
    }
}

/// Differing pixels (largest channel difference above `threshold`) per kind of the topmost
/// top-level slide shape whose box (rotated boxes by their bounds) covers them; `backdrop` for
/// pixels no slide shape covers (background, layout and master graphics).
pub fn attribute(
    pres: &SlideFile,
    slide: &Slide,
    scale_x: f64,
    scale_y: f64,
    width: u32,
    height: u32,
    max_diff: &[u8],
    threshold: u8,
) -> BTreeMap<String, u64> {
    let (w, h) = (width as usize, height as usize);
    let mut owner = vec![u16::MAX; w * h];
    let mut kinds: Vec<&'static str> = Vec::new();
    if let Some(ctx) = Ctx::for_slide(pres, slide) {
        for s in slide.shapes.iter().filter(|s| !s.hidden) {
            let x = resolve::xfrm(&ctx, s);
            let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
            let (sin, cos) = x.rot.to_radians().sin_cos();
            let hw = (x.w * cos.abs() + x.h * sin.abs()) / 2.0;
            let hh = (x.w * sin.abs() + x.h * cos.abs()) / 2.0;
            let clamp = |v: f64, m: usize| v.max(0.0).min(m as f64) as usize;
            let (x0, x1) = (clamp(((cx - hw) * scale_x).floor(), w), clamp(((cx + hw) * scale_x).ceil(), w));
            let (y0, y1) = (clamp(((cy - hh) * scale_y).floor(), h), clamp(((cy + hh) * scale_y).ceil(), h));
            let k = kinds.len().min(u16::MAX as usize - 1) as u16;
            kinds.push(kind(s));
            for row in y0..y1 {
                owner[row * w + x0..row * w + x1].fill(k);
            }
        }
    }
    attribute_counts(&owner, &kinds, max_diff, threshold)
}

/// Count by owner first, allocating diagnostic names only once per nonempty owner.
fn attribute_counts(owner: &[u16], kinds: &[&str], max_diff: &[u8], threshold: u8) -> BTreeMap<String, u64> {
    // Preserve the existing u16 lookup exactly, including its sentinel alias when there
    // are at least 65,536 kinds. Changing the owner encoding is a separate behavior change.
    let slots = kinds.len().min(u16::MAX as usize + 1);
    let mut counts = vec![0u64; slots + 1];
    for (i, d) in max_diff.iter().enumerate() {
        if *d > threshold {
            let k = owner.get(i).copied().unwrap_or(u16::MAX) as usize;
            counts[k.min(slots)] += 1;
        }
    }
    let mut out = BTreeMap::new();
    for (k, count) in counts.into_iter().enumerate() {
        if count != 0 {
            let name = if k < slots { kinds[k] } else { "backdrop" };
            *out.entry(name.to_string()).or_default() += count;
        }
    }
    out
}

fn fill_kind(f: &Fill) -> Option<&'static str> {
    match f {
        Fill::Gradient(_) => Some("gradient"),
        Fill::Pattern(_) => Some("pattern"),
        Fill::Picture(_) => Some("picture-fill"),
        _ => None,
    }
}

/// Features the slide uses (counts by name), its own shapes walked through groups.
pub fn features(pres: &SlideFile, slide: &Slide) -> BTreeMap<String, u64> {
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    let mut add = |k: &str| *out.entry(k.to_string()).or_default() += 1;
    let mut cjk = false;
    kition_slide_model::walk(&slide.shapes, &mut |s, depth| {
        add(kind(s));
        if depth > 0 {
            add("nested");
        }
        if let ShapeKind::Opaque { label, .. } = &s.kind {
            add(&format!("object:{label}"));
        }
        if let ShapeKind::Chart(c) = &s.kind {
            add(&format!("chart:{:?}", c.kind).to_lowercase());
        }
        if s.effects.as_ref().is_some_and(|e| !e.is_empty()) {
            add("effects");
        }
        if let Some(k) = s.fill.as_ref().and_then(fill_kind) {
            add(k);
        }
        if s.xfrm.is_some_and(|x| x.rot != 0.0) {
            add("rotated");
        }
        if let Some(t) = &s.text {
            if t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| r.text.chars().any(is_cjk)) {
                cjk = true;
            }
            if t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| !matches!(r.kind, RunKind::Text | RunKind::Break)) {
                add("fields-or-math");
            }
        }
    });
    if cjk {
        out.insert("cjk-text".into(), 1);
    }
    if let Some(b) = &slide.background {
        let k = match b {
            kition_slide_model::Background::Fill { fill } => fill_kind(fill).unwrap_or("solid"),
            _ => "theme-style",
        };
        out.insert(format!("background:{k}"), 1);
    }
    if slide.hidden {
        out.insert("hidden-slide".into(), 1);
    }
    if pres.master_for(slide).is_some_and(|(_, l)| l.is_none()) {
        out.insert("no-layout".into(), 1);
    }
    out
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0xAC00..=0xD7AF)
}

/// Font families the slide's text and themes request.
pub fn requested_fonts(pres: &SlideFile) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    let mut theme_fonts = BTreeMap::new();
    for m in &pres.masters {
        let f = &m.theme.fonts;
        for (k, v) in [("+mj-lt", &f.major.latin), ("+mj-ea", &f.major.ea), ("+mn-lt", &f.minor.latin), ("+mn-ea", &f.minor.ea)] {
            if !v.is_empty() {
                set.insert(v.clone());
                theme_fonts.entry(k).or_insert_with(|| v.clone());
            }
        }
    }
    let mut take = |name: &Option<String>| {
        if let Some(n) = name {
            let n = theme_fonts.get(n.as_str()).cloned().unwrap_or_else(|| n.clone());
            if !n.is_empty() && !n.starts_with('+') {
                set.insert(n);
            }
        }
    };
    for s in &pres.slides {
        kition_slide_model::walk(&s.shapes, &mut |sh, _| {
            if let Some(t) = &sh.text {
                for r in t.paragraphs.iter().flat_map(|p| &p.runs) {
                    take(&r.props.font);
                    take(&r.props.font_ea);
                }
            }
            if let ShapeKind::Chart(chart) = &sh.kind {
                chart_fonts(chart, &mut take);
            }
        });
    }
    set
}

/// Visit chart text independently of shape text, including per-point label overrides.
fn chart_fonts(chart: &kition_slide_model::chart::Chart, take: &mut impl FnMut(&Option<String>)) {
    let fmt = &chart.fmt;
    let mut texts = vec![&fmt.text, &fmt.title_text, &fmt.legend_text];
    for axis in &fmt.axes {
        texts.extend([&axis.text, &axis.title_text]);
    }
    if let Some(table) = &fmt.data_table {
        texts.push(&table.text);
    }
    let mut labels: Vec<_> = fmt.labels.iter().chain(chart.series.iter().filter_map(|series| series.fmt.labels.as_ref())).collect();
    while let Some(label) = labels.pop() {
        texts.push(&label.text);
        labels.extend(label.points.iter().map(|(_, point)| point));
    }
    for text in texts {
        take(&text.font);
        take(&text.font_ea);
        take(&text.font_cs);
    }
}

/// The requested families the font database cannot find (it then substitutes).
pub fn missing_fonts(pres: &SlideFile) -> Vec<String> {
    let db = kition_slide_fonts::FontDb::global();
    requested_fonts(pres).into_iter().filter(|f| !db.has_family(f)).collect()
}

/// Requested families the font database cannot find, each with the family of the face drawn in
/// its place (an open substitute, or the general fallback), and for East Asian families the face
/// their East Asian text is drawn with (`"<latin> + <east asian> (East Asian)"`).
pub fn substitutions(pres: &SlideFile) -> BTreeMap<String, String> {
    let db = kition_slide_fonts::FontDb::global();
    missing_fonts(pres)
        .into_iter()
        .map(|f| {
            let latin = db.face(&f, "Regular").family.clone();
            let east_asian = kition_slide_fonts::east_asian_substitutes(&f).iter().find(|s| db.has_family(s));
            let used = match east_asian {
                Some(ea) => format!("{latin} + {ea} (East Asian)"),
                None => latin,
            };
            (f, used)
        })
        .collect()
}

pub fn map_json(m: &BTreeMap<String, u64>) -> Value {
    json!(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_model::chart::{Axis, Chart, ChartText, ChartType, DataLabels, DataTable};

    #[test]
    fn chart_only_typefaces_are_reported_in_font_diagnostics() {
        let text = |name: &str| ChartText { font: Some(name.into()), ..Default::default() };
        let label = |name: &str| DataLabels { text: text(name), ..Default::default() };
        let mut chart = Chart::sample(ChartType::Column);
        chart.fmt.text = text("Example Chart Default");
        chart.fmt.title_text.font_ea = Some("Example Chart Title".into());
        chart.fmt.legend_text.font_cs = Some("Example Chart Legend".into());
        chart.fmt.axes = vec![Axis { text: text("Example Chart Axis"), title_text: text("Example Chart Axis Title"), ..Default::default() }];
        chart.fmt.data_table = Some(DataTable { text: text("Example Chart Table"), ..Default::default() });
        let mut group = label("Example Chart Group Label");
        group.points.push((0, label("Example Chart Point Label")));
        chart.fmt.labels = Some(group);
        let mut series = label("Example Chart Series Label");
        let mut point = label("Example Chart Series Point");
        point.points.push((1, label("Example Chart Nested Point")));
        series.points.push((0, point));
        chart.series[0].fmt.labels = Some(series);
        let mut pres = SlideFile::default();
        std::sync::Arc::make_mut(&mut pres.slides[0]).shapes = vec![Shape { kind: ShapeKind::Chart(Box::new(chart)), ..Default::default() }];
        let requested = requested_fonts(&pres);
        let missing = missing_fonts(&pres);
        let substitutions = substitutions(&pres);
        for name in [
            "Example Chart Default",
            "Example Chart Title",
            "Example Chart Legend",
            "Example Chart Axis",
            "Example Chart Axis Title",
            "Example Chart Table",
            "Example Chart Group Label",
            "Example Chart Point Label",
            "Example Chart Series Label",
            "Example Chart Series Point",
            "Example Chart Nested Point",
        ] {
            assert!(requested.contains(name), "missing requested font: {name}");
            assert!(missing.iter().any(|font| font == name), "missing unavailable font: {name}");
            assert!(substitutions.contains_key(name), "missing font substitution: {name}");
        }
    }
}

#[cfg(test)]
#[path = "regions_count_tests.rs"]
mod count_tests;
