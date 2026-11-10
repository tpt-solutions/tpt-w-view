// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Minimal JPEG segment helpers for metadata preservation (Phase 5).
//!
//! Re-encoding pixels drops every metadata segment. To keep EXIF across a JPEG→JPEG conversion,
//! the APP1/Exif segment is copied verbatim from the source and spliced in after the SOI marker.

const SOI: [u8; 2] = [0xFF, 0xD8];
const EXIF_HEADER: &[u8] = b"Exif\0\0";

/// Returns the complete APP1/Exif segment (marker, length and payload) of a JPEG, if present.
/// Returns `None` for non-JPEG input or malformed marker structure.
pub fn exif_app1(jpeg: &[u8]) -> Option<&[u8]> {
    if jpeg.len() < 4 || jpeg[..2] != SOI {
        return None;
    }
    let mut pos = 2usize;
    while pos + 4 <= jpeg.len() {
        if jpeg[pos] != 0xFF {
            return None;
        }
        let marker = jpeg[pos + 1];
        match marker {
            0xFF => {
                // Fill byte; step over it.
                pos += 1;
                continue;
            }
            // Start of scan or end of image: metadata segments are all before this point.
            0xDA | 0xD9 => return None,
            // Standalone markers without a length field.
            0x01 | 0xD0..=0xD7 => {
                pos += 2;
                continue;
            }
            _ => {}
        }
        let len = usize::from(u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]));
        if len < 2 {
            return None;
        }
        let end = pos + 2 + len;
        if end > jpeg.len() {
            return None;
        }
        let payload = &jpeg[pos + 4..end];
        if marker == 0xE1 && payload.starts_with(EXIF_HEADER) {
            return Some(&jpeg[pos..end]);
        }
        pos = end;
    }
    None
}

/// Inserts `segment` immediately after the SOI marker of `jpeg`. Returns `None` if `jpeg` is not
/// a JPEG stream.
pub fn insert_after_soi(jpeg: &[u8], segment: &[u8]) -> Option<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[..2] != SOI {
        return None;
    }
    let mut out = Vec::with_capacity(jpeg.len() + segment.len());
    out.extend_from_slice(&SOI);
    out.extend_from_slice(segment);
    out.extend_from_slice(&jpeg[2..]);
    Some(out)
}

/// Yields `(marker, payload)` for every application segment (APP0-APP15) before the scan data.
/// `payload` excludes the marker and the two-byte length. Malformed structure ends the walk.
pub(crate) fn app_segments(jpeg: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    if jpeg.len() < 4 || jpeg[..2] != SOI {
        return out;
    }
    let mut pos = 2usize;
    while pos + 4 <= jpeg.len() && jpeg[pos] == 0xFF {
        let marker = jpeg[pos + 1];
        match marker {
            0xFF => {
                pos += 1;
                continue;
            }
            0xDA | 0xD9 => break,
            0x01 | 0xD0..=0xD7 => {
                pos += 2;
                continue;
            }
            _ => {}
        }
        let len = usize::from(u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]));
        if len < 2 || pos + 2 + len > jpeg.len() {
            break;
        }
        if (0xE0..=0xEF).contains(&marker) {
            out.push((marker, &jpeg[pos + 4..pos + 2 + len]));
        }
        pos += 2 + len;
    }
    out
}
