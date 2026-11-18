//! Arrange: order, group, align, distribute, rotate and flip.

use kition_slide_geom::{Point, Rect, Xfrm};
use kition_slide_model::{Shape, ShapeId, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("arrange.bringToFront", "Bring to Front", ["Arrange"], Some("Cmd+Shift+]"), "{ids?}", has_selection, |s, p| order(s, p, 2)),
        cmd!("arrange.bringForward", "Bring Forward", ["Arrange"], Some("Cmd+]"), "{ids?}", has_selection, |s, p| order(s, p, 1)),
        cmd!("arrange.sendBackward", "Send Backward", ["Arrange"], Some("Cmd+["), "{ids?}", has_selection, |s, p| order(s, p, -1)),
        cmd!("arrange.sendToBack", "Send to Back", ["Arrange"], Some("Cmd+Shift+["), "{ids?}", has_selection, |s, p| order(s, p, -2)),
        cmd!("arrange.reorder", "Reorder Objects", ["Arrange"], None, "{id, index: position in the stack (0 = back)}", has_slide, reorder),
        cmd!("arrange.group", "Group", ["Arrange"], Some("Cmd+Alt+G"), "{ids?}", has_two, group),
        cmd!("arrange.ungroup", "Ungroup", ["Arrange"], Some("Cmd+Alt+Shift+G"), "{ids?}", has_selection, ungroup),
        cmd!("arrange.regroup", "Regroup", ["Arrange"], Some("Cmd+Alt+J"), "{}", has_doc, regroup),
        cmd!(
            "arrange.align",
            "Align",
            ["Arrange", "Align or Distribute"],
            None,
            "{edge: left|center|right|top|middle|bottom, to?: slide|selection (default: slide for one object), ids?}",
            has_selection,
            align
        ),
        cmd!(
            "arrange.distribute",
            "Distribute",
            ["Arrange", "Align or Distribute"],
            None,
            "{dir: horizontal|vertical, to?: slide|selection, ids?}",
            has_selection,
            distribute
        ),
        cmd!("arrange.rotateLeft", "Rotate Left 90°", ["Arrange", "Rotate or Flip"], None, "{ids?}", has_selection, |s, p| s
            .execute("shape.rotate", &with_param(p, "by", json!(-90.0)))),
        cmd!("arrange.rotateRight", "Rotate Right 90°", ["Arrange", "Rotate or Flip"], None, "{ids?}", has_selection, |s, p| s
            .execute("shape.rotate", &with_param(p, "by", json!(90.0)))),
        cmd!("arrange.flipHorizontal", "Flip Horizontal", ["Arrange", "Rotate or Flip"], None, "{ids?}", has_selection, |s, p| s
            .execute("shape.flip", &with_param(p, "axis", json!("horizontal")))),
        cmd!("arrange.flipVertical", "Flip Vertical", ["Arrange", "Rotate or Flip"], None, "{ids?}", has_selection, |s, p| s
            .execute("shape.flip", &with_param(p, "axis", json!("vertical")))),
        cmd!("arrange.nudge", "Nudge", [], None, "{dx, dy, ids?} (arrow keys: 6 pt, with Alt 1 pt)", has_selection, |s, p| s
            .execute("shape.move", p)),
    ]
}

fn order(s: &mut Session, p: &Value, how: i32) -> Result<Value> {
    let ids = targets(s, p)?;
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("arrange", "no slide"))?;
        match how {
            2 => {
                let (mut moved, rest): (Vec<Shape>, Vec<Shape>) = list.drain(..).partition(|x| ids.contains(&x.id));
                list.extend(rest);
                list.append(&mut moved);
            }
            -2 => {
                let (mut moved, rest): (Vec<Shape>, Vec<Shape>) = list.drain(..).partition(|x| ids.contains(&x.id));
                moved.extend(rest);
                *list = moved;
            }
            1 => {
                for i in (0..list.len().saturating_sub(1)).rev() {
                    let (a, b) = (list.get(i).map(|x| ids.contains(&x.id)), list.get(i + 1).map(|x| ids.contains(&x.id)));
                    if a == Some(true) && b == Some(false) {
                        list.swap(i, i + 1);
                    }
                }
            }
            _ => {
                for i in 1..list.len() {
                    let (a, b) = (list.get(i - 1).map(|x| ids.contains(&x.id)), list.get(i).map(|x| ids.contains(&x.id)));
                    if a == Some(false) && b == Some(true) {
                        list.swap(i - 1, i);
                    }
                }
            }
        }
        Ok(())
    })?;
    ok()
}

fn reorder(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param(p, "id").ok_or_else(|| bad("arrange.reorder", "missing `id`"))?;
    let to = usize_param(p, "index").ok_or_else(|| bad("arrange.reorder", "missing `index`"))?;
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("arrange.reorder", "no slide"))?;
        let i = list.iter().position(|x| x.id == id).ok_or_else(|| bad("arrange.reorder", "not a top-level shape"))?;
        let sh = list.remove(i);
        list.insert(to.min(list.len()), sh);
        Ok(())
    })?;
    ok()
}

/// Bounds of shapes (rotation included).
fn union_bounds(xs: &[Xfrm]) -> Option<Rect> {
    let mut it = xs.iter().map(Xfrm::bounds);
    let first = it.next()?;
    Some(it.fold(first, |a, b| a.union(b)))
}

fn group(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    s.edit(|doc, sel| {
        let current = super::current_shapes(doc, sel).cloned().unwrap_or_default();
        let mut members: Vec<Shape> = current.iter().filter(|x| ids.contains(&x.id)).cloned().collect();
        if members.len() < 2 {
            return Err(bad("arrange.group", "select two or more objects on the slide"));
        }
        for m in &mut members {
            if m.xfrm.is_none() {
                m.xfrm = Some(xfrm_of(doc, sel, m));
            }
            // Placeholders lose their link when grouped (PowerPoint keeps them out of groups).
            m.ph = None;
        }
        let xs: Vec<Xfrm> = members.iter().filter_map(|m| m.xfrm).collect();
        let b = union_bounds(&xs).ok_or_else(|| bad("arrange.group", "nothing to group"))?;
        let gx = Xfrm::new(b.x0, b.y0, b.width(), b.height());
        let id = ShapeId(doc.alloc_id());
        let n = super::current_shapes(doc, sel).map(|v| v.len()).unwrap_or(0);
        let g = Shape {
            id,
            name: format!("Group {}", n + 1),
            xfrm: Some(gx),
            kind: ShapeKind::Group { children: members, child: gx },
            ..Default::default()
        };
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("arrange.group", "no slide"))?;
        let at = list.iter().rposition(|x| ids.contains(&x.id)).unwrap_or(list.len());
        list.insert(at + 1, g);
        list.retain(|x| !ids.contains(&x.id));
        sel.shapes = vec![id];
        Ok(json!({"id": id}))
    })
}

/// Refit the groups on the edited slide whose children an edit moved or resized: the group's
/// child space becomes the children's bounds again and its box follows, the children staying
/// where they are drawn (PowerPoint recomputes a group's extents when a member changes). Groups
/// the edit did not touch keep their boxes as read.
pub(crate) fn refit_groups(old: &kition_slide_model::SlideFile, doc: &mut kition_slide_model::SlideFile, sel: &crate::Selection) {
    let before = current_shapes(old, sel).map(Vec::as_slice).unwrap_or_default();
    let after = current_shapes(doc, sel).map(Vec::as_slice).unwrap_or_default();
    let changed = after.iter().any(|s| {
        matches!(s.kind, ShapeKind::Group { .. })
            && before.iter().find(|b| b.id == s.id).is_some_and(|b| matches!(b.kind, ShapeKind::Group { .. }) && b.kind != s.kind)
    });
    if !changed {
        return;
    }
    if let Some(list) = crate::shapes_mut(doc, sel) {
        refit_list(before, list, 0);
    }
}

fn refit_list(before: &[Shape], list: &mut [Shape], depth: usize) {
    if depth > 32 {
        return;
    }
    for sh in list.iter_mut() {
        let Some(was) = before.iter().find(|b| b.id == sh.id) else { continue };
        let (ShapeKind::Group { children, child }, ShapeKind::Group { children: old_children, .. }) = (&mut sh.kind, &was.kind) else { continue };
        refit_list(old_children, children, depth + 1);
        // Only a member moved, resized, added or removed changes the group's extents; text and
        // formatting do not.
        let boxes = |list: &[Shape]| list.iter().map(|c| (c.id, c.xfrm)).collect::<Vec<_>>();
        if boxes(children) == boxes(old_children) {
            continue;
        }
        let Some(u) = union_bounds(&children.iter().filter_map(|c| c.xfrm).collect::<Vec<_>>()) else { continue };
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        if near(u.x0, child.x) && near(u.y0, child.y) && near(u.width(), child.w) && near(u.height(), child.h) {
            continue;
        }
        let Some(gx) = sh.xfrm else { continue };
        let sx = if child.w.abs() > 1e-9 { gx.w / child.w } else { 1.0 };
        let sy = if child.h.abs() > 1e-9 { gx.h / child.h } else { 1.0 };
        let a = kition_slide_geom::group_child_affine(&gx, Rect::new(child.x, child.y, child.x + child.w, child.y + child.h));
        let c = a * u.center();
        let (w, h) = (u.width() * sx, u.height() * sy);
        if ![c.x, c.y, w, h].iter().all(|v| v.is_finite()) {
            continue;
        }
        sh.xfrm = Some(Xfrm { x: c.x - w / 2.0, y: c.y - h / 2.0, w, h, ..gx });
        *child = Xfrm::new(u.x0, u.y0, u.width(), u.height());
    }
}

/// A child's xfrm in slide space, given its group.
pub(crate) fn child_to_parent(group: &Xfrm, ch: &Xfrm, c: &Xfrm) -> Xfrm {
    let sx = if ch.w.abs() > 1e-9 { group.w / ch.w } else { 1.0 };
    let sy = if ch.h.abs() > 1e-9 { group.h / ch.h } else { 1.0 };
    let a = kition_slide_geom::group_child_affine(group, Rect::new(ch.x, ch.y, ch.x + ch.w, ch.y + ch.h));
    let center = a * Point::new(c.x + c.w / 2.0, c.y + c.h / 2.0);
    let (w, h) = (c.w * sx.abs(), c.h * sy.abs());
    let mut rot = c.rot + group.rot;
    let (mut fh, mut fv) = (c.flip_h, c.flip_v);
    if group.flip_h {
        fh = !fh;
        rot = -c.rot + group.rot;
    }
    if group.flip_v {
        fv = !fv;
        rot = -rot + 2.0 * group.rot;
    }
    Xfrm { x: center.x - w / 2.0, y: center.y - h / 2.0, w, h, rot: ((rot % 360.0) + 360.0) % 360.0, flip_h: fh, flip_v: fv }
}

/// The inverse of [`child_to_parent`]: a box on the group's parent as a box in its child space.
pub(crate) fn parent_to_child(group: &Xfrm, ch: &Xfrm, x: &Xfrm) -> Xfrm {
    let sx = if ch.w.abs() > 1e-9 { group.w / ch.w } else { 1.0 };
    let sy = if ch.h.abs() > 1e-9 { group.h / ch.h } else { 1.0 };
    let a = kition_slide_geom::group_child_affine(group, Rect::new(ch.x, ch.y, ch.x + ch.w, ch.y + ch.h));
    let center = a.inverse() * Point::new(x.x + x.w / 2.0, x.y + x.h / 2.0);
    let w = if sx.abs() > 1e-9 { x.w / sx.abs() } else { x.w };
    let h = if sy.abs() > 1e-9 { x.h / sy.abs() } else { x.h };
    // One flip of the group mirrors the child's turn; none or both keep it.
    let rot = if group.flip_h != group.flip_v { group.rot - x.rot } else { x.rot - group.rot };
    let (fh, fv) = (x.flip_h != group.flip_h, x.flip_v != group.flip_v);
    Xfrm { x: center.x - w / 2.0, y: center.y - h / 2.0, w, h, rot: rot.rem_euclid(360.0), flip_h: fh, flip_v: fv }
}

fn ungroup(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    s.edit(|doc, sel| {
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("arrange.ungroup", "no slide"))?;
        let mut out = Vec::with_capacity(list.len());
        let mut new_sel = vec![];
        for sh in list.drain(..) {
            if ids.contains(&sh.id)
                && let (ShapeKind::Group { children, child }, Some(gx)) = (&sh.kind, sh.xfrm)
            {
                for c in children {
                    let mut c = c.clone();
                    if let Some(cx) = c.xfrm {
                        c.xfrm = Some(child_to_parent(&gx, child, &cx));
                    }
                    // Nested groups scale too: their child space stays, their box is mapped.
                    new_sel.push(c.id);
                    out.push(c);
                }
                continue;
            }
            out.push(sh);
        }
        *list = out;
        sel.shapes = new_sel.clone();
        Ok(json!({"ids": new_sel}))
    })
}

fn regroup(s: &mut Session, _p: &Value) -> Result<Value> {
    // Regroup the most recently ungrouped objects (from the undo history).
    let st = s.doc()?;
    let last = st.history.undo.iter().rev().find(|e| e.label == "Ungroup").map(|e| e.selection.shapes.clone());
    match last {
        Some(prev_group) => {
            // The ungroup's selection held the group id; its children are the current selection when nothing else changed.
            let _ = prev_group;
            let ids = s.doc()?.selection.shapes.clone();
            if ids.len() >= 2 { s.execute("arrange.group", &json!({"ids": ids})) } else { Err(bad("arrange.regroup", "nothing to regroup")) }
        }
        None => Err(bad("arrange.regroup", "nothing to regroup")),
    }
}

fn align(s: &mut Session, p: &Value) -> Result<Value> {
    let edge = str_param(p, "edge").unwrap_or("left").to_string();
    let ids = targets(s, p)?;
    let st = s.doc()?;
    let size = st.doc.slide_size;
    let boxes: Vec<(ShapeId, Xfrm)> = ids.iter().filter_map(|id| st.shape(*id).map(|x| (*id, slide_xfrm_of(&st.doc, &st.selection, x)))).collect();
    let to_slide = match str_param(p, "to") {
        Some("slide") => true,
        Some(_) => false,
        None => boxes.len() < 2,
    };
    let reference = if to_slide {
        Rect::new(0.0, 0.0, size.width, size.height)
    } else {
        union_bounds(&boxes.iter().map(|b| b.1).collect::<Vec<_>>()).unwrap_or_default()
    };
    edit_shapes(s, p, "arrange.align", |sh| {
        if let Some(x) = sh.xfrm.as_mut() {
            let b = x.bounds();
            let (dx, dy) = match edge.as_str() {
                "left" => (reference.x0 - b.x0, 0.0),
                "center" => (reference.center().x - b.center().x, 0.0),
                "right" => (reference.x1 - b.x1, 0.0),
                "top" => (0.0, reference.y0 - b.y0),
                "middle" => (0.0, reference.center().y - b.center().y),
                "bottom" => (0.0, reference.y1 - b.y1),
                _ => (0.0, 0.0),
            };
            x.x += dx;
            x.y += dy;
        }
        Ok(())
    })
}

fn distribute(s: &mut Session, p: &Value) -> Result<Value> {
    let horiz = !str_param(p, "dir").unwrap_or("horizontal").starts_with('v');
    let ids = targets(s, p)?;
    let st = s.doc()?;
    let size = st.doc.slide_size;
    let mut boxes: Vec<(ShapeId, Rect)> =
        ids.iter().filter_map(|id| st.shape(*id).map(|x| (*id, slide_xfrm_of(&st.doc, &st.selection, x).bounds()))).collect();
    let to_slide = str_param(p, "to") == Some("slide") || boxes.len() < 3;
    if boxes.is_empty() {
        return ok();
    }
    boxes.sort_by(|a, b| if horiz { a.1.x0.total_cmp(&b.1.x0) } else { a.1.y0.total_cmp(&b.1.y0) });
    let total: f64 = boxes.iter().map(|b| if horiz { b.1.width() } else { b.1.height() }).sum();
    let (start, end) = if to_slide {
        if horiz { (0.0, size.width) } else { (0.0, size.height) }
    } else {
        let f = boxes.first().map(|b| b.1).unwrap_or_default();
        let l = boxes.last().map(|b| b.1).unwrap_or_default();
        if horiz { (f.x0, l.x1) } else { (f.y0, l.y1) }
    };
    let n = boxes.len();
    let gap = if n > 1 { ((end - start) - total) / (n - 1) as f64 } else { 0.0 };
    let mut targets_pos: Vec<(ShapeId, f64)> = vec![];
    let mut cur = start;
    if n == 1 {
        let b = boxes.first().map(|b| b.1).unwrap_or_default();
        cur = start + ((end - start) - if horiz { b.width() } else { b.height() }) / 2.0;
    }
    for (id, b) in &boxes {
        let d = cur - if horiz { b.x0 } else { b.y0 };
        targets_pos.push((*id, d));
        cur += (if horiz { b.width() } else { b.height() }) + gap;
    }
    edit_shapes(s, p, "arrange.distribute", |sh| {
        if let (Some(x), Some((_, d))) = (sh.xfrm.as_mut(), targets_pos.iter().find(|(i, _)| *i == sh.id)) {
            if horiz {
                x.x += d;
            } else {
                x.y += d;
            }
        }
        Ok(())
    })
}
