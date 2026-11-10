// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! AVIF decode and encode through the native dav1d decoder and the rav1e-based encoder.

use std::io::Cursor;

use image::codecs::avif::AvifEncoder;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::{ImageBackend, StdBackend};

fn encode_avif(img: &DynamicImage, quality: u8) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = AvifEncoder::new_with_speed_quality(&mut buf, 10, quality);
    img.write_with_encoder(encoder).expect("encode avif");
    buf.into_inner()
}

fn smooth_image(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            (x * 255 / w.max(1)) as u8,
            (y * 255 / h.max(1)) as u8,
            128,
            255,
        ])
    })
}

#[test]
fn avif_is_detected_and_probed() {
    let bytes = encode_avif(&DynamicImage::ImageRgba8(smooth_image(64, 48)), 80);
    let info = StdBackend.probe(&bytes).expect("probe avif");
    assert_eq!(info.format, ImageFormat::Avif);
    assert_eq!((info.width, info.height), (64, 48));
}

#[test]
fn avif_decodes_close_to_source() {
    let src = smooth_image(64, 48);
    let bytes = encode_avif(&DynamicImage::ImageRgba8(src.clone()), 90);
    let decoded = StdBackend
        .decode(&bytes)
        .expect("decode avif")
        .image
        .to_rgba8();
    assert_eq!(decoded.dimensions(), src.dimensions());

    // Lossy codec: compare average absolute error on the colour channels, not exact pixels.
    let total: u64 = src
        .pixels()
        .zip(decoded.pixels())
        .map(|(a, b)| {
            (0..3)
                .map(|c| u64::from(a.0[c].abs_diff(b.0[c])))
                .sum::<u64>()
        })
        .sum();
    let mean = total as f64 / (src.width() as f64 * src.height() as f64 * 3.0);
    assert!(
        mean < 8.0,
        "mean channel error {mean:.2} too high for quality 90"
    );
}

#[test]
fn corrupt_avif_is_an_error_not_a_panic() {
    let mut bytes = encode_avif(&DynamicImage::ImageRgba8(smooth_image(32, 32)), 70);
    bytes.truncate(bytes.len() / 3);
    assert!(StdBackend.decode(&bytes).is_err());
}
