// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Adds EXIF and XMP chunks to a WebP file (Phase 5). The image encoder writes pixels only, so the
//! RIFF container is rebuilt here: a `VP8X` header announces the EXIF and XMP chunks, which are
//! appended after the image data as the WebP container format requires.

const RIFF: &[u8; 4] = b"RIFF";
const WEBP: &[u8; 4] = b"WEBP";
const FLAG_XMP: u8 = 0x04;
const FLAG_EXIF: u8 = 0x08;

/// One RIFF chunk: its FourCC and payload (without padding).
struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

/// Splits a WebP file into its chunks. Returns `None` if the RIFF structure is malformed.
fn chunks(webp: &[u8]) -> Option<Vec<Chunk<'_>>> {
    if webp.len() < 12 || &webp[..4] != RIFF || &webp[8..12] != WEBP {
        return None;
    }
    let riff_len = u32::from_le_bytes(webp[4..8].try_into().ok()?) as usize;
    if riff_len + 8 != webp.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 12usize;
    while pos < webp.len() {
        let header = webp.get(pos..pos + 8)?;
        let kind: [u8; 4] = header[..4].try_into().ok()?;
        let len = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let data = webp.get(pos + 8..pos + 8 + len)?;
        out.push(Chunk { kind, data });
        pos += 8 + len + (len & 1);
    }
    Some(out)
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(kind);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

/// Returns a copy of `webp` that carries `exif` (a TIFF block) and `xmp` (an XMP packet). Either
/// may be absent. `width` and `height` are the image's pixel size, used for the `VP8X` canvas.
/// Returns `None` if `webp` is not a well-formed WebP file.
pub(crate) fn with_metadata(
    webp: &[u8],
    width: u32,
    height: u32,
    exif: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > (1 << 24) || height > (1 << 24) {
        return None;
    }
    let parts = chunks(webp)?;
    let mut flags = 0u8;
    let mut body = Vec::with_capacity(webp.len());

    for chunk in &parts {
        match &chunk.kind {
            b"VP8X" => {
                // Keep the existing header's alpha and animation bits; the metadata bits are ours.
                flags |= chunk.data.first().copied().unwrap_or(0) & !(FLAG_XMP | FLAG_EXIF);
            }
            // Replaced below: any metadata already in the file is superseded.
            b"EXIF" | b"XMP " => {}
            _ => push_chunk(&mut body, &chunk.kind, chunk.data),
        }
    }
    if exif.is_some() {
        flags |= FLAG_EXIF;
    }
    if xmp.is_some() {
        flags |= FLAG_XMP;
    }

    let mut vp8x = vec![flags, 0, 0, 0];
    let (w, h) = (width - 1, height - 1);
    vp8x.extend_from_slice(&w.to_le_bytes()[..3]);
    vp8x.extend_from_slice(&h.to_le_bytes()[..3]);

    let mut file = Vec::with_capacity(webp.len() + 64);
    file.extend_from_slice(RIFF);
    file.extend_from_slice(&[0; 4]); // size, filled in below
    file.extend_from_slice(WEBP);
    // The VP8X chunk must come first. An existing one was dropped above, so there is one.
    push_chunk(&mut file, b"VP8X", &vp8x);
    file.extend_from_slice(&body);
    if let Some(tiff) = exif {
        push_chunk(&mut file, b"EXIF", tiff);
    }
    if let Some(packet) = xmp {
        push_chunk(&mut file, b"XMP ", packet);
    }
    let riff_len = (file.len() - 8) as u32;
    file[4..8].copy_from_slice(&riff_len.to_le_bytes());
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid RIFF/WEBP with one chunk of the given kind and payload.
    fn file_with(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        push_chunk(&mut body, kind, payload);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
        out.extend_from_slice(b"WEBP");
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn adds_vp8x_and_metadata_chunks_in_order() {
        let base = file_with(b"VP8L", &[0x2f, 0, 0, 0, 0]);
        let out = with_metadata(&base, 10, 4, Some(b"tiff!"), Some(b"<x/>")).unwrap();
        let parts = chunks(&out).unwrap();
        let kinds: Vec<&[u8; 4]> = parts.iter().map(|c| &c.kind).collect();
        assert_eq!(kinds, [b"VP8X", b"VP8L", b"EXIF", b"XMP "]);
        assert_eq!(
            parts[0].data[0] & (FLAG_EXIF | FLAG_XMP),
            FLAG_EXIF | FLAG_XMP
        );
        assert_eq!(
            &parts[0].data[4..7],
            &9u32.to_le_bytes()[..3],
            "canvas width - 1"
        );
        assert_eq!(
            &parts[0].data[7..10],
            &3u32.to_le_bytes()[..3],
            "canvas height - 1"
        );
    }

    #[test]
    fn existing_metadata_is_replaced_not_duplicated() {
        let first = with_metadata(
            &file_with(b"VP8L", &[0x2f, 0, 0, 0, 0]),
            2,
            2,
            Some(b"old"),
            None,
        )
        .unwrap();
        let second = with_metadata(&first, 2, 2, Some(b"new!"), None).unwrap();
        let exif: Vec<_> = chunks(&second)
            .unwrap()
            .into_iter()
            .filter(|c| &c.kind == b"EXIF")
            .map(|c| c.data.to_vec())
            .collect();
        assert_eq!(exif, vec![b"new!".to_vec()]);
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert!(with_metadata(b"RIFF", 1, 1, None, None).is_none());
        assert!(with_metadata(&file_with(b"VP8L", &[1]), 0, 1, None, None).is_none());
    }
}
