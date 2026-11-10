# Changelog

All notable changes to tpt-view are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) as described in `docs/versioning.md`.

## [Unreleased]

### Added

- Core decode engine: JPEG, PNG, GIF (animated), BMP, TIFF, WebP, AVIF (decode and encode) and
  camera RAW (CR2, CR3, NEF, ARW, DNG, ORF, RAF, RW2, PEF, SRW) via rawler.
- EXIF metadata (orientation, make, model, date, GPS) and IPTC/XMP text fields for JPEG.
- Basic editing: crop, rotate (quarter turns and arbitrary angle), flip, resize, brightness,
  contrast, saturation, levels and tone curves, with undo/redo and non-destructive save.
- Directory browsing: scan, live watcher, persistent thumbnail cache, virtualized grid.
- Batch convert and rename: JPEG, PNG, WebP, TIFF and BMP output; resize; rename patterns; EXIF
  preservation for JPEG→JPEG; cancellation and progress.
- Scriptable CLI (`tpt-view-cli convert`) with JSON output, documented exit codes and Ctrl+C
  cancellation.
- Tauri desktop shell with a canvas viewer, browser, edit panel, batch dialog, settings, breadcrumb
  navigation, keyboard shortcuts and a slideshow.
- Tiled wgpu renderer (offscreen, with readback) and a frame-time benchmark.
- Windows file-association plan and Linux `.desktop` and shared-mime-info packaging files.
- Opt-in local crash logging.

### Not yet included

- HEIC decoding (needs native libheif and libde265).
- Displaying frames through the wgpu renderer in the desktop app (the viewer is Canvas 2D).
- Archon backend (the path-to-block layer exists; the capability-backed cache does not).
