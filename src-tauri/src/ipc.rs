// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! IPC boundary: input validation and wire types (Phase 7).
//!
//! Every path the frontend sends is checked here before any file is touched. Paths must be
//! absolute, free of `..` components and NUL bytes, and point at the expected kind of entry.
//! Paths normally originate from the native file dialog or the launch argument, so this is a
//! sanity boundary rather than the only access control.

use std::path::{Component, PathBuf};

use serde::{Deserialize, Serialize};
use tpt_view_core::batch::{BatchJob, FileReport, MetadataPolicy, Outcome, OutputFormat, Resize};
use tpt_view_core::edit::{Filter, Op};
use tpt_view_core::scan::{self, FileEntry};

/// Output formats the UI may request.
pub const OUTPUT_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp", "avif"];

/// Checks the textual form of a path: absolute, no `..`, no NUL.
pub fn validate_path_text(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() || raw.contains('\0') {
        return Err("empty or invalid path".into());
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(format!("path must be absolute: {raw}"));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("'..' components are not allowed".into());
    }
    Ok(path)
}

/// An existing, supported image file.
pub fn validate_image_file(raw: &str) -> Result<PathBuf, String> {
    let path = validate_path_text(raw)?;
    if !path.is_file() {
        return Err(format!("not a file: {raw}"));
    }
    if !scan::is_supported(&path) {
        return Err(format!("unsupported image type: {raw}"));
    }
    Ok(path)
}

/// An existing directory.
pub fn validate_directory(raw: &str) -> Result<PathBuf, String> {
    let path = validate_path_text(raw)?;
    if !path.is_dir() {
        return Err(format!("not a directory: {raw}"));
    }
    Ok(path)
}

/// A save destination: parent directory must exist and the extension must be a supported output.
pub fn validate_output_file(raw: &str) -> Result<PathBuf, String> {
    let path = validate_path_text(raw)?;
    let parent = path.parent().ok_or("output path has no parent directory")?;
    if !parent.is_dir() {
        return Err(format!(
            "output folder does not exist: {}",
            parent.display()
        ));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !OUTPUT_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!("unsupported output extension: '{ext}'"));
    }
    Ok(path)
}

/// Returns the first launch argument that names an image or directory, if any.
pub fn initial_path_from_args(args: impl IntoIterator<Item = String>) -> Option<String> {
    args.into_iter()
        .skip(1)
        .find(|arg| validate_image_file(arg).is_ok() || validate_directory(arg).is_ok())
}

// ---- wire types ----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EntryDto {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: u64,
}

impl From<FileEntry> for EntryDto {
    fn from(e: FileEntry) -> Self {
        EntryDto {
            name: e
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: e.path.to_string_lossy().into_owned(),
            size: e.size,
            modified_ms: (e.modified_nanos / 1_000_000) as u64,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MetadataDto {
    pub orientation: u32,
    pub make: Option<String>,
    pub model: Option<String>,
    pub date_time_original: Option<String>,
    /// `[latitude, longitude]` in signed decimal degrees.
    pub gps: Option<[f64; 2]>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct InfoDto {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub metadata: MetadataDto,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterDto {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos3,
}

impl From<FilterDto> for Filter {
    fn from(f: FilterDto) -> Self {
        match f {
            FilterDto::Nearest => Filter::Nearest,
            FilterDto::Bilinear => Filter::Bilinear,
            FilterDto::Bicubic => Filter::Bicubic,
            FilterDto::Lanczos3 => Filter::Lanczos3,
        }
    }
}

/// Edit operation as sent by the UI, tagged by `kind` (snake_case).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpDto {
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    RotateQuarterTurns {
        turns: u8,
    },
    RotateArbitrary {
        degrees: f32,
        expand: bool,
        filter: FilterDto,
    },
    FlipHorizontal,
    FlipVertical,
    Resize {
        width: u32,
        height: u32,
        filter: FilterDto,
    },
    Brightness {
        delta: i32,
    },
    Contrast {
        factor: f32,
    },
    Saturation {
        factor: f32,
    },
    Levels {
        black: u8,
        white: u8,
        gamma: f32,
    },
    Curve {
        points: Vec<(u8, u8)>,
    },
}

impl From<OpDto> for Op {
    fn from(dto: OpDto) -> Self {
        match dto {
            OpDto::Crop {
                x,
                y,
                width,
                height,
            } => Op::Crop {
                x,
                y,
                width,
                height,
            },
            OpDto::RotateQuarterTurns { turns } => Op::RotateQuarterTurns(turns),
            OpDto::RotateArbitrary {
                degrees,
                expand,
                filter,
            } => Op::RotateArbitrary {
                degrees,
                expand,
                filter: filter.into(),
            },
            OpDto::FlipHorizontal => Op::FlipHorizontal,
            OpDto::FlipVertical => Op::FlipVertical,
            OpDto::Resize {
                width,
                height,
                filter,
            } => Op::Resize {
                width,
                height,
                filter: filter.into(),
            },
            OpDto::Brightness { delta } => Op::Brightness(delta),
            OpDto::Contrast { factor } => Op::Contrast(factor),
            OpDto::Saturation { factor } => Op::Saturation(factor),
            OpDto::Levels {
                black,
                white,
                gamma,
            } => Op::Levels {
                black,
                white,
                gamma,
            },
            OpDto::Curve { points } => Op::Curve(points),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResizeDto {
    None,
    MaxEdge { px: u32 },
    Percent { percent: f32 },
    Exact { width: u32, height: u32 },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SaveDto {
    SaveAs { path: String },
    Overwrite { confirmed: bool },
}

#[derive(Debug, Deserialize)]
pub struct BatchDto {
    pub inputs: Vec<String>,
    pub output_dir: String,
    pub format: String,
    pub quality: u8,
    pub resize: ResizeDto,
    pub preserve_exif: bool,
    pub rename: String,
    pub start_counter: u32,
    pub overwrite: bool,
}

/// Validates a batch request and converts it into a core [`BatchJob`].
pub fn to_batch_job(dto: BatchDto) -> Result<BatchJob, String> {
    let inputs = dto
        .inputs
        .iter()
        .map(|raw| {
            validate_image_file(raw)
                .or_else(|_| validate_directory(raw))
                .map_err(|e| format!("input rejected: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output_dir = validate_directory(&dto.output_dir).or_else(|_| {
        // The output folder may not exist yet; the engine creates it, so validate its text only.
        validate_path_text(&dto.output_dir)
    })?;
    let format = OutputFormat::parse(&dto.format).map_err(|e| e.to_string())?;
    let resize = match dto.resize {
        ResizeDto::None => Resize::None,
        ResizeDto::MaxEdge { px } => Resize::MaxEdge(px),
        ResizeDto::Percent { percent } => Resize::Percent(percent),
        ResizeDto::Exact { width, height } => Resize::Exact { width, height },
    };
    let mut job = BatchJob::new(inputs, output_dir, format);
    job.quality = dto.quality;
    job.resize = resize;
    job.metadata = if dto.preserve_exif {
        MetadataPolicy::PreserveExif
    } else {
        MetadataPolicy::Strip
    };
    job.rename = dto.rename;
    job.start_counter = dto.start_counter;
    job.overwrite = dto.overwrite;
    Ok(job)
}

#[derive(Debug, Serialize)]
pub struct ReportDto {
    pub input: String,
    pub status: &'static str,
    pub output: Option<String>,
    pub message: Option<String>,
}

pub fn report_dto(report: &FileReport) -> ReportDto {
    let input = report.input.to_string_lossy().into_owned();
    match &report.outcome {
        Outcome::Converted { output } => ReportDto {
            input,
            status: "converted",
            output: Some(output.to_string_lossy().into_owned()),
            message: None,
        },
        Outcome::Skipped { reason } => ReportDto {
            input,
            status: "skipped",
            output: None,
            message: Some(reason.clone()),
        },
        Outcome::Failed { error } => ReportDto {
            input,
            status: "failed",
            output: None,
            message: Some(error.clone()),
        },
        Outcome::Cancelled => ReportDto {
            input,
            status: "cancelled",
            output: None,
            message: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_text_rules() {
        assert!(validate_path_text("").is_err());
        assert!(validate_path_text("relative/x.png").is_err());
        assert!(validate_path_text("C:\\a\\..\\b.png").is_err());
        assert!(validate_path_text("/tmp/x/../y.png").is_err());
        let ok = std::env::temp_dir();
        assert!(validate_path_text(&ok.to_string_lossy()).is_ok());
    }

    #[test]
    fn image_and_directory_validation() {
        let dir = std::env::temp_dir();
        let txt = dir.join(format!("tpt-ipc-{}.txt", std::process::id()));
        std::fs::write(&txt, b"x").unwrap();
        assert!(
            validate_image_file(&txt.to_string_lossy()).is_err(),
            "txt is not an image"
        );
        assert!(validate_directory(&txt.to_string_lossy()).is_err());
        assert!(validate_directory(&dir.to_string_lossy()).is_ok());
        let _ = std::fs::remove_file(&txt);
    }

    #[test]
    fn output_extension_and_parent_checked() {
        let dir = std::env::temp_dir();
        let good = dir.join("out.webp");
        assert!(validate_output_file(&good.to_string_lossy()).is_ok());
        assert!(validate_output_file(&dir.join("out.exe").to_string_lossy()).is_err());
        let missing = dir.join("no-such-folder-tpt").join("x.png");
        assert!(validate_output_file(&missing.to_string_lossy()).is_err());
    }

    #[test]
    fn op_dto_deserializes_from_ui_json() {
        let op: OpDto = serde_json::from_str(
            r#"{"kind":"rotate_arbitrary","degrees":12.5,"expand":true,"filter":"bicubic"}"#,
        )
        .unwrap();
        let op: Op = op.into();
        assert_eq!(
            op,
            Op::RotateArbitrary {
                degrees: 12.5,
                expand: true,
                filter: Filter::Bicubic
            }
        );
        let curve: OpDto =
            serde_json::from_str(r#"{"kind":"curve","points":[[0,0],[255,255]]}"#).unwrap();
        assert!(matches!(Op::from(curve), Op::Curve(p) if p.len() == 2));
        assert!(serde_json::from_str::<OpDto>(r#"{"kind":"unknown"}"#).is_err());
    }

    #[test]
    fn launch_argument_selection_skips_bad_entries() {
        let args = vec![
            "tpt-view".to_string(),
            "--flag".to_string(),
            "relative.png".to_string(),
        ];
        assert_eq!(initial_path_from_args(args), None);
    }

    #[test]
    fn batch_dto_rejects_traversal_and_bad_format() {
        let base = |inputs: Vec<String>, format: &str| BatchDto {
            inputs,
            output_dir: std::env::temp_dir().to_string_lossy().into_owned(),
            format: format.into(),
            quality: 90,
            resize: ResizeDto::None,
            preserve_exif: false,
            rename: "{name}".into(),
            start_counter: 1,
            overwrite: false,
        };
        assert!(to_batch_job(base(vec!["..\\secret.png".into()], "png")).is_err());
        assert!(
            to_batch_job(base(vec![], "gif")).is_err(),
            "unknown format rejected"
        );
        // An empty input list is accepted here; the core engine rejects it when the job runs.
        assert!(to_batch_job(base(vec![], "png")).is_ok());
    }
}
