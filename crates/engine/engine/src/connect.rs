//! Connectors glued to shapes: connection sites, snapping line ends to them, and rerouting attached
//! connectors whenever the shapes they connect move, resize or rotate.

use kition_slide_geom::{Point, Xfrm};
use kition_slide_model::{Shape, ShapeId, ShapeKind, SlideFile};

use crate::Selection;
use crate::cmd::xfrm_of;

/// Connection sites of `s` in slide coordinates (index = site index stored on connectors).
pub fn sites(doc: &SlideFile, sel: &Selection, s: &Shape) -> Vec<Point> {
    if s.is_line() || matches!(s.kind, ShapeKind::Group { .. } | ShapeKind::Ink { .. }) {
        return vec![];
    }
    let x = xfrm_of(doc, sel, s);
    let mut local = kition_slide_render::shape_geometry(s, x.w, x.h).sites;
    if local.is_empty() {
        local = vec![Point::new(x.w / 2.0, 0.0), Point::new(0.0, x.h / 2.0), Point::new(x.w / 2.0, x.h), Point::new(x.w, x.h / 2.0)];
    }
    let a = x.affine();
    local.into_iter().map(|p| a * p).collect()
}

/// The nearest connection site within `radius` of `p` on any visible shape except `exclude`.
pub fn nearest_site(
    doc: &SlideFile,
    sel: &Selection,
    shapes: &[Shape],
    p: Point,
    radius: f64,
    exclude: Option<ShapeId>,
) -> Option<(ShapeId, u32, Point)> {
    let mut best: Option<(f64, ShapeId, u32, Point)> = None;
    for s in shapes.iter().filter(|s| Some(s.id) != exclude && !s.hidden) {
        for (i, q) in sites(doc, sel, s).into_iter().enumerate() {
            let d = (q - p).hypot();
            if d <= radius && best.is_none_or(|b| d < b.0) {
                best = Some((d, s.id, i as u32, q));
            }
        }
    }
    best.map(|b| (b.1, b.2, b.3))
}

/// The shape whose sites should show while drawing or dragging a connector end near `p`.
pub fn site_shape_at(doc: &SlideFile, sel: &Selection, shapes: &[Shape], p: Point, margin: f64, exclude: Option<ShapeId>) -> Option<ShapeId> {
    shapes
        .iter()
        .rev()
        .filter(|s| Some(s.id) != exclude && !s.hidden && !s.is_line() && !matches!(s.kind, ShapeKind::Ink { .. }))
        .find(|s| xfrm_of(doc, sel, s).bounds().inflate(margin, margin).contains(p))
        .map(|s| s.id)
}

/// Line endpoints (start, end) of a line-like shape's box.
pub fn endpoints(x: &Xfrm) -> (Point, Point) {
    let a = x.affine();
    (a * Point::new(0.0, 0.0), a * Point::new(x.w, x.h))
}

/// Move the ends of connectors glued to shapes on the edited slide to the current site positions.
/// Connections to shapes that no longer exist are dropped.
pub fn reroute(doc: &mut SlideFile, sel: &Selection, before: &SlideFile) {
    let Some(list) = crate::shapes_mut(doc, sel) else { return };
    if !list.iter().any(|s| matches!(s.kind, ShapeKind::Connector { start: Some(_), .. } | ShapeKind::Connector { end: Some(_), .. })) {
        return;
    }
    let snapshot: Vec<Shape> = list.clone();
    let view = doc.clone();
    let old = match sel.target {
        crate::Target::Slides => {
            view.slides.get(sel.slide).and_then(|slide| before.slides.iter().find(|s| s.id == slide.id)).map(|s| s.shapes.as_slice())
        }
        crate::Target::Master { master } => before.masters.get(master).map(|m| m.shapes.as_slice()),
        crate::Target::Layout { master, layout } => before.masters.get(master).and_then(|m| m.layouts.get(layout)).map(|l| l.shapes.as_slice()),
    }
    .unwrap_or_default();
    let site = |id: ShapeId, idx: u32| -> Option<Point> {
        let s = snapshot.iter().find(|s| s.id == id)?;
        let v = sites(&view, sel, s);
        v.get(idx as usize).or(v.first()).copied()
    };
    let mut updates: Vec<(usize, Xfrm, bool, bool)> = vec![];
    for (i, s) in snapshot.iter().enumerate() {
        let ShapeKind::Connector { start, end } = &s.kind else { continue };
        if start.is_none() && end.is_none() {
            continue;
        }
        if let Some(previous) = old.iter().find(|old| old.id == s.id)
            && previous.kind == s.kind
            && previous.xfrm == s.xfrm
            && [start, end].into_iter().flatten().all(|(id, _)| match (old.iter().find(|s| s.id == *id), snapshot.iter().find(|s| s.id == *id)) {
                (Some(a), Some(b)) => a.geom == b.geom && xfrm_of(before, sel, a) == xfrm_of(&view, sel, b),
                (None, None) => true,
                _ => false,
            })
        {
            continue;
        }
        let x = xfrm_of(&view, sel, s);
        let (mut p0, mut p1) = endpoints(&x);
        let a = start.and_then(|(id, k)| site(id, k));
        let b = end.and_then(|(id, k)| site(id, k));
        if let Some(q) = a {
            p0 = q;
        }
        if let Some(q) = b {
            p1 = q;
        }
        let elbow = matches!(s.geom.preset_name(), Some(name) if name.starts_with("bentConnector") || name.starts_with("curvedConnector"));
        // Both glued ends on a top or bottom side: down (or up), across, down. A connector
        // between a top or bottom side and a left or right side keeps its across-first route
        // (PowerPoint uses a two-leg connector there).
        let side = |end: &Option<(ShapeId, u32)>, at: Option<Point>| match (end, at) {
            (Some((id, k)), Some(_)) => Some(site_side_vertical(&view, sel, &snapshot, *id, *k)),
            _ => None,
        };
        let vertical_first = elbow
            && match (side(start, a), side(end, b)) {
                (Some(v0), Some(v1)) => v0 && v1,
                (Some(v), None) | (None, Some(v)) => v,
                (None, None) => (x.rot.rem_euclid(180.0) - 90.0).abs() < 1.0,
            };
        let nx = connector_xfrm(p0, p1, vertical_first);
        let changed = (nx.x - x.x).abs() > 1e-6
            || (nx.y - x.y).abs() > 1e-6
            || (nx.w - x.w).abs() > 1e-6
            || (nx.h - x.h).abs() > 1e-6
            || (nx.rot - x.rot).abs() > 1e-6
            || nx.flip_h != x.flip_h
            || nx.flip_v != x.flip_v;
        let drop_a = start.is_some() && a.is_none();
        let drop_b = end.is_some() && b.is_none();
        if changed || drop_a || drop_b {
            updates.push((i, nx, drop_a, drop_b));
        }
    }
    let Some(list) = crate::shapes_mut(doc, sel) else { return };
    for (i, nx, drop_a, drop_b) in updates {
        if let Some(s) = list.get_mut(i) {
            s.xfrm = Some(nx);
            if let ShapeKind::Connector { start, end } = &mut s.kind {
                if drop_a {
                    *start = None;
                }
                if drop_b {
                    *end = None;
                }
            }
        }
    }
}

/// True when site `k` of shape `id` is on the top or bottom of its shape, so an elbow or curved
/// connector glued there leaves (or arrives) vertically.
fn site_side_vertical(doc: &SlideFile, sel: &Selection, shapes: &[Shape], id: ShapeId, k: u32) -> bool {
    let Some(s) = shapes.iter().find(|s| s.id == id) else { return false };
    let v = sites(doc, sel, s);
    let Some(p) = v.get(k as usize).or(v.first()).copied() else { return false };
    let x = xfrm_of(doc, sel, s);
    let a = x.affine();
    let local = a.inverse() * p;
    // The side the site sits on, in the shape's own frame, turned into slide space.
    let (dl, dr, dt, db) = (local.x.abs(), (x.w - local.x).abs(), local.y.abs(), (x.h - local.y).abs());
    let near = dl.min(dr).min(dt).min(db);
    // The outward normal of that side, in the shape's frame.
    let normal = if near == dt {
        kition_slide_geom::Vec2::new(0.0, -1.0)
    } else if near == db {
        kition_slide_geom::Vec2::new(0.0, 1.0)
    } else if near == dl {
        kition_slide_geom::Vec2::new(-1.0, 0.0)
    } else {
        kition_slide_geom::Vec2::new(1.0, 0.0)
    };
    let d = a * (local + normal) - a * local;
    d.y.abs() > d.x.abs()
}

/// The box of a connector from `p0` to `p1`. Elbow and curved connectors bend in their own frame
/// from the start horizontally first; `vertical_first` turns the frame a quarter turn (as
/// PowerPoint stores a connector leaving a top or bottom site, `rot="5400000"`) so the first and
/// last legs run vertically.
pub fn connector_xfrm(p0: Point, p1: Point, vertical_first: bool) -> Xfrm {
    let straight = crate::tools::line_xfrm(p0, p1, 0.0);
    if !vertical_first {
        return straight;
    }
    let (cx, cy) = ((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
    let (w, h) = (straight.h, straight.w);
    let mut best: Option<(f64, Xfrm)> = None;
    for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
        let x = Xfrm { x: cx - w / 2.0, y: cy - h / 2.0, w, h, rot: 90.0, flip_h, flip_v };
        let (q0, q1) = endpoints(&x);
        let err = (q0 - p0).hypot() + (q1 - p1).hypot();
        if best.is_none_or(|b| err < b.0) {
            best = Some((err, x));
        }
    }
    best.map(|b| b.1).unwrap_or(straight)
}

/// Glue end `end` (0 = start, 1 = end) of line `id` to `to` (or unglue with `None`). Plain lines
/// become connectors when glued.
pub fn set_glue(list: &mut [Shape], id: ShapeId, end: u8, to: Option<(ShapeId, u32)>) {
    let Some(s) = list.iter_mut().find(|s| s.id == id) else { return };
    if !matches!(s.kind, ShapeKind::Connector { .. }) {
        if to.is_none() || !matches!(s.kind, ShapeKind::Shape) {
            return;
        }
        s.kind = ShapeKind::Connector { start: None, end: None };
    }
    if let ShapeKind::Connector { start, end: e } = &mut s.kind {
        if end == 0 {
            *start = to;
        } else {
            *e = to;
        }
    }
}
