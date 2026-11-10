// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Linux platform integration (Phase 11).
//!
//! Generates the freedesktop.org `.desktop` entry and the shared-mime-info package so that
//! distribution packages and the in-app "set as default" flow use one source of truth. The
//! files under `packaging/linux/` are produced by [`desktop_entry`] and [`mime_package_xml`];
//! a test keeps them in sync.

use tpt_view_core::scan::{mime_for_extension, SUPPORTED_EXTENSIONS};

/// Application id used for the `.desktop` file and MIME package file names.
pub const APP_ID: &str = "tpt-view";

/// Every MIME type the decode engine supports, deduplicated and sorted.
pub fn supported_mime_types() -> Vec<&'static str> {
    let mut types: Vec<&'static str> = SUPPORTED_EXTENSIONS
        .iter()
        .filter_map(|ext| mime_for_extension(ext))
        .collect();
    types.sort_unstable();
    types.dedup();
    types
}

/// Contents of `tpt-view.desktop`. `Exec=%F` lets the desktop pass several files at once.
pub fn desktop_entry() -> String {
    let mut out = String::new();
    out.push_str("[Desktop Entry]\n");
    out.push_str("Type=Application\n");
    out.push_str("Version=1.0\n");
    out.push_str("Name=tpt-view\n");
    out.push_str("GenericName=Image Viewer\n");
    out.push_str("Comment=Fast image viewer and batch converter\n");
    out.push_str("Exec=tpt-view %F\n");
    out.push_str("Icon=tpt-view\n");
    out.push_str("Terminal=false\n");
    out.push_str("Categories=Graphics;Viewer;Photography;\n");
    out.push_str("Keywords=image;photo;viewer;convert;batch;\n");
    out.push_str("StartupNotify=true\n");
    out.push_str("StartupWMClass=tpt-view\n");
    out.push_str("MimeType=");
    for mime in supported_mime_types() {
        out.push_str(mime);
        out.push(';');
    }
    out.push('\n');
    out
}

/// Contents of the shared-mime-info package file. Each supported type gets its extension glob so
/// file managers match by name even when content sniffing is unavailable.
pub fn mime_package_xml() -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n",
    );
    for mime in supported_mime_types() {
        let globs: Vec<&str> = SUPPORTED_EXTENSIONS
            .iter()
            .copied()
            .filter(|ext| mime_for_extension(ext) == Some(mime))
            .collect();
        out.push_str(&format!("  <mime-type type=\"{mime}\">\n"));
        for ext in globs {
            out.push_str(&format!("    <glob pattern=\"*.{ext}\"/>\n"));
            out.push_str(&format!(
                "    <glob pattern=\"*.{}\"/>\n",
                ext.to_ascii_uppercase()
            ));
        }
        out.push_str("  </mime-type>\n");
    }
    out.push_str("</mime-info>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_advertises_every_supported_mime() {
        let entry = desktop_entry();
        for mime in supported_mime_types() {
            assert!(entry.contains(mime), "missing {mime}");
        }
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("Exec=tpt-view %F"));
    }

    #[test]
    fn mime_xml_covers_each_extension_in_both_cases() {
        let xml = mime_package_xml();
        for ext in SUPPORTED_EXTENSIONS {
            assert!(
                xml.contains(&format!("*.{ext}\"")),
                "missing lowercase {ext}"
            );
            assert!(xml.contains(&format!("*.{}\"", ext.to_ascii_uppercase())));
        }
        assert_eq!(
            xml.matches("<mime-type ").count(),
            supported_mime_types().len()
        );
    }

    #[test]
    fn mime_list_is_unique_and_sorted() {
        let types = supported_mime_types();
        let mut sorted = types.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(types, sorted);
    }

    #[test]
    fn packaged_files_match_generators() {
        // Keeps packaging/linux/ from drifting from the generator output.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/linux");
        let desktop = std::fs::read_to_string(root.join("tpt-view.desktop")).unwrap();
        assert_eq!(desktop, desktop_entry());
        let xml = std::fs::read_to_string(root.join("tpt-view-mime.xml")).unwrap();
        assert_eq!(xml, mime_package_xml());
    }
}

#[cfg(test)]
mod regenerate {
    /// Rewrites `packaging/linux/` from the generators. Run with:
    /// `cargo test -p tpt-view-linux-platform -- --ignored regenerate_packaging`
    #[test]
    #[ignore = "writes files; run manually after changing the generators"]
    fn regenerate_packaging() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/linux");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("tpt-view.desktop"), super::desktop_entry()).unwrap();
        std::fs::write(root.join("tpt-view-mime.xml"), super::mime_package_xml()).unwrap();
    }
}
