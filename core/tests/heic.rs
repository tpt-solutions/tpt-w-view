// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! HEIC decode through libheif. Fixtures are encoded in-process with libheif's HEVC encoder, so
//! no binary files are committed.

use std::path::Path;

use libheif_rs::{Channel, ColorSpace, CompressionFormat, HeifContext, Image, LibHeif, RgbChroma};
use tpt_view_core::heic::{decode_heic_bytes, is_heic_path};

const W: u32 = 64;
const H: u32 = 48;

fn encode_heic() -> Vec<u8> {
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
    ctx.encode_image(&image, &mut encoder, None)
        .expect("encode");
    ctx.write_to_bytes().expect("serialise")
}

#[test]
fn heic_round_trip_is_close_to_source() {
    let bytes = encode_heic();
    let decoded = decode_heic_bytes(&bytes).expect("decode heic").to_rgba8();
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
