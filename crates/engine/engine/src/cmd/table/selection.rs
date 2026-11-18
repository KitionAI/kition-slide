//! Table selection descriptions and geometry shared by commands, tools and overlays.

use super::*;
use kition_slide_geom::Rect;
use kition_slide_model::table::BorderEdges;
use kition_slide_model::{Fill, Line, ShapeId, text::Anchor};

pub(super) fn has_table(s: &Session) -> std::result::Result<(), String> {
    if s.active().is_some_and(|st| {
        st.selection.shapes.len() == 1
            && st.selection.shapes.first().and_then(|id| st.shape(*id)).is_some_and(|sh| matches!(sh.kind, ShapeKind::Table(_)))
    }) {
        Ok(())
    } else {
        // Named targets remain executable through the session's explicit-id path.
        Err(NOTHING_SELECTED.into())
    }
}

pub(super) fn table_id(s: &Session, p: &Value) -> Result<ShapeId> {
    id_param(p, "id").or_else(|| s.active().and_then(|st| st.selection.shapes.first().copied())).ok_or_else(|| bad("table", "select a table"))
}

pub fn selected_range(st: &crate::DocState, id: ShapeId) -> Option<((usize, usize), (usize, usize))> {
    if let Some((table, a, b)) = st.selection.cells
        && table == id
    {
        return Some(((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1))));
    }
    st.selection.text.as_ref().filter(|t| t.shape == id).and_then(|t| t.cell).map(|c| (c, c))
}

pub(super) fn target_cells(s: &Session, p: &Value, id: ShapeId, table: &Table) -> Result<Vec<(usize, usize)>> {
    let st = s.doc()?;
    let mut cells = std::collections::BTreeSet::new();
    let mut add = |r, c| -> Result<()> {
        let owner = table.cell_owner(r, c).ok_or_else(|| bad("table", "cell is outside the table"))?;
        cells.insert(owner);
        Ok(())
    };
    if let Some(value) = p.get("cells") {
        let list = value.as_array().ok_or_else(|| bad("table", "cells must be an array"))?;
        for cell in list {
            let r = cell.get(0).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or_else(|| bad("table", "invalid row"))?;
            let c = cell.get(1).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or_else(|| bad("table", "invalid column"))?;
            add(r, c)?;
        }
    } else {
        let (a, b) = selected_range(st, id).unwrap_or(((0, 0), (table.n_rows().saturating_sub(1), table.n_cols().saturating_sub(1))));
        if table.cell(a.0, a.1).is_none() || table.cell(b.0, b.1).is_none() {
            return Err(bad("table", "cell selection is outside the table"));
        }
        for r in a.0..=b.0 {
            for c in a.1..=b.1 {
                add(r, c)?;
            }
        }
    }
    Ok(cells.into_iter().collect())
}

pub fn cell_rect(st: &crate::DocState, id: ShapeId, cell: (usize, usize)) -> Option<Rect> {
    let ShapeKind::Table(table) = &st.shape(id)?.kind else { return None };
    let owner = table.cell_owner(cell.0, cell.1)?;
    kition_slide_render::table_cell_rects(table, &crate::tools::table_rows(st, table))
        .into_iter()
        .find(|&(r, c, _)| (r, c) == owner)
        .map(|(_, _, rect)| rect)
}

pub fn cell_layout(st: &crate::DocState, id: ShapeId, cell: (usize, usize)) -> Option<kition_slide_text::TextLayout> {
    let table_shape = st.shape(id)?;
    let ShapeKind::Table(table) = &table_shape.kind else { return None };
    let (r, c) = table.cell_owner(cell.0, cell.1)?;
    let ctx = super::super::text::ctx_for(&st.doc, &st.selection)?;
    let body = kition_slide_render::table_cell_body(&ctx, table, r, c, table.cell(r, c)?);
    let shape = kition_slide_model::Shape { text: Some(body.clone()), ..Default::default() };
    let transform = super::super::text::text_shape_transform(&ctx, st.shapes(), id, kition_slide_geom::Affine::IDENTITY, 0)
        .unwrap_or_else(|| kition_slide_model::resolve::xfrm(&ctx, table_shape).affine());
    Some(kition_slide_text::layout_with_transform(
        &ctx,
        &shape,
        &body,
        &kition_slide_text::Opts { rect: cell_rect(st, id, (r, c))?, fields: &kition_slide_text::NoFields, prompt_color: None, no_shrink: false },
        transform,
    ))
}

pub(super) fn state(s: &mut Session, p: &Value) -> Result<Value> {
    let Ok(id) = table_id(s, p) else { return Ok(Value::Null) };
    let st = s.doc()?;
    let Some(ShapeKind::Table(table)) = st.shape(id).map(|sh| &sh.kind) else { return Ok(Value::Null) };
    let range = selected_range(st, id).unwrap_or(((0, 0), (0, 0)));
    let owner = table.cell_owner(range.0.0, range.0.1).unwrap_or(range.0);
    let cell = table.cell(owner.0, owner.1);
    Ok(json!({"id": id, "rows": table.n_rows(), "cols": table.n_cols(), "from": range.0, "to": range.1,
        "style": table.style, "styles": kition_slide_model::table::table_styles(),
        "rowHeight": table.rows.get(owner.0).map(|r| r.height), "columnWidth": table.cols.get(owner.1),
        "canMerge": range.0 != range.1, "canSplit": cell.is_some_and(|c| c.row_span > 1 || c.grid_span > 1),
        // The edited (or first selected) cell's own lines, left, right, top, bottom: width in
        // points, 0 for a removed line, null where the table style draws it.
        "borders": cell.map(|c| c.borders.iter().map(|b| b.as_ref().map(|l| if matches!(l.fill, Some(Fill::None)) { 0.0 } else { l.width.unwrap_or(1.0) })).collect::<Vec<_>>()),
        "anchor": match cell.and_then(|c| c.anchor).unwrap_or(Anchor::Top) { Anchor::Middle => "center", Anchor::Bottom => "bottom", _ => "top" },
        "options": {"headerRow": table.first_row, "totalRow": table.last_row, "bandedRows": table.band_row,
            "firstColumn": table.first_col, "lastColumn": table.last_col, "bandedColumns": table.band_col}}))
}

pub(super) fn cell_format(s: &mut Session, p: &Value) -> Result<Value> {
    let id = table_id(s, p)?;
    let sh = shape_of(s, id)?;
    let ShapeKind::Table(table) = &sh.kind else { return Err(bad("table.cellFormat", "not a table")) };
    let cells = target_cells(s, p, id, table)?;
    let anchor = match str_param(p, "anchor") {
        Some("top") => Some(Anchor::Top),
        Some("center") => Some(Anchor::Middle),
        Some("bottom") => Some(Anchor::Bottom),
        Some(_) => return Err(bad("table.cellFormat", "invalid vertical alignment")),
        None => None,
    };
    let margins = p
        .get("margins")
        .map(|value| -> Result<[f64; 4]> {
            let values = value.as_array().filter(|a| a.len() == 4).ok_or_else(|| bad("table.cellFormat", "four margins required"))?;
            let mut result = [0.0; 4];
            for (out, value) in result.iter_mut().zip(values) {
                *out = value
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=1000.0).contains(n))
                    .ok_or_else(|| bad("table.cellFormat", "invalid margin"))?;
            }
            Ok(result)
        })
        .transpose()?;
    let border = match str_param(p, "border") {
        Some(name) => Some(border_change(p, name, "table.cellFormat")?),
        None => None,
    };
    let range = target_range(s, p, id, table)?;
    with_table(s, p, "table.cellFormat", move |table| {
        for &(r, c) in &cells {
            if let Some(cell) = table.cell_mut(r, c) {
                if let Some(anchor) = anchor {
                    cell.anchor = Some(anchor);
                }
                if let Some(margins) = margins {
                    cell.margins = Some(margins);
                }
            }
        }
        if let Some((edges, line)) = &border {
            table.set_borders(range.0, range.1, *edges, line);
        }
        Ok(())
    })
}

/// `{edges, color?, width?, none?}`: the border line a Borders menu item sets.
fn border_change(p: &Value, edges: &str, cmd: &str) -> Result<(BorderEdges, Line)> {
    // `none` alone (the old `border: none`) clears every line of the range.
    let (edges, remove) = if edges == "none" { ("all", true) } else { (edges, bool_or(p, "none", false)) };
    let edges = BorderEdges::parse(edges).ok_or_else(|| bad(cmd, format!("unknown border edges `{edges}`")))?;
    if remove {
        return Ok((edges, Line { fill: Some(Fill::None), ..Default::default() }));
    }
    let color = match p.get("color") {
        Some(_) => color_param(p, "color").ok_or_else(|| bad(cmd, "invalid color"))?,
        None => kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Tx1),
    };
    let width = match p.get("width") {
        Some(w) => w.as_f64().filter(|w| w.is_finite() && (0.25..=24.0).contains(w)).ok_or_else(|| bad(cmd, "width must be 0.25–24 pt"))?,
        None => 1.0,
    };
    Ok((edges, Line { fill: Some(Fill::solid(color)), width: Some(width), ..Default::default() }))
}

pub(super) fn border(s: &mut Session, p: &Value) -> Result<Value> {
    let id = table_id(s, p)?;
    let sh = shape_of(s, id)?;
    let ShapeKind::Table(table) = &sh.kind else { return Err(bad("table.border", "not a table")) };
    let edges = str_param(p, "edges").ok_or_else(|| bad("table.border", "missing `edges`"))?;
    let (edges, line) = border_change(p, edges, "table.border")?;
    let range = target_range(s, p, id, table)?;
    with_table(s, p, "table.border", move |table| {
        if table.set_borders(range.0, range.1, edges, &line) { Ok(()) } else { Err(bad("table.border", "cell selection is outside the table")) }
    })
}

/// Empties the text of the selected cells (Delete with cells selected), keeping their formatting.
pub(super) fn clear_cells(s: &mut Session, p: &Value) -> Result<Value> {
    let id = table_id(s, p)?;
    let sh = shape_of(s, id)?;
    let ShapeKind::Table(table) = &sh.kind else { return Err(bad("table.clearCells", "not a table")) };
    let cells = target_cells(s, p, id, table)?;
    with_table(s, p, "table.clearCells", move |table| {
        for &(r, c) in &cells {
            if let Some(cell) = table.cell_mut(r, c) {
                let end = kition_slide_model::edit::end(&cell.text);
                kition_slide_model::edit::delete(&mut cell.text, (0, 0), end);
            }
        }
        Ok(())
    })
}

/// The cell range to act on: `from`/`to` params, the selected range, the edited cell, or the
/// whole table.
fn target_range(s: &Session, p: &Value, id: ShapeId, table: &Table) -> Result<((usize, usize), (usize, usize))> {
    let whole = ((0, 0), (table.n_rows().saturating_sub(1), table.n_cols().saturating_sub(1)));
    let range = match (super::cell_param(p, "from"), super::cell_param(p, "to")) {
        (Some(a), b) => (a, b.unwrap_or(a)),
        (None, _) => selected_range(s.doc()?, id).unwrap_or(whole),
    };
    if table.cell(range.0.0, range.0.1).is_none() || table.cell(range.1.0, range.1.1).is_none() {
        return Err(bad("table", "cell selection is outside the table"));
    }
    Ok(range)
}

#[cfg(test)]
mod vertical_text_tests {
    use super::*;
    use kition_slide_model::{Shape, Xfrm, text::TextDir};
    use std::sync::Arc;

    #[test]
    fn vertical_cell_caret_matches_rotated_table_and_group_fallback() {
        for (rotation, grouped) in [(0.0, false), (90.0, false), (45.0, true)] {
            let mut session = Session::with_new();
            let state = session.doc_mut().unwrap();
            let mut doc = (*state.doc).clone();
            let id = ShapeId(77);
            let mut table = Table::new(1, 1, 100.0, 200.0);
            let cell = table.cell_mut(0, 0).unwrap();
            cell.text = kition_slide_model::text::TextBody::from_text(&char::from_u32(0x4e00).unwrap().to_string());
            cell.vertical = Some(TextDir::EaVertical);
            let mut x = Xfrm::new(100.0, 100.0, 100.0, 200.0);
            if !grouped {
                x.rot = rotation;
            }
            let table = Shape { id, xfrm: Some(x), kind: ShapeKind::Table(table), ..Default::default() };
            let shape = if grouped {
                let mut group = Xfrm::new(0.0, 0.0, 300.0, 300.0);
                group.rot = rotation;
                group.flip_h = true;
                Shape {
                    id: ShapeId(78),
                    xfrm: Some(group),
                    kind: ShapeKind::Group { children: vec![table], child: Xfrm::new(0.0, 0.0, 300.0, 300.0) },
                    ..Default::default()
                }
            } else {
                table
            };
            Arc::make_mut(&mut doc.slides[0]).shapes = vec![shape];
            state.doc = Arc::new(doc);
            let edited = cell_layout(state, id, (0, 0)).unwrap();
            let placed = kition_slide_render::place_slide(&state.doc, 0);
            let drawn = &placed.texts[0].layout;
            assert_eq!(edited.runs.iter().any(|r| r.upright), rotation == 0.0);
            assert_eq!(edited.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>(), drawn.runs.iter().map(|r| &r.glyphs).collect::<Vec<_>>());
            assert_eq!(edited.caret(kition_slide_text::Pos { para: 0, ch: 1 }), drawn.caret(kition_slide_text::Pos { para: 0, ch: 1 }));
        }
    }
}
