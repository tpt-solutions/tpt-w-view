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
