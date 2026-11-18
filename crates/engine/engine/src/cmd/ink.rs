//! `drawing.ink` and `drawing.erase`: the Kit Harness Protocol's `KitDrawing 1.2` ink over the
//! slide's ink layer (an ink shape holding the slide's strokes, in slide points), through the ink
//! contract Kition's kits share (`kition_office_core::ink`); and the Draw tab's pens as the commands
//! every kit has (`draw.pen`, `draw.pencil`, `draw.highlighter`, `draw.eraser`, `draw.select`,
//! `draw.state`), so the shared Draw controls drive them.

use kition_office_core::ink::{self, EraserGesture, InkStroke, InkTool};
use kition_office_core::model as dm;
use kition_slide_color::Rgba;
use kition_slide_model::{Shape, ShapeId, ShapeKind, Xfrm};
use serde_json::{Value, json};

use super::*;
use crate::tools::ToolKind;
use crate::{Result, Session};

/// How far around the eraser's path it takes ink, points.
pub const ERASER_RADIUS: f64 = 4.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "drawing.ink",
            "Draw Ink Strokes",
            [],
            None,
            "{strokes: [{tool?: pen|pencil|highlighter, color?, width?, points: [[x, y, pressure?], …]}]} (slide points; members left out take the current pen)",
            has_slide,
            add
        ),
        cmd!("drawing.erase", "Erase Ink", [], None, "{path: [[x, y], …] | x, y, radius?} (deletes the strokes the path touches)", has_slide, erase),
        cmd!(query "draw.pen", "Pen", [], None, "{color?: hex, width?: pt}", has_slide, |s, p| pen(s, p, "pen")),
        cmd!(query "draw.pencil", "Pencil", [], None, "{color?: hex, width?: pt}", has_slide, |s, p| pen(s, p, "pencil")),
        cmd!(query "draw.highlighter", "Highlighter", [], None, "{color?: hex, width?: pt}", has_slide, |s, p| pen(s, p, "highlighter")),
        cmd!(query "draw.eraser", "Eraser", [], None, "{}", has_slide, |s, _| {
            s.set_tool(ToolKind::Ink { mode: "eraser".into(), color: Rgba::BLACK, width: 2.0 });
            Ok(draw_state(s))
        }),
        cmd!(query "draw.select", "Select", [], None, "{}", has_slide, |s, _| {
            s.set_tool(ToolKind::Select);
            Ok(draw_state(s))
        }),
        cmd!(query "draw.state", "Draw State", [], None, "{}", has_slide, |s, _| Ok(draw_state(s))),
    ]
}

fn fail(cmd: &str, e: kition_office_core::ops::DrawingError) -> crate::EngineError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    bad(cmd, format!("{}{property}: {}", e.code, e.message))
}

/// A pen's colour and width when picked up without them.
fn pen_defaults(mode: &str) -> (Rgba, f64) {
    match mode {
        "highlighter" => (Rgba { r: 255, g: 230, b: 0, a: 255 }, 12.0),
        "pencil" => (Rgba::rgb(90, 90, 90), 1.0),
        _ => (Rgba::BLACK, 2.0),
    }
}

fn hex(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

/// Picks up pen `mode`: the same pen keeps its look, another starts with its own.
fn pen(s: &mut Session, p: &Value, mode: &str) -> Result<Value> {
    let (mut color, mut width) = match &s.tool.kind {
        ToolKind::Ink { mode: m, color, width } if m == mode => (*color, *width),
        _ => pen_defaults(mode),
    };
    if let Some(c) = str_param(p, "color") {
        let c = dm::Color::from_value(&json!(c)).ok_or_else(|| bad("draw.pen", "`color` is #RRGGBB"))?;
        let dm::ColorBase::Rgb { rgb } = c.base else { return Err(bad("draw.pen", "`color` is #RRGGBB")) };
        color = Rgba::rgb(rgb.0, rgb.1, rgb.2);
    }
    if let Some(w) = f64_param(p, "width") {
        width = w.clamp(ink::MIN_WIDTH, ink::MAX_WIDTH);
    }
    s.set_tool(ToolKind::Ink { mode: mode.into(), color, width });
    Ok(draw_state(s))
}

/// The pen in use: `{tool, color, width}` (`tool`: select, pen, pencil, highlighter or eraser).
pub fn draw_state(s: &Session) -> Value {
    match &s.tool.kind {
        ToolKind::Ink { mode, color, width } if mode != "eraser" => json!({ "tool": mode, "color": hex(*color), "width": width }),
        ToolKind::Ink { .. } => {
            let (c, w) = pen_defaults("pen");
            json!({ "tool": "eraser", "color": hex(c), "width": w })
        }
        _ => {
            let (c, w) = pen_defaults("pen");
            json!({ "tool": "select", "color": hex(c), "width": w })
        }
    }
}

/// A Slide stroke as a shared one.
pub fn stroke_of(st: &kition_slide_model::InkStroke) -> InkStroke {
    InkStroke {
        tool: if st.highlighter { InkTool::Highlighter } else { InkTool::Pen },
        color: Some(dm::Color::rgb(dm::Rgb(st.color.r, st.color.g, st.color.b))),
        width: st.width,
        points: st.points.iter().map(|(x, y, p)| [*x, *y, f64::from(*p)]).collect(),
    }
}

fn current_pen(s: &Session) -> InkStroke {
    let state = draw_state(s);
    InkStroke {
        tool: state["tool"].as_str().and_then(InkTool::parse).unwrap_or_default(),
        color: state["color"].as_str().and_then(|c| dm::Color::from_value(&json!(c))),
        width: state["width"].as_f64().unwrap_or(2.0),
        points: Vec::new(),
    }
}

/// Adds `strokes` to the slide's ink layer (made when it has none), in one edit. Answers the layer.
pub fn push_strokes(s: &mut Session, strokes: Vec<kition_slide_model::InkStroke>) -> Result<ShapeId> {
    let existing = s.doc()?.shapes().iter().rev().find(|x| matches!(x.kind, ShapeKind::Ink { .. })).map(|x| x.id);
    s.edit(|doc, sel| {
        let id = match existing {
            Some(id) => id,
            None => {
                let id = ShapeId(doc.alloc_id());
                let size = doc.slide_size;
                let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("drawing.ink", "no slide"))?;
                list.push(Shape {
                    id,
                    name: "Ink".into(),
                    xfrm: Some(Xfrm::new(0.0, 0.0, size.width, size.height)),
                    kind: ShapeKind::Ink { strokes: vec![] },
                    ..Default::default()
                });
                id
            }
        };
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("drawing.ink", "no slide"))?;
        if let Some(sh) = kition_slide_model::find_shape_mut(list, id)
            && let ShapeKind::Ink { strokes: have } = &mut sh.kind
        {
            have.extend(strokes);
        }
        Ok(id)
    })
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let parsed = ink::parse_strokes(p, &current_pen(s)).map_err(|e| fail("drawing.ink", e))?;
    let mut strokes = Vec::with_capacity(parsed.len());
    for st in &parsed {
        let color = match st.color.as_ref().map(|c| &c.base) {
            Some(dm::ColorBase::Rgb { rgb }) => Rgba::rgb(rgb.0, rgb.1, rgb.2),
            Some(dm::ColorBase::Theme { .. }) => {
                return Err(fail("drawing.ink", kition_office_core::ops::DrawingError::unsupported("color", "ink takes an RGB colour")));
            }
            None => pen_defaults("pen").0,
        };
        let mut points: Vec<(f64, f64, f32)> = st.points.iter().map(|q| (q[0], q[1], q[2] as f32)).collect();
        if points.len() == 1 {
            points.push(points[0]);
        }
        strokes.push(kition_slide_model::InkStroke { points, color, width: st.width, highlighter: st.tool == InkTool::Highlighter });
    }
    let n = strokes.len();
    let layer = push_strokes(s, strokes)?;
    Ok(json!({ "ids": [layer.0.to_string()], "strokes": n }))
}

/// Deletes the strokes `gesture`'s path touches on the slide being edited, in one edit (none when
/// it touches none). Answers how many.
pub fn erase_strokes(s: &mut Session, gesture: &EraserGesture) -> Result<usize> {
    let st = s.doc()?;
    let mut hits: Vec<(ShapeId, Vec<usize>)> = Vec::new();
    for sh in st.shapes() {
        if let ShapeKind::Ink { strokes } = &sh.kind {
            let shared: Vec<InkStroke> = strokes.iter().map(stroke_of).collect();
            let hit = gesture.hits(&shared);
            if !hit.is_empty() {
                hits.push((sh.id, hit));
            }
        }
    }
    let n = hits.iter().map(|(_, h)| h.len()).sum();
    if n == 0 {
        return Ok(0);
    }
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("drawing.erase", "no slide"))?;
        for (id, hit) in &hits {
            if let Some(sh) = kition_slide_model::find_shape_mut(list, *id)
                && let ShapeKind::Ink { strokes } = &mut sh.kind
            {
                let mut i = 0;
                strokes.retain(|_| {
                    let keep = !hit.contains(&i);
                    i += 1;
                    keep
                });
            }
        }
        Ok(())
    })?;
    Ok(n)
}

fn erase(s: &mut Session, p: &Value) -> Result<Value> {
    let gesture = ink::parse_eraser(p, ERASER_RADIUS).map_err(|e| fail("drawing.erase", e))?;
    let n = erase_strokes(s, &gesture)?;
    Ok(json!({ "erased": n }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::model::DrawingObject;

    fn inks(s: &mut Session) -> Vec<InkStroke> {
        let d = s.execute("drawing.describe", &json!({})).unwrap();
        let objects: Vec<DrawingObject> = serde_json::from_value(d["objects"].clone()).unwrap();
        objects.into_iter().filter(|o| o.kind == dm::ObjectKind::Ink).flat_map(|o| o.ink.unwrap_or_default()).collect()
    }

    #[test]
    fn strokes_draw_describe_undo_erase_and_reopen_as_pptx() {
        let mut s = Session::with_new();
        s.execute("draw.pencil", &json!({ "width": 3 })).unwrap();
        assert_eq!(s.execute("draw.state", &json!({})).unwrap()["tool"], "pencil");
        s.execute(
            "drawing.ink",
            &json!({ "strokes": [
                { "tool": "pen", "points": [[100, 100, 0.4], [200, 100, 0.8]], "color": "#C00000", "width": 2 },
                { "tool": "highlighter", "points": [[100, 160], [200, 160]] },
            ] }),
        )
        .unwrap();
        let st = inks(&mut s);
        assert_eq!(st.len(), 2);
        assert_eq!((st[0].width, st[0].color.clone()), (2.0, Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
        assert_eq!(st[0].points[1], [200.0, 100.0, 0.800000011920929], "pressure kept");
        assert_eq!((st[1].tool, st[1].width), (InkTool::Highlighter, 3.0), "the current pen's width");
        // One step each; the eraser takes the stroke its path crosses, and only it.
        let r = s.execute("drawing.erase", &json!({ "path": [[150, 80], [150, 120]] })).unwrap();
        assert_eq!(r["erased"], 1);
        assert_eq!(inks(&mut s).iter().map(|x| x.tool).collect::<Vec<_>>(), vec![InkTool::Highlighter]);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(inks(&mut s).len(), 2);
        // The ink layer is drawn on and erased, not moved.
        let d = s.execute("drawing.describe", &json!({})).unwrap();
        let id = d["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "ink").unwrap()["id"].clone();
        assert!(s.execute("drawing.transform", &json!({ "ids": [id], "dx": 5 })).is_err());

        let bytes = file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
        let mut back = Session::new();
        file::add_opened(&mut back, "i.pptx", None, file::open_slide_file("i.pptx", &bytes).unwrap());
        let reopened = inks(&mut back);
        assert_eq!(reopened.len(), 2, "the strokes reopen as ink, not as a group of shapes");
        let pen = reopened.iter().find(|x| x.tool == InkTool::Pen).unwrap();
        assert_eq!((pen.width, pen.color.clone()), (2.0, Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
        assert!((pen.points[0][0] - 100.0).abs() < 0.05 && (pen.points[1][0] - 200.0).abs() < 0.05, "{:?}", pen.points);
        assert!(reopened.iter().any(|x| x.tool == InkTool::Highlighter));
    }

    #[test]
    fn the_eraser_tool_erases_a_drag_in_one_step() {
        let mut s = Session::with_new();
        s.execute("drawing.ink", &json!({ "strokes": [{ "points": [[100, 100], [200, 100]] }, { "points": [[100, 200], [200, 200]] }] })).unwrap();
        s.execute("draw.eraser", &json!({})).unwrap();
        let undo = s.active().unwrap().history.undo.len();
        use crate::tools::{PointerEvent, PointerKind};
        let ev = |kind: PointerKind, x: f64, y: f64| PointerEvent { kind, x, y, mods: Default::default(), tol: 2.0 };
        s.pointer(ev(PointerKind::Down, 150.0, 80.0)).unwrap();
        s.pointer(ev(PointerKind::Drag, 150.0, 120.0)).unwrap();
        s.pointer(ev(PointerKind::Drag, 150.0, 220.0)).unwrap();
        s.pointer(ev(PointerKind::Up, 150.0, 220.0)).unwrap();
        assert!(inks(&mut s).is_empty(), "the drag crossed both strokes");
        assert_eq!(s.active().unwrap().history.undo.len(), undo + 1, "one undo step for the whole drag");
    }
}
