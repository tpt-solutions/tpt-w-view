// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Decode-engine tests. All fixtures are generated in memory so no binary files live in git.

use std::io::Cursor;
use std::time::Duration;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::{apply_orientation, ImageBackend, Orientation, StdBackend};

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, format).expect("encode fixture");
    buf.into_inner()
}

/// Gradient image where every pixel encodes its own coordinates, so transforms are checkable.
fn gradient(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x as u8).wrapping_mul(10),
            (y as u8).wrapping_mul(10),
            0,
            255,
        ])
    })
}

#[test]
fn round_trips_lossless_formats_exactly() {
    let backend = StdBackend;
    let src = DynamicImage::ImageRgba8(gradient(16, 9));
    for format in [ImageFormat::Png, ImageFormat::Bmp, ImageFormat::Tiff] {
        let bytes = encode(&src, format);
        let decoded = backend
            .decode(&bytes)
            .unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert_eq!(decoded.format, format);
        assert_eq!(
            decoded.image.to_rgba8(),
            src.to_rgba8(),
            "{format:?} pixels differ"
        );
    }
}

#[test]
fn webp_lossless_round_trip() {
    let backend = StdBackend;
    let src = DynamicImage::ImageRgba8(gradient(12, 12));
    let bytes = encode(&src, ImageFormat::WebP);
    let decoded = backend.decode(&bytes).expect("webp decode");
    assert_eq!(decoded.format, ImageFormat::WebP);
    assert_eq!(decoded.image.to_rgba8(), src.to_rgba8());
}

#[test]
fn jpeg_decodes_with_correct_dimensions() {
    let backend = StdBackend;
    let bytes = encode(
        &DynamicImage::ImageRgba8(gradient(40, 24)),
        ImageFormat::Jpeg,
    );
    let info = backend.probe(&bytes).expect("probe");
    assert_eq!(
        (info.width, info.height, info.format),
        (40, 24, ImageFormat::Jpeg)
    );
    let decoded = backend.decode(&bytes).expect("jpeg decode");
    assert_eq!(decoded.image.width(), 40);
    assert_eq!(decoded.image.height(), 24);
}

#[test]
fn probe_reports_header_without_full_decode() {
    let bytes = encode(&DynamicImage::ImageRgba8(gradient(7, 5)), ImageFormat::Png);
    let info = StdBackend.probe(&bytes).expect("probe");
    assert_eq!((info.width, info.height), (7, 5));
}

#[test]
fn animated_gif_yields_every_frame_with_delay() {
    let mut buf = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut buf);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        let frames = [(255u8, 40u32), (0u8, 80u32), (128u8, 120u32)].map(|(v, ms)| {
            let img = RgbaImage::from_pixel(4, 4, Rgba([v, v, v, 255]));
            image::Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(ms, 1))
        });
        encoder.encode_frames(frames).unwrap();
    }

    let frames = StdBackend.decode_frames(&buf).expect("gif frames");
    assert_eq!(frames.len(), 3);
    let delays: Vec<Duration> = frames.iter().map(|f| f.delay).collect();
    assert_eq!(
        delays,
        vec![
            Duration::from_millis(40),
            Duration::from_millis(80),
            Duration::from_millis(120)
        ]
    );
}

#[test]
fn still_image_decodes_as_single_frame() {
    let bytes = encode(&DynamicImage::ImageRgba8(gradient(5, 5)), ImageFormat::Png);
    let frames = StdBackend.decode_frames(&bytes).expect("frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].image.dimensions(), (5, 5));
}

#[test]
fn non_image_input_is_an_error_not_a_panic() {
    let err = StdBackend
        .decode(b"this is definitely not an image")
        .unwrap_err();
    assert!(
        matches!(err, tpt_view_core::Error::Unsupported(_)),
        "got {err:?}"
    );
    assert!(StdBackend.probe(&[]).is_err());
    assert!(StdBackend.read_metadata(&[]).is_err());
}

#[test]
fn truncated_file_is_an_error() {
    let bytes = encode(
        &DynamicImage::ImageRgba8(gradient(32, 32)),
        ImageFormat::Png,
    );
    assert!(StdBackend.decode(&bytes[..30]).is_err());
}

#[test]
fn corrupted_bytes_never_panic() {
    let bytes = encode(&DynamicImage::ImageRgba8(gradient(8, 8)), ImageFormat::Png);
    for i in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[i] ^= 0xFF;
        // Result may be Ok (e.g. ancillary chunk damage) or Err; it must not panic.
        let _ = StdBackend.decode(&damaged);
        let _ = StdBackend.decode_frames(&damaged);
    }
}

// ---- EXIF orientation & metadata -------------------------------------------------------------

/// One big-endian TIFF/IFD entry. `value` holds the inline value or the offset, padded to 4 bytes.
struct Entry {
    tag: u16,
    typ: u16,
    count: u32,
    value: [u8; 4],
}

fn entry_offset(tag: u16, typ: u16, count: u32, offset: u32) -> Entry {
    Entry {
        tag,
        typ,
        count,
        value: offset.to_be_bytes(),
    }
}

fn entry_inline_ascii(tag: u16, text: &[u8; 1]) -> Entry {
    // A one-character ASCII string plus NUL (count 2) fits inline in the 4-byte value field.
    Entry {
        tag,
        typ: 2,
        count: 2,
        value: [text[0], 0, 0, 0],
    }
}

fn write_ifd(out: &mut Vec<u8>, entries: &[Entry]) {
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for e in entries {
        out.extend_from_slice(&e.tag.to_be_bytes());
        out.extend_from_slice(&e.typ.to_be_bytes());
        out.extend_from_slice(&e.count.to_be_bytes());
        out.extend_from_slice(&e.value);
    }
    out.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
}

/// Builds a big-endian TIFF block: IFD0 (Orientation, Make, GPSInfo) plus a GPS IFD describing
/// 33°51'30" S, 151°12'0" W.
fn exif_tiff(orientation: u16) -> Vec<u8> {
    let make = b"Canon\0";
    let ifd0_len = 2 + 3 * 12 + 4;
    let gps_ifd_offset = 8 + ifd0_len as u32;
    let gps_len = 2 + 5 * 12 + 4;
    let data_start = gps_ifd_offset + gps_len as u32;
    let make_offset = data_start;
    let lat_offset = make_offset + make.len() as u32;
    let lon_offset = lat_offset + 24;

    let mut out = b"MM\0*".to_vec();
    out.extend_from_slice(&8u32.to_be_bytes());

    let mut orientation_value = [0u8; 4];
    orientation_value[..2].copy_from_slice(&orientation.to_be_bytes());
    write_ifd(
        &mut out,
        &[
            Entry {
                tag: 0x0112,
                typ: 3,
                count: 1,
                value: orientation_value,
            },
            entry_offset(0x010F, 2, make.len() as u32, make_offset),
            entry_offset(0x8825, 4, 1, gps_ifd_offset),
        ],
    );
    write_ifd(
        &mut out,
        &[
            entry_inline_ascii(0x0001, b"S"),
            entry_offset(0x0002, 5, 3, lat_offset),
            entry_inline_ascii(0x0003, b"W"),
            entry_offset(0x0004, 5, 3, lon_offset),
            Entry {
                tag: 0x0000,
                typ: 1,
                count: 4,
                value: [2, 3, 0, 0],
            }, // GPSVersionID
        ],
    );
    out.extend_from_slice(make);
    for (deg, min, sec) in [(33u32, 51u32, 30u32), (151, 12, 0)] {
        for (num, den) in [(deg, 1u32), (min, 1), (sec * 100, 100)] {
            out.extend_from_slice(&num.to_be_bytes());
            out.extend_from_slice(&den.to_be_bytes());
        }
    }
    out
}

/// Inserts an APP1/Exif segment directly after the JPEG SOI marker.
fn with_exif(jpeg: &[u8], tiff: &[u8]) -> Vec<u8> {
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(tiff);
    let seg_len = (payload.len() + 2) as u16;
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&seg_len.to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn exif_orientation_rotates_decoded_jpeg() {
    let jpeg = encode(
        &DynamicImage::ImageRgba8(gradient(40, 24)),
        ImageFormat::Jpeg,
    );
    let file = with_exif(&jpeg, &exif_tiff(6)); // 6 = rotate 90° CW to display

    let decoded = StdBackend.decode(&file).expect("decode with exif");
    assert_eq!(decoded.metadata.orientation, Orientation::Rotate90);
    assert_eq!((decoded.image.width(), decoded.image.height()), (24, 40));
}

#[test]
fn exif_metadata_fields_are_extracted() {
    let jpeg = encode(&DynamicImage::ImageRgba8(gradient(8, 8)), ImageFormat::Jpeg);
    let file = with_exif(&jpeg, &exif_tiff(1));
    let meta = StdBackend.read_metadata(&file).expect("metadata");

    assert_eq!(meta.orientation, Orientation::Normal);
    assert_eq!(meta.make.as_deref(), Some("Canon"));
    let gps = meta.gps.expect("gps present");
    assert!(
        (gps.latitude + (33.0 + 51.0 / 60.0 + 30.0 / 3600.0)).abs() < 1e-6,
        "{gps:?}"
    );
    assert!(
        (gps.longitude + (151.0 + 12.0 / 60.0)).abs() < 1e-6,
        "{gps:?}"
    );
}

#[test]
fn missing_exif_yields_defaults() {
    let bytes = encode(&DynamicImage::ImageRgba8(gradient(8, 8)), ImageFormat::Png);
    let meta = StdBackend.read_metadata(&bytes).expect("metadata");
    assert_eq!(meta, tpt_view_core::Metadata::default());
}

#[test]
fn all_orientations_map_pixels_correctly() {
    // Source is 3 wide × 2 high; each pixel encodes its own position.
    let (w, h) = (3u32, 2u32);
    let src = gradient(w, h);
    let source = DynamicImage::ImageRgba8(src.clone());

    for value in 1..=8u32 {
        let orientation = Orientation::from_exif(value);
        assert_eq!(orientation.to_exif(), value);
        let out = apply_orientation(source.clone(), orientation).to_rgba8();

        for y in 0..h {
            for x in 0..w {
                let expected = src.get_pixel(x, y);
                // Destination coordinates for each EXIF orientation.
                let (dx, dy) = match value {
                    1 => (x, y),
                    2 => (w - 1 - x, y),
                    3 => (w - 1 - x, h - 1 - y),
                    4 => (x, h - 1 - y),
                    5 => (y, x),
                    6 => (h - 1 - y, x),
                    7 => (h - 1 - y, w - 1 - x),
                    8 => (y, w - 1 - x),
                    _ => unreachable!(),
                };
                assert_eq!(
                    out.get_pixel(dx, dy),
                    expected,
                    "orientation {value}, src ({x},{y})"
                );
            }
        }
    }
}

#[test]
fn unknown_exif_orientation_defaults_to_normal() {
    assert_eq!(Orientation::from_exif(0), Orientation::Normal);
    assert_eq!(Orientation::from_exif(9), Orientation::Normal);
}
