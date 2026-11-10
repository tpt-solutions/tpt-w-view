// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// App shell: wires the browser, viewer, edit panel, batch dialog and info panel together, and
// owns the three modes (browse, view, edit), keyboard shortcuts and the slideshow.

import { open as pickOpen, save as pickSaveFile } from "@tauri-apps/plugin-dialog";
import { Browser } from "./browser";
import { openBatchDialog } from "./batch-dialog";
import { EditPanel, type EditController } from "./edit-panel";
import { $, h } from "./dom";
import { errorText, api, onDirChanged } from "./ipc";
import { renderInfo, renderInfoError } from "./info-panel";
import { openSettingsDialog } from "./settings-dialog";
import { applyTheme, loadSettings, saveSettings, type Settings, type Theme } from "./settings";
import type { EntryDto, OpDto, SaveDto } from "./types";
import { Viewer } from "./viewer";

const IMAGE_EXTENSIONS = [
  "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "avif", "heic", "heif",
  "cr2", "cr3", "nef", "arw", "dng", "orf", "raf", "rw2", "pef", "srw",
];
const SLIDESHOW_MS = 4000;

type Mode = "browse" | "view" | "edit";

let settings: Settings = loadSettings();
applyTheme(settings.theme);

// ---- state ---------------------------------------------------------------------------------

let dir: string | null = null;
let entries: EntryDto[] = [];
let mode: Mode = "browse";
let viewIndex = 0;
let sessionId: number | null = null;
let dirty = false;
let viewToken = 0;
let slideshowTimer: number | null = null;

// ---- DOM -----------------------------------------------------------------------------------

const toolbar = $("toolbar");
const browseHost = $("browse");
const viewHost = $("view");
const sidebar = $("sidebar");
const statusText = $("status-text");
const zoomText = $("zoom-text");
const toasts = $("toasts");
const crumbs = $("crumbs");
const infoHost = h("div", { class: "info-host" });
sidebar.append(infoHost);

// ---- components ----------------------------------------------------------------------------

const viewer = new Viewer(viewHost, (percent) => {
  zoomText.textContent = percent ? `${percent}%` : "";
});

const browser = new Browser(browseHost, settings.thumbEdge, {
  onOpen: (index) => void openIndex(index),
  onSelectionChange: () => updateStatus(),
});

const editController: EditController = {
  apply: async (op: OpDto) => {
    const id = requireSession();
    await api.editApply(id, op);
    dirty = true;
    await refreshEditPreview();
  },
  undo: async () => {
    const id = requireSession();
    if (await api.editUndo(id)) {
      await refreshEditPreview();
    } else {
      notify("Nothing to undo.");
    }
  },
  redo: async () => {
    const id = requireSession();
    if (await api.editRedo(id)) {
      await refreshEditPreview();
    } else {
      notify("Nothing to redo.");
    }
  },
  save: async (target: SaveDto, format: string, quality: number) => {
    const id = requireSession();
    const written = await api.editSave(id, target, format, quality);
    dirty = false;
    return written;
  },
  pickSavePath: async (suggestedName: string) =>
    pickSaveFile({
      defaultPath: suggestedName,
      filters: [{ name: "Image", extensions: IMAGE_EXTENSIONS }],
    }),
  notify,
};

const editPanel = new EditPanel(sidebar, editController, settings);

// ---- helpers -------------------------------------------------------------------------------

function notify(message: string, isError = false): void {
  const toast = h("div", { class: isError ? "toast error" : "toast", role: "status", text: message });
  toasts.append(toast);
  window.setTimeout(() => toast.remove(), 4000);
}

function requireSession(): number {
  if (sessionId === null) throw new Error("No edit session is open.");
  return sessionId;
}

function currentEntry(): EntryDto | null {
  return entries[viewIndex] ?? null;
}

function setMode(next: Mode): void {
  mode = next;
  browseHost.hidden = next !== "browse";
  crumbs.hidden = next !== "browse" || dir === null;
  viewHost.hidden = next === "browse";
  sidebar.hidden = next === "browse" || sidebarCollapsed;
  infoHost.hidden = next !== "view";
  editPanel.root.hidden = next !== "edit";
  updateStatus();
}

let sidebarCollapsed = false;

function updateStatus(): void {
  const parts: string[] = [];
  if (dir) parts.push(dir);
  if (mode === "browse") {
    const sel = browser.selectedIndices().length;
    parts.push(`${entries.length} image(s)`);
    if (sel) parts.push(`${sel} selected`);
  } else {
    const entry = currentEntry();
    if (entry) parts.push(`${viewIndex + 1}/${entries.length} ${entry.name}`);
    if (mode === "edit") parts.push("EDITING");
  }
  if (slideshowTimer !== null) parts.push("slideshow");
  statusText.textContent = parts.join("  ·  ");
}

async function refreshEditPreview(): Promise<void> {
  const id = requireSession();
  const url = await api.editPreview(id, 0);
  await viewer.show(url);
  const size = viewer.imageSize;
  if (size) editPanel.setImageSize(size.width, size.height);
  updateStatus();
}

// ---- directory and image navigation --------------------------------------------------------

async function loadDirectory(path: string): Promise<void> {
  if (mode === "edit" && !(await leaveEdit())) return;
  dir = path;
  entries = await api.listDirectory(path);
  browser.setEntries(entries);
  await api.watchDirectory(path);
  setMode("browse");
  renderCrumbs(path);
  updateStatus();
}

/** Splits a path into its root ("C:\", "/" or "") and named parts, keeping the native separator. */
function splitPath(path: string): { sep: string; root: string; parts: string[] } {
  const sep = path.includes("\\") ? "\\" : "/";
  const drive = /^[A-Za-z]:/.test(path);
  const rest = drive ? path.slice(2) : path;
  const parts = rest.split(/[\\/]+/).filter((p) => p.length > 0);
  const root = drive ? `${path.slice(0, 2)}${sep}` : path.startsWith("/") ? "/" : "";
  return { sep, root, parts };
}

/** Path of the crumb at `index`; -1 is the root. */
function pathAt(path: string, index: number): string {
  const { sep, root, parts } = splitPath(path);
  if (index < 0) return root || parts[0] || path;
  return root + parts.slice(0, index + 1).join(sep);
}

function renderCrumbs(path: string): void {
  const { root, parts } = splitPath(path);
  const crumb = (label: string, target: string) =>
    h("button", {
      type: "button",
      class: "crumb",
      text: label,
      onclick: () => void loadDirectory(target).catch(reportError),
    });
  const items: Node[] = [crumb(root || parts[0] || path, pathAt(path, -1))];
  const start = root ? 0 : 1;
  for (let i = start; i < parts.length; i++) {
    items.push(h("span", { class: "crumb-sep", "aria-hidden": "true", text: "›" }));
    items.push(crumb(parts[i], pathAt(path, i)));
  }
  crumbs.replaceChildren(...items);
  crumbs.hidden = false;
}

/** Opens the parent folder of the current directory. */
async function goUp(): Promise<void> {
  if (!dir) return;
  const { parts } = splitPath(dir);
  if (parts.length === 0) return;
  await loadDirectory(pathAt(dir, parts.length - 2)).catch(reportError);
}

async function openIndex(index: number): Promise<void> {
  if (entries.length === 0) return;
  if (mode === "edit") return;
  viewIndex = Math.min(Math.max(0, index), entries.length - 1);
  setMode("view");
  const token = ++viewToken;
  const entry = entries[viewIndex];
  try {
    const url = await api.preview(entry.path, 0);
    if (token !== viewToken) return;
    await viewer.show(url);
    const info = await api.imageInfo(entry.path);
    if (token !== viewToken) return;
    renderInfo(infoHost, entry, info);
  } catch (err) {
    if (token !== viewToken) return;
    renderInfoError(infoHost, errorText(err));
    notify(errorText(err), true);
  }
  updateStatus();
}

function step(delta: number): void {
  if (entries.length === 0) return;
  const next = (viewIndex + delta + entries.length) % entries.length;
  void openIndex(next);
}

function exitViewToBrowse(): void {
  stopSlideshow();
  setMode("browse");
}

async function openFile(path: string): Promise<void> {
  const parent = path.replace(/[\\/][^\\/]*$/, "");
  if (parent !== dir) await loadDirectory(parent);
  const index = entries.findIndex((e) => e.path === path);
  await openIndex(index >= 0 ? index : 0);
}

// ---- edit mode -----------------------------------------------------------------------------

async function enterEdit(): Promise<void> {
  const entry = currentEntry();
  if (!entry || mode !== "view") return;
  stopSlideshow();
  try {
    sessionId = await api.editOpen(entry.path);
    dirty = false;
    setMode("edit");
    await refreshEditPreview();
  } catch (err) {
    notify(errorText(err), true);
  }
}

/** Closes the edit session. Returns false if the user kept editing instead of discarding. */
async function leaveEdit(): Promise<boolean> {
  if (mode !== "edit") return true;
  if (dirty && !window.confirm("Discard unsaved edits?")) return false;
  const id = sessionId;
  sessionId = null;
  dirty = false;
  if (id !== null) await api.editClose(id);
  setMode("view");
  await openIndex(viewIndex);
  return true;
}

// ---- slideshow, theme, fullscreen ----------------------------------------------------------

function toggleSlideshow(): void {
  if (slideshowTimer !== null) {
    stopSlideshow();
    return;
  }
  if (mode !== "view") return;
  slideshowTimer = window.setInterval(() => step(1), SLIDESHOW_MS);
  updateStatus();
}

function stopSlideshow(): void {
  if (slideshowTimer !== null) {
    window.clearInterval(slideshowTimer);
    slideshowTimer = null;
    updateStatus();
  }
}

function cycleTheme(): void {
  const order: Theme[] = ["system", "light", "dark"];
  const next = order[(order.indexOf(settings.theme) + 1) % order.length];
  settings = { ...settings, theme: next };
  applyTheme(next);
  saveSettings(settings);
  notify(`Theme: ${next}`);
}

async function toggleFullscreen(): Promise<void> {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await document.documentElement.requestFullscreen();
    }
  } catch {
    notify("Fullscreen is not available here.", true);
  }
}

function openSettings(): void {
  openSettingsDialog({
    settings,
    onChange: (next) => {
      settings = next;
    },
    notify,
  });
}

function openBatch(): void {
  const selected = browser.selectedIndices().map((i) => entries[i].path);
  const inputs =
    selected.length > 0 ? selected : entries.map((e) => e.path);
  if (inputs.length === 0) {
    notify("Open a folder first.", true);
    return;
  }
  openBatchDialog({
    inputs,
    settings,
    onSettingsChange: (next) => {
      settings = next;
    },
    pickFolder: async () => {
      const picked = await pickOpen({ directory: true, multiple: false });
      return typeof picked === "string" ? picked : null;
    },
    notify,
  });
}

// ---- toolbar -------------------------------------------------------------------------------

function tool(label: string, title: string, onClick: () => void): HTMLButtonElement {
  return h("button", { type: "button", title, onclick: onClick, text: label }) as HTMLButtonElement;
}

toolbar.append(
  tool("Open folder", "Open a folder (Ctrl+Shift+O)", async () => {
    const picked = await pickOpen({ directory: true, multiple: false });
    if (typeof picked === "string") await loadDirectory(picked).catch(reportError);
  }),
  tool("Open image", "Open an image (Ctrl+O)", async () => {
    const picked = await pickOpen({
      multiple: false,
      filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }],
    });
    if (typeof picked === "string") await openFile(picked).catch(reportError);
  }),
  tool("Batch…", "Batch convert (B)", openBatch),
  tool("Edit", "Edit the current image (E)", () => void enterEdit()),
  tool("Slideshow", "Start or stop the slideshow (S)", toggleSlideshow),
  tool("Info", "Show or hide the sidebar (I)", () => {
    sidebarCollapsed = !sidebarCollapsed;
    sidebar.hidden = mode === "browse" || sidebarCollapsed;
  }),
  tool("Fullscreen", "Toggle fullscreen (F)", () => void toggleFullscreen()),
  tool("Theme", "Cycle theme", cycleTheme),
  tool("Settings", "Settings and maintenance", openSettings),
);

function reportError(err: unknown): void {
  notify(errorText(err), true);
}

// ---- keyboard ------------------------------------------------------------------------------

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLSelectElement ||
    target instanceof HTMLTextAreaElement
  );
}

document.addEventListener("keydown", (e) => {
  if (isTyping(e.target)) return;
  const ctrl = e.ctrlKey || e.metaKey;

  if (ctrl && e.key.toLowerCase() === "o") {
    e.preventDefault();
    const open = e.shiftKey ? loadFolderShortcut : loadFileShortcut;
    void open().catch(reportError);
    return;
  }
  if (ctrl && e.key.toLowerCase() === "z" && mode === "edit") {
    e.preventDefault();
    void editController.undo().catch(reportError);
    return;
  }
  if (ctrl && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z")) && mode === "edit") {
    e.preventDefault();
    void editController.redo().catch(reportError);
    return;
  }
  if (ctrl && e.key.toLowerCase() === "a" && mode === "browse") {
    e.preventDefault();
    browser.selectAll();
    return;
  }
  if (ctrl) return;

  if (mode === "browse") {
    switch (e.key) {
      case "ArrowLeft": browser.moveFocus(-1); break;
      case "ArrowRight": browser.moveFocus(1); break;
      case "ArrowUp": browser.moveRow(-1); break;
      case "ArrowDown": browser.moveRow(1); break;
      case "Enter": {
        const focused = browser.selectedIndices()[0];
        if (focused !== undefined) void openIndex(focused);
        break;
      }
      case "b": case "B": openBatch(); break;
      case "Backspace": void goUp(); break;
      default: return;
    }
    e.preventDefault();
    return;
  }

  if (mode === "view") {
    switch (e.key) {
      case "Escape": exitViewToBrowse(); break;
      case "ArrowLeft": case "PageUp": case "Backspace": step(-1); break;
      case "ArrowRight": case "PageDown": case " ": step(1); break;
      case "+": case "=": viewer.zoomCentre(1.25); break;
      case "-": viewer.zoomCentre(0.8); break;
      case "0": viewer.fitToWindow(); break;
      case "1": viewer.actualSize(); break;
      case "f": case "F": void toggleFullscreen(); break;
      case "s": case "S": toggleSlideshow(); break;
      case "i": case "I": sidebarCollapsed = !sidebarCollapsed; sidebar.hidden = sidebarCollapsed; break;
      case "e": case "E": void enterEdit(); break;
      case "b": case "B": openBatch(); break;
      case "r": case "R":
        // Rotate is an edit: enter edit mode and apply one quarter turn.
        void enterEdit().then(() => editController.apply({ kind: "rotate_quarter_turns", turns: 1 }))
          .catch(reportError);
        break;
      default: return;
    }
    e.preventDefault();
    return;
  }

  // Edit mode.
  switch (e.key) {
    case "Escape": void leaveEdit().catch(reportError); break;
    case "f": case "F": void toggleFullscreen(); break;
    default: return;
  }
  e.preventDefault();
});

async function loadFileShortcut(): Promise<void> {
  const picked = await pickOpen({
    multiple: false,
    filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }],
  });
  if (typeof picked === "string") await openFile(picked).catch(reportError);
}

async function loadFolderShortcut(): Promise<void> {
  const picked = await pickOpen({ directory: true, multiple: false });
  if (typeof picked === "string") await loadDirectory(picked).catch(reportError);
}

// ---- startup -------------------------------------------------------------------------------

onDirChanged(async (change) => {
  if (change.dir !== dir || mode === "edit") return;
  try {
    entries = await api.listDirectory(change.dir);
    browser.setEntries(entries);
    updateStatus();
  } catch (err) {
    reportError(err);
  }
});

window.addEventListener("beforeunload", () => {
  void api.watchStop();
});

async function start(): Promise<void> {
  setMode("browse");
  const initial = await api.initialPath();
  if (initial) {
    try {
      const isFile = /\.[A-Za-z0-9]+$/.test(initial);
      if (isFile) {
        await openFile(initial);
      } else {
        await loadDirectory(initial);
      }
    } catch (err) {
      reportError(err);
    }
  } else {
    statusText.textContent = "Open a folder or an image to begin.";
  }
}

void start();
