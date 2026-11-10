// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Archon backend (Phase 12).
//!
//! The path→block-id layer is always compiled: it has no Archon dependency. The backend that
//! turns block ids into `UnifiedPageCache` capabilities is gated behind the `archon` feature
//! and still needs the `tpt-archon-bridge` crate wired in.

pub mod path_map;

pub use path_map::{blocks_for, BlockRange, PathBlockMap};

#[cfg(feature = "archon")]
pub mod backend {
    //! Placeholder for the capability-backed cache. See `docs/archon-design.md`.
}
