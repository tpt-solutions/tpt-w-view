// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Folder tree for the browse mode. Starts at the drive or filesystem root that holds the open
// folder, expands the chain down to it, and loads each folder's children on first expansion.
// A click on a name opens that folder; a click on the arrow only expands or collapses it.

import { api } from "./ipc";
import { h } from "./dom";

export interface FolderTreeOptions {
  onOpen: (path: string) => void;
}

/** Children of one folder: `null` means the listing failed (for example, no permission). */
type Listing = string[] | null;

/** Root of an absolute path: "C:\" for Windows drives, "/" for Unix, otherwise the path itself. */
export function rootOf(path: string): string {
  const drive = /^([A-Za-z]:)[\\/]?/.exec(path);
  if (drive) return `${drive[1]}${path.includes("/") && !path.includes("\\") ? "/" : "\\"}`;
  if (path.startsWith("/")) return "/";
  return path;
}

/** Joins a child name onto a folder path without doubling the separator. */
export function joinPath(base: string, name: string): string {
  const sep = base.includes("\\") ? "\\" : "/";
  return base.endsWith("\\") || base.endsWith("/") ? `${base}${name}` : `${base}${sep}${name}`;
}

/** The folders from the root down to `path`, inclusive. */
export function chainTo(path: string): string[] {
  const root = rootOf(path);
  const rest = path.slice(root.length);
  const parts = rest.split(/[\\/]+/).filter((p) => p.length > 0);
  const chain = [root];
  for (const part of parts) {
    chain.push(joinPath(chain[chain.length - 1], part));
  }
  return chain;
}

export class FolderTree {
  private readonly listings = new Map<string, Listing>();
  private readonly pending = new Set<string>();
  private readonly expanded = new Set<string>();
  private readonly list: HTMLElement;
  private current: string | null = null;
  private roots: string[] = [];

  constructor(
    readonly root: HTMLElement,
    private readonly opts: FolderTreeOptions,
  ) {
    this.list = h("ul", { class: "tree", role: "tree", "aria-label": "Folders" });
    root.replaceChildren(this.list);
  }

  /** Shows the tree for `path`, expanding the chain down to it. */
  async reveal(path: string): Promise<void> {
    this.current = path;
    const chain = chainTo(path);
    this.roots = [chain[0]];
    // Re-read the chain so folders created since the last visit appear.
    for (const folder of chain) {
      this.listings.delete(folder);
      this.expanded.add(folder);
    }
    this.render();
    for (const folder of chain) {
      await this.load(folder);
      this.render();
    }
  }

  private async load(folder: string): Promise<void> {
    if (this.listings.has(folder) || this.pending.has(folder)) return;
    this.pending.add(folder);
    this.render();
    try {
      this.listings.set(folder, await api.listSubdirectories(folder));
    } catch {
      // Unreadable folders (permissions, removed drives) show as "Not available" in the tree.
      this.listings.set(folder, null);
    } finally {
      this.pending.delete(folder);
    }
  }

  private async toggle(folder: string): Promise<void> {
    if (this.expanded.has(folder)) {
      this.expanded.delete(folder);
      this.render();
      return;
    }
    this.expanded.add(folder);
    await this.load(folder);
    this.render();
  }

  private render(): void {
    this.list.replaceChildren(...this.roots.map((r) => this.node(r, 1)));
  }

  private node(folder: string, depth: number): HTMLElement {
    const isOpen = this.expanded.has(folder);
    const label = folder.replace(/[\\/]+$/, "") || folder;
    const name = label.split(/[\\/]/).filter((p) => p.length > 0).pop() ?? folder;
    const isCurrent = folder === this.current;
    const children: HTMLElement[] = [];

    if (isOpen) {
      if (this.pending.has(folder)) {
        children.push(h("li", { class: "tree-status", role: "none", text: "Loading…" }));
      } else {
        const listing = this.listings.get(folder);
        if (listing === null) {
          children.push(h("li", { class: "tree-status", role: "none", text: "Not available" }));
        } else if (listing) {
          for (const child of listing) children.push(this.node(child, depth + 1));
        }
      }
    }

    return h(
      "li",
      { role: "none" },
      h(
        "div",
        { class: "tree-row", style: `padding-left: ${(depth - 1) * 14}px` },
        h("button", {
          type: "button",
          class: "tree-toggle",
          text: isOpen ? "▾" : "▸",
          "aria-label": `${isOpen ? "Collapse" : "Expand"} ${name}`,
          "aria-expanded": String(isOpen),
          onclick: () => void this.toggle(folder),
        }),
        h("button", {
          type: "button",
          class: "tree-name",
          text: name,
          title: folder,
          role: "treeitem",
          "aria-current": isCurrent ? "true" : null,
          onclick: () => this.opts.onOpen(folder),
        }),
      ),
      children.length > 0 ? h("ul", { class: "tree", role: "group" }, ...children) : null,
    );
  }
}
