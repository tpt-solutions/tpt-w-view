// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Non-recursive directory scanning for supported image files (Phase 3).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::Result;

/// File extensions (lowercase) the decode engine handles today.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "cr2", "cr3", "nef", "arw", "dng",
    "orf", "raf", "rw2", "pef", "srw", "avif", "heic", "heif",
];

/// One image file found by [`scan_dir`]. `size` and `modified_nanos` form the cache identity
/// used by the thumbnail cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    /// Modification time as nanoseconds since the Unix epoch (0 if unavailable).
    pub modified_nanos: u128,
}

/// True when `path` has a supported image extension (case-insensitive).
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Lists the subdirectories directly inside `dir`, sorted by name (case-insensitive). Symbolic
/// links are not followed, so a link cannot make the folder tree loop.
pub fn list_subdirectories(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        if item.file_type()?.is_dir() {
            dirs.push(item.path());
        }
    }
    dirs.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    Ok(dirs)
}

/// Lists supported image files directly inside `dir`, sorted by file name (case-insensitive).
/// Subdirectories are not descended into; directory navigation is a UI concern.
pub fn scan_dir(dir: &Path) -> Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let meta = item.metadata()?;
        let path = item.path();
        if !meta.is_file() || !is_supported(&path) {
            continue;
        }
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        entries.push(FileEntry {
            path,
            size: meta.len(),
            modified_nanos,
        });
    }
    entries.sort_by_key(|e| {
        e.path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    Ok(entries)
}

/// MIME type for a supported extension (lowercase, no dot). Shared by the Linux `.desktop`/MIME
/// packaging and the Windows file-association plan so both platforms advertise the same set.
pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "cr2" => "image/x-canon-cr2",
        "cr3" => "image/x-canon-cr3",
        "nef" => "image/x-nikon-nef",
        "arw" => "image/x-sony-arw",
        "dng" => "image/x-adobe-dng",
        "orf" => "image/x-olympus-orf",
        "raf" => "image/x-fuji-raf",
        "rw2" => "image/x-panasonic-rw2",
        "pef" => "image/x-pentax-pef",
        "srw" => "image/x-samsung-srw",
        _ => return None,
    })
}
