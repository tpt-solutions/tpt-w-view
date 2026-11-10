# tpt-view

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

A hardware-accelerated, instant-loading image viewer and batch converter — an IrfanView / XnView
alternative with no ads, no nag screens and no registration prompts.

- **Core:** Rust. Decodes JPEG, PNG, GIF, BMP, TIFF, WebP and camera RAW. Editing, thumbnail
  cache, batch engine and a scriptable CLI all share one code path.
- **Desktop:** Tauri shell with a plain TypeScript frontend (Vite, no framework).
- **GPU:** wgpu renderer with tiled textures, offscreen readback and nearest/linear sampling.
- **Platforms:** Windows and Linux for v1. An optional Archon backend is gated behind the
  `archon` Cargo feature.

Built by TPT Solutions. Licensed under **MIT OR Apache-2.0**.

## Status

Pre-1.0. See [todo.md](todo.md) for the phase-by-phase roadmap and what is still open
(HEIC decoding, GPU display in the desktop app, Archon capability backend, and the
manual QA and release steps). User-facing changes are listed in [CHANGELOG.md](CHANGELOG.md).

## Repository layout

| Path | Purpose |
|------|---------|
| `core/` | Decode engine, RAW, metadata, editing, batch, thumbnails, watcher, tiling math |
| `cli/` | `tpt-view-cli`: headless batch conversion |
| `render/` | wgpu renderer (tiles, sampling, readback) and frame benchmark |
| `src-tauri/` | Tauri desktop shell and IPC commands |
| `src/` | TypeScript/HTML frontend |
| `platform-windows/` | File-association registry plan |
| `platform-linux/` | `.desktop` and shared-mime-info generators |
| `platform-archon/` | Path→block-id layer (Archon backend behind a feature flag) |
| `packaging/linux/` | Generated `.desktop` and MIME package files |
| `docs/` | User guide, CLI reference, install guide, design and security notes |

## Building

Prerequisites: the Rust toolchain (pinned in `rust-toolchain.toml`) and Node.js 20+.

```sh
# Rust workspace
cargo build --workspace
cargo test --workspace

# Frontend
npm ci
npm run dev      # dev server on http://localhost:1420
npm run build    # typecheck + production build into dist/

# Desktop bundle (installers)
npm run tauri build
```

## Documentation

- [User guide](docs/user-guide.md)
- [Install guide](docs/install.md)
- [CLI reference](docs/cli.md)
- [Archon design](docs/archon-design.md)
- [Security review](docs/security-review.md)
- [HDR scope](docs/hdr-scope.md)
- [Versioning and changelog](docs/versioning.md)
- [Post-release triage](docs/triage.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the SPDX header convention and pre-PR checks.
