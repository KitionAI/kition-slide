//! Charts: an embedded data grid plus type and element options.

use serde::{Deserialize, Serialize};

use kition_slide_color::ColorScheme;

use crate::style::{ColorRef, Effects, Fill, Glow, Line, Shadow};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    #[default]
    Column,
    StackedColumn,
    PercentColumn,
    Bar,
    StackedBar,
    PercentBar,
    Line,
    LineMarkers,
    StackedLine,
    Area,
    StackedArea,
    Pie,
    Doughnut,
    Scatter,
    Bubble,
    Radar,
    FilledRadar,
    Stock,
    Surface,
    Histogram,
    Pareto,
    BoxWhisker,
    Waterfall,
    Funnel,
    Treemap,
    Sunburst,
    Combo,
}

impl ChartType {
    pub const MAIN: [ChartType; 12] = [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Scatter,
        ChartType::Radar,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Histogram,
        ChartType::Treemap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ChartType::Column => "Clustered Column",
            ChartType::StackedColumn => "Stacked Column",
            ChartType::PercentColumn => "100% Stacked Column",
            ChartType::Bar => "Clustered Bar",
            ChartType::StackedBar => "Stacked Bar",
            ChartType::PercentBar => "100% Stacked Bar",
            ChartType::Line => "Line",
            ChartType::LineMarkers => "Line with Markers",
            ChartType::StackedLine => "Stacked Line",
            ChartType::Area => "Area",
            ChartType::StackedArea => "Stacked Area",
            ChartType::Pie => "Pie",
            ChartType::Doughnut => "Doughnut",
            ChartType::Scatter => "Scatter",
            ChartType::Bubble => "Bubble",
            ChartType::Radar => "Radar",
            ChartType::FilledRadar => "Filled Radar",
            ChartType::Stock => "Stock",
            ChartType::Surface => "Surface",
            ChartType::Histogram => "Histogram",
            ChartType::Pareto => "Pareto",
            ChartType::BoxWhisker => "Box and Whisker",
            ChartType::Waterfall => "Waterfall",
            ChartType::Funnel => "Funnel",
            ChartType::Treemap => "Treemap",
            ChartType::Sunburst => "Sunburst",
            ChartType::Combo => "Combo",
        }
    }
    pub fn is_bar_like(self) -> bool {
        matches!(
            self,
            ChartType::Column
                | ChartType::StackedColumn
                | ChartType::PercentColumn
                | ChartType::Bar
                | ChartType::StackedBar
                | ChartType::PercentBar
                | ChartType::Histogram
                | ChartType::Pareto
                | ChartType::Waterfall
        )
    }
    pub fn horizontal(self) -> bool {
        matches!(self, ChartType::Bar | ChartType::StackedBar | ChartType::PercentBar | ChartType::Funnel)
    }
    pub fn stacked(self) -> bool {
        matches!(
            self,
            ChartType::StackedColumn
                | ChartType::StackedBar
                | ChartType::StackedLine
                | ChartType::StackedArea
                | ChartType::PercentColumn
                | ChartType::PercentBar
        )
    }
    pub fn percent(self) -> bool {
        matches!(self, ChartType::PercentColumn | ChartType::PercentBar)
    }
}

/// A waterfall bar: where it starts and ends, and whether it rises, falls or is a total.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterfallStep {
    pub from: f64,
    pub to: f64,
    pub kind: WaterfallKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaterfallKind {
    Increase,
    Decrease,
    Total,
}

impl WaterfallKind {
    pub const ALL: [WaterfallKind; 3] = [WaterfallKind::Increase, WaterfallKind::Decrease, WaterfallKind::Total];
    pub fn label(self) -> &'static str {
        match self {
            WaterfallKind::Increase => "Increase",
            WaterfallKind::Decrease => "Decrease",
            WaterfallKind::Total => "Total",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
    /// X values for scatter/bubble.
    pub x_values: Vec<Option<f64>>,
    pub sizes: Vec<Option<f64>>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    /// Per-point fill overrides (pie slices).
    pub point_fills: Vec<(usize, Fill)>,
    pub data_labels: Option<bool>,
    pub smooth: bool,
    pub marker: Option<String>,
    /// Combo charts: this series' own type.
    pub kind: Option<ChartType>,
    /// Waterfall charts: the points that are totals (drawn from zero, not floating on the
    /// running total), by index.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub totals: Vec<usize>,
    pub secondary_axis: bool,
    /// Formatting read from the file the editor does not change (markers, labels, axes it uses).
    #[serde(skip_serializing_if = "SeriesFormat::is_default")]
    pub fmt: SeriesFormat,
}

impl Series {
    /// The bars of this series as a waterfall: each point floats from the running total to the
    /// running total plus its value; a total point stands from zero to its value, and the
    /// running total carries on from it. Blank points draw nothing.
    pub fn waterfall(&self) -> Vec<Option<WaterfallStep>> {
        let mut running = 0.0;
        self.values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let v = (*v).filter(|v| v.is_finite())?;
                Some(if self.totals.contains(&i) {
                    running = v;
                    WaterfallStep { from: 0.0, to: v, kind: WaterfallKind::Total }
                } else {
                    let from = running;
                    running += v;
                    WaterfallStep { from, to: running, kind: if v < 0.0 { WaterfallKind::Decrease } else { WaterfallKind::Increase } }
                })
            })
            .collect()
    }
}

/// Text properties of a chart element (`c:txPr` / a title's first run), in points and degrees.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartText {
    /// DrawingML typefaces for Latin, East Asian, and complex-script text.
    pub font: Option<String>,
    pub font_ea: Option<String>,
    pub font_cs: Option<String>,
    pub size: Option<f64>,
    /// Character spacing in points; explicit zero overrides inherited tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<ColorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glow: Option<Glow>,
    /// An authored effect list replaces the inherited list, even when empty.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub effects_explicit: bool,
    /// Text rotation (`bodyPr/@rot`), degrees; `None` when automatic.
    pub rot: Option<f64>,
    /// Vertical text (`bodyPr/@vert` other than `horz`).
    pub vertical: bool,
}

impl ChartText {
    /// Inherit one complete effect list; legacy nonempty lists also override.
    pub fn inherit_effects(&mut self, parent: &Self) {
        if !self.effects_explicit && self.shadow.is_none() && self.glow.is_none() {
            self.shadow = parent.shadow.clone();
            self.glow = parent.glow.clone();
            self.effects_explicit = parent.effects_explicit;
        }
    }
}

/// A manual layout (`c:manualLayout`): fractions of the chart's size. `edge` positions are from
/// the chart's top-left corner, the others (`factor`) offsets from the automatic position.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ManualLayout {
    /// The plot area's layout names its inner rectangle (without tick labels).
    pub inner: bool,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub x_edge: bool,
    pub y_edge: bool,
}

/// Which data labels show, where and how (`c:dLbls`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataLabels {
    pub value: bool,
    pub category: bool,
    pub series: bool,
    pub percent: bool,
    pub bubble_size: bool,
    pub legend_key: bool,
    /// `outEnd`, `inEnd`, `ctr`, `inBase`, `t`, `b`, `l`, `r`, `bestFit`.
    pub pos: Option<String>,
    /// Authored position from `c:layout` and optional text-box size from `c15:layout`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<ManualLayout>,
    pub num_fmt: Option<String>,
    pub separator: Option<String>,
    pub text: ChartText,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    /// Points whose label is deleted (`c:dLbl` with `c:delete`).
    pub deleted: Vec<usize>,
    /// Points whose label the file formats on its own (`c:dLbl`).
    pub points: Vec<(usize, DataLabels)>,
}

impl DataLabels {
    pub fn any(&self) -> bool {
        self.value || self.category || self.series || self.percent || self.bubble_size || self.points.iter().any(|(_, p)| p.any())
    }
    /// The labels of point `i`: its own when the file gives them.
    pub fn point(&self, i: usize) -> Option<&DataLabels> {
        if self.deleted.contains(&i) {
            return None;
        }
        let own = self.points.iter().find(|(k, _)| *k == i).map(|(_, p)| p);
        match own {
            Some(p) => p.any().then_some(p),
            None => (self.value || self.category || self.series || self.percent || self.bubble_size).then_some(self),
        }
    }
}

/// Marker properties that a single data point overrides (`c:dPt/c:marker`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PointMarker {
    /// An authored effect list; an empty list clears inherited marker effects.
    pub effects: Option<Effects>,
    pub symbol: Option<String>,
    pub size: Option<f64>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeriesFormat {
    /// `c:idx`: the series' place in the automatic colour sequence.
    pub idx: Option<u32>,
    pub marker_size: Option<f64>,
    pub marker_fill: Option<Fill>,
    pub marker_line: Option<Line>,
    /// Marker effects are independent of the series line or body effects.
    pub marker_effects: Option<Effects>,
    pub point_markers: Vec<(usize, PointMarker)>,
    pub labels: Option<DataLabels>,
    /// Pie slice explosion, as a fraction of the radius (series and per point).
    pub explosion: f64,
    pub point_explosion: Vec<(usize, f64)>,
    /// Per-point line overrides.
    pub point_lines: Vec<(usize, Line)>,
    /// The axes the series' chart group plots against (`c:axId`).
    pub axes: Vec<u32>,
    pub invert_if_negative: bool,
    /// The number format of the series' values (`c:numCache/c:formatCode`).
    pub num_fmt: Option<String>,
    pub err_bars: Vec<ErrorBars>,
    pub trendlines: Vec<Trendline>,
    /// Outer shadow of the series' marks, and of single points.
    pub shadow: Option<Shadow>,
    pub point_shadows: Vec<(usize, Shadow)>,
}

/// Error bars of a series (`c:errBars`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ErrorBars {
    /// `x` or `y`; `None`: along the values.
    pub dir: Option<String>,
    /// `both`, `plus`, `minus`.
    pub kind: String,
    /// `fixedVal`, `percentage`, `stdDev`, `stdErr`, `cust`.
    pub val_type: String,
    pub val: f64,
    pub no_end_cap: bool,
    pub line: Option<Line>,
    pub plus: Vec<Option<f64>>,
    pub minus: Vec<Option<f64>>,
}

/// A trendline of a series (`c:trendline`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Trendline {
    /// `linear`, `exp`, `log`, `poly`, `power`, `movingAvg`.
    pub kind: String,
    pub order: Option<u32>,
    pub period: Option<u32>,
    pub forward: f64,
    pub backward: f64,
    pub intercept: Option<f64>,
    pub line: Option<Line>,
    /// The trendline's own name (`c:name`), shown in the legend.
    pub name: Option<String>,
}

impl SeriesFormat {
    pub fn is_default(&self) -> bool {
        *self == SeriesFormat::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisKind {
    #[default]
    Category,
    Value,
    Date,
    Series,
}

/// An axis as the file describes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Axis {
    pub id: u32,
    pub kind: AxisKind,
    /// `b`, `l`, `r`, `t`.
    pub pos: String,
    pub deleted: bool,
    pub reversed: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major_unit: Option<f64>,
    pub minor_unit: Option<f64>,
    pub log_base: Option<f64>,
    pub num_fmt: Option<String>,
    /// `none`, `out`, `in`, `cross`.
    pub major_tick: String,
    pub minor_tick: String,
    /// `nextTo`, `low`, `high`, `none`.
    pub label_pos: String,
    pub line: Option<Line>,
    pub major_grid: Option<Line>,
    pub minor_grid: Option<Line>,
    pub text: ChartText,
    pub title: Option<String>,
    pub title_text: ChartText,
    pub cross_ax: u32,
    /// `autoZero`, `min`, `max`.
    pub crosses: Option<String>,
    pub crosses_at: Option<f64>,
    /// Value axes: categories sit between ticks (`between`) or on them (`midCat`).
    pub between: Option<bool>,
    pub label_skip: Option<u32>,
    pub label_offset: Option<f64>,
}

/// A data table under the plot area (`c:dTable`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataTable {
    pub horizontal: bool,
    pub vertical: bool,
    pub outline: bool,
    pub keys: bool,
    pub line: Option<Line>,
    pub text: ChartText,
}

/// Chart formatting read from the file that the editor does not change: layout, axes, text,
/// borders, labels and the colour scheme a theme override gives the chart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartFormat {
    /// The chart's theme override colour scheme (`themeOverride` part).
    pub scheme: Option<ColorScheme>,
    pub rounded: bool,
    pub chart_line: Option<Line>,
    pub plot_line: Option<Line>,
    /// Defaults for every text (`c:chartSpace/c:txPr`).
    pub text: ChartText,
    pub title_text: ChartText,
    pub title_layout: Option<ManualLayout>,
    pub title_overlay: bool,
    pub title_fill: Option<Fill>,
    pub title_line: Option<Line>,
    pub legend_text: ChartText,
    pub legend_layout: Option<ManualLayout>,
    pub legend_overlay: bool,
    pub legend_fill: Option<Fill>,
    pub legend_line: Option<Line>,
    /// Legend entries deleted (`c:legendEntry` with `c:delete`).
    pub legend_deleted: Vec<usize>,
    pub plot_layout: Option<ManualLayout>,
    pub axes: Vec<Axis>,
    /// Group-level data labels (`c:dLbls` of the chart group).
    pub labels: Option<DataLabels>,
    pub drop_lines: Option<Line>,
    pub hi_low_lines: Option<Line>,
    /// Up and down bars of a stock chart: (gap width, up fill, down fill).
    pub up_down_bars: Option<(f64, Option<Fill>, Option<Fill>)>,
    pub up_down_lines: (Option<Line>, Option<Line>),
    pub ser_lines: Option<Line>,
    pub first_slice_angle: f64,
    /// `c:scatterStyle`.
    pub scatter_style: Option<String>,
    pub blanks_as_zero: bool,
    pub data_table: Option<DataTable>,
    /// Bar-of-pie / pie-of-pie: `bar` or `pie`, the second plot's size (%) and split position.
    pub of_pie: Option<String>,
    pub second_pie_size: Option<f64>,
    pub split_pos: Option<f64>,
    pub split_type: Option<String>,
    pub radar_style: Option<String>,
    /// The chart is drawn in 3-D (`c:view3D` with a 3-D chart group).
    pub three_d: bool,
    pub bubble_scale: Option<f64>,
    /// The number format of numeric categories (dates…), from the category cache.
    pub cat_format: Option<String>,
    /// The 3-D view (`c:view3D`): rotation about x and y (degrees) and depth (%).
    pub view3d: Option<(f64, f64, f64)>,
    /// Outer shadows of the chart and plot areas.
    pub chart_shadow: Option<Shadow>,
    pub plot_shadow: Option<Shadow>,
}

/// A change to chart text from the text formatting commands: the fields given are set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartTextChange {
    pub font: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<ColorRef>,
}

impl ChartTextChange {
    pub fn is_empty(&self) -> bool {
        *self == ChartTextChange::default()
    }
}

impl Chart {
    /// Format all the chart's text, as PowerPoint does with the chart selected: the chart's
    /// default text takes the change, and so do the title, legend, axes and data labels that
    /// set their own. A size keeps their proportions to the default (the title stays larger).
    pub fn format_text(&mut self, change: &ChartTextChange) {
        let old_size = self.fmt.text.size.unwrap_or(10.0);
        let ratio = change.size.map(|z| z / old_size.max(0.1));
        let apply = |t: &mut ChartText, base: bool| {
            if let Some(f) = &change.font {
                t.font = Some(f.clone());
            }
            if let Some(b) = change.bold {
                t.bold = Some(b);
            }
            if let Some(i) = change.italic {
                t.italic = Some(i);
            }
            if let Some(c) = &change.color {
                t.color = Some(c.clone());
            }
            match (base, change.size, ratio) {
                (true, Some(z), _) => t.size = Some(z),
                (false, _, Some(r)) => t.size = t.size.map(|z| (z * r).clamp(1.0, 400.0)),
                _ => {}
            }
        };
        apply(&mut self.fmt.text, true);
        if change.color.is_some() {
            self.text_color = None;
        }
        apply(&mut self.fmt.title_text, false);
        apply(&mut self.fmt.legend_text, false);
        if let Some(l) = self.fmt.labels.as_mut() {
            apply(&mut l.text, false);
        }
        for a in &mut self.fmt.axes {
            apply(&mut a.text, false);
            apply(&mut a.title_text, false);
        }
        for s in &mut self.series {
            if let Some(l) = s.fmt.labels.as_mut() {
                apply(&mut l.text, false);
                for (_, p) in &mut l.points {
                    apply(&mut p.text, false);
                }
            }
        }
    }
}

impl ChartFormat {
    pub fn is_default(&self) -> bool {
        *self == ChartFormat::default()
    }
    pub fn axis(&self, id: u32) -> Option<&Axis> {
        self.axes.iter().find(|a| a.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartType,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub legend: Option<String>,
    pub data_labels: bool,
    pub gridlines: bool,
    pub axis_titles: (Option<String>, Option<String>),
    /// Chart style number (1–48) and colour palette (`colorful1`, `monochrome3`…).
    pub style: u32,
    pub palette: String,
    pub plot_fill: Option<Fill>,
    pub chart_fill: Option<Fill>,
    pub text_color: Option<ColorRef>,
    pub hole_size: f64,
    pub gap_width: f64,
    pub overlap: f64,
    pub vary_colors: bool,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    #[serde(skip_serializing_if = "ChartFormat::is_default")]
    pub fmt: ChartFormat,
    /// Original chart XML part kept for round-trip of unmodelled options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// The parts the chart part's relationships name, carried with it: its style and colour style
    /// parts (`chartStyle`, `chartColorStyle`) and its embedded workbook (`package`, named by the
    /// kept part's `c:externalData`, so written only while the chart is unchanged).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rels: Vec<crate::KeptRel>,
}

impl Default for Chart {
    fn default() -> Self {
        Chart {
            kind: ChartType::Column,
            title: Some("Chart Title".into()),
            categories: vec![],
            series: vec![],
            legend: Some("b".into()),
            data_labels: false,
            gridlines: true,
            axis_titles: (None, None),
            style: 2,
            palette: "colorful1".into(),
            plot_fill: None,
            chart_fill: None,
            text_color: None,
            hole_size: 0.5,
            gap_width: 2.19,
            overlap: -0.27,
            vary_colors: false,
            y_min: None,
            y_max: None,
            fmt: ChartFormat::default(),
            raw: None,
            rels: vec![],
        }
    }
}

impl Chart {
    /// The axis a chart made without axes of its own is drawn with (new charts in the editor):
    /// a value axis on the side has no line and carries the gridlines; a category axis a light line.
    pub fn default_axis(&self, kind: AxisKind, pos: &str, id: u32, cross: u32) -> Axis {
        let grid = ColorRef::rgb(kition_slide_color::Rgba::rgb(0xD9, 0xD9, 0xD9));
        let is_val = kind == AxisKind::Value && (!matches!(pos, "b" | "t") || self.kind.horizontal());
        Axis {
            id,
            kind,
            pos: pos.into(),
            major_tick: "none".into(),
            minor_tick: "none".into(),
            label_pos: "nextTo".into(),
            line: Some(if is_val { Line::none() } else { Line::solid(grid.clone(), 0.75) }),
            major_grid: (is_val && self.gridlines).then(|| Line::solid(grid, 0.75)),
            min: if is_val { self.y_min } else { None },
            max: if is_val { self.y_max } else { None },
            cross_ax: cross,
            // Tick labels are 9 pt against the default 10 pt chart text, and follow it.
            text: ChartText { size: Some(self.fmt.text.size.map_or(9.0, |z| (z * 0.9 * 10.0).round() / 10.0)), ..Default::default() },
            title: if is_val { self.axis_titles.1.clone() } else { self.axis_titles.0.clone() },
            ..Default::default()
        }
    }

    /// The ids of the axes the first series is drawn on: (category or X, value), as the renderer
    /// finds them; `None` for a chart without axes of its own.
    pub fn main_axes(&self) -> Option<(u32, u32)> {
        let horiz = self.kind.horizontal();
        let xy = matches!(self.kind, ChartType::Scatter | ChartType::Bubble);
        let ids = self.series.first().map(|s| s.fmt.axes.clone()).unwrap_or_default();
        let [a, b, ..] = ids.as_slice() else { return None };
        let (x, y) = (self.fmt.axis(*a)?, self.fmt.axis(*b)?);
        let a_is_x = if xy { matches!(x.pos.as_str(), "b" | "t") != horiz } else { x.kind != AxisKind::Value || y.kind == AxisKind::Value };
        Some(if a_is_x { (*a, *b) } else { (*b, *a) })
    }

    /// The chart's main axes, made from the defaults when it has none (so they can be formatted):
    /// (category or X, value) ids.
    pub fn ensure_axes(&mut self) -> (u32, u32) {
        if let Some(ids) = self.main_axes() {
            return ids;
        }
        let horiz = self.kind.horizontal();
        let xy = matches!(self.kind, ChartType::Scatter | ChartType::Bubble);
        let (cp, vp) = if horiz { ("l", "b") } else { ("b", "l") };
        let cat = self.default_axis(if xy { AxisKind::Value } else { AxisKind::Category }, cp, 1, 2);
        let val = self.default_axis(AxisKind::Value, vp, 2, 1);
        self.fmt.axes = vec![cat, val];
        for s in &mut self.series {
            s.fmt.axes = vec![1, 2];
        }
        (1, 2)
    }

    pub fn axis_mut(&mut self, id: u32) -> Option<&mut Axis> {
        self.fmt.axes.iter_mut().find(|a| a.id == id)
    }

    /// The sample data a new chart starts with (our own numbers).
    pub fn sample(kind: ChartType) -> Self {
        let cats = ["Q1", "Q2", "Q3", "Q4"];
        let data = [[4.2, 2.6, 2.1], [2.8, 4.1, 2.4], [3.6, 1.9, 3.3], [4.7, 3.0, 5.1]];
        let mut c = Chart { kind, categories: cats.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let n_series =
            if matches!(kind, ChartType::Pie | ChartType::Doughnut | ChartType::Funnel | ChartType::Waterfall | ChartType::Treemap) { 1 } else { 3 };
        for s in 0..n_series {
            c.series.push(Series {
                name: format!("Series {}", s + 1),
                values: data.iter().map(|row| row.get(s).copied()).collect(),
                x_values: if kind == ChartType::Scatter { (1..=4).map(|x| Some(x as f64)).collect() } else { vec![] },
                ..Default::default()
            });
        }
        c.vary_colors = n_series == 1 && kind != ChartType::Funnel;
        c.set_combo_defaults();
        c
    }
    /// A combination chart's series without a type of their own take PowerPoint's default
    /// combination, Clustered Column – Line: the last series is a line, the others columns.
    pub fn set_combo_defaults(&mut self) {
        if self.kind != ChartType::Combo {
            return;
        }
        let n = self.series.len();
        for (i, s) in self.series.iter_mut().enumerate() {
            if s.kind.is_none() || s.kind == Some(ChartType::Combo) {
                s.kind = Some(if n > 1 && i + 1 == n { ChartType::Line } else { ChartType::Column });
            }
        }
    }
    /// Value range across series (stacked sums for stacked types).
    pub fn value_range(&self) -> (f64, f64) {
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 0.0;
        let n = self.categories.len().max(self.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        if self.kind == ChartType::Waterfall {
            for step in self.series.iter().flat_map(Series::waterfall).flatten() {
                hi = hi.max(step.from.max(step.to));
                lo = lo.min(step.from.min(step.to));
            }
        } else if self.kind.stacked() {
            for i in 0..n {
                let (mut p, mut m) = (0.0, 0.0);
                for s in &self.series {
                    let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                    if v >= 0.0 { p += v } else { m += v }
                }
                hi = hi.max(p);
                lo = lo.min(m);
            }
        } else {
            for s in &self.series {
                for v in s.values.iter().flatten() {
                    if v.is_finite() {
                        hi = hi.max(*v);
                        lo = lo.min(*v);
                    }
                }
            }
        }
        if self.kind.percent() {
            return (0.0, 1.0);
        }
        (self.y_min.unwrap_or(lo), self.y_max.unwrap_or(hi))
    }
}

/// "Nice" axis scale: (min, max, major step) covering [lo, hi].
pub fn nice_scale(lo: f64, hi: f64) -> (f64, f64, f64) {
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
    let span = if hi - lo < 1e-12 { hi.abs().max(1.0) } else { hi - lo };
    let raw = span / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    let min = (lo / step).floor() * step;
    let max = (hi / step).ceil() * step;
    let max = if (max - min).abs() < 1e-12 { min + step } else { max };
    (min, max, step)
}

/// An automatic value axis scale like the spreadsheet reference renderers draw: the minimum is
/// zero unless every value sits far from it, the maximum the largest value plus 5% of the span,
/// rounded up to a major unit of 1, 2 or 5 × 10ⁿ chosen so the axis has at most `max_steps`
/// intervals. Fixed ends (`min`/`max`) and unit (`unit`) are kept. Returns (min, max, major).
pub fn auto_scale(lo: f64, hi: f64, min: Option<f64>, max: Option<f64>, unit: Option<f64>, max_steps: f64) -> (f64, f64, f64) {
    auto_scale_with_headroom(lo, hi, min, max, unit, max_steps, 0.05)
}

/// The automatic axis calculation with configurable upper headroom before nice-unit rounding.
/// Authored bounds and units remain authoritative. Lower-bound handling is unchanged.
pub fn auto_scale_with_headroom(
    lo: f64,
    hi: f64,
    min: Option<f64>,
    max: Option<f64>,
    unit: Option<f64>,
    max_steps: f64,
    upper_headroom: f64,
) -> (f64, f64, f64) {
    let upper_headroom = if upper_headroom.is_finite() { upper_headroom.clamp(0.0, 1.0) } else { 0.05 };
    let (mut lo, mut hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
    if let Some(m) = min {
        lo = lo.max(m);
        hi = hi.max(m);
    }
    if let Some(m) = max {
        hi = hi.min(m);
        lo = lo.min(m);
    }
    // Where the axis starts before rounding.
    let mut a = if let Some(m) = min {
        m
    } else if lo >= 0.0 {
        if hi > lo && hi > 0.0 && (hi - lo) / hi < 1.0 / 6.0 { lo - (hi - lo) / 2.0 } else { 0.0 }
    } else {
        lo
    };
    let mut b = if let Some(m) = max {
        m
    } else if hi > 0.0 || (lo >= 0.0 && hi == 0.0) {
        hi + (hi - a) * upper_headroom
    } else if hi < 0.0 && hi > -((lo - hi) / 6.0) {
        0.0
    } else {
        hi
    };
    if min.is_none() && a < 0.0 {
        // Negative values: extend by 5% of the span as well.
        a -= (b - a).abs() * 0.05;
    }
    if (b - a).abs() < 1e-12 {
        if b == 0.0 {
            b = 1.0;
        } else if b > 0.0 {
            a = a.min(0.0);
            b = if a == b { b * 2.0 } else { b };
        } else {
            b = 0.0;
        }
    }
    let span = (b - a).abs().max(1e-12);
    let steps = max_steps.clamp(1.0, 1000.0);
    let unit = unit.filter(|u| *u > 0.0 && span / *u <= 10_000.0).unwrap_or_else(|| {
        let mut u = 10f64.powf((span / steps).log10().floor());
        loop {
            let mut found = None;
            for f in [1.0, 2.0, 5.0] {
                let c = u * f;
                let n = (if max.is_some() { b } else { (b / c - 1e-9).ceil() * c } - if min.is_some() { a } else { (a / c + 1e-9).floor() * c }) / c;
                if n <= steps + 1e-9 {
                    found = Some(c);
                    break;
                }
            }
            if let Some(c) = found {
                break c;
            }
            u *= 10.0;
            if !u.is_finite() {
                break span;
            }
        }
    });
    let lo_out = if min.is_some() { a } else { (a / unit + 1e-9).floor() * unit };
    let hi_out = if max.is_some() { b } else { (b / unit - 1e-9).ceil() * unit };
    let hi_out = if hi_out <= lo_out { lo_out + unit } else { hi_out };
    (lo_out, hi_out, unit)
}

/// The amounts an error bar extends below and above a point with value `v`, given the values
/// the statistics run over (`values`: every value of the chart's series for standard deviation
/// bars, which sit on their mean; the bar's own series for standard error bars).
pub fn error_amounts(e: &ErrorBars, values: &[Option<f64>], i: usize, v: f64) -> (f64, f64, Option<f64>) {
    let vals: Vec<f64> = values.iter().flatten().copied().filter(|v| v.is_finite()).collect();
    let n = vals.len() as f64;
    let (minus, plus, centre) = match e.val_type.as_str() {
        "percentage" => (v.abs() * e.val / 100.0, v.abs() * e.val / 100.0, None),
        "stdDev" => {
            let mean = vals.iter().sum::<f64>() / n.max(1.0);
            let sd = if n > 1.0 { (vals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
            (sd * e.val, sd * e.val, Some(mean))
        }
        // The standard error of the mean: the sample standard deviation over sqrt(n), as WPS
        // draws it (callers pass the series' own values).
        "stdErr" => {
            let mean = vals.iter().sum::<f64>() / n.max(1.0);
            let se = if n > 1.0 { (vals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n * (n - 1.0))).sqrt() } else { 0.0 };
            (se, se, None)
        }
        "cust" => (e.minus.get(i).copied().flatten().unwrap_or(0.0).abs(), e.plus.get(i).copied().flatten().unwrap_or(0.0).abs(), None),
        _ => (e.val.abs(), e.val.abs(), None),
    };
    match e.kind.as_str() {
        "plus" => (0.0, plus, centre),
        "minus" => (minus, 0.0, centre),
        _ => (minus, plus, centre),
    }
}

/// The trendline's value at `x`, fitted to the points `(x, y)`; `None` when it can't be fitted.
pub fn trend_fn(t: &Trendline, pts: &[(f64, f64)]) -> Option<Box<dyn Fn(f64) -> f64>> {
    let fit_linear = |pts: &[(f64, f64)], intercept: Option<f64>| -> Option<(f64, f64)> {
        let n = pts.len() as f64;
        if n < 2.0 {
            return None;
        }
        if let Some(b) = intercept {
            let sxx: f64 = pts.iter().map(|(x, _)| x * x).sum();
            let sxy: f64 = pts.iter().map(|(x, y)| x * (y - b)).sum();
            return (sxx.abs() > 1e-12).then(|| (sxy / sxx, b));
        }
        let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
        let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
        let sxx: f64 = pts.iter().map(|(x, _)| (x - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
        (sxx.abs() > 1e-12).then(|| (sxy / sxx, my - sxy / sxx * mx))
    };
    match t.kind.as_str() {
        "exp" => {
            let lp: Vec<(f64, f64)> = pts.iter().filter(|p| p.1 > 0.0).map(|(x, y)| (*x, y.ln())).collect();
            let (a, b) = fit_linear(&lp, t.intercept.filter(|v| *v > 0.0).map(f64::ln))?;
            Some(Box::new(move |x| (b + a * x).exp()))
        }
        "log" => {
            let lp: Vec<(f64, f64)> = pts.iter().filter(|p| p.0 > 0.0).map(|(x, y)| (x.ln(), *y)).collect();
            let (a, b) = fit_linear(&lp, None)?;
            Some(Box::new(move |x| if x > 0.0 { b + a * x.ln() } else { f64::NAN }))
        }
        "power" => {
            let lp: Vec<(f64, f64)> = pts.iter().filter(|p| p.0 > 0.0 && p.1 > 0.0).map(|(x, y)| (x.ln(), y.ln())).collect();
            let (a, b) = fit_linear(&lp, None)?;
            Some(Box::new(move |x| if x > 0.0 { (b + a * x.ln()).exp() } else { f64::NAN }))
        }
        "poly" => {
            let k = t.order.unwrap_or(2).clamp(2, 6) as usize;
            if pts.len() <= k {
                return None;
            }
            // Normal equations, solved by Gaussian elimination.
            let mut m = vec![vec![0.0; k + 2]; k + 1];
            for (x, y) in pts {
                for (r, row) in m.iter_mut().enumerate() {
                    for (c, cell) in row.iter_mut().take(k + 1).enumerate() {
                        *cell += x.powi((r + c) as i32);
                    }
                    row[k + 1] += y * x.powi(r as i32);
                }
            }
            for col in 0..=k {
                let piv = (col..=k).max_by(|a, b| m[*a][col].abs().total_cmp(&m[*b][col].abs()))?;
                m.swap(col, piv);
                let d = m[col][col];
                if d.abs() < 1e-12 {
                    return None;
                }
                let pivot = m[col].clone();
                for (r, row) in m.iter_mut().enumerate() {
                    if r != col {
                        let f = row[col] / d;
                        for (cell, p) in row.iter_mut().zip(&pivot).skip(col) {
                            *cell -= f * p;
                        }
                    }
                }
            }
            let coef: Vec<f64> = (0..=k).map(|r| m[r][k + 1] / m[r][r]).collect();
            Some(Box::new(move |x| coef.iter().enumerate().map(|(p, c)| c * x.powi(p as i32)).sum()))
        }
        "movingAvg" => None,
        _ => {
            let (a, b) = fit_linear(pts, t.intercept)?;
            Some(Box::new(move |x| b + a * x))
        }
    }
}

/// Formats a number with a spreadsheet number format code (the subset charts use: `General`,
/// digits and grouping, percentages, literals, dates). `None` means `General`.
pub fn format_number(v: f64, code: Option<&str>) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let code = code.unwrap_or("General");
    let sections: Vec<&str> = split_sections(code);
    let (sec, v) = match sections.as_slice() {
        [_, neg, ..] if v < 0.0 => (*neg, -v),
        [_, _, zero, ..] if v == 0.0 => (*zero, v),
        [first, ..] => (*first, v),
        [] => ("General", v),
    };
    let sec = strip_brackets(sec);
    if sec.trim().is_empty() || sec.eq_ignore_ascii_case("general") {
        return general(v);
    }
    if sec == "@" {
        return general(v);
    }
    if is_date_code(&sec) {
        return format_date(v, &sec);
    }
    format_digits(v, &sec)
}

fn split_sections(code: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let mut quoted = false;
    for (i, ch) in code.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

/// Drops `[$-804]`, `[Red]` and similar bracketed parts (a currency `[$¥-804]` keeps its symbol).
fn strip_brackets(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find(']') else {
            rest = "";
            break;
        };
        let inner = &rest[i + 1..i + j];
        if let Some(cur) = inner.strip_prefix('$') {
            out.push_str(cur.split('-').next().unwrap_or(""));
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

fn general(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-9..1e11).contains(&a) {
        let s = format!("{:.5E}", v);
        // 1.23450E7 → 1.2345E+07
        if let Some((m, e)) = s.split_once('E') {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            let e: i32 = e.parse().unwrap_or(0);
            return format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
        }
        return s;
    }
    // Ten significant digits at most.
    let digits = (10 - (a.log10().floor() as i32 + 1)).clamp(0, 10) as usize;
    let s = format!("{v:.digits$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

fn is_date_code(s: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            'y' | 'Y' | 'd' | 'D' if !quoted => return true,
            'm' | 'M' if !quoted && !s.contains('0') && !s.contains('#') => return true,
            _ => {}
        }
    }
    false
}

/// Days since 1899-12-30 (the 1900 date system as spreadsheets count it) → (year, month, day).
pub fn serial_date(v: f64) -> (i64, u32, u32) {
    let days = v.floor() as i64 - 25569; // → days since 1970-01-01
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn format_date(v: f64, code: &str) -> String {
    let (y, mo, d) = serial_date(v);
    let mut out = String::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let run = chars[i..].iter().take_while(|c| c.eq_ignore_ascii_case(&ch)).count();
        match ch.to_ascii_lowercase() {
            '"' => {
                let end = chars[i + 1..].iter().position(|c| *c == '"').map(|p| i + 1 + p).unwrap_or(chars.len());
                out.extend(&chars[i + 1..end]);
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(c) = chars.get(i + 1) {
                    out.push(*c);
                }
                i += 2;
                continue;
            }
            'y' => out.push_str(&if run <= 2 { format!("{:02}", y.rem_euclid(100)) } else { format!("{y}") }),
            'm' => out.push_str(&match run {
                1 => format!("{mo}"),
                2 => format!("{mo:02}"),
                3 => MONTHS.get(mo as usize - 1).map(|m| m[..3].to_string()).unwrap_or_default(),
                _ => MONTHS.get(mo as usize - 1).map(|m| m.to_string()).unwrap_or_default(),
            }),
            'd' => out.push_str(&if run == 1 { format!("{d}") } else { format!("{d:02}") }),
            '_' | '*' => {
                i += 2;
                continue;
            }
            '@' | ';' => {}
            _ => {
                for _ in 0..run {
                    out.push(ch);
                }
            }
        }
        i += run;
    }
    out
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn format_digits(v: f64, code: &str) -> String {
    // Split the code into literal prefix, the numeric pattern and a literal suffix.
    let chars: Vec<char> = code.chars().collect();
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut pattern = String::new();
    let mut seen_digit = false;
    let mut percent = 0;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let lit: Option<String> = match ch {
            '"' => {
                let end = chars[i + 1..].iter().position(|c| *c == '"').map(|p| i + 1 + p).unwrap_or(chars.len());
                let s: String = chars[i + 1..end.min(chars.len())].iter().collect();
                i = end + 1;
                Some(s)
            }
            '\\' => {
                let s = chars.get(i + 1).map(|c| c.to_string()).unwrap_or_default();
                i += 2;
                Some(s)
            }
            '_' => {
                i += 2;
                Some(" ".into())
            }
            '*' => {
                i += 2;
                Some(String::new())
            }
            '0' | '#' | '?' | '.' | ',' if ch != ',' || seen_digit => {
                seen_digit = true;
                pattern.push(ch);
                i += 1;
                None
            }
            '%' => {
                percent += 1;
                i += 1;
                Some("%".into())
            }
            'E' | 'e' if seen_digit => {
                pattern.push('E');
                i += 1;
                None
            }
            _ => {
                i += 1;
                Some(ch.to_string())
            }
        };
        if let Some(l) = lit {
            if seen_digit { suffix.push_str(&l) } else { prefix.push_str(&l) }
        }
    }
    if !seen_digit {
        return format!("{prefix}{suffix}");
    }
    let mut x = v * 100f64.powi(percent);
    let neg = x < 0.0;
    x = x.abs();
    let int_pat: &str = pattern.split('.').next().unwrap_or("");
    let frac_pat: &str = pattern.split_once('.').map(|(_, f)| f).unwrap_or("");
    // Trailing commas scale by 1000.
    let trailing = int_pat.len() - int_pat.trim_end_matches(',').len();
    x /= 1000f64.powi(trailing as i32);
    let group = int_pat.trim_end_matches(',').contains(',');
    let decimals = frac_pat.chars().filter(|c| matches!(c, '0' | '#' | '?')).count();
    let min_dec = frac_pat.chars().filter(|c| *c == '0').count();
    let min_int = int_pat.chars().filter(|c| *c == '0').count();
    let s = format!("{x:.decimals$}");
    let (ip, fp) = s.split_once('.').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((s.clone(), String::new()));
    let mut fp = fp;
    while fp.len() > min_dec && fp.ends_with('0') {
        fp.pop();
    }
    let mut ip = if ip == "0" && min_int == 0 { String::new() } else { ip };
    while ip.len() < min_int {
        ip.insert(0, '0');
    }
    if group && ip.len() > 3 {
        let mut g = String::new();
        for (k, ch) in ip.chars().enumerate() {
            if k > 0 && (ip.len() - k) % 3 == 0 {
                g.push(',');
            }
            g.push(ch);
        }
        ip = g;
    }
    let num = if fp.is_empty() { ip } else { format!("{ip}.{fp}") };
    let sign = if neg && x * 10f64.powi(decimals as i32) >= 0.5 { "-" } else { "" };
    format!("{sign}{prefix}{num}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scale_covers() {
        let (a, b, s) = nice_scale(0.0, 5.1);
        assert!(a <= 0.0 && b >= 5.1 && s > 0.0);
        let (a, b, _) = nice_scale(f64::NAN, 3.0);
        assert!(a.is_finite() && b.is_finite());
        let (a, b, _) = nice_scale(7.0, 7.0);
        assert!(b > a);
    }

    #[test]
    fn automatic_upper_headroom_can_be_disabled_without_changing_cartesian_scales() {
        assert_eq!(auto_scale(1.8, 5.0, None, None, None, 10.0), (0.0, 6.0, 1.0));
        for (maximum, axis_maximum, unit) in [(5.0, 5.0, 0.5), (32.0, 35.0, 5.0), (34.0, 35.0, 5.0)] {
            assert_eq!(auto_scale_with_headroom(1.0, maximum, None, None, None, 10.0, 0.0), (0.0, axis_maximum, unit));
        }
        assert_eq!(auto_scale_with_headroom(1.0, 5.0, Some(2.0), Some(9.0), Some(2.0), 10.0, 0.0), (2.0, 9.0, 2.0));
        let (_, max, unit) = auto_scale_with_headroom(0.0, 5.0, None, None, Some(0.7), 10.0, 0.0);
        assert!((max - 5.6).abs() < 1e-12);
        assert_eq!(unit, 0.7);
        let (min, max, unit) = auto_scale_with_headroom(0.0, 0.0, None, None, None, 10.0, 0.0);
        assert!(min.is_finite() && max > min && unit > 0.0);
    }

    #[test]
    fn auto_scale_like_reference() {
        // Largest value 5 → 0..6 by 1; 4.5 with room for ten steps → 0..5 by 0.5.
        assert_eq!(auto_scale(0.0, 5.0, None, None, None, 10.0), (0.0, 6.0, 1.0));
        assert_eq!(auto_scale(2.5, 4.5, None, None, None, 10.0), (0.0, 5.0, 0.5));
        // 13 → 0..14 by 2, not 0..12.5 by 2.5.
        assert_eq!(auto_scale(0.0, 13.0, None, None, None, 10.0), (0.0, 14.0, 2.0));
        // Values far from zero keep a non-zero minimum.
        let (a, b, u) = auto_scale(100.0, 110.0, None, None, None, 10.0);
        assert!(a > 0.0 && a <= 100.0 && b >= 110.0 && u > 0.0, "{a} {b} {u}");
        // Fixed ends are kept.
        assert_eq!(auto_scale(0.0, 5.0, Some(1.0), Some(9.0), Some(2.0), 10.0), (1.0, 9.0, 2.0));
        // Negative values.
        let (a, b, _) = auto_scale(-3.0, 4.0, None, None, None, 10.0);
        assert!(a <= -3.0 && b >= 4.0);
        // Degenerate input never loops or panics.
        let (a, b, u) = auto_scale(f64::NAN, f64::INFINITY, None, None, None, 0.0);
        assert!(a.is_finite() && b > a && u > 0.0);
        // Equal values start at zero.
        assert_eq!(auto_scale(1.0, 1.0, None, Some(4.0), Some(1.0), 10.0), (0.0, 4.0, 1.0));
        let (a, b, _) = auto_scale(0.0, 0.0, None, None, None, 10.0);
        assert!(b > a);
    }

    #[test]
    fn error_bars_and_trendlines() {
        let vals = [Some(2.0), Some(4.0), Some(6.0)];
        let pct = ErrorBars { kind: "minus".into(), val_type: "percentage".into(), val: 100.0, ..Default::default() };
        assert_eq!(error_amounts(&pct, &vals, 1, 4.0), (4.0, 0.0, None));
        let se = ErrorBars { kind: "both".into(), val_type: "stdErr".into(), ..Default::default() };
        let (m, p, _) = error_amounts(&se, &vals, 0, 2.0);
        // Sample standard deviation 2 over sqrt(3).
        assert!((m - 2.0 / 3.0f64.sqrt()).abs() < 1e-9 && m == p);
        let sd = ErrorBars { kind: "both".into(), val_type: "stdDev".into(), val: 1.0, ..Default::default() };
        let (m, _, c) = error_amounts(&sd, &vals, 0, 2.0);
        assert_eq!(c, Some(4.0));
        assert!((m - 2.0).abs() < 1e-9);
        let lin = Trendline { kind: "linear".into(), ..Default::default() };
        let f = trend_fn(&lin, &[(1.0, 3.0), (2.0, 5.0), (3.0, 7.0)]).unwrap();
        assert!((f(4.0) - 9.0).abs() < 1e-9);
        let poly = Trendline { kind: "poly".into(), order: Some(2), ..Default::default() };
        let f = trend_fn(&poly, &[(0.0, 1.0), (1.0, 2.0), (2.0, 5.0), (3.0, 10.0)]).unwrap();
        assert!((f(4.0) - 17.0).abs() < 1e-6);
        let exp = Trendline { kind: "exp".into(), ..Default::default() };
        let f = trend_fn(&exp, &[(0.0, 1.0), (1.0, std::f64::consts::E)]).unwrap();
        assert!((f(2.0) - std::f64::consts::E.powi(2)).abs() < 1e-6);
        assert!(trend_fn(&lin, &[(1.0, 1.0)]).is_none());
    }

    #[test]
    fn number_formats() {
        assert_eq!(format_number(4.3, None), "4.3");
        assert_eq!(format_number(2.0, Some("General")), "2");
        assert_eq!(format_number(0.1 + 0.2, None), "0.3");
        assert_eq!(format_number(0.25, Some("0%")), "25%");
        assert_eq!(format_number(0.256, Some("0.0%")), "25.6%");
        assert_eq!(format_number(1234.5, Some("#,##0.00")), "1,234.50");
        assert_eq!(format_number(-3.0, Some("0.0")), "-3.0");
        assert_eq!(format_number(12.0, Some("0\"元\"")), "12元");
        assert_eq!(format_number(45000.0, Some("yyyy/m/d")), "2023/3/15");
        assert_eq!(format_number(45000.0, Some("m\"月\"d\"日\"")), "3月15日");
        assert_eq!(format_number(45000.0, Some("[$-804]mm/dd;@")), "03/15");
        assert_eq!(format_number(5.0, Some("0.0;(0.0)")), "5.0");
        assert_eq!(format_number(-5.0, Some("0.0;(0.0)")), "(5.0)");
    }

    #[test]
    fn sample_ranges() {
        let c = Chart::sample(ChartType::StackedColumn);
        let (_, hi) = c.value_range();
        assert!(hi > 10.0);
        assert_eq!(Chart::sample(ChartType::Pie).series.len(), 1);
    }
}
