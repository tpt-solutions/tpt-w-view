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
}

const KEY = "tpt-view.settings";

export const DEFAULT_SETTINGS: Settings = {
  defaultFormat: "jpg",
  quality: 90,
  thumbEdge: 256,
  renamePresets: ["{name}", "img_{counter:03}", "{date}_{name}"],
  theme: "system",
};

export function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<Settings>;
    return { ...DEFAULT_SETTINGS, ...parsed };
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
