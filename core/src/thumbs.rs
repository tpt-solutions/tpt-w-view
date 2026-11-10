// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Persistent on-disk thumbnail cache (Phase 3).
//!
//! Each thumbnail is stored as `<cache_dir>/<key>.png`, where `key` is a stable 64-bit FNV-1a
//! hash of the source path, size, modification time and target edge length. Changing a file
//! therefore produces a new key (automatic invalidation); old entries are removed by [`prune`].
//!
//! [`prune`]: ThumbnailCache::prune

use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{ImageFormat, ImageReader};

use crate::backend::ImageBackend;
use crate::codec::StdBackend;
use crate::error::Result;
use crate::scan::FileEntry;

/// Result of a [`ThumbnailCache::get_or_create`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub path: PathBuf,
    /// True when the thumbnail already existed on disk and no decode was needed.
    pub cache_hit: bool,
}

#[derive(Debug, Clone)]
pub struct ThumbnailCache {
    dir: PathBuf,
    max_edge: u32,
}

/// Stable 64-bit FNV-1a. `std`'s hashers are not guaranteed stable across releases, and cache
/// keys must survive upgrades.
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        // Separator so ("ab","c") and ("a","bc") hash differently.
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

impl ThumbnailCache {
    /// Opens (creating if needed) a cache in `dir` producing thumbnails no larger than
    /// `max_edge` pixels on their longest side.
    pub fn new(dir: impl Into<PathBuf>, max_edge: u32) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            max_edge: max_edge.max(1),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Cache key for `entry` at this cache's edge length.
    pub fn key_for(&self, entry: &FileEntry) -> String {
        let path = entry.path.to_string_lossy();
        let size = entry.size.to_le_bytes();
        let mtime = entry.modified_nanos.to_le_bytes();
        let edge = self.max_edge.to_le_bytes();
        format!("{:016x}", fnv1a(&[path.as_bytes(), &size, &mtime, &edge]))
    }

    /// Path where the thumbnail for `entry` is (or would be) stored.
    pub fn path_for(&self, entry: &FileEntry) -> PathBuf {
        self.dir.join(format!("{}.png", self.key_for(entry)))
    }

    /// Returns the cached thumbnail, generating and storing it on a miss.
    pub fn get_or_create(&self, entry: &FileEntry) -> Result<Thumbnail> {
        let path = self.path_for(entry);
        if path.is_file() {
            return Ok(Thumbnail {
                path,
                cache_hit: true,
            });
        }

        let source = if crate::raw::is_raw_path(&entry.path) {
            crate::raw::load_path(&entry.path)?.image
        } else if crate::heic::is_heic_path(&entry.path) {
            crate::heic::load_path(&entry.path)?
        } else {
            let bytes = std::fs::read(&entry.path)?;
            StdBackend.decode(&bytes)?.image
        };
        let thumb = source.thumbnail(self.max_edge, self.max_edge).to_rgba8();

        // Write to a temp name then rename, so readers never observe a half-written file.
        let tmp = path.with_extension("png.tmp");
        {
            let mut buf = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(thumb).write_to(&mut buf, ImageFormat::Png)?;
            std::fs::write(&tmp, buf.into_inner())?;
        }
        std::fs::rename(&tmp, &path)?;

        Ok(Thumbnail {
            path,
            cache_hit: false,
        })
    }

    /// Deletes cached thumbnails that no longer correspond to any entry in `live`.
    /// Returns the number of files removed.
    pub fn prune(&self, live: &[FileEntry]) -> Result<usize> {
        let keep: HashSet<String> = live
            .iter()
            .map(|e| format!("{}.png", self.key_for(e)))
            .collect();
        let mut removed = 0;
        for item in std::fs::read_dir(&self.dir)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            if name.ends_with(".png") && !keep.contains(&name) {
                std::fs::remove_file(item.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// Opens an already-cached thumbnail's dimensions without decoding pixels. Used by tests and
/// the grid to confirm cache contents.
pub fn cached_dimensions(path: &Path) -> Result<(u32, u32)> {
    Ok(ImageReader::open(path)?.into_dimensions()?)
}
