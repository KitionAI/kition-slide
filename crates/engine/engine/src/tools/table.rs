//! Cell hit testing, cell editing and divider dragging reuse the renderer's grown rows and merged
//! rectangles, so what the pointer hits is what the slide shows.

use super::*;

/// Narrowest column or row a divider drag leaves, in points.
const MIN_TRACK: f64 = 8.0;

pub(super) fn hit_cell(st: &crate::DocState, id: ShapeId, p: Point) -> Option<(usize, usize)> {
    let sh = st.shape(id)?;
    let ShapeKind::Table(table) = &sh.kind else { return None };
    let local = xfrm_of(&st.doc, &st.selection, sh).affine().inverse() * p;
    kition_slide_render::table_cell_rects(table, &table_rows(st, table)).into_iter().find(|(_, _, rect)| rect.contains(local)).map(|(r, c, _)| (r, c))
}

/// The interior grid line under `p` where it is drawn (a merged cell hides the part it spans):
/// `(vertical, index)`, `index` being the column (or row) that starts at the line.
pub(super) fn divider_at(st: &crate::DocState, id: ShapeId, p: Point, tol: f64) -> Option<(bool, usize)> {
    let sh = st.shape(id)?;
    let ShapeKind::Table(table) = &sh.kind else { return None };
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let local = x.affine().inverse() * p;
    if local.x < -tol || local.y < -tol || local.x > x.w + tol || local.y > x.h + tol {
        return None;
    }
    let heights = table_rows(st, table);
    let rects = kition_slide_render::table_cell_rects(table, &heights);
    // Drawn at `pos` when some cell has an edge there spanning the pointer's position along it.
    let drawn = |vertical: bool, pos: f64| {
        rects.iter().any(|(_, _, r)| {
            let (lo, hi, from, to, along) = if vertical { (r.x0, r.x1, r.y0, r.y1, local.y) } else { (r.y0, r.y1, r.x0, r.x1, local.x) };
            ((lo - pos).abs() < 1e-6 || (hi - pos).abs() < 1e-6) && along >= from && along <= to
        })
    };
    let mut edge = 0.0;
    for (i, w) in table.cols.iter().enumerate().take(table.n_cols().saturating_sub(1)) {
        edge += w;
        if (local.x - edge).abs() <= tol && drawn(true, edge) {
            return Some((true, i + 1));
        }
    }
    let mut edge = 0.0;
    for (i, h) in heights.iter().enumerate().take(heights.len().saturating_sub(1)) {
        edge += h;
        if (local.y - edge).abs() <= tol && drawn(false, edge) {
            return Some((false, i + 1));
        }
    }
    None
}

impl Session {
    /// The grid line of the table under the pointer (hover feedback and divider drags).
    pub(super) fn table_divider_hit(&self, p: Point, tol: f64) -> Option<Hit> {
        let Some(Hit::Shape { id, .. }) = self.hit_test(p, tol) else { return None };
        let (vertical, index) = divider_at(self.active()?, id, p, tol)?;
        Some(Hit::TableDivider { id, vertical, index })
    }

    pub(super) fn table_down(&mut self, p: Point, mods: Mods, tol: f64) -> Result<bool> {
        if !mods.cmd
            && !mods.shift
            && let Some(Hit::TableDivider { id, vertical, index }) = self.table_divider_hit(p, tol)
        {
            let st = self.doc()?;
            let Some(ShapeKind::Table(table)) = st.shape(id).map(|sh| &sh.kind) else { return Ok(false) };
            let (cols, rows) = (table.cols.clone(), table_rows(st, table));
            self.select(|_, sel| {
                sel.shapes = vec![id];
                sel.text = None;
                sel.cells = None;
            })?;
            self.begin("Table Size");
            self.tool.drag = Some(Drag::Divider { id, vertical, index, start: p, cols, rows });
            return Ok(true);
        }
        let Some(Hit::Shape { id, .. }) = self.hit_test(p, tol) else { return Ok(false) };
        let st = self.doc()?;
        let Some(cell) = hit_cell(st, id, p) else { return Ok(false) };
        let sh = st.shape(id).ok_or_else(|| cmd::bad("table", "table disappeared"))?;
        let x = xfrm_of(&st.doc, &st.selection, sh);
        let local = x.affine().inverse() * p;
        // The outer frame remains a move target; handles retain resize and rotation.
        if local.x < tol || local.y < tol || local.x > x.w - tol || local.y > x.h - tol || mods.cmd {
            return Ok(false);
        }
        if mods.shift {
            let anchor = range_anchor(st, id).unwrap_or(cell);
            self.execute("table.selectCells", &json!({"id": id, "from": anchor, "to": cell}))?;
            self.tool.drag = Some(Drag::Cells { shape: id, anchor });
        } else {
            self.execute("text.edit", &json!({"id": id, "cell": cell}))?;
            let st = self.doc()?;
            let pos = st.shape(id).map(|sh| text_pos(st, sh, p)).unwrap_or((0, 0));
            self.execute("text.select", &json!({"anchor": pos, "caret": pos}))?;
            self.tool.drag = Some(Drag::TextSelect { shape: id });
        }
        Ok(true)
    }

    /// Moves a grid line live inside the open "Table Size" interaction, from the sizes the drag
    /// started with: a column line trades width between its two columns (the table keeps its
    /// width); a row line sets the row above it (the table grows or shrinks; rows still grow to
    /// fit their text).
    pub(super) fn table_divider_drag(&mut self, id: ShapeId, vertical: bool, index: usize, delta: Vec2, cols: &[f64], rows: &[f64]) -> Result<()> {
        self.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).ok_or_else(|| cmd::bad("tool", "no slide"))?;
            let Some(sh) = kition_slide_model::find_shape_mut(list, id) else { return Ok(()) };
            let ShapeKind::Table(t) = &mut sh.kind else { return Ok(()) };
            let before = index.checked_sub(1);
            if vertical {
                let (Some(&left), Some(&right)) = (before.and_then(|i| cols.get(i)), cols.get(index)) else { return Ok(()) };
                if left + right < 2.0 * MIN_TRACK || t.cols.len() != cols.len() {
                    return Ok(());
                }
                let d = delta.x.clamp(MIN_TRACK - left, right - MIN_TRACK);
                t.cols = cols.to_vec();
                if let Some(c) = before.and_then(|i| t.cols.get_mut(i)) {
                    *c = left + d;
                }
                if let Some(c) = t.cols.get_mut(index) {
                    *c = right - d;
                }
            } else {
                let Some(&above) = before.and_then(|i| rows.get(i)) else { return Ok(()) };
                if t.rows.len() != rows.len() {
                    return Ok(());
                }
                if let Some(row) = before.and_then(|i| t.rows.get_mut(i)) {
                    row.height = (above + delta.y).max(MIN_TRACK);
                }
            }
            let (w, h) = (t.width(), t.height());
            if let Some(x) = sh.xfrm.as_mut() {
                x.w = w;
                x.h = h;
            }
            Ok(())
        })
    }
}

/// The cell a Shift-click range grows from: the range's first corner, else the edited cell.
fn range_anchor(st: &crate::DocState, id: ShapeId) -> Option<(usize, usize)> {
    if let Some((table, anchor, _)) = st.selection.cells
        && table == id
    {
        return Some(anchor);
    }
    st.selection.text.as_ref().filter(|t| t.shape == id).and_then(|t| t.cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_table() -> (Session, ShapeId, Xfrm) {
        let mut s = Session::with_new();
        let r = s.execute("table.insert", &json!({"rows": 3, "cols": 3})).unwrap();
        let id = ShapeId(r["id"].as_u64().unwrap() as u32);
        let st = s.doc().unwrap();
        let x = xfrm_of(&st.doc, &st.selection, st.shape(id).unwrap());
        (s, id, x)
    }

    fn table(s: &Session, id: ShapeId) -> kition_slide_model::Table {
        match &s.doc().unwrap().shape(id).unwrap().kind {
            ShapeKind::Table(t) => t.clone(),
            _ => panic!("not a table"),
        }
    }

    fn ev(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
        PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 }
    }

    #[test]
    fn dragging_a_column_line_trades_width_in_one_undo_step() {
        let (mut s, id, x) = session_with_table();
        let before = table(&s, id);
        let steps = s.doc().unwrap().history.undo.len();
        let line = x.x + before.cols[0];
        let mid = x.y + 10.0;
        assert!(matches!(s.table_divider_hit(Point::new(line + 1.0, mid), 3.0), Some(Hit::TableDivider { vertical: true, index: 1, .. })));
        s.pointer(ev(PointerKind::Down, line, mid)).unwrap();
        s.pointer(ev(PointerKind::Drag, line + 10.0, mid)).unwrap();
        s.pointer(ev(PointerKind::Drag, line + 30.0, mid)).unwrap();
        s.pointer(ev(PointerKind::Up, line + 30.0, mid)).unwrap();
        let after = table(&s, id);
        assert!((after.cols[0] - before.cols[0] - 30.0).abs() < 1e-6);
        assert!((after.cols[1] - before.cols[1] + 30.0).abs() < 1e-6);
        assert!((after.width() - before.width()).abs() < 1e-6, "the table keeps its width");
        assert!(s.doc().unwrap().selection.text.is_none(), "a divider drag does not start text editing");
        assert_eq!(s.doc().unwrap().history.undo.len(), steps + 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(table(&s, id).cols, before.cols);
    }

    #[test]
    fn a_column_line_stops_before_a_column_vanishes() {
        let (mut s, id, x) = session_with_table();
        let before = table(&s, id);
        let line = x.x + before.cols[0];
        s.pointer(ev(PointerKind::Down, line, x.y + 10.0)).unwrap();
        s.pointer(ev(PointerKind::Drag, line + 5000.0, x.y + 10.0)).unwrap();
        s.pointer(ev(PointerKind::Up, line + 5000.0, x.y + 10.0)).unwrap();
        let after = table(&s, id);
        assert!((after.cols[1] - MIN_TRACK).abs() < 1e-6);
        assert!((after.width() - before.width()).abs() < 1e-6);
    }

    #[test]
    fn a_row_line_grows_the_row_above_and_the_frame() {
        let (mut s, id, x) = session_with_table();
        let before = table(&s, id);
        let line = x.y + table_rows(s.doc().unwrap(), &before)[0];
        let at = x.x + 20.0;
        s.pointer(ev(PointerKind::Down, at, line)).unwrap();
        s.pointer(ev(PointerKind::Drag, at, line + 25.0)).unwrap();
        s.pointer(ev(PointerKind::Up, at, line + 25.0)).unwrap();
        let after = table(&s, id);
        let st = s.doc().unwrap();
        let frame = xfrm_of(&st.doc, &st.selection, st.shape(id).unwrap());
        assert!(after.rows[0].height > before.rows[0].height + 20.0);
        assert!((frame.h - after.height()).abs() < 1e-6);
    }

    #[test]
    fn escape_during_a_divider_drag_restores_the_table() {
        let (mut s, id, x) = session_with_table();
        let before = table(&s, id);
        let steps = s.doc().unwrap().history.undo.len();
        let line = x.x + before.cols[0];
        s.pointer(ev(PointerKind::Down, line, x.y + 10.0)).unwrap();
        s.pointer(ev(PointerKind::Drag, line + 40.0, x.y + 10.0)).unwrap();
        assert_ne!(table(&s, id).cols, before.cols);
        s.key("Escape", Mods::default()).unwrap();
        s.pointer(ev(PointerKind::Up, line + 40.0, x.y + 10.0)).unwrap();
        assert_eq!(table(&s, id).cols, before.cols);
        assert_eq!(s.doc().unwrap().history.undo.len(), steps, "a cancelled gesture leaves no undo step");
    }

    #[test]
    fn merged_cells_hide_the_line_they_span() {
        let (mut s, id, x) = session_with_table();
        s.execute("table.merge", &json!({"id": id, "from": [0, 0], "to": [0, 1]})).unwrap();
        let t = table(&s, id);
        let line = x.x + t.cols[0];
        let st = s.doc().unwrap();
        let first_row = x.y + 5.0;
        let second_row = x.y + table_rows(st, &t)[0] + 5.0;
        assert_eq!(divider_at(st, id, Point::new(line, first_row), 3.0), None);
        assert_eq!(divider_at(st, id, Point::new(line, second_row), 3.0), Some((true, 1)));
    }

    #[test]
    fn shift_click_grows_from_the_range_anchor() {
        let (mut s, id, _) = session_with_table();
        s.execute("table.selectCells", &json!({"id": id, "from": [2, 2], "to": [0, 0]})).unwrap();
        assert_eq!(range_anchor(s.doc().unwrap(), id), Some((2, 2)));
    }
}
