// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! GPU rendering for tpt-view (Phase 2).
//!
//! Images are uploaded as fixed-size tiles so arbitrarily large pictures fit within texture
//! limits. A frame draws every visible tile as a textured quad, positioned by a zoom and pan
//! transform, using either nearest or linear sampling. Frames are rendered offscreen and read
//! back to the CPU. That keeps the pipeline testable without a window; a swapchain target can
//! reuse the same pipeline later.
//!
//! Backend choice: the default is the platform's primary backend (Vulkan or DX12 on Windows,
//! Vulkan on Linux, Metal on macOS). Set `TPT_VIEW_BACKEND` to `vulkan`, `dx12`, `metal` or `gl`
//! to force one. If no hardware adapter exists, a software fallback adapter is tried before
//! failing with [`RenderError::NoAdapter`], so callers can fall back to the CPU path.

use image::RgbaImage;
use tpt_view_core::tiling::{Rect, TileGrid};
use wgpu::util::DeviceExt;

/// Default tile edge in pixels. Chosen to stay well within common texture-size limits.
pub const DEFAULT_TILE_SIZE: u32 = 2048;

/// Texture format for all images and render targets.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug)]
pub enum RenderError {
    /// No GPU or software adapter could be created.
    NoAdapter,
    /// Device creation failed.
    Device(String),
    /// Something went wrong while drawing or reading back.
    Frame(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NoAdapter => write!(f, "no GPU or software adapter available"),
            RenderError::Device(m) => write!(f, "device creation failed: {m}"),
            RenderError::Frame(m) => write!(f, "frame failed: {m}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Texture sampling used when a tile is drawn at a scale other than 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sampling {
    Nearest,
    Linear,
}

/// Camera transform and output size. Image pixel `(0, 0)` lands at screen `(offset_x, offset_y)`
/// and each image pixel covers `zoom` screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub out_width: u32,
    pub out_height: u32,
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

struct Tile {
    rect: Rect,
    bind_source: wgpu::TextureView,
}

/// An image uploaded as a grid of GPU textures.
pub struct TiledImage {
    width: u32,
    height: u32,
    grid: TileGrid,
    tiles: Vec<Tile>,
}

impl TiledImage {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The tile layout this image was uploaded with.
    pub fn grid(&self) -> &TileGrid {
        &self.grid
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

/// Owns the device, the pipeline and shared samplers.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    adapter_name: String,
}

const SHADER: &str = r#"
struct Quad {
    rect: vec4<f32>,   // screen-space pixels: x0, y0, x1, y1
    uv: vec4<f32>,     // texture coordinates: u0, v0, u1, v1
    screen: vec2<f32>, // render target size in pixels
    pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> quad: Quad;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[index];
    let px = mix(quad.rect.xy, quad.rect.zw, c);
    let ndc = vec2<f32>(px.x / quad.screen.x * 2.0 - 1.0, 1.0 - px.y / quad.screen.y * 2.0);
    var out: VsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(quad.uv.xy, quad.uv.zw, c);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(source, samp, in.uv);
}
"#;

/// Chooses the backend set from `TPT_VIEW_BACKEND`, defaulting to the platform primary set.
fn backends_from_env() -> wgpu::Backends {
    match std::env::var("TPT_VIEW_BACKEND").ok().as_deref() {
        Some("vulkan") => wgpu::Backends::VULKAN,
        Some("dx12") => wgpu::Backends::DX12,
        Some("metal") => wgpu::Backends::METAL,
        Some("gl") => wgpu::Backends::GL,
        _ => wgpu::Backends::PRIMARY,
    }
}

impl Renderer {
    /// Creates a renderer on the best available adapter.
    pub fn new() -> Result<Self, RenderError> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends_from_env();
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .or_else(|_| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: true,
                compatible_surface: None,
                apply_limit_buckets: false,
            }))
        })
        .map_err(|_| RenderError::NoAdapter)?;

        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("tpt-view"),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tpt-view-bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpt-view-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tpt-view-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tpt-view-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            nearest,
            linear,
            adapter_name,
        })
    }

    /// Name of the adapter in use, e.g. for diagnostics or the status bar.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Uploads `image` as tiles of [`DEFAULT_TILE_SIZE`].
    pub fn upload(&self, image: &RgbaImage) -> TiledImage {
        self.upload_with_tile_size(image, DEFAULT_TILE_SIZE)
    }

    /// Uploads `image` as tiles of `tile_size` pixels. Edge tiles are clipped to the image.
    pub fn upload_with_tile_size(&self, image: &RgbaImage, tile_size: u32) -> TiledImage {
        let (width, height) = image.dimensions();
        let grid = TileGrid::new(width, height, tile_size.max(1))
            .expect("image dimensions and tile size are non-zero");
        let mut tiles = Vec::with_capacity(grid.len());
        for row in 0..grid.rows() {
            for col in 0..grid.columns() {
                let rect = grid.tile_rect(col, row).expect("in range");
                let tile_img =
                    image::imageops::crop_imm(image, rect.x, rect.y, rect.width, rect.height)
                        .to_image();
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("tile"),
                    size: wgpu::Extent3d {
                        width: rect.width,
                        height: rect.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    tile_img.as_raw(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(rect.width * 4),
                        rows_per_image: Some(rect.height),
                    },
                    wgpu::Extent3d {
                        width: rect.width,
                        height: rect.height,
                        depth_or_array_layers: 1,
                    },
                );
                tiles.push(Tile {
                    rect,
                    bind_source: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                });
            }
        }
        TiledImage {
            width,
            height,
            grid,
            tiles,
        }
    }

    /// Draws `source` through `view` and reads the result back as RGBA8. Pixels outside the
    /// image are transparent.
    pub fn render(
        &self,
        source: &TiledImage,
        view: &View,
        sampling: Sampling,
    ) -> Result<RgbaImage, RenderError> {
        if view.out_width == 0
            || view.out_height == 0
            || !(view.zoom.is_finite() && view.zoom > 0.0)
        {
            return Err(RenderError::Frame(
                "view size and zoom must be positive".into(),
            ));
        }
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width: view.out_width,
                height: view.out_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = match sampling {
            Sampling::Nearest => &self.nearest,
            Sampling::Linear => &self.linear,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            // Build one bind group per visible tile. Uniforms are small, so this is cheap.
            let mut draws = Vec::new();
            for tile in &source.tiles {
                let x0 = tile.rect.x as f32 * view.zoom + view.offset_x;
                let y0 = tile.rect.y as f32 * view.zoom + view.offset_y;
                let x1 = x0 + tile.rect.width as f32 * view.zoom;
                let y1 = y0 + tile.rect.height as f32 * view.zoom;
                if x1 <= 0.0
                    || y1 <= 0.0
                    || x0 >= view.out_width as f32
                    || y0 >= view.out_height as f32
                {
                    continue;
                }
                let uniforms: [f32; 12] = [
                    x0,
                    y0,
                    x1,
                    y1, // rect
                    0.0,
                    0.0,
                    1.0,
                    1.0, // uv
                    view.out_width as f32,
                    view.out_height as f32,
                    0.0,
                    0.0,
                ];
                let mut bytes = Vec::with_capacity(48);
                for v in uniforms {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("quad"),
                        contents: &bytes,
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tile-bindings"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&tile.bind_source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                });
                draws.push((buffer, bind_group));
            }

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw-tiles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for (_, bind_group) in &draws {
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
        }

        // Read back with the row stride padded to the copy alignment.
        let unpadded = view.out_width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(view.out_height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(view.out_height),
                },
            },
            wgpu::Extent3d {
                width: view.out_width,
                height: view.out_height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Frame(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Frame(e.to_string()))?
            .map_err(|e| RenderError::Frame(e.to_string()))?;

        let data = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Frame(e.to_string()))?;
        let mut pixels = Vec::with_capacity((unpadded * view.out_height) as usize);
        for row in 0..view.out_height as usize {
            let start = row * padded as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        readback.unmap();

        RgbaImage::from_raw(view.out_width, view.out_height, pixels)
            .ok_or_else(|| RenderError::Frame("readback size mismatch".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn checker(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        })
    }

    /// Returns `None` (and logs why) when the machine has no usable adapter, so CI without a
    /// GPU reports a skip rather than a failure.
    fn renderer_or_skip() -> Option<Renderer> {
        match Renderer::new() {
            Ok(r) => {
                eprintln!("render tests using adapter: {}", r.adapter_name());
                Some(r)
            }
            Err(e) => {
                eprintln!("skipping GPU render test: {e}");
                None
            }
        }
    }

    #[test]
    fn identity_view_reproduces_source_exactly() {
        let Some(r) = renderer_or_skip() else { return };
        let src = checker(4, 4);
        let tiled = r.upload(&src);
        let out = r
            .render(
                &tiled,
                &View {
                    out_width: 4,
                    out_height: 4,
                    zoom: 1.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                },
                Sampling::Nearest,
            )
            .expect("render");
        assert_eq!(out, src);
    }

    #[test]
    fn tiles_join_without_seams() {
        let Some(r) = renderer_or_skip() else { return };
        let src = checker(5, 3);
        let tiled = r.upload_with_tile_size(&src, 2);
        assert_eq!(tiled.tile_count(), 6, "3 columns x 2 rows");
        let out = r
            .render(
                &tiled,
                &View {
                    out_width: 5,
                    out_height: 3,
                    zoom: 1.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                },
                Sampling::Nearest,
            )
            .expect("render");
        assert_eq!(out, src);
    }

    #[test]
    fn zoom_two_replicates_each_pixel_in_a_block() {
        let Some(r) = renderer_or_skip() else { return };
        let src = checker(3, 2);
        let tiled = r.upload(&src);
        let out = r
            .render(
                &tiled,
                &View {
                    out_width: 6,
                    out_height: 4,
                    zoom: 2.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                },
                Sampling::Nearest,
            )
            .expect("render");
        for y in 0..4 {
            for x in 0..6 {
                assert_eq!(
                    out.get_pixel(x, y),
                    src.get_pixel(x / 2, y / 2),
                    "at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn pan_moves_image_and_leaves_transparent_border() {
        let Some(r) = renderer_or_skip() else { return };
        let src = RgbaImage::from_pixel(2, 2, Rgba([9, 9, 9, 255]));
        let tiled = r.upload(&src);
        let out = r
            .render(
                &tiled,
                &View {
                    out_width: 4,
                    out_height: 4,
                    zoom: 1.0,
                    offset_x: 2.0,
                    offset_y: 0.0,
                },
                Sampling::Nearest,
            )
            .expect("render");
        assert_eq!(out.get_pixel(0, 0).0[3], 0, "left of image is transparent");
        assert_eq!(out.get_pixel(3, 1), &Rgba([9, 9, 9, 255]));
    }

    #[test]
    fn invalid_view_is_rejected_before_drawing() {
        let Some(r) = renderer_or_skip() else { return };
        let tiled = r.upload(&checker(2, 2));
        let bad = View {
            out_width: 0,
            out_height: 2,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        assert!(r.render(&tiled, &bad, Sampling::Nearest).is_err());
    }
}
