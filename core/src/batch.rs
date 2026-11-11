// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Batch conversion and rename job engine (Phase 5).
//!
//! The GUI (Phase 9) and the CLI (Phase 6) both drive [`run`], so there is one code path.
//! Jobs are cancellable between files and report progress after each file.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

use crate::backend::ImageBackend;
use crate::codec::StdBackend;
use crate::error::{Error, Result};
use crate::jpeg_meta;
use crate::png_meta;
use crate::rename::{self, RenameContext};
use crate::scan;
use crate::webp_meta;

/// Output encodings for batch conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Png,
    WebP,
    Tiff,
    Bmp,
    Avif,
}

impl OutputFormat {
    /// Parses a user-facing name such as `jpg`, `tiff` or `webp` (case-insensitive).
    pub fn parse(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "png" => Ok(Self::Png),
            "webp" => Ok(Self::WebP),
            "tif" | "tiff" => Ok(Self::Tiff),
            "bmp" => Ok(Self::Bmp),
            "avif" => Ok(Self::Avif),
            other => Err(Error::Invalid(format!(
                "unsupported output format '{other}'"
            ))),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Avif => "avif",
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
            Self::Tiff => ImageFormat::Tiff,
            Self::Bmp => ImageFormat::Bmp,
            Self::Avif => ImageFormat::Avif,
        }
    }
}

/// How each image is scaled before encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resize {
    /// Keep the original dimensions.
    None,
    /// Fit within a square of this edge length, preserving aspect ratio. Never upscales.
    MaxEdge(u32),
    /// Scale both dimensions by this percentage (e.g. `50.0`). Aspect ratio is preserved.
    Percent(f32),
    /// Force exact dimensions, ignoring aspect ratio.
    Exact { width: u32, height: u32 },
}

/// Whether metadata survives conversion.
///
/// `PreserveExif` applies when the source is a JPEG. JPEG output keeps the EXIF, XMP and IPTC
/// (Photoshop) segments verbatim. PNG output keeps EXIF as an `eXIf` chunk and XMP as an `iTXt`
/// chunk; WebP output keeps them as `EXIF` and `XMP ` chunks. IPTC has no PNG or WebP form and is
/// dropped. TIFF, BMP and AVIF output are stripped. Pixels are left
/// un-rotated whenever metadata is kept, so the copied orientation tag stays correct. Copied values
/// are not rewritten after a resize, so stale width/height fields inside them can disagree with the
/// output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPolicy {
    Strip,
    PreserveExif,
}

/// A complete batch conversion request.
#[derive(Debug, Clone)]
pub struct BatchJob {
    /// Image files and/or directories (directories are scanned non-recursively).
    pub inputs: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub format: OutputFormat,
    /// JPEG quality, 1..=100. Ignored by lossless formats.
    pub quality: u8,
    pub resize: Resize,
    pub metadata: MetadataPolicy,
    /// Rename pattern; see [`crate::rename`]. Defaults to `{name}`.
    pub rename: String,
    /// Value of `{counter}` for the first input.
    pub start_counter: u32,
    /// When false, existing output files are skipped rather than replaced.
    pub overwrite: bool,
    /// Upper bound on the estimated working memory for one file, in bytes. Raster, HEIC and RAW
    /// files are checked from their headers before decoding. Defaults to
    /// [`DEFAULT_MEMORY_BUDGET`].
    pub memory_budget_bytes: u64,
}

/// Default per-file working-memory budget: 1 GiB.
pub const DEFAULT_MEMORY_BUDGET: u64 = 1 << 30;

impl BatchJob {
    /// A job with sensible defaults: quality 90, no resize, stripped metadata, `{name}` naming.
    pub fn new(inputs: Vec<PathBuf>, output_dir: impl Into<PathBuf>, format: OutputFormat) -> Self {
        Self {
            inputs,
            output_dir: output_dir.into(),
            format,
            quality: 90,
            resize: Resize::None,
            metadata: MetadataPolicy::Strip,
            rename: "{name}".into(),
            start_counter: 1,
            overwrite: false,
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET,
        }
    }
}

/// What happened to one input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Converted {
        output: PathBuf,
    },
    Skipped {
        reason: String,
    },
    Failed {
        error: String,
    },
    /// The job was cancelled before this file was processed.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub input: PathBuf,
    pub outcome: Outcome,
}

/// Progress snapshot emitted after each file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

/// Runs `job`, calling `progress` after each file and checking `cancel` before each one.
///
/// Job-level problems (bad settings, unreadable directory, uncreatable output folder) return
/// `Err`. Per-file problems are reported as [`Outcome::Failed`] and do not stop the batch.
pub fn run(
    job: &BatchJob,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Vec<FileReport>> {
    validate(job)?;
    std::fs::create_dir_all(&job.output_dir)?;
    let files = expand_inputs(&job.inputs)?;
    let total = files.len();

    let mut reports = Vec::with_capacity(total);
    for (index, input) in files.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            reports.push(FileReport {
                input,
                outcome: Outcome::Cancelled,
            });
            continue;
        }
        let counter = job.start_counter.saturating_add(index as u32);
        let outcome = match process_one(job, &input, counter) {
            Ok(outcome) => outcome,
            Err(err) => Outcome::Failed {
                error: err.to_string(),
            },
        };
        reports.push(FileReport { input, outcome });
        progress(Progress {
            done: index + 1,
            total,
        });
    }
    Ok(reports)
}

fn validate(job: &BatchJob) -> Result<()> {
    if !(1..=100).contains(&job.quality) {
        return Err(Error::Invalid(format!(
            "quality must be 1..=100, got {}",
            job.quality
        )));
    }
    match job.resize {
        Resize::MaxEdge(0) => return Err(Error::Invalid("max edge must be > 0".into())),
        Resize::Percent(p) if !(p.is_finite() && p > 0.0) => {
            return Err(Error::Invalid("percent must be a positive number".into()))
        }
        Resize::Exact { width, height } if width == 0 || height == 0 => {
            return Err(Error::Invalid("exact dimensions must be > 0".into()))
        }
        _ => {}
    }
    if job.inputs.is_empty() {
        return Err(Error::Invalid("no input files given".into()));
    }
    if job.memory_budget_bytes == 0 {
        return Err(Error::Invalid("memory budget must be > 0".into()));
    }
    // Reject a bad pattern before touching any file.
    rename::render(
        &job.rename,
        &RenameContext {
            stem: "x",
            counter: job.start_counter,
            date: None,
            camera: None,
        },
    )?;
    Ok(())
}

fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            files.extend(scan::scan_dir(input)?.into_iter().map(|e| e.path));
        } else {
            files.push(input.clone());
        }
    }
    Ok(files)
}

fn process_one(job: &BatchJob, input: &Path, counter: u32) -> Result<Outcome> {
    // RAW files are developed through rawler; they have no JPEG segments to preserve.
    let (pixels, meta, bytes) = if crate::raw::is_raw_path(input) {
        let (width, height) = crate::raw::probe_dimensions(input)?;
        check_memory_budget(job.memory_budget_bytes, width, height)?;
        let developed = crate::raw::load_path(input)?;
        (developed.image, developed.metadata, None)
    } else if crate::heic::is_heic_path(input) {
        let bytes = std::fs::read(input)?;
        let (width, height) = crate::heic::probe_dimensions(&bytes)?;
        check_memory_budget(job.memory_budget_bytes, width, height)?;
        let decoded = crate::heic::decode_heic_bytes(&bytes)?;
        (decoded.image, decoded.metadata, None)
    } else {
        let bytes = std::fs::read(input)?;
        let backend = StdBackend;
        let info = backend.probe(&bytes)?;
        check_memory_budget(job.memory_budget_bytes, info.width, info.height)?;
        let meta = backend.read_metadata(&bytes)?;
        // Keeping metadata means keeping the orientation tag, so pixels must not be rotated here.
        let carry = carries_metadata(job, &bytes);
        let pixels = if carry {
            backend.decode_pixels(&bytes)?
        } else {
            backend.decode(&bytes)?.image
        };
        (pixels, meta, carry.then_some(bytes))
    };
    let pixels = resize(pixels, job.resize);

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let name = rename::render(
        &job.rename,
        &RenameContext {
            stem,
            counter,
            date: meta.date_time_original.as_deref(),
            camera: meta.model.as_deref(),
        },
    )?;
    let output = job
        .output_dir
        .join(format!("{name}.{}", job.format.extension()));
    if output.exists() && !job.overwrite {
        return Ok(Outcome::Skipped {
            reason: format!("{} already exists", output.display()),
        });
    }

    let mut encoded = encode(&pixels, job.format, job.quality)?;
    if let Some(source) = bytes.as_deref() {
        encoded = match job.format {
            OutputFormat::Jpeg => {
                let segments = jpeg_meta::preserved_segments(source);
                jpeg_meta::insert_after_soi(&encoded, &segments).unwrap_or(encoded)
            }
            OutputFormat::Png => {
                let mut chunks = Vec::new();
                if let Some(tiff) = jpeg_meta::exif_tiff(source) {
                    chunks.push(png_meta::chunk(b"eXIf", tiff));
                }
                if let Some(xmp) = jpeg_meta::xmp_packet(source) {
                    chunks.push(png_meta::chunk(b"iTXt", &png_meta::xmp_itxt_body(xmp)));
                }
                png_meta::insert_after_ihdr(&encoded, &chunks).unwrap_or(encoded)
            }
            OutputFormat::WebP => {
                let exif = jpeg_meta::exif_tiff(source);
                let xmp = jpeg_meta::xmp_packet(source);
                webp_meta::with_metadata(&encoded, pixels.width(), pixels.height(), exif, xmp)
                    .unwrap_or(encoded)
            }
            _ => encoded,
        };
    }

    write_atomic(&output, &encoded)?;
    Ok(Outcome::Converted { output })
}

/// Whether this file's metadata is carried into the output: the policy asks for it, the source is
/// a JPEG, and the output format can hold it (JPEG or PNG). Pixels are then left un-rotated so the
/// copied orientation tag stays correct.
fn carries_metadata(job: &BatchJob, source: &[u8]) -> bool {
    job.metadata == MetadataPolicy::PreserveExif
        && source.starts_with(&[0xFF, 0xD8])
        && matches!(
            job.format,
            OutputFormat::Jpeg | OutputFormat::Png | OutputFormat::WebP
        )
}

/// Rejects an image whose decode, resize and encode working set would exceed `budget`. The peak
/// is approximated as the decoded RGBA8 buffer plus one working copy.
fn check_memory_budget(budget: u64, width: u32, height: u32) -> Result<()> {
    let needed = u64::from(width) * u64::from(height) * 4 * 2;
    if needed > budget {
        return Err(Error::Invalid(format!(
            "{width}x{height} image needs about {} MiB of working memory, over the {} MiB budget",
            needed >> 20,
            budget >> 20
        )));
    }
    Ok(())
}

fn resize(img: DynamicImage, spec: Resize) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let (nw, nh) = match spec {
        Resize::None => return img,
        Resize::MaxEdge(edge) => {
            let longest = w.max(h);
            if longest <= edge {
                return img;
            }
            let scale = f64::from(edge) / f64::from(longest);
            (scale_dim(w, scale), scale_dim(h, scale))
        }
        Resize::Percent(p) => {
            let scale = f64::from(p) / 100.0;
            (scale_dim(w, scale), scale_dim(h, scale))
        }
        Resize::Exact { width, height } => (width, height),
    };
    if (nw, nh) == (w, h) {
        return img;
    }
    img.resize_exact(nw, nh, FilterType::Lanczos3)
}

fn scale_dim(dim: u32, scale: f64) -> u32 {
    ((f64::from(dim) * scale).round() as u32).max(1)
}

fn encode(img: &DynamicImage, format: OutputFormat, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        OutputFormat::Jpeg => {
            // JPEG has no alpha channel; flatten to RGB first.
            let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            rgb.write_with_encoder(encoder)?;
        }
        OutputFormat::Avif => {
            // Speed 6 is the encoder default: a balance of size and time. Quality maps directly.
            let encoder =
                image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut buf, 6, quality);
            img.write_with_encoder(encoder)?;
        }
        other => img.write_to(&mut buf, other.image_format())?,
    }
    Ok(buf.into_inner())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp-tpt");
    std::fs::write(&tmp, bytes)?;
    // `rename` replaces an existing destination on both Windows and Unix.
    std::fs::rename(&tmp, path)?;
    Ok(())
}
