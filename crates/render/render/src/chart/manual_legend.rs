//! Layout inside an explicitly sized legend. Its position does not force the flow direction.
use kurbo::Rect;

/// Pack in existing legend entry order, then distribute spare space around the rows and their entries.
/// Compress preferred gaps when all natural item widths fit on one row. Otherwise preserve
/// multirow packing. An undersized box keeps natural item sizes and centered overflow rather than overlapping
/// items, shrinking text, or silently dropping entries.
pub(super) fn cells(widths: &[f64], row_height: f64, min_gap: f64, rect: Rect) -> Vec<Rect> {
    if widths.is_empty() {
        return Vec::new();
    }
    let natural_width: f64 = widths.iter().sum();
    let single_row = natural_width <= rect.width();
    let min_gap = if single_row && widths.len() > 1 { min_gap.min((rect.width() - natural_width) / (widths.len() - 1) as f64) } else { min_gap };
    let mut rows: Vec<(usize, usize, f64)> = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (i, &width) in widths.iter().enumerate() {
        let next = if i == start { width } else { used + min_gap + width };
        if i > start && !single_row && next > rect.width() {
            rows.push((start, i, used));
            start = i;
            used = width;
        } else {
            used = next;
        }
    }
    rows.push((start, widths.len(), used));
    let spare_y = rect.height() - row_height * rows.len() as f64;
    let gap_y = spare_y.max(0.0) / (rows.len() + 1) as f64;
    let mut y = rect.y0 + if spare_y < 0.0 { spare_y / 2.0 } else { gap_y };
    let mut out = Vec::with_capacity(widths.len());
    for (start, end, used) in rows {
        let count = end - start;
        let natural_width = used - min_gap * count.saturating_sub(1) as f64;
        let gap_x = ((rect.width() - natural_width) / (count + 1) as f64).max(min_gap);
        let occupied = natural_width + gap_x * count.saturating_sub(1) as f64;
        let mut x = rect.x0 + (rect.width() - occupied) / 2.0;
        for &width in widths.get(start..end).unwrap_or_default() {
            out.push(Rect::new(x, y, x + width, y + row_height));
            x += width + gap_x;
        }
        y += row_height + gap_y;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_short_and_narrow_tall_boxes_reflow_without_changing_item_sizes() {
        let widths = [30.0; 4];
        let wide = cells(&widths, 12.0, 6.0, Rect::new(10.0, 20.0, 160.0, 44.0));
        assert!(wide.iter().all(|r| r.y0 == wide[0].y0));
        let tall = cells(&widths, 12.0, 6.0, Rect::new(10.0, 20.0, 50.0, 220.0));
        assert!(tall.iter().all(|r| r.x0 == tall[0].x0));
        assert!(tall.windows(2).all(|r| r[1].y0 > r[0].y1));
        assert!(wide.iter().chain(&tall).all(|r| r.width() == 30.0 && r.height() == 12.0));
    }

    #[test]
    fn a_complete_natural_row_compresses_gaps_before_wrapping() {
        let rect = Rect::new(10.0, 20.0, 100.0, 44.0);
        let cells = cells(&[18.0, 27.0, 35.0], 12.0, 8.0, rect);
        assert_eq!(cells, [Rect::new(10.0, 26.0, 28.0, 38.0), Rect::new(33.0, 26.0, 60.0, 38.0), Rect::new(65.0, 26.0, 100.0, 38.0)]);
        assert!(cells.windows(2).all(|r| r[0].x1 < r[1].x0));
    }

    #[test]
    fn exact_natural_fit_preserves_sizes_without_negative_gaps() {
        let cells = cells(&[18.0, 27.0, 35.0], 12.0, 8.0, Rect::new(0.0, 0.0, 80.0, 12.0));
        assert_eq!(cells, [Rect::new(0.0, 0.0, 18.0, 12.0), Rect::new(18.0, 0.0, 45.0, 12.0), Rect::new(45.0, 0.0, 80.0, 12.0)]);
    }

    #[test]
    fn incomplete_natural_rows_keep_existing_multicolumn_packing() {
        // Three natural items could squeeze into this box, but the complete four-item
        // row cannot. Retain the existing two-by-two arrangement and preferred gaps.
        let cells = cells(&[30.0; 4], 12.0, 6.0, Rect::new(0.0, 0.0, 100.0, 80.0));
        assert_eq!(cells[0].y0, cells[1].y0);
        assert_eq!(cells[2].y0, cells[3].y0);
        assert!(cells[0].y1 < cells[2].y0);
        assert!(cells.windows(2).filter(|r| r[0].y0 == r[1].y0).all(|r| r[1].x0 - r[0].x1 >= 6.0));
    }

    #[test]
    fn multiple_columns_preserve_row_major_entry_order() {
        let cells = cells(&[29.0, 30.0, 31.0, 32.0], 12.0, 6.0, Rect::new(0.0, 0.0, 80.0, 100.0));
        assert_eq!(cells.iter().map(|r| r.width()).collect::<Vec<_>>(), [29.0, 30.0, 31.0, 32.0]);
        assert_eq!(cells[0].y0, cells[1].y0);
        assert_eq!(cells[2].y0, cells[3].y0);
        assert!(cells[0].x1 < cells[1].x0 && cells[2].x1 < cells[3].x0);
        assert!(cells[1].y1 < cells[2].y0);
    }

    #[test]
    fn extra_height_is_distributed_between_rows_and_outer_edges() {
        let rect = Rect::new(10.0, 20.0, 50.0, 120.0);
        let cells = cells(&[30.0; 3], 12.0, 6.0, rect);
        let before = cells[0].y0 - rect.y0;
        let between = cells[1].y0 - cells[0].y1;
        let after = rect.y1 - cells[2].y1;
        assert!((before - between).abs() < 1e-9 && (between - after).abs() < 1e-9);
        assert!(cells.iter().all(|r| rect.contains(r.origin()) && rect.contains((r.x1, r.y1))));
    }

    #[test]
    fn undersized_boxes_retain_every_entry_without_overlap_or_font_scaling() {
        let cells = cells(&[60.0, 70.0, 80.0], 12.0, 6.0, Rect::new(0.0, 0.0, 40.0, 20.0));
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].width(), 60.0);
        assert_eq!(cells[2].width(), 80.0);
        assert!(cells.iter().all(|r| r.height() == 12.0 && r.center().x == 20.0));
        assert!(cells.windows(2).all(|r| r[0].y1 <= r[1].y0));
        assert_eq!((cells[0].y0 + cells[2].y1) / 2.0, 10.0);
    }

    #[test]
    fn exact_fit_keeps_minimum_separation_and_empty_input_has_no_cells() {
        let cells = cells(&[30.0, 40.0], 12.0, 6.0, Rect::new(0.0, 0.0, 76.0, 12.0));
        assert_eq!(cells, [Rect::new(0.0, 0.0, 30.0, 12.0), Rect::new(36.0, 0.0, 76.0, 12.0)]);
        assert!(super::cells(&[], 12.0, 6.0, Rect::ZERO).is_empty());
    }

    #[test]
    fn explicit_legend_box_overrides_side_flow_but_position_only_layout_does_not() {
        use super::super::*;
        use kition_slide_model::SlideFile;
        let presentation = SlideFile::default();
        let rctx = Ctx::for_slide(&presentation, &presentation.slides[0]).unwrap();
        let layout = |manual| {
            let mut chart = Chart::sample(ChartType::Column);
            chart.fmt.legend_layout = manual;
            let base = Txt {
                font: Some("Inter".into()),
                font_ea: None,
                font_cs: None,
                size: 9.0,
                spacing: None,
                bold: false,
                italic: false,
                color: Rgba::BLACK,
                rot: None,
            };
            let env =
                Env { rctx: &rctx, c: &chart, m: Affine::IDENTITY, pal: Pal { rctx: &rctx, scheme: None }, base: base.clone(), size: (240.0, 160.0) };
            let entries: Vec<_> =
                (0..4).map(|_| Entry { text: "AB".into(), key: Key::Box, fill: None, col: Rgba::BLACK, line: None, series: None }).collect();
            legend_layout(&env, &entries, &base, "r", Rect::new(0.0, 0.0, 240.0, 160.0), 240.0, 160.0)
        };
        let positioned = ManualLayout { x: Some(0.2), y: Some(0.1), x_edge: true, y_edge: true, ..Default::default() };
        let wide = layout(Some(ManualLayout { w: Some(0.5), h: Some(0.2), ..positioned.clone() }));
        assert_eq!(wide.rect, Rect::new(48.0, 16.0, 168.0, 48.0));
        assert!(wide.items.iter().all(|item| item.0.y0 == wide.items[0].0.y0));
        for control in [layout(None), layout(Some(positioned.clone()))] {
            assert!(control.items.windows(2).all(|items| items[0].0.y1 <= items[1].0.y0));
        }
        let legacy = layout(Some(positioned.clone()));
        let same = |actual: f64, expected: f64| {
            assert!(actual == expected || (actual - expected).abs() < 1e-9 || (actual.is_nan() && expected.is_nan()), "{actual} != {expected}")
        };
        let mut dimensions = vec![(Some(0.5), None), (None, Some(0.5)), (None, None)];
        for invalid in [0.0, -0.25, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            dimensions.push((Some(invalid), Some(0.5)));
            dimensions.push((Some(0.5), Some(invalid)));
        }
        // Legacy side legends use a common key/text column. Authored height only
        // shifts the packed block; authored width does not reposition those columns.
        for (width, height) in dimensions {
            let result = layout(Some(ManualLayout { w: width, h: height, ..positioned.clone() }));
            let height = height.map(|v| v * 160.0).unwrap_or(legacy.rect.height());
            let shift_y = (height - legacy.rect.height()) / 2.0;
            same(result.rect.x1, legacy.rect.x0 + width.map(|v| v * 240.0).unwrap_or(legacy.rect.width()));
            same(result.rect.y1, legacy.rect.y0 + height);
            for ((key, text), (old_key, old_text)) in result.items.iter().zip(&legacy.items) {
                for (actual, old) in [(key, old_key), (text, old_text)] {
                    same(actual.x0, old.x0);
                    same(actual.x1, old.x1);
                    same(actual.y0, old.y0 + shift_y);
                    same(actual.y1, old.y1 + shift_y);
                }
            }
        }
    }
}
