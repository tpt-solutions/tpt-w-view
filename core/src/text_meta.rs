// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! IPTC-IIM (Photoshop APP13) and XMP (APP1) text metadata for JPEG files (Phase 1).
//!
//! Only the descriptive fields a viewer shows are extracted: title, description (caption),
//! creator, copyright and keywords. IPTC is preferred where both formats carry a field, since
//! it is the older, more widely written standard for photo agencies.
//!
//! The XMP reader is a deliberately small scanner, not a full RDF parser. It handles the element
//! forms that editors write (`<dc:title><rdf:Alt><rdf:li>…`) and entity-escaped text.

use crate::jpeg_meta::app_segments;

const PHOTOSHOP_HEADER: &[u8] = b"Photoshop 3.0\0";
const IPTC_RESOURCE_ID: u16 = 0x0404;
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// Descriptive text fields. Every field is optional; keywords may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
    pub keywords: Vec<String>,
}

/// Reads IPTC and XMP text fields from a JPEG. Non-JPEG input yields empty metadata.
pub(crate) fn read(jpeg: &[u8]) -> TextMetadata {
    let mut iptc = TextMetadata::default();
    let mut xmp = TextMetadata::default();
    for (marker, payload) in app_segments(jpeg) {
        match marker {
            0xED if payload.starts_with(PHOTOSHOP_HEADER) => {
                if let Some(iim) = photoshop_iptc_block(payload) {
                    iptc = parse_iim(iim);
                }
            }
            0xE1 if payload.starts_with(XMP_HEADER) => {
                let xml = String::from_utf8_lossy(&payload[XMP_HEADER.len()..]);
                xmp = parse_xmp(&xml);
            }
            _ => {}
        }
    }
    merge(iptc, xmp)
}

/// IPTC wins per field; XMP fills gaps.
fn merge(iptc: TextMetadata, xmp: TextMetadata) -> TextMetadata {
    TextMetadata {
        title: iptc.title.or(xmp.title),
        description: iptc.description.or(xmp.description),
        creator: iptc.creator.or(xmp.creator),
        copyright: iptc.copyright.or(xmp.copyright),
        keywords: if iptc.keywords.is_empty() {
            xmp.keywords
        } else {
            iptc.keywords
        },
    }
}

/// Finds the IPTC-NAA (0x0404) resource inside a Photoshop APP13 payload.
fn photoshop_iptc_block(payload: &[u8]) -> Option<&[u8]> {
    let mut pos = PHOTOSHOP_HEADER.len();
    while pos + 12 <= payload.len() {
        if &payload[pos..pos + 4] != b"8BIM" {
            return None;
        }
        let id = u16::from_be_bytes([payload[pos + 4], payload[pos + 5]]);
        // Pascal name: length byte + bytes, padded so the total is even.
        let name_len = usize::from(payload[pos + 6]);
        let name_total = (1 + name_len).div_ceil(2) * 2;
        let size_at = pos + 6 + name_total;
        if size_at + 4 > payload.len() {
            return None;
        }
        let size = u32::from_be_bytes([
            payload[size_at],
            payload[size_at + 1],
            payload[size_at + 2],
            payload[size_at + 3],
        ]) as usize;
        let data_start = size_at + 4;
        let data_end = data_start.checked_add(size)?;
        if data_end > payload.len() {
            return None;
        }
        if id == IPTC_RESOURCE_ID {
            return Some(&payload[data_start..data_end]);
        }
        // Resource data is padded to an even length.
        pos = data_end + (size % 2);
    }
    None
}

/// Parses IPTC-IIM record 2 datasets. Extended-length datasets are skipped.
fn parse_iim(data: &[u8]) -> TextMetadata {
    let mut out = TextMetadata::default();
    let mut pos = 0usize;
    while pos + 5 <= data.len() && data[pos] == 0x1C {
        let record = data[pos + 1];
        let dataset = data[pos + 2];
        let len = u16::from_be_bytes([data[pos + 3], data[pos + 4]]);
        if len & 0x8000 != 0 {
            break;
        }
        let end = pos + 5 + usize::from(len);
        if end > data.len() {
            break;
        }
        if record == 2 {
            let text = String::from_utf8_lossy(&data[pos + 5..end])
                .trim()
                .to_string();
            if !text.is_empty() {
                // First occurrence wins for single-valued fields; keywords accumulate.
                match dataset {
                    5 => {
                        out.title.get_or_insert(text);
                    }
                    25 => out.keywords.push(text),
                    80 => {
                        out.creator.get_or_insert(text);
                    }
                    116 => {
                        out.copyright.get_or_insert(text);
                    }
                    120 => {
                        out.description.get_or_insert(text);
                    }
                    _ => {}
                }
            }
        }
        // Always advance, whatever the dataset, so the loop cannot stall.
        pos = end;
    }
    out
}

/// Extracts the descriptive fields from an XMP packet.
fn parse_xmp(xml: &str) -> TextMetadata {
    TextMetadata {
        title: element_text(xml, "dc:title").into_iter().next(),
        description: element_text(xml, "dc:description").into_iter().next(),
        creator: element_text(xml, "dc:creator").into_iter().next(),
        copyright: element_text(xml, "dc:rights").into_iter().next(),
        keywords: element_text(xml, "dc:subject"),
    }
}

/// All text values of the first `<tag>…</tag>` element. Containers (`rdf:Alt`, `rdf:Seq`,
/// `rdf:Bag`) yield one value per `rdf:li`; a plain element yields its own text.
fn element_text(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let Some(start) = find_open(xml, &open) else {
        return Vec::new();
    };
    let Some(body_start) = xml[start..].find('>').map(|i| start + i + 1) else {
        return Vec::new();
    };
    let Some(body_len) = xml[body_start..].find(&close) else {
        return Vec::new();
    };
    let body = &xml[body_start..body_start + body_len];

    let items = li_values(body);
    let values = if items.is_empty() {
        vec![strip_tags(body)]
    } else {
        items
    };
    values
        .into_iter()
        .map(|v| unescape(v.trim()))
        .filter(|v| !v.is_empty())
        .collect()
}

/// Finds `<tag` followed by a delimiter, so `<dc:title` does not match `<dc:titleX`.
fn find_open(xml: &str, open: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = xml[from..].find(open) {
        let at = from + i;
        let next = xml[at + open.len()..].chars().next();
        if matches!(
            next,
            Some('>') | Some(' ') | Some('\n') | Some('\r') | Some('\t') | Some('/')
        ) {
            return Some(at);
        }
        from = at + open.len();
    }
    None
}

fn li_values(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<rdf:li") {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let content = &after[gt + 1..];
        let Some(end) = content.find("</rdf:li>") else {
            break;
        };
        out.push(strip_tags(&content[..end]));
        rest = &content[end + "</rdf:li>".len()..];
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iim_datasets_are_parsed() {
        let mut iim = Vec::new();
        for (ds, text) in [
            (5u8, "Harbour"),
            (120, "Morning light"),
            (80, "A. Photographer"),
            (116, "(c) 2026"),
            (25, "sea"),
            (25, "boats"),
        ] {
            iim.extend_from_slice(&[0x1C, 2, ds]);
            iim.extend_from_slice(&(text.len() as u16).to_be_bytes());
            iim.extend_from_slice(text.as_bytes());
        }
        let meta = parse_iim(&iim);
        assert_eq!(meta.title.as_deref(), Some("Harbour"));
        assert_eq!(meta.description.as_deref(), Some("Morning light"));
        assert_eq!(meta.creator.as_deref(), Some("A. Photographer"));
        assert_eq!(meta.copyright.as_deref(), Some("(c) 2026"));
        assert_eq!(meta.keywords, vec!["sea", "boats"]);
    }

    #[test]
    fn photoshop_block_locates_iptc_resource_past_other_resources() {
        // One unrelated resource (id 0x03ED, odd length 3 padded to 4) before the IPTC one.
        let mut payload = PHOTOSHOP_HEADER.to_vec();
        payload.extend_from_slice(b"8BIM");
        payload.extend_from_slice(&0x03EDu16.to_be_bytes());
        payload.extend_from_slice(&[0, 0]); // empty pascal name, padded to 2 bytes
        payload.extend_from_slice(&3u32.to_be_bytes());
        payload.extend_from_slice(&[1, 2, 3, 0]); // data padded to even
        payload.extend_from_slice(b"8BIM");
        payload.extend_from_slice(&IPTC_RESOURCE_ID.to_be_bytes());
        payload.extend_from_slice(&[0, 0]);
        let block = [0x1C, 2, 5, 0, 2, b'h', b'i'];
        payload.extend_from_slice(&(block.len() as u32).to_be_bytes());
        payload.extend_from_slice(&block);
        let found = photoshop_iptc_block(&payload).expect("iptc block");
        assert_eq!(found, &block);
    }

    #[test]
    fn xmp_alt_bag_and_plain_elements() {
        let xml = r#"<x:xmpmeta><rdf:RDF><rdf:Description>
            <dc:title><rdf:Alt><rdf:li xml:lang="x-default">Sunset &amp; sea</rdf:li></rdf:Alt></dc:title>
            <dc:subject><rdf:Bag><rdf:li>beach</rdf:li><rdf:li>sun</rdf:li></rdf:Bag></dc:subject>
            <dc:creator><rdf:Seq><rdf:li>Jane Doe</rdf:li></rdf:Seq></dc:creator>
            <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">CC-BY</rdf:li></rdf:Alt></dc:rights>
            <dc:description>Plain text &lt;wow&gt;</dc:description>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let meta = parse_xmp(xml);
        assert_eq!(meta.title.as_deref(), Some("Sunset & sea"));
        assert_eq!(meta.keywords, vec!["beach", "sun"]);
        assert_eq!(meta.creator.as_deref(), Some("Jane Doe"));
        assert_eq!(meta.copyright.as_deref(), Some("CC-BY"));
        assert_eq!(meta.description.as_deref(), Some("Plain text <wow>"));
    }

    #[test]
    fn element_match_does_not_confuse_prefixes() {
        let xml = "<dc:titleExtra>wrong</dc:titleExtra><dc:title>right</dc:title>";
        assert_eq!(element_text(xml, "dc:title"), vec!["right"]);
    }

    #[test]
    fn iptc_wins_and_xmp_fills_gaps() {
        let iptc = TextMetadata {
            title: Some("IPTC title".into()),
            ..Default::default()
        };
        let xmp = TextMetadata {
            title: Some("XMP title".into()),
            creator: Some("XMP creator".into()),
            keywords: vec!["xmp".into()],
            ..Default::default()
        };
        let merged = merge(iptc, xmp);
        assert_eq!(merged.title.as_deref(), Some("IPTC title"));
        assert_eq!(merged.creator.as_deref(), Some("XMP creator"));
        assert_eq!(merged.keywords, vec!["xmp"]);
    }

    #[test]
    fn corrupt_iim_stops_cleanly() {
        assert_eq!(
            parse_iim(&[0x1C, 2, 120, 0xFF, 0xFF, 1]),
            TextMetadata::default()
        );
        assert_eq!(parse_iim(&[]), TextMetadata::default());
    }
}
