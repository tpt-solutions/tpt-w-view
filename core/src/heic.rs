// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! HEIC/HEIF decoding through libheif (HEVC decode via libde265). Phase 1.
//!
//! Only the primary image is decoded; sequences and auxiliary images (depth maps, thumbnails)
//! are ignored. Orientation transforms stored in the file are applied by libheif itself.

use std::path::Path;

use image::{DynamicImage, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

use crate::error::{Error, Result};

/// HEIF container extensions (lowercase, no dot).
pub const HEIC_EXTENSIONS: &[&str] = &["heic", "heif"];

/// True when `path` has a HEIF container extension (case-insensitive).
pub fn is_heic_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| HEIC_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn map_err(err: libheif_rs::HeifError) -> Error {
    Error::Decode(format!("heif: {err}"))
}

/// Decodes the primary image of a HEIC/HEIF file to 8-bit RGBA.
pub fn decode_heic_bytes(bytes: &[u8]) -> Result<DynamicImage> {
    let ctx = HeifContext::read_from_bytes(bytes).map_err(map_err)?;
    let handle = ctx.primary_image_handle().map_err(map_err)?;
    let decoded = LibHeif::new()
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(map_err)?;

    let width = decoded.width();
    let height = decoded.height();
    let plane = decoded
        .planes()
        .interleaved
        .ok_or_else(|| Error::Decode("heif: decoder returned no interleaved plane".into()))?;

    // libheif rows may be padded; copy each row's visible pixels into a tight buffer.
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * plane.stride;
        pixels.extend_from_slice(&plane.data[start..start + row_bytes]);
    }
    RgbaImage::from_raw(width, height, pixels)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| Error::Decode("heif: pixel buffer size mismatch".into()))
}

/// Reads and decodes a HEIC/HEIF file from disk.
pub fn load_path(path: &Path) -> Result<DynamicImage> {
    let bytes = std::fs::read(path)?;
    decode_heic_bytes(&bytes)
}
