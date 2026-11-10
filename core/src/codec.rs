// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use std::io::Cursor;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageFormat, ImageReader};

use crate::backend::{DecodedImage, Frame, ImageBackend, ImageInfo};
use crate::error::{Error, Result};
use crate::metadata::{read_exif, Metadata};
use crate::orientation::apply_orientation;

/// Default backend built on the `image` crate for JPEG, PNG, GIF (incl. animated), BMP, TIFF
/// and WebP. AVIF, HEIC and RAW decoders plug in as additional backends later.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdBackend;

fn guess_format(bytes: &[u8]) -> Result<ImageFormat> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .format()
        .ok_or_else(|| Error::Unsupported("unrecognised image signature".into()))
}

impl StdBackend {
    /// Decodes pixels exactly as stored, without applying EXIF orientation. Used when the
    /// orientation tag itself must be carried through unchanged.
    pub fn decode_pixels(&self, bytes: &[u8]) -> Result<image::DynamicImage> {
        let format = guess_format(bytes)?;
        Ok(ImageReader::with_format(Cursor::new(bytes), format).decode()?)
    }
}

impl ImageBackend for StdBackend {
    fn probe(&self, bytes: &[u8]) -> Result<ImageInfo> {
        let format = guess_format(bytes)?;
        let (width, height) =
            ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
        Ok(ImageInfo {
            width,
            height,
            format,
        })
    }

    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        let format = guess_format(bytes)?;
        let raw = self.decode_pixels(bytes)?;
        let metadata = read_exif(bytes);
        let image = apply_orientation(raw, metadata.orientation);
        Ok(DecodedImage {
            image,
            metadata,
            format,
        })
    }

    fn decode_frames(&self, bytes: &[u8]) -> Result<Vec<Frame>> {
        let format = guess_format(bytes)?;
        if format != ImageFormat::Gif {
            let still = self.decode(bytes)?;
            return Ok(vec![Frame {
                image: still.image.to_rgba8(),
                delay: Duration::ZERO,
            }]);
        }

        let decoder = GifDecoder::new(Cursor::new(bytes))?;
        let frames = decoder.into_frames().collect_frames()?;
        Ok(frames
            .into_iter()
            .map(|frame| {
                let delay = Duration::from(frame.delay());
                Frame {
                    image: frame.into_buffer(),
                    delay,
                }
            })
            .collect())
    }

    fn read_metadata(&self, bytes: &[u8]) -> Result<Metadata> {
        // Validate the signature so non-images are reported as errors rather than empty metadata.
        guess_format(bytes)?;
        Ok(read_exif(bytes))
    }
}
