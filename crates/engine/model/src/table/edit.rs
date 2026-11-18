//! Grid edits keep merged cells attached to their content and formatting.

use super::{Cell, Row, Table};

impl Table {
    /// The visible owner of a coordinate, including a covered part of a merged cell.
    pub fn cell_owner(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let cell = self.cell(row, col)?;
        if !cell.is_covered() {
            return Some((row, col));
        }
        self.merged_regions().into_iter().find(|&(r, c, re, ce)| r <= row && row < re && c <= col && col < ce).map(|(r, c, _, _)| (r, c))
    }

    /// Half-open rectangles of visible merged cells, bounded by the actual grid.
    pub(super) fn merged_regions(&self) -> Vec<(usize, usize, usize, usize)> {
        let mut out = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate().take(self.n_cols()) {
                if !cell.is_covered() && (cell.row_span > 1 || cell.grid_span > 1) {
                    out.push((
                        r,
                        c,
                        r.saturating_add(cell.row_span.max(1) as usize).min(self.n_rows()),
                        c.saturating_add(cell.grid_span.max(1) as usize).min(self.n_cols()),
                    ));
                }
            }
        }
        out
    }

    pub(super) fn edit_grid(&mut self, at: usize, rows: bool, insert: bool) -> bool {
        let size = if rows { self.n_rows() } else { self.n_cols() };
        if (insert && size >= 1000) || (!insert && (size <= 1 || at >= size)) {
            return false;
        }
        self.settle_ids();
        let at = at.min(size);
        let regions: Vec<_> = self
            .merged_regions()
            .into_iter()
            .filter_map(|(r, c, re, ce)| {
                let saved = self.cell(r, c)?.clone();
                let (start, end) = if rows { (r, re) } else { (c, ce) };
                let (start2, end2) = if insert {
                    (start + usize::from(at <= start), end + usize::from(at < end))
                } else {
                    (start.saturating_sub(usize::from(at < start)), end.saturating_sub(usize::from(at < end)))
                };
                if start2 >= end2 {
                    return None;
                }
                let bounds = if rows { (start2, c, end2, ce) } else { (r, start2, re, end2) };
                Some((bounds, (!insert && at == start).then_some(saved)))
            })
            .collect();
        if rows {
            if insert {
                let height = self.rows.get(at.saturating_sub(1)).map_or(30.0, |r| r.height);
                self.rows.insert(at, Row { identity: crate::identity::fresh(), height, cells: (0..self.n_cols()).map(|_| Cell::new("")).collect() });
            } else {
                self.rows.remove(at);
            }
        } else if insert {
            let width = self.cols.get(at.saturating_sub(1)).copied().unwrap_or(80.0);
            self.cols.insert(at, width);
            self.col_ids.insert(at, crate::identity::fresh());
            for row in &mut self.rows {
                row.cells.insert(at.min(row.cells.len()), Cell::new(""));
            }
        } else {
            self.cols.remove(at);
            self.col_ids.remove(at);
            for row in &mut self.rows {
                if at < row.cells.len() {
                    row.cells.remove(at);
                }
            }
        }
        for row in &mut self.rows {
            for cell in &mut row.cells {
                cell.grid_span = 1;
                cell.row_span = 1;
                cell.h_merge = false;
                cell.v_merge = false;
            }
        }
        for ((r, c, re, ce), saved) in regions {
            if let Some(cell) = self.cell_mut(r, c) {
                if let Some(saved) = saved {
                    *cell = saved;
                }
                cell.grid_span = (ce - c) as u32;
                cell.row_span = (re - r) as u32;
                cell.h_merge = false;
                cell.v_merge = false;
            }
            for rr in r..re {
                for cc in c..ce {
                    if (rr != r || cc != c)
                        && let Some(cell) = self.cell_mut(rr, cc)
                    {
                        cell.h_merge = cc != c;
                        cell.v_merge = rr != r;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_edits_preserve_merges_and_promote_deleted_owners() {
        let mut table = Table::new(4, 4, 400.0, 40.0);
        table.cell_mut(1, 1).unwrap().text = crate::TextBody::from_text("Keep this");
        assert!(table.merge(1, 1, 2, 2));
        table.insert_row(2);
        table.insert_col(2);
        assert_eq!(table.cell_owner(3, 3), Some((1, 1)));
        assert_eq!(table.cell(1, 1).unwrap().row_span, 3);
        assert!(table.delete_row(1));
        assert!(table.delete_col(1));
        assert_eq!(table.cell(1, 1).unwrap().text.text(), "Keep this");
        assert_eq!(table.cell_owner(2, 2), Some((1, 1)));
        assert!(table.split(2, 2));
        assert_eq!(table.cell_owner(2, 2), Some((2, 2)));
    }

    #[test]
    fn partial_overlap_is_rejected_without_changing_content() {
        let mut table = Table::new(3, 3, 300.0, 30.0);
        assert!(table.merge(0, 0, 1, 1));
        let before = table.clone();
        assert!(!table.merge(1, 1, 2, 2));
        assert_eq!(table, before);
        assert!(table.merge(0, 0, 2, 2));
    }
}
