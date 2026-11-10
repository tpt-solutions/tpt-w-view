// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! End-to-end pipeline: browse a directory, build thumbnails, edit one image non-destructively,
//! save it, then batch-convert the folder. Mirrors what the GUI does, through the public API.

use std::sync::atomic::AtomicBool;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::batch::{self, BatchJob, Outcome, OutputFormat, Resize};
use tpt_view_core::edit::{EditSession, Filter, Op, SaveTarget};
use tpt_view_core::scan;
use tpt_view_core::thumbs::{self, ThumbnailCache};
use tpt_view_core::{ImageBackend, StdBackend};

#[test]
fn browse_thumbnail_edit_save_then_batch() {
    let root = std::env::temp_dir().join(format!("tpt-pipeline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let photos = root.join("photos");
    let cache_dir = root.join("cache");
    let out_dir = root.join("converted");
    std::fs::create_dir_all(&photos).unwrap();

    // 1. Browse: three photos plus a file that must be ignored.
    for (name, w, h) in [
        ("beach.png", 640, 480),
        ("city.jpg", 800, 600),
        ("sky.bmp", 320, 240),
    ] {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 160, 255])
        }));
        img.save(photos.join(name)).unwrap();
    }
    std::fs::write(photos.join("readme.txt"), b"not an image").unwrap();

    let entries = scan::scan_dir(&photos).unwrap();
    let names: Vec<_> = entries
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["beach.png", "city.jpg", "sky.bmp"]);

    // 2. Thumbnails: first pass generates, second pass is served from the cache.
    let cache = ThumbnailCache::new(&cache_dir, 128).unwrap();
    for entry in &entries {
        assert!(!cache.get_or_create(entry).unwrap().cache_hit);
    }
    for entry in &entries {
        let thumb = cache.get_or_create(entry).unwrap();
        assert!(thumb.cache_hit);
        let (w, h) = thumbs::cached_dimensions(&thumb.path).unwrap();
        assert!(w.max(h) <= 128, "thumbnail {w}x{h} exceeds edge");
    }

    // 3. Edit one photo non-destructively: crop, rotate, undo the rotate, then save as.
    let beach = &entries[0];
    let decoded = StdBackend
        .decode(&std::fs::read(&beach.path).unwrap())
        .unwrap();
    let mut session = EditSession::new(decoded.image, Some(beach.path.clone()));
    session
        .apply(&Op::Crop {
            x: 10,
            y: 10,
            width: 300,
            height: 200,
        })
        .unwrap();
    session.apply(&Op::RotateQuarterTurns(1)).unwrap();
    assert!(session.undo());
    session
        .apply(&Op::Resize {
            width: 150,
            height: 100,
            filter: Filter::Lanczos3,
        })
        .unwrap();

    // The original file is untouched by editing.
    let original_after_edit = StdBackend
        .probe(&std::fs::read(&beach.path).unwrap())
        .unwrap();
    assert_eq!(
        (original_after_edit.width, original_after_edit.height),
        (640, 480)
    );

    let saved = session
        .save(
            &SaveTarget::SaveAs(root.join("beach-edited.png")),
            ImageFormat::Png,
            90,
        )
        .unwrap();
    let saved_info = StdBackend.probe(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!((saved_info.width, saved_info.height), (150, 100));

    // Overwriting the original still needs explicit confirmation.
    assert!(session
        .save(
            &SaveTarget::Overwrite { confirmed: false },
            ImageFormat::Png,
            90
        )
        .is_err());

    // 4. Batch-convert the whole folder to WebP at half size, with a counter rename.
    let mut job = BatchJob::new(vec![photos.clone()], &out_dir, OutputFormat::WebP);
    job.resize = Resize::Percent(50.0);
    job.rename = "shot_{counter:02}_{name:lower}".into();
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(reports.len(), 3);
    assert!(
        reports
            .iter()
            .all(|r| matches!(r.outcome, Outcome::Converted { .. })),
        "{reports:?}"
    );

    let beach_out = out_dir.join("shot_01_beach.webp");
    let info = StdBackend
        .probe(&std::fs::read(&beach_out).unwrap())
        .unwrap();
    assert_eq!((info.width, info.height), (320, 240));

    let _ = std::fs::remove_dir_all(&root);
}
