// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! HEIC/HEIF decoding through libheif (HEVC decode via libde265). Phase 1.
//!
//! Only the primary image is decoded; sequences and auxiliary images (depth maps, thumbnails)
//! are ignored. libheif applies the rotation and mirroring stored as HEIF properties. The EXIF
//! orientation tag is applied here, so files that only carry it are still shown upright.

use std::path::Path;

use image::{DynamicImage, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, ImageHandle, LibHeif, RgbChroma};

use crate::error::{Error, Result};
use crate::metadata::{read_heif_exif, Metadata};
use crate::orientation::apply_orientation;

/// HEIF container extensions (lowercase, no dot).
pub const HEIC_EXTENSIONS: &[&str] = &["heic", "heif"];

/// A decoded HEIF image: upright 8-bit RGBA pixels plus EXIF metadata.
#[derive(Debug, Clone)]
pub struct HeicDecoded {
    pub image: DynamicImage,
    pub metadata: Metadata,
}

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

/// Reads the first EXIF block of the primary image, if there is one.
fn read_exif_block(handle: &ImageHandle) -> Option<Metadata> {
    let count = handle.number_of_metadata_blocks(b"Exif");
    if count <= 0 {
        return None;
    }
    let mut ids = vec![0u32; count as usize];
    let found = handle.metadata_block_ids(&mut ids, b"Exif");
    let id = *ids.get(..found)?.first()?;
    let block = handle.metadata(id).ok()?;
    read_heif_exif(&block)
}

/// Decodes a HEIC/HEIF file held in memory.
pub fn decode_heic_bytes(bytes: &[u8]) -> Result<HeicDecoded> {
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
    let raw = RgbaImage::from_raw(width, height, pixels)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| Error::Decode("heif: pixel buffer size mismatch".into()))?;

    let metadata = read_exif_block(&handle).unwrap_or_default();
    let image = apply_orientation(raw, metadata.orientation);
    Ok(HeicDecoded { image, metadata })
}

/// Reads and decodes a HEIC/HEIF file from disk.
pub fn load_path(path: &Path) -> Result<HeicDecoded> {
    let bytes = std::fs::read(path)?;
    decode_heic_bytes(&bytes)
}
