// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Minimal JPEG segment helpers for metadata preservation (Phase 5).
//!
//! Re-encoding pixels drops every metadata segment. To keep metadata across a JPEG→JPEG
//! conversion, the EXIF, XMP and IPTC (Photoshop APP13) segments are copied verbatim from the
//! source and spliced in after the SOI marker, in their original order.

const SOI: [u8; 2] = [0xFF, 0xD8];
const EXIF_HEADER: &[u8] = b"Exif\0\0";
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXTENSION_HEADER: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const PHOTOSHOP_HEADER: &[u8] = b"Photoshop 3.0\0";

/// Returns the complete bytes (marker, length and payload) of every metadata segment that a
/// JPEG→JPEG conversion should keep: APP1/Exif, APP1/XMP (including extended XMP parts) and
/// APP13/Photoshop (which carries IPTC-IIM). Segments are returned in file order.
///
/// Returns an empty list for non-JPEG input or malformed marker structure.
pub fn preserved_segments(jpeg: &[u8]) -> Vec<&[u8]> {
    walk_segments(jpeg)
        .into_iter()
        .filter(|seg| is_preserved(seg.marker, seg.payload))
        .map(|seg| seg.raw)
        .collect()
}

fn is_preserved(marker: u8, payload: &[u8]) -> bool {
    match marker {
        0xE1 => {
            payload.starts_with(EXIF_HEADER)
                || payload.starts_with(XMP_HEADER)
                || payload.starts_with(XMP_EXTENSION_HEADER)
        }
        0xED => payload.starts_with(PHOTOSHOP_HEADER),
        _ => false,
    }
}

/// The TIFF block of a JPEG's APP1/Exif segment (the payload after the `Exif\0\0` header).
pub fn exif_tiff(jpeg: &[u8]) -> Option<&[u8]> {
    walk_segments(jpeg)
        .into_iter()
        .find(|seg| seg.marker == 0xE1 && seg.payload.starts_with(EXIF_HEADER))
        .map(|seg| &seg.payload[EXIF_HEADER.len()..])
}

/// The XMP packet of a JPEG's standard APP1/XMP segment (the payload after the XMP header).
/// Extended XMP parts are not returned.
pub fn xmp_packet(jpeg: &[u8]) -> Option<&[u8]> {
    walk_segments(jpeg)
        .into_iter()
        .find(|seg| seg.marker == 0xE1 && seg.payload.starts_with(XMP_HEADER))
        .map(|seg| &seg.payload[XMP_HEADER.len()..])
}

/// Inserts `segments` immediately after the SOI marker of `jpeg`, in the order given. Returns
/// `None` if `jpeg` is not a JPEG stream.
pub fn insert_after_soi(jpeg: &[u8], segments: &[&[u8]]) -> Option<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[..2] != SOI {
        return None;
    }
    let extra: usize = segments.iter().map(|s| s.len()).sum();
    let mut out = Vec::with_capacity(jpeg.len() + extra);
    out.extend_from_slice(&SOI);
    for segment in segments {
        out.extend_from_slice(segment);
    }
    out.extend_from_slice(&jpeg[2..]);
    Some(out)
}

/// One application segment: the full bytes as stored, plus the payload after the length field.
pub(crate) struct AppSegment<'a> {
    pub marker: u8,
    pub raw: &'a [u8],
    pub payload: &'a [u8],
}

/// Yields `(marker, payload)` for every application segment (APP0-APP15) before the scan data.
/// `payload` excludes the marker and the two-byte length. Malformed structure ends the walk.
pub(crate) fn app_segments(jpeg: &[u8]) -> Vec<(u8, &[u8])> {
    walk_segments(jpeg)
        .into_iter()
        .map(|seg| (seg.marker, seg.payload))
        .collect()
}

/// Walks the marker segments before the scan data and returns every APP0-APP15 segment.
/// Malformed structure ends the walk.
fn walk_segments(jpeg: &[u8]) -> Vec<AppSegment<'_>> {
    let mut out = Vec::new();
    if jpeg.len() < 4 || jpeg[..2] != SOI {
        return out;
    }
    let mut pos = 2usize;
    while pos + 4 <= jpeg.len() && jpeg[pos] == 0xFF {
        let marker = jpeg[pos + 1];
        match marker {
            0xFF => {
                // Fill byte; step over it.
                pos += 1;
                continue;
            }
            // Start of scan or end of image: metadata segments are all before this point.
            0xDA | 0xD9 => break,
            // Standalone markers without a length field.
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
        let end = pos + 2 + len;
        if (0xE0..=0xEF).contains(&marker) {
            out.push(AppSegment {
                marker,
                raw: &jpeg[pos..end],
                payload: &jpeg[pos + 4..end],
            });
        }
        pos = end;
    }
    out
}
