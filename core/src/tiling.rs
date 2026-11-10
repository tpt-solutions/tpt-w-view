// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Tiling and mip-level arithmetic for large-image display (Phase 2).
//!
//! Huge images are uploaded to the GPU as fixed-size tiles, and each tile is shown at the mip
//! level closest to the on-screen zoom. This module is the pure geometry behind that: it does
//! not allocate GPU resources, so it is testable on any machine.

use std::ops::Range;

/// Pixel rectangle in image space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Fixed-size tile grid over an image of `width` × `height` pixels. Edge tiles are clipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
}

impl TileGrid {
    pub fn new(width: u32, height: u32, tile_size: u32) -> Option<Self> {
        (width > 0 && height > 0 && tile_size > 0).then_some(Self {
            width,
            height,
            tile_size,
        })
    }

    pub fn columns(&self) -> u32 {
        self.width.div_ceil(self.tile_size)
    }

    pub fn rows(&self) -> u32 {
        self.height.div_ceil(self.tile_size)
    }

    /// Total number of tiles.
    pub fn len(&self) -> usize {
        self.columns() as usize * self.rows() as usize
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Image-space rectangle for tile `(col, row)`, clipped to the image. `None` if out of range.
    pub fn tile_rect(&self, col: u32, row: u32) -> Option<Rect> {
        if col >= self.columns() || row >= self.rows() {
            return None;
        }
        let x = col * self.tile_size;
        let y = row * self.tile_size;
        Some(Rect {
            x,
            y,
            width: (self.width - x).min(self.tile_size),
            height: (self.height - y).min(self.tile_size),
        })
    }

    /// Tiles intersecting `region` (image space), as `(col_range, row_range)`. The region is
    /// clipped to the image first, so an off-image region yields empty ranges.
    pub fn tiles_covering(&self, region: Rect) -> (Range<u32>, Range<u32>) {
        let x0 = region.x.min(self.width);
        let y0 = region.y.min(self.height);
        let x1 = region.x.saturating_add(region.width).min(self.width);
        let y1 = region.y.saturating_add(region.height).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return (0..0, 0..0);
        }
        let ts = self.tile_size;
        (x0 / ts..(x1 - 1) / ts + 1, y0 / ts..(y1 - 1) / ts + 1)
    }
}

/// Number of mip levels for an image, including level 0 (full resolution), down to 1×1.
pub fn mip_level_count(width: u32, height: u32) -> u32 {
    let longest = width.max(height).max(1);
    32 - longest.leading_zeros()
}

/// Dimensions of mip `level` (each level halves, rounding up, never below 1).
pub fn mip_size(width: u32, height: u32, level: u32) -> (u32, u32) {
    let shrink = |d: u32| (d >> level.min(31)).max(1);
    (shrink(width), shrink(height))
}

/// Picks the mip level to sample for an on-screen `zoom` (screen pixels per image pixel).
///
/// Zoom 1.0 samples level 0. Zooming out by half picks level 1, and so on. Zooming in never
/// picks a coarser level than 0. The result is clamped to the available levels.
pub fn mip_level_for_zoom(zoom: f64, levels: u32) -> u32 {
    if !(zoom.is_finite() && zoom > 0.0) || levels == 0 {
        return 0;
    }
    if zoom >= 1.0 {
        return 0;
    }
    let level = (-zoom.log2()).floor() as u32;
    level.min(levels - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_counts_and_clips_edge_tiles() {
        let g = TileGrid::new(1000, 300, 256).unwrap();
        assert_eq!((g.columns(), g.rows()), (4, 2));
        assert_eq!(g.len(), 8);
        assert_eq!(
            g.tile_rect(3, 1),
            Some(Rect {
                x: 768,
                y: 256,
                width: 232,
                height: 44
            })
        );
        assert_eq!(g.tile_rect(4, 0), None);
    }

    #[test]
    fn covering_region_maps_to_tile_ranges() {
        let g = TileGrid::new(1000, 1000, 256).unwrap();
        let (cols, rows) = g.tiles_covering(Rect {
            x: 250,
            y: 0,
            width: 20,
            height: 300,
        });
        assert_eq!(cols, 0..2);
        assert_eq!(rows, 0..2);
        let (cols, _) = g.tiles_covering(Rect {
            x: 2000,
            y: 0,
            width: 10,
            height: 10,
        });
        assert!(cols.is_empty());
    }

    #[test]
    fn invalid_grid_is_none() {
        assert!(TileGrid::new(0, 10, 256).is_none());
        assert!(TileGrid::new(10, 10, 0).is_none());
    }

    #[test]
    fn mip_chain_length_and_sizes() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(256, 256), 9);
        assert_eq!(mip_level_count(1000, 10), 10);
        assert_eq!(mip_size(1000, 10, 0), (1000, 10));
        assert_eq!(mip_size(1000, 10, 1), (500, 5));
        assert_eq!(mip_size(1000, 10, 9), (1, 1));
    }

    #[test]
    fn mip_selection_follows_zoom() {
        assert_eq!(mip_level_for_zoom(2.0, 8), 0, "zoomed in");
        assert_eq!(mip_level_for_zoom(1.0, 8), 0);
        assert_eq!(mip_level_for_zoom(0.5, 8), 1);
        assert_eq!(mip_level_for_zoom(0.26, 8), 1);
        assert_eq!(mip_level_for_zoom(0.25, 8), 2);
        assert_eq!(mip_level_for_zoom(1e-9, 8), 7, "clamped to last level");
        assert_eq!(mip_level_for_zoom(f64::NAN, 8), 0);
    }
}
