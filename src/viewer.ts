// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Main image viewer: fit-to-window, actual size, pan and zoom. Drawn with Canvas 2D for now;
// the wgpu renderer (Phase 2) replaces the drawing step without changing this interface.

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
      this.drag = { x: e.clientX, y: e.clientY, ox: this.offsetX, oy: this.offsetY };
      this.canvas.classList.add("dragging");
    });
    this.on(this.canvas, "pointermove", (e) => {
      if (!this.drag) return;
      this.fitMode = false;
      this.offsetX = this.drag.ox + (e.clientX - this.drag.x);
      this.offsetY = this.drag.oy + (e.clientY - this.drag.y);
      this.draw();
    });
    const endDrag = () => {
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
    this.fitToWindow();
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
