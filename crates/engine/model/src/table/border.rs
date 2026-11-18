//! Cell borders set on a range's edges, the way a table's Borders menu applies them.

use super::Table;
use crate::style::Line;

/// Which lines of a cell range a border applies to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderEdges {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    /// Lines between the range's rows / columns.
    pub inside_h: bool,
    pub inside_v: bool,
    pub diag_down: bool,
    pub diag_up: bool,
}

impl BorderEdges {
    /// `all`, `outside`, `inside`, `top`, `bottom`, `left`, `right`, `insideH`, `insideV`,
    /// `diagDown`, `diagUp`.
    pub fn parse(name: &str) -> Option<Self> {
        let none = Self::default();
        Some(match name {
            "all" => Self { top: true, bottom: true, left: true, right: true, inside_h: true, inside_v: true, ..none },
            "outside" => Self { top: true, bottom: true, left: true, right: true, ..none },
            "inside" => Self { inside_h: true, inside_v: true, ..none },
            "top" => Self { top: true, ..none },
            "bottom" => Self { bottom: true, ..none },
            "left" => Self { left: true, ..none },
            "right" => Self { right: true, ..none },
            "insideH" => Self { inside_h: true, ..none },
            "insideV" => Self { inside_v: true, ..none },
            "diagDown" => Self { diag_down: true, ..none },
            "diagUp" => Self { diag_up: true, ..none },
            _ => return None,
        })
    }
}

/// Border slots of [`super::Cell::borders`].
const LEFT: usize = 0;
const RIGHT: usize = 1;
const TOP: usize = 2;
const BOTTOM: usize = 3;

impl Table {
    /// The range (r0,c0)–(r1,c1) inclusive grown until no merged cell crosses its edge.
    pub fn expand_range(&self, from: (usize, usize), to: (usize, usize)) -> Option<((usize, usize), (usize, usize))> {
        let (mut r0, mut r1) = (from.0.min(to.0), from.0.max(to.0));
        let (mut c0, mut c1) = (from.1.min(to.1), from.1.max(to.1));
        if r1 >= self.n_rows() || c1 >= self.n_cols() {
            return None;
        }
        let regions = self.merged_regions();
        loop {
            let mut grew = false;
            for &(r, c, re, ce) in &regions {
                if r <= r1 && re > r0 && c <= c1 && ce > c0 {
                    let (nr0, nr1, nc0, nc1) = (r0.min(r), r1.max(re - 1), c0.min(c), c1.max(ce - 1));
                    grew |= (nr0, nr1, nc0, nc1) != (r0, r1, c0, c1);
                    (r0, r1, c0, c1) = (nr0, nr1, nc0, nc1);
                }
            }
            if !grew {
                return Some(((r0, c0), (r1, c1)));
            }
        }
    }

    /// Sets `line` (a [`crate::Fill::None`] line removes the border) on `edges` of the range.
    /// Lines shared with a neighbouring cell are written on both cells, as PowerPoint writes
    /// them, so neither the neighbour's own border nor the table style draws over the change.
    pub fn set_borders(&mut self, from: (usize, usize), to: (usize, usize), edges: BorderEdges, line: &Line) -> bool {
        let Some(((r0, c0), (r1, c1))) = self.expand_range(from, to) else { return false };
        let mut writes: Vec<(usize, usize, usize)> = Vec::new();
        // Every grid line segment inside or around the range, by the cells on either side.
        for r in r0..=r1 {
            for c in c0..=c1 {
                let horizontal = |top: bool| {
                    if top { r == r0 && edges.top || r > r0 && edges.inside_h } else { r == r1 && edges.bottom || r < r1 && edges.inside_h }
                };
                let vertical = |left: bool| {
                    if left { c == c0 && edges.left || c > c0 && edges.inside_v } else { c == c1 && edges.right || c < c1 && edges.inside_v }
                };
                if horizontal(true) {
                    writes.push((r, c, TOP));
                    if let Some(above) = r.checked_sub(1) {
                        writes.push((above, c, BOTTOM));
                    }
                }
                if horizontal(false) {
                    writes.push((r, c, BOTTOM));
                    writes.push((r + 1, c, TOP));
                }
                if vertical(true) {
                    writes.push((r, c, LEFT));
                    if let Some(before) = c.checked_sub(1) {
                        writes.push((r, before, RIGHT));
                    }
                }
                if vertical(false) {
                    writes.push((r, c, RIGHT));
                    writes.push((r, c + 1, LEFT));
                }
            }
        }
        for (r, c, slot) in writes {
            // A segment inside a merged cell belongs to no visible line; its owner's edge does.
            let Some((orow, ocol)) = self.cell_owner(r, c) else { continue };
            let Some(cell) = self.cell(orow, ocol) else { continue };
            let (re, ce) = (orow + cell.row_span.max(1) as usize, ocol + cell.grid_span.max(1) as usize);
            let on_edge = match slot {
                TOP => r == orow,
                BOTTOM => r + 1 == re,
                LEFT => c == ocol,
                _ => c + 1 == ce,
            };
            if on_edge
                && let Some(cell) = self.cell_mut(orow, ocol)
                && let Some(b) = cell.borders.get_mut(slot)
            {
                *b = Some(line.clone());
            }
        }
        if edges.diag_down || edges.diag_up {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    if self.cell_owner(r, c) != Some((r, c)) {
                        continue;
                    }
                    if let Some(cell) = self.cell_mut(r, c) {
                        if edges.diag_down {
                            cell.diag_down = Some(line.clone());
                        }
                        if edges.diag_up {
                            cell.diag_up = Some(line.clone());
                        }
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
    use crate::style::Fill;

    fn red() -> Line {
        Line { fill: Some(Fill::solid(crate::ColorRef::rgb(kition_slide_color::Rgba::rgb(0xFF, 0, 0)))), width: Some(2.0), ..Default::default() }
    }

    fn slots(t: &Table, r: usize, c: usize) -> [bool; 4] {
        let cell = t.cell(r, c).unwrap();
        std::array::from_fn(|i| cell.borders[i].is_some())
    }

    #[test]
    fn outside_borders_write_both_sides_of_each_line() {
        let mut t = Table::new(4, 4, 400.0, 40.0);
        assert!(t.set_borders((1, 1), (2, 2), BorderEdges::parse("outside").unwrap(), &red()));
        // Corners of the range: [left, right, top, bottom].
        assert_eq!(slots(&t, 1, 1), [true, false, true, false]);
        assert_eq!(slots(&t, 2, 2), [false, true, false, true]);
        // Neighbours share the line.
        assert_eq!(slots(&t, 0, 1), [false, false, false, true]);
        assert_eq!(slots(&t, 1, 0), [false, true, false, false]);
        assert_eq!(slots(&t, 3, 2), [false, false, true, false]);
        assert_eq!(slots(&t, 2, 3), [true, false, false, false]);
        // Nothing far away.
        assert_eq!(slots(&t, 0, 0), [false; 4]);
    }

    #[test]
    fn inside_borders_leave_the_outline_alone() {
        let mut t = Table::new(3, 3, 300.0, 30.0);
        t.set_borders((0, 0), (2, 2), BorderEdges::parse("inside").unwrap(), &red());
        assert_eq!(slots(&t, 0, 0), [false, true, false, true]);
        assert_eq!(slots(&t, 1, 1), [true; 4]);
        assert_eq!(slots(&t, 2, 2), [true, false, true, false]);
    }

    #[test]
    fn a_range_through_a_merge_grows_to_hold_it() {
        let mut t = Table::new(3, 3, 300.0, 30.0);
        assert!(t.merge(0, 1, 1, 2));
        assert_eq!(t.expand_range((1, 0), (1, 1)), Some(((0, 0), (1, 2))));
        t.set_borders((1, 0), (1, 1), BorderEdges::parse("top").unwrap(), &red());
        // The merged owner gets its top line; covered cells keep nothing.
        assert!(t.cell(0, 1).unwrap().borders[TOP].is_some());
        assert!(t.cell(0, 2).unwrap().borders[TOP].is_none());
        assert!(t.cell(0, 0).unwrap().borders[TOP].is_some());
    }

    #[test]
    fn a_merged_cell_takes_its_bottom_line_from_below() {
        let mut t = Table::new(3, 2, 200.0, 30.0);
        assert!(t.merge(0, 0, 1, 0));
        t.set_borders((2, 0), (2, 0), BorderEdges::parse("top").unwrap(), &red());
        assert!(t.cell(2, 0).unwrap().borders[TOP].is_some());
        assert!(t.cell(0, 0).unwrap().borders[BOTTOM].is_some(), "the merge spans to the line");
        assert!(t.cell(1, 0).unwrap().borders[BOTTOM].is_none(), "covered cells stay untouched");
    }

    #[test]
    fn diagonals_and_removal() {
        let mut t = Table::new(2, 2, 200.0, 30.0);
        t.set_borders((0, 0), (0, 0), BorderEdges::parse("diagDown").unwrap(), &red());
        assert!(t.cell(0, 0).unwrap().diag_down.is_some());
        let none = Line { fill: Some(Fill::None), ..Default::default() };
        t.set_borders((0, 0), (1, 1), BorderEdges::parse("all").unwrap(), &none);
        assert!(t.rows.iter().flat_map(|r| &r.cells).all(|c| c.borders.iter().all(|b| b.as_ref().is_some_and(|l| l.fill == Some(Fill::None)))));
        assert!(!t.set_borders((0, 0), (5, 5), BorderEdges::parse("all").unwrap(), &none));
        assert_eq!(BorderEdges::parse("sideways"), None);
    }
}
