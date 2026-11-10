// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Basic editing tools (Phase 4): crop, rotate, flip, resize, colour and levels adjustments,
//! and a non-destructive edit session with undo/redo and an explicit save flow.

use std::collections::VecDeque;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::error::{Error, Result};

/// Resampling filter used for resize and arbitrary-angle rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Bilinear,
    /// Catmull-Rom bicubic.
    Bicubic,
    Lanczos3,
}

impl Filter {
    fn to_image(self) -> FilterType {
        match self {
            Filter::Nearest => FilterType::Nearest,
            Filter::Bilinear => FilterType::Triangle,
            Filter::Bicubic => FilterType::CatmullRom,
            Filter::Lanczos3 => FilterType::Lanczos3,
        }
    }
}

/// One editing operation. Operations are value types so they can be logged or replayed.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Crop to a rectangle in source pixel coordinates.
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// Rotate by a multiple of 90°, clockwise. `quarter_turns` is taken modulo 4.
    RotateQuarterTurns(u8),
    /// Rotate by an arbitrary angle in degrees, clockwise. With `expand` the canvas grows to hold
    /// the whole rotated image; otherwise it keeps its size and corners become transparent.
    RotateArbitrary {
        degrees: f32,
        expand: bool,
        filter: Filter,
    },
    FlipHorizontal,
    FlipVertical,
    /// Resize to exact dimensions.
    Resize {
        width: u32,
        height: u32,
        filter: Filter,
    },
    /// Add `delta` (-255..=255) to every colour channel.
    Brightness(i32),
    /// Scale contrast around mid-grey. 1.0 is unchanged; must be > 0.
    Contrast(f32),
    /// Scale saturation. 0.0 is greyscale, 1.0 unchanged; must be >= 0.
    Saturation(f32),
    /// Input black/white points and gamma (1.0 = linear). `black < white`, gamma > 0.
    Levels {
        black: u8,
        white: u8,
        gamma: f32,
    },
    /// Tone curve through control points `(input, output)`, strictly increasing in input.
    Curve(Vec<(u8, u8)>),
}

impl Op {
    /// Validates parameters without touching pixels.
    pub fn validate(&self) -> Result<()> {
        match self {
            Op::Crop { width, height, .. } if *width == 0 || *height == 0 => {
                Err(Error::Invalid("crop size must be > 0".into()))
            }
            Op::Resize { width, height, .. } if *width == 0 || *height == 0 => {
                Err(Error::Invalid("resize dimensions must be > 0".into()))
            }
            Op::RotateArbitrary { degrees, .. } if !degrees.is_finite() => {
                Err(Error::Invalid("rotation angle must be finite".into()))
            }
            Op::Brightness(d) if !(-255..=255).contains(d) => {
                Err(Error::Invalid("brightness delta must be -255..=255".into()))
            }
            Op::Contrast(f) if !(f.is_finite() && *f > 0.0) => {
                Err(Error::Invalid("contrast factor must be > 0".into()))
            }
            Op::Saturation(f) if !(f.is_finite() && *f >= 0.0) => {
                Err(Error::Invalid("saturation factor must be >= 0".into()))
            }
            Op::Levels {
                black,
                white,
                gamma,
            } => {
                if black >= white {
                    return Err(Error::Invalid(
                        "levels: black point must be below white".into(),
                    ));
                }
                if !(gamma.is_finite() && *gamma > 0.0) {
                    return Err(Error::Invalid("levels: gamma must be > 0".into()));
                }
                Ok(())
            }
            Op::Curve(points) => {
                if points.len() < 2 {
                    return Err(Error::Invalid("curve needs at least two points".into()));
                }
                if points.windows(2).any(|w| w[0].0 >= w[1].0) {
                    return Err(Error::Invalid(
                        "curve inputs must be strictly increasing".into(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Applies this operation to `img`, returning the new pixels.
    pub fn apply(&self, img: &RgbaImage) -> Result<RgbaImage> {
        self.validate()?;
        Ok(match self {
            Op::Crop {
                x,
                y,
                width,
                height,
            } => {
                if x.checked_add(*width).is_none_or(|r| r > img.width())
                    || y.checked_add(*height).is_none_or(|b| b > img.height())
                {
                    return Err(Error::Invalid("crop rectangle exceeds image bounds".into()));
                }
                imageops::crop_imm(img, *x, *y, *width, *height).to_image()
            }
            Op::RotateQuarterTurns(n) => match n % 4 {
                0 => img.clone(),
                1 => imageops::rotate90(img),
                2 => imageops::rotate180(img),
                _ => imageops::rotate270(img),
            },
            Op::RotateArbitrary {
                degrees,
                expand,
                filter,
            } => rotate_arbitrary(img, *degrees, *expand, *filter),
            Op::FlipHorizontal => imageops::flip_horizontal(img),
            Op::FlipVertical => imageops::flip_vertical(img),
            Op::Resize {
                width,
                height,
                filter,
            } => imageops::resize(img, *width, *height, filter.to_image()),
            Op::Saturation(factor) => map_saturation(img, *factor),
            Op::Brightness(_) | Op::Contrast(_) | Op::Levels { .. } | Op::Curve(_) => {
                let lut = channel_lut(self);
                let mut out = img.clone();
                for px in out.pixels_mut() {
                    px.0[0] = lut[px.0[0] as usize];
                    px.0[1] = lut[px.0[1] as usize];
                    px.0[2] = lut[px.0[2] as usize];
                }
                out
            }
        })
    }
}

/// Builds a 256-entry lookup table for the per-channel operations.
fn channel_lut(op: &Op) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = i as f32;
        let out = match op {
            Op::Brightness(delta) => v + *delta as f32,
            Op::Contrast(f) => (v - 128.0) * f + 128.0,
            Op::Levels {
                black,
                white,
                gamma,
            } => {
                let span = f32::from(*white) - f32::from(*black);
                let t = ((v - f32::from(*black)) / span).clamp(0.0, 1.0);
                t.powf(1.0 / gamma) * 255.0
            }
            Op::Curve(points) => curve_value(points, v),
            _ => v,
        };
        *slot = out.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Piecewise-linear interpolation through control points, flat beyond the first and last.
fn curve_value(points: &[(u8, u8)], x: f32) -> f32 {
    let (first, last) = (points[0], points[points.len() - 1]);
    if x <= f32::from(first.0) {
        return f32::from(first.1);
    }
    if x >= f32::from(last.0) {
        return f32::from(last.1);
    }
    for w in points.windows(2) {
        let (x0, y0) = (f32::from(w[0].0), f32::from(w[0].1));
        let (x1, y1) = (f32::from(w[1].0), f32::from(w[1].1));
        if x >= x0 && x <= x1 {
            let t = (x - x0) / (x1 - x0);
            return y0 + (y1 - y0) * t;
        }
    }
    f32::from(last.1)
}

fn map_saturation(img: &RgbaImage, factor: f32) -> RgbaImage {
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let [r, g, b, _] = px.0;
        let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
        let grey = 0.299 * r + 0.587 * g + 0.114 * b;
        let mix = |c: f32| (grey + (c - grey) * factor).round().clamp(0.0, 255.0) as u8;
        px.0[0] = mix(r);
        px.0[1] = mix(g);
        px.0[2] = mix(b);
    }
    out
}

/// Bilinear sample with transparent outside the source. Returns premultiplied-free RGBA.
fn sample_bilinear(img: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let mut acc = [0f32; 4];
    for (dy, wy) in [(0i64, 1.0 - fy), (1, fy)] {
        for (dx, wx) in [(0i64, 1.0 - fx), (1, fx)] {
            let px = x0 as i64 + dx;
            let py = y0 as i64 + dy;
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let p = img.get_pixel(px as u32, py as u32).0;
            let weight = wx * wy;
            for c in 0..4 {
                acc[c] += f32::from(p[c]) * weight;
            }
        }
    }
    acc
}

fn rotate_arbitrary(img: &RgbaImage, degrees: f32, expand: bool, filter: Filter) -> RgbaImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let theta = degrees.to_radians();
    let (sin, cos) = theta.sin_cos();

    let (out_w, out_h) = if expand {
        let nw = (w * cos.abs() + h * sin.abs() - 1e-3).ceil().max(1.0);
        let nh = (w * sin.abs() + h * cos.abs() - 1e-3).ceil().max(1.0);
        (nw as u32, nh as u32)
    } else {
        (img.width(), img.height())
    };

    // Clockwise rotation on screen (y grows downward): inverse-map each output pixel back.
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (ocx, ocy) = (out_w as f32 / 2.0, out_h as f32 / 2.0);
    let mut out = RgbaImage::new(out_w, out_h);
    let nearest = filter == Filter::Nearest;

    for oy in 0..out_h {
        for ox in 0..out_w {
            let dx = ox as f32 + 0.5 - ocx;
            let dy = oy as f32 + 0.5 - ocy;
            // Inverse rotation.
            let sx = dx * cos + dy * sin + cx - 0.5;
            let sy = -dx * sin + dy * cos + cy - 0.5;
            let px = if nearest {
                let (ix, iy) = (sx.round(), sy.round());
                if ix < 0.0 || iy < 0.0 || ix >= w || iy >= h {
                    [0.0; 4]
                } else {
                    img.get_pixel(ix as u32, iy as u32).0.map(f32::from)
                }
            } else {
                sample_bilinear(img, sx, sy)
            };
            out.put_pixel(ox, oy, Rgba(px.map(|c| c.round().clamp(0.0, 255.0) as u8)));
        }
    }
    out
}

/// Where a save should write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveTarget {
    /// Write to a new path; the original file is never touched.
    SaveAs(PathBuf),
    /// Replace the original file. Requires explicit confirmation from the user.
    Overwrite { confirmed: bool },
}

/// Maximum number of undo steps kept in memory.
pub const UNDO_LIMIT: usize = 32;

/// A non-destructive editing session over one image.
///
/// The original pixels are kept until the caller saves. Edits are recorded as full snapshots in
/// a bounded undo stack, which keeps undo/redo O(1) at the cost of memory.
#[derive(Debug)]
pub struct EditSession {
    source_path: Option<PathBuf>,
    current: RgbaImage,
    undo: VecDeque<RgbaImage>,
    redo: Vec<RgbaImage>,
}

impl EditSession {
    pub fn new(original: DynamicImage, source_path: Option<PathBuf>) -> Self {
        Self {
            source_path,
            current: original.to_rgba8(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    pub fn current(&self) -> &RgbaImage {
        &self.current
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Applies `op`. On error the session is unchanged.
    pub fn apply(&mut self, op: &Op) -> Result<()> {
        let next = op.apply(&self.current)?;
        self.push_undo();
        self.current = next;
        self.redo.clear();
        Ok(())
    }

    /// Reverts the last change. Returns `false` if there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        let old = std::mem::replace(&mut self.current, prev);
        self.redo.push(old);
        true
    }

    /// Re-applies the last undone change. Returns `false` if there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.push_undo();
        self.current = next;
        true
    }

    fn push_undo(&mut self) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(self.current.clone());
    }

    /// Writes the edited image. Overwriting the original requires `confirmed == true`, and
    /// is refused when the session has no source path.
    pub fn save(&self, target: &SaveTarget, format: ImageFormat, quality: u8) -> Result<PathBuf> {
        let path = match target {
            SaveTarget::SaveAs(path) => path.clone(),
            SaveTarget::Overwrite { confirmed } => {
                if !confirmed {
                    return Err(Error::Invalid(
                        "overwriting the original needs confirmation".into(),
                    ));
                }
                self.source_path.clone().ok_or_else(|| {
                    Error::Invalid("session has no source path to overwrite".into())
                })?
            }
        };
        let bytes = encode(&self.current, format, quality)?;
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}

fn encode(img: &RgbaImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            DynamicImage::ImageRgb8(rgb).write_with_encoder(encoder)?;
        }
        other => DynamicImage::ImageRgba8(img.clone()).write_to(&mut buf, other)?,
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grad(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            Rgba([(x * 10) as u8, (y * 10) as u8, 100, 255])
        })
    }

    #[test]
    fn crop_extracts_region_and_rejects_out_of_bounds() {
        let img = grad(10, 8);
        let out = Op::Crop {
            x: 2,
            y: 3,
            width: 4,
            height: 2,
        }
        .apply(&img)
        .unwrap();
        assert_eq!(out.dimensions(), (4, 2));
        assert_eq!(out.get_pixel(0, 0), img.get_pixel(2, 3));
        assert!(Op::Crop {
            x: 8,
            y: 0,
            width: 4,
            height: 1
        }
        .apply(&img)
        .is_err());
        assert!(Op::Crop {
            x: 0,
            y: 0,
            width: 0,
            height: 1
        }
        .apply(&img)
        .is_err());
    }

    #[test]
    fn quarter_turns_and_flips() {
        let img = grad(3, 2);
        let r = Op::RotateQuarterTurns(1).apply(&img).unwrap();
        assert_eq!(r.dimensions(), (2, 3));
        // Top-left of source goes to top-right after a clockwise turn.
        assert_eq!(r.get_pixel(1, 0), img.get_pixel(0, 0));
        assert_eq!(Op::RotateQuarterTurns(4).apply(&img).unwrap(), img);
        let f = Op::FlipHorizontal.apply(&img).unwrap();
        assert_eq!(f.get_pixel(2, 0), img.get_pixel(0, 0));
        let v = Op::FlipVertical.apply(&img).unwrap();
        assert_eq!(v.get_pixel(0, 1), img.get_pixel(0, 0));
    }

    #[test]
    fn arbitrary_rotation_matches_quarter_turn_at_90_degrees() {
        // Use a uniform-colour interior so resampling noise cannot cause mismatches.
        let img = RgbaImage::from_pixel(20, 10, Rgba([200, 50, 25, 255]));
        let rot = Op::RotateArbitrary {
            degrees: 90.0,
            expand: true,
            filter: Filter::Bilinear,
        }
        .apply(&img)
        .unwrap();
        assert_eq!(rot.dimensions(), (10, 20));
        let centre = rot.get_pixel(5, 10).0;
        assert!((i32::from(centre[0]) - 200).abs() <= 2, "{centre:?}");
    }

    #[test]
    fn arbitrary_rotation_without_expand_keeps_size_and_fills_corners() {
        let img = RgbaImage::from_pixel(20, 20, Rgba([255, 255, 255, 255]));
        let rot = Op::RotateArbitrary {
            degrees: 45.0,
            expand: false,
            filter: Filter::Nearest,
        }
        .apply(&img)
        .unwrap();
        assert_eq!(rot.dimensions(), (20, 20));
        assert_eq!(rot.get_pixel(0, 0).0[3], 0, "corner should be transparent");
        assert_eq!(rot.get_pixel(10, 10).0, [255, 255, 255, 255]);
    }

    #[test]
    fn resize_sets_exact_dimensions() {
        let out = Op::Resize {
            width: 5,
            height: 3,
            filter: Filter::Lanczos3,
        }
        .apply(&grad(40, 40))
        .unwrap();
        assert_eq!(out.dimensions(), (5, 3));
    }

    #[test]
    fn brightness_contrast_levels_pixel_spot_checks() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([100, 100, 100, 77]));
        let b = Op::Brightness(50).apply(&img).unwrap();
        assert_eq!(b.get_pixel(0, 0).0, [150, 150, 150, 77], "alpha untouched");
        let c = Op::Contrast(2.0).apply(&img).unwrap();
        assert_eq!(c.get_pixel(0, 0).0[0], 72); // (100-128)*2+128
        let l = Op::Levels {
            black: 100,
            white: 200,
            gamma: 1.0,
        }
        .apply(&img)
        .unwrap();
        assert_eq!(l.get_pixel(0, 0).0[0], 0);
        let lw = Op::Levels {
            black: 0,
            white: 255,
            gamma: 1.0,
        }
        .apply(&img)
        .unwrap();
        assert_eq!(lw, img);
    }

    #[test]
    fn saturation_zero_is_greyscale() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
        let g = Op::Saturation(0.0).apply(&img).unwrap().get_pixel(0, 0).0;
        assert_eq!(g[0], g[1]);
        assert_eq!(g[1], g[2]);
        assert_eq!(Op::Saturation(1.0).apply(&img).unwrap(), img);
    }

    #[test]
    fn curve_interpolates_and_validates() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        // Map 0->0, 128->64, 255->255: 128 should land at 64.
        let out = Op::Curve(vec![(0, 0), (128, 64), (255, 255)])
            .apply(&img)
            .unwrap();
        assert_eq!(out.get_pixel(0, 0).0[0], 64);
        assert!(Op::Curve(vec![(0, 0)]).validate().is_err());
        assert!(Op::Curve(vec![(10, 0), (10, 5)]).validate().is_err());
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(Op::Brightness(300).validate().is_err());
        assert!(Op::Contrast(0.0).validate().is_err());
        assert!(Op::Saturation(-1.0).validate().is_err());
        assert!(Op::Levels {
            black: 9,
            white: 9,
            gamma: 1.0
        }
        .validate()
        .is_err());
        assert!(Op::Levels {
            black: 0,
            white: 9,
            gamma: 0.0
        }
        .validate()
        .is_err());
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut session = EditSession::new(DynamicImage::ImageRgba8(grad(8, 8)), None);
        let original = session.current().clone();
        session.apply(&Op::FlipHorizontal).unwrap();
        session.apply(&Op::RotateQuarterTurns(1)).unwrap();
        let edited = session.current().clone();
        assert!(session.can_undo());

        assert!(session.undo());
        assert!(session.undo());
        assert_eq!(session.current(), &original);
        assert!(!session.undo());

        assert!(session.redo());
        assert!(session.redo());
        assert_eq!(session.current(), &edited);
        assert!(!session.can_redo());
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let mut session = EditSession::new(DynamicImage::ImageRgba8(grad(4, 4)), None);
        session.apply(&Op::FlipHorizontal).unwrap();
        session.undo();
        assert!(session.can_redo());
        session.apply(&Op::FlipVertical).unwrap();
        assert!(!session.can_redo());
    }

    #[test]
    fn failed_op_leaves_session_unchanged() {
        let mut session = EditSession::new(DynamicImage::ImageRgba8(grad(4, 4)), None);
        let before = session.current().clone();
        assert!(session
            .apply(&Op::Crop {
                x: 9,
                y: 9,
                width: 1,
                height: 1
            })
            .is_err());
        assert_eq!(session.current(), &before);
        assert!(!session.can_undo());
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut session = EditSession::new(DynamicImage::ImageRgba8(grad(2, 2)), None);
        for _ in 0..(UNDO_LIMIT + 10) {
            session.apply(&Op::FlipHorizontal).unwrap();
        }
        let mut undone = 0;
        while session.undo() {
            undone += 1;
        }
        assert_eq!(undone, UNDO_LIMIT);
    }

    #[test]
    fn overwrite_requires_confirmation_and_source_path() {
        let dir = std::env::temp_dir().join(format!("tpt-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("orig.png");
        let session = EditSession::new(DynamicImage::ImageRgba8(grad(4, 4)), Some(src.clone()));

        let err = session.save(
            &SaveTarget::Overwrite { confirmed: false },
            ImageFormat::Png,
            90,
        );
        assert!(err.is_err());
        assert!(!src.exists(), "unconfirmed overwrite must not write");

        session
            .save(
                &SaveTarget::Overwrite { confirmed: true },
                ImageFormat::Png,
                90,
            )
            .unwrap();
        assert!(src.is_file());

        let copy = dir.join("copy.jpg");
        session
            .save(&SaveTarget::SaveAs(copy.clone()), ImageFormat::Jpeg, 85)
            .unwrap();
        assert!(copy.is_file());

        let no_source = EditSession::new(DynamicImage::ImageRgba8(grad(4, 4)), None);
        assert!(no_source
            .save(
                &SaveTarget::Overwrite { confirmed: true },
                ImageFormat::Png,
                90
            )
            .is_err());
    }
}
