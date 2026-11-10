// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Image information sidebar: file identity, dimensions, camera data and GPS.

import { formatBytes, h } from "./dom";
import type { EntryDto, InfoDto } from "./types";

const ORIENTATION_TEXT: Record<number, string> = {
  1: "Normal",
  2: "Mirrored horizontally",
  3: "Rotated 180°",
  4: "Mirrored vertically",
  5: "Transposed",
  6: "Rotated 90° CW",
  7: "Transverse",
  8: "Rotated 270° CW",
};

export function renderInfo(host: HTMLElement, entry: EntryDto, info: InfoDto): void {
  const m = info.metadata;
  const rows: Array<[string, string | null]> = [
    ["File", entry.name],
    ["Size", formatBytes(entry.size)],
    ["Format", info.format],
    ["Dimensions", `${info.width} × ${info.height} px`],
    ["Orientation", ORIENTATION_TEXT[m.orientation] ?? String(m.orientation)],
    ["Camera make", m.make],
    ["Camera model", m.model],
    ["Taken", m.date_time_original],
    ["GPS", m.gps ? `${m.gps[0].toFixed(6)}, ${m.gps[1].toFixed(6)}` : null],
    ["Title", m.title],
    ["Description", m.description],
    ["Creator", m.creator],
    ["Copyright", m.copyright],
    ["Keywords", m.keywords.length ? m.keywords.join(", ") : null],
  ];
  const list = h("dl", { class: "info-list" });
  for (const [label, value] of rows) {
    if (value === null || value === "") continue;
    list.append(h("dt", { text: label }), h("dd", { text: value }));
  }
  host.replaceChildren(h("h3", { text: "Image info" }), list);
}

export function renderInfoError(host: HTMLElement, message: string): void {
  host.replaceChildren(h("h3", { text: "Image info" }), h("p", { class: "muted", text: message }));
}
