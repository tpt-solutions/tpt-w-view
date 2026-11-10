// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Settings dialog: output defaults, rename presets, theme, opt-in local crash logging, and
// thumbnail cache maintenance. Changes are saved when the user presses Save.

import { h } from "./dom";
import { api, errorText } from "./ipc";
import { applyTheme, DEFAULT_SETTINGS, saveSettings, type Settings, type Theme } from "./settings";

export interface SettingsDialogOptions {
  settings: Settings;
  onChange: (settings: Settings) => void;
  notify: (message: string, isError?: boolean) => void;
}

const FORMATS = ["jpg", "png", "webp", "tiff", "bmp", "avif"];
const THUMB_SIZES = [128, 192, 256, 384];

export function openSettingsDialog(opts: SettingsDialogOptions): void {
  const current = opts.settings;

  const format = h("select", {}, ...FORMATS.map((f) => h("option", { value: f }, f))) as HTMLSelectElement;
  format.value = current.defaultFormat;
  const quality = h("input", { type: "number", min: 1, max: 100, value: current.quality, class: "num" }) as HTMLInputElement;
  const thumbEdge = h(
    "select",
    {},
    ...THUMB_SIZES.map((px) => h("option", { value: px }, `${px} px`)),
  ) as HTMLSelectElement;
  thumbEdge.value = String(current.thumbEdge);
  const presets = h("textarea", { rows: 4, class: "wide" }) as HTMLTextAreaElement;
  presets.value = current.renamePresets.join("\n");
  const theme = h(
    "select",
    {},
    h("option", { value: "system" }, "Follow system"),
    h("option", { value: "light" }, "Light"),
    h("option", { value: "dark" }, "Dark"),
  ) as HTMLSelectElement;
  theme.value = current.theme;

  const crash = h("input", { type: "checkbox" }) as HTMLInputElement;
  const crashNote = h("p", { class: "muted", text: "Off by default. When on, a crash writes a report to a local file. Nothing is sent anywhere." });
  void api
    .crashLoggingEnabled()
    .then((on) => (crash.checked = on))
    .catch(() => (crash.disabled = true));

  const cacheStatus = h("span", { class: "muted", role: "status" });
  const clearCache = h("button", {
    type: "button",
    text: "Clear thumbnail cache",
    onclick: async () => {
      try {
        const removed = await api.clearThumbnailCache();
        cacheStatus.textContent = `Removed ${removed} cached thumbnail(s).`;
      } catch (err) {
        opts.notify(errorText(err), true);
      }
    },
  });

  const save = h("button", { type: "button", class: "primary", text: "Save" }) as HTMLButtonElement;
  const cancel = h("button", { type: "button", text: "Cancel" }) as HTMLButtonElement;
  const reset = h("button", { type: "button", text: "Reset defaults" }) as HTMLButtonElement;

  const close = () => overlay.remove();
  const overlay: HTMLElement = h(
    "div",
    {
      class: "overlay",
      onclick: (e: Event) => {
        if (e.target === overlay) close();
      },
    },
    h(
      "div",
      { class: "dialog", role: "dialog", "aria-modal": "true", "aria-label": "Settings" },
      h("h2", { text: "Settings" }),
      h(
        "div",
        { class: "form-grid" },
        h("label", {}, "Default output format"), format,
        h("label", {}, "Default JPEG quality"), quality,
        h("label", {}, "Thumbnail size"), thumbEdge,
        h("label", {}, "Rename presets (one per line)"), presets,
        h("label", {}, "Theme"), theme,
        h("label", {}, "Crash logging"), h("div", {}, h("label", {}, crash, " Keep local crash reports"), crashNote),
        h("label", {}, "Thumbnails"), h("div", {}, clearCache, " ", cacheStatus),
      ),
      h("div", { class: "dialog-actions" }, reset, cancel, save),
    ),
  );

  reset.addEventListener("click", () => {
    format.value = DEFAULT_SETTINGS.defaultFormat;
    quality.value = String(DEFAULT_SETTINGS.quality);
    thumbEdge.value = String(DEFAULT_SETTINGS.thumbEdge);
    presets.value = DEFAULT_SETTINGS.renamePresets.join("\n");
    theme.value = DEFAULT_SETTINGS.theme;
  });
  cancel.addEventListener("click", close);

  save.addEventListener("click", async () => {
    const renamePresets = presets.value
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    if (renamePresets.length === 0) {
      opts.notify("Keep at least one rename preset.", true);
      return;
    }
    const q = Number(quality.value);
    if (!Number.isInteger(q) || q < 1 || q > 100) {
      opts.notify("JPEG quality must be a whole number from 1 to 100.", true);
      return;
    }
    const next: Settings = {
      defaultFormat: format.value,
      quality: q,
      thumbEdge: Number(thumbEdge.value),
      renamePresets,
      theme: theme.value as Theme,
    };
    try {
      await api.setCrashLogging(crash.checked);
    } catch (err) {
      opts.notify(errorText(err), true);
    }
    saveSettings(next);
    applyTheme(next.theme);
    opts.onChange(next);
    opts.notify("Settings saved.");
    close();
  });

  document.body.append(overlay);
  format.focus();
}
