//! Table Design and Table Layout tabs.

use kition_slide_model::{ShapeKind, Table};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

mod selection;
use selection::{border, clear_cells, has_table, state};
pub use selection::{cell_layout, cell_rect, selected_range};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "table.state", "Inspect Table Selection", [], None, "{id?}", has_doc, state),
        cmd!(
            "table.cellFormat",
            "Cell Formatting",
            ["Table Layout", "Alignment"],
            None,
            "{anchor?: top|center|bottom, margins?: [left,right,top,bottom], border?: none | edges of table.border, color?, width?: pt, id?}",
            has_table,
            selection::cell_format
        ),
        cmd!(
            "table.border",
            "Borders",
            ["Table Design", "Draw Borders"],
            None,
            "{edges: all|outside|inside|top|bottom|left|right|insideH|insideV|diagDown|diagUp, color?, width?: pt (0.25–24), none?: bool, from?: [r,c], to?: [r,c], id?} (default range: selected cells, else the whole table)",
            has_table,
            border
        ),
        cmd!("table.clearCells", "Clear Cells", [], None, "{cells?: [[r,c],…], id?} (default: selected cells)", has_table, clear_cells),
        cmd!("table.insertRowAbove", "Insert Above", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_table, |s, p| rows_cols(
            s, p, "rowAbove"
        )),
        cmd!("table.insertRowBelow", "Insert Below", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_table, |s, p| rows_cols(
            s, p, "rowBelow"
        )),
        cmd!("table.insertColumnLeft", "Insert Left", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_table, |s, p| rows_cols(
            s, p, "colLeft"
        )),
        cmd!("table.insertColumnRight", "Insert Right", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_table, |s, p| rows_cols(
            s, p, "colRight"
        )),
        cmd!("table.deleteRow", "Delete Rows", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_table, |s, p| rows_cols(s, p, "delRow")),
        cmd!("table.deleteColumn", "Delete Columns", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_table, |s, p| rows_cols(
            s, p, "delCol"
        )),
        cmd!("table.merge", "Merge Cells", ["Table Layout", "Merge"], None, "{from: [r, c], to: [r, c], id?}", has_table, merge),
        cmd!("table.split", "Split Cells", ["Table Layout", "Merge"], None, "{cell: [r, c], id?}", has_table, split),
        cmd!(
            "table.style",
            "Table Styles",
            ["Table Design", "Table Styles"],
            None,
            "{style: medium2-accent1|light1-accent2|dark1-tx1|grid|none…, id?}",
            has_table,
            style
        ),
        cmd!(
            "table.options",
            "Table Style Options",
            ["Table Design", "Table Style Options"],
            None,
            "{headerRow?, totalRow?, bandedRows?, firstColumn?, lastColumn?, bandedColumns?: bool, id?}",
            has_table,
            options
        ),
        cmd!(
            "table.cellFill",
            "Shading",
            ["Table Design", "Table Styles"],
            None,
            "{color? | none?, cells?: [[r,c],…] (default: selected cells or all), id?}",
            has_table,
            cell_fill
        ),
        cmd!("table.distributeRows", "Distribute Rows", ["Table Layout", "Cell Size"], None, "{id?}", has_table, |s, p| distribute(s, p, true)),
        cmd!("table.distributeColumns", "Distribute Columns", ["Table Layout", "Cell Size"], None, "{id?}", has_table, |s, p| distribute(
            s, p, false
        )),
        cmd!("table.columnWidth", "Width", ["Table Layout", "Cell Size"], None, "{col, width: pt, id?}", has_table, column_width),
        cmd!("table.rowHeight", "Height", ["Table Layout", "Cell Size"], None, "{row, height: pt, id?}", has_table, row_height),
        cmd!(noundo "table.selectCells", "Select Cells", [], None, "{from: [r,c], to: [r,c], id?}", has_table, select_cells),
    ]
}

fn cell_param(p: &Value, key: &str) -> Option<(usize, usize)> {
    let a = p.get(key)?.as_array()?;
    Some((usize::try_from(a.first()?.as_u64()?).ok()?, usize::try_from(a.get(1)?.as_u64()?).ok()?))
}

/// The table's row/col to act on: params, the edited cell, or the selected cells.
fn at(s: &Session, p: &Value) -> (usize, usize) {
    let st = s.active();
    let id = id_param(p, "id");
    let cell = st
        .and_then(|d| d.selection.text.as_ref().filter(|t| id.is_none_or(|id| t.shape == id)).and_then(|t| t.cell))
        .or_else(|| st.and_then(|d| d.selection.cells.filter(|c| id.is_none_or(|id| c.0 == id)).map(|c| c.1)));
    (usize_param(p, "row").or(cell.map(|c| c.0)).unwrap_or(0), usize_param(p, "col").or(cell.map(|c| c.1)).unwrap_or(0))
}

fn with_table(s: &mut Session, p: &Value, cmd: &str, f: impl Fn(&mut Table) -> Result<()>) -> Result<Value> {
    let id = match id_param(p, "id") {
        Some(i) => i,
        None => {
            let st = s.doc()?;
            st.selection
                .text
                .as_ref()
                .filter(|t| t.cell.is_some())
                .map(|t| t.shape)
                .or(st.selection.cells.map(|c| c.0))
                .or_else(|| st.selection.shapes.first().copied())
                .ok_or_else(|| bad(cmd, "select a table"))?
        }
    };
    let r = edit_shapes(s, &json!({"id": id}), cmd, |sh| match &mut sh.kind {
        ShapeKind::Table(t) => {
            f(t)?;
            t.normalize();
            let (w, h) = (t.width(), t.height());
            if let Some(x) = sh.xfrm.as_mut() {
                x.w = w;
                x.h = h;
            }
            Ok(())
        }
        _ => Err(bad(cmd, "not a table")),
    })?;
    s.select(|doc, sel| {
        let table = super::current_shapes(doc, sel)
            .and_then(|v| kition_slide_model::find_shape(v, id))
            .and_then(|x| if let ShapeKind::Table(t) = &x.kind { Some(t) } else { None });
        let Some(table) = table else { return };
        let clamp = |(r, c): (usize, usize)| (r.min(table.n_rows().saturating_sub(1)), c.min(table.n_cols().saturating_sub(1)));
        if let Some(text) = sel.text.as_mut().filter(|text| text.shape == id)
            && let Some(cell) = text.cell
        {
            let cell = clamp(cell);
            let owner = table.cell_owner(cell.0, cell.1).unwrap_or(cell);
            if text.cell != Some(owner) {
                text.anchor = (0, 0);
                text.caret = (0, 0);
            }
            text.cell = Some(owner);
        }
        if let Some((table_id, a, b)) = sel.cells
            && table_id == id
        {
            sel.cells = Some((id, clamp(a), clamp(b)));
        }
    })?;
    Ok(r)
}

fn rows_cols(s: &mut Session, p: &Value, op: &str) -> Result<Value> {
    let (r, c) = at(s, p);
    let op = op.to_string();
    with_table(s, p, "table", move |t| {
        if r >= t.n_rows() || c >= t.n_cols() {
            return Err(bad("table", "cell is outside the table"));
        }
        if ((op == "rowAbove" || op == "rowBelow") && t.n_rows() >= 1000) || ((op == "colLeft" || op == "colRight") && t.n_cols() >= 1000) {
            return Err(bad("table", "table dimension limit reached"));
        }
        match op.as_str() {
            "rowAbove" => t.insert_row(r),
            "rowBelow" => t.insert_row(r + 1),
            "colLeft" => t.insert_col(c),
            "colRight" => t.insert_col(c + 1),
            "delRow" => {
                if !t.delete_row(r) {
                    return Err(bad("table.deleteRow", "can't delete the last row"));
                }
            }
            _ => {
                if !t.delete_col(c) {
                    return Err(bad("table.deleteColumn", "can't delete the last column"));
                }
            }
        }
        Ok(())
    })
}

fn merge(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param(p, "id");
    let sel = s.doc()?.selection.cells.filter(|(table, _, _)| id.is_none_or(|id| *table == id));
    let from = cell_param(p, "from").or(sel.map(|c| c.1)).ok_or_else(|| bad("table.merge", "missing `from`"))?;
    let to = cell_param(p, "to").or(sel.map(|c| c.2)).ok_or_else(|| bad("table.merge", "missing `to`"))?;
    with_table(
        s,
        p,
        "table.merge",
        move |t| if t.merge(from.0, from.1, to.0, to.1) { Ok(()) } else { Err(bad("table.merge", "can't merge those cells")) },
    )
}

fn split(s: &mut Session, p: &Value) -> Result<Value> {
    let cell = cell_param(p, "cell").unwrap_or_else(|| at(s, p));
    with_table(s, p, "table.split", move |t| if t.split(cell.0, cell.1) { Ok(()) } else { Err(bad("table.split", "that cell isn't merged")) })
}

fn style(s: &mut Session, p: &Value) -> Result<Value> {
    let st = str_param(p, "style").ok_or_else(|| bad("table.style", "missing `style`"))?.to_string();
    with_table(s, p, "table.style", move |t| {
        t.style = st.clone();
        Ok(())
    })
}

fn options(s: &mut Session, p: &Value) -> Result<Value> {
    with_table(s, p, "table.options", |t| {
        if let Some(v) = bool_param(p, "headerRow") {
            t.first_row = v;
        }
        if let Some(v) = bool_param(p, "totalRow") {
            t.last_row = v;
        }
        if let Some(v) = bool_param(p, "bandedRows") {
            t.band_row = v;
        }
        if let Some(v) = bool_param(p, "firstColumn") {
            t.first_col = v;
        }
        if let Some(v) = bool_param(p, "lastColumn") {
            t.last_col = v;
        }
        if let Some(v) = bool_param(p, "bandedColumns") {
            t.band_col = v;
        }
        Ok(())
    })
}

fn cell_fill(s: &mut Session, p: &Value) -> Result<Value> {
    let fill =
        if bool_or(p, "none", false) { Some(kition_slide_model::Fill::None) } else { color_param(p, "color").map(kition_slide_model::Fill::solid) };
    if fill.is_none() && p.get("color").is_some() {
        return Err(bad("table.cellFill", "invalid color"));
    }
    let id = selection::table_id(s, p)?;
    let sh = shape_of(s, id)?;
    let ShapeKind::Table(table) = &sh.kind else { return Err(bad("table.cellFill", "not a table")) };
    let cells = selection::target_cells(s, p, id, table)?;
    with_table(s, p, "table.cellFill", move |t| {
        for &(r, c) in &cells {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.fill.clone_from(&fill);
            }
        }
        Ok(())
    })
}

fn distribute(s: &mut Session, p: &Value, rows: bool) -> Result<Value> {
    with_table(s, p, "table.distribute", move |t| {
        if rows {
            let h = t.height() / t.n_rows().max(1) as f64;
            t.rows.iter_mut().for_each(|r| r.height = h);
        } else {
            let w = t.width() / t.n_cols().max(1) as f64;
            t.cols.iter_mut().for_each(|c| *c = w);
        }
        Ok(())
    })
}

fn column_width(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, c) = at(s, p);
    let w = f64_param(p, "width").ok_or_else(|| bad("table.columnWidth", "missing `width`"))?.clamp(4.0, 4000.0);
    with_table(s, p, "table.columnWidth", move |t| {
        let x = t.cols.get_mut(c).ok_or_else(|| bad("table.columnWidth", "no such column"))?;
        *x = w;
        Ok(())
    })
}

fn row_height(s: &mut Session, p: &Value) -> Result<Value> {
    let (r, _) = at(s, p);
    let h = f64_param(p, "height").ok_or_else(|| bad("table.rowHeight", "missing `height`"))?.clamp(4.0, 4000.0);
    with_table(s, p, "table.rowHeight", move |t| {
        let x = t.rows.get_mut(r).ok_or_else(|| bad("table.rowHeight", "no such row"))?;
        x.height = h;
        Ok(())
    })
}

fn select_cells(s: &mut Session, p: &Value) -> Result<Value> {
    let id = selection::table_id(s, p)?;
    let sh = shape_of(s, id)?;
    let ShapeKind::Table(table) = &sh.kind else { return Err(bad("table.selectCells", "not a table")) };
    let from = cell_param(p, "from").unwrap_or((0, 0));
    let to = cell_param(p, "to").unwrap_or(from);
    if table.cell(from.0, from.1).is_none() || table.cell(to.0, to.1).is_none() {
        return Err(bad("table.selectCells", "cell is outside the table"));
    }
    s.select(|_, sel| {
        sel.cells = Some((id, from, to));
        sel.shapes = vec![id];
        sel.text = None;
    })?;
    ok()
}
