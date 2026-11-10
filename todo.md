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

Legend: `[x]` done in this repo · `[~]` partly done (note says what is missing) · `[ ]` not started
or needs something outside this repo (a person, real hardware, a native library, or an account).

---

## Status Summary

| Phase | Area | Progress |
|-------|------|----------|
| 0  | Project Setup & Licensing                          | 🟡 Mostly done (repo hosting outstanding) |
| 1  | Core Image Decode Engine                           | 🟡 Partial (common formats, AVIF, RAW, EXIF/IPTC/XMP done; HEIC/streaming open) |
| 2  | GPU-Accelerated Rendering (wgpu)                   | 🟡 Partial (offscreen tiled renderer tested on GPU; not yet shown in the app) |
| 3  | Directory Browsing & Persistent Thumbnail Cache    | ✅ Done (folder tree and mip-based thumbnails are backlog-level extras) |
| 4  | Basic Editing Tools                                | ✅ Done (numeric crop; on-canvas crop selection pending) |
| 5  | Batch Conversion & Rename Engine (Core)            | 🟡 Partial (AVIF output and IPTC/XMP preservation open) |
| 6  | CLI (Scriptable Batch Mode)                        | ✅ Done |
| 7  | Tauri Shell & IPC Layer                            | ✅ Done (built and launched locally; installer verification in progress) |
| 8  | Frontend — Viewer                                  | 🟡 Partial (Canvas 2D viewer; wgpu display not wired) |
| 9  | Frontend — Browser & Batch UI                      | 🟡 Partial (no folder tree; settings location fixed) |
| 10 | Windows Platform Integration                       | 🟡 Partial (registration plan written; not applied on a real machine) |
| 11 | Linux Platform Integration                         | 🟡 Partial (desktop/MIME files generated; not tested on a distro) |
| 12 | Archon Support (Scoped, Real Integration)          | 🟡 Partial (path→block layer and design doc; capability backend open) |
| 13 | Performance & Polish                               | 🟡 Partial (crash logging, benchmarks; branding and startup profiling open) |
| 14 | Testing & QA                                       | 🟡 Partial (automated tests done; manual QA open) |
| 15 | Release v1                                         | 🟡 Partial (docs and changelog done; installers built locally; tag and publish open) |

Automated tests at the time of writing: 122 passing across the workspace (plus one ignored
regeneration helper), `cargo fmt --check` and `cargo clippy -D warnings` clean, `tsc --noEmit` clean.

---

## Phase 0 — Project Setup & Licensing
- [x] Init git repo, `.gitignore` (Rust + Node/Tauri targets + Windows/Linux build artifacts)
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (dual license, TPT Solutions copyright)
- [x] Add SPDX dual-license header/notice convention for source files (`CONTRIBUTING.md`)
- [x] Cargo workspace: `core/`, `platform-windows/`, `platform-linux/`, `platform-archon/`, `cli/`,
      `render/`, `src-tauri/`, and `src/` (TS/HTML frontend, Vite); shared `[workspace.package]`
- [x] `rust-toolchain.toml` (MSRV pin), `rustfmt.toml`, `clippy.toml`
- [x] `tauri.conf.json` (app id, window defaults, bundle identifiers, file associations)
- [x] TypeScript config (tsconfig, Vite, no framework)
- [x] `README.md` (from spec.txt), build instructions, license badge, documentation links
- [x] CI workflow (GitHub Actions, build+lint+test on Windows + Linux runners) —
      `.github/workflows/ci.yml` is written but has not run on GitHub
- [ ] Repo hosting under `tpt-solutions` GitHub org — external; not done from this repo

## Phase 1 — Core Image Decode Engine
- [x] Integrate `image` crate for common raster formats: JPEG, PNG, GIF (including animated),
      BMP, TIFF, WebP (`core/src/codec.rs`)
- [x] AVIF decode and encode: dav1d built from source (through `dav1d-sys`, meson + ninja + NASM +
      pkgconf) and the rav1e-based encoder. Covered by `core/tests/avif.rs` and the format matrix.
- [~] HEIC/HEIF decode — blocked on Windows. libde265 is built and installed under
      `~\tools\libde265` (approved and done). `libheif-sys` on MSVC does not use its embedded build;
      it needs libheif from vcpkg. Cloning and bootstrapping vcpkg was denied by the auto-mode
      permission check, so libheif is not installed. HEIC is not wired into the code yet.
- [~] RAW camera decode via `rawler` 0.7 (`core/src/raw.rs`): CR2, CR3, NEF, ARW, DNG, ORF, RAF,
      RW2, PEF, SRW. Developed through rawler's pipeline (demosaic, white balance, sRGB). Tested on
      synthetic Bayer data and error paths only — **no real camera sample has been run**, and
      coverage depends on rawler's camera support.
- [~] EXIF/IPTC/XMP metadata — EXIF (orientation, make/model, DateTimeOriginal, GPS) and
      IPTC-IIM plus XMP text fields (title, description, creator, copyright, keywords) are read
      from JPEG (`core/src/text_meta.rs`). PNG/TIFF XMP and writing metadata are not done.
- [ ] Streaming/progressive decode path for very large (gigapixel-scale) images — not started
- [x] Shared `ImageBackend`-style trait (probe, decode, decode_frames, read_metadata) so
      platform crates and the Archon backend share one interface (`core/src/backend.rs`)
- [~] Unit tests: decode correctness across the format matrix (PNG/BMP/TIFF/WebP/JPEG/GIF),
      corrupt-file handling (truncation, byte-flip fuzz, non-image input), EXIF orientation (all 8),
      metadata parsing. RAW edge cases across manufacturers are pending real samples.

## Phase 2 — GPU-Accelerated Rendering (wgpu)
- [~] wgpu render pipeline (`render/src/lib.rs`): textured quads sampled per tile, offscreen target
      with readback. Tested on a real adapter (Intel Iris Xe) with pixel-exact checks. Drawing to
      a window surface in the desktop app is not wired (see Phase 8).
- [~] Tiled texture upload (2048 px tiles, seams verified). Mip-chain generation on the GPU is not
      done; mip-level selection math exists in `core/src/tiling.rs`.
- [~] GPU pan/zoom with nearest and linear sampling. Bicubic and Lanczos on the GPU are not done.
- [x] Backend selection: primary backends by default, `TPT_VIEW_BACKEND` override, software
      fallback adapter, and a clear `NoAdapter` error for CPU fallback.
- [x] HDR/wide-gamut scope recorded: v1 is SDR sRGB; deferral reasons and a later plan in
      `docs/hdr-scope.md`.
- [x] Frame-time benchmark: `render/examples/frame_bench.rs` (≈5 ms per 1080p frame at 1:1 and
      ≈15 ms at fit-to-window on integrated graphics, including readback).

## Phase 3 — Directory Browsing & Persistent Thumbnail Cache
- [x] Directory scanning implementation (own code — not derived from Archon) (`core/src/scan.rs`)
- [x] Filesystem watcher for live directory updates, with coalescing (`core/src/watch.rs`)
- [x] Thumbnail generation pipeline built on Phase 1's decode engine (`core/src/thumbs.rs`)
- [x] Persistent on-disk thumbnail cache, keyed by path+size+mtime+edge length, with invalidation on
      file change and pruning (`<cache_dir>/<fnv1a>.png`, file-based — not SQLite)
- [x] Instant re-open benchmarked: `core/examples/cache_bench.rs` — 200 images, cold 2.07 s versus
      warm 6.5 ms
- [x] Virtualized grid data structure (`core/src/grid.rs`), mirrored in the browser
- [x] Tests: cache invalidation, scan filtering, watcher events, cold versus warm benchmark

## Phase 4 — Basic Editing Tools
- [~] Crop tool — numeric crop (x, y, width, height) in the edit panel. Freeform and fixed-ratio
      crop with an on-canvas selection are not built.
- [x] Rotate (90°-step and arbitrary angle with optional canvas expand), horizontal/vertical flip
- [x] Resize with selectable resampling filter (nearest, bilinear, bicubic, Lanczos3)
- [x] Brightness, contrast, saturation, levels (black/white/gamma) and tone curves
- [x] Non-destructive edit session: originals untouched; save as a new file or overwrite behind an
      explicit confirmation enforced in core (`core/src/edit.rs`)
- [x] Undo/redo stack (bounded at 32 snapshots)
- [x] Unit tests: pixel-level spot checks per operation, undo/redo, failed-op safety, save flow

## Phase 5 — Batch Conversion & Rename Engine (Core)
- [x] Batch format conversion: JPEG, PNG, WebP, TIFF, BMP and AVIF output.
- [x] Batch resize (exact, max-edge that never upscales, percentage)
- [x] Pattern-based rename engine (`{counter:N}`, `{name:lower|upper}`, `{date}`, `{camera}`)
      (`core/src/rename.rs`)
- [~] Metadata policy per job: strip (default) or preserve EXIF for JPEG→JPEG only
      (`core/src/jpeg_meta.rs`). IPTC/XMP are not carried over, and EXIF is not carried between formats.
- [x] Progress reporting and cancellable jobs (cancellation is checked between files)
- [x] Job engine shared by the GUI and CLI (`core/src/batch.rs`)
- [x] Tests: format matrix (6 sources × 5 outputs), rename correctness, EXIF preserve/strip,
      cancellation between files, invalid-settings rejection

## Phase 6 — CLI (Scriptable Batch Mode)
- [x] Standalone CLI binary invoking the Phase 5 batch engine headlessly (`cli/src/main.rs`)
- [x] Argument syntax for convert/resize/rename/metadata options (shell expands globs)
- [x] Scriptable exit codes (0, 1, 2, 3, 130) and `--json` report
- [x] Ctrl+C cancels between files and exits 130 (the handler itself is not covered by an
      automated test)
- [x] CLI help (`--help`) and reference (`docs/cli.md`)
- [x] Integration tests: 9-case CLI matrix (formats, resize modes, rename, overwrite, JSON, errors,
      exit codes) in `cli/tests/cli_matrix.rs`

## Phase 7 — Tauri Shell & IPC Layer
- [x] Tauri app scaffold wired to `core` (`src-tauri/`), with capabilities and CSP
- [x] Command layer: list directory, thumbnails, image info, preview, edit session commands,
      batch run and cancel, directory watch, cache clear, crash-logging toggle
- [x] Event push from Rust to frontend: `batch-progress`, `dir-changed`
- [~] File association / launch handling — launch argument opens the file or folder, and bundle
      `fileAssociations` are configured. Verified only as a build; not yet tested on an installed
      OS.
- [x] IPC boundary tests: path validation (absolute, no `..`, no NUL, existence and type), output
      extension checks, DTO parsing, launch-argument selection (`src-tauri/src/ipc.rs`)

## Phase 8 — Frontend — Viewer
- [x] App shell (plain TS/HTML + Vite), light/dark/system theme
- [x] Main viewer: pan (drag), zoom (wheel and keys), fit-to-window (never enlarges), actual size,
      fullscreen (`src/viewer.ts`)
- [~] Slideshow — fixed 4-second interval, no transitions, interval not configurable
- [x] Keyboard and mouse navigation (next/prev, zoom, fit, actual, rotate-as-edit); full table in
      `docs/user-guide.md`
- [x] Image info/EXIF panel, including IPTC/XMP text fields (`src/info-panel.ts`)
- [~] Wire viewer to the wgpu renderer — decode is via IPC, but drawing is Canvas 2D. Hooking the
      renderer in needs a decision on window surface versus per-frame readback.

## Phase 9 — Frontend — Browser & Batch UI
- [x] Thumbnail grid browser: virtualized, lazy thumbnails with a concurrency limit, selection
- [~] Folder navigation — breadcrumbs with click-to-jump and parent navigation. A folder tree in
      the sidebar is not built.
- [x] Multi-select (click, Ctrl, Shift, Ctrl+A)
- [x] Batch convert/rename dialog wired to Phase 5 (format, resize, rename with presets, EXIF,
      overwrite, live progress, cancel, per-file results)
- [~] Basic editing UI (`src/edit-panel.ts`) — numeric crop only; sliders and levels are wired
- [~] Settings dialog (`src/settings-dialog.ts`): default format, JPEG quality, thumbnail size,
      rename presets, theme, crash logging, cache clear. The output folder is picked per batch
      rather than stored as a default.

## Phase 10 — Windows Platform Integration
- [~] File association / default-app registration: `platform-windows/src/file_assoc.rs` builds the
      per-user registry plan as data (tested). `apply()` is compiled on Windows but has not been run,
      because it writes to HKCU. Windows does not allow apps to set the default themselves, so the
      plan registers capabilities and leaves the choice to the user.
- [ ] Explorer thumbnail/shell integration — no shell extension is built, so this does not conflict
      with Explorer. A short decision note is still missing.
- [~] Windows packaging: NSIS installer builds locally (`tpt-view_0.1.0_x64-setup.exe`, per-user
      install, WebView2 bootstrapper, file associations). Not yet installed and run on a clean machine.
      Code signing is out of scope for v1 (decided by the project owner).
- [ ] Manual QA: Windows 10/11, GPU backend verification (Vulkan/DX12), file association flow —
      needs people and machines

## Phase 11 — Linux Platform Integration
- [ ] Vulkan/OpenGL backend verification across common distros — needs hardware and distros
- [x] MIME type / file-association integration: `.desktop` entry and shared-mime-info package
      generated by `platform-linux` and kept in sync by a test (`packaging/linux/`)
- [~] Linux packaging: `.deb` with desktop and MIME files and dependencies configured; AppImage
      target enabled. Not built on Linux here.
- [ ] Manual QA: GNOME and KDE, X11 and Wayland — needs sessions

## Phase 12 — Archon Support (Scoped, Real Integration)
- [ ] `platform-archon` backend implementing the `ImageBackend` trait over Archon — not started
      (needs `tpt-archon-bridge` as a dependency)
- [x] Own path→block-id mapping layer (`platform-archon/src/path_map.rs`): never reuses ids,
      retires ranges on growth or removal, tested without Archon
- [ ] Layer the thumbnail/decode pipeline over Archon's `UnifiedPageCache`/`CorePageCache` —
      not started
- [x] `docs/archon-design.md`: real-versus-aspirational scope, proposed upstream primitives, test
      strategy, open questions
- [x] Cargo feature flag (`archon`) gating the backend; off by default, so Windows/Linux builds never
      depend on it
- [ ] Unit/integration tests against `tpt-archon-bridge`'s test capability issuer — not started

## Phase 13 — Performance & Polish
- [~] Benchmark huge/gigapixel-scale decode and display latency — frame-time benchmark exists
      (6000×4000 tiles); no gigapixel decode benchmark yet
- [ ] Startup time profiling (cold start, warm thumbnail-cache start) — not done
- [~] App icon: a generated placeholder ("t" on blue) is in `src-tauri/icons/`. TPT Solutions branding
      assets are still to come.
- [~] Accessibility pass — toolbar and dialogs have labels and roles; keyboard navigation covers the
      main flows. Screen-reader testing has not been done.
- [x] Crash reporting / local error logging (opt-in only): off by default; a panic hook writes to
      the app log directory only when enabled (`src-tauri/src/diagnostics.rs`)

## Phase 14 — Testing & QA
- [x] Unit tests: decode engine, batch conversion, thumbnail cache, edit operations, rename, grid,
      tiling, watcher, RAW, metadata, IPC validation
- [x] Integration tests: browse → thumbnail → edit → save → batch (`core/tests/pipeline.rs`),
      CLI invocation matrix (`cli/tests/cli_matrix.rs`), format matrix, EXIF and IPTC/XMP end-to-end
- [ ] Manual QA: Windows 10/11 (GPU backend, file association, batch/CLI flows)
- [ ] Manual QA: Linux (X11 + Wayland, GNOME and KDE)
- [~] Performance QA: directories with tens of thousands of images — the cache benchmark covers 200
      images. A run at tens of thousands is still to do.
- [~] Security review: `docs/security-review.md` covers path handling, IPC, overwrite safety and
      crash logging, with open items (file-access scoping, fuzzing, memory budget)

## Phase 15 — Release v1
- [x] Versioning scheme (SemVer, pre-1.0 rules) and changelog process (`docs/versioning.md`,
      `CHANGELOG.md`)
- [~] Windows installer (NSIS via the Tauri bundler) — builds locally; clean-machine install not yet tested
- [~] Linux packaging (`.deb`, AppImage) — configured; not built here
- [x] LICENSE files included as bundle resources
- [x] User-facing docs: install guide, user guide, CLI reference
- [ ] Tag v1.0.0 and publish to GitHub (tpt-solutions org) — publishing, deliberately not done
- [x] Post-release triage process (`docs/triage.md`)

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
- [ ] HEIC decoding (needs libde265 + libheif; see Phase 1)
- [ ] Display frames through the wgpu renderer inside the desktop app
