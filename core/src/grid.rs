// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Virtualized thumbnail-grid geometry (Phase 3). Given a scroll position and viewport, computes
//! which item indices need to be realised, so tens of thousands of entries never need DOM or
//! thumbnail work up front.

use std::ops::Range;

/// Fixed-size cell layout for a thumbnail grid. All values are in the same unit (CSS pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLayout {
    pub item_width: f64,
    pub item_height: f64,
    pub gap: f64,
}

impl GridLayout {
    /// Number of columns that fit in `viewport_width` (always at least one).
    pub fn columns(&self, viewport_width: f64) -> usize {
        if (self.item_width.is_nan() || self.item_width <= 0.0) || !viewport_width.is_finite() {
            return 1;
        }
        let cols = ((viewport_width + self.gap) / (self.item_width + self.gap)).floor();
        if cols < 1.0 {
            1
        } else {
            cols as usize
        }
    }

    /// Total scrollable height for `total` items at the given viewport width.
    pub fn content_height(&self, total: usize, viewport_width: f64) -> f64 {
        let rows = total.div_ceil(self.columns(viewport_width));
        if rows == 0 {
            return 0.0;
        }
        rows as f64 * self.item_height + (rows - 1) as f64 * self.gap
    }

    /// Indices of items that intersect the viewport, widened by `overscan_rows` on each side so
    /// fast scrolling does not reveal blank cells.
    pub fn visible_range(
        &self,
        total: usize,
        viewport_width: f64,
        viewport_height: f64,
        scroll_top: f64,
        overscan_rows: usize,
    ) -> Range<usize> {
        if total == 0 || (self.item_height.is_nan() || self.item_height <= 0.0) {
            return 0..0;
        }
        let cols = self.columns(viewport_width);
        let row_pitch = self.item_height + self.gap;
        let first_row = (scroll_top.max(0.0) / row_pitch).floor() as usize;
        let rows_in_view = (viewport_height.max(0.0) / row_pitch).ceil() as usize + 1;

        let start_row = first_row.saturating_sub(overscan_rows);
        let end_row = first_row + rows_in_view + overscan_rows;

        let start = (start_row * cols).min(total);
        let end = (end_row.saturating_mul(cols)).min(total);
        start..end.max(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: GridLayout = GridLayout {
        item_width: 100.0,
        item_height: 100.0,
        gap: 10.0,
    };

    #[test]
    fn column_count_follows_width() {
        assert_eq!(LAYOUT.columns(0.0), 1);
        assert_eq!(LAYOUT.columns(100.0), 1);
        assert_eq!(LAYOUT.columns(220.0), 2);
        assert_eq!(LAYOUT.columns(1000.0), 9);
    }

    #[test]
    fn content_height_accounts_for_gaps() {
        // 10 items, 4 columns -> 3 rows: 3*100 + 2*10 gaps.
        assert_eq!(LAYOUT.content_height(10, 450.0), 320.0);
        assert_eq!(LAYOUT.content_height(0, 450.0), 0.0);
    }

    #[test]
    fn visible_range_at_top_and_middle() {
        // Viewport 450 wide (4 cols), 230 tall, no overscan.
        // 230 / 110 pitch -> ceil(2.09) + 1 = 4 rows realised, 4 columns each.
        let top = LAYOUT.visible_range(100_000, 450.0, 230.0, 0.0, 0);
        assert_eq!(top, 0..16);
        let mid = LAYOUT.visible_range(100_000, 450.0, 230.0, 5500.0, 0);
        // 5500 / 110 = row 50 exactly.
        assert_eq!(mid.start, 50 * 4);
    }

    #[test]
    fn overscan_widens_range_and_clamps_to_total() {
        let r = LAYOUT.visible_range(20, 450.0, 230.0, 0.0, 1);
        assert_eq!(r, 0..20);
        // Realised rows 0..(4 + 2) with overscan 2 -> 6 rows * 4 columns.
        let r = LAYOUT.visible_range(100_000, 450.0, 230.0, 0.0, 2);
        assert_eq!(r, 0..24);
    }

    #[test]
    fn scrolling_past_end_yields_empty_range() {
        let r = LAYOUT.visible_range(10, 450.0, 230.0, 1_000_000.0, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn degenerate_layout_is_safe() {
        let bad = GridLayout {
            item_width: 0.0,
            item_height: 0.0,
            gap: 0.0,
        };
        assert_eq!(bad.columns(500.0), 1);
        assert!(bad.visible_range(50, 500.0, 500.0, 0.0, 0).is_empty());
        assert_eq!(LAYOUT.columns(f64::NAN), 1);
    }
}
