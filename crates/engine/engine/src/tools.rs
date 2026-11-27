//! Pointer tools. Gestures arrive in slide coordinates (points) through [`Session::pointer`] from
//! the UI, the control channel or MCP alike; each finished gesture is one undo step.

use std::sync::Arc;

mod table;

use kition_slide_geom::{Affine, Point, Rect, Vec2, Xfrm};
use kition_slide_model::{Shape, ShapeId, ShapeKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cmd::{self, xfrm_of};
use crate::{HistoryEntry, Result, Session, TextSel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerKind {
    #[default]
    Down,
    Drag,
    Up,
    Move,
    DoubleClick,
    TripleClick,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f64,
    pub y: f64,
    pub mods: Mods,
    /// Hit tolerance in points: [`TOL_PX`] screen pixels at the current zoom. The handles' reach
    /// and the rotation handle's place, which are fixed on screen, are measured from it.
    pub tol: f64,
}

/// The screen pixels a pointer event's `tol` stands for: `tol` is this many pixels in points.
pub const TOL_PX: f64 = 4.0;
/// A press this close to a handle's centre (screen pixels) takes the handle: a resize handle, an
/// adjust handle, a line's end (plan 0B, walkthrough D7; the handles are drawn 9 px across).
pub const HANDLE_REACH_PX: f64 = 7.0;
/// The rotation handle's reach (screen pixels; it is drawn 13 px across).
pub const ROTATE_REACH_PX: f64 = 8.0;
/// The rotation handle's centre sits this far above the middle of the top edge, in screen pixels
/// whatever the zoom (WPS W-B1: about 18 px).
pub const ROTATE_GAP_PX: f64 = 18.0;
/// Around a selected object's frame, a press this close (screen pixels) that hits nothing else is
/// a press on that object: a handle missed by a little keeps the selection and moves the object,
/// and never clears the selection as a press on the empty slide does (walkthrough D7).
pub const FRAME_BAND_PX: f64 = 10.0;

/// Points per screen pixel, from a pointer event's tolerance.
pub fn px_of(tol: f64) -> f64 {
    if tol.is_finite() && tol > 0.0 { tol / TOL_PX } else { 1.0 }
}

/// Where a selected frame's handles are, at `px` points per screen pixel: the 8 resize handles
/// (NW, N, NE, E, SE, S, SW, W) on the frame, and the rotation handle [`ROTATE_GAP_PX`] above the
/// middle of its top edge, in its rotated (and flipped) frame. The one place both the hit test and
/// the handles drawn on the canvas take them from.
pub fn handles(x: &Xfrm, px: f64) -> ([Point; 8], Point) {
    let (pts, _) = box_handles(x);
    let up = if x.flip_v { 1.0 } else { -1.0 };
    let gap = ROTATE_GAP_PX * if px.is_finite() && px > 0.0 { px } else { 1.0 };
    let rot =
        Affine::translate(Vec2::new(x.x + x.w / 2.0, x.y + x.h / 2.0)) * Affine::rotate(x.rot.to_radians()) * Point::new(0.0, up * (x.h / 2.0 + gap));
    (pts, rot)
}

/// What the pointer would do where it is, for its shape on screen ([`Session::pointing`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Pointing {
    /// Nothing under the pointer (a press clears the selection and starts a marquee).
    Default,
    /// An object a press selects and a drag moves.
    Move,
    /// Text a press puts the caret in (or text being selected).
    Text,
    /// A resize handle: `angle` is the direction it pulls on screen, in degrees from 0 (→) to
    /// 180, after the object's rotation and flips.
    Resize {
        angle: f64,
    },
    Rotate,
    /// A shape's adjust handle.
    Adjust,
    /// A line's end.
    LineEnd,
    /// A table's grid line between columns (`vertical`) or rows.
    Divider {
        vertical: bool,
    },
    /// A drawing tool: a press starts a shape, a line or a stroke.
    Draw,
    /// The text box tool.
    TextTool,
    /// Format Painter or Animation Painter is loaded: a press paints.
    Copy,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ToolKind {
    #[default]
    Select,
    /// Draw a preset shape.
    Shape {
        preset: String,
    },
    TextBox,
    /// Ink: pen (`pen`, `highlighter`) or `eraser`.
    Ink {
        mode: String,
        color: kition_slide_color::Rgba,
        width: f64,
    },
}

/// What a point hits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Hit {
    /// Resize handle 0..8: NW, N, NE, E, SE, S, SW, W.
    Resize {
        id: ShapeId,
        handle: u8,
    },
    Rotate {
        id: ShapeId,
    },
    Adjust {
        id: ShapeId,
        index: usize,
    },
    /// Line end 0/1 of a line-like shape.
    LineEnd {
        id: ShapeId,
        end: u8,
    },
    Shape {
        id: ShapeId,
        text: bool,
    },
    /// An interior grid line of a table: the line before column (or row) `index`.
    TableDivider {
        id: ShapeId,
        vertical: bool,
        index: usize,
    },
}

#[derive(Clone, Debug)]
enum Drag {
    Move {
        ids: Vec<ShapeId>,
        start: Point,
        origs: Vec<(ShapeId, Xfrm)>,
        moved: bool,
        duplicate: bool,
        enter_text: Option<(ShapeId, Point)>,
    },
    Resize {
        id: ShapeId,
        handle: u8,
        orig: Xfrm,
        start: Point,
    },
    LineEnd {
        id: ShapeId,
        end: u8,
        orig: Xfrm,
    },
    Rotate {
        ids: Vec<ShapeId>,
        center: Point,
        start: f64,
        origs: Vec<(ShapeId, Xfrm)>,
    },
    Adjust {
        id: ShapeId,
        index: usize,
        orig: Xfrm,
    },
    Marquee {
        start: Point,
        add: bool,
    },
    Create {
        start: Point,
    },
    TextSelect {
        shape: ShapeId,
    },
    Cells {
        shape: ShapeId,
        anchor: (usize, usize),
    },
    /// A table grid line, from the column widths and drawn row heights when the drag began.
    Divider {
        id: ShapeId,
        vertical: bool,
        index: usize,
        start: Point,
        cols: Vec<f64>,
        rows: Vec<f64>,
    },
    Ink {
        points: Vec<(f64, f64, f32)>,
    },
    /// The eraser's sweep: what it touches is erased on release, as one step.
    Erase {
        gesture: kition_office_core::ink::EraserGesture,
    },
    /// Freeform drawing (`freeform` polygon, `curve`, `scribble`): placed points; `pressed` while
    /// the button is down (dragging adds scribbled points).
    Freeform {
        mode: String,
        points: Vec<Point>,
        pressed: bool,
    },
}

/// Shape-gallery names that draw a freeform path instead of a preset.
pub const FREEFORM_TOOLS: [(&str, &str); 3] = [("curve", "Curve"), ("freeform", "Freeform: Shape"), ("scribble", "Freeform: Scribble")];

pub fn is_freeform_tool(name: &str) -> bool {
    FREEFORM_TOOLS.iter().any(|f| f.0 == name)
}

/// A snapping guide line to draw (slide coordinates): vertical (x) or horizontal (y).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    pub vertical: bool,
    pub pos: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ToolState {
    pub kind: ToolKind,
    /// Keep the tool after one use (double-click on a shape in the gallery: lock drawing mode).
    pub sticky: bool,
    drag: Option<Drag>,
    pub marquee: Option<Rect>,
    pub preview: Option<Xfrm>,
    pub guides: Vec<Guide>,
    pub hover: Option<Hit>,
    pub ink_preview: Vec<(f64, f64, f32)>,
    /// Connection sites to show (slide coordinates) while drawing or dragging a line end, and the
    /// site the end would glue to.
    pub sites: Vec<Point>,
    pub glue: Option<Point>,
    /// Freeform being drawn: points so far (plus the rubber-band point), smoothed, and whether
    /// the pointer is over the start (closing the shape).
    pub path_preview: Vec<Point>,
    pub path_smooth: bool,
    pub path_closing: bool,
}

impl ToolState {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// A freeform path is being drawn (Enter/Escape/double-click finish it).
    pub fn drawing_freeform(&self) -> bool {
        matches!(self.drag, Some(Drag::Freeform { .. }))
    }
}

/// Handle positions (8 resize + rotate) and resizing come from the shared drawing code.
pub use kition_office_core::edit::{box_handles, resize_box};

impl Session {
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.tool.kind = kind;
        self.tool.drag = None;
        self.tool.preview = None;
        self.tool.marquee = None;
    }

    /// What's under `p` on the current slide: a selected object's handles first (the nearest
    /// within reach), then the objects, topmost first, then the band around a selected frame.
    pub fn hit_test(&self, p: Point, tol: f64) -> Option<Hit> {
        let st = self.active()?;
        let px = px_of(tol);
        let reach = HANDLE_REACH_PX * px;
        // Handles of selected shapes first.
        if st.selection.text.is_none() || st.selection.shapes.len() == 1 {
            for id in st.selection.shapes.iter().rev() {
                let Some(sh) = st.shape(*id) else { continue };
                let x = xfrm_of(&st.doc, &st.selection, sh);
                if sh.is_line() {
                    let a = x.affine();
                    let ends = [(0u8, a * Point::new(0.0, 0.0)), (1u8, a * Point::new(x.w, x.h))];
                    if let Some((end, _)) = nearest(ends.iter().map(|(end, q)| (*end, (*q - p).hypot())), reach) {
                        return Some(Hit::LineEnd { id: *id, end });
                    }
                    continue;
                }
                let geo = kition_slide_render::shape_geometry(sh, x.w, x.h);
                let a = x.affine();
                if let Some((index, _)) = nearest(geo.handles.iter().map(|hd| (hd.index, (a * hd.pos - p).hypot())), reach) {
                    return Some(Hit::Adjust { id: *id, index });
                }
                let (pts, rot) = handles(&x, px);
                if (rot - p).hypot() <= ROTATE_REACH_PX * px && st.selection.text.is_none() {
                    return Some(Hit::Rotate { id: *id });
                }
                if let Some((handle, _)) = nearest(pts.iter().enumerate().map(|(i, q)| (i as u8, (*q - p).hypot())), reach) {
                    return Some(Hit::Resize { id: *id, handle });
                }
            }
        }
        // Shapes, topmost first.
        for sh in st.shapes().iter().rev() {
            if sh.hidden || sh.locked {
                continue;
            }
            if let Some(h) = hit_shape(st, sh, p, tol) {
                return Some(h);
            }
        }
        // Just outside a selected object's frame: that object, not the empty slide.
        let band = FRAME_BAND_PX * px;
        for id in st.selection.shapes.iter().rev() {
            let Some(sh) = st.shape(*id) else { continue };
            if sh.is_line() || sh.hidden || sh.locked {
                continue;
            }
            let x = xfrm_of(&st.doc, &st.selection, sh);
            let local = x.affine().inverse() * p;
            if local.x >= -band && local.y >= -band && local.x <= x.w + band && local.y <= x.h + band {
                return Some(Hit::Shape { id: *id, text: false });
            }
        }
        None
    }

    /// What a press at `p` (the pointer's last place on the slide, if it is on it) would do, for
    /// the pointer's shape: a gesture under way keeps its own; a loaded painter paints; a drawing
    /// tool draws; otherwise what [`Session::hit_test`] finds there. Text is only where a press
    /// puts the caret: the text being edited, a text box or placeholder, a table's cells, and the
    /// lines of a selected shape's text (not a shape without text).
    pub fn pointing(&self, p: Option<Point>, tol: f64) -> Pointing {
        let Some(st) = self.active() else { return Pointing::Default };
        if let Some(drag) = &self.tool.drag {
            return match drag {
                Drag::Move { moved: true, .. } => Pointing::Move,
                Drag::Resize { id, handle, .. } => {
                    st.shape(*id).map_or(Pointing::Default, |sh| resize_pointing(&xfrm_of(&st.doc, &st.selection, sh), *handle))
                }
                Drag::Rotate { .. } => Pointing::Rotate,
                Drag::Adjust { .. } => Pointing::Adjust,
                Drag::LineEnd { .. } => Pointing::LineEnd,
                Drag::TextSelect { .. } => Pointing::Text,
                Drag::Divider { vertical, .. } => Pointing::Divider { vertical: *vertical },
                Drag::Create { .. } | Drag::Ink { .. } | Drag::Erase { .. } | Drag::Freeform { .. } => Pointing::Draw,
                Drag::Move { .. } | Drag::Marquee { .. } | Drag::Cells { .. } => self.pointing_at(p, tol),
            };
        }
        match &self.tool.kind {
            ToolKind::Select => {}
            ToolKind::TextBox => return Pointing::TextTool,
            ToolKind::Shape { .. } | ToolKind::Ink { .. } => return Pointing::Draw,
        }
        if self.painter.is_some() || self.anim_painter.is_some() {
            return Pointing::Copy;
        }
        self.pointing_at(p, tol)
    }

    fn pointing_at(&self, p: Option<Point>, tol: f64) -> Pointing {
        let Some(st) = self.active() else { return Pointing::Default };
        let Some(p) = p.filter(|p| p.x.is_finite() && p.y.is_finite()) else { return Pointing::Default };
        if matches!(self.tool.kind, ToolKind::Select)
            && let Some(Hit::TableDivider { vertical, .. }) = self.table_divider_hit(p, tol)
        {
            return Pointing::Divider { vertical };
        }
        let hit = self.hit_test(p, tol);
        match hit {
            Some(Hit::Resize { id, handle }) => {
                st.shape(id).map_or(Pointing::Default, |sh| resize_pointing(&xfrm_of(&st.doc, &st.selection, sh), handle))
            }
            Some(Hit::Rotate { .. }) => Pointing::Rotate,
            Some(Hit::Adjust { .. }) => Pointing::Adjust,
            Some(Hit::LineEnd { .. }) => Pointing::LineEnd,
            Some(Hit::TableDivider { vertical, .. }) => Pointing::Divider { vertical },
            Some(Hit::Shape { id, text }) => {
                // The text being edited takes the caret anywhere in its frame.
                if let Some(t) = &st.selection.text
                    && !t.notes
                    && t.shape == id
                {
                    let inside = match t.cell {
                        Some(cell) => table::hit_cell(st, id, p) == Some(cell),
                        None => st.shape(id).is_some_and(|sh| xfrm_of(&st.doc, &st.selection, sh).bounds().inflate(tol, tol).contains(p)),
                    };
                    if inside {
                        return Pointing::Text;
                    }
                }
                let Some(sh) = st.shape(id) else { return Pointing::Default };
                let x = xfrm_of(&st.doc, &st.selection, sh);
                let local = x.affine().inverse() * p;
                let within = local.x >= tol && local.y >= tol && local.x <= x.w - tol && local.y <= x.h - tol;
                if matches!(sh.kind, ShapeKind::Table(_)) {
                    // A press in a cell edits it; the frame moves the table.
                    return if within && table::hit_cell(st, id, p).is_some() { Pointing::Text } else { Pointing::Move };
                }
                if !text || !within {
                    return Pointing::Move;
                }
                if is_text_first(st, id) || (st.selection.shapes.contains(&id) && over_lines(st, sh, p, tol)) {
                    Pointing::Text
                } else {
                    Pointing::Move
                }
            }
            None => Pointing::Default,
        }
    }

    /// Whether the pointer at `p` is the text cursor: a press there places the caret, or text is
    /// being selected ([`Session::pointing`]).
    pub fn over_text(&self, p: Point, tol: f64) -> bool {
        self.pointing(Some(p), tol) == Pointing::Text
    }

    /// Feed a pointer event to the active tool. Returns what happened (for agents). What it
    /// changed is written to the document's replica (a gesture's when it ends: one step).
    pub fn pointer(&mut self, ev: PointerEvent) -> Result<Value> {
        // A press (a click that puts the caret anywhere, even where it was) ends a run of typing
        // or nudges (WPS W-H2).
        if !matches!(ev.kind, PointerKind::Move | PointerKind::Drag | PointerKind::Up) {
            self.end_run();
        }
        let r = self.pointer_event(ev);
        self.sync_all_kdm();
        r
    }

    fn pointer_event(&mut self, ev: PointerEvent) -> Result<Value> {
        let tol = if ev.tol > 0.0 { ev.tol } else { 4.0 };
        let p = Point::new(ev.x, ev.y);
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Ok(Value::Null);
        }
        match ev.kind {
            PointerKind::Move => {
                if let Some(Drag::Freeform { points, mode, .. }) = &self.tool.drag {
                    let closing = points.len() > 2 && points.first().is_some_and(|s| (*s - p).hypot() <= tol * 2.0);
                    let mut v = points.clone();
                    v.push(p);
                    self.tool.path_preview = v;
                    self.tool.path_smooth = mode == "curve";
                    self.tool.path_closing = closing;
                    return Ok(Value::Null);
                }
                let hit = self.hit_test(p, tol);
                self.tool.hover = match hit {
                    Some(Hit::Shape { .. }) if matches!(self.tool.kind, ToolKind::Select) => self.table_divider_hit(p, tol).or(hit),
                    hit => hit,
                };
                let line_tool = matches!(&self.tool.kind, ToolKind::Shape { preset } if kition_slide_geom::preset::is_line_like(preset));
                if line_tool {
                    self.update_sites(p, tol, None);
                } else {
                    self.tool.sites.clear();
                    self.tool.glue = None;
                }
                Ok(Value::Null)
            }
            PointerKind::Down => self.down(p, ev.mods, tol),
            PointerKind::Drag => self.drag(p, ev.mods, tol),
            PointerKind::Up => self.up(p, ev.mods),
            PointerKind::DoubleClick => self.double_click(p, tol),
            PointerKind::TripleClick => {
                if self.active().is_some_and(|d| d.selection.text.is_some()) {
                    self.execute("text.selectParagraph", &json!({}))
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    fn begin(&mut self, label: &str) {
        let origin = self.origin();
        if let Some(st) = self.active_mut()
            && st.interaction.is_none()
        {
            st.interaction = Some(crate::Interaction { label: label.into(), doc: st.doc.clone(), selection: st.selection.clone(), origin });
        }
    }

    /// Ends a gesture where it is (a merged draft or a restored version takes the document): what
    /// it changed so far is its one undo step, and its later pointer events do nothing.
    pub(crate) fn end_gesture(&mut self) {
        if self.active().is_none_or(|st| st.interaction.is_none()) {
            return;
        }
        self.tool.drag = None;
        self.tool.preview = None;
        self.tool.marquee = None;
        self.tool.guides.clear();
        self.tool.sites.clear();
        self.tool.glue = None;
        self.tool.ink_preview.clear();
        self.commit();
    }

    /// Escape mid-gesture: drop the drag and put back what its live preview changed (no undo
    /// step). Returns whether a gesture was under way.
    pub fn cancel_gesture(&mut self) -> bool {
        if self.tool.drag.is_none() || self.tool.drawing_freeform() {
            return false;
        }
        self.tool.drag = None;
        self.tool.preview = None;
        self.tool.marquee = None;
        self.tool.guides.clear();
        self.tool.sites.clear();
        self.tool.glue = None;
        self.tool.ink_preview.clear();
        if let Some(st) = self.active_mut()
            && let Some(i) = st.interaction.take()
        {
            st.doc = i.doc;
            st.selection = i.selection;
            st.revision += 1;
        }
        true
    }

    fn commit(&mut self) {
        if let Some(st) = self.active_mut()
            && let Some(i) = st.interaction.take()
            && !Arc::ptr_eq(&i.doc, &st.doc)
        {
            crate::push_undo(st, HistoryEntry { label: i.label, doc: i.doc, selection: i.selection, steps: 0, agent: false });
        }
    }

    fn down(&mut self, p: Point, mods: Mods, tol: f64) -> Result<Value> {
        if let Some(Drag::Freeform { mode, mut points, .. }) = self.tool.drag.clone() {
            // Next click of a freeform: close on the start point, else add a vertex.
            if points.len() > 2 && points.first().is_some_and(|s| (*s - p).hypot() <= tol * 2.0) {
                self.tool.drag = Some(Drag::Freeform { mode, points, pressed: false });
                return self.finish_freeform(true);
            }
            points.push(p);
            self.tool.path_preview = points.clone();
            self.tool.drag = Some(Drag::Freeform { mode, points, pressed: true });
            return Ok(Value::Null);
        }
        match self.tool.kind.clone() {
            ToolKind::Shape { preset } if is_freeform_tool(&preset) => {
                self.tool.drag = Some(Drag::Freeform { mode: preset.clone(), points: vec![p], pressed: true });
                self.tool.path_preview = vec![p];
                self.tool.path_smooth = preset == "curve";
                self.tool.path_closing = false;
                return Ok(Value::Null);
            }
            ToolKind::Shape { .. } | ToolKind::TextBox => {
                self.tool.drag = Some(Drag::Create { start: p });
                self.tool.preview = Some(Xfrm::new(p.x, p.y, 0.0, 0.0));
                return Ok(Value::Null);
            }
            ToolKind::Ink { mode, .. } => {
                if mode == "eraser" {
                    self.tool.drag = Some(Drag::Erase {
                        gesture: kition_office_core::ink::EraserGesture::start(p, (tol * 2.0).max(crate::cmd::ink::ERASER_RADIUS)),
                    });
                    self.tool.path_preview = vec![p];
                    return Ok(Value::Null);
                }
                self.tool.drag = Some(Drag::Ink { points: vec![(p.x, p.y, 0.5)] });
                self.tool.ink_preview = vec![(p.x, p.y, 0.5)];
                return Ok(Value::Null);
            }
            ToolKind::Select => {}
        }
        if self.table_down(p, mods, tol)? {
            return Ok(Value::Null);
        }
        let st = self.doc()?;
        // Clicking inside the text being edited moves the caret / starts a text selection.
        if let Some(t) = st.selection.text.clone()
            && !t.notes
            && t.cell.is_none()
            && let Some(sh) = st.shape(t.shape)
        {
            let x = xfrm_of(&st.doc, &st.selection, sh);
            if x.bounds().inflate(tol, tol).contains(p)
                && !matches!(self.hit_test(p, tol), Some(Hit::Resize { .. } | Hit::Rotate { .. } | Hit::Adjust { .. }))
            {
                let pos = text_pos(st, sh, p);
                let shift = mods.shift;
                self.select(|_, sel| {
                    if let Some(tt) = sel.text.as_mut() {
                        tt.caret = pos;
                        if !shift {
                            tt.anchor = pos;
                        }
                    }
                })?;
                self.tool.drag = Some(Drag::TextSelect { shape: t.shape });
                return Ok(json!({"caret": [pos.0, pos.1]}));
            }
        }
        let hit = self.hit_test(p, tol);
        let st = self.doc()?;
        match hit {
            Some(Hit::Resize { id, handle }) => {
                let orig = st.shape(id).map(|s| xfrm_of(&st.doc, &st.selection, s)).unwrap_or_default();
                self.begin("Size");
                self.tool.drag = Some(Drag::Resize { id, handle, orig, start: p });
            }
            Some(Hit::LineEnd { id, end }) => {
                let orig = st.shape(id).map(|s| xfrm_of(&st.doc, &st.selection, s)).unwrap_or_default();
                self.begin("Size");
                self.tool.drag = Some(Drag::LineEnd { id, end, orig });
            }
            Some(Hit::Rotate { id }) => {
                let ids: Vec<ShapeId> = if st.selection.shapes.contains(&id) { st.selection.shapes.clone() } else { vec![id] };
                let origs: Vec<(ShapeId, Xfrm)> = ids.iter().filter_map(|i| st.shape(*i).map(|s| (*i, xfrm_of(&st.doc, &st.selection, s)))).collect();
                let center = st.shape(id).map(|s| xfrm_of(&st.doc, &st.selection, s).center()).unwrap_or(p);
                let start = (p.y - center.y).atan2(p.x - center.x);
                self.begin("Rotate");
                self.tool.drag = Some(Drag::Rotate { ids, center, start, origs });
            }
            Some(Hit::Adjust { id, index }) => {
                let orig = st.shape(id).map(|s| xfrm_of(&st.doc, &st.selection, s)).unwrap_or_default();
                self.begin("Adjust Shape");
                self.tool.drag = Some(Drag::Adjust { id, index, orig });
            }
            Some(Hit::Shape { id, text }) => {
                let selected = st.selection.shapes.contains(&id);
                let toggle = mods.shift || mods.cmd;
                let ids: Vec<ShapeId> = if toggle {
                    let mut v = st.selection.shapes.clone();
                    if selected {
                        v.retain(|x| *x != id);
                    } else {
                        v.push(id);
                    }
                    v
                } else if selected {
                    st.selection.shapes.clone()
                } else {
                    vec![id]
                };
                let origs: Vec<(ShapeId, Xfrm)> = ids.iter().filter_map(|i| st.shape(*i).map(|s| (*i, xfrm_of(&st.doc, &st.selection, s)))).collect();
                // A click (no drag) on text puts the caret there; unselected text boxes and
                // placeholders take the caret straight away like PowerPoint.
                let enter_text = (text && !toggle && (selected || is_text_first(st, id))).then_some((id, p));
                let ids2 = ids.clone();
                self.select(|_, sel| {
                    sel.shapes = ids2;
                    sel.text = None;
                    sel.cells = None;
                })?;
                self.tool.drag = Some(Drag::Move { ids, start: p, origs, moved: false, duplicate: mods.alt, enter_text });
                // Format Painter applies on click.
                if self.painter.is_some() && !toggle {
                    let _ = self.execute("format.painterApply", &json!({"ids": [id]}));
                }
                // Animation Painter applies on click.
                if self.anim_painter.is_some() && !toggle {
                    let _ = self.execute("animation.painterApply", &json!({"ids": [id]}));
                }
            }
            // `hit_test` reports shapes; grid lines are found (and dragged) by `table_down`.
            Some(Hit::TableDivider { .. }) => {}
            None => {
                let add = mods.shift || mods.cmd;
                self.select(|_, sel| {
                    sel.text = None;
                    sel.cells = None;
                    if !add {
                        sel.shapes.clear();
                    }
                })?;
                self.tool.drag = Some(Drag::Marquee { start: p, add });
                self.tool.marquee = Some(Rect::from_points(p, p));
            }
        }
        Ok(json!({"hit": serde_json::to_value(&hit).unwrap_or_default()}))
    }

    fn drag(&mut self, p: Point, mods: Mods, tol: f64) -> Result<Value> {
        let Some(drag) = self.tool.drag.clone() else { return Ok(Value::Null) };
        match drag {
            Drag::Move { ids, start, origs, moved, duplicate, enter_text } => {
                let mut d = p - start;
                if !moved && d.hypot() < tol.max(1.0) {
                    return Ok(Value::Null);
                }
                if !moved {
                    self.begin(if duplicate { "Duplicate" } else { "Move" });
                    if duplicate {
                        // Alt-drag: drag a copy, leave the original.
                        self.execute_dup_in_place(&ids)?;
                    }
                }
                let ids_now = if duplicate { self.doc()?.selection.shapes.clone() } else { ids.clone() };
                // Shift keeps the move on one axis: the snap does not pull it off it either.
                let lock = mods.shift.then_some(d.x.abs() > d.y.abs());
                match lock {
                    Some(true) => d.y = 0.0,
                    Some(false) => d.x = 0.0,
                    None => {}
                }
                // Smart guides on the moved group's bounds.
                let bounds = origs.iter().map(|(_, x)| x.bounds()).reduce(|a, b| a.union(b)).unwrap_or_default();
                let moved_b = bounds + d;
                let (mut snap, mut guides) = self.snap_rect(moved_b, &ids_now, tol * 1.5, !mods.cmd);
                match lock {
                    Some(true) => snap.y = 0.0,
                    Some(false) => snap.x = 0.0,
                    None => {}
                }
                guides.retain(|g| lock.is_none_or(|across| g.vertical == across));
                d += snap;
                self.tool.guides = guides;
                let origs2: Vec<(ShapeId, Xfrm)> =
                    if duplicate { ids_now.iter().copied().zip(origs.iter().map(|o| o.1)).collect() } else { origs.clone() };
                self.edit(|doc, sel| {
                    let list = crate::shapes_mut(doc, sel).ok_or_else(|| cmd::bad("move", "no slide"))?;
                    for (id, o) in &origs2 {
                        if let Some(sh) = kition_slide_model::find_shape_mut(list, *id) {
                            sh.xfrm = Some(Xfrm { x: o.x + d.x, y: o.y + d.y, ..*o });
                        }
                    }
                    // A connector dragged away from shapes that stay put comes unglued at those ends.
                    let moved: Vec<ShapeId> = origs2.iter().map(|o| o.0).collect();
                    for sh in list.iter_mut().filter(|s| moved.contains(&s.id)) {
                        if let ShapeKind::Connector { start, end } = &mut sh.kind {
                            for c in [start, end] {
                                if c.is_some_and(|(to, _)| !moved.contains(&to)) {
                                    *c = None;
                                }
                            }
                        }
                    }
                    Ok(())
                })?;
                self.tool.drag =
                    Some(Drag::Move { ids: ids_now, start, origs: origs2, moved: true, duplicate: false, enter_text: enter_text.filter(|_| false) });
            }
            Drag::Resize { id, handle, orig, start } => {
                let nx = resize_box(&orig, handle, p - start, mods.shift || self.is_picture(id), mods.alt);
                let nx = self.snap_resize(nx, &orig, handle, id, tol);
                self.set_xfrm(id, nx)?;
            }
            Drag::LineEnd { id, end, orig } => {
                let a = orig.affine();
                let (p0, p1) = (a * Point::new(0.0, 0.0), a * Point::new(orig.w, orig.h));
                let mut q = p;
                let other = if end == 0 { p1 } else { p0 };
                if mods.shift {
                    q = constrain_45(other, q);
                }
                let glue = self.update_sites(q, tol, Some(id));
                if let Some((_, _, at)) = glue {
                    q = at;
                }
                let (s0, s1) = if end == 0 { (q, p1) } else { (p0, q) };
                self.set_xfrm(id, line_xfrm(s0, s1, orig.rot))?;
                self.edit(|doc, sel| {
                    if let Some(list) = crate::shapes_mut(doc, sel) {
                        crate::connect::set_glue(list, id, end, glue.map(|g| (g.0, g.1)));
                    }
                    Ok(())
                })?;
            }
            Drag::Rotate { ids, center, start, origs } => {
                let ang = (p.y - center.y).atan2(p.x - center.x);
                let mut delta = (ang - start).to_degrees();
                for (id, o) in &origs {
                    let mut r = o.rot + delta;
                    if mods.shift {
                        r = (r / 15.0).round() * 15.0;
                        delta = r - o.rot;
                    }
                    let mut nx = *o;
                    nx.rot = ((r % 360.0) + 360.0) % 360.0;
                    if ids.len() > 1 {
                        // Rotate positions around the common centre too.
                        let c = o.center();
                        let rc = Affine::translate(center.to_vec2()) * Affine::rotate(delta.to_radians()) * Affine::translate(-center.to_vec2()) * c;
                        nx.x = rc.x - o.w / 2.0;
                        nx.y = rc.y - o.h / 2.0;
                    }
                    self.set_xfrm(*id, nx)?;
                }
            }
            Drag::Adjust { id, index, orig } => {
                let st = self.doc()?;
                let Some(sh) = st.shape(id).cloned() else { return Ok(Value::Null) };
                let local = orig.affine().inverse() * p;
                let geo = kition_slide_render::shape_geometry(&sh, orig.w, orig.h);
                let Some(h) = geo.handles.iter().find(|h| h.index == index) else { return Ok(Value::Null) };
                let free2d = matches!(sh.geom.preset_name(), Some(n) if n.contains("Callout") && !n.contains("Arrow"));
                let vals: Vec<(usize, f64)> = if free2d {
                    let dx = (local.x - orig.w / 2.0) / orig.w.max(1e-6) * 1e5;
                    let dy = (local.y - orig.h / 2.0) / orig.h.max(1e-6) * 1e5;
                    vec![(0, dx.round()), (1, dy.round())]
                } else {
                    vec![(index, h.value_at(local))]
                };
                self.edit(|doc, sel| {
                    let list = crate::shapes_mut(doc, sel).ok_or_else(|| cmd::bad("adjust", "no slide"))?;
                    if let Some(sh) = kition_slide_model::find_shape_mut(list, id)
                        && let kition_slide_model::Geom::Preset { name, adj } = &mut sh.geom
                    {
                        let defaults = kition_slide_geom::preset::info(name).map(|x| x.defaults).unwrap_or(&[]);
                        for (i, v) in &vals {
                            while adj.len() <= *i {
                                let k = adj.len();
                                adj.push(defaults.get(k).copied().unwrap_or(0.0));
                            }
                            if let Some(slot) = adj.get_mut(*i) {
                                *slot = *v;
                            }
                        }
                    }
                    Ok(())
                })?;
            }
            Drag::Marquee { start, .. } => {
                self.tool.marquee = Some(Rect::from_points(start, p));
            }
            Drag::Create { start } => {
                let line = matches!(&self.tool.kind, ToolKind::Shape { preset } if kition_slide_geom::preset::is_line_like(preset));
                let x = if line {
                    let mut q = if mods.shift { constrain_45(start, p) } else { p };
                    if let Some((_, _, at)) = self.update_sites(q, tol, None) {
                        q = at;
                    }
                    let s0 = self.glue_at(start, tol).map(|g| g.2).unwrap_or(start);
                    line_xfrm(s0, q, 0.0)
                } else {
                    let mut r = Rect::from_points(start, p);
                    if mods.shift {
                        let s = r.width().max(r.height());
                        let sx = if p.x < start.x { -s } else { s };
                        let sy = if p.y < start.y { -s } else { s };
                        r = Rect::from_points(start, start + Vec2::new(sx, sy));
                    }
                    if mods.alt {
                        let d = p - start;
                        r = Rect::new(start.x - d.x.abs(), start.y - d.y.abs(), start.x + d.x.abs(), start.y + d.y.abs());
                    }
                    Xfrm::new(r.x0, r.y0, r.width(), r.height())
                };
                self.tool.preview = Some(x);
            }
            Drag::Divider { id, vertical, index, start, cols, rows } => {
                self.table_divider_drag(id, vertical, index, p - start, &cols, &rows)?;
            }
            Drag::Cells { shape, anchor } => {
                let st = self.doc()?;
                if let Some(cell) = table::hit_cell(st, shape, p)
                    && st.selection.cells != Some((shape, anchor, cell))
                {
                    self.execute("table.selectCells", &json!({"id": shape, "from": anchor, "to": cell}))?;
                }
            }
            Drag::TextSelect { shape } => {
                let st = self.doc()?;
                if let Some(anchor) = st.selection.text.as_ref().and_then(|t| t.cell)
                    && let Some(cell) = table::hit_cell(st, shape, p)
                    && cell != anchor
                {
                    self.execute("table.selectCells", &json!({"id": shape, "from": anchor, "to": cell}))?;
                    self.tool.drag = Some(Drag::Cells { shape, anchor });
                    return Ok(Value::Null);
                }
                // Only a caret that moved changes the selection: the drag reports far more often
                // than it crosses a character, and each change repaints the whole view.
                if let Some(sh) = st.shape(shape)
                    && let pos = text_pos(st, sh, p)
                    && st.selection.text.as_ref().is_some_and(|t| t.caret != pos)
                {
                    self.select(|_, sel| {
                        if let Some(t) = sel.text.as_mut() {
                            t.caret = pos;
                        }
                    })?;
                }
            }
            Drag::Freeform { mode, mut points, pressed } => {
                // Dragging scribbles (except for curves, which only take clicked points).
                if pressed && mode != "curve" && points.last().is_none_or(|l| (*l - p).hypot() >= 1.5) {
                    points.push(p);
                }
                let mut v = points.clone();
                if mode == "curve" {
                    v.push(p);
                }
                self.tool.path_preview = v;
                self.tool.drag = Some(Drag::Freeform { mode, points, pressed });
            }
            Drag::Ink { mut points } => {
                points.push((p.x, p.y, 0.5));
                self.tool.ink_preview = points.clone();
                self.tool.drag = Some(Drag::Ink { points });
            }
            Drag::Erase { mut gesture } => {
                gesture.drag(p);
                self.tool.path_preview = gesture.path().to_vec();
                self.tool.drag = Some(Drag::Erase { gesture });
            }
        }
        Ok(Value::Null)
    }

    fn up(&mut self, p: Point, mods: Mods) -> Result<Value> {
        let drag = self.tool.drag.take();
        self.tool.guides.clear();
        self.tool.sites.clear();
        self.tool.glue = None;
        let mut out = Value::Null;
        match drag {
            Some(Drag::Move { moved: false, enter_text: Some((id, at)), .. }) => {
                // Plain click on text: caret at the click.
                let st = self.doc()?;
                if let Some(sh) = st.shape(id) {
                    let pos = text_pos(st, sh, at);
                    self.execute("text.edit", &json!({"id": id, "at": [pos.0, pos.1]}))?;
                    out = json!({"editing": id});
                }
            }
            Some(Drag::Marquee { start, add }) => {
                let r = Rect::from_points(start, p);
                self.tool.marquee = None;
                if r.width() > 1.0 || r.height() > 1.0 {
                    let st = self.doc()?;
                    let ids: Vec<ShapeId> = st
                        .shapes()
                        .iter()
                        .filter(|s| !s.hidden && !s.locked)
                        .filter(|s| contains_rect(r, xfrm_of(&st.doc, &st.selection, s).bounds()))
                        .map(|s| s.id)
                        .collect();
                    self.select(|_, sel| {
                        if add {
                            for i in &ids {
                                if !sel.shapes.contains(i) {
                                    sel.shapes.push(*i);
                                }
                            }
                        } else {
                            sel.shapes = ids.clone();
                        }
                    })?;
                    out = json!({"selected": ids});
                }
            }
            Some(Drag::Create { start }) => {
                let kind = self.tool.kind.clone();
                let preview = self.tool.preview.take();
                let dragged = (p - start).hypot() > 2.0;
                let res = match &kind {
                    ToolKind::Shape { preset } => {
                        let line = kition_slide_geom::preset::is_line_like(preset);
                        let rect = match preview.filter(|_| dragged) {
                            Some(x) => x,
                            None if line => Xfrm::new(start.x, start.y, 72.0, 0.0),
                            None => Xfrm::new(start.x, start.y, 72.0, 72.0),
                        };
                        let r = self.execute("shape.insert", &json!({"preset": preset, "rect": [rect.x, rect.y, rect.w, rect.h], "flip": format!("{}{}", if rect.flip_h { "h" } else { "" }, if rect.flip_v { "v" } else { "" })}))?;
                        if let (Some(id), Some(x)) = (r.get("id").and_then(Value::as_u64), preview.filter(|_| dragged && line)) {
                            let id = ShapeId(id as u32);
                            let _ = self.set_xfrm_cmd(id, x);
                            // Glue the ends that landed on connection sites.
                            let (p0, p1) = crate::connect::endpoints(&x);
                            let (g0, g1) = (self.glue_at(p0, 4.0), self.glue_at(p1, 4.0));
                            if g0.is_some() || g1.is_some() {
                                let _ = self.edit(|doc, sel| {
                                    if let Some(list) = crate::shapes_mut(doc, sel) {
                                        crate::connect::set_glue(list, id, 0, g0.map(|g| (g.0, g.1)));
                                        crate::connect::set_glue(list, id, 1, g1.map(|g| (g.0, g.1)));
                                    }
                                    Ok(())
                                });
                            }
                        }
                        r
                    }
                    ToolKind::TextBox => {
                        let w = preview.filter(|_| dragged).map(|x| x.w).unwrap_or(0.0);
                        let at = preview.filter(|_| dragged).map(|x| Point::new(x.x, x.y)).unwrap_or(start);
                        self.execute("textBox.insert", &json!({"rect": [at.x, at.y, w, 30.0]}))?
                    }
                    _ => Value::Null,
                };
                if !self.tool.sticky {
                    self.tool.kind = ToolKind::Select;
                }
                out = res;
            }
            Some(Drag::Freeform { mode, points, .. }) => {
                if mode == "scribble" {
                    // A scribble ending back at its start closes into a filled shape.
                    let closed = points.len() > 3 && points.first().zip(points.last()).is_some_and(|(a, b)| (*a - *b).hypot() <= 6.0);
                    self.tool.drag = Some(Drag::Freeform { mode, points, pressed: false });
                    return self.finish_freeform(closed);
                }
                // Polygon / curve: keep going until double-click, Enter or a click on the start.
                self.tool.drag = Some(Drag::Freeform { mode, points, pressed: false });
                return Ok(Value::Null);
            }
            Some(Drag::Ink { points }) => {
                self.tool.ink_preview.clear();
                if points.len() > 1 {
                    self.begin("Ink");
                    out = self.add_ink(points)?;
                }
            }
            Some(Drag::Erase { gesture }) => {
                self.tool.path_preview.clear();
                self.begin("Erase");
                let n = crate::cmd::ink::erase_strokes(self, &gesture)?;
                out = json!({ "erased": n });
            }
            Some(Drag::Resize { id, .. }) => {
                self.commit();
                let _ = cmd::text::fit_text_box(self, id);
                self.refit_after_resize(id)?;
            }
            Some(_) => self.commit(),
            None => {}
        }
        // Any open interaction (move etc.) ends here.
        self.commit();
        let _ = mods;
        Ok(out)
    }

    fn double_click(&mut self, p: Point, tol: f64) -> Result<Value> {
        if let Some(Drag::Freeform { mode, mut points, .. }) = self.tool.drag.clone() {
            // The double-click's own click added a duplicate vertex.
            while points.len() > 2 && points.last().zip(points.get(points.len() - 2)).is_some_and(|(a, b)| (*a - *b).hypot() <= tol) {
                points.pop();
            }
            self.tool.drag = Some(Drag::Freeform { mode, points, pressed: false });
            return self.finish_freeform(false);
        }
        if !matches!(self.tool.kind, ToolKind::Select) {
            // Double-click while drawing: lock drawing mode off.
            self.tool.sticky = false;
            return Ok(Value::Null);
        }
        let st = self.doc()?;
        if st.selection.text.is_some() {
            return self.execute("text.selectWord", &json!({}));
        }
        match self.hit_test(p, tol) {
            Some(Hit::Shape { id, .. }) => {
                let st = self.doc()?;
                let Some(sh) = st.shape(id).cloned() else { return Ok(Value::Null) };
                match &sh.kind {
                    ShapeKind::Table(_) => {
                        if let Some(cell) = table::hit_cell(st, id, p) {
                            self.execute("text.edit", &json!({"id": id, "cell": cell}))?;
                            let st = self.doc()?;
                            let pos = text_pos(st, &sh, p);
                            return self.execute("text.selectWord", &json!({"at": pos}));
                        }
                        Ok(Value::Null)
                    }
                    ShapeKind::Shape | ShapeKind::Connector { .. } if !sh.is_line() => {
                        let pos = text_pos(st, &sh, p);
                        self.execute("text.edit", &json!({"id": id, "at": [pos.0, pos.1]}))?;
                        self.execute("text.selectWord", &json!({}))
                    }
                    ShapeKind::Group { .. } => Ok(Value::Null),
                    _ => {
                        self.ui_requests.push(crate::UiRequest::Pane { id: "format".into() });
                        Ok(Value::Null)
                    }
                }
            }
            _ => Ok(Value::Null),
        }
    }

    fn is_picture(&self, id: ShapeId) -> bool {
        self.active().and_then(|d| d.shape(id)).is_some_and(|s| matches!(s.kind, ShapeKind::Picture { .. } | ShapeKind::Media(_)))
    }

    /// End the freeform being drawn: create the shape (if it has at least two points).
    pub fn finish_freeform(&mut self, closed: bool) -> Result<Value> {
        let Some(Drag::Freeform { mode, points, .. }) = self.tool.drag.take() else { return Ok(Value::Null) };
        self.tool.path_preview.clear();
        self.tool.path_closing = false;
        if !self.tool.sticky {
            self.tool.kind = ToolKind::Select;
        }
        if points.len() < 2 {
            return Ok(Value::Null);
        }
        let pts: Vec<[f64; 2]> = points.iter().map(|q| [q.x, q.y]).collect();
        self.execute("shape.freeform", &json!({"points": pts, "closed": closed, "smooth": mode == "curve"}))
    }

    /// The connection site within glue range of `p` (excluding shape `exclude`).
    fn glue_at(&self, p: Point, tol: f64) -> Option<(ShapeId, u32, Point)> {
        let st = self.active()?;
        crate::connect::nearest_site(&st.doc, &st.selection, st.shapes(), p, (tol * 2.0).max(6.0), None)
    }

    /// Show the sites of the shape near `p` and return the site an end at `p` would glue to.
    fn update_sites(&mut self, p: Point, tol: f64, exclude: Option<ShapeId>) -> Option<(ShapeId, u32, Point)> {
        let (sites, glue) = match self.active() {
            Some(st) => {
                let shapes = st.shapes();
                let near = crate::connect::site_shape_at(&st.doc, &st.selection, shapes, p, tol * 3.0, exclude);
                let sites = near
                    .and_then(|id| shapes.iter().find(|s| s.id == id))
                    .map(|s| crate::connect::sites(&st.doc, &st.selection, s))
                    .unwrap_or_default();
                (sites, crate::connect::nearest_site(&st.doc, &st.selection, shapes, p, (tol * 2.0).max(6.0), exclude))
            }
            None => (vec![], None),
        };
        self.tool.sites = sites;
        self.tool.glue = glue.map(|g| g.2);
        glue
    }

    fn set_xfrm(&mut self, id: ShapeId, x: Xfrm) -> Result<()> {
        // Rows scale from their drawn heights: a stored height is only a minimum (often 0).
        let drawn = self.active().and_then(|st| match &st.shape(id)?.kind {
            ShapeKind::Table(t) => Some(table_rows(st, t)),
            _ => None,
        });
        self.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).ok_or_else(|| cmd::bad("tool", "no slide"))?;
            if let Some(sh) = kition_slide_model::find_shape_mut(list, id) {
                sh.xfrm = Some(x);
                if let ShapeKind::Table(t) = &mut sh.kind {
                    // Tables resize their columns and rows proportionally.
                    let heights: Vec<f64> = match &drawn {
                        Some(d) if d.len() == t.rows.len() => d.clone(),
                        _ => t.rows.iter().map(|r| r.height).collect(),
                    };
                    let (tw, th) = (t.width().max(1e-6), heights.iter().map(|h| h.max(0.0)).sum::<f64>());
                    let kx = x.w / tw;
                    t.cols.iter_mut().for_each(|c| *c *= kx);
                    if th > 1e-6 {
                        let ky = x.h / th;
                        t.rows.iter_mut().zip(&heights).for_each(|(r, h)| r.height = h.max(0.0) * ky);
                    }
                }
            }
            Ok(())
        })
    }

    fn set_xfrm_cmd(&mut self, id: ShapeId, x: Xfrm) -> Result<Value> {
        self.execute("shape.setBounds", &json!({"id": id, "x": x.x, "y": x.y, "w": x.w, "h": x.h}))?;
        if x.flip_h || x.flip_v {
            self.edit(|doc, sel| {
                if let Some(sh) = crate::shapes_mut(doc, sel).and_then(|l| kition_slide_model::find_shape_mut(l, id))
                    && let Some(xx) = sh.xfrm.as_mut()
                {
                    xx.flip_h = x.flip_h;
                    xx.flip_v = x.flip_v;
                }
                Ok(())
            })?;
        }
        Ok(Value::Null)
    }

    fn refit_after_resize(&mut self, id: ShapeId) -> Result<()> {
        // A text box resized by hand keeps wrapping at its new width.
        let st = self.doc()?;
        if st.shape(id).is_some_and(|s| s.text_box) {
            let _ = cmd::text::fit_text_box(self, id);
        }
        Ok(())
    }

    fn execute_dup_in_place(&mut self, ids: &[ShapeId]) -> Result<()> {
        self.edit(|doc, sel| {
            let list = cmd::current_shapes(doc, sel).cloned().unwrap_or_default();
            let mut copies: Vec<Shape> = list.iter().filter(|x| ids.contains(&x.id)).cloned().collect();
            let boxes: Vec<Xfrm> = copies.iter().map(|c| xfrm_of(doc, sel, c)).collect();
            cmd::edit::reid(doc, &mut copies);
            for (c, b) in copies.iter_mut().zip(boxes) {
                c.xfrm = Some(b);
                c.ph = None;
            }
            let new_ids: Vec<ShapeId> = copies.iter().map(|c| c.id).collect();
            let shapes = crate::shapes_mut(doc, sel).ok_or_else(|| cmd::bad("duplicate", "no slide"))?;
            shapes.extend(copies);
            sel.shapes = new_ids;
            Ok(())
        })
    }

    /// Smart guides: snap `r` edges/centres to the slide and other shapes. Returns the offset to
    /// apply and the guides to draw.
    fn snap_rect(&self, r: Rect, exclude: &[ShapeId], tol: f64, enabled: bool) -> (Vec2, Vec<Guide>) {
        let Some(st) = self.active() else { return (Vec2::ZERO, vec![]) };
        if !enabled || !self.prefs.smart_guides {
            return (Vec2::ZERO, vec![]);
        }
        let size = st.doc.slide_size;
        let mut xs: Vec<(f64, Rect)> = vec![
            (0.0, Rect::new(0.0, 0.0, 0.0, size.height)),
            (size.width / 2.0, Rect::new(size.width / 2.0, 0.0, size.width / 2.0, size.height)),
            (size.width, Rect::new(size.width, 0.0, size.width, size.height)),
        ];
        let mut ys: Vec<(f64, Rect)> = vec![
            (0.0, Rect::new(0.0, 0.0, size.width, 0.0)),
            (size.height / 2.0, Rect::new(0.0, size.height / 2.0, size.width, size.height / 2.0)),
            (size.height, Rect::new(0.0, size.height, size.width, size.height)),
        ];
        for sh in st.shapes() {
            if exclude.contains(&sh.id) || sh.hidden {
                continue;
            }
            let b = xfrm_of(&st.doc, &st.selection, sh).bounds();
            for x in [b.x0, b.center().x, b.x1] {
                xs.push((x, b));
            }
            for y in [b.y0, b.center().y, b.y1] {
                ys.push((y, b));
            }
        }
        let mut best_x: Option<(f64, f64, Rect)> = None;
        for edge in [r.x0, r.center().x, r.x1] {
            for (x, b) in &xs {
                let d = x - edge;
                if d.abs() <= tol && best_x.is_none_or(|(bd, _, _)| d.abs() < bd.abs()) {
                    best_x = Some((d, *x, *b));
                }
            }
        }
        let mut best_y: Option<(f64, f64, Rect)> = None;
        for edge in [r.y0, r.center().y, r.y1] {
            for (y, b) in &ys {
                let d = y - edge;
                if d.abs() <= tol && best_y.is_none_or(|(bd, _, _)| d.abs() < bd.abs()) {
                    best_y = Some((d, *y, *b));
                }
            }
        }
        let mut off = Vec2::ZERO;
        let mut guides = vec![];
        if let Some((d, x, b)) = best_x {
            off.x = d;
            let rr = r + Vec2::new(d, 0.0);
            guides.push(Guide { vertical: true, pos: x, from: b.y0.min(rr.y0), to: b.y1.max(rr.y1) });
        } else if self.prefs.snap_to_grid {
            off.x = kition_slide_geom::snap(r.x0, self.prefs.grid_spacing) - r.x0;
        }
        if let Some((d, y, b)) = best_y {
            off.y = d;
            let rr = r + Vec2::new(0.0, d);
            guides.push(Guide { vertical: false, pos: y, from: b.x0.min(rr.x0), to: b.x1.max(rr.x1) });
        } else if self.prefs.snap_to_grid {
            off.y = kition_slide_geom::snap(r.y0, self.prefs.grid_spacing) - r.y0;
        }
        (off, guides)
    }

    fn snap_resize(&mut self, x: Xfrm, _orig: &Xfrm, handle: u8, id: ShapeId, tol: f64) -> Xfrm {
        if x.rot != 0.0 {
            self.tool.guides.clear();
            return x;
        }
        let r = x.rect();
        let (off, guides) = self.snap_rect(r, &[id], tol * 1.5, true);
        let mut nx = x;
        // Only the dragged edges move.
        let moves_left = matches!(handle, 0 | 6 | 7);
        let moves_right = matches!(handle, 2..=4);
        let moves_top = matches!(handle, 0..=2);
        let moves_bottom = matches!(handle, 4..=6);
        if moves_left {
            nx.x += off.x;
            nx.w -= off.x;
        } else if moves_right {
            nx.w += off.x;
        }
        if moves_top {
            nx.y += off.y;
            nx.h -= off.y;
        } else if moves_bottom {
            nx.h += off.y;
        }
        nx.w = nx.w.max(0.0);
        nx.h = nx.h.max(0.0);
        self.tool.guides = guides;
        nx
    }

    fn add_ink(&mut self, points: Vec<(f64, f64, f32)>) -> Result<Value> {
        let ToolKind::Ink { mode, color, width } = self.tool.kind.clone() else { return Ok(Value::Null) };
        // Simplified to within a tenth of the pen's width, as every kit's pen stroke is.
        let raw: Vec<[f64; 3]> = points.iter().map(|(x, y, p)| [*x, *y, f64::from(*p)]).collect();
        let points =
            kition_office_core::ink::simplify(&raw, (width / 10.0).clamp(0.05, 1.0)).into_iter().map(|q| (q[0], q[1], q[2] as f32)).collect();
        let stroke = kition_slide_model::InkStroke { points, color, width, highlighter: mode == "highlighter" };
        let id = crate::cmd::ink::push_strokes(self, vec![stroke])?;
        Ok(json!({"ink": id}))
    }
}

fn is_text_first(st: &crate::DocState, id: ShapeId) -> bool {
    st.shape(id).is_some_and(|s| s.ph.is_some() || s.text_box)
}

/// The key of the nearest of `items` (key, distance) within `reach`.
fn nearest<K: Copy>(items: impl Iterator<Item = (K, f64)>, reach: f64) -> Option<(K, f64)> {
    items.filter(|(_, d)| d.is_finite() && *d <= reach).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The direction resize handle `handle` of frame `x` pulls on screen, from 0° to 180°.
fn resize_pointing(x: &Xfrm, handle: u8) -> Pointing {
    const DIRS: [(f64, f64); 8] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];
    let Some(&(dx, dy)) = DIRS.get(usize::from(handle)) else { return Pointing::Move };
    let local = Point::new(if x.flip_h { -dx } else { dx }, if x.flip_v { -dy } else { dy });
    let d = Affine::rotate(x.rot.to_radians()) * local;
    let angle = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
    Pointing::Resize { angle: if angle.is_finite() { angle } else { 0.0 } }
}

/// Whether `p` is on the lines of shape `sh`'s text (not on the shape's empty space).
fn over_lines(st: &crate::DocState, sh: &Shape, p: Point, tol: f64) -> bool {
    let t = TextSel { shape: sh.id, ..Default::default() };
    let Some(layout) = cmd::text::layout_for(st, &t) else { return false };
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let rect = kition_slide_render::shape_geometry(sh, x.w, x.h).text_rect;
    let local = kition_slide_render::text_transform(x.affine(), rect, layout.rotation).inverse() * p;
    layout.lines.iter().any(|l| l.extent.1 > l.extent.0 && Rect::new(l.extent.0, l.top, l.extent.1, l.bottom).inflate(tol, tol).contains(local))
}

fn contains_rect(outer: Rect, inner: Rect) -> bool {
    inner.x0 >= outer.x0 && inner.y0 >= outer.y0 && inner.x1 <= outer.x1 && inner.y1 <= outer.y1
}

fn constrain_45(a: Point, b: Point) -> Point {
    let d = b - a;
    let ang = d.y.atan2(d.x);
    let snapped = (ang / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
    let len = d.hypot();
    a + Vec2::new(snapped.cos(), snapped.sin()) * len
}

/// A line-like shape's box for endpoints `a` → `b` (flips encode direction).
pub fn line_xfrm(a: Point, b: Point, _rot: f64) -> Xfrm {
    let r = Rect::from_points(a, b);
    Xfrm { x: r.x0, y: r.y0, w: r.width(), h: r.height(), rot: 0.0, flip_h: b.x < a.x, flip_v: b.y < a.y }
}

fn hit_shape(st: &crate::DocState, sh: &Shape, p: Point, tol: f64) -> Option<Hit> {
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let inv = x.affine().inverse();
    let local = inv * p;
    match &sh.kind {
        ShapeKind::Group { children, child } => {
            // A group is hit when any child is hit; the group is what gets selected.
            let ch = Rect::new(child.x, child.y, child.x + child.w, child.y + child.h);
            let m = kition_slide_geom::group_child_affine(&x, ch);
            let q = m.inverse() * p;
            for c in children.iter().rev() {
                if let Some(cx) = c.xfrm {
                    let cl = cx.affine().inverse() * q;
                    if cl.x >= -tol && cl.y >= -tol && cl.x <= cx.w + tol && cl.y <= cx.h + tol {
                        return Some(Hit::Shape { id: sh.id, text: false });
                    }
                }
            }
            None
        }
        ShapeKind::Ink { strokes } => {
            for s in strokes {
                if s.points.iter().any(|(px, py, _)| (Point::new(*px, *py) - p).hypot() <= tol + s.width) {
                    return Some(Hit::Shape { id: sh.id, text: false });
                }
            }
            None
        }
        _ => {
            if sh.is_line() {
                let geo = kition_slide_render::shape_geometry(sh, x.w, x.h);
                let lw = sh.line.as_ref().and_then(|l| l.width).unwrap_or(1.0);
                if kition_slide_geom::near_path(&geo.outline(), local, tol.max(lw)) {
                    return Some(Hit::Shape { id: sh.id, text: false });
                }
                return None;
            }
            let inside = local.x >= -tol && local.y >= -tol && local.x <= x.w + tol && local.y <= x.h + tol;
            if !inside {
                return None;
            }
            let has_text = matches!(sh.kind, ShapeKind::Shape) && (sh.text.as_ref().is_some_and(|t| !t.is_empty()) || sh.ph.is_some() || sh.text_box);
            // Empty interiors of unfilled, text-less shapes don't catch clicks.
            let filled = !matches!(sh.fill, Some(kition_slide_model::Fill::None)) || sh.style.is_some();
            let geo = kition_slide_render::shape_geometry(sh, x.w, x.h);
            let in_geo = {
                use kition_slide_geom::KurboShape;
                geo.outline().winding(local) != 0 || kition_slide_geom::near_path(&geo.outline(), local, tol)
            };
            if has_text
                || sh.ph.is_some()
                || matches!(
                    sh.kind,
                    ShapeKind::Picture { .. } | ShapeKind::Table(_) | ShapeKind::Chart(_) | ShapeKind::Media(_) | ShapeKind::Opaque { .. }
                )
                || (filled && in_geo)
                || kition_slide_geom::near_path(&geo.outline(), local, tol)
            {
                return Some(Hit::Shape { id: sh.id, text: has_text || matches!(sh.kind, ShapeKind::Shape) });
            }
            None
        }
    }
}

/// Text position under slide point `p` in shape `sh`.
pub fn text_pos(st: &crate::DocState, sh: &Shape, p: Point) -> (usize, usize) {
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let cell = st.selection.text.as_ref().filter(|t| t.shape == sh.id).and_then(|t| t.cell);
    let t = TextSel { shape: sh.id, cell, ..Default::default() };
    match cmd::text::layout_for(st, &t) {
        Some(l) => {
            // The text's own transform: flips don't mirror text, vertical text is turned.
            let tr = cell
                .and_then(|cell| cmd::table::cell_rect(st, sh.id, cell))
                .unwrap_or_else(|| kition_slide_render::shape_geometry(sh, x.w, x.h).text_rect);
            let local = kition_slide_render::text_transform(x.affine(), tr, l.rotation).inverse() * p;
            let h = l.hit(local);
            (h.para, h.ch)
        }
        None => (0, 0),
    }
}

/// Row heights of table `t` as drawn: rows grow to fit their text.
pub fn table_rows(st: &crate::DocState, t: &kition_slide_model::Table) -> Vec<f64> {
    cmd::table_drawn_heights(&st.doc, &st.selection, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_corner_and_flip() {
        let o = Xfrm::new(100.0, 100.0, 100.0, 50.0);
        let n = resize_box(&o, 4, Vec2::new(20.0, 10.0), false, false);
        assert!((n.x - 100.0).abs() < 1e-9 && (n.w - 120.0).abs() < 1e-9 && (n.h - 60.0).abs() < 1e-9);
        let n = resize_box(&o, 0, Vec2::new(10.0, 10.0), false, false);
        assert!((n.x - 110.0).abs() < 1e-9 && (n.w - 90.0).abs() < 1e-9);
        let n = resize_box(&o, 3, Vec2::new(-150.0, 0.0), false, false);
        assert!(n.flip_h && (n.w - 50.0).abs() < 1e-9 && (n.x - 50.0).abs() < 1e-9);
        let n = resize_box(&o, 4, Vec2::new(100.0, 0.0), true, false);
        assert!((n.w / n.h - 2.0).abs() < 1e-9);
        let n = resize_box(&o, 3, Vec2::new(10.0, 0.0), false, true);
        assert!((n.w - 120.0).abs() < 1e-9 && (n.x - 90.0).abs() < 1e-9);
    }

    #[test]
    fn resize_rotated_keeps_opposite_corner() {
        let o = Xfrm { x: 100.0, y: 100.0, w: 100.0, h: 50.0, rot: 90.0, ..Default::default() };
        let anchor = o.affine() * Point::new(0.0, 0.0);
        let n = resize_box(&o, 4, Vec2::new(-10.0, 30.0), false, false);
        let anchor2 = n.affine() * Point::new(0.0, 0.0);
        assert!((anchor - anchor2).hypot() < 1e-6, "{anchor:?} {anchor2:?}");
    }
}
