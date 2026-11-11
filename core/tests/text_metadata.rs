// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! End-to-end IPTC and XMP extraction through the public backend API.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tpt_view_core::{ImageBackend, StdBackend};

fn base_jpeg() -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([90, 120, 200, 255])));
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Jpeg).unwrap();
    buf.into_inner()
}

/// Inserts an application segment (marker + length + payload) directly after SOI.
fn insert_app_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn iim_dataset(out: &mut Vec<u8>, dataset: u8, text: &str) {
    out.extend_from_slice(&[0x1C, 2, dataset]);
    out.extend_from_slice(&(text.len() as u16).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}

/// Photoshop APP13 payload holding one IPTC-NAA resource.
fn app13_with_iptc(datasets: &[(u8, &str)]) -> Vec<u8> {
    let mut iim = Vec::new();
    for (ds, text) in datasets {
        iim_dataset(&mut iim, *ds, text);
    }
    let mut payload = b"Photoshop 3.0\0".to_vec();
    payload.extend_from_slice(b"8BIM");
    payload.extend_from_slice(&0x0404u16.to_be_bytes());
    payload.extend_from_slice(&[0, 0]); // empty pascal name, padded to even
    payload.extend_from_slice(&(iim.len() as u32).to_be_bytes());
    payload.extend_from_slice(&iim);
    if iim.len() % 2 == 1 {
        payload.push(0);
    }
    payload
}

fn app1_xmp(xml: &str) -> Vec<u8> {
    let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    payload.extend_from_slice(xml.as_bytes());
    payload
}

#[test]
fn iptc_fields_are_read_from_a_real_jpeg() {
    let file = insert_app_segment(
        &base_jpeg(),
        0xED,
        &app13_with_iptc(&[
            (5, "Harbour at dawn"),
            (120, "Boats idle"),
            (80, "J. Doe"),
            (25, "sea"),
            (25, "boats"),
        ]),
    );
    let meta = StdBackend.read_metadata(&file).unwrap();
    assert_eq!(meta.title.as_deref(), Some("Harbour at dawn"));
    assert_eq!(meta.description.as_deref(), Some("Boats idle"));
    assert_eq!(meta.creator.as_deref(), Some("J. Doe"));
    assert_eq!(meta.keywords, vec!["sea", "boats"]);
}

#[test]
fn xmp_fields_fill_in_when_iptc_is_absent() {
    let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:title><rdf:Alt><rdf:li xml:lang="x-default">Market &amp; stalls</rdf:li></rdf:Alt></dc:title>
        <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">All rights reserved</rdf:li></rdf:Alt></dc:rights>
        <dc:subject><rdf:Bag><rdf:li>food</rdf:li></rdf:Bag></dc:subject>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let file = insert_app_segment(&base_jpeg(), 0xE1, &app1_xmp(xml));
    let meta = StdBackend.read_metadata(&file).unwrap();
    assert_eq!(meta.title.as_deref(), Some("Market & stalls"));
    assert_eq!(meta.copyright.as_deref(), Some("All rights reserved"));
    assert_eq!(meta.keywords, vec!["food"]);
    assert_eq!(meta.creator, None);
}

#[test]
fn iptc_takes_precedence_over_xmp() {
    let xml = "<dc:title>From XMP</dc:title>";
    let with_xmp = insert_app_segment(&base_jpeg(), 0xE1, &app1_xmp(xml));
    let file = insert_app_segment(&with_xmp, 0xED, &app13_with_iptc(&[(5, "From IPTC")]));
    let meta = StdBackend.read_metadata(&file).unwrap();
    assert_eq!(meta.title.as_deref(), Some("From IPTC"));
}

#[test]
fn plain_jpeg_has_no_text_fields() {
    let meta = StdBackend.read_metadata(&base_jpeg()).unwrap();
    assert_eq!(meta.title, None);
    assert!(meta.keywords.is_empty());
}

#[test]
fn truncated_app13_does_not_break_decoding() {
    let mut payload = app13_with_iptc(&[(5, "cut off")]);
    payload.truncate(payload.len() - 6);
    let file = insert_app_segment(&base_jpeg(), 0xED, &payload);
    // Metadata may be empty, but the image itself must still decode.
    let decoded = StdBackend.decode(&file).expect("image still decodes");
    assert_eq!((decoded.image.width(), decoded.image.height()), (8, 8));
}

/// Source JPEG carrying IPTC (title, keyword) and XMP (creator, rights) fields.
fn source_with_text_metadata() -> Vec<u8> {
    let xmp = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:creator><rdf:Seq><rdf:li>A. Photographer</rdf:li></rdf:Seq></dc:creator>
        <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">Licensed</rdf:li></rdf:Alt></dc:rights>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let with_xmp = insert_app_segment(&base_jpeg(), 0xE1, &app1_xmp(xmp));
    insert_app_segment(
        &with_xmp,
        0xED,
        &app13_with_iptc(&[(5, "Quay at noon"), (25, "harbour")]),
    )
}

fn convert_jpeg(policy: tpt_view_core::batch::MetadataPolicy) -> Vec<u8> {
    use std::sync::atomic::AtomicBool;
    use tpt_view_core::batch::{self, BatchJob, OutputFormat};

    let base =
        std::env::temp_dir().join(format!("tpt-view-text-{}-{:?}", std::process::id(), policy));
    let _ = std::fs::remove_dir_all(&base);
    let (input, output) = (base.join("in"), base.join("out"));
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("src.jpg"), source_with_text_metadata()).unwrap();

    let mut job = BatchJob::new(vec![input.join("src.jpg")], &output, OutputFormat::Jpeg);
    job.metadata = policy;
    let reports = batch::run(&job, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(
        matches!(reports[0].outcome, batch::Outcome::Converted { .. }),
        "{reports:?}"
    );
    let bytes = std::fs::read(output.join("src.jpg")).unwrap();
    let _ = std::fs::remove_dir_all(&base);
    bytes
}

#[test]
fn preserve_policy_keeps_iptc_and_xmp_across_jpeg_conversion() {
    use tpt_view_core::batch::MetadataPolicy;

    let meta = StdBackend
        .read_metadata(&convert_jpeg(MetadataPolicy::PreserveExif))
        .unwrap();
    assert_eq!(meta.title.as_deref(), Some("Quay at noon"), "IPTC title");
    assert_eq!(meta.keywords, vec!["harbour"], "IPTC keywords");
    assert_eq!(
        meta.creator.as_deref(),
        Some("A. Photographer"),
        "XMP creator"
    );
    assert_eq!(meta.copyright.as_deref(), Some("Licensed"), "XMP rights");
}

#[test]
fn strip_policy_drops_iptc_and_xmp() {
    use tpt_view_core::batch::MetadataPolicy;

    let meta = StdBackend
        .read_metadata(&convert_jpeg(MetadataPolicy::Strip))
        .unwrap();
    assert_eq!(meta.title, None);
    assert_eq!(meta.creator, None);
    assert!(meta.keywords.is_empty());
}

/// Inserts an uncompressed XMP iTXt chunk directly after the PNG IHDR chunk.
fn png_with_xmp(png: &[u8], xml: &str) -> Vec<u8> {
    let mut data = b"XML:com.adobe.xmp\0".to_vec();
    data.extend_from_slice(&[0, 0]); // compression flag and method: uncompressed
    data.extend_from_slice(b"\0\0"); // empty language tag and translated keyword
    data.extend_from_slice(xml.as_bytes());

    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    chunk.extend_from_slice(&[0; 4]); // CRC is not checked by the reader

    // Signature (8) + IHDR chunk (4 + 4 + 13 + 4) puts the insertion point at byte 33.
    let at = 33;
    let mut out = png[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[at..]);
    out
}

#[test]
fn xmp_fields_are_read_from_png_itxt() {
    let mut png_buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255])))
        .write_to(&mut png_buf, ImageFormat::Png)
        .unwrap();
    let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:creator><rdf:Seq><rdf:li>P. Maker</rdf:li></rdf:Seq></dc:creator>
        <dc:title><rdf:Alt><rdf:li xml:lang="x-default">Pixel &amp; ink</rdf:li></rdf:Alt></dc:title>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let file = png_with_xmp(&png_buf.into_inner(), xml);
    let meta = StdBackend.read_metadata(&file).unwrap();
    assert_eq!(meta.creator.as_deref(), Some("P. Maker"));
    assert_eq!(meta.title.as_deref(), Some("Pixel & ink"));
}

#[test]
fn png_without_xmp_has_no_text_fields() {
    let mut png_buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255])))
        .write_to(&mut png_buf, ImageFormat::Png)
        .unwrap();
    let meta = StdBackend.read_metadata(&png_buf.into_inner()).unwrap();
    assert_eq!(meta.title, None);
    assert_eq!(meta.creator, None);
}

/// Minimal little-endian TIFF whose only IFD0 entry is ApplicationNotes (700) holding `xml`.
fn tiff_with_xmp(xml: &[u8]) -> Vec<u8> {
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    out.extend_from_slice(&1u16.to_le_bytes()); // one entry
    out.extend_from_slice(&0x02BCu16.to_le_bytes()); // tag 700, ApplicationNotes
    out.extend_from_slice(&1u16.to_le_bytes()); // type BYTE
    out.extend_from_slice(&(xml.len() as u32).to_le_bytes());
    out.extend_from_slice(&26u32.to_le_bytes()); // data follows the IFD: 8 + 2 + 12 + 4
    out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    out.extend_from_slice(xml);
    out
}

#[test]
fn xmp_fields_are_read_from_tiff_application_notes() {
    let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:creator><rdf:Seq><rdf:li>T. Scanner</rdf:li></rdf:Seq></dc:creator>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let meta = StdBackend
        .read_metadata(&tiff_with_xmp(xml.as_bytes()))
        .unwrap();
    assert_eq!(meta.creator.as_deref(), Some("T. Scanner"));
}

/// Inserts an iTXt XMP chunk with an explicit compression flag, method and text after IHDR.
fn png_with_itxt_xmp(png: &[u8], compression_flag: u8, method: u8, text: &[u8]) -> Vec<u8> {
    let mut data = b"XML:com.adobe.xmp\0".to_vec();
    data.extend_from_slice(&[compression_flag, method]);
    data.extend_from_slice(b"\0\0");
    data.extend_from_slice(text);

    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    chunk.extend_from_slice(&[0; 4]);

    let at = 33;
    let mut out = png[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[at..]);
    out
}

fn base_png() -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, Rgba([9, 9, 9, 255])))
        .write_to(&mut buf, ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

#[test]
fn compressed_png_xmp_is_inflated() {
    let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:creator><rdf:Seq><rdf:li>Z. Packed</rdf:li></rdf:Seq></dc:creator>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let zipped = miniz_oxide::deflate::compress_to_vec_zlib(xml.as_bytes(), 6);
    let file = png_with_itxt_xmp(&base_png(), 1, 0, &zipped);
    assert_eq!(
        StdBackend.read_metadata(&file).unwrap().creator.as_deref(),
        Some("Z. Packed")
    );
}

#[test]
fn zlib_bomb_in_png_xmp_is_refused_quickly() {
    // 32 MiB of zeros compresses to a few kilobytes; the reader must give up, not expand it.
    let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; 32 << 20], 6);
    let file = png_with_itxt_xmp(&base_png(), 1, 0, &bomb);
    assert_eq!(StdBackend.read_metadata(&file).unwrap().creator, None);
}

#[test]
fn unknown_compression_method_is_ignored() {
    let file = png_with_itxt_xmp(&base_png(), 1, 3, b"not zlib");
    assert_eq!(StdBackend.read_metadata(&file).unwrap().creator, None);
}

/// Encodes a small WebP, then adds an XMP chunk through the public batch path.
#[test]
fn xmp_is_read_from_webp_xmp_chunk() {
    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(6, 6, Rgba([5, 5, 5, 255])))
        .write_to(&mut buf, ImageFormat::WebP)
        .unwrap();
    let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
        <dc:creator><rdf:Seq><rdf:li>W. Container</rdf:li></rdf:Seq></dc:creator>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let file = webp_with_xmp(&buf.into_inner(), xml.as_bytes());
    assert_eq!(
        StdBackend.read_metadata(&file).unwrap().creator.as_deref(),
        Some("W. Container")
    );
}

/// Rebuilds a WebP file with an extra `XMP ` chunk, keeping its image data.
fn webp_with_xmp(webp: &[u8], xml: &[u8]) -> Vec<u8> {
    let mut chunk = b"XMP ".to_vec();
    chunk.extend_from_slice(&(xml.len() as u32).to_le_bytes());
    chunk.extend_from_slice(xml);
    if xml.len() % 2 == 1 {
        chunk.push(0);
    }
    let mut out = webp.to_vec();
    out.extend_from_slice(&chunk);
    let len = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&len.to_le_bytes());
    out
}
