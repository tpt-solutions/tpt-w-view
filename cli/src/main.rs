// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! `tpt-view-cli`: headless batch conversion for scripts and automation (Phase 6).
//!
//! Exit codes:
//! - `0` every file converted or intentionally skipped
//! - `1` at least one file failed (the rest of the batch still ran)
//! - `2` usage error (bad arguments)
//! - `3` job-level error (unreadable input directory, uncreatable output folder, ...)
//! - `130` interrupted by Ctrl+C (the conventional SIGINT status)

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tpt_view_core::batch::{
    self, BatchJob, FileReport, MetadataPolicy, Outcome, OutputFormat, Resize,
};

const USAGE: &str = "\
tpt-view-cli: batch image conversion

USAGE:
    tpt-view-cli convert <INPUT>... --out <DIR> [OPTIONS]

INPUT may be image files or directories (scanned, not recursive).

OPTIONS:
    -o, --out <DIR>          Output directory (required)
    -f, --format <FMT>       jpg | png | webp | tiff | bmp | avif   [default: jpg]
    -q, --quality <1-100>    JPEG quality                    [default: 90]
        --max-edge <PX>      Fit within a PX x PX box (never upscales)
        --percent <P>        Scale by P percent
        --size <WxH>         Force exact dimensions
    -r, --rename <PATTERN>   Rename pattern, e.g. \"img_{counter:03}\"  [default: {name}]
        --start <N>          First value of {counter}         [default: 1]
        --preserve-exif      Keep metadata from JPEG sources (JPEG, PNG or WebP output); default strips
        --overwrite          Replace existing outputs instead of skipping them
        --json               Print a machine-readable JSON report on stdout
    -h, --help               Show this help
    -V, --version            Show the version

Resize flags are mutually exclusive.";

const EXIT_OK: u8 = 0;
const EXIT_FILE_FAILURES: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_JOB_ERROR: u8 = 3;
const EXIT_INTERRUPTED: u8 = 130;

#[derive(Debug, PartialEq)]
struct ConvertArgs {
    inputs: Vec<std::path::PathBuf>,
    out: std::path::PathBuf,
    format: OutputFormat,
    quality: u8,
    resize: Resize,
    rename: String,
    start: u32,
    preserve_exif: bool,
    overwrite: bool,
    json: bool,
}

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Convert(ConvertArgs),
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter().peekable();
    let Some(first) = iter.next() else {
        return Err("missing command; try --help".into());
    };
    match first.as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "-V" | "--version" | "version" => return Ok(Command::Version),
        "convert" => {}
        other => return Err(format!("unknown command '{other}'")),
    }

    let mut inputs = Vec::new();
    let mut out = None;
    let mut format = OutputFormat::Jpeg;
    let mut quality = 90u8;
    let mut resize_flags = Vec::new();
    let mut rename = "{name}".to_string();
    let mut start = 1u32;
    let mut preserve_exif = false;
    let mut overwrite = false;
    let mut json = false;

    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| -> Result<String, String> {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match arg.as_str() {
            "-o" | "--out" => out = Some(value(arg)?),
            "-f" | "--format" => {
                format = OutputFormat::parse(&value(arg)?).map_err(|e| e.to_string())?
            }
            "-q" | "--quality" => {
                let raw = value(arg)?;
                quality = raw
                    .parse::<u8>()
                    .ok()
                    .filter(|q| (1..=100).contains(q))
                    .ok_or_else(|| format!("quality must be 1-100, got '{raw}'"))?;
            }
            "--max-edge" => resize_flags.push(Resize::MaxEdge(parse_u32(arg, &value(arg)?)?)),
            "--percent" => {
                let raw = value(arg)?;
                let p = raw
                    .parse::<f32>()
                    .map_err(|_| format!("--percent expects a number, got '{raw}'"))?;
                resize_flags.push(Resize::Percent(p));
            }
            "--size" => {
                let raw = value(arg)?;
                let (w, h) = raw
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--size expects WxH, got '{raw}'"))?;
                resize_flags.push(Resize::Exact {
                    width: parse_u32("--size width", w)?,
                    height: parse_u32("--size height", h)?,
                });
            }
            "-r" | "--rename" => rename = value(arg)?,
            "--start" => start = parse_u32(arg, &value(arg)?)?,
            "--preserve-exif" => preserve_exif = true,
            "--overwrite" => overwrite = true,
            "--json" => json = true,
            "-h" | "--help" => return Ok(Command::Help),
            flag if flag.starts_with('-') => return Err(format!("unknown option '{flag}'")),
            path => inputs.push(path.into()),
        }
    }

    if resize_flags.len() > 1 {
        return Err("--max-edge, --percent and --size are mutually exclusive".into());
    }
    if inputs.is_empty() {
        return Err("no input files or directories given".into());
    }
    let out = out.ok_or("--out <DIR> is required")?;

    Ok(Command::Convert(ConvertArgs {
        inputs,
        out: out.into(),
        format,
        quality,
        resize: resize_flags.pop().unwrap_or(Resize::None),
        rename,
        start,
        preserve_exif,
        overwrite,
        json,
    }))
}

fn parse_u32(flag: &str, raw: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{flag} expects a positive integer, got '{raw}'"))
}

/// Escapes a string for inclusion in a JSON document.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn report_json(reports: &[FileReport]) -> String {
    let mut converted = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    let mut rows = Vec::with_capacity(reports.len());

    for r in reports {
        let input = json_string(&r.input.to_string_lossy());
        let row = match &r.outcome {
            Outcome::Converted { output } => {
                converted += 1;
                format!(
                    "{{\"input\":{input},\"status\":\"converted\",\"output\":{}}}",
                    json_string(&output.to_string_lossy())
                )
            }
            Outcome::Skipped { reason } => {
                skipped += 1;
                format!(
                    "{{\"input\":{input},\"status\":\"skipped\",\"reason\":{}}}",
                    json_string(reason)
                )
            }
            Outcome::Failed { error } => {
                failed += 1;
                format!(
                    "{{\"input\":{input},\"status\":\"failed\",\"error\":{}}}",
                    json_string(error)
                )
            }
            Outcome::Cancelled => {
                cancelled += 1;
                format!("{{\"input\":{input},\"status\":\"cancelled\"}}")
            }
        };
        rows.push(row);
    }

    format!(
        "{{\"summary\":{{\"total\":{},\"converted\":{converted},\"skipped\":{skipped},\"failed\":{failed},\"cancelled\":{cancelled}}},\"files\":[{}]}}",
        reports.len(),
        rows.join(",")
    )
}

fn run_convert(args: ConvertArgs) -> ExitCode {
    let job = BatchJob {
        inputs: args.inputs,
        output_dir: args.out,
        format: args.format,
        quality: args.quality,
        resize: args.resize,
        metadata: if args.preserve_exif {
            MetadataPolicy::PreserveExif
        } else {
            MetadataPolicy::Strip
        },
        rename: args.rename,
        start_counter: args.start,
        overwrite: args.overwrite,
        memory_budget_bytes: batch::DEFAULT_MEMORY_BUDGET,
    };

    // Ctrl+C stops the batch between files; files not yet started are reported as cancelled.
    let cancel = Arc::new(AtomicBool::new(false));
    let handler_flag = Arc::clone(&cancel);
    if let Err(err) = ctrlc::set_handler(move || handler_flag.store(true, Ordering::Relaxed)) {
        eprintln!("warning: could not install Ctrl+C handler: {err}");
    }
    let json = args.json;
    let result = batch::run(&job, &cancel, |p| {
        if !json {
            eprintln!("[{}/{}]", p.done, p.total);
        }
    });

    let reports = match result {
        Ok(reports) => reports,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(EXIT_JOB_ERROR);
        }
    };

    if json {
        println!("{}", report_json(&reports));
    } else {
        for r in &reports {
            match &r.outcome {
                Outcome::Converted { output } => println!("ok      {}", output.display()),
                Outcome::Skipped { reason } => println!("skipped {reason}"),
                Outcome::Failed { error } => eprintln!("failed  {}: {error}", r.input.display()),
                Outcome::Cancelled => eprintln!("cancelled {}", r.input.display()),
            }
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return ExitCode::from(EXIT_INTERRUPTED);
    }
    let any_failed = reports
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Failed { .. } | Outcome::Cancelled));
    if any_failed {
        ExitCode::from(EXIT_FILE_FAILURES)
    } else {
        ExitCode::from(EXIT_OK)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::from(EXIT_OK)
        }
        Ok(Command::Version) => {
            println!("tpt-view-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::from(EXIT_OK)
        }
        Ok(Command::Convert(convert)) => run_convert(convert),
        Err(msg) => {
            eprintln!("error: {msg}\n\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_full_convert_command() {
        let cmd = parse_args(&args(
            "convert a.png dir --out out -f webp -q 75 --max-edge 512 -r img_{counter:03} --start 5 --preserve-exif --json",
        ))
        .unwrap();
        let Command::Convert(c) = cmd else {
            panic!("expected convert")
        };
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(c.format, OutputFormat::WebP);
        assert_eq!(c.quality, 75);
        assert_eq!(c.resize, Resize::MaxEdge(512));
        assert_eq!(c.rename, "img_{counter:03}");
        assert_eq!(c.start, 5);
        assert!(c.preserve_exif && c.json && !c.overwrite);
    }

    #[test]
    fn rejects_bad_usage() {
        assert!(parse_args(&args("")).is_err());
        assert!(parse_args(&args("frobnicate")).is_err());
        assert!(
            parse_args(&args("convert a.png")).is_err(),
            "--out is required"
        );
        assert!(
            parse_args(&args("convert --out o")).is_err(),
            "inputs required"
        );
        assert!(parse_args(&args("convert a --out o -q 0")).is_err());
        assert!(parse_args(&args("convert a --out o --max-edge 10 --percent 50")).is_err());
        assert!(parse_args(&args("convert a --out o --size 10by10")).is_err());
        assert!(parse_args(&args("convert a --out o --bogus")).is_err());
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_args(&args("--help")).unwrap(), Command::Help);
        assert_eq!(parse_args(&args("version")).unwrap(), Command::Version);
    }

    #[test]
    fn json_escaping_is_valid() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }
}
