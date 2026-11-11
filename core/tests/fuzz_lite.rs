// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Deterministic mutation pass over the untrusted-input entry points (security review, open item
//! 2). It is not coverage-guided fuzzing: it applies seeded random mutations to valid seed files
//! and checks that every entry point returns a value or an error, and never panics. Failures
//! reproduce from the fixed seed.

use std::io::Cursor;
use std::sync::atomic::AtomicBool;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::batch::{self, BatchJob, MetadataPolicy, OutputFormat};
use tpt_view_core::{ImageBackend, StdBackend};

/// xorshift64*: small, dependency-free, and fixed by seed so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, format).unwrap();
    buf.into_inner()
}

fn insert_after(bytes: &[u8], at: usize, chunk: &[u8]) -> Vec<u8> {
    let mut out = bytes[..at].to_vec();
    out.extend_from_slice(chunk);
    out.extend_from_slice(&bytes[at..]);
    out
}

fn seeds() -> Vec<Vec<u8>> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(12, 9, Rgba([40, 90, 200, 255])));
    let jpeg = encode(&img, ImageFormat::Jpeg);
    let png = encode(&img, ImageFormat::Png);

    // JPEG with an XMP APP1 and an IPTC APP13 segment after SOI.
    let xmp = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:RDF><rdf:Description>\
        <dc:creator><rdf:Seq><rdf:li>Fuzz</rdf:li></rdf:Seq></dc:creator>\
        </rdf:Description></rdf:RDF></x:xmpmeta>";
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&((xmp.len() + 2) as u16).to_be_bytes());
    app1.extend_from_slice(xmp);
    let mut iim = vec![0x1C, 2, 5, 0, 4];
    iim.extend_from_slice(b"Test");
    let mut app13 = b"Photoshop 3.0\08BIM".to_vec();
    app13.extend_from_slice(&[0x04, 0x04, 0, 0, 0, 0, iim.len() as u8]);
    app13.extend_from_slice(&iim);
    let mut seg13 = vec![0xFF, 0xED];
    seg13.extend_from_slice(&((app13.len() + 2) as u16).to_be_bytes());
    seg13.extend_from_slice(&app13);
    let mut jpeg_meta = insert_after(&jpeg, 2, &seg13);
    jpeg_meta = insert_after(&jpeg_meta, 2, &app1);

    // PNG with an uncompressed XMP iTXt chunk after IHDR.
    let mut itxt = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
    itxt.extend_from_slice(xmp);
    let mut chunk = (itxt.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&itxt);
    chunk.extend_from_slice(&[0; 4]);
    let png_meta = insert_after(&png, 33, &chunk);

    // Little-endian TIFF with the XMP packet in tag 700.
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x02BCu16.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&(xmp.len() as u32).to_le_bytes());
    tiff.extend_from_slice(&26u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(xmp);

    vec![jpeg, jpeg_meta, png, png_meta, tiff]
}

fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut data = seed.to_vec();
    match rng.below(4) {
        0 => {
            // Flip a handful of random bytes.
            for _ in 0..=rng.below(8) {
                let i = rng.below(data.len());
                data[i] = rng.next() as u8;
            }
        }
        1 => {
            // Truncate.
            let keep = rng.below(data.len() + 1);
            data.truncate(keep);
        }
        2 => {
            // Corrupt a 16- or 32-bit length-like field.
            let i = rng.below(data.len().saturating_sub(4).max(1));
            let value = rng.next() as u32;
            let width = if rng.below(2) == 0 { 2 } else { 4 };
            let bytes = value.to_be_bytes();
            for (k, b) in bytes[4 - width..].iter().enumerate() {
                if let Some(slot) = data.get_mut(i + k) {
                    *slot = *b;
                }
            }
        }
        _ => {
            // Insert random bytes at a random offset.
            let at = rng.below(data.len() + 1);
            let extra: Vec<u8> = (0..=rng.below(16)).map(|_| rng.next() as u8).collect();
            data.splice(at..at, extra);
        }
    }
    data
}

#[test]
fn mutated_inputs_never_panic_in_decode_or_metadata() {
    let backend = StdBackend;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let seeds = seeds();
    for round in 0..4000 {
        let seed = &seeds[round % seeds.len()];
        let data = mutate(&mut rng, seed);
        // Each call may fail with an error; none may panic.
        let _ = backend.probe(&data);
        let _ = backend.read_metadata(&data);
        let _ = backend.decode(&data);
    }
}

#[test]
fn mutated_jpegs_survive_metadata_preserving_conversion() {
    let dir = std::env::temp_dir().join(format!("tpt-view-fuzz-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (input, output) = (dir.join("in"), dir.join("out"));
    std::fs::create_dir_all(&input).unwrap();
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let jpeg_seeds = [seeds().swap_remove(0), seeds().swap_remove(1)];

    let mut files = Vec::new();
    for n in 0..60 {
        let path = input.join(format!("m{n:03}.jpg"));
        std::fs::write(&path, mutate(&mut rng, &jpeg_seeds[n % 2])).unwrap();
        files.push(path);
    }
    let mut job = BatchJob::new(files, &output, OutputFormat::Jpeg);
    job.metadata = MetadataPolicy::PreserveExif;
    // A per-file failure is fine; the batch as a whole must return normally.
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(reports.len(), 60);
    let _ = std::fs::remove_dir_all(&dir);
}
