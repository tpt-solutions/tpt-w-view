// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Core image decode engine for tpt-view.
//!
//! Platform crates, the CLI and (later) the Tauri shell all talk to images through the
//! [`ImageBackend`] trait so that the Archon backend can share the same interface.

mod backend;
pub mod batch;
mod codec;
pub mod edit;
mod error;
pub mod grid;
pub mod heic;
mod jpeg_meta;
mod metadata;
mod orientation;
mod png_meta;
pub mod raw;
pub mod rename;
pub mod scan;
mod text_meta;
pub mod thumbs;
pub mod tiling;
pub mod watch;
mod webp_meta;

pub use backend::{DecodedImage, Frame, ImageBackend, ImageInfo};
pub use codec::StdBackend;
pub use error::{Error, Result};
pub use metadata::{GpsCoordinates, Metadata};
pub use orientation::{apply_orientation, Orientation};
