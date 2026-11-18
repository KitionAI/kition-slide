//! Charts, laid out like the reference renderers draw them: chart and plot areas with their fills
//! and borders, title, legend, axes with automatic scales, gridlines, tick marks and tick labels,
//! the series marks (bars, lines, areas, markers, pie slices, radar webs, bubbles, stock bars) and
//! data labels. Manual layouts (`c:manualLayout`) place the plot area, title and legend where the
//! file puts them; everything else is laid out automatically. Sizes are in points.

use kition_slide_color::{ColorScheme, ColorTransform, Rgba, SchemeSlot};
use kition_slide_model::Shape;
use kition_slide_model::chart::{
    Axis, AxisKind, Chart, ChartText, ChartType, DataLabels, ErrorBars, ManualLayout, Series, WaterfallKind, auto_scale, error_amounts,
    format_number, trend_fn,
};
use kition_slide_model::resolve::Ctx;
use kition_slide_model::style::{ColorBase, ColorRef, Fill, Line, PatternFill, Shadow};
use kition_slide_model::text::{Align, Anchor, Bullet, TextBody};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use vello_common::filter_effects::{EdgeMode, Filter, FilterPrimitive};
use vello_cpu::RenderContext;

use crate::color;

#[cfg(test)]
mod axis_direction_tests;
#[cfg(test)]
mod axis_title_tests;
#[cfg(test)]
mod category_count_tests;
#[cfg(test)]
mod line_end_tests;
mod manual_legend;
#[cfg(test)]
mod marker_effect_tests;
mod marker_effects;
#[cfg(test)]
mod percent_error_tests;
#[cfg(test)]
mod pie_label_tests;
mod pie_labels;
#[cfg(test)]
mod radar_grid_tests;
#[cfg(test)]
mod radar_scale_tests;
#[cfg(test)]
mod text_spacing_tests;

const SLOTS: [SchemeSlot; 6] =
    [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];

/// Padding inside the chart area.
const PAD: f64 = 7.5;
/// Gap between the title and what follows it.
const TITLE_GAP: f64 = 8.25;
/// Gap between the title and the plot area when no legend sits between them.
const TITLE_PLOT_GAP: f64 = 15.0;
/// Gap between the legend and the plot area.
const LEGEND_GAP: f64 = 13.5;
/// Gap between tick labels and their axis.
const LABEL_GAP: f64 = 5.0;
/// Room an axis title keeps from the chart's edge, and the room it takes with its gap.
const TITLE_SIDE: f64 = 3.4;
const TITLE_ROOM: f64 = TITLE_SIDE + LABEL_GAP - 1.5;
/// Half the length of an error bar's end cap.
const CAP: f64 = 2.75;
/// Extra room (pt) between a line point (or its marker) and a label beside it.
const LINE_LABEL_GAP: f64 = 4.5;
/// Share of its automatic plot area a pie with automatically placed labels takes.
const PIE_FIT: f64 = 0.875;
/// Data table row height, in text sizes. The reference's rows follow the font's own height
/// (1.61 in Microsoft YaHei, 1.58 in PingFang, at 9, 10 and 14 pt); text here keeps one line
/// height in every face, so the rows take the middle.
const DT_ROW: f64 = 1.6;
/// The data table's heading row is this much taller than a series row, points.
const DT_HEAD_EXTRA: f64 = 1.0;

/// Height of a data table of `series` rows under its heading row, for text of `size`.
fn table_height(size: f64, series: usize) -> f64 {
    (series as f64 + 1.0) * size * DT_ROW + DT_HEAD_EXTRA
}
/// Extra room under the plot area's labels.
const BOTTOM_EXTRA: f64 = 1.5;
/// Room the plot area keeps at a side without tick labels.
const PLOT_SIDE: f64 = 3.4;
/// Major tick mark length.
const TICK: f64 = 4.0;
/// Default colour of chart text (`tx1` at 65%).
const TEXT: Rgba = Rgba::rgb(0x59, 0x59, 0x59);
const GRID: Rgba = Rgba::rgb(0xD9, 0xD9, 0xD9);

/// Colours resolved through the chart's own colour scheme when a theme override gives it one.
struct Pal<'a> {
    rctx: &'a Ctx<'a>,
    scheme: Option<&'a ColorScheme>,
}

impl Pal<'_> {
    fn col(&self, c: &ColorRef) -> Rgba {
        match self.scheme {
            Some(s) => {
                let c = match &c.base {
                    ColorBase::Scheme { slot } => {
                        ColorRef { base: ColorBase::Scheme { slot: self.rctx.master.map_slot(*slot) }, mods: c.mods.clone() }
                    }
                    _ => c.clone(),
                };
                c.resolve(s, None)
            }
            None => self.rctx.color(c, None),
        }
    }
    /// The colour a fill shows (its first stop for gradients); `None` for no fill.
    fn fill_rgba(&self, f: &Fill) -> Option<Rgba> {
        match f {
            Fill::Solid { color: c } => Some(self.col(c)),
            Fill::Gradient(g) => g.stops.first().map(|s| self.col(&s.color)),
            Fill::Pattern(p) => Some(self.col(&p.fg)),
            Fill::Picture(p) => p.adjust.duotone.as_ref().map(|(d, l)| self.col(d).lerp(self.col(l), 0.3)),
            _ => None,
        }
    }
    fn line_rgba(&self, l: &Line) -> Option<Rgba> {
        l.fill.as_ref().and_then(|f| self.fill_rgba(f))
    }
    /// The fill with every colour resolved, so the shared painter draws the chart's scheme.
    fn bake(&self, f: &Fill) -> Fill {
        let rgb = |c: &ColorRef| ColorRef::rgb(self.col(c));
        match f {
            Fill::Solid { color: c } => Fill::Solid { color: rgb(c) },
            Fill::Gradient(g) => {
                let mut g = g.clone();
                for s in &mut g.stops {
                    s.color = rgb(&s.color);
                }
                Fill::Gradient(g)
            }
            Fill::Pattern(p) => Fill::Pattern(PatternFill { preset: p.preset.clone(), fg: rgb(&p.fg), bg: rgb(&p.bg) }),
            // Pictures recoloured as a duotone show their colours' blend (the picture painter
            // draws no duotone).
            Fill::Picture(p) => {
                let mut p = p.clone();
                if let Some((dark, light)) = &p.adjust.duotone {
                    p.adjust.duotone = Some((rgb(dark), rgb(light)));
                }
                Fill::Picture(p)
            }
            other => other.clone(),
        }
    }
}

/// Everything a chart's parts are drawn with.
struct Env<'a> {
    rctx: &'a Ctx<'a>,
    c: &'a Chart,
    m: Affine,
    pal: Pal<'a>,
    /// Default text of the chart.
    base: Txt,
    size: (f64, f64),
}

#[derive(Clone, Debug)]
struct Txt {
    font: Option<String>,
    font_ea: Option<String>,
    font_cs: Option<String>,
    size: f64,
    spacing: Option<f64>,
    bold: bool,
    italic: bool,
    color: Rgba,
    rot: Option<f64>,
}

impl Env<'_> {
    /// Text properties of an element over the chart's defaults.
    fn txt(&self, t: &ChartText, size: f64, bold: bool) -> Txt {
        let d = &self.c.fmt.text;
        Txt {
            font: t.font.as_ref().or(d.font.as_ref()).cloned(),
            font_ea: t.font_ea.as_ref().or(d.font_ea.as_ref()).cloned(),
            font_cs: t.font_cs.as_ref().or(d.font_cs.as_ref()).cloned(),
            size: t.size.unwrap_or(size),
            spacing: t.spacing.or(d.spacing).filter(|s| s.is_finite()).map(|s| s.clamp(-4000.0, 4000.0)),
            bold: t.bold.or(d.bold).unwrap_or(bold),
            italic: t.italic.or(d.italic).unwrap_or(false),
            color: t.color.as_ref().or(d.color.as_ref()).map(|c| self.pal.col(c)).unwrap_or(self.base.color),
            rot: t.rot,
        }
    }
}

/// Colour of series/point `i`: accents, then darker and lighter rounds.
fn auto_color(pal: &Pal, c: &Chart, i: usize) -> Rgba {
    // Colorful palettes 1–4 start at accents 1, 2, 3 and 4 and go round the six accents;
    // monochrome palettes 1–6 shade one accent (7–13 shade accents 1–6 and then dark 1).
    let number = |prefix: &str| c.palette.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).max(1);
    let mono = c.palette.starts_with("monochrome");
    let first = if mono { (number("monochrome") - 1) % 7 } else { (number("colorful") - 1) % 4 };
    let slot = SLOTS.get((first + i) % 6).copied().unwrap_or(SchemeSlot::Accent1);
    let mono_slot = SLOTS.get(first).copied().unwrap_or(SchemeSlot::Dk1);
    let base = pal.col(&ColorRef::scheme(if mono { mono_slot } else { slot }));
    let round = if mono { i } else { i / 6 };
    let t = |m: &[ColorTransform]| kition_slide_color::apply(base, m);
    match round {
        0 => base,
        1 => t(&[ColorTransform::LumMod(60000)]),
        2 => t(&[ColorTransform::LumMod(80000), ColorTransform::LumOff(20000)]),
        3 => t(&[ColorTransform::LumMod(80000)]),
        4 => t(&[ColorTransform::LumMod(60000), ColorTransform::LumOff(40000)]),
        5 => t(&[ColorTransform::LumMod(50000)]),
        n => t(&[ColorTransform::LumMod(100000 - (n as i32 % 5) * 12000)]),
    }
}

/// The automatic colour index of series `i` (`c:idx` when the file gives it).
fn series_index(s: &Series, i: usize) -> usize {
    s.fmt.idx.map(|v| v as usize).unwrap_or(i)
}

/// The series' main colour: its line for line-like series, else its fill, else automatic.
fn series_rgba(e: &Env, i: usize, s: &Series, line_kind: bool) -> Rgba {
    let auto = || auto_color(&e.pal, e.c, series_index(s, i));
    if line_kind {
        return s.line.as_ref().and_then(|l| e.pal.line_rgba(l)).unwrap_or_else(auto);
    }
    match s.fill.as_ref() {
        Some(Fill::None) => s.line.as_ref().and_then(|l| e.pal.line_rgba(l)).unwrap_or_else(auto),
        Some(f) => e.pal.fill_rgba(f).unwrap_or_else(auto),
        None => auto(),
    }
}

fn is_line_kind(k: ChartType) -> bool {
    matches!(k, ChartType::Line | ChartType::LineMarkers | ChartType::StackedLine | ChartType::Scatter | ChartType::Radar | ChartType::Stock)
}

/// Conventional bar plots get their category slots from the value arrays. An
/// extra cached category label does not add a point, but explicit blank values
/// still occupy their original slots. Other chart types keep their existing extent.
fn category_count(c: &Chart) -> usize {
    let values = c.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let conventional_bar = |kind| {
        matches!(
            kind,
            ChartType::Column | ChartType::StackedColumn | ChartType::PercentColumn | ChartType::Bar | ChartType::StackedBar | ChartType::PercentBar
        )
    };
    if values > 0 && conventional_bar(c.kind) && c.series.iter().all(|s| conventional_bar(s.kind.unwrap_or(c.kind))) {
        values
    } else {
        c.categories.len().max(values)
    }
}

fn visible(l: Option<&Line>) -> bool {
    l.is_some_and(|l| l.fill.as_ref().is_some_and(|f| !f.is_none()))
}

// ---------------------------------------------------------------------------------------------
// Text

fn body(text: &str, t: &Txt, align: Align, wrap: bool) -> TextBody {
    let mut body = TextBody::from_text(text);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    body.body.inset_t = Some(0.0);
    body.body.inset_b = Some(0.0);
    body.body.anchor = Some(Anchor::Middle);
    body.body.wrap = Some(wrap);
    if let Some(r) = t.rot.filter(|r| r.abs() > 0.01) {
        body.body.rot = Some(r);
    }
    for p in &mut body.paragraphs {
        p.props.align = Some(align);
        p.props.bullet = Some(Bullet::None);
        p.props.margin_left = Some(0.0);
        p.props.indent = Some(0.0);
        for run in &mut p.runs {
            run.props.font = t.font.clone();
            run.props.font_ea = t.font_ea.clone();
            run.props.font_cs = t.font_cs.clone();
            run.props.size = Some(t.size);
            run.props.spacing = t.spacing;
            run.props.bold = Some(t.bold);
            run.props.italic = Some(t.italic);
            run.props.fill = Some(Fill::solid(ColorRef::rgb(t.color)));
        }
    }
    body
}

/// Size of `text` laid out unrotated (wrapped at `max_w` when given).
fn measure(e: &Env, text: &str, t: &Txt, max_w: Option<f64>) -> (f64, f64) {
    if text.is_empty() {
        return (0.0, 0.0);
    }
    let mut t2 = t.clone();
    t2.rot = None;
    let b = body(text, &t2, Align::Left, max_w.is_some());
    let tmp = Shape { text: Some(b.clone()), ..Default::default() };
    let w = max_w.unwrap_or(100_000.0).max(1.0);
    let l = kition_slide_text::layout(
        e.rctx,
        &tmp,
        &b,
        &kition_slide_text::Opts {
            rect: Rect::new(0.0, 0.0, w, 100_000.0),
            fields: &kition_slide_text::NoFields,
            prompt_color: None,
            no_shrink: true,
        },
    );
    (l.content_width, l.content_height)
}

/// Size of a label's bounding box once rotated.
fn rotated_size(size: (f64, f64), rot: Option<f64>) -> (f64, f64) {
    match rot {
        Some(r) if r.abs() > 0.01 => {
            let a = r.to_radians();
            let (s, c) = (a.sin().abs(), a.cos().abs());
            (size.0 * c + size.1 * s, size.0 * s + size.1 * c)
        }
        _ => size,
    }
}

/// Draws `text` in `r` (aligned within it), rotated about its centre by the text's rotation.
fn draw_text(ctx: &mut RenderContext, e: &Env, text: &str, t: &Txt, r: Rect, align: Align, wrap: bool) {
    if text.is_empty() || !(r.is_finite() && r.width() >= 0.0) {
        return;
    }
    let b = body(text, t, align, wrap);
    let tmp = Shape { text: Some(b.clone()), ..Default::default() };
    crate::draw_text(ctx, e.rctx, &tmp, &b, r, e.m, &kition_slide_text::NoFields, None);
}

/// Draws a one-line label of (unrotated) size `size` centred at `p`.
fn draw_at(ctx: &mut RenderContext, e: &Env, text: &str, t: &Txt, p: Point, size: (f64, f64)) {
    let r = Rect::new(p.x - size.0 / 2.0 - 1.0, p.y - size.1 / 2.0, p.x + size.0 / 2.0 + 1.0, p.y + size.1 / 2.0);
    draw_text(ctx, e, text, t, r, Align::Center, false);
}

// ---------------------------------------------------------------------------------------------
// Painting

fn fill_rect(ctx: &mut RenderContext, e: &Env, f: &Fill, r: Rect) {
    fill_path(ctx, e, f, &r.to_path(0.1), r);
}

fn fill_path(ctx: &mut RenderContext, e: &Env, f: &Fill, p: &BezPath, bounds: Rect) {
    if matches!(f, Fill::None | Fill::Group | Fill::Background) {
        return;
    }
    let baked = e.pal.bake(f);
    crate::paint::fill_path(ctx, e.rctx, &baked, None, p, bounds, e.m, kition_slide_geom::preset::FillMode::Norm, 1.0, e.rctx.pres);
}

fn solid(ctx: &mut RenderContext, e: &Env, c: Rgba, p: &BezPath) {
    ctx.set_transform(e.m);
    ctx.set_paint(color(c, 1.0));
    ctx.fill_path(p);
}

/// Strokes `p` with line `l` (its colour, width, dash, cap and join), else with `fallback` colour
/// and `width`. A line without fill (`a:noFill`) draws nothing.
fn stroke(ctx: &mut RenderContext, e: &Env, l: Option<&Line>, fallback: Option<Rgba>, width: f64, p: &BezPath) {
    let col = match l.and_then(|l| l.fill.as_ref()) {
        Some(Fill::None) => return,
        Some(f) => e.pal.fill_rgba(f).or(fallback),
        None => fallback,
    };
    let Some(col) = col else { return };
    let w = l.and_then(|l| l.width).unwrap_or(width);
    if w.is_nan() || w <= 0.0 {
        return;
    }
    use kition_slide_model::style::{LineCap, LineJoin};
    let cap = match l.and_then(|l| l.cap) {
        Some(LineCap::Round) => kurbo::Cap::Round,
        Some(LineCap::Square) => kurbo::Cap::Square,
        _ => kurbo::Cap::Butt,
    };
    let join = match l.and_then(|l| l.join) {
        Some(LineJoin::Miter) => kurbo::Join::Miter,
        Some(LineJoin::Bevel) => kurbo::Join::Bevel,
        _ => kurbo::Join::Round,
    };
    let min_w = 0.5 / crate::scale_of(e.m).max(1e-6);
    let dash: Vec<f64> = l.and_then(|l| l.dash.as_ref()).map(|d| d.pattern().iter().map(|v| v * w.max(0.75)).collect()).unwrap_or_default();
    // Straight horizontal and vertical lines land on whole device pixels, as the reference
    // renders draw them (crisp, never smeared over two pixel rows).
    let authored_width = w;
    let (path, m, w, dash, snapped) = match snap_lines(e.m, p, w) {
        Some((dp, wpx, k)) => (dp, Affine::IDENTITY, wpx, dash.iter().map(|v| v * k).collect::<Vec<f64>>(), true),
        None => (p.clone(), e.m, w.max(min_w), dash, false),
    };
    let mut sk = kurbo::Stroke::new(w).with_caps(cap).with_join(join);
    if !dash.is_empty() && dash.iter().all(|v| *v > 0.0) {
        sk = sk.with_dashes(0.0, dash);
    }
    // Only supported closed asymmetric compounds use this paint path. Other strokes
    // retain their existing geometry, gradient fallback and coordinate frame.
    if let Some(mut outline) = crate::compound::closed_outline(&path, &sk, l.and_then(|l| l.compound)) {
        ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
        if let Some(fill @ (Fill::Gradient(_) | Fill::Pattern(_))) = l.and_then(|l| l.fill.as_ref()) {
            let bounds = if snapped {
                // snap_lines accepts positive finite axis scales, so this inverse is valid.
                // Restore chart coordinates while keeping the unsnapped authored paint frame.
                outline = e.m.inverse() * outline;
                p.bounding_box().inflate(authored_width / 2.0, authored_width / 2.0)
            } else {
                path.bounding_box().inflate(w / 2.0, w / 2.0)
            };
            fill_path(ctx, e, fill, &outline, bounds);
        } else {
            ctx.set_transform(m);
            ctx.set_paint(color(col, 1.0));
            ctx.fill_path(&outline);
        }
        return;
    }
    // A compound line (`cmpd`): parallel bands of the line's width, filled even-odd from the
    // outlines of nested strokes (whole width, then each inner edge).
    let bands = l.and_then(|l| l.compound).and_then(compound_widths).filter(|_| sk.dash_pattern.is_empty());
    let compound_outline = |bands: &[f64]| {
        let mut outline = BezPath::new();
        for f in bands {
            let s = kurbo::Stroke::new(w * f).with_caps(sk.start_cap).with_join(sk.join);
            outline.extend(crate::stroke::outline(&path, &s, 0.05));
        }
        outline
    };
    // Gradient and pattern lines: fill the stroke's outline with the line's fill.
    if let Some(f @ (Fill::Gradient(_) | Fill::Pattern(_))) = l.and_then(|l| l.fill.as_ref())
        && m == e.m
    {
        let b = path.bounding_box().inflate(w / 2.0, w / 2.0);
        if let Some(bands) = bands {
            ctx.set_fill_rule(vello_cpu::peniko::Fill::EvenOdd);
            fill_path(ctx, e, f, &compound_outline(bands), b);
            ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
        } else {
            fill_path(ctx, e, f, &crate::stroke::outline(&path, &sk, 0.05), b);
        }
        if let Some(l) = l {
            line_ends(ctx, e, l, col, l.width.unwrap_or(width), p);
        }
        return;
    }
    ctx.set_transform(m);
    ctx.set_paint(color(col, 1.0));
    if let Some(bands) = bands {
        ctx.set_fill_rule(vello_cpu::peniko::Fill::EvenOdd);
        ctx.fill_path(&compound_outline(bands));
        ctx.set_fill_rule(vello_cpu::peniko::Fill::NonZero);
    } else {
        ctx.set_stroke(sk);
        crate::stroke::paint(ctx, &path);
    }
    if let Some(l) = l {
        line_ends(ctx, e, l, col, l.width.unwrap_or(width), p);
    }
}

/// The nested stroke widths (fractions of the line width) whose outlines, filled even-odd, draw a
/// compound line: double lines are thirds (line, gap, line), triple lines fifths.
fn compound_widths(c: kition_slide_model::style::Compound) -> Option<&'static [f64]> {
    use kition_slide_model::style::Compound;
    match c {
        Compound::Single => None,
        Compound::Double | Compound::ThickThin | Compound::ThinThick => Some(&[1.0, 1.0 / 3.0]),
        Compound::Triple => Some(&[1.0, 0.6, 0.2]),
    }
}

/// Arrowheads and other line ends (`a:headEnd` at the path's start, `a:tailEnd` at its end).
fn line_ends(ctx: &mut RenderContext, e: &Env, l: &Line, col: Rgba, w: f64, p: &BezPath) {
    if l.head.as_ref().is_none_or(|h| h.kind == "none") && l.tail.as_ref().is_none_or(|t| t.kind == "none") {
        return;
    }
    // Closed outlines have no line ends. Keep each open subpath independent so
    // gaps in a series do not attach arrows across disconnected runs.
    let mut remaining = p.elements();
    while !remaining.is_empty() {
        let end =
            remaining.iter().enumerate().skip(1).find_map(|(i, el)| matches!(el, kurbo::PathEl::MoveTo(_)).then_some(i)).unwrap_or(remaining.len());
        let (subpath, rest) = remaining.split_at(end);
        remaining = rest;
        if !subpath.iter().any(|el| matches!(el, kurbo::PathEl::ClosePath)) {
            open_line_ends(ctx, e, l, col, w, subpath);
        }
    }
}

fn open_line_ends(ctx: &mut RenderContext, e: &Env, l: &Line, col: Rgba, w: f64, elements: &[kurbo::PathEl]) {
    let pts: Vec<Point> = elements
        .iter()
        .filter_map(|el| match el {
            kurbo::PathEl::MoveTo(q) | kurbo::PathEl::LineTo(q) => Some(*q),
            kurbo::PathEl::CurveTo(_, _, q) | kurbo::PathEl::QuadTo(_, q) => Some(*q),
            kurbo::PathEl::ClosePath => None,
        })
        .collect();
    let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
        return;
    };
    let next = pts.iter().copied().find(|q| (*q - first).hypot() > 1e-6);
    let prev = pts.iter().rev().copied().find(|q| (*q - last).hypot() > 1e-6);
    let w = w.max(0.75);
    for (end, tip, from) in [(&l.head, first, next), (&l.tail, last, prev)] {
        let (Some(end), Some(from)) = (end, from) else { continue };
        if end.kind == "none" {
            continue;
        }
        let d = (tip - from) / (tip - from).hypot();
        let n = kurbo::Vec2::new(-d.y, d.x);
        let (width, len) = crate::line_end::dimensions(end, w);
        let hw = width / 2.0;
        let mut path = BezPath::new();
        match end.kind.as_str() {
            "oval" => path = kurbo::Ellipse::new(tip, (len / 2.0, hw), d.atan2()).to_path(0.05),
            "diamond" => {
                path.move_to(tip + d * len / 2.0);
                path.line_to(tip + n * hw);
                path.line_to(tip - d * len / 2.0);
                path.line_to(tip - n * hw);
                path.close_path();
            }
            "stealth" => {
                path.move_to(tip);
                path.line_to(tip - d * len + n * hw);
                path.line_to(tip - d * len * 0.6);
                path.line_to(tip - d * len - n * hw);
                path.close_path();
            }
            "arrow" => {
                let mut v = BezPath::new();
                v.move_to(tip - d * len + n * hw);
                v.line_to(tip);
                v.line_to(tip - d * len - n * hw);
                ctx.set_transform(e.m);
                ctx.set_paint(color(col, 1.0));
                ctx.set_stroke(kurbo::Stroke::new(w));
                crate::stroke::paint(ctx, &v);
                continue;
            }
            _ => {
                path.move_to(tip);
                path.line_to(tip - d * len + n * hw);
                path.line_to(tip - d * len - n * hw);
                path.close_path();
            }
        }
        solid(ctx, e, col, &path);
    }
}

/// Axis decorations follow increasing values or category indices. The shaft is
/// already built in increasing screen coordinates; swap decorations when that
/// direction opposes the axis order, without reversing the shaft or its dashes.
fn axis_line(axis: &Axis, along_x: bool) -> Option<std::borrow::Cow<'_, Line>> {
    let line = axis.line.as_ref()?;
    if along_x != axis.reversed
        || (line.head.as_ref().is_none_or(|end| end.kind == "none") && line.tail.as_ref().is_none_or(|end| end.kind == "none"))
    {
        return Some(std::borrow::Cow::Borrowed(line));
    }
    let mut line = line.clone();
    std::mem::swap(&mut line.head, &mut line.tail);
    Some(std::borrow::Cow::Owned(line))
}

/// The device scale of `m` when it only scales and translates (no rotation, skew or flip).
fn axis_scale(m: Affine) -> Option<(f64, f64)> {
    let [a, b, c, d, _, _] = m.as_coeffs();
    (b.abs() < 1e-9 && c.abs() < 1e-9 && a > 1e-9 && d > 1e-9).then_some((a, d))
}

/// `p` (only horizontal and vertical segments) in device space with every coordinate on a pixel
/// centre (odd widths) or edge (even widths), the width rounded to whole pixels (at least one),
/// and the device scale; `None` for any other path or transform.
fn snap_lines(m: Affine, p: &BezPath, w: f64) -> Option<(BezPath, f64, f64)> {
    let (sx, sy) = axis_scale(m)?;
    let k = (sx + sy) / 2.0;
    let wpx = (w * k).round().max(1.0);
    if wpx > 6.0 {
        return None;
    }
    let odd = wpx as i64 % 2 == 1;
    let snap = |v: f64| if odd { (v + 1e-6).floor() + 0.5 } else { v.round() };
    let mut out = BezPath::new();
    let mut last: Option<Point> = None;
    let mut start: Option<Point> = None;
    for el in p.elements() {
        match *el {
            kurbo::PathEl::MoveTo(q) => {
                let q = m * q;
                let q = Point::new(snap(q.x), snap(q.y));
                out.move_to(q);
                last = Some(q);
                start = Some(q);
            }
            kurbo::PathEl::LineTo(q) => {
                let q = m * q;
                let q = Point::new(snap(q.x), snap(q.y));
                let l = last?;
                if (q.x - l.x).abs() > 0.01 && (q.y - l.y).abs() > 0.01 {
                    return None;
                }
                out.line_to(q);
                last = Some(q);
            }
            kurbo::PathEl::ClosePath => {
                if let (Some(l), Some(s0)) = (last, start)
                    && (s0.x - l.x).abs() > 0.01
                    && (s0.y - l.y).abs() > 0.01
                {
                    return None;
                }
                out.close_path();
            }
            _ => return None,
        }
    }
    Some((out, wpx, k))
}

/// `r` with its edges on whole device pixels (bars and boxes are drawn crisp).
fn snap_rect(m: Affine, r: Rect) -> Rect {
    let Some(_) = axis_scale(m) else { return r };
    let a = m * Point::new(r.x0, r.y0);
    let b = m * Point::new(r.x1, r.y1);
    let inv = m.inverse();
    let a = inv * Point::new(a.x.round(), a.y.round());
    let b = inv * Point::new(b.x.round(), b.y.round());
    Rect::new(a.x, a.y, b.x, b.y)
}

/// An outer shadow of `path` (filled, or stroked `stroke_w` wide), under what is drawn next.
fn shadow(ctx: &mut RenderContext, e: &Env, sh: Option<&Shadow>, path: &BezPath, stroke_w: Option<f64>) {
    let Some(sh) = sh else { return };
    let c = e.pal.col(&sh.color);
    if c.a == 0 || path.elements().is_empty() {
        return;
    }
    let b = path.bounding_box();
    let anchor = |a: &str| -> Point {
        let x = if a.ends_with('l') {
            b.x0
        } else if a.ends_with('r') {
            b.x1
        } else {
            b.center().x
        };
        let y = if a.starts_with('t') {
            b.y0
        } else if a.starts_with('b') {
            b.y1
        } else {
            b.center().y
        };
        Point::new(x, y)
    };
    let o = anchor(if sh.align.is_empty() { "b" } else { &sh.align }).to_vec2();
    let d = kurbo::Vec2::new(sh.dir.to_radians().cos(), sh.dir.to_radians().sin()) * sh.dist;
    let local =
        Affine::translate(o) * Affine::new([sh.sx, sh.ky.to_radians().tan(), sh.kx.to_radians().tan(), sh.sy, 0.0, 0.0]) * Affine::translate(-o);
    let sm = e.m * Affine::translate(d) * local;
    let k = crate::scale_of(e.m);
    // vello keeps filtered layers' tiles in u16: never blur anything far outside the canvas.
    let db = sm.transform_rect_bbox(b).inflate(sh.blur * k * 2.0, sh.blur * k * 2.0);
    if !(db.is_finite() && db.x0 > -30_000.0 && db.y0 > -30_000.0 && db.x1 < 30_000.0 && db.y1 < 30_000.0) {
        return;
    }
    let sigma = (sh.blur * 0.5 * k).clamp(0.0, 500.0) as f32;
    ctx.set_transform(Affine::IDENTITY);
    let filter = (sigma > 0.01).then(|| Filter::from_primitive(FilterPrimitive::GaussianBlur { std_deviation: sigma, edge_mode: EdgeMode::None }));
    ctx.push_layer(None, None, Some(c.a as f32 / 255.0), None, filter);
    ctx.set_transform(sm);
    ctx.set_paint(color(c.with_alpha(255), 1.0));
    match stroke_w {
        Some(w) => {
            ctx.set_stroke(kurbo::Stroke::new(w).with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round));
            crate::stroke::paint(ctx, path);
        }
        None => ctx.fill_path(path),
    }
    ctx.pop_layer();
}

fn seg(a: Point, b: Point) -> BezPath {
    let mut p = BezPath::new();
    p.move_to(a);
    p.line_to(b);
    p
}

/// A box (chart area, plot area, legend, title, label): fill then border.
fn boxed(ctx: &mut RenderContext, e: &Env, r: Rect, fill: Option<&Fill>, line: Option<&Line>, radius: f64) {
    let path = if radius > 0.0 { kurbo::RoundedRect::from_rect(r, radius).to_path(0.1) } else { r.to_path(0.1) };
    if let Some(f) = fill {
        let sr = if radius > 0.0 { r } else { snap_rect(e.m, r) };
        let fp = if radius > 0.0 { path.clone() } else { sr.to_path(0.1) };
        fill_path(ctx, e, f, &fp, sr);
    }
    if visible(line) {
        stroke(ctx, e, line, None, 0.75, &path);
    }
}

// ---------------------------------------------------------------------------------------------
// Markers

/// The path of marker `symbol` centred at `p` with size `s`, and whether it is only stroked.
fn marker_path(symbol: &str, p: Point, s: f64) -> (BezPath, bool) {
    let h = s / 2.0;
    let mut b = BezPath::new();
    match symbol {
        "square" | "picture" => return (Rect::new(p.x - h, p.y - h, p.x + h, p.y + h).to_path(0.1), false),
        "diamond" => {
            b.move_to((p.x, p.y - h));
            b.line_to((p.x + h, p.y));
            b.line_to((p.x, p.y + h));
            b.line_to((p.x - h, p.y));
            b.close_path();
        }
        "triangle" => {
            b.move_to((p.x, p.y - h));
            b.line_to((p.x + h, p.y + h));
            b.line_to((p.x - h, p.y + h));
            b.close_path();
        }
        "x" | "star" | "plus" => {
            if symbol != "plus" {
                b.move_to((p.x - h, p.y - h));
                b.line_to((p.x + h, p.y + h));
                b.move_to((p.x + h, p.y - h));
                b.line_to((p.x - h, p.y + h));
            }
            if symbol != "x" {
                b.move_to((p.x, p.y - h));
                b.line_to((p.x, p.y + h));
            }
            if symbol == "plus" {
                b.move_to((p.x - h, p.y));
                b.line_to((p.x + h, p.y));
            }
            return (b, true);
        }
        "dash" => return (Rect::new(p.x - h, p.y - s / 10.0, p.x + h, p.y + s / 10.0).to_path(0.1), false),
        "dot" => return (Rect::new(p.x - h / 2.0, p.y - s / 10.0, p.x + h / 2.0, p.y + s / 10.0).to_path(0.1), false),
        _ => {
            // Ellipse emits a full arc without ClosePath. Markers are closed shapes,
            // so joins, compound bands and endpoint suppression need explicit closure.
            b = kurbo::Ellipse::new(p, (h, h), 0.0).to_path(0.1);
            b.close_path();
        }
    }
    (b, false)
}

/// Automatic marker symbols in series order.
const AUTO_MARKERS: [&str; 9] = ["diamond", "square", "triangle", "x", "star", "circle", "plus", "dot", "dash"];

/// Apply a point override without cloning the series' potentially large data arrays.
fn point_marker(series: &Series, index: usize) -> std::borrow::Cow<'_, Series> {
    let Some((_, marker)) = series.fmt.point_markers.iter().find(|(point, _)| *point == index) else {
        return std::borrow::Cow::Borrowed(series);
    };
    let mut effective = Series { marker: marker.symbol.clone().or_else(|| series.marker.clone()), ..Default::default() };
    effective.fmt.idx = series.fmt.idx;
    effective.fmt.marker_size = marker.size.or(series.fmt.marker_size);
    effective.fmt.marker_fill = marker.fill.clone().or_else(|| series.fmt.marker_fill.clone());
    effective.fmt.marker_effects = marker.effects.clone().or_else(|| series.fmt.marker_effects.clone());
    effective.fmt.marker_line = marker
        .line
        .clone()
        .map(|mut line| {
            if let Some(base) = &series.fmt.marker_line {
                line.inherit(base);
            }
            line
        })
        .or_else(|| series.fmt.marker_line.clone());
    std::borrow::Cow::Owned(effective)
}

/// The dimensions of a picture marker before an explicit marker size is applied.
fn picture_marker_size(e: &Env, s: &Series) -> Option<(f64, f64)> {
    let Fill::Picture(fill) = s.fmt.marker_fill.as_ref()? else { return None };
    let media = e.rctx.pres.media(fill.media)?;
    crate::images::natural_size(&media.data).filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0 && *w <= 4096.0 && *h <= 4096.0)
}

fn marker_size(e: &Env, s: &Series, symbol: &str) -> f64 {
    s.fmt
        .marker_size
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= 4096.0)
        .unwrap_or_else(|| if symbol == "picture" { picture_marker_size(e, s).map(|(w, h)| w.max(h)).unwrap_or(5.0) } else { 5.0 })
}

/// The marker a series draws, if any: its symbol and size.
fn series_marker(e: &Env, kind: ChartType, i: usize, s: &Series) -> Option<(String, f64)> {
    let size = marker_size(e, s, s.marker.as_deref().unwrap_or(""));
    if let Some(m) = &s.marker {
        return (m != "none").then(|| (m.clone(), size));
    }
    // No symbol in the file: line-with-markers charts pick the automatic one.
    (kind == ChartType::LineMarkers).then(|| (AUTO_MARKERS[series_index(s, i) % AUTO_MARKERS.len()].to_string(), size))
}

fn marker_geometry(e: &Env, s: &Series, symbol: &str, size: f64, p: Point) -> (BezPath, bool) {
    if symbol == "picture" {
        let (w, h) = picture_marker_size(e, s).map(|(w, h)| (size * w / w.max(h), size * h / w.max(h))).unwrap_or((size, size));
        (Rect::new(p.x - w / 2.0, p.y - h / 2.0, p.x + w / 2.0, p.y + h / 2.0).to_path(0.1), false)
    } else {
        marker_path(symbol, p, size)
    }
}

fn draw_marker(ctx: &mut RenderContext, e: &Env, s: &Series, col: Rgba, symbol: &str, size: f64, p: Point) {
    let (path, stroked) = marker_geometry(e, s, symbol, size, p);
    if !stroked {
        match &s.fmt.marker_fill {
            Some(f) => fill_path(ctx, e, f, &path, path.bounding_box()),
            None => solid(ctx, e, col, &path),
        }
    }
    stroke(ctx, e, s.fmt.marker_line.as_ref(), Some(col), 0.75, &path);
}

// ---------------------------------------------------------------------------------------------
// Data labels

#[cfg(test)]
mod label_separator_tests;

/// The labels a series shows: its own, else the chart group's, else the simple flags.
fn series_labels(c: &Chart, s: &Series) -> Option<DataLabels> {
    if let Some(l) = &s.fmt.labels {
        return l.any().then(|| l.clone());
    }
    if let Some(l) = &c.fmt.labels
        && l.any()
    {
        return Some(l.clone());
    }
    s.data_labels.unwrap_or(c.data_labels).then(|| DataLabels { value: true, ..Default::default() })
}

/// The text of point `i`'s label.
fn label_text(c: &Chart, s: &Series, l: &DataLabels, i: usize, v: f64, pct: Option<f64>) -> String {
    let mut parts: Vec<String> = vec![];
    if l.series {
        parts.push(s.name.clone());
    }
    if l.category {
        parts.push(category_label(c, i));
    }
    if l.value {
        parts.push(format_number(v, l.num_fmt.as_deref().or(s.fmt.num_fmt.as_deref())));
    }
    if l.percent
        && let Some(p) = pct
    {
        parts.push(format_number(p, Some(l.num_fmt.as_deref().filter(|f| f.contains('%')).unwrap_or("0%"))));
    }
    if l.bubble_size
        && let Some(b) = s.sizes.get(i).copied().flatten()
    {
        parts.push(format_number(b, None));
    }
    // DrawingML defaults category-and-percentage pie labels to separate lines.
    let pie_category_percent = s.kind.unwrap_or(c.kind) == ChartType::Pie && l.category && l.percent && !l.series && !l.value && !l.bubble_size;
    let sep = l.separator.as_deref().unwrap_or(if pie_category_percent { "\n" } else { ", " });
    parts.join(sep)
}

fn category_label(c: &Chart, i: usize) -> String {
    let raw = c.categories.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1));
    match (&c.fmt.cat_format, raw.trim().parse::<f64>()) {
        (Some(code), Ok(v)) => format_number(v, Some(code)),
        _ => raw,
    }
}

/// A data label to draw once every mark is down: its text sits against `at`, offset along
/// (`dx`, `dy`) (−1, 0 or 1 on each axis; 0, 0 centres it).
struct LabelReq {
    text: String,
    at: Point,
    dx: f64,
    dy: f64,
    t: Txt,
    fill: Option<Fill>,
    line: Option<Line>,
    layout: Option<ManualLayout>,
}

/// Label factor coordinates offset the automatic box even when an explicit size is present.
/// Edge coordinates use the chart origin. Invalid manual layouts retain automatic placement.
fn data_label_rect(at: Point, direction: (f64, f64), measured: (f64, f64), layout: Option<&ManualLayout>, chart: (f64, f64)) -> Rect {
    let automatic = |bw: f64, bh: f64| {
        let cx = at.x + direction.0 * (bw / 2.0 + 2.0);
        let cy = at.y + direction.1 * (bh / 2.0 + 2.0);
        Rect::new(cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0)
    };
    let valid_coord = |v: Option<f64>, edge: bool| {
        let lower = if edge { 0.0 } else { -1.0 };
        v.is_none_or(|v| v.is_finite() && (lower..=1.0).contains(&v))
    };
    let valid_size = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v));
    let Some(l) = layout.filter(|l| valid_coord(l.x, l.x_edge) && valid_coord(l.y, l.y_edge) && valid_size(l.w) && valid_size(l.h)) else {
        return automatic(measured.0, measured.1);
    };
    let bw = l.w.map_or(measured.0, |w| w * chart.0);
    let bh = l.h.map_or(measured.1, |h| h * chart.1);
    let r = automatic(bw, bh);
    let mut x = l.x.map_or(r.x0, |v| if l.x_edge { v * chart.0 } else { r.x0 + v * chart.0 });
    let mut y = l.y.map_or(r.y0, |v| if l.y_edge { v * chart.1 } else { r.y0 + v * chart.1 });
    // MS-OE376 x/y place a valid manual box at the closest point inside the chart.
    x = x.clamp(0.0, (chart.0 - bw).max(0.0));
    y = y.clamp(0.0, (chart.1 - bh).max(0.0));
    Rect::new(x, y, x + bw, y + bh)
}

fn draw_label(ctx: &mut RenderContext, e: &Env, l: &LabelReq) {
    let (tw, th) = measure(e, &l.text, &l.t, None);
    let framed = l.fill.as_ref().is_some_and(|f| !f.is_none()) || visible(l.line.as_ref());
    let (ix, iy) = if framed { (3.0, 1.5) } else { (1.5, 0.0) };
    let (bw, bh) = (tw + 2.0 * ix, th + 2.0 * iy);
    let r = data_label_rect(l.at, (l.dx, l.dy), (bw, bh), l.layout.as_ref(), e.size);
    if framed {
        boxed(ctx, e, r, l.fill.as_ref(), l.line.as_ref(), 0.0);
    }
    draw_text(ctx, e, &l.text, &l.t, r.inflate(1.0, 0.0), Align::Center, false);
}

fn label_layout_for_draw(l: &DataLabels) -> Option<ManualLayout> {
    let mut layout = l.layout.clone()?;
    if l.pos.as_deref() == Some("bestFit") {
        // Best-fit automatic anchors are not fully modeled yet. Preserve authored
        // positions for round trips, but defer their offsets instead of guessing an anchor.
        layout.x = None;
        layout.y = None;
    }
    (layout.x.is_some() || layout.y.is_some() || layout.w.is_some() || layout.h.is_some()).then_some(layout)
}

fn label_req(e: &Env, l: &DataLabels, text: String, at: Point, dx: f64, dy: f64) -> LabelReq {
    LabelReq { text, at, dx, dy, t: e.txt(&l.text, e.base.size, false), fill: l.fill.clone(), line: l.line.clone(), layout: label_layout_for_draw(l) }
}

// ---------------------------------------------------------------------------------------------
// Entry point

pub fn draw(ctx: &mut RenderContext, rctx: &Ctx, c: &Chart, m: Affine, w: f64, h: f64) {
    if !(w.is_finite() && h.is_finite() && w > 1.0 && h > 1.0) {
        return;
    }
    let pal = Pal { rctx, scheme: c.fmt.scheme.as_ref() };
    let base_col = c.fmt.text.color.as_ref().or(c.text_color.as_ref()).map(|t| pal.col(t)).unwrap_or(TEXT);
    let base = Txt {
        font: c.fmt.text.font.clone(),
        font_ea: c.fmt.text.font_ea.clone(),
        font_cs: c.fmt.text.font_cs.clone(),
        size: c.fmt.text.size.unwrap_or(10.0),
        spacing: c.fmt.text.spacing.filter(|s| s.is_finite()).map(|s| s.clamp(-4000.0, 4000.0)),
        bold: c.fmt.text.bold.unwrap_or(false),
        italic: false,
        color: base_col,
        rot: None,
    };
    let e = Env { rctx, c, m, pal, base, size: (w, h) };
    let f = &c.fmt;
    // Chart area.
    let radius = if f.rounded { (w.min(h) * 0.05).min(10.0) } else { 0.0 };
    // The reference draws a translucent chart border twice over (it shows darker).
    let frame_line = f.chart_line.as_ref().map(|l| match l.fill.as_ref().and_then(|x| e.pal.fill_rgba(x)) {
        Some(col) if col.a < 255 => {
            let a = 255.0 - (255.0 - col.a as f64).powi(2) / 255.0;
            Line { fill: Some(Fill::solid(ColorRef::rgb(col.with_alpha(a.round() as u8)))), ..l.clone() }
        }
        _ => l.clone(),
    });
    if c.chart_fill.as_ref().is_some_and(|f| !f.is_none()) {
        shadow(ctx, &e, f.chart_shadow.as_ref(), &Rect::new(0.0, 0.0, w, h).to_path(0.1), None);
    }
    boxed(ctx, &e, Rect::new(0.0, 0.0, w, h), c.chart_fill.as_ref(), frame_line.as_ref(), radius);

    let mut area = Rect::new(PAD, PAD, w - PAD, h - PAD - BOTTOM_EXTRA);
    // Title.
    let mut title_draw = None;
    let mut title_bottom = None;
    if let Some(t) = c.title.as_ref().filter(|t| !t.trim().is_empty()) {
        let tt = e.txt(&f.title_text, e.base.size * 1.4, false);
        let (tw, th) = rotated_size(measure(&e, t, &tt, Some(w * 0.8)), tt.rot);
        let auto = ((w - tw) / 2.0, PAD);
        let (x, y) = f.title_layout.as_ref().map(|l| place(l, w, h, auto)).unwrap_or(auto);
        if !f.title_overlay {
            area.y0 = area.y0.max(PAD + th + TITLE_GAP);
            title_bottom = Some(PAD + th);
        }
        title_draw = Some((t.clone(), tt, Rect::new(x, y, x + tw, y + th)));
    }
    // Legend.
    let legend = legend_entries(&e);
    let mut legend_draw = None;
    if let Some(pos) = c.legend.as_deref()
        && !legend.is_empty()
    {
        let lt = e.txt(&f.legend_text, e.base.size, false);
        let lay = legend_layout(&e, &legend, &lt, pos, area, w, h);
        if !f.legend_overlay && f.legend_layout.is_none() {
            match pos {
                "t" => area.y0 = lay.rect.y1 + LEGEND_GAP,
                "b" => area.y1 = lay.rect.y0 - bottom_legend_gap(c),
                "l" => area.x0 = lay.rect.x1 + LEGEND_GAP,
                _ => area.x1 = lay.rect.x0 - LEGEND_GAP,
            }
        }
        legend_draw = Some((lay, lt));
    }
    // Without a legend between them, the plot area keeps a wider gap under the title.
    let legend_on_top = c.legend.as_deref() == Some("t") && !legend.is_empty() && !f.legend_overlay && f.legend_layout.is_none();
    if let Some(b) = title_bottom
        && !legend_on_top
    {
        area.y0 = area.y0.max(b + TITLE_PLOT_GAP);
    }
    if area.width() < 4.0 || area.height() < 4.0 {
        area = Rect::new(PAD, PAD, (w - PAD).max(PAD + 4.0), (h - PAD).max(PAD + 4.0));
    }
    // Plot area: the manual layout, else what is left.
    let (outer, inner) = match &f.plot_layout {
        Some(l) => {
            let (x, y) = place(l, w, h, (area.x0, area.y0));
            let r = Rect::new(x, y, x + l.w.map(|v| v * w).unwrap_or(area.width()), y + l.h.map(|v| v * h).unwrap_or(area.height()));
            if l.inner { (None, Some(r)) } else { (Some(r), None) }
        }
        None => (Some(area), None),
    };
    match c.kind {
        ChartType::Pie | ChartType::Doughnut => draw_pie(ctx, &e, outer, inner),
        ChartType::Radar | ChartType::FilledRadar => draw_radar(ctx, &e, outer, inner),
        ChartType::Funnel => draw_funnel(ctx, &e, inner.or(outer).unwrap_or(area)),
        ChartType::Treemap => draw_treemap(ctx, &e, inner.or(outer).unwrap_or(area)),
        _ => draw_xy(ctx, &e, outer, inner, Rect::new(0.0, 0.0, w, h)),
    }
    if let Some((t, tt, r)) = title_draw {
        boxed(ctx, &e, r.inflate(2.0, 1.0), f.title_fill.as_ref(), f.title_line.as_ref(), 0.0);
        let wrap = r.width() >= w * 0.8 - 0.5;
        draw_text(ctx, &e, &t, &tt, r.inflate(1.0, 0.0), Align::Center, wrap);
    }
    if let Some((lay, lt)) = legend_draw {
        draw_legend(ctx, &e, &legend, &lay, &lt);
    }
}

/// The top-left corner a manual layout gives an element (`auto`: its automatic position).
fn place(l: &ManualLayout, w: f64, h: f64, auto: (f64, f64)) -> (f64, f64) {
    let x = match l.x {
        Some(x) if l.x_edge || l.w.is_some() => x * w,
        Some(x) => auto.0 + x * w,
        None => auto.0,
    };
    let y = match l.y {
        Some(y) if l.y_edge || l.h.is_some() => y * h,
        Some(y) => auto.1 + y * h,
        None => auto.1,
    };
    (x, y)
}

// ---------------------------------------------------------------------------------------------
// Legend

#[cfg(test)]
mod legend_tests;

#[derive(Clone, Copy, PartialEq)]
enum Key {
    Box,
    Line,
}

struct Entry {
    text: String,
    key: Key,
    fill: Option<Fill>,
    col: Rgba,
    line: Option<Line>,
    /// Series index, for its marker.
    series: Option<usize>,
}

fn legend_entries(e: &Env) -> Vec<Entry> {
    let c = e.c;
    let by_point = matches!(c.kind, ChartType::Pie | ChartType::Doughnut | ChartType::Treemap)
        || (c.vary_colors && c.series.len() == 1 && !is_line_kind(c.kind) && !matches!(c.kind, ChartType::Waterfall | ChartType::Funnel));
    let mut out = vec![];
    if c.kind == ChartType::Waterfall {
        // A waterfall's legend names its bars: increase, decrease and total.
        for k in WaterfallKind::ALL {
            out.push(Entry { text: k.label().into(), key: Key::Box, fill: None, col: waterfall_color(e, k), line: None, series: None });
        }
        return out;
    }
    if by_point {
        let Some(s) = c.series.first() else { return out };
        let n = if c.series.len() == 1 { category_count(c) } else { c.categories.len().max(s.values.len()) };
        for i in 0..n.min(500) {
            if c.fmt.legend_deleted.contains(&i) {
                continue;
            }
            let fill = s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f.clone()).or_else(|| s.fill.clone().filter(|_| !c.vary_colors));
            let col = fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or_else(|| auto_color(&e.pal, c, i));
            let line = s.fmt.point_lines.iter().find(|(k, _)| *k == i).map(|(_, l)| l.clone()).or_else(|| s.line.clone());
            out.push(Entry { text: category_label(c, i), key: Key::Box, fill, col, line, series: None });
        }
        return out;
    }
    for (i, s) in c.series.iter().enumerate().take(500) {
        if c.fmt.legend_deleted.contains(&i) {
            continue;
        }
        let kind = s.kind.unwrap_or(c.kind);
        let line_kind = is_line_kind(kind);
        let col = series_rgba(e, i, s, line_kind);
        out.push(Entry {
            text: s.name.clone(),
            key: if line_kind { Key::Line } else { Key::Box },
            fill: s.fill.clone(),
            col,
            line: s.line.clone(),
            series: Some(i),
        });
    }
    if legend_bottom_up(c) {
        out.reverse();
    }
    // Trendlines follow the series.
    for (i, s) in c.series.iter().enumerate().take(500) {
        for t in &s.fmt.trendlines {
            let kind = match t.kind.as_str() {
                "exp" => "Expon.".to_string(),
                "log" => "Log.".to_string(),
                "poly" => "Poly.".to_string(),
                "power" => "Power".to_string(),
                "movingAvg" => format!("{} per. Mov. Avg.", t.period.unwrap_or(2)),
                _ => "Linear".to_string(),
            };
            let text = t.name.clone().unwrap_or_else(|| format!("{kind} ({})", s.name));
            let col = series_rgba(e, i, s, is_line_kind(s.kind.unwrap_or(c.kind)));
            out.push(Entry { text, key: Key::Line, fill: None, col, line: t.line.clone(), series: None });
        }
    }
    out
}

/// Clustered horizontal bars follow their top-to-bottom category-axis direction.
/// A right-hand legend for a vertical stack lists its top series first; horizontal
/// stacks keep series order. Mixed groups and reversed value axes keep the existing path.
fn legend_bottom_up(c: &Chart) -> bool {
    let axes = find_axes(c);
    if c.kind.stacked() && c.series.iter().all(|s| s.kind.unwrap_or(c.kind) == c.kind) && axes.val2.is_none() && !axes.val.reversed {
        return !c.kind.horizontal() && c.legend.as_deref() == Some("r");
    }
    c.kind.horizontal() && c.series.iter().all(|s| s.kind.is_none_or(|k| k.horizontal())) && !axes.cat.reversed
}

struct LegendLayout {
    rect: Rect,
    /// Per entry: the key's and the text's rectangles.
    items: Vec<(Rect, Rect)>,
}

fn key_width(k: Key, t: &Txt) -> f64 {
    match k {
        Key::Box => t.size * 0.7,
        Key::Line => t.size * 2.4,
    }
}

fn legend_layout(e: &Env, entries: &[Entry], t: &Txt, pos: &str, area: Rect, w: f64, h: f64) -> LegendLayout {
    let sizes: Vec<(f64, f64)> = entries.iter().map(|en| measure(e, &en.text, t, None)).collect();
    let row_h = sizes.iter().map(|s| s.1).fold(t.size * 1.2, f64::max);
    let key_gap = t.size * 0.35;
    let item_gap = t.size * 0.8;
    let widths: Vec<f64> = entries.iter().zip(&sizes).map(|(en, s)| key_width(en.key, t) + key_gap + s.0).collect();
    let ins = 3.0;
    let manual = e.c.fmt.legend_layout.as_ref();
    if let Some(layout) = manual
        && let (Some(lw), Some(lh)) = (layout.w.map(|v| v * w), layout.h.map(|v| v * h))
        && lw.is_finite()
        && lh.is_finite()
        && lw > 0.0
        && lh > 0.0
    {
        let auto = match pos {
            "t" => ((w - lw) / 2.0, area.y0),
            "b" => ((w - lw) / 2.0, area.y1 - lh),
            "l" => (area.x0, area.y0 + (area.height() - lh) / 2.0),
            "tr" => (area.x1 - lw, area.y0),
            _ => (area.x1 - lw, area.y0 + (area.height() - lh) / 2.0),
        };
        let (x, y) = place(layout, w, h, auto);
        if x.is_finite() && y.is_finite() && (x + lw).is_finite() && (y + lh).is_finite() {
            let rect = Rect::new(x, y, x + lw, y + lh);
            let items = manual_legend::cells(&widths, row_h, item_gap, rect)
                .into_iter()
                .zip(entries)
                .map(|(cell, entry)| {
                    let kw = key_width(entry.key, t);
                    (Rect::new(cell.x0, cell.y0, cell.x0 + kw, cell.y1), Rect::new(cell.x0 + kw + key_gap, cell.y0, cell.x1, cell.y1))
                })
                .collect();
            return LegendLayout { rect, items };
        }
    }
    let mut items = vec![(Rect::ZERO, Rect::ZERO); entries.len()];
    let rect;
    if matches!(pos, "t" | "b") {
        // Rows that fit the chart's width.
        let max_w = manual.and_then(|l| l.w).map(|v| v * w).unwrap_or(w - 2.0 * PAD) - 2.0 * ins;
        let mut rows: Vec<Vec<usize>> = vec![vec![]];
        let mut row_w = 0.0;
        for (i, iw) in widths.iter().enumerate() {
            let empty = rows.last().is_none_or(|r| r.is_empty());
            let add = if empty { *iw } else { item_gap + iw };
            if row_w + add > max_w && !empty {
                rows.push(vec![i]);
                row_w = *iw;
            } else {
                if let Some(r) = rows.last_mut() {
                    r.push(i);
                }
                row_w += add;
            }
        }
        let row_width = |r: &Vec<usize>| r.iter().map(|i| widths[*i]).sum::<f64>() + item_gap * r.len().saturating_sub(1) as f64;
        let total_w = rows.iter().map(row_width).fold(0.0, f64::max);
        let lw = total_w + 2.0 * ins;
        let lh = row_h * rows.len() as f64 + 2.0 * ins;
        let auto = ((w - lw) / 2.0, if pos == "t" { area.y0 } else { area.y1 - lh });
        let (x, y) = manual.map(|l| place(l, w, h, auto)).unwrap_or(auto);
        let lw2 = manual.and_then(|l| l.w).map(|v| v * w).unwrap_or(lw);
        let lh2 = manual.and_then(|l| l.h).map(|v| v * h).unwrap_or(lh);
        rect = Rect::new(x, y, x + lw2, y + lh2);
        let y0 = y + (lh2 - lh) / 2.0 + ins;
        for (ri, r) in rows.iter().enumerate() {
            let mut cx = x + (lw2 - row_width(r)) / 2.0;
            let cy = y0 + row_h * ri as f64;
            for &i in r {
                let kw = key_width(entries[i].key, t);
                items[i] = (Rect::new(cx, cy, cx + kw, cy + row_h), Rect::new(cx + kw + key_gap, cy, cx + widths[i], cy + row_h));
                cx += widths[i] + item_gap;
            }
        }
    } else {
        let kw = entries.iter().map(|en| key_width(en.key, t)).fold(0.0, f64::max);
        let tw = sizes.iter().map(|s| s.0).fold(0.0, f64::max);
        let lw = kw + key_gap + tw + 2.0 * ins;
        let lh = row_h * entries.len() as f64 + 2.0 * ins;
        let auto = match pos {
            "l" => (area.x0, area.y0 + (area.height() - lh) / 2.0),
            "tr" => (area.x1 - lw, area.y0),
            _ => (area.x1 - lw, area.y0 + (area.height() - lh) / 2.0),
        };
        let (x, y) = manual.map(|l| place(l, w, h, auto)).unwrap_or(auto);
        let lw2 = manual.and_then(|l| l.w).map(|v| v * w).unwrap_or(lw);
        let lh2 = manual.and_then(|l| l.h).map(|v| v * h).unwrap_or(lh);
        rect = Rect::new(x, y, x + lw2, y + lh2);
        let y0 = y + (lh2 - lh) / 2.0 + ins;
        for (i, en) in entries.iter().enumerate() {
            let cy = y0 + row_h * i as f64;
            let k = key_width(en.key, t);
            let kx = x + ins + (kw - k) / 2.0;
            let tx = x + ins + kw + key_gap;
            items[i] = (Rect::new(kx, cy, kx + k, cy + row_h), Rect::new(tx, cy, tx + tw + 1.0, cy + row_h));
        }
    }
    LegendLayout { rect, items }
}

fn draw_legend(ctx: &mut RenderContext, e: &Env, entries: &[Entry], lay: &LegendLayout, t: &Txt) {
    let f = &e.c.fmt;
    boxed(ctx, e, lay.rect, f.legend_fill.as_ref(), f.legend_line.as_ref(), 0.0);
    for (en, (kr, tr)) in entries.iter().zip(&lay.items) {
        let cy = kr.center().y;
        match en.key {
            Key::Box => {
                let s = kr.width();
                let r = Rect::new(kr.x0, cy - s / 2.0, kr.x1, cy + s / 2.0);
                match &en.fill {
                    Some(fl) => fill_rect(ctx, e, fl, r),
                    None => solid(ctx, e, en.col, &r.to_path(0.1)),
                }
                if visible(en.line.as_ref()) {
                    stroke(ctx, e, en.line.as_ref(), None, 0.75, &r.to_path(0.1));
                }
            }
            Key::Line => {
                stroke(ctx, e, en.line.as_ref(), Some(en.col), 2.25, &seg(Point::new(kr.x0, cy), Point::new(kr.x1, cy)));
                if let Some(si) = en.series
                    && let Some(s) = e.c.series.get(si)
                    && let Some((sym, _)) = series_marker(e, s.kind.unwrap_or(e.c.kind), si, s)
                {
                    let mc = s.fmt.marker_fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or(en.col);
                    // Keys use the nominal marker size, independent of picture resolution.
                    draw_marker(ctx, e, s, mc, &sym, marker_size(e, s, "").min(t.size), kr.center());
                }
            }
        }
        draw_text(ctx, e, &en.text, t, *tr, Align::Left, false);
    }
}

// ---------------------------------------------------------------------------------------------
// Cartesian charts

/// A value scale.
#[derive(Clone, Copy, Debug)]
struct Scale {
    min: f64,
    max: f64,
    major: f64,
    log: Option<f64>,
    reversed: bool,
}

impl Scale {
    /// 0..1 along the axis.
    fn frac(&self, v: f64) -> f64 {
        let f = match self.log {
            Some(b) => {
                let l = |x: f64| x.max(1e-300).ln() / b.ln();
                (l(v) - l(self.min)) / (l(self.max) - l(self.min)).max(1e-12)
            }
            None => (v - self.min) / (self.max - self.min).max(1e-12),
        };
        if self.reversed { 1.0 - f } else { f }
    }
    fn ticks(&self) -> Vec<f64> {
        let mut out = vec![];
        if let Some(b) = self.log {
            let mut v = self.min;
            while v <= self.max * (1.0 + 1e-9) && out.len() < 200 {
                out.push(v);
                v *= b;
            }
            return out;
        }
        let n = ((self.max - self.min) / self.major + 1e-6).floor().clamp(0.0, 1000.0) as usize;
        for k in 0..=n {
            let v = self.min + k as f64 * self.major;
            out.push(if v.abs() < self.major * 1e-9 { 0.0 } else { v });
        }
        out
    }
}

/// Axes for charts made without them (new charts in the editor).
fn default_axis(c: &Chart, kind: AxisKind, pos: &str, id: u32, cross: u32) -> Axis {
    c.default_axis(kind, pos, id, cross)
}

struct Axes {
    cat: Axis,
    val: Axis,
    val2: Option<Axis>,
}

fn find_axes(c: &Chart) -> Axes {
    let horiz = c.kind.horizontal();
    let xy = matches!(c.kind, ChartType::Scatter | ChartType::Bubble);
    let (cp, vp) = if horiz { ("l", "b") } else { ("b", "l") };
    let get = |id: u32| c.fmt.axis(id).cloned();
    let ids = c.series.first().map(|s| s.fmt.axes.clone()).unwrap_or_default();
    let (mut cat, mut val) = (None, None);
    if let [a, b, ..] = ids.as_slice() {
        match (get(*a), get(*b)) {
            (Some(a), Some(b)) => {
                let a_is_x = if xy { matches!(a.pos.as_str(), "b" | "t") != horiz } else { a.kind != AxisKind::Value || b.kind == AxisKind::Value };
                (cat, val) = if a_is_x { (Some(a), Some(b)) } else { (Some(b), Some(a)) };
            }
            (a, b) => (cat, val) = (a, b),
        }
    }
    let cat = cat.unwrap_or_else(|| default_axis(c, if xy { AxisKind::Value } else { AxisKind::Category }, cp, 1, 2));
    let val = val.unwrap_or_else(|| default_axis(c, AxisKind::Value, vp, 2, 1));
    let mut val2 = None;
    for s in &c.series {
        if s.fmt.axes.len() >= 2 && !s.fmt.axes.contains(&val.id) {
            for id in &s.fmt.axes {
                if let Some(a) = get(*id)
                    && a.kind == AxisKind::Value
                    && !(xy && matches!(a.pos.as_str(), "b" | "t"))
                {
                    val2.get_or_insert(a);
                }
            }
        }
    }
    Axes { cat, val, val2 }
}

fn on_secondary(ax: &Axes, s: &Series) -> bool {
    ax.val2.as_ref().is_some_and(|v| s.fmt.axes.contains(&v.id) && !s.fmt.axes.contains(&ax.val.id))
}

/// Percent-stacked line/area groups (the model keeps them as stacked kinds): their axis is 0–100%.
fn percent_group(c: &Chart) -> bool {
    c.raw.as_deref().is_some_and(|r| r.contains("\"percentStacked\""))
}

fn is_percent(c: &Chart, k: ChartType) -> bool {
    k.percent() || (matches!(k, ChartType::StackedArea | ChartType::StackedLine) && percent_group(c))
}

/// The value range the series on one axis need (sums for stacked groups).
fn value_range(c: &Chart, ax: &Axes, secondary: bool) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mine: Vec<&Series> = c.series.iter().filter(|s| on_secondary(ax, s) == secondary).collect();
    let n = category_count(c);
    let kind_of = |s: &Series| s.kind.unwrap_or(c.kind);
    let mut kinds: Vec<ChartType> = vec![];
    for s in &mine {
        if !kinds.contains(&kind_of(s)) {
            kinds.push(kind_of(s));
        }
    }
    for k in kinds {
        let group: Vec<&&Series> = mine.iter().filter(|s| kind_of(s) == k).collect();
        if is_percent(c, k) {
            lo = lo.min(0.0);
            hi = hi.max(1.0);
            for i in 0..n {
                if group.iter().any(|s| s.values.get(i).copied().flatten().is_some_and(|v| v < 0.0)) {
                    lo = lo.min(-1.0);
                }
            }
            continue;
        }
        if k == ChartType::Waterfall {
            for step in group.iter().flat_map(|s| s.waterfall()).flatten() {
                hi = hi.max(step.from.max(step.to));
                lo = lo.min(step.from.min(step.to));
            }
            continue;
        }
        if k.stacked() {
            for i in 0..n {
                let (mut p, mut m) = (0.0, 0.0);
                for s in &group {
                    let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                    if v >= 0.0 { p += v } else { m += v }
                }
                hi = hi.max(p);
                lo = lo.min(m);
            }
        } else {
            for s in &group {
                for v in s.values.iter().flatten().filter(|v| v.is_finite()) {
                    hi = hi.max(*v);
                    lo = lo.min(*v);
                }
                if let Some((a, b)) = err_extent(c, s, false) {
                    lo = lo.min(a);
                    hi = hi.max(b);
                }
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    (lo, hi)
}

/// How far series `s`'s error bars along x (`x`) or y reach: the auto scale makes room for them.
fn err_extent(c: &Chart, s: &Series, x: bool) -> Option<(f64, f64)> {
    let mut out: Option<(f64, f64)> = None;
    for eb in s.fmt.err_bars.iter().filter(|b| (b.dir.as_deref() == Some("x")) == x) {
        // Statistical extents do not vary by point; compute them once per series.
        let statistics = matches!(eb.val_type.as_str(), "stdErr" | "stdDev").then(|| error_amounts(eb, &err_values(c, s, eb, x), 0, 0.0));
        let n = s.values.len().max(s.x_values.len());
        for i in 0..n {
            let v = if x { Some(x_of(s, i)) } else { s.values.get(i).copied().flatten() };
            let Some(v) = v.filter(|v| v.is_finite()) else { continue };
            let (m, p, centre) = statistics.unwrap_or_else(|| error_amounts(eb, &[], i, v));
            let base = centre.unwrap_or(v);
            let (a, b) = (base - m, base + p);
            if a.is_finite() && b.is_finite() {
                out = Some(out.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
            }
        }
    }
    out
}

fn x_range(c: &Chart) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in &c.series {
        for i in 0..s.values.len().max(s.x_values.len()) {
            let x = x_of(s, i);
            hi = hi.max(x);
            lo = lo.min(x);
        }
        if let Some((a, b)) = err_extent(c, s, true) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    if lo.is_finite() { (lo, hi) } else { (0.0, 1.0) }
}

/// A scatter point's x: its x value, else its 1-based index (text or missing x values).
fn x_of(s: &Series, i: usize) -> f64 {
    if s.x_values.iter().any(|x| x.is_some()) { s.x_values.get(i).copied().flatten().unwrap_or((i + 1) as f64) } else { (i + 1) as f64 }
}

/// The value scale of an axis of length `len` whose widest tick label is `label_w` wide.
fn scale_for(a: &Axis, lo: f64, hi: f64, len: f64, t: &Txt, horizontal: bool, label_w: f64) -> Scale {
    let per = if horizontal { (label_w + t.size * 1.5).max(t.size * 2.0) } else { t.size * 2.2 };
    let steps = (len / per).floor().clamp(1.0, 10.0);
    if let Some(b) = a.log_base {
        let lo = lo.max(1e-300);
        let min = a.min.filter(|v| *v > 0.0).unwrap_or_else(|| b.powf((lo.ln() / b.ln()).floor()));
        let max = a.max.filter(|v| *v > min).unwrap_or_else(|| b.powf((hi.max(min).ln() / b.ln()).ceil()).max(min * b));
        return Scale { min, max, major: b, log: Some(b), reversed: a.reversed };
    }
    let (min, max, major) = auto_scale(lo, hi, a.min, a.max, a.major_unit, steps);
    Scale { min, max, major, log: None, reversed: a.reversed }
}

fn axis_text(e: &Env, a: &Axis) -> Txt {
    e.txt(&a.text, e.c.fmt.text.size.unwrap_or(9.0), false)
}

fn shows_labels(a: &Axis) -> bool {
    !a.deleted && a.label_pos != "none"
}

/// A bottom legend follows axis text when that text occupies the plot's lower edge.
/// Keep the larger plot-to-legend gap when no text separates them.
fn bottom_legend_gap(c: &Chart) -> f64 {
    if matches!(c.kind, ChartType::Pie | ChartType::Doughnut | ChartType::Radar | ChartType::FilledRadar) {
        return LEGEND_GAP;
    }
    let axes = find_axes(c);
    let below = if c.kind.horizontal() { &axes.val } else { &axes.cat };
    if below.title.as_ref().is_some_and(|t| !t.trim().is_empty()) || c.fmt.data_table.is_some() || bottom_axis_labels(c, &axes) {
        TITLE_GAP
    } else {
        LEGEND_GAP
    }
}

/// Whether source properties place the primary horizontal axis labels at the lower edge.
/// Keep the existing gap when an explicit crossing cannot be resolved without the final scale.
fn bottom_axis_labels(c: &Chart, axes: &Axes) -> bool {
    let below = if c.kind.horizontal() { &axes.val } else { &axes.cat };
    if !shows_labels(below) {
        return false;
    }
    match below.label_pos.as_str() {
        "low" => return true,
        "high" => return false,
        _ => {}
    }
    if c.kind.horizontal() {
        return below.crosses_at.is_none() && (axes.cat.reversed == (below.crosses.as_deref() == Some("max")));
    }
    let vertical = &axes.val;
    let reversed = vertical.reversed;
    if let Some(at) = below.crosses_at {
        let edge = if reversed { vertical.max } else { vertical.min };
        return edge.is_some_and(|edge| at == edge);
    }
    match below.crosses.as_deref() {
        Some("min") => !reversed,
        Some("max") => reversed,
        _ => {
            if vertical.log_base.is_some() {
                return !reversed;
            }
            let (lo, hi) = value_range(c, axes, false);
            if reversed { vertical.max.unwrap_or(hi.max(0.0)) <= 0.0 } else { vertical.min.unwrap_or(lo.min(0.0)) >= 0.0 }
        }
    }
}

fn value_labels(a: &Axis, s: &Scale, percent: bool) -> Vec<(f64, String)> {
    let code = a.num_fmt.as_deref().filter(|f| *f != "General").or(if percent { Some("0%") } else { None });
    s.ticks().into_iter().map(|v| (v, format_number(v, code))).collect()
}

fn widest(e: &Env, labels: &[(f64, String)], t: &Txt) -> (f64, f64) {
    labels.iter().map(|(_, s)| measure(e, s, t, None)).fold((0.0, 0.0), |a, b| (a.0.max(b.0), a.1.max(b.1)))
}

/// The axis title's text, its text and its size once drawn (vertical axes turn it).
fn axis_title(e: &Env, a: &Axis, vertical: bool) -> Option<(String, Txt, (f64, f64))> {
    // An authored title remains a separate chart element when the axis itself is hidden.
    let t = a.title.as_ref().filter(|t| !t.trim().is_empty())?;
    let mut tt = e.txt(&a.title_text, e.c.fmt.text.size.unwrap_or(10.0), false);
    if vertical && tt.rot.is_none() {
        tt.rot = Some(-90.0);
    }
    let s = measure(e, t, &tt, None);
    Some((t.clone(), tt.clone(), rotated_size(s, tt.rot)))
}

fn tick_out(a: &Axis) -> f64 {
    if matches!(a.major_tick.as_str(), "out" | "cross") { TICK } else { 0.0 }
}

fn draw_xy(ctx: &mut RenderContext, e: &Env, outer: Option<Rect>, inner: Option<Rect>, chart_bounds: Rect) {
    let c = e.c;
    let ax = find_axes(c);
    let horiz = c.kind.horizontal();
    let xy = matches!(c.kind, ChartType::Scatter | ChartType::Bubble);
    let ncat = category_count(c).max(1);
    let cat_t = axis_text(e, &ax.cat);
    let val_t = axis_text(e, &ax.val);
    let val2_t = ax.val2.as_ref().map(|a| axis_text(e, a)).unwrap_or_else(|| val_t.clone());
    let (lo, hi) = value_range(c, &ax, false);
    let (lo2, hi2) = if ax.val2.is_some() { value_range(c, &ax, true) } else { (0.0, 1.0) };
    let percent = is_percent(c, c.kind);
    let (xlo, xhi) = if xy { x_range(c) } else { (0.0, 1.0) };
    // A category chart's data table shares its columns with the plotted categories, so
    // each point sits at a cell centre even when the value axis requests mid-category crossing.
    let between = (c.fmt.data_table.is_some() && !horiz && !xy)
        || ax.val.between.unwrap_or(!matches!(c.kind, ChartType::Area | ChartType::StackedArea) || c.fmt.axes.is_empty());
    let cat_labels: Vec<String> = (0..ncat).map(|i| category_label(c, i)).collect();
    // A data table under a column or line chart heads its columns with the categories instead
    // of the category axis' labels. A date axis backed only by text falls back to category
    // labels; genuine numeric dates retain their independent axis labels and table header.
    let text_dates = ax.cat.kind == AxisKind::Date
        && c.categories.iter().any(|s| !s.trim().is_empty())
        && !c.categories.iter().any(|s| s.trim().parse::<f64>().is_ok_and(f64::is_finite));
    let table_heads = c.fmt.data_table.is_some() && !horiz && !xy && (ax.cat.kind == AxisKind::Category || text_dates);
    let cat_labels_shown = shows_labels(&ax.cat) && !table_heads;
    let (below_axis, below_t) = if horiz { (&ax.val, &val_t) } else { (&ax.cat, &cat_t) };
    let table_label_h = if c.fmt.data_table.is_some() && shows_labels(below_axis) && !table_heads {
        measure(e, "0", below_t, None).1 + LABEL_GAP + tick_out(below_axis)
    } else {
        0.0
    };
    let table_h = c.fmt.data_table.as_ref().map(|dt| table_height(e.txt(&dt.text, cat_t.size, false).size, c.series.len()));

    // The scales for a plot rectangle.
    let compute = |r: Rect| -> (Scale, Option<Scale>, Option<Scale>) {
        let vlen = if horiz { r.width() } else { r.height() };
        let probe = |a: &Axis, lo: f64, hi: f64, t: &Txt, p: bool| {
            // 100% charts run from 0 (or −100%) to 100% unless the file fixes the ends.
            let mut pa;
            let a = if p {
                pa = a.clone();
                pa.min = pa.min.or(Some(lo.max(-1.0)));
                pa.max = pa.max.or(Some(1.0));
                &pa
            } else {
                a
            };
            let s0 = scale_for(a, lo, hi, vlen, t, horiz, t.size * 2.0);
            scale_for(a, lo, hi, vlen, t, horiz, widest(e, &value_labels(a, &s0, p), t).0)
        };
        let vs = probe(&ax.val, lo, hi, &val_t, percent);
        let vs2 = ax.val2.as_ref().map(|a| probe(a, lo2, hi2, &val2_t, false));
        let xs = xy.then(|| {
            let s0 = scale_for(&ax.cat, xlo, xhi, r.width(), &cat_t, true, cat_t.size * 2.0);
            scale_for(&ax.cat, xlo, xhi, r.width(), &cat_t, true, widest(e, &value_labels(&ax.cat, &s0, false), &cat_t).0)
        });
        (vs, vs2, xs)
    };
    // Lay out: the inner plot rectangle leaves room for tick labels and axis titles.
    let base = inner.or(outer).unwrap_or(Rect::ZERO);
    let mut r = base;
    if inner.is_some()
        && let Some(dt) = &c.fmt.data_table
    {
        let t = e.txt(&dt.text, cat_t.size, false);
        let foot = table_label_h + 1.0 + table_h.unwrap_or(0.0);
        r = contain_table_plot(r, chart_bounds, table_head_width(e, dt, &t), foot);
    }
    let mut scales = compute(r);
    if inner.is_none() {
        for _ in 0..2 {
            let mut nr = base;
            let left_labels = if horiz { shows_labels(&ax.cat) } else { shows_labels(&ax.val) };
            if !left_labels {
                nr.x0 += PLOT_SIDE;
            }
            if !ax.val2.as_ref().is_some_and(shows_labels) {
                nr.x1 -= PLOT_SIDE;
            }
            if shows_labels(&ax.val) {
                let labels = value_labels(&ax.val, &scales.0, percent);
                let (vw, vh) = widest(e, &labels, &val_t);
                if horiz {
                    nr.y1 -= vh + LABEL_GAP + tick_out(&ax.val);
                    // The last label is centred on the plot's end.
                    let last = labels.last().map(|(_, s)| measure(e, s, &val_t, None).0).unwrap_or(0.0);
                    nr.x1 = nr.x1.min(base.x1 - last / 2.0);
                } else {
                    nr.x0 += vw + LABEL_GAP + tick_out(&ax.val);
                }
            }
            // Category labels on the tick marks (areas) overhang the plot's ends by half.
            if !between && !horiz && !xy && cat_labels_shown {
                let w = |i: usize| cat_labels.get(i).map(|s| measure(e, s, &cat_t, None).0).unwrap_or(0.0);
                nr.x1 = nr.x1.min(base.x1 - w(ncat - 1) / 2.0);
                nr.x0 = nr.x0.max(base.x0 + w(0) / 2.0);
            }
            if xy && shows_labels(&ax.cat) {
                let labels = value_labels(&ax.cat, &scales.2.unwrap_or(scales.0), false);
                let last = labels.last().map(|(_, s)| measure(e, s, &cat_t, None).0).unwrap_or(0.0);
                nr.x1 = nr.x1.min(base.x1 - last / 2.0);
            }
            if let (Some(a2), Some(s2)) = (&ax.val2, &scales.1)
                && shows_labels(a2)
            {
                nr.x1 -= widest(e, &value_labels(a2, s2, false), &val2_t).0 + LABEL_GAP + tick_out(a2);
            }
            if cat_labels_shown {
                if xy {
                    let (_, h) = widest(e, &value_labels(&ax.cat, &scales.2.unwrap_or(scales.0), false), &cat_t);
                    nr.y1 -= h + LABEL_GAP + tick_out(&ax.cat);
                } else if horiz {
                    let w = cat_labels.iter().map(|s| measure(e, s, &cat_t, None).0).fold(0.0, f64::max);
                    nr.x0 += w.min(base.width() * 0.4) + LABEL_GAP + tick_out(&ax.cat);
                } else {
                    let (h, _) = cat_label_block(e, &cat_labels, &cat_t, nr.width() / ncat as f64, &ax.cat);
                    nr.y1 -= h + LABEL_GAP + tick_out(&ax.cat);
                }
            }
            // The left axis title's room: a data table's first column starts after it.
            let mut left_title = 0.0;
            if let Some((_, _, s)) = axis_title(e, &ax.val, !horiz) {
                if horiz {
                    nr.y1 -= s.1 + TITLE_ROOM
                } else {
                    nr.x0 += s.0 + TITLE_ROOM;
                    left_title = s.0 + TITLE_ROOM;
                }
            }
            if let Some((_, _, s)) = axis_title(e, &ax.cat, horiz) {
                if horiz {
                    nr.x0 += s.0 + TITLE_ROOM;
                    left_title = s.0 + TITLE_ROOM;
                } else {
                    nr.y1 -= s.1 + TITLE_ROOM
                }
            }
            if let Some(a2) = &ax.val2
                && let Some((_, _, s)) = axis_title(e, a2, true)
            {
                nr.x1 -= s.0 + TITLE_ROOM;
            }
            if let Some(dt) = &c.fmt.data_table {
                let t = e.txt(&dt.text, cat_t.size, false);
                nr.y1 -= table_height(t.size, c.series.len());
                // Keep the existing cell padding, expanding only when a drawn key and
                // its own series name need more space than that minimum reservation.
                let minimum =
                    c.series.iter().map(|s| measure(e, &s.name, &t, None).0).fold(0.0, f64::max) + 6.0 + if dt.keys { t.size * 1.2 } else { 0.0 };
                let head = table_head_width(e, dt, &t).max(minimum);
                nr.x0 = nr.x0.max(base.x0 + left_title + head);
            }
            if nr.width() < 4.0 || nr.height() < 4.0 {
                nr = base;
            }
            r = nr;
            scales = compute(r);
        }
    }
    if !(r.is_finite() && r.width() > 0.5 && r.height() > 0.5) {
        return;
    }
    let (vs, vs2, xs) = scales;
    if c.plot_fill.as_ref().is_some_and(|f| !f.is_none()) {
        shadow(ctx, e, c.fmt.plot_shadow.as_ref(), &r.to_path(0.1), None);
    }
    boxed(ctx, e, r, c.plot_fill.as_ref(), None, 0.0);
    let vpos = |s: &Scale, v: f64| if horiz { r.x0 + s.frac(v) * r.width() } else { r.y1 - s.frac(v) * r.height() };
    // Category positions: left to right; for horizontal bars the first category at the bottom.
    let rev = ax.cat.reversed != horiz;
    let band = if horiz { r.height() } else { r.width() } / if between { ncat as f64 } else { (ncat as f64 - 1.0).max(1.0) };
    let cat_pos = |i: f64| -> f64 {
        let off = if between { band * (i + 0.5) } else { band * i };
        match (horiz, rev) {
            (true, true) => r.y1 - off,
            (true, false) => r.y0 + off,
            (false, true) => r.x1 - off,
            (false, false) => r.x0 + off,
        }
    };
    let xsc = xs.unwrap_or(vs);
    let xpos = |v: f64| r.x0 + xsc.frac(v) * r.width();
    let cat_ticks: Vec<f64> = if xy {
        xsc.ticks().into_iter().map(xpos).collect()
    } else {
        let n = if between { ncat } else { ncat - 1 };
        (0..=n).map(|k| cat_pos(k as f64 - if between { 0.5 } else { 0.0 })).collect()
    };
    // Gridlines.
    let grid = |ctx: &mut RenderContext, l: &Line, across_val: bool, positions: &[f64]| {
        for p in positions {
            let path =
                if across_val != horiz { seg(Point::new(*p, r.y0), Point::new(*p, r.y1)) } else { seg(Point::new(r.x0, *p), Point::new(r.x1, *p)) };
            stroke(ctx, e, Some(l), Some(GRID), 0.75, &path);
        }
    };
    if let Some(g) = &ax.val.minor_grid {
        let minor = ax.val.minor_unit.unwrap_or(vs.major / 5.0).max((vs.max - vs.min) / 500.0);
        let n = ((vs.max - vs.min) / minor).round().clamp(0.0, 500.0) as usize;
        let pos: Vec<f64> = (0..=n).map(|k| vpos(&vs, vs.min + k as f64 * minor)).collect();
        grid(ctx, g, false, &pos);
    }
    let val_ticks = vs.ticks();
    if let Some(g) = &ax.val.major_grid {
        let pos: Vec<f64> = val_ticks.iter().map(|v| vpos(&vs, *v)).collect();
        grid(ctx, g, false, &pos);
    }
    if let Some(g) = &ax.cat.major_grid {
        grid(ctx, g, true, &cat_ticks);
    }
    // The plot area's border lies over its gridlines, as in the reference.
    boxed(ctx, e, r, None, c.fmt.plot_line.as_ref(), 0.0);
    // Series.
    let mut labels: Vec<LabelReq> = vec![];
    if xy {
        draw_scatter(ctx, e, r, &|v| vpos(&vs, v), &xpos, &mut labels);
    } else {
        let g = Geo { r, horiz, cat_pos: &cat_pos, band, rev };
        draw_series(ctx, e, &g, &ax, &vs, vs2.as_ref(), &mut labels);
    }
    // Where each axis line runs.
    let cross = |s: &Scale, a: &Axis| -> f64 {
        let v = match (a.crosses.as_deref(), a.crosses_at) {
            (_, Some(v)) => v,
            (Some("max"), _) => s.max,
            (Some("min"), _) => s.min,
            _ => 0.0f64.clamp(s.min.min(s.max), s.max.max(s.min)),
        };
        if s.log.is_some() && v <= 0.0 { s.min } else { v }
    };
    let cat_line_at = vpos(&vs, cross(&vs, &ax.cat));
    let val_line_at = if xy {
        xpos(cross(&xsc, &ax.val))
    } else {
        let at_max = ax.val.crosses.as_deref() == Some("max");
        match (horiz, rev != at_max) {
            (true, true) => r.y1,
            (true, false) => r.y0,
            (false, true) => r.x1,
            (false, false) => r.x0,
        }
    };
    let fallback = |a: &Axis| {
        if c.fmt.axes.is_empty() && a.kind != AxisKind::Value { Some(Rgba::rgb(0xBF, 0xBF, 0xBF)) } else { None }
    };
    let line_across = |at: f64, along_x: bool| {
        if along_x { seg(Point::new(r.x0, at), Point::new(r.x1, at)) } else { seg(Point::new(at, r.y0), Point::new(at, r.y1)) }
    };
    if !ax.cat.deleted {
        stroke(ctx, e, axis_line(&ax.cat, !horiz).as_deref(), fallback(&ax.cat), 0.75, &line_across(cat_line_at, !horiz));
    }
    if !ax.val.deleted {
        stroke(ctx, e, axis_line(&ax.val, horiz).as_deref(), None, 0.75, &line_across(val_line_at, horiz));
    }
    if let Some(a2) = ax.val2.as_ref().filter(|a| !a.deleted) {
        stroke(ctx, e, axis_line(a2, false).as_deref(), None, 0.75, &seg(Point::new(r.x1, r.y0), Point::new(r.x1, r.y1)));
    }
    // Tick marks.
    let tick_len = |a: &Axis| -> (f64, f64) {
        match a.major_tick.as_str() {
            "out" => (0.0, TICK),
            "in" => (TICK, 0.0),
            "cross" => (TICK, TICK),
            _ => (0.0, 0.0),
        }
    };
    if !ax.val.deleted {
        let (i, o) = tick_len(&ax.val);
        if i + o > 0.0 {
            for v in &val_ticks {
                let p = vpos(&vs, *v);
                let path = if horiz {
                    seg(Point::new(p, val_line_at - i), Point::new(p, val_line_at + o))
                } else {
                    seg(Point::new(val_line_at - o, p), Point::new(val_line_at + i, p))
                };
                stroke(ctx, e, ax.val.line.as_ref(), None, 0.75, &path);
            }
        }
    }
    if !ax.cat.deleted {
        let (i, o) = tick_len(&ax.cat);
        if i + o > 0.0 {
            for p in &cat_ticks {
                let path = if horiz {
                    seg(Point::new(cat_line_at - o, *p), Point::new(cat_line_at + i, *p))
                } else {
                    seg(Point::new(*p, cat_line_at - i), Point::new(*p, cat_line_at + o))
                };
                stroke(ctx, e, ax.cat.line.as_ref(), fallback(&ax.cat), 0.75, &path);
            }
        }
    }
    // Tick labels.
    if shows_labels(&ax.val) {
        let (low, high) = (ax.val.label_pos == "low", ax.val.label_pos == "high");
        for (v, s) in value_labels(&ax.val, &vs, percent) {
            let p = vpos(&vs, v);
            let sz = measure(e, &s, &val_t, None);
            if horiz {
                let y = if high {
                    r.y0 - LABEL_GAP - tick_out(&ax.val) - sz.1 / 2.0
                } else {
                    (if low { r.y1 } else { val_line_at }) + LABEL_GAP + tick_out(&ax.val) + sz.1 / 2.0
                };
                draw_at(ctx, e, &s, &val_t, Point::new(p, y), sz);
            } else if high {
                let x0 = r.x1 + LABEL_GAP + tick_out(&ax.val);
                draw_text(ctx, e, &s, &val_t, Rect::new(x0, p - sz.1 / 2.0, x0 + sz.0 + 2.0, p + sz.1 / 2.0), Align::Left, false);
            } else {
                let x1 = (if low { r.x0 } else { val_line_at }) - LABEL_GAP - tick_out(&ax.val);
                draw_text(ctx, e, &s, &val_t, Rect::new(x1 - sz.0 - 2.0, p - sz.1 / 2.0, x1, p + sz.1 / 2.0), Align::Right, false);
            }
        }
    }
    if let (Some(a2), Some(s2)) = (&ax.val2, &vs2)
        && shows_labels(a2)
    {
        for (v, s) in value_labels(a2, s2, false) {
            let p = vpos(s2, v);
            let sz = measure(e, &s, &val2_t, None);
            let x0 = r.x1 + LABEL_GAP + tick_out(a2);
            draw_text(ctx, e, &s, &val2_t, Rect::new(x0, p - sz.1 / 2.0, x0 + sz.0 + 2.0, p + sz.1 / 2.0), Align::Left, false);
        }
    }
    if cat_labels_shown {
        let (low, high) = (ax.cat.label_pos == "low", ax.cat.label_pos == "high");
        if xy {
            for (v, txt) in value_labels(&ax.cat, &xsc, false) {
                let sz = measure(e, &txt, &cat_t, None);
                let y = if high {
                    r.y0 - LABEL_GAP - tick_out(&ax.cat) - sz.1 / 2.0
                } else {
                    (if low { r.y1 } else { cat_line_at }) + LABEL_GAP + tick_out(&ax.cat) + sz.1 / 2.0
                };
                draw_at(ctx, e, &txt, &cat_t, Point::new(xpos(v), y), sz);
            }
        } else if horiz {
            for (i, txt) in cat_labels.iter().enumerate() {
                let p = cat_pos(i as f64);
                let sz = measure(e, txt, &cat_t, None);
                let x1 = (if low { r.x0 } else { cat_line_at }) - LABEL_GAP - tick_out(&ax.cat);
                draw_text(ctx, e, txt, &cat_t, Rect::new(x1 - sz.0 - 2.0, p - sz.1 / 2.0, x1, p + sz.1 / 2.0), Align::Right, false);
            }
        } else {
            let slot = r.width() / ncat as f64;
            let (_, rot) = cat_label_block(e, &cat_labels, &cat_t, slot, &ax.cat);
            let skip = ax.cat.label_skip.unwrap_or(1).max(1) as usize;
            let y0 = (if low { r.y1 } else { cat_line_at }) + LABEL_GAP + tick_out(&ax.cat);
            for (i, txt) in cat_labels.iter().enumerate().step_by(skip) {
                let p = cat_pos(i as f64);
                match rot {
                    Some(a) => {
                        let mut t = cat_t.clone();
                        t.rot = Some(a);
                        let sz = measure(e, txt, &cat_t, None);
                        let (rw, rh) = rotated_size(sz, Some(a));
                        // The turned label ends at its tick.
                        let cx = if a < 0.0 { p - rw / 2.0 + sz.1 / 2.0 } else { p + rw / 2.0 - sz.1 / 2.0 };
                        draw_at(ctx, e, txt, &t, Point::new(cx, y0 + rh / 2.0), sz);
                    }
                    None => {
                        let w = slot * skip as f64;
                        let (_, h) = measure(e, txt, &cat_t, Some(w.max(1.0)));
                        draw_text(ctx, e, txt, &cat_t, Rect::new(p - w / 2.0, y0, p + w / 2.0, y0 + h), Align::Center, true);
                    }
                }
            }
        }
    }
    // Axis titles.
    // An inner manual layout excludes axes and their labels. It does not move automatic
    // axis titles into the plot: their outside edge still belongs to the chart area.
    let outer_r = outer.unwrap_or(chart_bounds);
    let bottom_title_y = |height: f64| {
        let edge = outer_r.y1 - TITLE_SIDE - height / 2.0;
        if inner.is_some()
            && let Some(table_h) = table_h
        {
            // Invert the automatic layout's title reservation so an inner manual plot's
            // title follows its table, while staying within the chart's available edge.
            edge.min(r.y1 + table_label_h + table_h + height / 2.0 + TITLE_ROOM - TITLE_SIDE)
        } else {
            edge
        }
    };
    if let Some((t, tt, s)) = axis_title(e, &ax.val, !horiz) {
        let p = if horiz { Point::new(r.center().x, bottom_title_y(s.1)) } else { Point::new(outer_r.x0 + TITLE_SIDE + s.0 / 2.0, r.center().y) };
        draw_at(ctx, e, &t, &tt, p, measure(e, &t, &tt, None));
    }
    if let Some((t, tt, s)) = axis_title(e, &ax.cat, horiz) {
        let p = if horiz { Point::new(outer_r.x0 + TITLE_SIDE + s.0 / 2.0, r.center().y) } else { Point::new(r.center().x, bottom_title_y(s.1)) };
        draw_at(ctx, e, &t, &tt, p, measure(e, &t, &tt, None));
    }
    if let Some(a2) = &ax.val2
        && let Some((t, tt, s)) = axis_title(e, a2, true)
    {
        draw_at(ctx, e, &t, &tt, Point::new(outer_r.x1 - TITLE_SIDE - s.0 / 2.0, r.center().y), measure(e, &t, &tt, None));
    }
    if let Some(dt) = &c.fmt.data_table {
        // Under the plot: tick labels, the table, then the axis title.
        let y0 = r.y1 + table_label_h + 1.0;
        draw_data_table(ctx, e, dt, r, y0, &cat_labels, &cat_t);
    }
    for l in &labels {
        draw_label(ctx, e, l);
    }
}

/// Height of the category labels under a column chart, and their rotation when they don't fit
/// side by side (labels with spaces or CJK text wrap instead).
fn cat_label_block(e: &Env, labels: &[String], t: &Txt, slot: f64, a: &Axis) -> (f64, Option<f64>) {
    if let Some(r) = t.rot {
        let h = labels.iter().map(|s| rotated_size(measure(e, s, t, None), Some(r)).1).fold(0.0, f64::max);
        return (h, Some(r));
    }
    let skip = a.label_skip.unwrap_or(1).max(1) as f64;
    let room = (slot * skip).max(1.0);
    let sizes: Vec<(f64, f64)> = labels.iter().map(|s| measure(e, s, t, None)).collect();
    let wraps = |s: &String| s.contains(' ') || s.chars().any(|c| c as u32 >= 0x2E80);
    if sizes.iter().all(|s| s.0 <= room * 0.98) || labels.iter().all(wraps) {
        let h = labels.iter().map(|s| measure(e, s, t, Some(room)).1).fold(t.size * 1.2, f64::max);
        return (h, None);
    }
    (sizes.iter().map(|s| rotated_size(*s, Some(-45.0)).1).fold(0.0, f64::max), Some(-45.0))
}

/// Category geometry of a plot.
struct Geo<'a> {
    r: Rect,
    horiz: bool,
    cat_pos: &'a dyn Fn(f64) -> f64,
    band: f64,
    /// Categories run against the drawing direction (right to left, or bottom to top).
    rev: bool,
}

/// Bars, lines, areas and stock series against categories.
fn draw_series(ctx: &mut RenderContext, e: &Env, g: &Geo, ax: &Axes, vs: &Scale, vs2: Option<&Scale>, labels: &mut Vec<LabelReq>) {
    let c = e.c;
    let (r, horiz, band) = (g.r, g.horiz, g.band);
    let ncat = category_count(c).max(1);
    let kind_of = |s: &Series| s.kind.unwrap_or(c.kind);
    let vpos = |s: &Scale, v: f64| if horiz { r.x0 + s.frac(v) * r.width() } else { r.y1 - s.frac(v) * r.height() };
    let mk = |along: f64, val: f64| if horiz { Point::new(val, along) } else { Point::new(along, val) };
    // Groups in drawing order: areas, bars, then lines.
    let order = |k: ChartType| match k {
        ChartType::Area | ChartType::StackedArea => 0,
        k if k.is_bar_like() => 1,
        _ => 2,
    };
    let mut groups: Vec<(ChartType, bool)> = vec![];
    for s in &c.series {
        let key = (kind_of(s), on_secondary(ax, s));
        if !groups.contains(&key) {
            groups.push(key);
        }
    }
    groups.sort_by_key(|k| order(k.0));
    for (kind, sec) in groups {
        let members: Vec<(usize, &Series)> = c.series.iter().enumerate().filter(|(_, s)| kind_of(s) == kind && on_secondary(ax, s) == sec).collect();
        let sc = if sec { *vs2.unwrap_or(vs) } else { *vs };
        let base_v = if sc.log.is_some() { sc.min } else { 0.0f64.clamp(sc.min.min(sc.max), sc.max.max(sc.min)) };
        let stacked = kind.stacked();
        let percent = is_percent(c, kind);
        let totals: Vec<f64> =
            (0..ncat).map(|i| members.iter().map(|(_, s)| s.values.get(i).copied().flatten().unwrap_or(0.0).abs()).sum::<f64>().max(1e-12)).collect();
        let total = |i: usize| totals.get(i).copied().unwrap_or(1.0);
        let value = |s: &Series, i: usize| -> Option<f64> {
            let v = s.values.get(i).copied().flatten().or(if c.fmt.blanks_as_zero { Some(0.0) } else { None });
            v.map(|v| if percent { v / total(i) } else { v })
        };
        if kind.is_bar_like() {
            let n = members.len() as f64;
            let gap = c.gap_width.clamp(0.0, 5.0);
            let overlap = if stacked { 1.0 } else { c.overlap.clamp(-1.0, 1.0) };
            let bw = band / (n - (n - 1.0) * overlap + gap).max(0.1);
            let step = bw * (1.0 - overlap);
            let dir = if g.rev { -1.0 } else { 1.0 };
            let mut pos_acc = vec![0.0; ncat];
            let mut neg_acc = vec![0.0; ncat];
            // Where each bar ends (series, point, across, value), for error bars.
            let mut bar_ends: Vec<(usize, usize, f64, f64)> = vec![];
            for (k, (si, s)) in members.iter().enumerate() {
                let col = series_rgba(e, *si, s, false);
                let lbl = series_labels(c, s);
                let k = if stacked { 0.0 } else { k as f64 };
                let steps = if kind == ChartType::Waterfall { s.waterfall() } else { vec![] };
                for i in 0..ncat {
                    let Some(v) = value(s, i) else { continue };
                    let wf = steps.get(i).copied().flatten();
                    let (a, b) = if let Some(step) = wf {
                        (step.from, step.to)
                    } else if stacked {
                        let acc = if v >= 0.0 { &mut pos_acc } else { &mut neg_acc };
                        let a = acc.get(i).copied().unwrap_or(0.0);
                        if let Some(x) = acc.get_mut(i) {
                            *x += v;
                        }
                        (a, a + v)
                    } else {
                        (base_v, v)
                    };
                    let start = -band / 2.0 + gap * bw / 2.0 + k * step;
                    let centre = (g.cat_pos)(i as f64);
                    let (p0, p1) = (centre + dir * start, centre + dir * (start + bw));
                    let (v0, v1) = (vpos(&sc, a), vpos(&sc, b));
                    let rect = snap_rect(
                        e.m,
                        if horiz { Rect::new(v0, p0, v1, p1) } else { Rect::new(p0, v0, p1, v1) }.abs().intersect(r.inflate(0.5, 0.5)),
                    );
                    let fill = s.point_fills.iter().find(|(p, _)| *p == i).map(|(_, f)| f).or(s.fill.as_ref());
                    if !fill.is_some_and(|f| f.is_none()) {
                        shadow(ctx, e, point_shadow(s, i), &rect.to_path(0.1), None);
                    }
                    match (fill, wf) {
                        (Some(f), _) => fill_rect(ctx, e, f, rect),
                        (None, Some(step)) => solid(ctx, e, waterfall_color(e, step.kind), &rect.to_path(0.1)),
                        (None, None) => {
                            solid(ctx, e, if c.vary_colors && c.series.len() == 1 { auto_color(&e.pal, c, i) } else { col }, &rect.to_path(0.1))
                        }
                    }
                    let line = s.fmt.point_lines.iter().find(|(p, _)| *p == i).map(|(_, l)| l).or(s.line.as_ref());
                    if visible(line) {
                        stroke(ctx, e, line, None, 0.75, &rect.to_path(0.1));
                    }
                    bar_ends.push((*si, i, (p0 + p1) / 2.0, b));
                    if let Some(l) = lbl.as_ref().and_then(|l| l.point(i)) {
                        let raw = s.values.get(i).copied().flatten().unwrap_or(0.0);
                        let text = label_text(c, s, l, i, raw, Some(raw / total(i)));
                        // +1: the bar grows towards larger screen x (horizontal) or smaller y.
                        let grows = if horiz { v1 >= v0 } else { v1 <= v0 };
                        let sign = if grows { 1.0 } else { -1.0 };
                        // Outside the bar's end the reference leaves 3.5 pt more than inside labels.
                        let (at, d) = match l.pos.as_deref().unwrap_or(if stacked { "ctr" } else { "outEnd" }) {
                            "inEnd" => (v1, -sign),
                            "ctr" => ((v0 + v1) / 2.0, 0.0),
                            "inBase" => (v0, sign),
                            _ => (v1 + if horiz { 3.5 } else { -3.5 } * sign, sign),
                        };
                        let mid = (p0 + p1) / 2.0;
                        labels.push(if horiz { label_req(e, l, text, mk(mid, at), d, 0.0) } else { label_req(e, l, text, mk(mid, at), 0.0, -d) });
                    }
                }
            }
            for (si, s) in &members {
                // Percentage standard error uses normalized plotted coordinates, including
                // positive/negative stacks. Missing bars do not enter the statistic.
                let plotted: Vec<_> = bar_ends.iter().filter(|(j, _, _, _)| j == si).map(|(_, _, _, b)| Some(*b)).collect();
                for &(_, i, along, b) in bar_ends.iter().filter(|(j, _, _, _)| j == si) {
                    for eb in s.fmt.err_bars.iter().filter(|x| x.dir.as_deref() != Some("x")) {
                        let values = category_err_values(c, s, eb, percent, &plotted);
                        let (m, pl, centre) = error_amounts(eb, &values, i, b);
                        let base = centre.unwrap_or(b);
                        err_bar(ctx, e, eb, mk(along, vpos(&sc, base)), mk(along, vpos(&sc, base - m)), mk(along, vpos(&sc, base + pl)));
                    }
                }
            }
            for (si, s) in &members {
                let col = series_rgba(e, *si, s, false);
                trendlines(ctx, e, s, col, &|x| (g.cat_pos)(x - 1.0), &|y| vpos(&sc, y), horiz, ncat);
            }
            continue;
        }
        // Lines, areas and stock series.
        let area = matches!(kind, ChartType::Area | ChartType::StackedArea);
        let mut acc = vec![0.0; ncat];
        let mut prev_top: Option<Vec<Point>> = None;
        let mut areas: Vec<(usize, BezPath, Rgba)> = vec![];
        let mut stock_pts: Vec<Vec<Option<Point>>> = vec![];
        let mut marks: Vec<(usize, Rgba, Vec<Option<Point>>, Vec<Option<f64>>)> = vec![];
        for (si, s) in &members {
            let col = series_rgba(e, *si, s, !area);
            let mut pts: Vec<Option<Point>> = vec![];
            let mut below: Vec<f64> = vec![];
            let mut plotted: Vec<Option<f64>> = vec![];
            for i in 0..ncat {
                let base_i = if stacked { acc.get(i).copied().unwrap_or(0.0) } else { base_v };
                below.push(base_i);
                let v = match (stacked, value(s, i)) {
                    (true, Some(v)) => {
                        if let Some(x) = acc.get_mut(i) {
                            *x += v;
                        }
                        Some(base_i + v)
                    }
                    (true, None) => area.then_some(base_i),
                    (false, v) => v,
                };
                plotted.push(v);
                pts.push(v.map(|v| mk((g.cat_pos)(i as f64), vpos(&sc, v))));
            }
            if area {
                let top: Vec<Point> =
                    pts.iter().enumerate().map(|(i, p)| p.unwrap_or_else(|| mk((g.cat_pos)(i as f64), vpos(&sc, base_v)))).collect();
                let mut a = BezPath::new();
                for (k, p) in top.iter().enumerate() {
                    if k == 0 { a.move_to(*p) } else { a.line_to(*p) }
                }
                match (&prev_top, stacked) {
                    (Some(prev), true) => {
                        for p in prev.iter().rev() {
                            a.line_to(*p);
                        }
                    }
                    _ => {
                        if let (Some(l), Some(f)) = (top.last(), top.first()) {
                            let b = vpos(&sc, base_v);
                            if horiz {
                                a.line_to((b, l.y));
                                a.line_to((b, f.y));
                            } else {
                                a.line_to((l.x, b));
                                a.line_to((f.x, b));
                            }
                        }
                    }
                }
                a.close_path();
                areas.push((*si, a, col));
                if stacked {
                    prev_top = Some(top);
                }
            } else {
                stock_pts.push(pts.clone());
            }
            marks.push((*si, col, pts.clone(), plotted.clone()));
            if let Some(sl) = series_labels(c, s) {
                for (i, p) in pts.iter().enumerate() {
                    let Some(p) = p else { continue };
                    let Some(l) = sl.point(i).filter(|_| s.values.get(i).copied().flatten().is_some()) else {
                        continue;
                    };
                    let raw = s.values.get(i).copied().flatten().unwrap_or(0.0);
                    let text = label_text(c, s, l, i, raw, Some(raw / total(i)));
                    let (dx, dy) = match l.pos.as_deref().unwrap_or(if area { "ctr" } else { "r" }) {
                        "t" => (0.0, -1.0),
                        "b" => (0.0, 1.0),
                        "l" => (-1.0, 0.0),
                        "ctr" => (0.0, 0.0),
                        _ => (1.0, 0.0),
                    };
                    let at = if area && dx == 0.0 && dy == 0.0 {
                        let q = vpos(&sc, below.get(i).copied().unwrap_or(base_v));
                        if horiz { Point::new((p.x + q) / 2.0, p.y) } else { Point::new(p.x, (p.y + q) / 2.0) }
                    } else if !area {
                        // The reference keeps a line's labels clear of its marker, and a little
                        // further out than other labels.
                        let marker = point_marker(s, i);
                        let push =
                            series_marker(e, kind, *si, &marker).filter(|(m, _)| m != "none").map_or(0.0, |(_, size)| size / 2.0) + LINE_LABEL_GAP;
                        Point::new(p.x + dx * push, p.y + dy * push)
                    } else {
                        *p
                    };
                    labels.push(label_req(e, l, text, at, dx, dy));
                }
            }
        }
        // Stacked areas: later series are drawn over earlier ones.
        for (si, a, col) in &areas {
            let s = &c.series[*si];
            shadow(ctx, e, s.fmt.shadow.as_ref(), a, None);
            match &s.fill {
                Some(f) => fill_path(ctx, e, f, a, a.bounding_box()),
                None => solid(ctx, e, *col, a),
            }
            if visible(s.line.as_ref()) {
                stroke(ctx, e, s.line.as_ref(), None, 0.75, a);
            }
        }
        if !area && kind == ChartType::Stock {
            draw_stock(ctx, e, &stock_pts, band, horiz);
        }
        if let Some(dl) = &c.fmt.drop_lines {
            for (_, _, pts, _) in &marks {
                for p in pts.iter().flatten() {
                    let foot = if horiz { Point::new(r.x0, p.y) } else { Point::new(p.x, r.y1) };
                    stroke(ctx, e, Some(dl), Some(Rgba::BLACK), 0.75, &seg(*p, foot));
                }
            }
        }
        for (si, col, pts, plotted) in &marks {
            let s = &c.series[*si];
            for (i, p) in pts.iter().enumerate() {
                let Some(p) = p else { continue };
                let marker = point_marker(s, i);
                if let Some((sym, size)) = series_marker(e, kind, *si, &marker) {
                    marker_effects::below(ctx, e, &marker, *col, &sym, size, *p);
                }
            }
            if !area && (kind != ChartType::Stock || visible(s.line.as_ref())) {
                stroke_polyline(ctx, e, s, *col, pts, s.smooth);
            }
            for eb in s.fmt.err_bars.iter().filter(|b| b.dir.as_deref() != Some("x")) {
                for (i, (p, v)) in pts.iter().zip(plotted).enumerate() {
                    let (Some(p), Some(v)) = (p, v) else { continue };
                    let (m, pl, centre) = error_amounts(eb, &category_err_values(c, s, eb, percent, plotted), i, *v);
                    let base = centre.unwrap_or(*v);
                    let along = if horiz { p.y } else { p.x };
                    err_bar(ctx, e, eb, mk(along, vpos(&sc, base)), mk(along, vpos(&sc, base - m)), mk(along, vpos(&sc, base + pl)));
                }
            }
            trendlines(ctx, e, s, *col, &|x| (g.cat_pos)(x - 1.0), &|y| vpos(&sc, y), horiz, ncat);
            for (i, p) in pts.iter().enumerate() {
                let Some(p) = p else { continue };
                let marker = point_marker(s, i);
                if let Some((sym, size)) = series_marker(e, kind, *si, &marker) {
                    draw_marker(ctx, e, &marker, *col, &sym, size, *p);
                }
            }
        }
        // A line chart's high-low lines and up/down bars go over its lines and markers, as the
        // reference draws them.
        if !area && kind != ChartType::Stock {
            draw_stock(ctx, e, &stock_pts, band, horiz);
        }
    }
}

/// Every value of every series (or every x value): standard deviation error bars are computed
/// over all of them, as spreadsheets do.
fn all_values(c: &Chart, x: bool) -> Vec<Option<f64>> {
    c.series.iter().flat_map(|s| if x { s.x_values.iter() } else { s.values.iter() }).copied().collect()
}

/// The values error bar `eb` of series `s` computes its statistics over: the series' own for
/// standard error, every series' otherwise.
fn err_values(c: &Chart, s: &Series, eb: &ErrorBars, x: bool) -> Vec<Option<f64>> {
    if eb.val_type == "stdErr" { if x { s.x_values.clone() } else { s.values.clone() } } else { all_values(c, x) }
}

/// Percentage standard error uses normalized plotted coordinates instead of raw data units.
/// Ordinary stacks and other error modes retain their existing source-value statistics.
fn category_err_values(c: &Chart, s: &Series, eb: &ErrorBars, percent: bool, plotted: &[Option<f64>]) -> Vec<Option<f64>> {
    if percent && eb.val_type == "stdErr" { plotted.to_vec() } else { err_values(c, s, eb, false) }
}

/// The shadow of point `i` of a series: its own, else the series'.
fn point_shadow(s: &Series, i: usize) -> Option<&Shadow> {
    s.fmt.point_shadows.iter().find(|(k, _)| *k == i).map(|(_, sh)| sh).or(s.fmt.shadow.as_ref())
}

/// An error bar at data point `at` reaching `lo` and `hi` (screen points), with end caps.
fn err_bar(ctx: &mut RenderContext, e: &Env, eb: &ErrorBars, at: Point, lo: Point, hi: Point) {
    let line = eb.line.clone();
    let fallback = Some(TEXT);
    for end in [lo, hi] {
        if (end - at).hypot() < 0.01 {
            continue;
        }
        stroke(ctx, e, line.as_ref(), fallback, 0.75, &seg(at, end));
        if !eb.no_end_cap {
            let d = end - at;
            let n = kurbo::Vec2::new(-d.y, d.x) / d.hypot() * CAP;
            let cap = BezPath::from_vec(vec![kurbo::PathEl::MoveTo(end - n), kurbo::PathEl::LineTo(end + n)]);
            let mut plain = line.clone().unwrap_or_default();
            plain.head = None;
            plain.tail = None;
            stroke(ctx, e, line.is_some().then_some(&plain), fallback, 0.75, &cap);
        }
    }
}

/// A series' trendlines over x = 1..=n (category numbers) or its x values, mapped by `xp`/`yp`.
fn trendlines(ctx: &mut RenderContext, e: &Env, s: &Series, col: Rgba, xp: &dyn Fn(f64) -> f64, yp: &dyn Fn(f64) -> f64, horiz: bool, n: usize) {
    if s.fmt.trendlines.is_empty() {
        return;
    }
    let xy = matches!(e.c.kind, ChartType::Scatter | ChartType::Bubble);
    let pts: Vec<(f64, f64)> =
        (0..n).filter_map(|i| s.values.get(i).copied().flatten().map(|v| (if xy { x_of(s, i) } else { (i + 1) as f64 }, v))).collect();
    let x0 = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let x1 = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    if !(x0.is_finite() && x1.is_finite()) {
        return;
    }
    for t in &s.fmt.trendlines {
        let Some(f) = trend_fn(t, &pts) else { continue };
        let (a, b) = (x0 - t.backward, x1 + t.forward);
        let mut path = BezPath::new();
        for k in 0..=64 {
            let x = a + (b - a) * k as f64 / 64.0;
            let y = f(x);
            if !y.is_finite() {
                continue;
            }
            let p = if horiz { Point::new(yp(y), xp(x)) } else { Point::new(xp(x), yp(y)) };
            if path.elements().is_empty() { path.move_to(p) } else { path.line_to(p) }
        }
        stroke(ctx, e, t.line.as_ref(), Some(col), 1.5, &path);
    }
}

/// A series' line through its points (gaps break it), straight or smoothed.
fn stroke_polyline(ctx: &mut RenderContext, e: &Env, s: &Series, col: Rgba, pts: &[Option<Point>], smooth: bool) {
    let mut runs: Vec<Vec<Point>> = vec![vec![]];
    for p in pts {
        match p {
            Some(p) => {
                if let Some(r) = runs.last_mut() {
                    r.push(*p);
                }
            }
            None => runs.push(vec![]),
        }
    }
    let mut path = BezPath::new();
    for run in runs.iter().filter(|r| r.len() >= 2) {
        path.move_to(run[0]);
        if smooth && run.len() > 2 {
            // Catmull-Rom through the points, as cubic Béziers.
            for k in 0..run.len() - 1 {
                let p0 = run[k.saturating_sub(1)];
                let (p1, p2) = (run[k], run[k + 1]);
                let p3 = run[(k + 2).min(run.len() - 1)];
                let t = 1.0 / 6.0;
                let c1 = Point::new(p1.x + (p2.x - p0.x) * t, p1.y + (p2.y - p0.y) * t);
                let c2 = Point::new(p2.x - (p3.x - p1.x) * t, p2.y - (p3.y - p1.y) * t);
                path.curve_to(c1, c2, p2);
            }
        } else {
            for p in &run[1..] {
                path.line_to(*p);
            }
        }
    }
    if path.elements().is_empty() {
        return;
    }
    if !s.line.as_ref().is_some_and(|l| l.fill.as_ref().is_some_and(|f| f.is_none())) {
        shadow(ctx, e, s.fmt.shadow.as_ref(), &path, Some(s.line.as_ref().and_then(|l| l.width).unwrap_or(2.25)));
    }
    stroke(ctx, e, s.line.as_ref(), Some(col), 2.25, &path);
}

/// High-low lines and up/down bars of a stock or line chart group.
fn draw_stock(ctx: &mut RenderContext, e: &Env, series: &[Vec<Option<Point>>], band: f64, horiz: bool) {
    let f = &e.c.fmt;
    let hi_low = f.hi_low_lines.as_ref();
    let n = series.iter().map(|s| s.len()).max().unwrap_or(0);
    let dark = Rgba::rgb(0x40, 0x40, 0x40);
    if let Some(hl) = hi_low {
        for i in 0..n {
            let ps: Vec<Point> = series.iter().filter_map(|s| s.get(i).copied().flatten()).collect();
            let Some(first) = ps.first() else { continue };
            let (a, b) = if horiz {
                let lo = ps.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
                let hi = ps.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
                (Point::new(lo, first.y), Point::new(hi, first.y))
            } else {
                let lo = ps.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
                let hi = ps.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
                (Point::new(first.x, lo), Point::new(first.x, hi))
            };
            stroke(ctx, e, Some(hl), Some(dark), 0.75, &seg(a, b));
        }
    }
    if let Some((gap, up, down)) = &f.up_down_bars
        && series.len() >= 2
        && let (Some(open), Some(close)) = (series.first(), series.last())
    {
        let bw = band / (1.0 + gap);
        for i in 0..n {
            let (Some(o), Some(cl)) = (open.get(i).copied().flatten(), close.get(i).copied().flatten()) else {
                continue;
            };
            let rising = if horiz { cl.x >= o.x } else { cl.y <= o.y };
            let rect =
                if horiz { Rect::new(o.x, o.y - bw / 2.0, cl.x, o.y + bw / 2.0) } else { Rect::new(o.x - bw / 2.0, o.y, o.x + bw / 2.0, cl.y) }.abs();
            let (fill, line) = if rising { (up, &f.up_down_lines.0) } else { (down, &f.up_down_lines.1) };
            match fill {
                Some(fl) => fill_rect(ctx, e, fl, rect),
                None => solid(ctx, e, if rising { Rgba::WHITE } else { dark }, &rect.to_path(0.1)),
            }
            stroke(ctx, e, line.as_ref(), Some(dark), 0.75, &rect.to_path(0.1));
        }
    }
}

fn scatter_marker(e: &Env, si: usize, marker: &Series, lines: bool) -> Option<(String, f64)> {
    series_marker(e, ChartType::Scatter, si, marker).or_else(|| {
        (marker.marker.is_none() && !lines)
            .then(|| (AUTO_MARKERS[series_index(marker, si) % AUTO_MARKERS.len()].to_string(), marker_size(e, marker, "")))
    })
}

/// Scatter and bubble series.
fn draw_scatter(ctx: &mut RenderContext, e: &Env, r: Rect, ypos: &dyn Fn(f64) -> f64, xpos: &dyn Fn(f64) -> f64, labels: &mut Vec<LabelReq>) {
    let c = e.c;
    let bubble = c.kind == ChartType::Bubble;
    let max_size = c.series.iter().flat_map(|s| s.sizes.iter().flatten()).fold(0.0f64, |a, b| a.max(b.abs()));
    let max_d = r.width().min(r.height()) * 0.25 * c.fmt.bubble_scale.unwrap_or(100.0) / 100.0;
    let style = c.fmt.scatter_style.as_deref().unwrap_or("marker");
    for (si, s) in c.series.iter().enumerate() {
        let pts: Vec<Option<Point>> =
            (0..s.values.len()).map(|i| s.values.get(i).copied().flatten().map(|v| Point::new(xpos(x_of(s, i)), ypos(v)))).collect();
        let col = series_rgba(e, si, s, !bubble);
        if bubble {
            for (i, p) in pts.iter().enumerate() {
                let (Some(p), Some(sz)) = (p, s.sizes.get(i).copied().flatten()) else {
                    continue;
                };
                if max_size <= 0.0 || sz <= 0.0 {
                    continue;
                }
                let d = max_d * (sz / max_size).sqrt();
                let circle = kurbo::Ellipse::new(*p, (d / 2.0, d / 2.0), 0.0).to_path(0.1);
                match s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f).or(s.fill.as_ref()) {
                    Some(f) => fill_path(ctx, e, f, &circle, circle.bounding_box()),
                    None => solid(ctx, e, if c.vary_colors { auto_color(&e.pal, c, i) } else { col }, &circle),
                }
                stroke(ctx, e, s.line.as_ref(), None, 0.75, &circle);
            }
        } else {
            let lines = match &s.line {
                Some(l) => l.fill.as_ref().is_none_or(|f| !f.is_none()),
                None => matches!(style, "line" | "lineMarker" | "smooth" | "smoothMarker"),
            };
            for (i, p) in pts.iter().enumerate() {
                let Some(p) = p else { continue };
                let marker = point_marker(s, i);
                if let Some((sym, size)) = scatter_marker(e, si, &marker, lines) {
                    let mc = marker.fmt.marker_fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or(col);
                    marker_effects::below(ctx, e, &marker, mc, &sym, size, *p);
                }
            }
            if lines {
                stroke_polyline(ctx, e, s, col, &pts, s.smooth);
            }
            for (i, p) in pts.iter().enumerate() {
                let Some(p) = p else { continue };
                let marker = point_marker(s, i);
                let selected = scatter_marker(e, si, &marker, lines);
                if let Some((sym, size)) = selected {
                    let mc = marker.fmt.marker_fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or(col);
                    draw_marker(ctx, e, &marker, mc, &sym, size, *p);
                }
            }
        }
        for eb in &s.fmt.err_bars {
            for (i, p) in pts.iter().enumerate() {
                let (Some(p), Some(v)) = (p, s.values.get(i).copied().flatten()) else {
                    continue;
                };
                if eb.dir.as_deref() == Some("x") {
                    let x = x_of(s, i);
                    let (m, pl, centre) = error_amounts(eb, &err_values(c, s, eb, true), i, x);
                    let b = centre.unwrap_or(x);
                    err_bar(ctx, e, eb, Point::new(xpos(b), p.y), Point::new(xpos(b - m), p.y), Point::new(xpos(b + pl), p.y));
                } else {
                    let (m, pl, centre) = error_amounts(eb, &err_values(c, s, eb, false), i, v);
                    let b = centre.unwrap_or(v);
                    err_bar(ctx, e, eb, Point::new(p.x, ypos(b)), Point::new(p.x, ypos(b - m)), Point::new(p.x, ypos(b + pl)));
                }
            }
        }
        trendlines(ctx, e, s, col, xpos, ypos, false, s.values.len());
        if let Some(sl) = series_labels(c, s) {
            for (i, p) in pts.iter().enumerate() {
                let Some(p) = p else { continue };
                let Some(l) = sl.point(i) else { continue };
                let text = label_text(c, s, l, i, s.values.get(i).copied().flatten().unwrap_or(0.0), None);
                let (dx, dy) = match l.pos.as_deref().unwrap_or(if bubble { "ctr" } else { "r" }) {
                    "t" => (0.0, -1.0),
                    "b" => (0.0, 1.0),
                    "l" => (-1.0, 0.0),
                    "ctr" => (0.0, 0.0),
                    _ => (1.0, 0.0),
                };
                labels.push(label_req(e, l, text, *p, dx, dy));
            }
        }
    }
}

/// A hidden series line leaves only its marker in a compact key slot.
fn table_key_width(c: &Chart, s: &Series, t: &Txt) -> f64 {
    let kind = s.kind.unwrap_or(c.kind);
    // Stock table keys stay compact, including an explicitly connected price series.
    let has_line = kind != ChartType::Stock && is_line_kind(kind) && !s.line.as_ref().and_then(|l| l.fill.as_ref()).is_some_and(Fill::is_none);
    key_width(if has_line { Key::Line } else { Key::Box }, t)
}

/// Width of a data table's first column: legend keys and series names.
fn table_head_width(e: &Env, dt: &kition_slide_model::chart::DataTable, t: &Txt) -> f64 {
    e.c.series
        .iter()
        .map(|s| {
            let name = measure(e, &s.name, t, None).0;
            let key = if dt.keys { table_key_width(e.c, s, t) + t.size * 0.35 } else { 0.0 };
            name + key
        })
        .fold(0.0, f64::max)
        + 6.0
}

/// A manual inner plot still carries its data table outside the plotting rectangle. Keep that
/// table within the chart when a translation can do so without resizing the requested plot.
fn contain_table_plot(plot: Rect, chart: Rect, head: f64, foot: f64) -> Rect {
    let left = chart.x0 + PAD + head;
    let right = chart.x1 - PAD - plot.width();
    let top = chart.y0 + PAD;
    let bottom = chart.y1 - BOTTOM_EXTRA - foot - plot.height();
    let x = if left <= right { plot.x0.clamp(left, right) } else { plot.x0 };
    let y = if top <= bottom { plot.y0.clamp(top, bottom) } else { plot.y0 };
    Rect::new(x, y, x + plot.width(), y + plot.height())
}

/// A data table under the plot area: one column per category, one row per series.
#[allow(clippy::too_many_arguments)]
fn draw_data_table(ctx: &mut RenderContext, e: &Env, dt: &kition_slide_model::chart::DataTable, r: Rect, y0: f64, cats: &[String], cat_t: &Txt) {
    let c = e.c;
    let t = e.txt(&dt.text, cat_t.size, false);
    let row_h = t.size * DT_ROW;
    let ncat = cats.len().max(1);
    let col_w = r.width() / ncat as f64;
    let head_w = table_head_width(e, dt, &t);
    let x0 = r.x0 - head_w;
    let line = dt.line.clone().unwrap_or_else(|| Line::solid(ColorRef::rgb(GRID), 0.75));
    let rows = c.series.len() + 1;
    let head_h = row_h + DT_HEAD_EXTRA;
    // The top of row `k` (0: the heading row).
    let row_y = |k: usize| if k == 0 { y0 } else { y0 + head_h + row_h * (k - 1) as f64 };
    let y1 = row_y(rows);
    if dt.horizontal {
        for k in 0..=rows {
            let y = row_y(k);
            stroke(ctx, e, Some(&line), None, 0.75, &seg(Point::new(if k == 0 { r.x0 } else { x0 }, y), Point::new(r.x1, y)));
        }
    }
    if dt.vertical {
        for k in 0..=ncat {
            let x = r.x0 + col_w * k as f64;
            stroke(ctx, e, Some(&line), None, 0.75, &seg(Point::new(x, y0), Point::new(x, y1)));
        }
    }
    if dt.outline {
        stroke(ctx, e, Some(&line), None, 0.75, &seg(Point::new(x0, row_y(1)), Point::new(x0, y1)));
    }
    for (i, cat) in cats.iter().enumerate() {
        let x = r.x0 + col_w * i as f64;
        draw_text(ctx, e, cat, cat_t, Rect::new(x, y0, x + col_w, y0 + head_h), Align::Center, false);
    }
    // Stacked vertical charts list the top series first, matching the visible stack.
    let top_down = c.kind.stacked() && !c.kind.horizontal();
    let order: Vec<usize> = if top_down { (0..c.series.len()).rev().collect() } else { (0..c.series.len()).collect() };
    for (row, k) in order.into_iter().enumerate() {
        let Some(s) = c.series.get(k) else { continue };
        let y = row_y(row + 1);
        let mut tx = x0 + 3.0;
        if dt.keys {
            let line_kind = is_line_kind(s.kind.unwrap_or(c.kind));
            let col = series_rgba(e, k, s, line_kind);
            let kw = table_key_width(c, s, &t);
            let cy = y + row_h / 2.0;
            if line_kind {
                stroke(ctx, e, s.line.as_ref(), Some(col), 2.25, &seg(Point::new(tx, cy), Point::new(tx + kw, cy)));
                if let Some((sym, _)) = series_marker(e, s.kind.unwrap_or(c.kind), k, s) {
                    let mc = s.fmt.marker_fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or(col);
                    draw_marker(ctx, e, s, mc, &sym, marker_size(e, s, "").min(t.size), Point::new(tx + kw / 2.0, cy));
                }
            } else {
                match &s.fill {
                    Some(f) => fill_rect(ctx, e, f, Rect::new(tx, cy - kw / 2.0, tx + kw, cy + kw / 2.0)),
                    None => solid(ctx, e, col, &Rect::new(tx, cy - kw / 2.0, tx + kw, cy + kw / 2.0).to_path(0.1)),
                }
            }
            tx += kw + t.size * 0.35;
        }
        draw_text(ctx, e, &s.name, &t, Rect::new(tx, y, r.x0 - 2.0, y + row_h), Align::Left, false);
        for i in 0..ncat {
            let Some(v) = s.values.get(i).copied().flatten() else { continue };
            let x = r.x0 + col_w * i as f64;
            draw_text(ctx, e, &format_number(v, s.fmt.num_fmt.as_deref()), &t, Rect::new(x, y, x + col_w, y + row_h), Align::Center, false);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pie and doughnut

/// A waterfall bar's colour: increases in accent 1, decreases in accent 2, totals in accent 3
/// (PowerPoint's waterfall), from the chart's palette.
fn waterfall_color(e: &Env, k: WaterfallKind) -> Rgba {
    let i = match k {
        WaterfallKind::Increase => 0,
        WaterfallKind::Decrease => 1,
        WaterfallKind::Total => 2,
    };
    auto_color(&e.pal, e.c, i)
}

/// The colour and fill of point `i` of the first series, for charts coloured by point.
fn point_fill(e: &Env, s: &Series, i: usize, by_point: bool) -> (Option<Fill>, Rgba) {
    let fill = s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f.clone()).or_else(|| s.fill.clone());
    let col = fill.as_ref().and_then(|f| e.pal.fill_rgba(f)).unwrap_or_else(|| auto_color(&e.pal, e.c, if by_point { i } else { 0 }));
    (fill, col)
}

/// Draws the value labels of a funnel or treemap: `text` centred in `r` when it fits.
fn label_in(ctx: &mut RenderContext, e: &Env, text: &str, r: Rect, on: Rgba) {
    let lum = 0.299 * f64::from(on.r) + 0.587 * f64::from(on.g) + 0.114 * f64::from(on.b);
    let mut t = e.txt(&e.c.fmt.labels.as_ref().map(|l| l.text.clone()).unwrap_or_default(), e.base.size, false);
    if e.c.fmt.labels.as_ref().and_then(|l| l.text.color.as_ref()).is_none() && e.c.fmt.text.color.is_none() {
        t.color = if lum < 150.0 { Rgba::WHITE } else { TEXT };
    }
    let (tw, th) = measure(e, text, &t, None);
    if tw <= r.width() - 2.0 && th <= r.height() {
        draw_at(ctx, e, text, &t, r.center(), (tw, th));
    }
}

/// A funnel: one bar per category, centred, as wide as its value against the largest, from the
/// first category at the top down, with the category names on the left and values in the bars.
fn draw_funnel(ctx: &mut RenderContext, e: &Env, area: Rect) {
    let c = e.c;
    let Some(s) = c.series.first() else { return };
    let n = category_count(c).min(500);
    let max = s.values.iter().flatten().filter(|v| v.is_finite()).fold(0.0f64, |m, v| m.max(v.abs()));
    if n == 0 || max <= 0.0 || !area.is_finite() {
        return;
    }
    let t = e.txt(&ChartText::default(), e.base.size, false);
    let names: Vec<String> = (0..n).map(|i| category_label(c, i)).collect();
    let label_w = names.iter().map(|name| measure(e, name, &t, None).0).fold(0.0, f64::max).min(area.width() * 0.35);
    let plot = Rect::new(area.x0 + label_w + LABEL_GAP, area.y0, area.x1, area.y1);
    let band = plot.height() / n as f64;
    let gap = band * c.gap_width.clamp(0.0, 5.0) / (1.0 + c.gap_width.clamp(0.0, 5.0)) * 0.3;
    let lbl = series_labels(c, s).or(Some(DataLabels { value: true, ..Default::default() }));
    for (i, name) in names.iter().enumerate() {
        let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
        let w = plot.width() * v.abs() / max;
        let cx = plot.center().x;
        let y0 = plot.y0 + i as f64 * band + gap / 2.0;
        let r = Rect::new(cx - w / 2.0, y0, cx + w / 2.0, y0 + band - gap);
        let (fill, col) = point_fill(e, s, i, c.vary_colors);
        match &fill {
            Some(f) => fill_rect(ctx, e, f, r),
            None => solid(ctx, e, col, &r.to_path(0.1)),
        }
        let (nw, nh) = measure(e, name, &t, None);
        draw_text(
            ctx,
            e,
            name,
            &t,
            Rect::new(area.x0, r.center().y - nh / 2.0, area.x0 + label_w.max(nw.min(label_w)), r.center().y + nh / 2.0),
            Align::Right,
            false,
        );
        if let Some(l) = lbl.as_ref().and_then(|l| l.point(i)) {
            label_in(ctx, e, &label_text(c, s, l, i, v, None), r, col);
        }
    }
}

/// A treemap: each category a rectangle with area in proportion to its value, laid out in
/// squarified rows, each named with its category (and value with data labels).
fn draw_treemap(ctx: &mut RenderContext, e: &Env, area: Rect) {
    let c = e.c;
    let Some(s) = c.series.first() else { return };
    let n = category_count(c).min(500);
    let mut items: Vec<(usize, f64)> =
        (0..n).filter_map(|i| Some((i, s.values.get(i).copied().flatten().filter(|v| v.is_finite() && *v > 0.0)?))).collect();
    items.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = items.iter().map(|x| x.1).sum();
    if items.is_empty() || total <= 0.0 || !area.is_finite() || area.width() < 1.0 || area.height() < 1.0 {
        return;
    }
    let scale = area.width() * area.height() / total;
    let mut rest = area;
    let mut placed: Vec<(usize, Rect)> = vec![];
    let mut k = 0;
    while k < items.len() {
        let short = rest.width().min(rest.height()).max(1e-9);
        // Grow the row while its worst aspect ratio improves.
        let worst = |row: &[(usize, f64)]| {
            let sum: f64 = row.iter().map(|x| x.1 * scale).sum();
            let side = sum / short;
            row.iter()
                .map(|x| {
                    let len = x.1 * scale / side.max(1e-9);
                    (side / len.max(1e-9)).max(len / side.max(1e-9))
                })
                .fold(0.0, f64::max)
        };
        let mut end = k + 1;
        while end < items.len() && worst(&items[k..end + 1]) <= worst(&items[k..end]) {
            end += 1;
        }
        let row = &items[k..end];
        let sum: f64 = row.iter().map(|x| x.1 * scale).sum();
        let side = sum / short;
        let mut at = 0.0;
        for (i, v) in row {
            let len = v * scale / side.max(1e-9);
            let r = if rest.width() >= rest.height() {
                Rect::new(rest.x0, rest.y0 + at, rest.x0 + side, rest.y0 + at + len)
            } else {
                Rect::new(rest.x0 + at, rest.y0, rest.x0 + at + len, rest.y0 + side)
            };
            placed.push((*i, r));
            at += len;
        }
        rest = if rest.width() >= rest.height() {
            Rect::new(rest.x0 + side, rest.y0, rest.x1, rest.y1)
        } else {
            Rect::new(rest.x0, rest.y0 + side, rest.x1, rest.y1)
        };
        k = end;
    }
    let lbl = series_labels(c, s);
    for (i, r) in placed {
        let r = r.inset(-0.75);
        let (fill, col) = point_fill(e, s, i, true);
        match &fill {
            Some(f) => fill_rect(ctx, e, f, r),
            None => solid(ctx, e, col, &r.to_path(0.1)),
        }
        let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
        let text = match lbl.as_ref().and_then(|l| l.point(i)) {
            Some(l) if l.value && !l.category => format!("{}\n{}", category_label(c, i), label_text(c, s, l, i, v, None)),
            Some(l) => label_text(c, s, l, i, v, None),
            None => category_label(c, i),
        };
        let mut t = e.txt(&ChartText::default(), e.base.size, false);
        if e.c.fmt.text.color.is_none() {
            let lum = 0.299 * f64::from(col.r) + 0.587 * f64::from(col.g) + 0.114 * f64::from(col.b);
            t.color = if lum < 150.0 { Rgba::WHITE } else { TEXT };
        }
        let pad = 4.0;
        draw_text(
            ctx,
            e,
            &text,
            &t,
            Rect::new(r.x0 + pad, r.y0 + pad, (r.x1 - pad).max(r.x0 + pad), (r.y1 - pad).max(r.y0 + pad)),
            Align::Left,
            true,
        );
    }
}

fn draw_pie(ctx: &mut RenderContext, e: &Env, outer: Option<Rect>, inner: Option<Rect>) {
    let c = e.c;
    if c.fmt.of_pie.is_some() && c.kind == ChartType::Pie {
        draw_of_pie(ctx, e, inner.or(outer).unwrap_or(Rect::ZERO));
        return;
    }
    let Some(s) = c.series.first() else { return };
    let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).abs()).collect();
    let total: f64 = vals.iter().sum();
    if !(total > 0.0 && total.is_finite()) {
        return;
    }
    let lbl = series_labels(c, s);
    let outside = lbl.as_ref().is_some_and(|l| matches!(l.pos.as_deref(), Some("outEnd" | "bestFit")));
    let area = inner.or(outer).unwrap_or(Rect::ZERO);
    let margin = match (&lbl, outside && inner.is_none()) {
        // Outside labels: the pie leaves room for the tallest one.
        (Some(l), true) => {
            (0..vals.len())
                .filter_map(|i| {
                    let p = l.point(i)?;
                    let t = e.txt(&p.text, e.base.size, false);
                    let text = label_text(c, s, p, i, s.values.get(i).copied().flatten().unwrap_or(0.0), Some(vals[i] / total));
                    Some(measure(e, &text, &t, None).1 + 3.0)
                })
                .fold(0.0, f64::max)
                * 0.7
        }
        _ => 0.0,
    };
    let max_expl = s.fmt.point_explosion.iter().map(|(_, x)| *x).fold(s.fmt.explosion, f64::max);
    // A 3-D pie is a tilted disc with a wall under its front half.
    let view = c.fmt.view3d.filter(|_| c.fmt.three_d && c.kind == ChartType::Pie);
    let (tilt, depth_k) = match view {
        Some((rx, _, _)) => (rx.to_radians().sin().abs().max(0.05), 0.32 * rx.to_radians().cos().abs()),
        None => (1.0, 0.0),
    };
    // With labels placed automatically an automatic plot area keeps the pie at 7/8 of its room;
    // labels outside shrink it to fit them.
    // Exploded slices use that room first.
    let best_fit = lbl.as_ref().is_some_and(|l| matches!(l.pos.as_deref(), None | Some("bestFit")));
    let fit = if c.fmt.plot_layout.is_none() && best_fit { PIE_FIT } else { 1.0 };
    let half = (area.width() / 2.0).min(area.height() / (2.0 * tilt + depth_k));
    let rad = if view.is_some() {
        // A 3-D pie keeps 7/8 of its room, and 7/8 of that with labels around it.
        half * PIE_FIT * if lbl.is_some() && (outside || best_fit) { PIE_FIT } else { 1.0 } / (1.0 + max_expl)
    } else {
        (half * fit).min(half / (1.0 + max_expl)).min(half - margin).max(1.0)
    };
    let depth = rad * depth_k;
    let cen = Point::new(area.center().x, area.center().y - depth / 2.0);
    let doughnut = c.kind == ChartType::Doughnut;
    let hole = if doughnut { c.hole_size.clamp(0.1, 0.9) } else { 0.0 };
    let start = match view {
        Some((_, ry, _)) => ry + c.fmt.first_slice_angle,
        None => c.fmt.first_slice_angle,
    };
    // Each slice: index, value, start angle, sweep, centre (exploded).
    let mut slices = vec![];
    let mut a0 = (start - 90.0).to_radians().rem_euclid(std::f64::consts::TAU);
    for (i, v) in vals.iter().enumerate() {
        let sweep = v / total * std::f64::consts::TAU;
        if sweep <= 0.0 {
            continue;
        }
        let expl = s.fmt.point_explosion.iter().find(|(k, _)| *k == i).map(|(_, x)| *x).unwrap_or(s.fmt.explosion);
        let mid = a0 + sweep / 2.0;
        let cc = Point::new(cen.x + rad * expl * mid.cos(), cen.y + rad * expl * tilt * mid.sin());
        slices.push((i, *v, a0, sweep, cc));
        a0 += sweep;
    }
    let on = |cc: Point, k: f64, a: f64| Point::new(cc.x + rad * k * a.cos(), cc.y + rad * k * tilt * a.sin());
    // Points along the rim from `a` to `b` (radius fraction `k`).
    let arc = |p: &mut BezPath, cc: Point, k: f64, a: f64, b: f64, dy: f64| {
        let n = ((b - a).abs() / 0.03).ceil().clamp(1.0, 400.0) as usize;
        for t in 0..=n {
            let q = on(cc, k, a + (b - a) * t as f64 / n as f64);
            p.line_to(Point::new(q.x, q.y + dy));
        }
    };
    let fill_of = |i: usize| s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f).or(if c.vary_colors { None } else { s.fill.as_ref() });
    let line_of = |i: usize| s.fmt.point_lines.iter().find(|(k, _)| *k == i).map(|(_, l)| l).or(s.line.as_ref());
    let default_line = |l: Option<&Line>| if c.fmt.axes.is_empty() && l.is_none() { Some(Rgba::WHITE) } else { None };
    if depth > 0.0 {
        // Walls: the part of each slice's rim facing the viewer (angles 0..π), darker.
        for &(i, _, a0, sweep, cc) in &slices {
            for k in 0..=1 {
                let base = k as f64 * std::f64::consts::TAU;
                let (lo, hi) = (a0.max(base), (a0 + sweep).min(base + std::f64::consts::PI));
                if hi <= lo {
                    continue;
                }
                let mut w = BezPath::new();
                w.move_to(on(cc, 1.0, lo));
                arc(&mut w, cc, 1.0, lo, hi, 0.0);
                arc(&mut w, cc, 1.0, hi, lo, depth);
                w.close_path();
                let col = fill_of(i).and_then(|f| e.pal.fill_rgba(f)).unwrap_or_else(|| auto_color(&e.pal, c, i));
                wall(ctx, e, col, &w, cc.x, rad);
                stroke(ctx, e, line_of(i), default_line(line_of(i)), 0.75, &w);
            }
        }
    }
    let mut reqs = vec![];
    for &(i, v, a0, sweep, cc) in &slices {
        let mid = a0 + sweep / 2.0;
        let mut p = BezPath::new();
        if doughnut {
            p.move_to(on(cc, 1.0, a0));
            arc(&mut p, cc, 1.0, a0, a0 + sweep, 0.0);
            arc(&mut p, cc, hole, a0 + sweep, a0, 0.0);
            p.close_path();
        } else {
            p.move_to(cc);
            arc(&mut p, cc, 1.0, a0, a0 + sweep, 0.0);
            p.close_path();
        }
        shadow(ctx, e, point_shadow(s, i), &p, None);
        match fill_of(i) {
            // The lit top of a 3-D pie shows its colour a little brighter.
            Some(f) if depth > 0.0 && matches!(f, Fill::Solid { .. }) => solid(ctx, e, lit(e.pal.fill_rgba(f).unwrap_or(Rgba::WHITE), TOP_LIGHT), &p),
            Some(f) => fill_path(ctx, e, f, &p, p.bounding_box()),
            None if depth > 0.0 => solid(ctx, e, lit(auto_color(&e.pal, c, i), TOP_LIGHT), &p),
            None => solid(ctx, e, auto_color(&e.pal, c, i), &p),
        }
        let line = line_of(i);
        stroke(ctx, e, line, default_line(line), 0.75, &p);
        if let Some(l) = lbl.as_ref().and_then(|l| l.point(i)) {
            let text = label_text(c, s, l, i, s.values.get(i).copied().flatten().unwrap_or(0.0), Some(v / total));
            let ring = if doughnut { (1.0 + hole) / 2.0 } else { 0.0 };
            let (at, dx, dy) = match l.pos.as_deref() {
                Some("outEnd") => (on(cc, 1.0, mid), mid.cos(), mid.sin()),
                Some("inEnd") => {
                    let mut fraction = if doughnut { ring } else { 0.75 };
                    if !doughnut && !c.fmt.three_d && l.layout.is_none() {
                        let t = e.txt(&l.text, e.base.size, false);
                        let (tw, th) = measure(e, &text, &t, None);
                        let framed = l.fill.as_ref().is_some_and(|f| !f.is_none()) || visible(l.line.as_ref());
                        let (ix, iy) = if framed { (3.0, 1.5) } else { (1.5, 0.0) };
                        let (bw, bh) = (tw + 2.0 * ix, th + 2.0 * iy);
                        let (rw, rh) = rotated_size((bw, bh), t.rot);
                        // The label frame stays unrotated while its text rotates.
                        let (bw, bh) = if framed { (bw.max(rw), bh.max(rh)) } else { (rw, rh) };
                        // Keep the measured label box inside the rim with the chart label gap.
                        if let Some(distance) = pie_labels::inside_end_radius(rad - 2.0, bw / 2.0, bh / 2.0, mid) {
                            fraction = distance / rad;
                        }
                    }
                    (on(cc, fraction, mid), 0.0, 0.0)
                }
                Some("ctr") => (on(cc, if doughnut { ring } else { 0.5 }, mid), 0.0, 0.0),
                _ => (on(cc, if doughnut { ring } else { 0.65 }, mid), 0.0, 0.0),
            };
            reqs.push(label_req(e, l, text, at, dx, dy));
        }
    }
    for l in &reqs {
        draw_label(ctx, e, l);
    }
}

/// How much brighter the top of a 3-D pie shows than its colour (measured on the reference renders).
const TOP_LIGHT: f64 = 1.06;

/// `c` scaled by `k` per channel (alpha kept).
fn lit(c: Rgba, k: f64) -> Rgba {
    let f = |v: u8| (v as f64 * k).round().clamp(0.0, 255.0) as u8;
    Rgba { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a }
}

/// The wall of a 3-D pie slice: a cylinder lit from the front, a little from the left, so it is
/// brightest left of the centre and falls to black at its right edge. Brightness is
/// `0.074 + 1.026 (n·L)` with `n` the wall's normal and `L = (-0.25, 0, 0.97)`, fitted to the
/// reference renders; `cx` and `rad` are the slice's centre and radius.
fn wall(ctx: &mut RenderContext, e: &Env, col: Rgba, p: &BezPath, cx: f64, rad: f64) {
    use vello_cpu::peniko;
    let stops: Vec<peniko::ColorStop> = (0..=40)
        .map(|k| {
            let u = k as f64 / 20.0 - 1.0;
            let dot = (-0.25 * u + 0.97 * (1.0 - u * u).max(0.0).sqrt()).max(0.0);
            peniko::ColorStop { offset: k as f32 / 40.0, color: color(lit(col, 0.074 + 1.026 * dot), 1.0).into() }
        })
        .collect();
    ctx.set_transform(e.m);
    ctx.set_paint(peniko::Gradient::new_linear(Point::new(cx - rad, 0.0), Point::new(cx + rad, 0.0)).with_stops(stops.as_slice()));
    ctx.fill_path(p);
}

/// Pie of pie and bar of pie: the last points (by position, value or percentage) leave the pie
/// as one combined slice facing a smaller pie or a stacked bar that shows them, joined by series
/// lines.
fn draw_of_pie(ctx: &mut RenderContext, e: &Env, area: Rect) {
    let c = e.c;
    let f = &c.fmt;
    let Some(s) = c.series.first() else { return };
    let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).abs()).collect();
    let total: f64 = vals.iter().sum();
    if !(total > 0.0 && total.is_finite()) || vals.len() < 2 {
        return;
    }
    let n = vals.len();
    let split = f.split_pos.unwrap_or(if n > 3 { 2.0 } else { 1.0 });
    let second: Vec<bool> = match f.split_type.as_deref() {
        Some("val") => vals.iter().map(|v| *v < split).collect(),
        Some("percent") => vals.iter().map(|v| v / total * 100.0 < split).collect(),
        _ => {
            let k = (split.round().max(1.0) as usize).min(n - 1);
            (0..n).map(|i| i >= n - k).collect()
        }
    };
    let other: f64 = vals.iter().zip(&second).filter(|(_, s)| **s).map(|(v, _)| v).sum();
    if other <= 0.0 {
        return;
    }
    let bar = f.of_pie.as_deref() == Some("bar");
    let k2 = f.second_pie_size.unwrap_or(75.0) / 100.0;
    let gap = c.gap_width.clamp(0.0, 5.0);
    let lbl = series_labels(c, s);
    // Width in radii: the pie, the gap, the second plot.
    // The gap is the gap width × the radius (half that before a bar).
    let gap_r = if bar { gap / 2.0 } else { gap };
    let units = 2.0 + gap_r + if bar { k2 } else { 2.0 * k2 };
    let fit = if lbl.is_some() { PIE_FIT } else { 1.0 };
    let r = (area.width() / units).min(area.height() / 2.0) * fit;
    let r2 = r * k2;
    let x0 = area.center().x - units * r / 2.0;
    let cen = Point::new(x0 + r, area.center().y);
    let fill_of = |i: usize| s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f).or(if c.vary_colors { None } else { s.fill.as_ref() });
    let line_of = |i: usize| s.fmt.point_lines.iter().find(|(k, _)| *k == i).map(|(_, l)| l).or(s.line.as_ref());
    let paint = |ctx: &mut RenderContext, i: usize, p: &BezPath| {
        match fill_of(i) {
            Some(fl) => fill_path(ctx, e, fl, p, p.bounding_box()),
            None => solid(ctx, e, auto_color(&e.pal, c, i), p),
        }
        stroke(ctx, e, line_of(i), None, 0.75, p);
    };
    let wedge = |cc: Point, rr: f64, a0: f64, sweep: f64| {
        let mut p = BezPath::new();
        p.move_to(cc);
        p.line_to(Point::new(cc.x + rr * a0.cos(), cc.y + rr * a0.sin()));
        p.extend(kurbo::Arc::new(cc, (rr, rr), a0, sweep, 0.0).append_iter(0.05));
        p.close_path();
        p
    };
    let mut reqs = vec![];
    let mut label = |i: usize, at: Point, dx: f64, dy: f64| {
        if let Some(l) = lbl.as_ref().and_then(|l| l.point(i)) {
            let text = label_text(c, s, l, i, s.values.get(i).copied().flatten().unwrap_or(0.0), Some(vals[i] / total));
            reqs.push(label_req(e, l, text, at, dx, dy));
        }
    };
    // The first plot: the combined slice centred on the right, the others clockwise after it.
    let other_sweep = other / total * std::f64::consts::TAU;
    let mut a = other_sweep / 2.0;
    for (i, v) in vals.iter().enumerate() {
        if second[i] || *v <= 0.0 {
            continue;
        }
        let sweep = v / total * std::f64::consts::TAU;
        let p = wedge(cen, r, a, sweep);
        shadow(ctx, e, point_shadow(s, i), &p, None);
        paint(ctx, i, &p);
        let mid = a + sweep / 2.0;
        let pos = lbl.as_ref().and_then(|l| l.point(i)).and_then(|l| l.pos.clone());
        match pos.as_deref() {
            Some("outEnd") => label(i, Point::new(cen.x + r * mid.cos(), cen.y + r * mid.sin()), mid.cos(), mid.sin()),
            _ => label(i, Point::new(cen.x + r * 0.65 * mid.cos(), cen.y + r * 0.65 * mid.sin()), 0.0, 0.0),
        }
        a += sweep;
    }
    let combined = wedge(cen, r, -other_sweep / 2.0, other_sweep);
    // The combined slice is painted like the point after the first plot's last one.
    let combined_i = (0..n).filter(|i| !second[*i]).count();
    match s.fill.as_ref().filter(|_| !c.vary_colors) {
        Some(fl) => fill_path(ctx, e, fl, &combined, combined.bounding_box()),
        None => solid(ctx, e, auto_color(&e.pal, c, combined_i), &combined),
    }
    stroke(ctx, e, s.line.as_ref(), None, 0.75, &combined);
    let top = Point::new(cen.x + r * (other_sweep / 2.0).cos(), cen.y - r * (other_sweep / 2.0).sin());
    let bottom = Point::new(top.x, cen.y + r * (other_sweep / 2.0).sin());
    let sx = x0 + (2.0 + gap_r) * r;
    let ser_line = f.ser_lines.clone();
    if bar {
        // A stacked column of the split points, the first on top.
        let h = 2.0 * r2;
        let mut y = cen.y - r2;
        for (i, v) in vals.iter().enumerate() {
            if !second[i] {
                continue;
            }
            let bh = v / other * h;
            let rect = snap_rect(e.m, Rect::new(sx, y, sx + r2, y + bh));
            paint(ctx, i, &rect.to_path(0.1));
            let pos = lbl.as_ref().and_then(|l| l.point(i)).and_then(|l| l.pos.clone());
            match pos.as_deref() {
                Some("outEnd") | Some("bestFit") | None => label(i, Point::new(sx + r2, y + bh / 2.0), 1.0, 0.0),
                _ => label(i, Point::new(sx + r2 / 2.0, y + bh / 2.0), 0.0, 0.0),
            }
            y += bh;
        }
        if let Some(l) = &ser_line {
            stroke(ctx, e, Some(l), None, 0.75, &seg(top, Point::new(sx, cen.y - r2)));
            stroke(ctx, e, Some(l), None, 0.75, &seg(bottom, Point::new(sx, cen.y + r2)));
        }
    } else {
        let c2 = Point::new(sx + r2, cen.y);
        // The second pie starts where the first one's slices do, clockwise.
        let mut a = other_sweep / 2.0;
        for (i, v) in vals.iter().enumerate() {
            if !second[i] {
                continue;
            }
            let sweep = v / other * std::f64::consts::TAU;
            let p = wedge(c2, r2, a, sweep);
            paint(ctx, i, &p);
            let mid = a + sweep / 2.0;
            label(i, Point::new(c2.x + r2 * 0.65 * mid.cos(), c2.y + r2 * 0.65 * mid.sin()), 0.0, 0.0);
            a += sweep;
        }
        if let Some(l) = &ser_line {
            // Tangent-ish joins to the second pie's top and bottom.
            stroke(ctx, e, Some(l), None, 0.75, &seg(top, Point::new(c2.x, c2.y - r2)));
            stroke(ctx, e, Some(l), None, 0.75, &seg(bottom, Point::new(c2.x, c2.y + r2)));
        }
    }
    for l in &reqs {
        draw_label(ctx, e, l);
    }
}

// ---------------------------------------------------------------------------------------------
// Radar

/// Nonnegative radial scales round the data range directly to a nice major unit.
/// Keep logarithmic and negative ranges on the established scaling path.
fn radar_scale(a: &Axis, lo: f64, hi: f64, t: &Txt) -> Scale {
    if a.log_base.is_some() || lo < 0.0 || a.min.is_some_and(|min| min < 0.0) {
        return scale_for(a, lo, hi, t.size * 2.2 * 10.0, t, false, 0.0);
    }
    let (min, max, major) = kition_slide_model::chart::auto_scale_with_headroom(lo, hi, a.min, a.max, a.major_unit, 10.0, 0.0);
    Scale { min, max, major, log: None, reversed: a.reversed }
}

fn draw_radar(ctx: &mut RenderContext, e: &Env, outer: Option<Rect>, inner: Option<Rect>) {
    let c = e.c;
    let ax = find_axes(c);
    let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).max(1);
    let cat_t = axis_text(e, &ax.cat);
    let val_t = axis_text(e, &ax.val);
    let cats: Vec<String> = (0..ncat).map(|i| category_label(c, i)).collect();
    let area = inner.or(outer).unwrap_or(Rect::ZERO);
    let (lw, lh) = if inner.is_none() && shows_labels(&ax.cat) {
        let (w, h) = cats.iter().map(|s| measure(e, s, &cat_t, None)).fold((0.0, 0.0), |a: (f64, f64), b| (a.0.max(b.0), a.1.max(b.1)));
        (w + LABEL_GAP, h + LABEL_GAP)
    } else {
        (0.0, 0.0)
    };
    let rad = (area.width() / 2.0 - lw).min(area.height() / 2.0 - lh).max(1.0);
    let cen = area.center();
    let (lo, hi) = value_range(c, &ax, false);
    // Radar value axes take up to ten intervals whatever their length.
    let vs = radar_scale(&ax.val, lo, hi, &val_t);
    let ang = |i: usize| -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / ncat as f64;
    let at = |i: usize, v: f64| {
        let f = vs.frac(v).max(0.0) * rad;
        Point::new(cen.x + f * ang(i).cos(), cen.y + f * ang(i).sin())
    };
    // Web: value gridlines as polygons, category spokes.
    if let Some(g) = &ax.val.major_grid {
        for v in vs.ticks() {
            let mut p = BezPath::new();
            for i in 0..ncat {
                if i == 0 { p.move_to(at(i, v)) } else { p.line_to(at(i, v)) }
            }
            p.close_path();
            stroke(ctx, e, Some(g), Some(GRID), 0.75, &p);
        }
    }
    // Each radial spoke is a value axis. Category-axis grid styling does not
    // create spokes; the value axis owns their visibility and endpoint styles.
    if !ax.val.deleted
        && let Some(line) = ax.val.line.as_ref()
    {
        for i in 0..ncat {
            let outer = Point::new(cen.x + rad * ang(i).cos(), cen.y + rad * ang(i).sin());
            let (start, end) = if ax.val.reversed { (outer, cen) } else { (cen, outer) };
            stroke(ctx, e, Some(line), Some(GRID), 0.75, &seg(start, end));
        }
    }
    let filled = c.kind == ChartType::FilledRadar;
    let mut labels = vec![];
    for (si, s) in c.series.iter().enumerate() {
        let col = series_rgba(e, si, s, !filled);
        let pts: Vec<Option<Point>> = (0..ncat).map(|i| s.values.get(i).copied().flatten().map(|v| at(i, v))).collect();
        let mut p = BezPath::new();
        for (k, q) in pts.iter().flatten().enumerate() {
            if k == 0 { p.move_to(*q) } else { p.line_to(*q) }
        }
        if p.elements().is_empty() {
            continue;
        }
        p.close_path();
        if filled {
            match &s.fill {
                Some(f) => fill_path(ctx, e, f, &p, p.bounding_box()),
                None => solid(ctx, e, col, &p),
            }
            stroke(ctx, e, s.line.as_ref(), None, 0.75, &p);
        } else {
            for (i, q) in pts.iter().enumerate() {
                let Some(q) = q else { continue };
                let marker = point_marker(s, i);
                if let Some((symbol, size)) = series_marker(e, c.kind, si, &marker) {
                    marker_effects::below(ctx, e, &marker, col, &symbol, size, *q);
                }
            }
            stroke(ctx, e, s.line.as_ref(), Some(col), 2.25, &p);
            for (i, q) in pts.iter().enumerate() {
                let Some(q) = q else { continue };
                let marker = point_marker(s, i);
                if let Some((symbol, size)) = series_marker(e, c.kind, si, &marker) {
                    draw_marker(ctx, e, &marker, col, &symbol, size, *q);
                }
            }
        }
        if let Some(sl) = series_labels(c, s) {
            for (i, q) in pts.iter().enumerate() {
                let (Some(q), Some(l)) = (q, sl.point(i)) else { continue };
                let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                labels.push(label_req(e, l, label_text(c, s, l, i, v, None), *q, 0.0, -1.0));
            }
        }
    }
    if shows_labels(&ax.cat) {
        for (i, t) in cats.iter().enumerate() {
            let sz = measure(e, t, &cat_t, None);
            let a = ang(i);
            let d = rad + LABEL_GAP;
            draw_at(ctx, e, t, &cat_t, Point::new(cen.x + d * a.cos() + a.cos() * sz.0 / 2.0, cen.y + d * a.sin() + a.sin() * sz.1 / 2.0), sz);
        }
    }
    if shows_labels(&ax.val) {
        for (v, t) in value_labels(&ax.val, &vs, false) {
            let p = at(0, v);
            let sz = measure(e, &t, &val_t, None);
            draw_text(
                ctx,
                e,
                &t,
                &val_t,
                Rect::new(p.x - sz.0 - LABEL_GAP - 2.0, p.y - sz.1 / 2.0, p.x - LABEL_GAP, p.y + sz.1 / 2.0),
                Align::Right,
                false,
            );
        }
    }
    for l in &labels {
        draw_label(ctx, e, l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderOpts, render_slide};
    use kition_slide_model::chart::{Axis, AxisKind, ManualLayout};
    use kition_slide_model::{ShapeId, ShapeKind, SlideFile, Xfrm};

    fn slide_file(c: Chart) -> SlideFile {
        let mut p = SlideFile::default();
        let s = std::sync::Arc::make_mut(&mut p.slides[0]);
        s.shapes = vec![Shape {
            id: ShapeId(900),
            xfrm: Some(Xfrm::new(0.0, 0.0, 960.0, 540.0)),
            kind: ShapeKind::Chart(Box::new(c)),
            ..Default::default()
        }];
        p
    }

    fn render(c: Chart) -> crate::Image {
        render_slide(&slide_file(c), 0, &RenderOpts { scale: 1.0, ..Default::default() })
    }

    /// A column chart whose inner plot area the file places at (100, 100)–(860, 440).
    fn placed_column() -> Chart {
        let mut c = Chart::sample(ChartType::Column);
        c.title = None;
        c.legend = None;
        c.fmt.plot_layout = Some(ManualLayout {
            inner: true,
            x: Some(100.0 / 960.0),
            y: Some(100.0 / 540.0),
            w: Some(760.0 / 960.0),
            h: Some(340.0 / 540.0),
            x_edge: true,
            y_edge: true,
        });
        c
    }

    #[test]
    fn data_table_centres_categories_without_changing_standalone_mid_category_axes() {
        for kind in [AxisKind::Category, AxisKind::Date] {
            let mut c = placed_column();
            c.kind = ChartType::Line;
            c.categories = vec!["1".into(), "2".into(), "3".into(), "4".into()];
            c.series.truncate(1);
            c.series[0].values = vec![Some(1.0); 4];
            c.series[0].line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)), 2.0));
            c.series[0].fmt.axes = vec![1, 2];
            c.fmt.axes = vec![
                Axis { id: 1, kind, pos: "b".into(), deleted: true, cross_ax: 2, ..Default::default() },
                Axis {
                    id: 2,
                    kind: AxisKind::Value,
                    pos: "l".into(),
                    deleted: true,
                    cross_ax: 1,
                    min: Some(0.0),
                    max: Some(2.0),
                    between: Some(false),
                    ..Default::default()
                },
            ];
            let extent = |chart| {
                let image = render(chart);
                let xs: Vec<u32> = (90..870)
                    .filter(|&x| {
                        (264..276).any(|y| {
                            let [r, g, b, _] = image.pixel(x, y);
                            r > 200 && g < 80 && b < 80
                        })
                    })
                    .collect();
                (*xs.first().expect("series start"), *xs.last().expect("series end"))
            };
            let standalone = extent(c.clone());
            assert!(standalone.0.abs_diff(100) <= 1 && standalone.1.abs_diff(860) <= 1, "mid-category ends: {standalone:?}");
            c.fmt.data_table = Some(kition_slide_model::chart::DataTable::default());
            let table = extent(c);
            // Four equally spaced table columns cover x=100..860; their centres are
            // 195, 385, 575 and 765. The plotted series must share those centres.
            assert!(table.0.abs_diff(195) <= 1 && table.1.abs_diff(765) <= 1, "table-aligned ends: {table:?}");
        }
    }

    #[test]
    fn bottom_legend_spacing_follows_visible_axis_text_ownership() {
        for kind in [ChartType::Column, ChartType::Line, ChartType::Bar, ChartType::Scatter] {
            let mut c = Chart::sample(kind);
            c.fmt.axes = vec![
                c.default_axis(AxisKind::Category, if kind.horizontal() { "l" } else { "b" }, 1, 2),
                c.default_axis(AxisKind::Value, if kind.horizontal() { "b" } else { "l" }, 2, 1),
            ];
            for series in &mut c.series {
                series.fmt.axes = vec![1, 2];
            }
            let below = usize::from(kind.horizontal());
            assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
            c.fmt.axes[below].label_pos = "none".into();
            assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
            c.fmt.axes[below].title = Some("Axis title".into());
            c.fmt.axes[below].deleted = true;
            assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
            c.fmt.axes[below].title = Some("  ".into());
            assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
            c.fmt.data_table = Some(Default::default());
            assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        }
        for kind in [ChartType::Pie, ChartType::Doughnut, ChartType::Radar, ChartType::FilledRadar] {
            assert_eq!(bottom_legend_gap(&Chart::sample(kind)), LEGEND_GAP);
        }
    }

    #[test]
    fn bottom_legend_spacing_respects_high_interior_and_reversed_crossings() {
        let mut c = Chart::sample(ChartType::Column);
        c.fmt.axes = vec![c.default_axis(AxisKind::Category, "b", 1, 2), c.default_axis(AxisKind::Value, "l", 2, 1)];
        for series in &mut c.series {
            series.fmt.axes = vec![1, 2];
        }
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[0].label_pos = "high".into();
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[0].label_pos = "nextTo".into();
        c.series[0].values[0] = Some(-2.0);
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[0].crosses = Some("min".into());
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[0].crosses = Some("max".into());
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[1].reversed = true;
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[0].crosses_at = Some(3.0);
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[1].max = Some(3.0);
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[0].crosses_at = Some(2.0);
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[0].label_pos = "low".into();
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[0].label_pos = "high".into();
        c.fmt.axes[0].title = Some("Retained title".into());
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
    }

    #[test]
    fn bottom_legend_spacing_handles_bar_orientation_and_logarithmic_crossings() {
        let mut c = Chart::sample(ChartType::Bar);
        c.fmt.axes = vec![c.default_axis(AxisKind::Category, "l", 1, 2), c.default_axis(AxisKind::Value, "b", 2, 1)];
        for series in &mut c.series {
            series.fmt.axes = vec![1, 2];
        }
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[1].crosses = Some("max".into());
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[0].reversed = true;
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[1].crosses_at = Some(1.0);
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[1].label_pos = "low".into();
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[1].label_pos = "high".into();
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);

        c.kind = ChartType::Column;
        c.fmt.axes[0].reversed = false;
        c.fmt.axes[0].pos = "b".into();
        c.fmt.axes[1].pos = "l".into();
        c.fmt.axes[1].log_base = Some(10.0);
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
        c.fmt.axes[1].reversed = true;
        assert_eq!(bottom_legend_gap(&c), LEGEND_GAP);
        c.fmt.axes[0].crosses = Some("max".into());
        assert_eq!(bottom_legend_gap(&c), TITLE_GAP);
    }

    #[test]
    fn automatic_table_keeps_long_line_keys_clear_of_the_axis_title() {
        let mut c = placed_column();
        c.kind = ChartType::Line;
        c.fmt.plot_layout = None;
        c.series.truncate(1);
        c.series[0].line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0)), 1.0));
        c.series[0].fmt.axes = vec![1, 2];
        c.fmt.axes = vec![
            Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), cross_ax: 2, ..Default::default() },
            Axis {
                id: 2,
                kind: AxisKind::Value,
                pos: "l".into(),
                cross_ax: 1,
                title: Some("Value axis".into()),
                title_text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(255, 0, 0))), ..Default::default() },
                ..Default::default()
            },
        ];
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable {
            outline: true,
            horizontal: true,
            keys: true,
            line: Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 1.0)),
            ..Default::default()
        });
        let image = render(c);
        let mut title_right = None;
        let mut table_left = None;
        for y in 0..540 {
            for x in 0..960 {
                let [r, g, b, _] = image.pixel(x, y);
                if r > 200 && g < 80 && b < 80 {
                    title_right = Some(title_right.map_or(x, |right: u32| right.max(x)));
                }
                if b > 200 && r < 80 && g < 80 {
                    table_left = Some(table_left.map_or(x, |left: u32| left.min(x)));
                }
            }
        }
        assert!(table_left.expect("table border") > title_right.expect("axis title"), "table keys must not enter the axis-title gutter");
    }

    #[test]
    fn a_data_tables_heading_row_is_taller_than_its_series_rows() {
        // Measured on the reference: 9 pt rows of 14.4 pt under a 15.4 pt heading row.
        assert!((table_height(9.0, 3) - (15.4 + 3.0 * 14.4)).abs() < 1e-9);
        assert!((table_height(9.0, 0) - 15.4).abs() < 1e-9);
    }

    #[test]
    fn data_table_width_uses_complete_rows_in_mixed_charts() {
        let mut c = placed_column();
        c.kind = ChartType::Line;
        c.fmt.plot_layout = None;
        c.plot_fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 255))));
        c.series.truncate(1);
        c.series[0].name = "M".into();
        c.series[0].line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0)), 1.0));
        c.series[0].fmt.axes = vec![1, 2];
        c.fmt.axes = vec![
            Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), deleted: true, cross_ax: 2, ..Default::default() },
            Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), deleted: true, cross_ax: 1, ..Default::default() },
        ];
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable { keys: true, ..Default::default() });
        let left = |chart| {
            let image = render(chart);
            (0..960).find(|&x| image.pixel(x, 100) == [255, 0, 255, 255]).expect("plot fill")
        };
        let first = left(c.clone());
        let mut box_series = c.series[0].clone();
        box_series.name = "MM".into();
        box_series.kind = Some(ChartType::Column);
        box_series.fill = Some(Fill::None);
        c.series.push(box_series);
        // The longer name has a shorter box key, so its complete row remains narrower
        // than the first row. Mixing the maximum name with the other row's key is wrong.
        assert_eq!(left(c), first, "a narrower added row must not widen the table header");
    }

    #[test]
    fn marker_only_table_keys_use_the_same_slot_as_box_keys() {
        let mut c = placed_column();
        c.kind = ChartType::Line;
        let text = Txt { font: None, font_ea: None, font_cs: None, size: 10.0, spacing: None, bold: false, italic: false, color: TEXT, rot: None };
        let mut s = c.series[0].clone();
        s.line = Some(Line::none());
        s.marker = Some("circle".into());
        let marker = table_key_width(&c, &s, &text);
        s.kind = Some(ChartType::Column);
        assert_eq!(table_key_width(&c, &s, &text), marker, "an invisible line must not reserve a long line key");
        s.kind = Some(ChartType::Line);
        s.line = Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 128, 0)), 1.0));
        assert!(table_key_width(&c, &s, &text) > marker, "a visible line keeps its longer key");
        s.kind = Some(ChartType::Stock);
        assert_eq!(table_key_width(&c, &s, &text), marker, "a connected stock series retains a compact table key");
    }

    #[test]
    fn auto_ranges_make_room_for_error_bars() {
        let mut c = Chart::sample(ChartType::Scatter);
        c.series.truncate(1);
        let s = &mut c.series[0];
        s.x_values = vec![Some(0.7), Some(1.8), Some(2.6)];
        s.values = vec![Some(2.7), Some(3.2), Some(0.8)];
        let bar = |dir: &str| ErrorBars { dir: Some(dir.into()), kind: "both".into(), val_type: "stdErr".into(), ..Default::default() };
        s.fmt.err_bars = vec![bar("y"), bar("x")];
        // Standard errors: y 1.266 / sqrt(3), x 0.954 / sqrt(3).
        let (_, hi) = value_range(&c, &find_axes(&c), false);
        assert!((hi - (3.2 + 0.7311)).abs() < 1e-3, "{hi}");
        let (lo, hi) = x_range(&c);
        assert!((lo - (0.7 - 0.5508)).abs() < 1e-3 && (hi - (2.6 + 0.5508)).abs() < 1e-3, "{lo} {hi}");
    }

    #[test]
    fn stacked_data_tables_list_series_from_the_top_down() {
        // A stacked column chart lists its series from the top of the stack down.
        let row_of = |kind: ChartType| -> Vec<u32> {
            let mut c = Chart::sample(kind);
            c.title = None;
            c.legend = None;
            c.fmt.data_table = Some(kition_slide_model::chart::DataTable { keys: true, ..Default::default() });
            let cols = [Rgba::rgb(255, 0, 0), Rgba::rgb(0, 255, 0), Rgba::rgb(0, 0, 255)];
            for (s, col) in c.series.iter_mut().zip(cols) {
                s.fill = Some(Fill::solid(ColorRef::rgb(col)));
            }
            let img = render(c);
            // Each key is its colour's leftmost pixel.
            cols.iter()
                .map(|col| {
                    let want = [col.r, col.g, col.b, 255];
                    (0..960u32).find_map(|x| (0..540u32).find(|&y| img.pixel(x, y) == want)).unwrap_or(0)
                })
                .collect()
        };
        let stacked = row_of(ChartType::StackedColumn);
        assert!(stacked[0] > stacked[1] && stacked[1] > stacked[2], "{stacked:?}");
        let clustered = row_of(ChartType::Column);
        assert!(clustered[0] < clustered[1] && clustered[1] < clustered[2], "{clustered:?}");
    }

    #[test]
    fn compound_lines_draw_parallel_bands() {
        let mut c = placed_column();
        c.series.truncate(1);
        c.series[0].values = vec![Some(2.0)];
        c.categories.truncate(1);
        c.series[0].fill = Some(Fill::None);
        let mut l = Line::solid(ColorRef::rgb(Rgba::BLACK), 9.0);
        l.compound = Some(kition_slide_model::style::Compound::Double);
        c.series[0].line = Some(l);
        let img = render(c);
        // Down the bar's left edge, across it: line, gap, line.
        let y = 420;
        let row: Vec<bool> = (100..400).map(|x| img.pixel(x, y)[0] < 100).collect();
        let runs = row.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(runs >= 4, "two dark bands along the edge: {runs} transitions");
    }

    #[test]
    fn gradient_compound_lines_keep_their_gap() {
        use kition_slide_model::style::{Gradient, GradientShape, GradientStop};
        let mut c = placed_column();
        c.series.truncate(1);
        c.series[0].values = vec![Some(2.0)];
        c.categories.truncate(1);
        c.series[0].fill = Some(Fill::None);
        let black = ColorRef::rgb(Rgba::BLACK);
        let mut l = Line::solid(black.clone(), 9.0);
        l.fill = Some(Fill::Gradient(Gradient {
            stops: vec![GradientStop { pos: 0.0, color: black.clone() }, GradientStop { pos: 1.0, color: black }],
            shape: GradientShape::Linear { angle: 0.0, scaled: false },
            tile_rect: None,
            rotate_with_shape: true,
        }));
        l.compound = Some(kition_slide_model::style::Compound::Double);
        c.series[0].line = Some(l);
        let img = render(c);
        let row: Vec<bool> = (100..400).map(|x| img.pixel(x, 420)[0] < 100).collect();
        let runs = row.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(runs >= 4, "two dark bands along the edge: {runs} transitions");
    }

    #[test]
    fn standard_error_bars_use_their_own_series() {
        let c = Chart::sample(ChartType::Line);
        let se = ErrorBars { kind: "both".into(), val_type: "stdErr".into(), ..Default::default() };
        assert_eq!(err_values(&c, &c.series[1], &se, false), c.series[1].values);
        let sd = ErrorBars { val_type: "stdDev".into(), ..se };
        assert_eq!(err_values(&c, &c.series[1], &sd, false).len(), 12);
    }

    #[test]
    fn up_down_bars_cover_a_line_charts_lines() {
        // Open low, a middle series with a big black marker, close high.
        let chart = |bars: bool| {
            let mut c = Chart::sample(ChartType::Line);
            c.title = None;
            c.legend = None;
            c.categories.truncate(1);
            for (s, v) in c.series.iter_mut().zip([1.0, 3.0, 5.0]) {
                s.values = vec![Some(v)];
                s.marker = Some("square".into());
                s.fmt.marker_size = Some(20.0);
                s.fmt.marker_fill = Some(Fill::solid(ColorRef::rgb(Rgba::BLACK)));
            }
            if bars {
                c.fmt.up_down_bars = Some((0.0, Some(Fill::solid(ColorRef::rgb(Rgba::WHITE))), None));
            }
            c
        };
        let dark =
            |img: &crate::Image| (0..img.width).flat_map(|x| (0..img.height).map(move |y| (x, y))).filter(|&(x, y)| img.pixel(x, y)[0] < 60).count();
        let (with, without) = (dark(&render(chart(true))), dark(&render(chart(false))));
        // The white up bar, drawn last, hides the middle marker.
        assert!(with + 200 < without, "{with} vs {without} dark pixels");
    }

    #[test]
    fn horizontal_bars_list_their_legend_bottom_up() {
        assert!(legend_bottom_up(&Chart::sample(ChartType::Bar)));
        assert!(!legend_bottom_up(&Chart::sample(ChartType::StackedBar)));
        assert!(!legend_bottom_up(&Chart::sample(ChartType::Column)));
        let mut c = Chart::sample(ChartType::Bar);
        c.fmt.axes = vec![
            Axis { id: 1, kind: AxisKind::Category, pos: "l".into(), reversed: true, cross_ax: 2, ..Default::default() },
            Axis { id: 2, kind: AxisKind::Value, pos: "b".into(), cross_ax: 1, ..Default::default() },
        ];
        for s in &mut c.series {
            s.fmt.axes = vec![1, 2];
        }
        assert!(!legend_bottom_up(&c), "a reversed category axis keeps the order");
    }

    #[test]
    fn three_d_pie_walls_are_lit_from_the_front_left() {
        let mut c = Chart::sample(ChartType::Pie);
        c.title = None;
        c.legend = None;
        c.fmt.three_d = true;
        c.fmt.view3d = Some((30.0, 0.0, 100.0));
        c.series.truncate(1);
        c.series[0].values = vec![Some(1.0)];
        c.categories.truncate(1);
        let img = render(c);
        let lit = |x: u32, y: u32| img.pixel(x, y)[..3].iter().map(|v| *v as u32).sum::<u32>();
        // The widest row is the disc's equator; the wall shows a few pixels above each column's
        // lowest drawn pixel.
        let width = |y: u32| (0..960).filter(|&x| lit(x, y) < 760).count();
        let eq = (0..540).max_by_key(|&y| width(y)).unwrap();
        let xs: Vec<u32> = (0..960).filter(|&x| lit(x, eq) < 760).collect();
        let (x0, x1) = (xs[0], *xs.last().unwrap());
        let r = (x1 - x0) / 2;
        let wall = |x: u32| lit(x, (0..540).rev().find(|&y| lit(x, y) < 760).unwrap() - 4);
        let (front_left, right_edge) = (wall(x0 + r - r / 4), wall(x1 - r / 8));
        assert!(front_left > right_edge * 2, "bright front {front_left}, dark right edge {right_edge}");
    }

    #[test]
    fn theme_override_colours_the_series() {
        let mut c = placed_column();
        let mut scheme = kition_slide_color::ColorScheme { name: "override".into(), colors: [Rgba::BLACK; 12] };
        scheme.set(SchemeSlot::Accent1, Rgba::rgb(0x12, 0xC0, 0x34));
        c.fmt.scheme = Some(scheme);
        c.series.truncate(1);
        let img = render(c);
        // The first bar (value 4.2 of a 0–4.5 axis) stands in the first category's middle.
        let px = img.pixel(195, 420);
        assert_eq!(&px[..3], &[0x12, 0xC0, 0x34], "{px:?}");
    }

    #[test]
    fn manual_inner_layout_places_the_plot_and_snaps_gridlines() {
        let mut c = placed_column();
        c.series.clear();
        c.fmt.axes = vec![
            Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), deleted: true, cross_ax: 2, ..Default::default() },
            Axis {
                id: 2,
                kind: AxisKind::Value,
                pos: "l".into(),
                deleted: true,
                max: Some(4.0),
                major_unit: Some(1.0),
                major_grid: Some(Line::solid(ColorRef::rgb(Rgba::rgb(0x80, 0x80, 0x80)), 0.75)),
                cross_ax: 1,
                ..Default::default()
            },
        ];
        let mut s = Series { name: "S".into(), values: vec![Some(1.0)], ..Default::default() };
        s.fmt.axes = vec![1, 2];
        s.fill = Some(Fill::None);
        c.series.push(s);
        let img = render(c);
        // Gridlines every 85 px from y = 440 up to y = 100, one crisp pixel row each.
        for y in [100u32, 185, 270, 355] {
            let row = img.pixel(480, y);
            assert!(row[0] < 0x90, "gridline at {y}: {row:?}");
            assert_eq!(img.pixel(480, y + 1), [255, 255, 255, 255], "below {y}");
        }
        // Nothing left of the plot area.
        assert_eq!(img.pixel(90, 270), [255, 255, 255, 255]);
    }

    #[test]
    fn manual_inner_plot_keeps_axis_titles_outside_the_plot() {
        let mut c = placed_column();
        c.series.truncate(1);
        c.series[0].values.clear();
        c.series[0].fmt.axes = vec![1, 2];
        c.fmt.axes = vec![
            Axis {
                id: 1,
                kind: AxisKind::Category,
                pos: "b".into(),
                cross_ax: 2,
                title: Some("Category axis".into()),
                title_text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(255, 0, 0))), ..Default::default() },
                ..Default::default()
            },
            Axis {
                id: 2,
                kind: AxisKind::Value,
                pos: "l".into(),
                cross_ax: 1,
                title: Some("Value axis".into()),
                title_text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(0, 128, 0))), ..Default::default() },
                ..Default::default()
            },
        ];
        let image = render(c);
        let mut title_pixels = [0; 2];
        for y in 0..540 {
            for x in 0..960 {
                let [r, g, b, _] = image.pixel(x, y);
                if r > 200 && g < 80 && b < 80 {
                    assert!(y > 440, "category title inside the inner plot: {x},{y}");
                    title_pixels[0] += 1;
                }
                if r < 80 && g > 100 && b < 80 {
                    assert!(x < 100, "value title inside the inner plot: {x},{y}");
                    title_pixels[1] += 1;
                }
            }
        }
        assert!(title_pixels.iter().all(|n| *n > 20), "both titles must be visible: {title_pixels:?}");
    }

    #[test]
    fn manual_plot_moves_with_its_table_without_changing_its_height() {
        let mut c = placed_column();
        c.fmt.plot_layout = Some(ManualLayout { inner: true, x: Some(0.2), y: Some(0.3), w: Some(0.7), h: Some(0.6), x_edge: true, y_edge: true });
        c.plot_fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 255))));
        c.fmt.axes = vec![
            Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), deleted: true, cross_ax: 2, ..Default::default() },
            Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), deleted: true, cross_ax: 1, ..Default::default() },
        ];
        for series in &mut c.series {
            series.fill = Some(Fill::None);
            series.line = Some(Line::none());
            series.fmt.axes = vec![1, 2];
        }
        let small_chart = |c| {
            let mut p = slide_file(c);
            std::sync::Arc::make_mut(&mut p.slides[0]).shapes[0].xfrm = Some(Xfrm::new(0.0, 0.0, 480.0, 270.0));
            render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() })
        };
        let before = small_chart(c.clone());
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable {
            outline: true,
            horizontal: true,
            vertical: true,
            line: Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 1.0)),
            ..Default::default()
        });
        let after = small_chart(c);
        let plot_rows = |image: &crate::Image| (0..540).filter(|&y| image.pixel(240, y) == [255, 0, 255, 255]).collect::<Vec<_>>();
        let (old_rows, new_rows) = (plot_rows(&before), plot_rows(&after));
        assert!(old_rows.len() > 150 && old_rows.len().abs_diff(new_rows.len()) <= 1, "the plot must retain its height");
        assert!(new_rows.first() < old_rows.first(), "an overflowing table must move its plot upward");
        let blue = |x, y| {
            let [r, g, b, _] = after.pixel(x, y);
            b > 200 && r < 80 && g < 80
        };
        assert!((200..270).any(|y| (0..480).filter(|&x| blue(x, y)).count() > 200), "the table border must remain visible");
        assert!(!(270..340).any(|y| (0..480).any(|x| blue(x, y))), "the table must fit inside the chart frame");
    }

    #[test]
    fn manual_plot_with_room_for_table_preserves_title_and_legend() {
        let mut c = placed_column();
        c.title = Some("Chart title".into());
        c.legend = Some("b".into());
        let before = render(c.clone());
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable { outline: true, ..Default::default() });
        let after = render(c);
        // This table fits below the requested plot and above the bottom legend. Neither
        // the plot, its heading nor its non-overlay legend should move to make room.
        for y in (0..440).chain(510..540) {
            for x in 0..960 {
                assert_eq!(after.pixel(x, y), before.pixel(x, y), "unchanged plot/title/legend at {x},{y}");
            }
        }
        assert!((510..540).any(|y| (200..760).any(|x| after.pixel(x, y) != [255, 255, 255, 255])), "the legend must remain visible");
    }

    #[test]
    fn manual_plot_axis_title_follows_table_when_the_chart_has_spare_height() {
        let mut c = placed_column();
        c.fmt.plot_layout.as_mut().expect("manual layout").h = Some(100.0 / 540.0);
        c.series.truncate(1);
        c.series[0].values.clear();
        c.series[0].fmt.axes = vec![1, 2];
        c.fmt.axes = vec![
            Axis {
                id: 1,
                kind: AxisKind::Category,
                pos: "b".into(),
                cross_ax: 2,
                title: Some("Category axis".into()),
                title_text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(255, 0, 0))), ..Default::default() },
                ..Default::default()
            },
            Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), deleted: true, cross_ax: 1, ..Default::default() },
        ];
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable {
            outline: true,
            horizontal: true,
            line: Some(Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)), 1.0)),
            ..Default::default()
        });
        let image = render(c);
        let table_bottom = (0..540)
            .rfind(|&y| {
                let [r, g, b, _] = image.pixel(480, y);
                b > 200 && r < 80 && g < 80
            })
            .expect("table border");
        let mut count = 0;
        for y in 0..540 {
            for x in 0..960 {
                let [r, g, b, _] = image.pixel(x, y);
                if r > 200 && g < 80 && b < 80 {
                    assert!(y > table_bottom && y < table_bottom + 35, "title must follow table at {table_bottom}, got {y}");
                    count += 1;
                }
            }
        }
        assert!(count > 20, "the title must be visible");
    }

    #[test]
    fn text_date_axis_uses_one_header_but_numeric_dates_keep_axis_labels() {
        let mut c = placed_column();
        c.categories = vec!["Jan".into(), "Feb".into(), "Mar".into()];
        c.series.truncate(1);
        c.series[0].values.clear();
        c.series[0].fmt.axes = vec![1, 2];
        c.fmt.axes = vec![
            Axis {
                id: 1,
                kind: AxisKind::Date,
                pos: "b".into(),
                cross_ax: 2,
                text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(255, 0, 0))), ..Default::default() },
                ..Default::default()
            },
            Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), deleted: true, cross_ax: 1, ..Default::default() },
        ];
        c.fmt.data_table = Some(kition_slide_model::chart::DataTable { outline: true, ..Default::default() });
        let label_bands = |chart| {
            let image = render(chart);
            let mut bands = 0;
            let mut previous = false;
            for y in 440..540 {
                let has_label = (100..860).any(|x| {
                    let [r, g, b, _] = image.pixel(x, y);
                    r > 200 && g < 80 && b < 80
                });
                if has_label && !previous {
                    bands += 1;
                }
                previous = has_label;
            }
            bands
        };
        assert_eq!(label_bands(c.clone()), 1, "text categories belong to a single visible table header");
        c.categories = vec!["46023".into(), "46024".into(), "46025".into()];
        c.fmt.cat_format = Some("yyyy/m/d".into());
        assert_eq!(label_bands(c), 2, "numeric dates retain separate axis labels and table header");
    }

    #[test]
    fn manual_label_factor_offsets_survive_explicit_box_sizes() {
        let l = ManualLayout { x: Some(0.1), y: Some(-0.1), w: Some(0.2), h: Some(0.1), ..Default::default() };
        let at = Point::new(400.0, 300.0);
        let r = data_label_rect(at, (0.0, 0.0), (20.0, 10.0), Some(&l), (1000.0, 500.0));
        assert_eq!(r, Rect::new(400.0, 225.0, 600.0, 275.0));
        let edge = ManualLayout { x_edge: true, y_edge: true, y: Some(0.2), ..l };
        assert_eq!(data_label_rect(at, (0.0, 0.0), (20.0, 10.0), Some(&edge), (1000.0, 500.0)), Rect::new(100.0, 100.0, 300.0, 150.0));
    }

    #[test]
    fn invalid_label_layout_keeps_automatic_bounds_exactly() {
        let at = Point::new(123.456789, 234.567891);
        let dir = (0.7, -0.3);
        let size = (21.23456, 11.34567);
        let cx = at.x + dir.0 * (size.0 / 2.0 + 2.0);
        let cy = at.y + dir.1 * (size.1 / 2.0 + 2.0);
        let expected = Rect::new(cx - size.0 / 2.0, cy - size.1 / 2.0, cx + size.0 / 2.0, cy + size.1 / 2.0);
        assert_eq!(data_label_rect(at, dir, size, None, (960.0, 540.0)), expected);
        for l in [
            ManualLayout { x: Some(f64::NAN), ..Default::default() },
            ManualLayout { x: Some(-0.1), x_edge: true, ..Default::default() },
            ManualLayout { y: Some(f64::INFINITY), ..Default::default() },
            ManualLayout { w: Some(2.0), ..Default::default() },
        ] {
            assert_eq!(data_label_rect(at, dir, size, Some(&l), (960.0, 540.0)), expected);
        }
        let l = ManualLayout { x: Some(0.95), y: Some(0.95), x_edge: true, y_edge: true, w: Some(0.2), h: Some(0.2), ..Default::default() };
        assert_eq!(data_label_rect(at, dir, size, Some(&l), (1000.0, 500.0)), Rect::new(800.0, 400.0, 1000.0, 500.0));
    }

    #[test]
    fn manual_label_offsets_move_rendered_text_by_chart_fractions() {
        let mut c = placed_column();
        c.series.truncate(1);
        c.categories.truncate(1);
        c.series[0].values = vec![Some(1.0)];
        c.series[0].fmt.labels = Some(DataLabels {
            value: true,
            pos: Some("ctr".into()),
            text: ChartText { color: Some(ColorRef::rgb(Rgba::rgb(255, 0, 0))), ..Default::default() },
            ..Default::default()
        });
        let bounds = |chart| {
            let img = render(chart);
            let mut pixels = vec![];
            for y in 0..img.height {
                for x in 0..img.width {
                    let [r, g, b, _] = img.pixel(x, y);
                    if r > 200 && g < 80 && b < 80 {
                        pixels.push((x, y));
                    }
                }
            }
            assert!(!pixels.is_empty());
            (pixels.iter().map(|p| p.0).min().unwrap(), pixels.iter().map(|p| p.1).min().unwrap())
        };
        for position in ["ctr", "outEnd"] {
            let labels = c.series[0].fmt.labels.as_mut().unwrap();
            labels.pos = Some(position.into());
            labels.layout = None;
            let before = bounds(c.clone());
            c.series[0].fmt.labels.as_mut().unwrap().layout = Some(ManualLayout { x: Some(0.1), y: Some(0.1), ..Default::default() });
            let after = bounds(c.clone());
            assert_eq!(after, (before.0 + 96, before.1 + 54), "{position}");
        }
    }

    #[test]
    fn best_fit_offsets_keep_automatic_pixels_and_preserve_authored_sizes() {
        let mut c = Chart::sample(ChartType::Pie);
        c.series[0].fmt.labels = Some(DataLabels { value: true, pos: Some("bestFit".into()), ..Default::default() });
        let before = render(c.clone());
        let authored = ManualLayout { x: Some(0.1), y: Some(-0.1), ..Default::default() };
        c.series[0].fmt.labels.as_mut().unwrap().layout = Some(authored.clone());
        assert_eq!(render(c.clone()).pixels, before.pixels);
        let labels = c.series[0].fmt.labels.as_mut().unwrap();
        assert_eq!(labels.layout.as_ref(), Some(&authored));
        assert!(label_layout_for_draw(labels).is_none());
        labels.layout.as_mut().unwrap().w = Some(0.2);
        labels.layout.as_mut().unwrap().h = Some(0.1);
        let drawn = label_layout_for_draw(labels).unwrap();
        assert_eq!((drawn.x, drawn.y, drawn.w, drawn.h), (None, None, Some(0.2), Some(0.1)));
    }

    #[test]
    fn every_kind_and_marker_draws_without_panicking() {
        let kinds = [
            ChartType::Column,
            ChartType::StackedBar,
            ChartType::PercentColumn,
            ChartType::Line,
            ChartType::LineMarkers,
            ChartType::StackedArea,
            ChartType::Pie,
            ChartType::Doughnut,
            ChartType::Scatter,
            ChartType::Bubble,
            ChartType::Radar,
            ChartType::FilledRadar,
            ChartType::Stock,
        ];
        for k in kinds {
            let mut c = Chart::sample(k);
            for (i, s) in c.series.iter_mut().enumerate() {
                s.marker = Some(["star", "x", "plus", "dot", "dash", "triangle", "diamond", "square", "circle"][i % 9].into());
                s.sizes = vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0)];
                s.fmt.labels = Some(DataLabels { value: true, category: true, pos: Some("outEnd".into()), ..Default::default() });
            }
            let img = render(c);
            assert!(img.pixels.as_chunks::<4>().0.iter().any(|p| p[0] < 200), "{k:?} drew nothing");
        }
        // Pie of pie, bar of pie and a 3-D pie.
        for of in ["pie", "bar"] {
            let mut c = Chart::sample(ChartType::Pie);
            c.fmt.of_pie = Some(of.into());
            render(c);
        }
        let mut c = Chart::sample(ChartType::Pie);
        c.fmt.three_d = true;
        c.fmt.view3d = Some((30.0, 0.0, 100.0));
        render(c);
        // Degenerate input: no values, NaN and huge values, zero-size frames.
        let mut c = Chart::sample(ChartType::Column);
        c.series[0].values = vec![Some(f64::NAN), Some(1e300), Some(-1e300), None];
        render(c);
        let mut p = slide_file(Chart::sample(ChartType::Line));
        std::sync::Arc::make_mut(&mut p.slides[0]).shapes[0].xfrm = Some(Xfrm::new(0.0, 0.0, 0.5, 0.5));
        render_slide(&p, 0, &RenderOpts { scale: 1.0, ..Default::default() });
    }

    #[test]
    fn point_marker_outline_inherits_unspecified_properties() {
        use kition_slide_model::chart::PointMarker;
        let mut series = Series::default();
        series.fmt.marker_line = Some(Line {
            width: Some(1.0),
            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)))),
            dash: Some(kition_slide_model::style::Dash::Preset { name: "dash".into() }),
            cap: Some(kition_slide_model::style::LineCap::Round),
            ..Default::default()
        });
        series.fmt.point_markers = vec![(0, PointMarker { line: Some(Line { width: Some(3.0), ..Default::default() }), ..Default::default() })];
        let effective = point_marker(&series, 0);
        let outline = effective.fmt.marker_line.as_ref().unwrap();
        assert_eq!(outline.width, Some(3.0));
        assert_eq!(outline.fill, series.fmt.marker_line.as_ref().unwrap().fill);
        assert_eq!(outline.dash, series.fmt.marker_line.as_ref().unwrap().dash);
        assert_eq!(outline.cap, series.fmt.marker_line.as_ref().unwrap().cap);
        series.fmt.point_markers[0].1.line = Some(Line::none());
        assert_eq!(point_marker(&series, 0).fmt.marker_line.as_ref().unwrap().fill, Some(Fill::None));
    }

    #[test]
    fn picture_markers_use_natural_aspect_and_explicit_size() {
        use kition_slide_model::style::PictureFill;
        use kition_slide_model::{MediaId, MediaItem};
        use std::{io::Cursor, sync::Arc};
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(20, 10, image::Rgba([255, 0, 0, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let render_marker = |size, points, legend| {
            let mut chart = placed_column();
            chart.kind = ChartType::Line;
            if legend {
                chart.legend = Some("b".into());
                chart.fmt.legend_text.size = Some(9.0);
            }
            chart.y_min = Some(0.0);
            chart.y_max = Some(4.0);
            chart.fmt.plot_layout.as_mut().unwrap().w = Some(600.0 / 960.0);
            chart.series.truncate(1);
            let series = &mut chart.series[0];
            series.kind = Some(ChartType::Line);
            series.values = vec![Some(2.0)];
            series.marker = Some("picture".into());
            series.fmt.marker_size = size;
            series.fmt.marker_fill = Some(Fill::Picture(PictureFill { media: MediaId(901), ..Default::default() }));
            series.fmt.marker_line = Some(Line::none());
            series.line = Some(Line::none());
            chart.categories.truncate(1);
            if points {
                use kition_slide_model::chart::PointMarker;
                chart.series[0].values = vec![Some(2.0); 3];
                chart.categories = vec!["First".into(), "Second".into(), "Third".into()];
                chart.series[0].fmt.point_markers = vec![
                    (
                        0,
                        PointMarker {
                            symbol: Some("square".into()),
                            size: Some(8.0),
                            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 0, 255)))),
                            ..Default::default()
                        },
                    ),
                    (1, PointMarker { symbol: Some("none".into()), ..Default::default() }),
                ];
            }
            let mut pres = slide_file(chart);
            pres.media.push(MediaItem {
                id: MediaId(901),
                name: "marker.png".into(),
                content_type: "image/png".into(),
                data: Arc::new(png.get_ref().clone()),
                link: None,
            });
            render_slide(&pres, 0, &RenderOpts { scale: 1.0, ..Default::default() })
        };
        for (size, want) in [(None, (20, 10)), (Some(12.0), (12, 6)), (Some(f64::NAN), (20, 10))] {
            let image = render_marker(size, false, false);
            let points: Vec<_> = (0..image.height)
                .flat_map(|y| (0..image.width).map(move |x| (x, y)))
                .filter(|(x, y)| image.pixel(*x, *y) == [255, 0, 0, 255])
                .collect();
            let x0 = points.iter().map(|p| p.0).min().expect("marker pixels");
            let x1 = points.iter().map(|p| p.0).max().unwrap();
            let y0 = points.iter().map(|p| p.1).min().unwrap();
            let y1 = points.iter().map(|p| p.1).max().unwrap();
            assert_eq!((x1 - x0 + 1, y1 - y0 + 1), want);
        }
        let image = render_marker(None, true, false);
        let count =
            |color| (0..image.height).flat_map(|y| (0..image.width).map(move |x| (x, y))).filter(|(x, y)| image.pixel(*x, *y) == color).count();
        assert_eq!(count([255, 0, 0, 255]), 200, "only the third point inherits the picture");
        assert_eq!(count([0, 0, 255, 255]), 64, "the first point overrides size, symbol, and fill");
        let image = render_marker(None, false, true);
        let key_pixels: Vec<_> = (440..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .filter(|(x, y)| image.pixel(*x, *y) == [255, 0, 0, 255])
            .collect();
        let width = key_pixels.iter().map(|p| p.0).max().expect("legend picture pixels") - key_pixels.iter().map(|p| p.0).min().unwrap() + 1;
        assert!((4..=5).contains(&width), "legend uses nominal size rather than the 20pt picture width: {width}");
    }

    #[test]
    fn marker_paths_have_their_size() {
        for sym in AUTO_MARKERS {
            let (p, _) = marker_path(sym, Point::new(10.0, 10.0), 6.0);
            let b = p.bounding_box();
            assert!(b.width() <= 6.01 && b.height() <= 6.01 && b.width() > 0.5, "{sym}: {b:?}");
        }
    }
}
