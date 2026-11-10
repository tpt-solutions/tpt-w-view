// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! tpt-view's own path → block-id mapping (Phase 12).
//!
//! Archon's `Resource::Page` is a raw block id with no file identity, so tpt-view owns the
//! mapping from file paths to block ranges. This module has no Archon dependency: it allocates
//! ids and tracks ownership, and the Archon backend (behind the `archon` feature) turns those
//! ids into capabilities.
//!
//! Block ids are never reused after release. Reuse would let a stale handle silently read a
//! different file's cached bytes, which is exactly the kind of capability confusion the Phase 14
//! security review looks for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A contiguous run of block ids holding one file's cached bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRange {
    pub first: u64,
    pub count: u64,
}

impl BlockRange {
    pub fn ids(&self) -> impl Iterator<Item = u64> {
        self.first..self.first + self.count
    }
}

/// Maps paths to block ranges and back.
#[derive(Debug, Default)]
pub struct PathBlockMap {
    next_block: u64,
    by_path: HashMap<PathBuf, BlockRange>,
    by_block: HashMap<u64, PathBuf>,
}

impl PathBlockMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensures `path` has blocks covering `byte_len` bytes at `block_size` bytes per block.
    /// Returns the existing range if one is already allocated and large enough. A file that has
    /// grown gets a fresh range; the old one is released.
    pub fn allocate(&mut self, path: &Path, byte_len: u64, block_size: u64) -> BlockRange {
        let needed = blocks_for(byte_len, block_size);
        if let Some(existing) = self.by_path.get(path) {
            if existing.count == needed {
                return *existing;
            }
        }
        self.release(path);

        let range = BlockRange {
            first: self.next_block,
            count: needed,
        };
        self.next_block += needed;
        for id in range.ids() {
            self.by_block.insert(id, path.to_path_buf());
        }
        self.by_path.insert(path.to_path_buf(), range);
        range
    }

    /// Frees the blocks for `path`. The ids stay retired and are never handed out again.
    pub fn release(&mut self, path: &Path) -> Option<BlockRange> {
        let range = self.by_path.remove(path)?;
        for id in range.ids() {
            self.by_block.remove(&id);
        }
        Some(range)
    }

    pub fn range_for(&self, path: &Path) -> Option<BlockRange> {
        self.by_path.get(path).copied()
    }

    /// Which file currently owns block `id`, if any.
    pub fn owner_of(&self, id: u64) -> Option<&Path> {
        self.by_block.get(&id).map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }
}

/// Number of blocks needed for `byte_len` bytes. An empty file still takes one block so every
/// path has a handle.
pub fn blocks_for(byte_len: u64, block_size: u64) -> u64 {
    assert!(block_size > 0, "block size must be positive");
    byte_len.div_ceil(block_size).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_ceil_blocks_and_round_trips_owner() {
        let mut map = PathBlockMap::new();
        let a = map.allocate(Path::new("a.jpg"), 10_000, 4096);
        assert_eq!(a.count, 3);
        assert_eq!(map.owner_of(a.first), Some(Path::new("a.jpg")));
        assert_eq!(map.owner_of(a.first + 2), Some(Path::new("a.jpg")));
        assert_eq!(map.owner_of(a.first + 3), None);
    }

    #[test]
    fn distinct_files_never_share_blocks() {
        let mut map = PathBlockMap::new();
        let a = map.allocate(Path::new("a.png"), 8192, 4096);
        let b = map.allocate(Path::new("b.png"), 8192, 4096);
        assert!(a.ids().all(|id| !b.ids().any(|other| other == id)));
    }

    #[test]
    fn unchanged_file_keeps_its_range() {
        let mut map = PathBlockMap::new();
        let first = map.allocate(Path::new("x.webp"), 100, 64);
        let again = map.allocate(Path::new("x.webp"), 100, 64);
        assert_eq!(first, again);
    }

    #[test]
    fn released_ids_are_never_reused() {
        let mut map = PathBlockMap::new();
        let old = map.allocate(Path::new("gone.gif"), 100, 64);
        map.release(Path::new("gone.gif"));
        assert_eq!(map.owner_of(old.first), None);
        let fresh = map.allocate(Path::new("new.gif"), 100, 64);
        assert!(
            fresh.first >= old.first + old.count,
            "ids must not be recycled"
        );
        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn grown_file_gets_new_range_and_old_one_is_retired() {
        let mut map = PathBlockMap::new();
        let small = map.allocate(Path::new("g.tif"), 64, 64);
        let big = map.allocate(Path::new("g.tif"), 640, 64);
        assert_eq!(big.count, 10);
        assert!(big.first > small.first);
        assert_eq!(map.owner_of(small.first), None);
        assert_eq!(map.range_for(Path::new("g.tif")), Some(big));
    }

    #[test]
    fn empty_file_still_gets_one_block() {
        assert_eq!(blocks_for(0, 4096), 1);
    }
}
