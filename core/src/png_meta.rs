// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Writes metadata chunks into PNG files (Phase 5): `eXIf` for EXIF and an uncompressed
//! `XML:com.adobe.xmp` `iTXt` chunk for XMP. Chunks go directly after IHDR, which is where the
//! PNG specification places them, and every chunk carries a correct CRC.

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const IHDR_END: usize = 8 + 4 + 4 + 13 + 4;
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// CRC-32 (ISO 3309, the polynomial PNG uses), computed bit by bit.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &byte in *part {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }
    !crc
}

/// One complete PNG chunk: length, type, data and CRC.
pub(crate) fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 12);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
    out
}

/// Body of an uncompressed XMP `iTXt` chunk holding `xml`.
pub(crate) fn xmp_itxt_body(xml: &[u8]) -> Vec<u8> {
    let mut body = XMP_KEYWORD.to_vec();
    body.push(0); // keyword terminator
    body.extend_from_slice(&[0, 0]); // compression flag and method: uncompressed
    body.extend_from_slice(&[0, 0]); // empty language tag and translated keyword
    body.extend_from_slice(xml);
    body
}

/// Inserts `chunks` directly after the IHDR chunk. Returns `None` if `png` is not a PNG whose
/// first chunk is IHDR.
pub(crate) fn insert_after_ihdr(png: &[u8], chunks: &[Vec<u8>]) -> Option<Vec<u8>> {
    if png.len() < IHDR_END || !png.starts_with(PNG_SIGNATURE) || &png[12..16] != b"IHDR" {
        return None;
    }
    let extra: usize = chunks.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(png.len() + extra);
    out.extend_from_slice(&png[..IHDR_END]);
    for c in chunks {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&png[IHDR_END..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_standard_check_value() {
        // The IEND chunk's CRC is a well-known constant.
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn chunk_layout_is_length_type_data_crc() {
        let c = chunk(b"abcd", b"xyz");
        assert_eq!(&c[..4], &3u32.to_be_bytes());
        assert_eq!(&c[4..8], b"abcd");
        assert_eq!(&c[8..11], b"xyz");
        assert_eq!(c.len(), 15);
    }
}
