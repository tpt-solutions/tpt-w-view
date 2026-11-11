# Installing tpt-view

## Windows 10 and 11

- **Installer:** the Tauri bundler produces an NSIS installer. It installs for the current user, so
  no administrator rights are needed. If the WebView2 runtime is missing, the installer downloads
  its bootstrapper. Windows 11 already includes WebView2.
- **Default viewer:** after installing, open *Settings → Apps → Default apps* and choose tpt-view for
  the image types you want. Windows does not let apps set their own default.
- **Build a runnable desktop binary:** a plain `cargo build --release` produces an executable that
  loads the UI from the Vite dev server (`localhost:1420`) and opens a blank window. Build with
  `npm run tauri build` for the installer, or with `cargo build --release -p tpt-view-app --features
  tauri/custom-protocol` for a standalone `target/release/tpt-view.exe` that embeds the UI.
- **Build from source:** install Rust (the version in `rust-toolchain.toml`) and Node.js 20 or
  later, then run `npm ci` and `npm run tauri build` from the repository root.
  - The AVIF decoder (dav1d) is compiled from source during the build, so you also need the
    Visual Studio C++ build tools, `meson`, `ninja` (both available from pip), NASM (version 2.14
    or later, on `PATH`) and `pkg-config` (or `pkgconf`) on `PATH`. `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always`
    makes the build use this path explicitly.

## Linux

- **Debian and Ubuntu:** install the `.deb` package. It depends on `libwebkit2gtk-4.1-0` and
  `libgtk-3-0`, and it installs the `.desktop` entry and shared-mime-info package so file managers
  offer tpt-view for supported formats.
- **Other distributions:** the AppImage needs no installation. Copy the files from
  `packaging/linux/` into `~/.local/share/applications/` and `~/.local/share/mime/packages/`, then
  run `update-mime-database ~/.local/share/mime`.
- **GPU:** Vulkan is used when available. Set `TPT_VIEW_BACKEND=gl` to try OpenGL if Vulkan is not
  working.

## CLI only

The command-line tool is `tpt-view-cli`. It is built with the workspace:

```sh
cargo build --release -p tpt-view-cli
```

See `docs/cli.md` for options and exit codes.
