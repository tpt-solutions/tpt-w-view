# tpt-view — Project Checklist

TPT Solutions — dual-licensed under MIT OR Apache-2.0.
A hardware-accelerated, instant-loading image viewer & batch converter (IrfanView / XnView
alternative). Rust core (`image` crate + RAW/HEIC decoding, GPU acceleration via wgpu) + Tauri UI
(plain TS/HTML frontend). Targets Windows and Linux for v1, with a scoped Archon backend.

Reference: Archon platform — https://github.com/tpt-solutions/tpt-archon

Decisions locked in for this roadmap:
- **RAW camera format support ships in v1**, not backlog — it's a stated differentiator in
  spec.txt, so broad camera-model RAW decode is a core Phase 1 deliverable alongside common
  formats (JPEG/PNG/GIF/BMP/TIFF/WebP/AVIF/HEIC).
- **Basic editing tools ship in v1**: crop, rotate/flip, resize, basic color/levels adjustment —
  feature parity with IrfanView/XnView, not a pure view-only tool.
- **A scriptable CLI ships in v1**, separate from the GUI, for headless/scripted batch conversion
  (IrfanView `/convert`-style automation).
- **Persistent disk-backed thumbnail cache**, not in-memory-only — needed to actually deliver
  spec.txt's "instant... scrolling through directories with tens of thousands of images" goal on
  Windows and Linux, independent of any Archon-specific backend.
- **Archon integration is scoped and real, not aspirational.** Investigation of the freshly-cloned
  `tpt-archon` repo found `tpt-archon-bridge`'s `UnifiedPageCache`/`CorePageCache<D: BlockDevice>`
  and `Capability`/`Resource::Page` machinery is real (zero-copy `map_read`/`map_write`/`unmap`
  over block IDs), but there is **no filesystem/directory semantics** (`Resource` only has
  `Page`/`Channel`/`Device` variants — no path, directory, or file-metadata concept anywhere), **no
  image decoding/thumbnailing code**, and **no GPU/rendering/texture capability** anywhere in the
  Archon workspace (its only "gpu" code is an IR-emission-only crate for the SQL engine's
  vectorized scans, unrelated to rendering). So spec.txt's claim that Archon provides "instant,
  zero-allocation scrolling through directories" via the unified page cache is aspirational
  marketing copy, not existing capability. tpt-view's `platform-archon` backend therefore builds
  its own directory-walk/watch layer, its own image-decode/thumbnail pipeline, and its own path→
  block-id mapping — reusing only Archon's raw `UnifiedPageCache`/`CorePageCache` as a generic
  zero-copy byte cache for decoded thumbnail/pixel data keyed by block id, exactly mirroring how
  tpt-vault layers its own crypto engine over the same page cache. A `docs/archon-design.md`
  documents this scope and proposes upstream primitives (a directory/file-aware `Resource`
  variant, a real GPU/texture resource) that could deepen the integration later.

---

## Status Summary

| Phase | Area | Progress |
|-------|------|----------|
| 0  | Project Setup & Licensing                          | ❌ Not Started |
| 1  | Core Image Decode Engine                           | ❌ Not Started |
| 2  | GPU-Accelerated Rendering (wgpu)                   | ❌ Not Started |
| 3  | Directory Browsing & Persistent Thumbnail Cache    | ❌ Not Started |
| 4  | Basic Editing Tools                                | ❌ Not Started |
| 5  | Batch Conversion & Rename Engine (Core)            | ❌ Not Started |
| 6  | CLI (Scriptable Batch Mode)                        | ❌ Not Started |
| 7  | Tauri Shell & IPC Layer                             | ❌ Not Started |
| 8  | Frontend — Viewer                                   | ❌ Not Started |
| 9  | Frontend — Browser & Batch UI                       | ❌ Not Started |
| 10 | Windows Platform Integration                        | ❌ Not Started |
| 11 | Linux Platform Integration                          | ❌ Not Started |
| 12 | Archon Support (Scoped, Real Integration)           | ❌ Not Started |
| 13 | Performance & Polish                                | ❌ Not Started |
| 14 | Testing & QA                                        | ❌ Not Started |
| 15 | Release v1                                          | ❌ Not Started |

---

## Phase 0 — Project Setup & Licensing
- [ ] Init git repo, `.gitignore` (Rust + Node/Tauri targets + Windows/Linux build artifacts)
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` (dual license, TPT Solutions copyright)
- [ ] Add SPDX dual-license header/notice convention for source files
- [ ] Cargo workspace: `core/` (image decode, thumbnail cache, batch engine, shared traits),
      `platform-windows/`, `platform-linux/`, `platform-archon/`, `cli/`, `src-tauri/`, `src/`
      (TS/HTML frontend, Vite); shared `[workspace.package]` (`license`, `authors = ["TPT
      Solutions"]`, `repository`)
- [ ] `rust-toolchain.toml` (MSRV pin), `rustfmt.toml`, `clippy.toml`
- [ ] Configure `tauri.conf.json` (app id, window defaults, bundle identifiers)
- [ ] TypeScript config (tsconfig, Vite, no framework)
- [ ] `README.md` (from spec.txt), build instructions, license badges
- [ ] Set up CI (GitHub Actions, build+lint+test on Windows + Linux runners)
- [ ] Repo hosting under `tpt-solutions` GitHub org

## Phase 1 — Core Image Decode Engine
- [ ] Integrate `image` crate for common raster formats: JPEG, PNG, GIF (including animated),
      BMP, TIFF, WebP
- [ ] AVIF decode support (e.g. `libavif`/`ravif` bindings)
- [ ] HEIC/HEIF decode support
- [ ] RAW camera format decode (e.g. `rawler` or equivalent), broad camera-model coverage,
      demosaicing pipeline
- [ ] EXIF/IPTC/XMP metadata extraction (orientation, camera info, GPS, timestamps)
- [ ] Streaming/progressive decode path for very large (gigapixel-scale) images, avoiding full
      in-memory decode where possible
- [ ] Shared `ImageBackend`-style trait (decode, metadata, thumbnail-source extraction) so
      platform crates and the Archon backend share one interface
- [ ] Unit tests: decode correctness across format matrix, corrupt-file handling, RAW edge cases
      (multiple camera manufacturers), EXIF orientation correctness

## Phase 2 — GPU-Accelerated Rendering (wgpu)
- [ ] wgpu render pipeline for instant on-screen display of decoded images
- [ ] Texture upload/streaming path for huge images (tiled upload, mipmap generation)
- [ ] GPU-accelerated pan/zoom with high-quality resampling (bilinear/bicubic/Lanczos on GPU)
- [ ] Backend selection: Vulkan on Linux, Vulkan/DX12 on Windows, graceful fallback path
- [ ] HDR/wide-gamut display consideration (document scope; full HDR pipeline may defer)
- [ ] Frame-time/latency benchmarks for large-image display and rapid next/prev navigation

## Phase 3 — Directory Browsing & Persistent Thumbnail Cache
- [ ] Directory scanning implementation (own code — not derived from Archon, which has no
      filesystem semantics)
- [ ] Filesystem watcher for live directory updates (new/removed/renamed files)
- [ ] Thumbnail generation pipeline built on Phase 1's decode engine
- [ ] Persistent on-disk thumbnail cache (e.g. SQLite or cache-dir + index), keyed by
      path+mtime+size, with invalidation on file change
- [ ] Instant re-open of previously-browsed large directories (cache hit path measured/benchmarked)
- [ ] Virtualized/incremental grid data structure supporting tens of thousands of entries without
      full upfront thumbnail generation
- [ ] Unit/integration tests: cache invalidation correctness, watcher event handling, cold-start
      vs warm-cache directory open benchmarks

## Phase 4 — Basic Editing Tools
- [ ] Crop tool (freeform + fixed aspect ratios)
- [ ] Rotate (90°-step) and arbitrary-angle rotate, horizontal/vertical flip
- [ ] Resize with selectable resampling algorithm/quality
- [ ] Basic color/levels adjustment: brightness, contrast, saturation, basic curves/levels
- [ ] Non-destructive edit session (edits applied on save, original preserved until overwrite
      confirmed, or explicit "save as")
- [ ] Undo/redo stack for the active edit session
- [ ] Unit tests: edit operation correctness (pixel-level spot checks), non-destructive save flow

## Phase 5 — Batch Conversion & Rename Engine (Core)
- [ ] Batch format conversion: any Phase 1 source format → common output formats (JPEG, PNG,
      WebP, AVIF, TIFF, BMP)
- [ ] Batch resize as part of conversion (target dimensions/max-edge/percentage scaling)
- [ ] Pattern-based batch rename engine (counter, EXIF date/camera tokens, original name tokens,
      case transforms)
- [ ] Metadata preservation vs stripping options (EXIF/IPTC/XMP) per batch job
- [ ] Progress reporting and cancellable jobs (per-file and overall progress)
- [ ] Job engine designed as a shared library so both the GUI (Phase 9) and CLI (Phase 6) drive
      the same code path
- [ ] Unit tests: conversion round-trips across format matrix, rename pattern correctness,
      metadata preservation/stripping verification, cancellation mid-job

## Phase 6 — CLI (Scriptable Batch Mode)
- [ ] Standalone CLI binary invoking the Phase 5 batch engine headlessly
- [ ] Argument syntax for convert/resize/rename operations (input glob/dir, output format,
      rename pattern, metadata options)
- [ ] Scriptable exit codes and machine-readable output mode (for automation pipelines)
- [ ] CLI help/usage documentation
- [ ] Integration tests: CLI invocation matrix against the same fixtures used for Phase 5 engine
      tests

## Phase 7 — Tauri Shell & IPC Layer
- [ ] Tauri app scaffold wired to `core` (decode engine, thumbnail cache, batch engine)
- [ ] Command layer: open directory/file, decode/render requests, editing operations, batch job
      control
- [ ] Event push from Rust to frontend (thumbnail-ready, decode-complete, batch progress)
- [ ] File association / "open with" launch handling (path passed as CLI arg to the GUI binary)
- [ ] IPC boundary tests (command validation, no arbitrary path escape beyond intended scope)

## Phase 8 — Frontend — Viewer
- [ ] App shell (plain TS/HTML + Vite), light/dark theme
- [ ] Main viewer: pan/zoom, fit-to-window/actual-size toggle, fullscreen mode
- [ ] Slideshow mode (configurable interval, transition)
- [ ] Keyboard and mouse navigation (next/prev, zoom shortcuts, rotate shortcuts)
- [ ] Image info/EXIF panel (camera, exposure, GPS if present, file info)
- [ ] Wire viewer to Phase 2's wgpu render pipeline and Phase 1's decode engine via IPC

## Phase 9 — Frontend — Browser & Batch UI
- [ ] Thumbnail grid browser view wired to Phase 3's cache, virtualized scrolling
- [ ] Folder tree / breadcrumb navigation
- [ ] Multi-select for batch target selection
- [ ] Batch convert/rename dialog wired to Phase 5's engine (format, rename pattern, metadata
      options, progress UI)
- [ ] Basic editing tool UI (crop/rotate/resize/color adjust) wired to Phase 4
- [ ] Settings: default output format/location, thumbnail cache size/location, rename pattern
      presets

## Phase 10 — Windows Platform Integration
- [ ] File association / "set as default image viewer" registration flow
- [ ] Explorer thumbnail/shell integration considerations (does not need to replace Explorer's
      own thumbnailer, but should not conflict with it)
- [ ] Windows-specific packaging (installer prerequisites, code signing consideration)
- [ ] Manual QA: Windows 10/11, GPU backend verification (Vulkan/DX12), file association flow

## Phase 11 — Linux Platform Integration
- [ ] Vulkan/OpenGL backend verification across common distros
- [ ] MIME type / file-association integration (freedesktop.org `.desktop` entry, MIME database)
- [ ] Linux-specific packaging considerations (see Phase 15 for installer formats)
- [ ] Manual QA: at least one GNOME and one KDE session, X11 and Wayland

## Phase 12 — Archon Support (Scoped, Real Integration)
- [ ] `platform-archon` crate implementing the Phase 1 `ImageBackend`-style trait, with its own
      directory-walk/watch layer (Archon has no filesystem semantics to build on)
- [ ] Own path→block-id mapping layer (Archon's `Resource::Page` is a raw block id, not a file
      identity — tpt-view must own this mapping)
- [ ] Layer the Phase 3 thumbnail/decode pipeline over Archon's real `UnifiedPageCache`/
      `CorePageCache` (`map_read`/`map_write`/`unmap`), used as a generic zero-copy cache for
      decoded thumbnail/pixel bytes keyed by block id — mirroring tpt-vault's pattern of layering
      its own crypto engine over the same page cache
      obtained via `CapabilityIssuer::mint`-issued capabilities
- [ ] `docs/archon-design.md`: documents this real-vs-aspirational scope (no filesystem, no image
      decode, no GPU/texture support exists upstream in Archon today), and proposes future
      upstream primitives — a directory/file-aware `Resource` variant, a real GPU/texture
      resource — that would let a future version integrate more deeply
- [ ] Cargo feature flag (`archon`) gating this backend so it never blocks Windows/Linux v1
- [ ] Unit/integration tests against `tpt-archon-bridge`'s test capability issuer (CI-safe, no
      real Archon kernel required)

## Phase 13 — Performance & Polish
- [ ] Benchmark huge/gigapixel-scale image decode and display latency
- [ ] Startup time profiling (cold start, warm thumbnail-cache start)
- [ ] App icon, branding assets (TPT Solutions)
- [ ] Accessibility pass (keyboard navigation, screen reader labels across all views)
- [ ] Crash reporting / local error logging (opt-in only)

## Phase 14 — Testing & QA
- [ ] Unit tests: decode engine (format matrix, RAW/HEIC edge cases), batch conversion round-
      trips, thumbnail cache invalidation, edit operations
- [ ] Integration tests: directory browse → thumbnail → open → edit → save, batch job end-to-end,
      CLI invocation matrix
- [ ] Manual QA: Windows 10/11 (GPU backend, file association, batch/CLI flows)
- [ ] Manual QA: Linux (X11 + Wayland, at least one GNOME and one KDE session)
- [ ] Performance QA: directories with tens of thousands of images (cold-start and warm-cache
      scroll performance)
- [ ] Security review: batch/CLI path handling (no path traversal beyond intended scope), IPC
      command validation, Archon capability boundary (no capability forgery/reuse)

## Phase 15 — Release v1
- [ ] Finalize versioning scheme (SemVer) and changelog process
- [ ] Windows installer (MSI/NSIS via Tauri bundler)
- [ ] Linux packaging (.deb/.rpm/AppImage via Tauri bundler)
- [ ] Publish LICENSE files + notices in release artifacts
- [ ] User-facing docs (install guide, viewer/editor guide, batch/CLI usage guide)
- [ ] Tag v1.0.0 release, publish to GitHub (tpt-solutions org)
- [ ] Post-release triage process for bug reports

---

## Backlog / Future Phases (post-v1)
- [ ] macOS support
- [ ] Plugin/extension system
- [ ] Print support
- [ ] Color management (ICC profile support)
- [ ] Advanced editing (layers, filters beyond basic adjust)
- [ ] Cloud storage source integration (browse/convert from cloud-synced folders directly)
- [ ] Real Archon-native directory/file-resource capability once upstream lands (replacing the
      hand-rolled path→block-id layer with a first-class Archon primitive)
