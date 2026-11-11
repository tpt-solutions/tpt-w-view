// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BatchDto,
  BatchProgress,
  DirChanges,
  EntryDto,
  InfoDto,
  OpDto,
  ReportDto,
  SaveDto,
} from "./types";

// Typed wrappers around the Rust commands. Top-level argument names are camelCase (Tauri maps
// them to snake_case); nested DTO objects keep the snake_case keys from types.ts.
export const api = {
  initialPath: () => invoke<string | null>("initial_path"),
  listDirectory: (dir: string) => invoke<EntryDto[]>("list_directory", { dir }),
  listSubdirectories: (dir: string) => invoke<string[]>("list_subdirectories", { dir }),
  thumbnail: (path: string, maxEdge: number) =>
    invoke<string>("thumbnail", { path, maxEdge }),
  imageInfo: (path: string) => invoke<InfoDto>("image_info", { path }),
  preview: (path: string, maxEdge: number) => invoke<string>("preview", { path, maxEdge }),

  editOpen: (path: string) => invoke<number>("edit_open", { path }),
  editApply: (id: number, op: OpDto) => invoke<void>("edit_apply", { id, op }),
  editUndo: (id: number) => invoke<boolean>("edit_undo", { id }),
  editRedo: (id: number) => invoke<boolean>("edit_redo", { id }),
  editPreview: (id: number, maxEdge: number) =>
    invoke<string>("edit_preview", { id, maxEdge }),
  editSave: (id: number, target: SaveDto, format: string, quality: number) =>
    invoke<string>("edit_save", { id, target, format, quality }),
  editClose: (id: number) => invoke<void>("edit_close", { id }),

  batchRun: (jobId: number, job: BatchDto) =>
    invoke<ReportDto[]>("batch_run", { jobId, job }),
  batchCancel: (jobId: number) => invoke<boolean>("batch_cancel", { jobId }),

  watchDirectory: (dir: string) => invoke<void>("watch_directory", { dir }),
  watchStop: () => invoke<void>("watch_stop"),

  crashLoggingEnabled: () => invoke<boolean>("crash_logging_enabled"),
  setCrashLogging: (enabled: boolean) => invoke<void>("set_crash_logging", { enabled }),
  clearThumbnailCache: () => invoke<number>("clear_thumbnail_cache"),
};

export function onBatchProgress(cb: (p: BatchProgress) => void): Promise<UnlistenFn> {
  return listen<BatchProgress>("batch-progress", (e) => cb(e.payload));
}

export function onDirChanged(cb: (c: DirChanges) => void): Promise<UnlistenFn> {
  return listen<DirChanges>("dir-changed", (e) => cb(e.payload));
}

/** Human-readable message from an invoke rejection (strings from Rust, or Error objects). */
export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "unexpected error";
}
