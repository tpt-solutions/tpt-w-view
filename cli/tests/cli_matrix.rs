// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Black-box CLI matrix: runs the real binary across formats, resize modes and error paths and
//! checks exit codes and produced files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use image::{DynamicImage, Rgba, RgbaImage};

const BIN: &str = env!("CARGO_BIN_EXE_tpt-view-cli");

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_source(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let path = dir.join(name);
    let img = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255])
    });
    DynamicImage::ImageRgba8(img).save(&path).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().expect("spawn cli")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn dims(path: &Path) -> (u32, u32) {
    image::image_dimensions(path).unwrap()
}

#[test]
fn every_output_format_is_written_with_correct_extension() {
    let root = workdir("formats");
    let src = make_source(&root, "pic.png", 64, 32);
    for (flag, ext) in [
        ("jpg", "jpg"),
        ("jpeg", "jpg"),
        ("png", "png"),
        ("webp", "webp"),
        ("tiff", "tiff"),
        ("bmp", "bmp"),
        ("avif", "avif"),
    ] {
        let out_dir = root.join(format!("out-{flag}"));
        let out = run(&[
            "convert",
            src.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--format",
            flag,
        ]);
        assert_eq!(
            code(&out),
            0,
            "format {flag}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let produced = out_dir.join(format!("pic.{ext}"));
        assert_eq!(dims(&produced), (64, 32), "format {flag}");
    }
}

#[test]
fn resize_modes_produce_expected_dimensions() {
    let root = workdir("resize");
    let src_dir = root.join("in");
    fs::create_dir_all(&src_dir).unwrap();
    make_source(&src_dir, "wide.png", 400, 200);

    let cases: [(&[&str], (u32, u32)); 3] = [
        (&["--max-edge", "100"], (100, 50)),
        (&["--percent", "25"], (100, 50)),
        (&["--size", "37x11"], (37, 11)),
    ];
    for (i, (flags, expected)) in cases.iter().enumerate() {
        let out_dir = root.join(format!("out{i}"));
        let mut args = vec![
            "convert",
            src_dir.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--format",
            "png",
        ];
        args.extend_from_slice(flags);
        let out = run(&args);
        assert_eq!(
            code(&out),
            0,
            "{flags:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(dims(&out_dir.join("wide.png")), *expected, "{flags:?}");
    }
}

#[test]
fn rename_pattern_and_start_counter_apply_per_file() {
    let root = workdir("rename");
    let src_dir = root.join("in");
    fs::create_dir_all(&src_dir).unwrap();
    make_source(&src_dir, "a.png", 8, 8);
    make_source(&src_dir, "b.png", 8, 8);
    let out_dir = root.join("out");
    let out = run(&[
        "convert",
        src_dir.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--format",
        "png",
        "--rename",
        "shot_{counter:03}_{name:upper}",
        "--start",
        "10",
    ]);
    assert_eq!(code(&out), 0);
    assert!(out_dir.join("shot_010_A.png").is_file());
    assert!(out_dir.join("shot_011_B.png").is_file());
}

#[test]
fn overwrite_flag_controls_existing_outputs() {
    let root = workdir("overwrite");
    let src = make_source(&root, "o.png", 8, 8);
    let out_dir = root.join("out");
    let base = [
        "convert",
        src.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--format",
        "png",
    ];

    assert_eq!(code(&run(&base)), 0);
    // Second run without --overwrite skips the existing file, which still counts as success.
    let skipped = run(&base);
    assert_eq!(code(&skipped), 0);
    assert!(String::from_utf8_lossy(&skipped.stdout).contains("skipped"));

    let mut forced: Vec<&str> = base.to_vec();
    forced.push("--overwrite");
    assert_eq!(code(&run(&forced)), 0);
}

#[test]
fn json_report_describes_every_file() {
    let root = workdir("json");
    let src_dir = root.join("in");
    fs::create_dir_all(&src_dir).unwrap();
    make_source(&src_dir, "good.png", 8, 8);
    fs::write(src_dir.join("bad.png"), b"not really a png").unwrap();
    let out_dir = root.join("out");

    let out = run(&[
        "convert",
        src_dir.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--format",
        "png",
        "--json",
    ]);
    assert_eq!(code(&out), 1, "one failed file means exit 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("{\"summary\""), "{stdout}");
    assert!(stdout.contains("\"total\":2"));
    assert!(stdout.contains("\"converted\":1"));
    assert!(stdout.contains("\"failed\":1"));
    assert!(stdout.contains("\"status\":\"failed\""));
}

#[test]
fn corrupt_file_alone_exits_one_and_keeps_going() {
    let root = workdir("corrupt");
    let bad = root.join("bad.jpg");
    fs::write(&bad, b"\xFF\xD8garbage").unwrap();
    let out = run(&[
        "convert",
        bad.to_str().unwrap(),
        "--out",
        root.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("failed"));
}

#[test]
fn usage_errors_exit_two() {
    let root = workdir("usage");
    let out_dir = root.join("out");
    let out = out_dir.to_str().unwrap();
    assert_eq!(code(&run(&[])), 2);
    assert_eq!(code(&run(&["bogus-command"])), 2);
    assert_eq!(code(&run(&["convert", "x.png"])), 2, "--out missing");
    assert_eq!(
        code(&run(&["convert", "x.png", "--out", out, "--format", "gif"])),
        2
    );
    assert_eq!(
        code(&run(&["convert", "x.png", "--out", out, "-q", "0"])),
        2
    );
    assert_eq!(
        code(&run(&[
            "convert",
            "x.png",
            "--out",
            out,
            "--max-edge",
            "5",
            "--percent",
            "5"
        ])),
        2
    );
}

#[test]
fn unusable_output_location_is_a_job_error() {
    let root = workdir("joberr");
    let src = make_source(&root, "s.png", 8, 8);
    // A regular file where the output directory should be makes directory creation fail.
    let blocker = root.join("blocker");
    fs::write(&blocker, b"file").unwrap();
    let out_dir = blocker.join("sub");
    let out = run(&[
        "convert",
        src.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 3);
}

#[test]
fn help_and_version_exit_zero() {
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    assert!(String::from_utf8_lossy(&help.stdout).contains("USAGE"));
    let version = run(&["--version"]);
    assert_eq!(code(&version), 0);
    assert!(String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
}
