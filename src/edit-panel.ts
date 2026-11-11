// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Editing controls for the open edit session. Each control maps to one OpDto. Sliders apply
// their operation when released and then return to neutral, so every adjustment is a single
// undoable step rather than a stream of tiny ones.

import { h, type Child } from "./dom";
import type { FilterName, OpDto, SaveDto } from "./types";
import type { Settings } from "./settings";
import { CROP_ASPECTS, type CropRect } from "./crop-geometry";

export interface EditController {
  apply(op: OpDto): Promise<void>;
  undo(): Promise<void>;
  redo(): Promise<void>;
  save(target: SaveDto, format: string, quality: number): Promise<string>;
  /** Native "save as" dialog; resolves to null when cancelled. */
  pickSavePath(suggestedName: string): Promise<string | null>;
  notify(message: string, isError?: boolean): void;
}

const FORMATS = ["jpg", "png", "webp", "tiff", "bmp", "avif"];
const FILTERS: FilterName[] = ["bilinear", "bicubic", "lanczos3", "nearest"];

export class EditPanel {
  readonly root: HTMLElement;
  private readonly widthInput: HTMLInputElement;
  private readonly heightInput: HTMLInputElement;
  private readonly cropInputs: Record<"x" | "y" | "w" | "h", HTMLInputElement>;
  private readonly lockAspect: HTMLInputElement;
  private readonly format: HTMLSelectElement;
  private readonly quality: HTMLInputElement;
  private readonly cropAspectSelect: HTMLSelectElement;
  private aspect = 1;
  /** Called when the crop aspect ratio dropdown changes. */
  onCropAspect: ((ratio: number | null) => void) | null = null;
  private readonly controls: HTMLButtonElement[] = [];

  constructor(
    host: HTMLElement,
    private readonly controller: EditController,
    settings: Settings,
  ) {
    this.widthInput = h("input", { type: "number", min: 1, class: "num" }) as HTMLInputElement;
    this.heightInput = h("input", { type: "number", min: 1, class: "num" }) as HTMLInputElement;
    this.lockAspect = h("input", { type: "checkbox", checked: true }) as HTMLInputElement;
    this.cropInputs = {
      x: h("input", { type: "number", min: 0, value: 0, class: "num" }) as HTMLInputElement,
      y: h("input", { type: "number", min: 0, value: 0, class: "num" }) as HTMLInputElement,
      w: h("input", { type: "number", min: 1, class: "num" }) as HTMLInputElement,
      h: h("input", { type: "number", min: 1, class: "num" }) as HTMLInputElement,
    };
    this.cropAspectSelect = h(
      "select",
      { "aria-label": "Crop aspect ratio", onchange: () => this.onCropAspect?.(CROP_ASPECTS[Number(this.cropAspectSelect.value)].ratio) },
      ...CROP_ASPECTS.map((a, i) => h("option", { value: String(i) }, a.label)),
    ) as HTMLSelectElement;
    this.format = h("select", {}, ...FORMATS.map((f) => h("option", { value: f }, f))) as HTMLSelectElement;
    this.format.value = settings.defaultFormat;
    this.quality = h("input", {
      type: "number",
      min: 1,
      max: 100,
      value: settings.quality,
      class: "num",
    }) as HTMLInputElement;

    this.root = h(
      "div",
      { class: "edit-panel" },
      this.section("Transform",
        this.button("Rotate left", () => this.run({ kind: "rotate_quarter_turns", turns: 3 })),
        this.button("Rotate right", () => this.run({ kind: "rotate_quarter_turns", turns: 1 })),
        this.button("Flip H", () => this.run({ kind: "flip_horizontal" })),
        this.button("Flip V", () => this.run({ kind: "flip_vertical" })),
      ),
      this.arbitraryRotate(),
      this.resizeSection(),
      this.cropSection(),
      this.section("Colour",
        this.slider("Brightness", -255, 255, 1, 0, (v) => ({ kind: "brightness", delta: v })),
        this.slider("Contrast", 0.5, 2, 0.05, 1, (v) => ({ kind: "contrast", factor: v })),
        this.slider("Saturation", 0, 2, 0.05, 1, (v) => ({ kind: "saturation", factor: v })),
        this.levelsSection(),
      ),
      this.section("History",
        this.button("Undo", () => this.history("undo")),
        this.button("Redo", () => this.history("redo")),
      ),
      this.saveSection(),
    ) as HTMLElement;
    host.append(this.root);
  }

  /** Sets the size used to seed the resize and crop fields. */
  setImageSize(width: number, height: number): void {
    this.aspect = width / Math.max(1, height);
    this.widthInput.value = String(width);
    this.heightInput.value = String(height);
    this.cropInputs.x.value = "0";
    this.cropInputs.y.value = "0";
    this.cropInputs.w.value = String(width);
    this.cropInputs.h.value = String(height);
  }

  /** Fills the crop fields from a rectangle selected on the image. */
  setCropRect(rect: CropRect): void {
    this.cropInputs.x.value = String(rect.x);
    this.cropInputs.y.value = String(rect.y);
    this.cropInputs.w.value = String(rect.w);
    this.cropInputs.h.value = String(rect.h);
  }

  setEnabled(enabled: boolean): void {
    for (const b of this.controls) b.disabled = !enabled;
  }

  private section(title: string, ...children: Child[]): HTMLElement {
    return h("section", { class: "panel-section" }, h("h3", { text: title }), ...children);
  }

  private button(label: string, onClick: () => void): HTMLButtonElement {
    const b = h("button", { type: "button", onclick: onClick, text: label }) as HTMLButtonElement;
    this.controls.push(b);
    return b;
  }

  private async run(op: OpDto): Promise<void> {
    try {
      await this.controller.apply(op);
    } catch (err) {
      this.controller.notify(String(err), true);
    }
  }

  private async history(which: "undo" | "redo"): Promise<void> {
    try {
      if (which === "undo") await this.controller.undo();
      else await this.controller.redo();
    } catch (err) {
      this.controller.notify(String(err), true);
    }
  }

  private arbitraryRotate(): HTMLElement {
    const degrees = h("input", { type: "number", value: 0, step: 0.5, class: "num" }) as HTMLInputElement;
    const expand = h("input", { type: "checkbox", checked: true }) as HTMLInputElement;
    const filter = h(
      "select",
      {},
      ...FILTERS.map((f) => h("option", { value: f }, f)),
    ) as HTMLSelectElement;
    return this.section(
      "Rotate by angle",
      h("label", {}, "Degrees ", degrees),
      h("label", {}, expand, " Expand canvas"),
      h("label", {}, "Filter ", filter),
      this.button("Rotate", () =>
        this.run({
          kind: "rotate_arbitrary",
          degrees: Number(degrees.value),
          expand: expand.checked,
          filter: filter.value as FilterName,
        }),
      ),
    );
  }

  private resizeSection(): HTMLElement {
    const filter = h(
      "select",
      {},
      ...FILTERS.map((f) => h("option", { value: f }, f)),
    ) as HTMLSelectElement;
    this.widthInput.addEventListener("change", () => {
      if (this.lockAspect.checked) {
        this.heightInput.value = String(Math.max(1, Math.round(Number(this.widthInput.value) / this.aspect)));
      }
    });
    this.heightInput.addEventListener("change", () => {
      if (this.lockAspect.checked) {
        this.widthInput.value = String(Math.max(1, Math.round(Number(this.heightInput.value) * this.aspect)));
      }
    });
    return this.section(
      "Resize",
      h("label", {}, "W ", this.widthInput),
      h("label", {}, "H ", this.heightInput),
      h("label", {}, this.lockAspect, " Keep aspect"),
      h("label", {}, "Filter ", filter),
      this.button("Resize", () =>
        this.run({
          kind: "resize",
          width: Number(this.widthInput.value),
          height: Number(this.heightInput.value),
          filter: filter.value as FilterName,
        }),
      ),
    );
  }

  private cropSection(): HTMLElement {
    const { x, y, w, h: hh } = this.cropInputs;
    return this.section(
      "Crop",
      h("label", {}, "X ", x, " Y ", y),
      h("label", {}, "W ", w, " H ", hh),
      h("label", {}, "Aspect ", this.cropAspectSelect),
      h("p", { class: "muted", text: "Drag on the image to select a crop area." }),
      this.button("Crop", () =>
        this.run({
          kind: "crop",
          x: Number(x.value),
          y: Number(y.value),
          width: Number(w.value),
          height: Number(hh.value),
        }),
      ),
    );
  }

  private levelsSection(): HTMLElement {
    const black = h("input", { type: "number", min: 0, max: 254, value: 0, class: "num" }) as HTMLInputElement;
    const white = h("input", { type: "number", min: 1, max: 255, value: 255, class: "num" }) as HTMLInputElement;
    const gamma = h("input", { type: "number", min: 0.1, max: 4, step: 0.05, value: 1, class: "num" }) as HTMLInputElement;
    return this.section(
      "Levels",
      h("label", {}, "Black ", black, " White ", white),
      h("label", {}, "Gamma ", gamma),
      this.button("Apply levels", () =>
        this.run({
          kind: "levels",
          black: Number(black.value),
          white: Number(white.value),
          gamma: Number(gamma.value),
        }),
      ),
    );
  }

  private slider(
    label: string,
    min: number,
    max: number,
    step: number,
    neutral: number,
    toOp: (value: number) => OpDto,
  ): HTMLElement {
    const input = h("input", { type: "range", min, max, step, value: neutral }) as HTMLInputElement;
    const readout = h("span", { class: "readout", text: String(neutral) });
    input.addEventListener("input", () => {
      readout.textContent = input.value;
    });
    input.addEventListener("change", async () => {
      const value = Number(input.value);
      // Reset to neutral before awaiting so the control never shows a stale adjustment.
      input.value = String(neutral);
      readout.textContent = String(neutral);
      if (value === neutral) return;
      await this.run(toOp(value));
    });
    return h("label", { class: "slider" }, `${label} `, input, readout);
  }

  private saveSection(): HTMLElement {
    const quality = this.quality;
    const format = this.format;
    return this.section(
      "Save",
      h("label", {}, "Format ", format, " Quality ", quality),
      this.button("Save as…", async () => {
        try {
          const path = await this.controller.pickSavePath(`edited.${format.value}`);
          if (!path) return;
          const written = await this.controller.save(
            { mode: "save_as", path },
            format.value,
            Number(quality.value),
          );
          this.controller.notify(`Saved ${written}`);
        } catch (err) {
          this.controller.notify(String(err), true);
        }
      }),
      this.button("Overwrite original…", async () => {
        if (!window.confirm("Replace the original file with the edited image? This cannot be undone.")) {
          return;
        }
        try {
          const written = await this.controller.save(
            { mode: "overwrite", confirmed: true },
            format.value,
            Number(quality.value),
          );
          this.controller.notify(`Overwrote ${written}`);
        } catch (err) {
          this.controller.notify(String(err), true);
        }
      }),
    );
  }
}
