// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use std::time::Duration;

use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::error::Result;
use crate::metadata::Metadata;

/// A fully decoded still image with its orientation already applied.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub image: DynamicImage,
    pub metadata: Metadata,
    pub format: ImageFormat,
}

/// One frame of an animation.
#[derive(Debug, Clone)]
pub struct Frame {
    pub image: RgbaImage,
    pub delay: Duration,
}

/// Dimensions and format obtained from the container header, without decoding pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

/// Shared decode interface. Platform crates and the Archon backend implement or consume this
/// trait so every frontend goes through one code path.
pub trait ImageBackend {
    /// Reads dimensions and format from the header only.
    fn probe(&self, bytes: &[u8]) -> Result<ImageInfo>;

    /// Decodes a still image, applying EXIF orientation.
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage>;

    /// Decodes every frame of an animated image. Still formats return a single frame.
    fn decode_frames(&self, bytes: &[u8]) -> Result<Vec<Frame>>;

    /// Extracts metadata without decoding pixels.
    fn read_metadata(&self, bytes: &[u8]) -> Result<Metadata>;
}
