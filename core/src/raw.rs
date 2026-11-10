// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Camera RAW decoding (Phase 1) via the pure-Rust `rawler` crate.
//!
//! The raw sensor data is developed through rawler's pipeline (rescale, demosaic, crop to the
//! active area, white balance, colour calibration, sRGB encoding) into an 8-bit RGB image, and
//! the EXIF orientation is then applied. Camera coverage is whatever rawler supports; unknown
//! models return an error rather than a guess.
//!
//! Limitations: no lens or noise correction beyond rawler's defaults, and no per-file develop
//! settings yet. Those belong with the editing pipeline.

use std::path::Path;

use image::DynamicImage;
use rawler::decoders::RawDecodeParams;
use rawler::imgop::develop::RawDevelop;
use rawler::rawsource::RawSource;
use rawler::RawImage;

use crate::error::{Error, Result};
use crate::metadata::Metadata;
use crate::orientation::{apply_orientation, Orientation};

/// Camera RAW extensions (lowercase, no dot) recognised by the engine.
pub const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "dng", "orf", "raf", "rw2", "pef", "srw",
];

/// True when `path` has a camera RAW extension (case-insensitive).
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// A developed RAW file: upright 8-bit RGB pixels plus the metadata rawler reported.
#[derive(Debug, Clone)]
pub struct RawDecoded {
    pub image: DynamicImage,
    pub metadata: Metadata,
}

fn map_err(err: rawler::RawlerError) -> Error {
    match err {
        rawler::RawlerError::Unsupported { .. } => Error::Unsupported(err.to_string()),
        other => Error::Decode(other.to_string()),
    }
}

/// Decodes and develops a RAW file from disk. The file is memory-mapped where possible.
pub fn load_path(path: &Path) -> Result<RawDecoded> {
    let source = RawSource::new(path)?;
    let raw = rawler::decode(&source, &RawDecodeParams::default()).map_err(map_err)?;
    develop(&raw)
}

/// Decodes and develops a RAW file already held in memory.
pub fn decode_raw_bytes(bytes: &[u8]) -> Result<RawDecoded> {
    let source = RawSource::new_from_slice(bytes);
    let raw = rawler::decode(&source, &RawDecodeParams::default()).map_err(map_err)?;
    develop(&raw)
}

/// Runs the default develop pipeline on decoded sensor data and orients the result.
pub(crate) fn develop(raw: &RawImage) -> Result<RawDecoded> {
    let developed = RawDevelop::default()
        .develop_intermediate(raw)
        .map_err(map_err)?
        .to_dynamic_image()
        .ok_or_else(|| Error::Decode("development produced no image".into()))?;

    let orientation = Orientation::from_exif(u32::from(raw.orientation.to_u16()));
    let image = apply_orientation(DynamicImage::ImageRgb8(developed.to_rgb8()), orientation);

    let metadata = Metadata {
        orientation,
        make: non_empty(&raw.clean_make),
        model: non_empty(&raw.clean_model),
        ..Metadata::default()
    };
    Ok(RawDecoded { image, metadata })
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawler::cfa::PlaneColor;
    use rawler::decoders::Camera;
    use rawler::pixarray::PixU16;
    use rawler::rawimage::{BlackLevel, CFAConfig, RawPhotometricInterpretation, WhiteLevel};
    use rawler::CFA;

    /// Builds an in-memory RGGB Bayer RawImage with a horizontal luminance ramp.
    fn bayer_ramp(width: usize, height: usize) -> RawImage {
        let mut data = Vec::with_capacity(width * height);
        for _y in 0..height {
            for x in 0..width {
                data.push((x * 4000 / width.max(1)) as u16);
            }
        }
        let mut cam = Camera::new();
        cam.cfa = CFA::new("RGGB");
        RawImage::new(
            cam,
            PixU16::new_with(data, width, height),
            1,
            [1.0, 1.0, 1.0, 1.0],
            RawPhotometricInterpretation::Cfa(CFAConfig::new(
                &CFA::new("RGGB"),
                &PlaneColor::new("RGGB"),
            )),
            Some(BlackLevel::new(&[0_u32], 1, 1, 1)),
            Some(WhiteLevel::new_bits(12, 1)),
            false,
        )
    }

    #[test]
    fn extensions_are_classified_case_insensitively() {
        assert!(is_raw_path(Path::new("IMG_0001.CR2")));
        assert!(is_raw_path(Path::new("shot.dng")));
        assert!(!is_raw_path(Path::new("photo.jpg")));
        assert!(!is_raw_path(Path::new("no_extension")));
    }

    #[test]
    fn develops_bayer_data_to_upright_rgb() {
        let raw = bayer_ramp(16, 8);
        let decoded = develop(&raw).expect("develop");
        assert_eq!((decoded.image.width(), decoded.image.height()), (16, 8));

        let rgb = decoded.image.to_rgb8();
        let left = rgb.get_pixel(0, 4).0;
        let right = rgb.get_pixel(15, 4).0;
        let lum = |p: [u8; 3]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        assert!(
            lum(right[..3].try_into().unwrap()) > lum(left[..3].try_into().unwrap()),
            "ramp should brighten left to right: {left:?} vs {right:?}"
        );
    }

    #[test]
    fn orientation_is_applied_after_develop() {
        let mut raw = bayer_ramp(16, 8);
        raw.orientation = rawler::decoders::Orientation::Rotate90;
        let decoded = develop(&raw).expect("develop");
        assert_eq!(decoded.metadata.orientation, Orientation::Rotate90);
        assert_eq!((decoded.image.width(), decoded.image.height()), (8, 16));
    }

    #[test]
    fn every_raw_extension_is_scanned() {
        for ext in RAW_EXTENSIONS {
            assert!(
                crate::scan::SUPPORTED_EXTENSIONS.contains(ext),
                "{ext} missing from scan list"
            );
        }
    }

    #[test]
    fn garbage_bytes_are_an_error_not_a_panic() {
        assert!(decode_raw_bytes(b"definitely not a camera file").is_err());
        assert!(decode_raw_bytes(&[]).is_err());
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let missing = std::env::temp_dir().join("tpt-no-such-raw-file.cr2");
        assert!(matches!(load_path(&missing), Err(Error::Io(_))));
    }
}
