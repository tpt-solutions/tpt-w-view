# HDR and wide-gamut display: scope decision

Decision: **v1 displays SDR sRGB only.** The full HDR pipeline is deferred. This document records
what v1 does and what a later HDR pass would need.

## What v1 does

- Decoding produces 8-bit RGBA. RAW files are developed to 16-bit internally by rawler and then
  converted to 8-bit sRGB (`core/src/raw.rs`).
- The GPU renderer uses `Rgba8Unorm` textures and targets (`render/src/lib.rs`), so colour values
  pass through unchanged.
- ICC profiles are not applied. Images are treated as sRGB. Color management is a backlog item.

## Why defer

- HDR output needs a float or 10-bit swapchain, an OS colour-space query, and tone mapping for
  SDR content shown in an HDR window. Each step is platform-specific (DXGI on Windows, Wayland and
  X11 differ on Linux).
- The Tauri webview draws the chrome in SDR, so an HDR viewer would need a separate native surface
  composited with the webview. That is an architecture decision, not a rendering tweak.
- Most photos in the v1 target set are 8-bit sRGB or 8-bit JPEG, so the benefit is limited for v1.

## What a later HDR pass would need

1. A float16 render target and a surface format query (`wgpu::TextureFormat::Rgba16Float`).
2. Reading the display's colour volume on each platform.
3. Tone mapping for SDR inputs shown on HDR displays, and the reverse for HDR inputs on SDR.
4. ICC profile handling first (see the backlog item), since HDR depends on it.
