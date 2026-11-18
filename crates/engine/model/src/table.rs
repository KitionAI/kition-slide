//! Tables (graphic frames with `a:tbl`).

use serde::{Deserialize, Serialize};

use crate::style::{Fill, Line};
use crate::text::{Anchor, TextBody};

mod border;
mod edit;

pub use border::BorderEdges;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cell {
    pub text: TextBody,
    pub fill: Option<Fill>,
    /// Left, right, top, bottom borders; `None` = from the table style.
    pub borders: [Option<Line>; 4],
    pub diag_down: Option<Line>,
    pub diag_up: Option<Line>,
    /// Margins (l, r, t, b) in points; `None` = defaults (7.2, 7.2, 3.6, 3.6).
    pub margins: Option<[f64; 4]>,
    pub anchor: Option<Anchor>,
    pub grid_span: u32,
    pub row_span: u32,
    /// Covered by a merge to the left / above.
    pub h_merge: bool,
    pub v_merge: bool,
    pub vertical: Option<crate::text::TextDir>,
}

impl Cell {
    pub fn new(text: &str) -> Self {
        Cell { text: TextBody::from_text(text), grid_span: 1, row_span: 1, ..Default::default() }
    }
    pub fn is_covered(&self) -> bool {
        self.h_merge || self.v_merge
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Row {
    /// Persistent identity within the owning table, independent of cells, height and position.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub identity: String,
    pub height: f64,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Table {
    /// Column widths in points.
    pub cols: Vec<f64>,
    /// Persistent column identities, in exactly the same order as `cols`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub col_ids: Vec<String>,
    pub rows: Vec<Row>,
    /// Table style id (our built-in style names or a PPTX style GUID).
    pub style: String,
    pub first_row: bool,
    pub first_col: bool,
    pub last_row: bool,
    pub last_col: bool,
    pub band_row: bool,
    pub band_col: bool,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            cols: vec![],
            col_ids: vec![],
            rows: vec![],
            style: "medium2-accent1".into(),
            first_row: true,
            first_col: false,
            last_row: false,
            last_col: false,
            band_row: true,
            band_col: false,
        }
    }
}

impl Table {
    pub fn new(rows: usize, cols: usize, width: f64, row_height: f64) -> Self {
        let rows = rows.clamp(1, 1000);
        let cols = cols.clamp(1, 1000);
        let cw = if cols > 0 { width / cols as f64 } else { width };
        Table {
            cols: vec![cw; cols],
            col_ids: (0..cols).map(|_| crate::identity::fresh()).collect(),
            rows: (0..rows)
                .map(|_| Row { identity: crate::identity::fresh(), height: row_height, cells: (0..cols).map(|_| Cell::new("")).collect() })
                .collect(),
            ..Default::default()
        }
    }
    /// Repairs absent or duplicated import metadata. A file with stripped ids has deterministic
    /// new identities; continuity through an editor that strips them cannot be established.
    pub fn settle_ids(&mut self) -> bool {
        let mut changed = self.col_ids.len() != self.cols.len();
        let seed = if changed
            || !crate::identity::distinct(self.col_ids.iter().map(String::as_str))
            || !crate::identity::distinct(self.rows.iter().map(|row| row.identity.as_str()))
        {
            crate::identity::import_seed(self)
        } else {
            String::new()
        };
        if changed {
            self.col_ids.clear();
        }
        self.col_ids.resize(self.cols.len(), String::new());
        changed |= crate::identity::members(self.col_ids.iter_mut(), &format!("{seed}/columns"));
        changed |= crate::identity::members(self.rows.iter_mut().map(|row| &mut row.identity), &format!("{seed}/rows"));
        changed
    }
    pub fn n_rows(&self) -> usize {
        self.rows.len()
    }
    pub fn n_cols(&self) -> usize {
        self.cols.len()
    }
    pub fn cell(&self, r: usize, c: usize) -> Option<&Cell> {
        self.rows.get(r).and_then(|row| row.cells.get(c))
    }
    pub fn cell_mut(&mut self, r: usize, c: usize) -> Option<&mut Cell> {
        self.rows.get_mut(r).and_then(|row| row.cells.get_mut(c))
    }
    pub fn width(&self) -> f64 {
        self.cols.iter().sum()
    }
    pub fn height(&self) -> f64 {
        self.rows.iter().map(|r| r.height).sum()
    }
    pub fn insert_row(&mut self, at: usize) {
        self.edit_grid(at, true, true);
    }
    pub fn insert_col(&mut self, at: usize) {
        self.edit_grid(at, false, true);
    }
    pub fn delete_row(&mut self, at: usize) -> bool {
        self.edit_grid(at, true, false)
    }
    pub fn delete_col(&mut self, at: usize) -> bool {
        self.edit_grid(at, false, false)
    }
    /// Merge the rectangle (r0,c0)–(r1,c1) inclusive into its top-left cell.
    pub fn merge(&mut self, r0: usize, c0: usize, r1: usize, c1: usize) -> bool {
        let (r0, r1) = (r0.min(r1), r0.max(r1));
        let (c0, c1) = (c0.min(c1), c0.max(c1));
        if r1 >= self.rows.len() || c1 >= self.cols.len() || (r0 == r1 && c0 == c1) {
            return false;
        }
        // A partial overlap would leave covered cells outside the new merge orphaned.
        for (r, c, re, ce) in self.merged_regions() {
            if r <= r1 && re > r0 && c <= c1 && ce > c0 && (r < r0 || c < c0 || re > r1 + 1 || ce > c1 + 1) {
                return false;
            }
        }
        let mut text = vec![];
        for r in r0..=r1 {
            for c in c0..=c1 {
                if let Some(cell) = self.cell_mut(r, c) {
                    if !cell.text.is_empty() {
                        text.append(&mut cell.text.paragraphs);
                    }
                    cell.h_merge = c > c0;
                    cell.v_merge = r > r0;
                    cell.grid_span = 1;
                    cell.row_span = 1;
                }
            }
        }
        if let Some(cell) = self.cell_mut(r0, c0) {
            cell.grid_span = (c1 - c0 + 1) as u32;
            cell.row_span = (r1 - r0 + 1) as u32;
            cell.h_merge = false;
            cell.v_merge = false;
            if !text.is_empty() {
                cell.text.paragraphs = text;
            }
        }
        true
    }
    /// Undo a merge at (r, c).
    pub fn split(&mut self, r: usize, c: usize) -> bool {
        let Some((r, c)) = self.cell_owner(r, c) else { return false };
        let Some(cell) = self.cell(r, c) else { return false };
        let (gs, rs) = (cell.grid_span.max(1) as usize, cell.row_span.max(1) as usize);
        if gs == 1 && rs == 1 {
            return false;
        }
        for rr in r..r.saturating_add(rs).min(self.n_rows()) {
            for cc in c..c.saturating_add(gs).min(self.n_cols()) {
                if let Some(x) = self.cell_mut(rr, cc) {
                    x.h_merge = false;
                    x.v_merge = false;
                    x.grid_span = 1;
                    x.row_span = 1;
                }
            }
        }
        true
    }
    /// Make every row have exactly `cols.len()` cells (repair after loading).
    pub fn normalize(&mut self) {
        let n = self.cols.len();
        for r in &mut self.rows {
            r.cells.resize_with(n, || Cell::new(""));
            for c in &mut r.cells {
                c.grid_span = c.grid_span.max(1);
                c.row_span = c.row_span.max(1);
            }
        }
    }
}

/// Built-in table styles: (id, label). Styles are generated from the theme accents.
pub fn table_styles() -> Vec<(String, String)> {
    let mut v = vec![("none".to_string(), "No Style, No Grid".to_string()), ("grid".to_string(), "No Style, Table Grid".to_string())];
    for (kind, label) in [
        ("light1", "Light Style 1"),
        ("light2", "Light Style 2"),
        ("medium2", "Medium Style 2"),
        ("medium4", "Medium Style 4"),
        ("dark1", "Dark Style 1"),
    ] {
        v.push((format!("{kind}-tx1"), label.to_string()));
        for a in 1..=6 {
            v.push((format!("{kind}-accent{a}"), format!("{label} - Accent {a}")));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_delete_merge_split() {
        let mut t = Table::new(3, 3, 300.0, 30.0);
        assert_eq!((t.n_rows(), t.n_cols()), (3, 3));
        t.insert_row(1);
        t.insert_col(3);
        assert_eq!((t.n_rows(), t.n_cols()), (4, 4));
        assert!(t.delete_row(0) && t.delete_col(0));
        assert_eq!((t.n_rows(), t.n_cols()), (3, 3));
        assert!(t.merge(0, 0, 1, 1));
        assert_eq!(t.cell(0, 0).map(|c| (c.grid_span, c.row_span)), Some((2, 2)));
        assert!(t.cell(1, 1).is_some_and(|c| c.is_covered()));
        assert!(t.split(0, 0));
        assert!(!t.cell(1, 1).is_some_and(|c| c.is_covered()));
        assert!(!t.merge(5, 5, 6, 6));
        assert!(!t.delete_row(99));
        let mut one = Table::new(1, 1, 10.0, 10.0);
        assert!(!one.delete_row(0) && !one.delete_col(0));
        assert!(table_styles().len() > 30);
    }
}
