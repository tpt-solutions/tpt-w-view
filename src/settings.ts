// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Per-viewer preferences. Stored in localStorage, which can be unavailable (private windows,
// blocked storage), so every access is guarded and the defaults always work.

export type Theme = "system" | "light" | "dark";

export interface Settings {
  defaultFormat: string;
  quality: number;
  thumbEdge: number;
  renamePresets: string[];
  theme: Theme;
  /** Pre-filled output folder for batch jobs; empty means "ask each time". */
  defaultOutputDir: string;
  /** Seconds each image stays on screen during a slideshow, 1..=60. */
  slideshowSeconds: number;
}

const KEY = "tpt-view.settings";

export const DEFAULT_SETTINGS: Settings = {
  defaultFormat: "jpg",
  quality: 90,
  thumbEdge: 256,
  renamePresets: ["{name}", "img_{counter:03}", "{date}_{name}"],
  theme: "system",
  defaultOutputDir: "",
  slideshowSeconds: 4,
};

export function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<Settings>;
    const merged = { ...DEFAULT_SETTINGS, ...parsed };
    // Stored values can be hand-edited; keep the slideshow timer in its supported range.
    const seconds = Number(merged.slideshowSeconds);
    merged.slideshowSeconds = Number.isFinite(seconds)
      ? Math.min(60, Math.max(1, Math.round(seconds)))
      : DEFAULT_SETTINGS.slideshowSeconds;
    return merged;
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings: Settings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    // Storage unavailable: settings simply do not persist this session.
  }
}

/** Applies the theme by setting `data-theme` on the root element. */
export function applyTheme(theme: Theme): void {
  if (theme === "system") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", theme);
  }
}
