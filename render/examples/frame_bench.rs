// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Frame-time benchmark for the tiled renderer. Run with:
//! `cargo run --release -p tpt-view-render --example frame_bench`

use std::time::Instant;

use image::{Rgba, RgbaImage};
use tpt_view_render::{Renderer, Sampling, View};

fn main() {
    let renderer = match Renderer::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no adapter: {e}");
            return;
        }
    };
    println!("adapter: {}", renderer.adapter_name());

    // A large synthetic image so tiling is exercised; 6000 x 4000 needs several tiles.
    let (w, h) = (6000u32, 4000u32);
    let img = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });

    let t = Instant::now();
    let tiled = renderer.upload(&img);
    println!(
        "upload {}x{} ({} tiles): {:?}",
        w,
        h,
        tiled.tile_count(),
        t.elapsed()
    );

    let screen = (1920u32, 1080u32);
    for (label, zoom) in [
        ("fit (zoom 0.3)", 0.3f32),
        ("1:1 (zoom 1.0)", 1.0),
        ("zoom 4x", 4.0),
    ] {
        let view = View {
            out_width: screen.0,
            out_height: screen.1,
            zoom,
            offset_x: -(w as f32 * zoom - screen.0 as f32) / 2.0,
            offset_y: -(h as f32 * zoom - screen.1 as f32) / 2.0,
        };
        let runs = 5;
        let t = Instant::now();
        for _ in 0..runs {
            renderer
                .render(&tiled, &view, Sampling::Linear)
                .expect("render");
        }
        println!(
            "{label:>16}: {:?} per frame (incl. readback)",
            t.elapsed() / runs
        );
    }
}
