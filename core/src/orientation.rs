// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use image::DynamicImage;

/// EXIF orientation tag values (TIFF/EXIF spec, tag 0x0112).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    /// 1 — normal.
    #[default]
    Normal,
    /// 2 — mirrored horizontally.
    FlipHorizontal,
    /// 3 — rotated 180°.
    Rotate180,
    /// 4 — mirrored vertically.
    FlipVertical,
    /// 5 — mirrored over the main diagonal (transpose).
    Transpose,
    /// 6 — rotated 90° clockwise to display correctly.
    Rotate90,
    /// 7 — mirrored over the anti-diagonal (transverse).
    Transverse,
    /// 8 — rotated 270° clockwise (90° counter-clockwise) to display correctly.
    Rotate270,
}

impl Orientation {
    /// Maps a raw EXIF value to an [`Orientation`]. Unknown or missing values fall back to
    /// [`Orientation::Normal`], which is what viewers do for malformed tags.
    pub fn from_exif(value: u32) -> Self {
        match value {
            2 => Self::FlipHorizontal,
            3 => Self::Rotate180,
            4 => Self::FlipVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => Self::Normal,
        }
    }

    /// The raw EXIF value for this orientation.
    pub fn to_exif(self) -> u32 {
        match self {
            Self::Normal => 1,
            Self::FlipHorizontal => 2,
            Self::Rotate180 => 3,
            Self::FlipVertical => 4,
            Self::Transpose => 5,
            Self::Rotate90 => 6,
            Self::Transverse => 7,
            Self::Rotate270 => 8,
        }
    }
}

/// Returns `image` transformed so that it displays upright for `orientation`.
pub fn apply_orientation(image: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Normal => image,
        Orientation::FlipHorizontal => image.fliph(),
        Orientation::Rotate180 => image.rotate180(),
        Orientation::FlipVertical => image.flipv(),
        // Transpose = rotate 90° CW, then mirror horizontally.
        Orientation::Transpose => image.rotate90().fliph(),
        Orientation::Rotate90 => image.rotate90(),
        // Transverse = rotate 270° CW, then mirror horizontally.
        Orientation::Transverse => image.rotate270().fliph(),
        Orientation::Rotate270 => image.rotate270(),
    }
}
