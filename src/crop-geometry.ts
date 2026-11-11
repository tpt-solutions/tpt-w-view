// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Pure geometry for the on-canvas crop selection. Points are in image pixels. The result is a
// whole-pixel rectangle that lies inside the image, optionally constrained to an aspect ratio.

export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Point {
  x: number;
  y: number;
}

/**
 * Rectangle spanned by a drag from `anchor` to `pointer`. The drag direction decides which
 * corner the anchor is. With `aspect` (width divided by height), the shorter side is kept and the
 * longer one follows the ratio. The rectangle is shrunk, never moved, to stay inside the image.
 */
export function cropFromDrag(
  rawAnchor: Point,
  rawPointer: Point,
  bounds: { width: number; height: number },
  aspect: number | null,
): CropRect {
  // Points outside the image (a drag that starts or ends off the picture) are pulled onto it.
  const anchor = {
    x: clamp(rawAnchor.x, 0, bounds.width),
    y: clamp(rawAnchor.y, 0, bounds.height),
  };
  const pointer = {
    x: clamp(rawPointer.x, 0, bounds.width),
    y: clamp(rawPointer.y, 0, bounds.height),
  };
  const dx = pointer.x - anchor.x;
  const dy = pointer.y - anchor.y;
  const signX = dx < 0 ? -1 : 1;
  const signY = dy < 0 ? -1 : 1;
  let w = Math.abs(dx);
  let h = Math.abs(dy);

  if (aspect !== null && aspect > 0) {
    if (w / Math.max(h, Number.EPSILON) > aspect) {
      w = h * aspect;
    } else {
      h = w / aspect;
    }
  }

  // Room to the edge of the image in the direction of the drag.
  const roomX = signX > 0 ? bounds.width - anchor.x : anchor.x;
  const roomY = signY > 0 ? bounds.height - anchor.y : anchor.y;
  if (aspect !== null && aspect > 0) {
    const scale = Math.min(1, roomX / Math.max(w, Number.EPSILON), roomY / Math.max(h, Number.EPSILON));
    w *= scale;
    h *= scale;
  } else {
    w = Math.min(w, roomX);
    h = Math.min(h, roomY);
  }

  const left = signX > 0 ? anchor.x : anchor.x - w;
  const top = signY > 0 ? anchor.y : anchor.y - h;
  const x = clamp(Math.round(left), 0, Math.max(0, bounds.width));
  const y = clamp(Math.round(top), 0, Math.max(0, bounds.height));
  const width = clamp(Math.round(w), 0, bounds.width - x);
  const height = clamp(Math.round(h), 0, bounds.height - y);
  return { x, y, w: width, h: height };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Width-to-height ratios offered by the crop panel. `null` is free-form. */
export const CROP_ASPECTS: ReadonlyArray<{ label: string; ratio: number | null }> = [
  { label: "Free", ratio: null },
  { label: "1:1", ratio: 1 },
  { label: "4:3", ratio: 4 / 3 },
  { label: "3:2", ratio: 3 / 2 },
  { label: "16:9", ratio: 16 / 9 },
];
