// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Cold versus warm directory open for the thumbnail cache. Run with:
//! `cargo run --release -p tpt-view-core --example cache_bench -- <image-count> [threads]`
//! Threads default to 1 (sequential). Larger values split the cold pass across threads, which is
//! how the desktop app's concurrent thumbnail requests behave.

use std::time::Instant;

use image::{DynamicImage, Rgba, RgbaImage};
use tpt_view_core::scan;
use tpt_view_core::thumbs::ThumbnailCache;

fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let threads: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    let base = std::env::temp_dir().join(format!("tpt-cache-bench-{}", std::process::id()));
    let photos = base.join("photos");
    let cache_dir = base.join("cache");
    std::fs::create_dir_all(&photos).unwrap();

    let t = Instant::now();
    for i in 0..count {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(1200, 900, |x, y| {
            Rgba([(x + i as u32) as u8, y as u8, 90, 255])
        }));
        img.save(photos.join(format!("img_{i:05}.png"))).unwrap();
    }
    println!("generated {count} images in {:?}", t.elapsed());

    let cache = ThumbnailCache::new(&cache_dir, 256).unwrap();

    let t = Instant::now();
    let entries = scan::scan_dir(&photos).unwrap();
    println!("scan {} entries: {:?}", entries.len(), t.elapsed());

    let t = Instant::now();
    let chunk = entries.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for part in entries.chunks(chunk) {
            let cache = &cache;
            scope.spawn(move || {
                for e in part {
                    cache.get_or_create(e).unwrap();
                }
            });
        }
    });
    println!(
        "cold open with {threads} thread(s) (generate all thumbnails): {:?}",
        t.elapsed()
    );

    let t = Instant::now();
    let hits = entries
        .iter()
        .filter(|e| cache.get_or_create(e).unwrap().cache_hit)
        .count();
    println!("warm open ({hits} cache hits): {:?}", t.elapsed());

    let _ = std::fs::remove_dir_all(&base);
}
