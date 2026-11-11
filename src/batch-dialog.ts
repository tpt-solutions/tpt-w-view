// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Batch convert/rename dialog. Drives the same core engine as the CLI, with live progress from
// the "batch-progress" event and a cancel button that stops the job between files.

import { api, errorText, onBatchProgress } from "./ipc";
import { h } from "./dom";
import { saveSettings, type Settings } from "./settings";
import type { BatchDto, ReportDto, ResizeDto } from "./types";
import type { UnlistenFn } from "@tauri-apps/api/event";

export interface BatchDialogOptions {
  inputs: string[];
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
  pickFolder: () => Promise<string | null>;
  notify: (message: string, isError?: boolean) => void;
}

const FORMATS = ["jpg", "png", "webp", "tiff", "bmp", "avif"];

export function openBatchDialog(opts: BatchDialogOptions): void {
  const { settings } = opts;
  let outputDir = settings.defaultOutputDir;
  let jobId = 0;
  let unlisten: UnlistenFn | null = null;

  const format = h("select", {}, ...FORMATS.map((f) => h("option", { value: f }, f))) as HTMLSelectElement;
  format.value = settings.defaultFormat;
  const quality = h("input", { type: "number", min: 1, max: 100, value: settings.quality, class: "num" }) as HTMLInputElement;
  const resizeMode = h(
    "select",
    {},
    h("option", { value: "none" }, "Keep size"),
    h("option", { value: "max_edge" }, "Fit within box"),
    h("option", { value: "percent" }, "Scale percent"),
    h("option", { value: "exact" }, "Exact size"),
  ) as HTMLSelectElement;
  const resizeValue = h("input", { type: "number", min: 1, value: 1024, class: "num" }) as HTMLInputElement;
  const resizeHeight = h("input", { type: "number", min: 1, value: 768, class: "num", hidden: true }) as HTMLInputElement;
  const rename = h("input", { type: "text", value: settings.renamePresets[0] ?? "{name}", class: "wide" }) as HTMLInputElement;
  const presets = h(
    "select",
    {},
    h("option", { value: "" }, "Presets…"),
    ...settings.renamePresets.map((p) => h("option", { value: p }, p)),
  ) as HTMLSelectElement;
  const counter = h("input", { type: "number", min: 0, value: 1, class: "num" }) as HTMLInputElement;
  const preserve = h("input", { type: "checkbox" }) as HTMLInputElement;
  const overwrite = h("input", { type: "checkbox" }) as HTMLInputElement;
  const outputLabel = h("span", { class: "path", text: outputDir || "No folder chosen" });
  const progress = h("progress", { max: 1, value: 0, hidden: true }) as HTMLProgressElement;
  const status = h("div", { class: "batch-status", role: "status" });
  const results = h("ul", { class: "batch-results" });
  const runButton = h("button", { type: "button", class: "primary", text: "Run" }) as HTMLButtonElement;
  const cancelButton = h("button", { type: "button", text: "Cancel job", disabled: true }) as HTMLButtonElement;
  const closeButton = h("button", { type: "button", text: "Close" }) as HTMLButtonElement;

  const syncResizeFields = () => {
    const mode = resizeMode.value;
    resizeValue.hidden = mode === "none";
    resizeHeight.hidden = mode !== "exact";
    resizeValue.placeholder = mode === "percent" ? "%" : "px";
  };
  resizeMode.addEventListener("change", syncResizeFields);
  syncResizeFields();

  presets.addEventListener("change", () => {
    if (presets.value) rename.value = presets.value;
  });

  const chooseFolder = h("button", {
    type: "button",
    text: "Choose output folder…",
    onclick: async () => {
      const picked = await opts.pickFolder();
      if (picked) {
        outputDir = picked;
        outputLabel.textContent = picked;
      }
    },
  });

  const buildDto = (): BatchDto => {
    const resize: ResizeDto = (() => {
      switch (resizeMode.value) {
        case "max_edge":
          return { mode: "max_edge", px: Number(resizeValue.value) };
        case "percent":
          return { mode: "percent", percent: Number(resizeValue.value) };
        case "exact":
          return { mode: "exact", width: Number(resizeValue.value), height: Number(resizeHeight.value) };
        default:
          return { mode: "none" };
      }
    })();
    return {
      inputs: opts.inputs,
      output_dir: outputDir,
      format: format.value,
      quality: Number(quality.value),
      resize,
      preserve_exif: preserve.checked,
      rename: rename.value,
      start_counter: Number(counter.value),
      overwrite: overwrite.checked,
    };
  };

  const finish = () => {
    runButton.disabled = false;
    cancelButton.disabled = true;
    unlisten?.();
    unlisten = null;
  };

  runButton.addEventListener("click", async () => {
    if (!outputDir) {
      opts.notify("Choose an output folder first.", true);
      return;
    }
    if (opts.inputs.length === 0) {
      opts.notify("Select images first.", true);
      return;
    }
    // Persist the choices that are worth remembering between sessions.
    const next: Settings = {
      ...settings,
      defaultFormat: format.value,
      quality: Number(quality.value),
    };
    opts.onSettingsChange(next);
    saveSettings(next);

    jobId = Date.now();
    results.replaceChildren();
    progress.hidden = false;
    progress.value = 0;
    status.textContent = "Starting…";
    runButton.disabled = true;
    cancelButton.disabled = false;

    unlisten = await onBatchProgress((p) => {
      if (p.job_id !== jobId) return;
      progress.max = p.total || 1;
      progress.value = p.done;
      status.textContent = `${p.done} / ${p.total}`;
    });

    try {
      const reports: ReportDto[] = await api.batchRun(jobId, buildDto());
      renderResults(results, reports);
      const failed = reports.filter((r) => r.status === "failed").length;
      const converted = reports.filter((r) => r.status === "converted").length;
      status.textContent = `Done: ${converted} converted, ${failed} failed.`;
      opts.notify(`Batch finished: ${converted} converted${failed ? `, ${failed} failed` : ""}.`, failed > 0);
    } catch (err) {
      status.textContent = errorText(err);
      opts.notify(errorText(err), true);
    } finally {
      finish();
    }
  });

  cancelButton.addEventListener("click", async () => {
    await api.batchCancel(jobId);
    status.textContent = "Cancelling after the current file…";
  });

  const close = () => {
    unlisten?.();
    overlay.remove();
  };
  closeButton.addEventListener("click", close);

  const panel = h(
    "div",
    { class: "dialog", role: "dialog", "aria-modal": "true", "aria-label": "Batch convert" },
    h("h2", { text: `Batch convert — ${opts.inputs.length} file(s)` }),
    h("div", { class: "form-grid" },
      h("label", {}, "Output folder"), h("div", {}, chooseFolder, " ", outputLabel),
      h("label", {}, "Format"), format,
      h("label", {}, "JPEG quality"), quality,
      h("label", {}, "Resize"), h("div", {}, resizeMode, " ", resizeValue, " ", resizeHeight),
      h("label", {}, "Rename"), h("div", {}, rename, " ", presets),
      h("label", {}, "Counter start"), counter,
      h("label", {}, "Keep EXIF"), h("label", {}, preserve, " JPEG → JPEG only"),
      h("label", {}, "Overwrite"), h("label", {}, overwrite, " Replace existing outputs"),
    ),
    progress,
    status,
    results,
    h("div", { class: "dialog-actions" }, runButton, cancelButton, closeButton),
  );
  const overlay = h("div", { class: "overlay", onclick: (e: Event) => {
    if (e.target === overlay) close();
  } }, panel);
  document.body.append(overlay);
  rename.focus();
}

function renderResults(list: HTMLElement, reports: ReportDto[]): void {
  for (const r of reports) {
    const name = r.input.split(/[\\/]/).pop() ?? r.input;
    const detail = r.output ?? r.message ?? "";
    list.append(
      h("li", { class: `result ${r.status}` },
        h("span", { class: "result-status", text: r.status }),
        h("span", { class: "result-name", text: name, title: r.input }),
        h("span", { class: "result-detail", text: detail }),
      ),
    );
  }
}
