// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

// Wire types mirroring src-tauri/src/ipc.rs. Keep these in sync when the Rust DTOs change.

export interface EntryDto {
  path: string;
  name: string;
  size: number;
  modified_ms: number;
}

export interface MetadataDto {
  orientation: number;
  make: string | null;
  model: string | null;
  date_time_original: string | null;
  /** [latitude, longitude] in signed decimal degrees. */
  gps: [number, number] | null;
  title: string | null;
  description: string | null;
  creator: string | null;
  copyright: string | null;
  keywords: string[];
}

export interface InfoDto {
  width: number;
  height: number;
  format: string;
  metadata: MetadataDto;
}

export type FilterName = "nearest" | "bilinear" | "bicubic" | "lanczos3";

export type OpDto =
  | { kind: "crop"; x: number; y: number; width: number; height: number }
  | { kind: "rotate_quarter_turns"; turns: number }
  | { kind: "rotate_arbitrary"; degrees: number; expand: boolean; filter: FilterName }
  | { kind: "flip_horizontal" }
  | { kind: "flip_vertical" }
  | { kind: "resize"; width: number; height: number; filter: FilterName }
  | { kind: "brightness"; delta: number }
  | { kind: "contrast"; factor: number }
  | { kind: "saturation"; factor: number }
  | { kind: "levels"; black: number; white: number; gamma: number }
  | { kind: "curve"; points: [number, number][] };

export type SaveDto =
  | { mode: "save_as"; path: string }
  | { mode: "overwrite"; confirmed: boolean };

export type ResizeDto =
  | { mode: "none" }
  | { mode: "max_edge"; px: number }
  | { mode: "percent"; percent: number }
  | { mode: "exact"; width: number; height: number };

export interface BatchDto {
  inputs: string[];
  output_dir: string;
  format: string;
  quality: number;
  resize: ResizeDto;
  preserve_exif: boolean;
  rename: string;
  start_counter: number;
  overwrite: boolean;
}

export interface ReportDto {
  input: string;
  status: "converted" | "skipped" | "failed" | "cancelled";
  output: string | null;
  message: string | null;
}

export interface BatchProgress {
  job_id: number;
  done: number;
  total: number;
}

export interface DirChanges {
  dir: string;
  changes: { kind: "added" | "modified" | "removed"; path: string }[];
}
