//! The shape gallery (plan stage 9, walkthrough F4; WPS W-E6, W-E10): every preset shape and the
//! freeform tools are drawing tools, and their icons are drawn from the presets' own geometry.

use kition_slide_engine::tools::{ToolKind, is_freeform_tool};
use kition_slide_geom::preset::{self, FillMode};
use kition_slide_geom::{KurboShape, Rect};
use serde_json::{Map, Value, json};

/// The drawing tool of a gallery name: any preset shape (`heart`, `flowChartDecision`…) or a
/// freeform tool (`curve`, `freeform`, `scribble`).
pub(crate) fn shape_tool(name: &str) -> Option<ToolKind> {
    (preset::info(name).is_some() || is_freeform_tool(name)).then(|| ToolKind::Shape { preset: name.to_owned() })
}

/// The icon side the gallery draws when it asks for none in range.
const ICON_SIDE: f64 = 20.0;

/// Every preset's icon, drawn `size` points square from its geometry at its default adjust values:
/// `{name: {view: [x, y, side, side], paths: [{d, fill, stroke}, …]}}`, each path an SVG path
/// (`d`), filled or not, stroked or not, and `view` the square around all of them (a callout's leader
/// line reaches outside its box). The freeform tools have none (the editor draws theirs).
pub fn preset_icons(size: f64) -> Value {
    let side = if size.is_finite() && (8.0..=256.0).contains(&size) { size } else { ICON_SIDE };
    let mut icons = Map::new();
    for info in preset::CATALOG {
        let Some(geometry) = preset::build(info.name, side, side, info.defaults) else { continue };
        let mut paths: Vec<Value> = Vec::new();
        let mut bounds = Rect::new(0.0, 0.0, side, side);
        for path in geometry.paths.iter().filter(|path| path.stroke || path.fill != FillMode::None) {
            let around = path.path.bounding_box();
            if around.is_finite() {
                bounds = bounds.union(around);
            }
            paths.push(json!({"d": path.path.to_svg(), "fill": path.fill != FillMode::None, "stroke": path.stroke}));
        }
        // The arrow connector's head is a line end, not geometry: drawn here as the gallery shows it.
        if info.name == "straightConnector1" {
            let head = side * 0.3;
            paths.push(json!({"d": format!("M{} {}L{side} {side}L{side} {}", side - head, side, side - head), "fill": false, "stroke": true}));
        }
        let span = bounds.width().max(bounds.height());
        let centre = bounds.center();
        let view = [centre.x - span / 2.0, centre.y - span / 2.0, span, span];
        icons.insert(info.name.to_owned(), json!({"view": view, "paths": paths}));
    }
    Value::Object(icons)
}
