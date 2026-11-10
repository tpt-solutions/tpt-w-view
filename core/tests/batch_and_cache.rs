// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Integration tests for directory scanning, the thumbnail cache and the batch engine.
//! Fixtures are generated at runtime in unique temp directories.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::batch::{
    self, BatchJob, MetadataPolicy, Outcome, OutputFormat, Progress, Resize,
};
use tpt_view_core::scan::{self, FileEntry};
use tpt_view_core::thumbs::{self, ThumbnailCache};
use tpt_view_core::{ImageBackend, Orientation, StdBackend};

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-view-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn gradient(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    }))
}

fn save(img: &DynamicImage, path: &Path) {
    img.save(path).expect("save fixture");
}

fn entry_named(entries: &[FileEntry], name: &str) -> FileEntry {
    entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == name)
        .cloned()
        .unwrap_or_else(|| panic!("no entry {name}"))
}

// ---- scan ------------------------------------------------------------------------------------

#[test]
fn scan_filters_and_sorts_case_insensitively() {
    let dir = tmpdir("scan");
    save(&gradient(4, 4), &dir.join("b_upper.PNG"));
    save(&gradient(4, 4), &dir.join("a.png"));
    fs::write(dir.join("notes.txt"), b"hi").unwrap();
    fs::create_dir_all(dir.join("subdir")).unwrap();
    save(&gradient(4, 4), &dir.join("subdir").join("nested.png"));

    let names: Vec<String> = scan::scan_dir(&dir)
        .unwrap()
        .into_iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.png", "b_upper.PNG"]);
}

// ---- thumbnail cache -------------------------------------------------------------------------

#[test]
fn thumbnail_cache_hits_misses_and_invalidates() {
    let src_dir = tmpdir("thumb-src");
    let cache_dir = tmpdir("thumb-cache");
    let src = src_dir.join("wide.png");
    save(&gradient(300, 100), &src);

    let cache = ThumbnailCache::new(&cache_dir, 64).unwrap();
    let entries = scan::scan_dir(&src_dir).unwrap();
    let entry = entry_named(&entries, "wide.png");

    let first = cache.get_or_create(&entry).unwrap();
    assert!(!first.cache_hit);
    assert_eq!(thumbs::cached_dimensions(&first.path).unwrap(), (64, 21));

    let second = cache.get_or_create(&entry).unwrap();
    assert!(second.cache_hit);
    assert_eq!(second.path, first.path);

    // Changing the file changes its identity, so the old thumbnail is no longer used.
    save(&gradient(200, 200), &src);
    let changed = entry_named(&scan::scan_dir(&src_dir).unwrap(), "wide.png");
    let third = cache.get_or_create(&changed).unwrap();
    assert!(!third.cache_hit);
    assert_ne!(third.path, first.path);

    // Pruning keeps only live entries: the stale thumbnail is removed.
    let removed = cache.prune(std::slice::from_ref(&changed)).unwrap();
    assert_eq!(removed, 1);
    assert!(!first.path.exists());
    assert!(third.path.exists());
}

#[test]
fn cache_key_depends_on_edge_length() {
    let src_dir = tmpdir("thumb-key");
    save(&gradient(10, 10), &src_dir.join("k.png"));
    let entry = entry_named(&scan::scan_dir(&src_dir).unwrap(), "k.png");
    let small = ThumbnailCache::new(tmpdir("thumb-key-a"), 32).unwrap();
    let large = ThumbnailCache::new(tmpdir("thumb-key-b"), 256).unwrap();
    assert_ne!(small.key_for(&entry), large.key_for(&entry));
}

// ---- batch engine ----------------------------------------------------------------------------

#[test]
fn batch_converts_renames_and_reports_per_file_failures() {
    let input = tmpdir("batch-in");
    let output = tmpdir("batch-out");
    save(&gradient(20, 10), &input.join("alpha.png"));
    fs::write(input.join("beta.png"), b"corrupt bytes").unwrap();
    save(&gradient(20, 10), &input.join("gamma.bmp"));

    let mut job = BatchJob::new(vec![input.clone()], &output, OutputFormat::Jpeg);
    job.rename = "img_{counter:03}".into();
    job.quality = 80;

    let cancel = AtomicBool::new(false);
    let mut ticks: Vec<Progress> = Vec::new();
    let reports = batch::run(&job, &cancel, |p| ticks.push(p)).unwrap();

    assert_eq!(reports.len(), 3);
    assert_eq!(ticks.last(), Some(&Progress { done: 3, total: 3 }));
    assert_eq!(ticks.len(), 3);

    let converted: Vec<&Outcome> = reports
        .iter()
        .map(|r| &r.outcome)
        .filter(|o| matches!(o, Outcome::Converted { .. }))
        .collect();
    assert_eq!(converted.len(), 2);
    assert!(output.join("img_001.jpg").is_file());
    assert!(output.join("img_003.jpg").is_file());
    assert!(!output.join("img_002.jpg").exists());
    assert!(matches!(reports[1].outcome, Outcome::Failed { .. }));

    let info = StdBackend
        .probe(&fs::read(output.join("img_001.jpg")).unwrap())
        .unwrap();
    assert_eq!((info.width, info.height), (20, 10));
}

#[test]
fn batch_resize_max_edge_never_upscales() {
    let input = tmpdir("resize-in");
    let output = tmpdir("resize-out");
    save(&gradient(400, 200), &input.join("big.png"));
    save(&gradient(40, 20), &input.join("small.png"));

    let mut job = BatchJob::new(vec![input], &output, OutputFormat::Png);
    job.resize = Resize::MaxEdge(100);
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(reports
        .iter()
        .all(|r| matches!(r.outcome, Outcome::Converted { .. })));

    let big = StdBackend
        .probe(&fs::read(output.join("big.png")).unwrap())
        .unwrap();
    assert_eq!((big.width, big.height), (100, 50));
    let small = StdBackend
        .probe(&fs::read(output.join("small.png")).unwrap())
        .unwrap();
    assert_eq!((small.width, small.height), (40, 20));
}

#[test]
fn existing_outputs_are_skipped_unless_overwrite() {
    let input = tmpdir("skip-in");
    let output = tmpdir("skip-out");
    save(&gradient(8, 8), &input.join("one.png"));
    let job = BatchJob::new(vec![input.join("one.png")], &output, OutputFormat::Png);

    let cancel = AtomicBool::new(false);
    let first = batch::run(&job, &cancel, |_| {}).unwrap();
    assert!(matches!(first[0].outcome, Outcome::Converted { .. }));

    let second = batch::run(&job, &cancel, |_| {}).unwrap();
    assert!(matches!(second[0].outcome, Outcome::Skipped { .. }));

    let mut forced = job.clone();
    forced.overwrite = true;
    let third = batch::run(&forced, &cancel, |_| {}).unwrap();
    assert!(matches!(third[0].outcome, Outcome::Converted { .. }));
}

#[test]
fn cancelled_job_marks_remaining_files_cancelled() {
    let input = tmpdir("cancel-in");
    let output = tmpdir("cancel-out");
    for name in ["a.png", "b.png", "c.png"] {
        save(&gradient(4, 4), &input.join(name));
    }
    let job = BatchJob::new(vec![input], &output, OutputFormat::Png);
    let cancel = AtomicBool::new(true);
    let reports = batch::run(&job, &cancel, |_| {}).unwrap();
    assert_eq!(reports.len(), 3);
    assert!(reports.iter().all(|r| r.outcome == Outcome::Cancelled));
    assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
}

#[test]
fn invalid_job_settings_fail_before_any_work() {
    let output = tmpdir("invalid-out");
    let input = tmpdir("invalid-in");
    save(&gradient(4, 4), &input.join("x.png"));
    let cancel = AtomicBool::new(false);

    let mut bad_quality = BatchJob::new(vec![input.join("x.png")], &output, OutputFormat::Jpeg);
    bad_quality.quality = 0;
    assert!(batch::run(&bad_quality, &cancel, |_| {}).is_err());

    let mut bad_pattern = BatchJob::new(vec![input.join("x.png")], &output, OutputFormat::Png);
    bad_pattern.rename = "{bogus}".into();
    assert!(batch::run(&bad_pattern, &cancel, |_| {}).is_err());

    let empty = BatchJob::new(vec![], &output, OutputFormat::Png);
    assert!(batch::run(&empty, &cancel, |_| {}).is_err());

    assert_eq!(OutputFormat::parse("JPG").unwrap(), OutputFormat::Jpeg);
    assert!(OutputFormat::parse("gif").is_err());
}

/// Minimal JPEG with a single-entry EXIF IFD carrying Orientation = `orientation`.
fn jpeg_with_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
    let mut jpeg_buf = std::io::Cursor::new(Vec::new());
    gradient(w, h)
        .write_to(&mut jpeg_buf, image::ImageFormat::Jpeg)
        .unwrap();
    let jpeg = jpeg_buf.into_inner();

    // TIFF (big-endian): header, IFD0 with one SHORT entry (tag 0x0112) inline.
    let mut tiff = b"MM\0*".to_vec();
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&[(orientation >> 8) as u8, orientation as u8, 0, 0]);
    tiff.extend_from_slice(&0u32.to_be_bytes());

    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn preserve_exif_keeps_orientation_tag_and_pixels_unrotated() {
    let input = tmpdir("exif-in");
    let output = tmpdir("exif-out");
    fs::write(input.join("rot.jpg"), jpeg_with_orientation(40, 24, 6)).unwrap();

    let mut job = BatchJob::new(vec![input.join("rot.jpg")], &output, OutputFormat::Jpeg);
    job.metadata = MetadataPolicy::PreserveExif;
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(
        matches!(reports[0].outcome, Outcome::Converted { .. }),
        "{reports:?}"
    );

    let bytes = fs::read(output.join("rot.jpg")).unwrap();
    let meta = StdBackend.read_metadata(&bytes).unwrap();
    assert_eq!(meta.orientation, Orientation::Rotate90);
    let info = StdBackend.probe(&bytes).unwrap();
    assert_eq!(
        (info.width, info.height),
        (40, 24),
        "pixels must stay un-rotated"
    );
}

#[test]
fn strip_policy_bakes_orientation_and_drops_exif() {
    let input = tmpdir("strip-in");
    let output = tmpdir("strip-out");
    fs::write(input.join("rot.jpg"), jpeg_with_orientation(40, 24, 6)).unwrap();

    let job = BatchJob::new(vec![input.join("rot.jpg")], &output, OutputFormat::Jpeg);
    batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();

    let bytes = fs::read(output.join("rot.jpg")).unwrap();
    assert_eq!(
        StdBackend.read_metadata(&bytes).unwrap().orientation,
        Orientation::Normal
    );
    let info = StdBackend.probe(&bytes).unwrap();
    assert_eq!(
        (info.width, info.height),
        (24, 40),
        "orientation applied to pixels"
    );
}

#[test]
fn conversion_round_trips_across_output_format_matrix() {
    use tpt_view_core::batch::OutputFormat;

    let input = tmpdir("matrix-in");
    let src = gradient(33, 17);
    // Feed every source format the engine reads through every output format.
    let sources = [
        ("from_png.png", ImageFormat::Png),
        ("from_bmp.bmp", ImageFormat::Bmp),
        ("from_tiff.tiff", ImageFormat::Tiff),
        ("from_webp.webp", ImageFormat::WebP),
        ("from_jpg.jpg", ImageFormat::Jpeg),
        ("from_gif.gif", ImageFormat::Gif),
        ("from_avif.avif", ImageFormat::Avif),
    ];
    for (name, format) in sources {
        src.save_with_format(input.join(name), format).unwrap();
    }

    let outputs = [
        OutputFormat::Jpeg,
        OutputFormat::Png,
        OutputFormat::WebP,
        OutputFormat::Tiff,
        OutputFormat::Bmp,
        OutputFormat::Avif,
    ];
    for out_format in outputs {
        let output = tmpdir(&format!("matrix-out-{}", out_format.extension()));
        let job = BatchJob::new(vec![input.clone()], &output, out_format);
        let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(reports.len(), sources.len());
        for report in &reports {
            let Outcome::Converted { output: out_path } = &report.outcome else {
                panic!("{out_format:?} {:?}: {:?}", report.input, report.outcome);
            };
            let bytes = fs::read(out_path).unwrap();
            let info = StdBackend.probe(&bytes).unwrap();
            assert_eq!(
                (info.width, info.height),
                (33, 17),
                "{out_format:?} {out_path:?}"
            );
        }
    }
}
