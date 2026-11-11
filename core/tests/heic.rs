// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! HEIC decode through libheif. Fixtures are encoded in-process with libheif's HEVC encoder, so
//! no binary files are committed.

use std::path::Path;

use libheif_rs::{Channel, ColorSpace, CompressionFormat, HeifContext, Image, LibHeif, RgbChroma};
use tpt_view_core::heic::{decode_heic_bytes, is_heic_path, probe_dimensions};

const W: u32 = 64;
const H: u32 = 48;

fn encode_heic() -> Vec<u8> {
    encode_heic_with_exif(None)
}

/// Little-endian TIFF block holding only an Orientation tag, wrapped the way HEIF stores EXIF
/// (a 4-byte offset to the TIFF header, here zero).
fn exif_block_with_orientation(orientation: u16) -> Vec<u8> {
    let mut block = 0u32.to_be_bytes().to_vec();
    block.extend_from_slice(b"II*\0");
    block.extend_from_slice(&8u32.to_le_bytes());
    block.extend_from_slice(&1u16.to_le_bytes());
    block.extend_from_slice(&0x0112u16.to_le_bytes());
    block.extend_from_slice(&3u16.to_le_bytes());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&orientation.to_le_bytes());
    block.extend_from_slice(&[0, 0]);
    block.extend_from_slice(&0u32.to_le_bytes());
    block
}

fn encode_heic_with_exif(exif: Option<Vec<u8>>) -> Vec<u8> {
    let mut image = Image::new(W, H, ColorSpace::Rgb(RgbChroma::Rgb)).expect("image");
    image
        .create_plane(Channel::Interleaved, W, H, 8)
        .expect("plane");
    {
        let planes = image.planes_mut();
        let plane = planes.interleaved.expect("interleaved plane");
        for y in 0..H as usize {
            for x in 0..W as usize {
                let i = y * plane.stride + x * 3;
                plane.data[i] = (x * 255 / W as usize) as u8;
                plane.data[i + 1] = (y * 255 / H as usize) as u8;
                plane.data[i + 2] = 128;
            }
        }
    }

    let lib = LibHeif::new();
    let mut encoder = lib
        .encoder_for_format(CompressionFormat::Hevc)
        .expect("hevc encoder");
    let mut ctx = HeifContext::new().expect("context");
    let handle = ctx
        .encode_image(&image, &mut encoder, None)
        .expect("encode");
    if let Some(block) = exif {
        ctx.add_exif_metadata(&handle, &block).expect("attach exif");
    }
    ctx.write_to_bytes().expect("serialise")
}

#[test]
fn probe_reads_dimensions_from_the_header() {
    assert_eq!(probe_dimensions(&encode_heic()).unwrap(), (W, H));
    assert!(probe_dimensions(b"not heif").is_err());
}

#[test]
fn oversized_heic_fails_the_batch_budget_check_before_decoding() {
    use std::sync::atomic::AtomicBool;
    use tpt_view_core::batch::{self, BatchJob, Outcome, OutputFormat};

    let base = std::env::temp_dir().join(format!("tpt-view-heic-budget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (input, output) = (base.join("in"), base.join("out"));
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("pic.heic"), encode_heic()).unwrap();

    let mut job = BatchJob::new(vec![input.join("pic.heic")], &output, OutputFormat::Png);
    // 64 x 48 x 4 x 2 = 24 576 bytes, so a 10 000-byte budget must reject it.
    job.memory_budget_bytes = 10_000;
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    match &reports[0].outcome {
        Outcome::Failed { error } => assert!(error.contains("budget"), "{error}"),
        other => panic!("expected a budget failure, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn heic_round_trip_is_close_to_source() {
    let bytes = encode_heic();
    let decoded = decode_heic_bytes(&bytes)
        .expect("decode heic")
        .image
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (W, H));

    // Lossy codec: check the gradient is preserved within a tolerance, not exact pixels.
    let mut total = 0u64;
    for y in 0..H {
        for x in 0..W {
            let p = decoded.get_pixel(x, y).0;
            let expect_r = (x as usize * 255 / W as usize) as i32;
            let expect_g = (y as usize * 255 / H as usize) as i32;
            total += (i32::from(p[0]) - expect_r).unsigned_abs() as u64;
            total += (i32::from(p[1]) - expect_g).unsigned_abs() as u64;
        }
    }
    let mean = total as f64 / (W as f64 * H as f64 * 2.0);
    assert!(mean < 12.0, "mean channel error {mean:.2} too high");
}

#[test]
fn exif_orientation_is_applied_to_heic() {
    // Source is 64 wide x 48 high; orientation 6 means "rotate 90 degrees clockwise to display".
    let bytes = encode_heic_with_exif(Some(exif_block_with_orientation(6)));
    let decoded = decode_heic_bytes(&bytes).expect("decode heic");
    assert_eq!(
        decoded.metadata.orientation,
        tpt_view_core::Orientation::Rotate90
    );
    assert_eq!(
        (decoded.image.width(), decoded.image.height()),
        (H, W),
        "pixels must be rotated to match the tag"
    );
}

#[test]
fn heic_without_exif_has_default_metadata() {
    let decoded = decode_heic_bytes(&encode_heic()).expect("decode heic");
    assert_eq!(decoded.metadata, tpt_view_core::Metadata::default());
}

#[test]
fn truncated_heic_is_an_error_not_a_panic() {
    let mut bytes = encode_heic();
    bytes.truncate(bytes.len() / 2);
    assert!(decode_heic_bytes(&bytes).is_err());
}

#[test]
fn non_heif_bytes_are_an_error() {
    assert!(decode_heic_bytes(b"this is not a heif container at all").is_err());
}

#[test]
fn heic_extensions_are_recognised() {
    assert!(is_heic_path(Path::new("IMG_1234.HEIC")));
    assert!(is_heic_path(Path::new("live.heif")));
    assert!(!is_heic_path(Path::new("photo.jpg")));
}
