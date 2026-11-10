// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Virtualized grid geometry. Mirrors core/src/grid.rs so the browser realises only the cells
// that intersect the viewport, keeping tens of thousands of entries responsive.

export interface GridLayout {
  itemWidth: number;
  itemHeight: number;
  gap: number;
}

export function columns(layout: GridLayout, viewportWidth: number): number {
  if (!(layout.itemWidth > 0) || !Number.isFinite(viewportWidth)) return 1;
  const cols = Math.floor((viewportWidth + layout.gap) / (layout.itemWidth + layout.gap));
  return Math.max(1, cols);
}

/** Height of the scrollable area for `total` items, excluding outer padding. */
export function contentHeight(layout: GridLayout, total: number, viewportWidth: number): number {
  const rows = Math.ceil(total / columns(layout, viewportWidth));
  if (rows === 0) return 0;
  return rows * layout.itemHeight + (rows - 1) * layout.gap;
}

/** Half-open index range of items intersecting the viewport, widened by `overscanRows`. */
export function visibleRange(
  layout: GridLayout,
  total: number,
  viewportWidth: number,
  viewportHeight: number,
  scrollTop: number,
  overscanRows: number,
): { start: number; end: number } {
  if (total === 0 || !(layout.itemHeight > 0)) return { start: 0, end: 0 };
  const cols = columns(layout, viewportWidth);
  const pitch = layout.itemHeight + layout.gap;
  const firstRow = Math.floor(Math.max(0, scrollTop) / pitch);
  const rowsInView = Math.ceil(Math.max(0, viewportHeight) / pitch) + 1;
  const startRow = Math.max(0, firstRow - overscanRows);
  const endRow = firstRow + rowsInView + overscanRows;
  const start = Math.min(startRow * cols, total);
  const end = Math.min(endRow * cols, total);
  return { start, end: Math.max(start, end) };
}
