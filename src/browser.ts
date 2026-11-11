// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Thumbnail grid: virtualized so only visible cells exist in the DOM, with thumbnails fetched
// lazily through a small concurrency-limited queue backed by the on-disk cache.

import { api } from "./ipc";
import type { EntryDto } from "./types";
import { columns, contentHeight, visibleRange, type GridLayout } from "./grid";
import { h } from "./dom";

const LAYOUT: GridLayout = { itemWidth: 160, itemHeight: 184, gap: 12 };
const OVERSCAN_ROWS = 2;
// Thumbnail requests in flight at once. Decoding runs on the backend's blocking pool, so this
// scales with the machine's cores, within a sensible range.
const MAX_CONCURRENT_THUMBS = Math.min(8, Math.max(2, navigator.hardwareConcurrency || 4));

export interface BrowserCallbacks {
  onOpen: (index: number) => void;
  onSelectionChange: (selected: number[]) => void;
}

export class Browser {
  private readonly scroller: HTMLElement;
  private readonly spacer: HTMLElement;
  private readonly cells = new Map<number, HTMLElement>();
  private readonly thumbs = new Map<string, string>();
  private readonly pending = new Set<number>();
  private inFlight = 0;
  private entries: EntryDto[] = [];
  private selected = new Set<number>();
  private anchor = 0;
  private frame: number | null = null;
  private generation = 0;

  constructor(
    host: HTMLElement,
    private readonly thumbEdge: number,
    private readonly callbacks: BrowserCallbacks,
  ) {
    this.spacer = h("div", { class: "grid-spacer" });
    this.scroller = h("div", { class: "grid-scroller", tabindex: 0 }, this.spacer);
    this.scroller.addEventListener("scroll", () => this.scheduleRender());
    window.addEventListener("resize", () => this.scheduleRender());
    // Re-render when the grid becomes visible again (it is hidden while viewing an image).
    new ResizeObserver(() => this.scheduleRender()).observe(this.scroller);
    host.append(this.scroller);
  }

  setEntries(entries: EntryDto[]): void {
    this.generation++;
    this.entries = entries;
    this.selected.clear();
    this.anchor = 0;
    this.pending.clear();
    this.thumbs.clear();
    for (const cell of this.cells.values()) cell.remove();
    this.cells.clear();
    this.scroller.scrollTop = 0;
    this.callbacks.onSelectionChange([]);
    this.render();
  }

  get count(): number {
    return this.entries.length;
  }

  selectedIndices(): number[] {
    return [...this.selected].sort((a, b) => a - b);
  }

  selectAll(): void {
    this.selected = new Set(this.entries.map((_, i) => i));
    this.refreshSelection();
  }

  /** Moves focus to `index`, scrolling it into view. */
  focusIndex(index: number): void {
    if (this.entries.length === 0) return;
    const i = Math.min(Math.max(0, index), this.entries.length - 1);
    this.anchor = i;
    this.selected = new Set([i]);
    this.ensureVisible(i);
    this.refreshSelection();
  }

  moveFocus(delta: number): void {
    this.focusIndex(this.anchor + delta);
  }

  /** Moves focus up (-1) or down (+1) by one row. */
  moveRow(direction: -1 | 1): void {
    this.focusIndex(this.anchor + direction * columns(LAYOUT, this.scroller.clientWidth));
  }

  private ensureVisible(index: number): void {
    const cols = columns(LAYOUT, this.scroller.clientWidth);
    const row = Math.floor(index / cols);
    const top = LAYOUT.gap + row * (LAYOUT.itemHeight + LAYOUT.gap);
    const bottom = top + LAYOUT.itemHeight;
    if (top < this.scroller.scrollTop) {
      this.scroller.scrollTop = top - LAYOUT.gap;
    } else if (bottom > this.scroller.scrollTop + this.scroller.clientHeight) {
      this.scroller.scrollTop = bottom - this.scroller.clientHeight + LAYOUT.gap;
    }
    this.scheduleRender();
  }

  private handleClick(index: number, e: MouseEvent): void {
    if (e.shiftKey) {
      const [lo, hi] = index < this.anchor ? [index, this.anchor] : [this.anchor, index];
      this.selected = new Set();
      for (let i = lo; i <= hi; i++) this.selected.add(i);
    } else if (e.ctrlKey || e.metaKey) {
      if (this.selected.has(index)) this.selected.delete(index);
      else this.selected.add(index);
      this.anchor = index;
    } else {
      this.selected = new Set([index]);
      this.anchor = index;
    }
    this.refreshSelection();
  }

  private refreshSelection(): void {
    for (const [index, cell] of this.cells) {
      cell.classList.toggle("selected", this.selected.has(index));
      cell.classList.toggle("focused", index === this.anchor);
    }
    this.callbacks.onSelectionChange(this.selectedIndices());
  }

  private scheduleRender(): void {
    if (this.frame !== null) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      this.render();
    });
  }

  private render(): void {
    const width = this.scroller.clientWidth;
    const height = this.scroller.clientHeight;
    const cols = columns(LAYOUT, width);
    const total = this.entries.length;
    this.spacer.style.height = `${contentHeight(LAYOUT, total, width) + LAYOUT.gap * 2}px`;

    const { start, end } = visibleRange(
      LAYOUT,
      total,
      width,
      height,
      this.scroller.scrollTop,
      OVERSCAN_ROWS,
    );

    for (const [index, cell] of this.cells) {
      if (index < start || index >= end) {
        cell.remove();
        this.cells.delete(index);
      }
    }
    for (let i = start; i < end; i++) {
      if (!this.cells.has(i)) this.createCell(i, cols);
    }
  }

  private createCell(index: number, cols: number): void {
    const entry = this.entries[index];
    const row = Math.floor(index / cols);
    const col = index % cols;
    const img = h("img", { alt: "", draggable: false });
    const label = h("div", { class: "cell-label", title: entry.path }, entry.name);
    const cell = h("div", { class: "cell", role: "option" }, img, label);
    cell.style.left = `${LAYOUT.gap + col * (LAYOUT.itemWidth + LAYOUT.gap)}px`;
    cell.style.top = `${LAYOUT.gap + row * (LAYOUT.itemHeight + LAYOUT.gap)}px`;
    cell.style.width = `${LAYOUT.itemWidth}px`;
    cell.style.height = `${LAYOUT.itemHeight}px`;
    cell.classList.toggle("selected", this.selected.has(index));
    cell.classList.toggle("focused", index === this.anchor);
    cell.addEventListener("click", (e) => this.handleClick(index, e));
    cell.addEventListener("dblclick", () => this.callbacks.onOpen(index));
    this.spacer.append(cell);
    this.cells.set(index, cell);

    const cached = this.thumbs.get(entry.path);
    if (cached) {
      img.src = cached;
    } else {
      this.enqueueThumb(index);
    }
  }

  private enqueueThumb(index: number): void {
    if (this.pending.has(index)) return;
    this.pending.add(index);
    this.pump();
  }

  private pump(): void {
    while (this.inFlight < MAX_CONCURRENT_THUMBS && this.pending.size > 0) {
      // Prefer the most recently requested indices: they are likeliest to still be on screen.
      const index = [...this.pending].pop() as number;
      this.pending.delete(index);
      // Skip cells that scrolled out of view while queued.
      if (!this.cells.has(index)) continue;
      const entry = this.entries[index];
      const cached = this.thumbs.get(entry.path);
      if (cached) {
        this.setThumb(index, cached);
      } else {
        this.loadThumb(index);
      }
    }
  }

  private setThumb(index: number, url: string): void {
    const img = this.cells.get(index)?.querySelector("img");
    if (img) img.src = url;
  }

  private loadThumb(index: number): void {
    const entry = this.entries[index];
    const gen = this.generation;
    this.inFlight++;
    api
      .thumbnail(entry.path, this.thumbEdge)
      .then((url) => {
        if (gen !== this.generation) return;
        this.thumbs.set(entry.path, url);
        this.setThumb(index, url);
      })
      .catch(() => {
        if (gen === this.generation) this.cells.get(index)?.classList.add("broken");
      })
      .finally(() => {
        // Always release the slot, even for requests from a previous folder.
        this.inFlight--;
        if (gen === this.generation) this.pump();
      });
  }

  destroy(): void {
    this.scroller.remove();
  }
}
