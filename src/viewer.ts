// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Main image viewer: fit-to-window, actual size, pan and zoom. Drawn with Canvas 2D for now;
// the wgpu renderer (Phase 2) replaces the drawing step without changing this interface.

import { cropFromDrag, type CropRect, type Point } from "./crop-geometry";

const MIN_SCALE = 0.01;
const MAX_SCALE = 32;

export class Viewer {
  private readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  private image: HTMLImageElement | null = null;
  private scale = 1;
  private fitMode = true;
  private offsetX = 0;
  private offsetY = 0;
  private drag: { x: number; y: number; ox: number; oy: number } | null = null;
  /** Crop selection in image pixels; active only while `cropListener` is set (edit view). */
  private cropListener: ((rect: CropRect) => void) | null = null;
  private cropAspect: number | null = null;
  private cropAnchor: Point | null = null;
  private cropSel: CropRect | null = null;
  private readonly resizeObserver: ResizeObserver;
  private readonly listeners: Array<() => void> = [];

  constructor(host: HTMLElement, private readonly onZoomChange?: (percent: number) => void) {
    this.canvas = document.createElement("canvas");
    this.canvas.className = "viewer-canvas";
    host.append(this.canvas);
    const ctx = this.canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas unavailable");
    this.ctx = ctx;

    this.on(this.canvas, "pointerdown", (e) => {
      this.canvas.setPointerCapture(e.pointerId);
      if (this.cropListener && this.image) {
        const start = this.imagePoint(e);
        this.cropAnchor = start;
        this.cropSel = null;
        this.draw();
        return;
      }
      this.drag = { x: e.clientX, y: e.clientY, ox: this.offsetX, oy: this.offsetY };
      this.canvas.classList.add("dragging");
    });
    this.on(this.canvas, "pointermove", (e) => {
      if (this.cropAnchor && this.image) {
        this.cropSel = cropFromDrag(
          this.cropAnchor,
          this.imagePoint(e),
          { width: this.image.naturalWidth, height: this.image.naturalHeight },
          this.cropAspect,
        );
        this.draw();
        return;
      }
      if (!this.drag) return;
      this.fitMode = false;
      this.offsetX = this.drag.ox + (e.clientX - this.drag.x);
      this.offsetY = this.drag.oy + (e.clientY - this.drag.y);
      this.draw();
    });
    const endDrag = () => {
      if (this.cropAnchor) {
        this.cropAnchor = null;
        const sel = this.cropSel;
        if (sel && sel.w > 0 && sel.h > 0) {
          this.cropListener?.(sel);
        } else {
          this.cropSel = null;
        }
        this.draw();
      }
      this.drag = null;
      this.canvas.classList.remove("dragging");
    };
    this.on(this.canvas, "pointerup", endDrag);
    this.on(this.canvas, "pointercancel", endDrag);
    this.on(this.canvas, "wheel", (e) => {
      e.preventDefault();
      const rect = this.canvas.getBoundingClientRect();
      const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
      this.zoomAt(factor, e.clientX - rect.left, e.clientY - rect.top);
    }, { passive: false });

    this.resizeObserver = new ResizeObserver(() => this.onResize());
    this.resizeObserver.observe(host);
    this.onResize();
  }

  private on<K extends keyof HTMLElementEventMap>(
    target: HTMLElement,
    type: K,
    fn: (e: HTMLElementEventMap[K]) => void,
    options?: AddEventListenerOptions,
  ): void {
    target.addEventListener(type, fn as EventListener, options);
    this.listeners.push(() => target.removeEventListener(type, fn as EventListener, options));
  }

  /** Shows a new image from a URL (data URLs are fine). Resolves once it has been drawn. */
  async show(url: string): Promise<void> {
    const img = new Image();
    img.decoding = "async";
    img.src = url;
    await img.decode();
    this.image = img;
    this.fitMode = true;
    this.cropSel = null;
    this.fitToWindow();
  }

  /**
   * Turns on crop selection: a drag draws a rectangle (instead of panning) and `onSelect` gets
   * each finished, non-empty rectangle in image pixels. Pass `null` to turn it off.
   */
  setCropMode(onSelect: ((rect: CropRect) => void) | null): void {
    this.cropListener = onSelect;
    this.cropAnchor = null;
    this.cropSel = null;
    this.canvas.classList.toggle("cropping", onSelect !== null);
    this.draw();
  }

  /** Width-to-height ratio for new selections, or `null` for free-form. */
  setCropAspect(aspect: number | null): void {
    this.cropAspect = aspect;
  }

  /** Converts a pointer event to image pixel coordinates, without clamping. */
  private imagePoint(e: PointerEvent): Point {
    const rect = this.canvas.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left - this.offsetX) / this.scale,
      y: (e.clientY - rect.top - this.offsetY) / this.scale,
    };
  }

  clear(): void {
    this.image = null;
    this.draw();
  }

  get imageSize(): { width: number; height: number } | null {
    return this.image ? { width: this.image.naturalWidth, height: this.image.naturalHeight } : null;
  }

  /** Scales down to fit, but never enlarges a small image. */
  fitToWindow(): void {
    if (!this.image) return;
    this.fitMode = true;
    const { cssWidth, cssHeight } = this.size();
    const s = Math.min(
      cssWidth / this.image.naturalWidth,
      cssHeight / this.image.naturalHeight,
      1,
    );
    this.setScale(s);
  }

  actualSize(): void {
    this.fitMode = false;
    this.setScale(1);
  }

  zoomAt(factor: number, x: number, y: number): void {
    if (!this.image) return;
    this.fitMode = false;
    const next = clamp(this.scale * factor, MIN_SCALE, MAX_SCALE);
    const ratio = next / this.scale;
    // Keep the image point under the cursor fixed.
    this.offsetX = x - (x - this.offsetX) * ratio;
    this.offsetY = y - (y - this.offsetY) * ratio;
    this.scale = next;
    this.draw();
  }

  zoomCentre(factor: number): void {
    const { cssWidth, cssHeight } = this.size();
    this.zoomAt(factor, cssWidth / 2, cssHeight / 2);
  }

  get zoomPercent(): number {
    return Math.round(this.scale * 100);
  }

  private setScale(scale: number): void {
    if (!this.image) return;
    this.scale = clamp(scale, MIN_SCALE, MAX_SCALE);
    const { cssWidth, cssHeight } = this.size();
    const w = this.image.naturalWidth * this.scale;
    const h = this.image.naturalHeight * this.scale;
    this.offsetX = (cssWidth - w) / 2;
    this.offsetY = (cssHeight - h) / 2;
    this.draw();
  }

  private size(): { cssWidth: number; cssHeight: number } {
    return {
      cssWidth: this.canvas.clientWidth,
      cssHeight: this.canvas.clientHeight,
    };
  }

  private onResize(): void {
    const { cssWidth, cssHeight } = this.size();
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.round(cssWidth * dpr));
    this.canvas.height = Math.max(1, Math.round(cssHeight * dpr));
    if (this.fitMode) {
      this.fitToWindow();
    } else {
      this.draw();
    }
  }

  /** Dims everything outside the selection and outlines the selection. */
  private drawCropOverlay(sel: CropRect, cssWidth: number, cssHeight: number): void {
    const ctx = this.ctx;
    const sx = this.offsetX + sel.x * this.scale;
    const sy = this.offsetY + sel.y * this.scale;
    const sw = sel.w * this.scale;
    const sh = sel.h * this.scale;
    ctx.fillStyle = "rgba(0, 0, 0, 0.5)";
    const fill = (x: number, y: number, w: number, hh: number) => {
      if (w > 0 && hh > 0) ctx.fillRect(x, y, w, hh);
    };
    fill(0, 0, cssWidth, sy);
    fill(0, sy + sh, cssWidth, cssHeight - (sy + sh));
    fill(0, sy, sx, sh);
    fill(sx + sw, sy, cssWidth - (sx + sw), sh);
    ctx.strokeStyle = "#ffffff";
    ctx.lineWidth = 1;
    ctx.setLineDash([6, 4]);
    ctx.strokeRect(sx + 0.5, sy + 0.5, sw, sh);
    ctx.setLineDash([]);
  }

  private draw(): void {
    const dpr = window.devicePixelRatio || 1;
    const { cssWidth, cssHeight } = this.size();
    const ctx = this.ctx;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    if (!this.image) {
      this.onZoomChange?.(0);
      return;
    }
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(
      this.image,
      this.offsetX,
      this.offsetY,
      this.image.naturalWidth * this.scale,
      this.image.naturalHeight * this.scale,
    );
    if (this.cropSel && this.cropSel.w > 0 && this.cropSel.h > 0) {
      this.drawCropOverlay(this.cropSel, cssWidth, cssHeight);
    }
    this.onZoomChange?.(this.zoomPercent);
  }

  destroy(): void {
    this.resizeObserver.disconnect();
    this.listeners.forEach((remove) => remove());
    this.canvas.remove();
  }
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}
