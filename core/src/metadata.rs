// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use std::io::Cursor;

use exif::{In, Reader, Tag, Value};

use crate::orientation::Orientation;

/// GPS position in signed decimal degrees (north and east are positive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

/// Metadata extracted from an image container (EXIF today; IPTC/XMP are pending).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub orientation: Orientation,
    pub make: Option<String>,
    pub model: Option<String>,
    /// Raw EXIF `DateTimeOriginal`, e.g. `"2024:01:02 03:04:05"`.
    pub date_time_original: Option<String>,
    pub gps: Option<GpsCoordinates>,
    /// IPTC title, falling back to XMP `dc:title`.
    pub title: Option<String>,
    /// IPTC caption, falling back to XMP `dc:description`.
    pub description: Option<String>,
    /// IPTC by-line, falling back to XMP `dc:creator`.
    pub creator: Option<String>,
    /// IPTC copyright notice, falling back to XMP `dc:rights`.
    pub copyright: Option<String>,
    /// IPTC keywords, falling back to XMP `dc:subject`.
    pub keywords: Vec<String>,
}

/// Reads EXIF metadata from `bytes`.
///
/// Missing or malformed EXIF never fails the caller: the defaults are returned instead, so a
/// broken metadata block cannot prevent an image from displaying.
pub(crate) fn read_exif(bytes: &[u8]) -> Metadata {
    let mut meta = read_exif_fields(bytes);
    let text = crate::text_meta::read(bytes);
    meta.title = text.title;
    meta.description = text.description;
    meta.creator = text.creator;
    meta.copyright = text.copyright;
    meta.keywords = text.keywords;
    meta
}

fn read_exif_fields(bytes: &[u8]) -> Metadata {
    let mut cursor = Cursor::new(bytes);
    let exif = match Reader::new().read_from_container(&mut cursor) {
        Ok(exif) => exif,
        Err(_) => return Metadata::default(),
    };

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .map(Orientation::from_exif)
        .unwrap_or_default();

    Metadata {
        orientation,
        make: ascii_field(&exif, Tag::Make),
        model: ascii_field(&exif, Tag::Model),
        date_time_original: ascii_field(&exif, Tag::DateTimeOriginal),
        gps: gps_coordinates(&exif),
        ..Metadata::default()
    }
}

fn ascii_field(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Ascii(parts) => {
            let first = parts.first()?;
            let text = String::from_utf8_lossy(first)
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_string();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn gps_coordinates(exif: &exif::Exif) -> Option<GpsCoordinates> {
    let latitude = dms_to_degrees(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S")?;
    let longitude = dms_to_degrees(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W")?;
    Some(GpsCoordinates {
        latitude,
        longitude,
    })
}

/// Converts a degrees/minutes/seconds rational triple plus hemisphere reference to degrees.
fn dms_to_degrees(
    exif: &exif::Exif,
    value_tag: Tag,
    ref_tag: Tag,
    negative_ref: &str,
) -> Option<f64> {
    let field = exif.get_field(value_tag, In::PRIMARY)?;
    let parts = match &field.value {
        Value::Rational(parts) if parts.len() >= 3 => parts,
        _ => return None,
    };
    let component = |r: &exif::Rational| -> Option<f64> {
        (r.denom != 0).then(|| f64::from(r.num) / f64::from(r.denom))
    };
    let degrees = component(&parts[0])?;
    let minutes = component(&parts[1])?;
    let seconds = component(&parts[2])?;
    let mut value = degrees + minutes / 60.0 + seconds / 3600.0;

    let hemisphere = ascii_field(exif, ref_tag);
    if hemisphere.as_deref() == Some(negative_ref) {
        value = -value;
    }
    value.is_finite().then_some(value)
}
