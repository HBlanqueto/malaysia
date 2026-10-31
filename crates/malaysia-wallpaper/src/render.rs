//! Vulkan/WGPU rendering for one wallpaper texture shared across outputs.

use std::collections::HashMap;
use std::sync::LazyLock;

use half::f16;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use wgpu::util::DeviceExt;

use crate::image::{DecodedImage, Pixels};

/// GPU state shared by every output surface.
pub struct RenderContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: Option<wgpu::RenderPipeline>,
    mip_pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    surface_format: Option<wgpu::TextureFormat>,
}

/// A texture created from the current wallpaper.
pub struct WallpaperTexture {
    pub bind_group: wgpu::BindGroup,
    pub size: (u32, u32),
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    surface_size: [f32; 2],
    image_size: [f32; 2],
    _pad: [f32; 2],
}

const SHADER: &str = concat!(
    include_str!("fullscreen.wgsl"),
    include_str!("wallpaper.wgsl")
);
const MIP_SHADER: &str = concat!(include_str!("fullscreen.wgsl"), include_str!("mip.wgsl"));

impl RenderContext {
    /// Creates a Vulkan-only wgpu context and logs the chosen adapter.
    ///
    /// Returns an error with a clear message when no Vulkan adapter is
    /// available; the caller exits with a non-zero status.
    pub async fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: None,
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                let vk_files = std::env::var("VK_DRIVER_FILES")
                    .unwrap_or_else(|_| "(sin definir)".to_string());
                let mut msg = format!("no Vulkan adapter found; VK_DRIVER_FILES={vk_files}");
                if vk_files != "(sin definir)" {
                    for path in vk_files.split(':').filter(|p| !p.is_empty()) {
                        let exists = std::path::Path::new(path).exists();
                        msg.push_str(&format!(
                            "\n  {path}: {}",
                            if exists { "existe" } else { "no existe" }
                        ));
                    }
                }
                anyhow::anyhow!(msg)
            })?;

        {
            let info = adapter.get_info();
            tracing::info!(
                adapter = %info.name,
                backend = ?info.backend,
                driver = %info.driver,
                "wgpu adapter"
            );
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("malaysia-wallpaper"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 4096,
                        ..wgpu::Limits::downlevel_defaults()
                    },
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await?;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wallpaper bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wallpaper sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wallpaper uniforms"),
            contents: bytemuck::cast_slice(&[Uniforms {
                surface_size: [1.0, 1.0],
                image_size: [1.0, 1.0],
                _pad: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipeline: None,
            mip_pipelines: HashMap::new(),
            sampler,
            bind_group_layout,
            uniform_buffer,
            surface_format: None,
        })
    }

    /// Creates a wgpu surface for a Wayland surface/display pair.
    ///
    /// # Safety
    ///
    /// The raw handles must remain valid for the lifetime of the returned
    /// surface.
    pub unsafe fn create_surface(
        &self,
        display: RawDisplayHandle,
        window: RawWindowHandle,
    ) -> anyhow::Result<wgpu::Surface<'static>> {
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: display,
            raw_window_handle: window,
        };
        Ok(unsafe { self.instance.create_surface_unsafe(target) }?)
    }

    /// Ensures the render pipeline exists for `format`.
    ///
    /// The pipeline target format must match the surface format, so it is
    /// created lazily once the first surface reports its capabilities.
    pub fn ensure_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.surface_format == Some(format) && self.pipeline.is_some() {
            return;
        }
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wallpaper pipeline layout"),
                bind_group_layouts: &[&self.bind_group_layout],
                push_constant_ranges: &[],
            });
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("wallpaper shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
        self.pipeline = Some(
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("wallpaper pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                }),
        );
        self.surface_format = Some(format);
    }

    /// Uploads a decoded image as a GPU texture with a full mip chain.
    pub fn upload_texture(&mut self, image: &DecodedImage) -> WallpaperTexture {
        let (width, height) = image.size;
        let format = match &image.pixels {
            Pixels::Srgb8(_) => wgpu::TextureFormat::Rgba8UnormSrgb,
            Pixels::Srgb16(_) => wgpu::TextureFormat::Rgba16Float,
        };
        let features = self.adapter.features();
        let mip_level_count = mip_level_count(format, features, width, height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wallpaper texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let (bytes_per_row, data) = match &image.pixels {
            Pixels::Srgb8(pixels) => (width * 4, bytemuck::cast_slice(pixels).to_vec()),
            Pixels::Srgb16(pixels) => (width * 8, srgb16_to_linear_f16(pixels)),
        };

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        generate_mipmaps(
            &self.device,
            &self.queue,
            &mut self.mip_pipelines,
            &self.sampler,
            &texture,
            format,
            mip_level_count,
        );

        let gpu_bytes = gpu_texture_bytes(width, height, format, mip_level_count);
        tracing::info!(
            width,
            height,
            ?format,
            mip_level_count,
            gpu_bytes,
            "wallpaper texture uploaded"
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wallpaper bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        WallpaperTexture {
            bind_group,
            size: (width, height),
        }
    }

    /// Updates the uniform buffer with the surface and image sizes for the
    /// cover/fill UV calculation.
    pub fn set_uniforms(&self, surface_width: u32, surface_height: u32, image: (u32, u32)) {
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[Uniforms {
                surface_size: [surface_width as f32, surface_height as f32],
                image_size: [image.0 as f32, image.1 as f32],
                _pad: [0.0; 2],
            }]),
        );
    }

    /// Encodes the wallpaper draw pass into `view` and submits it.
    pub fn draw(
        &mut self,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        texture: &WallpaperTexture,
    ) {
        self.ensure_pipeline(format);
        let pipeline = self.pipeline.as_ref().expect("pipeline was just ensured");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wallpaper encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wallpaper pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &texture.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Renders one frame to `surface` using the shared texture.
    ///
    /// Surface-outdated errors are logged and ignored; surface-lost errors
    /// are propagated so the caller can exit.
    pub fn render(
        &mut self,
        surface: &wgpu::Surface<'_>,
        format: wgpu::TextureFormat,
        texture: &WallpaperTexture,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Outdated) => {
                tracing::warn!("surface outdated, will reconfigure");
                return Err(wgpu::SurfaceError::Outdated);
            }
            Err(wgpu::SurfaceError::Lost) => {
                tracing::error!("surface lost");
                return Err(wgpu::SurfaceError::Lost);
            }
            Err(error) => return Err(error),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(&view, format, texture);
        output.present();
        Ok(())
    }
}

fn max_mip_levels(width: u32, height: u32) -> u32 {
    let max = width.max(height);
    if max == 0 {
        return 1;
    }
    (u32::BITS - max.leading_zeros()).max(1)
}

/// Returns the number of mip levels for `format`, or 1 when the format cannot
/// be filtered or used as a render target.
pub fn mip_level_count(
    format: wgpu::TextureFormat,
    features: wgpu::Features,
    width: u32,
    height: u32,
) -> u32 {
    let guaranteed = format.guaranteed_format_features(features);
    if !guaranteed
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        || !guaranteed
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        tracing::warn!(
            ?format,
            "format is not filterable or renderable; skipping mipmaps"
        );
        return 1;
    }
    max_mip_levels(width, height)
}

/// Approximate GPU bytes for a texture plus its mip chain.
fn gpu_texture_bytes(
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    mip_level_count: u32,
) -> u64 {
    let bpp = match format {
        wgpu::TextureFormat::Rgba8UnormSrgb => 4,
        wgpu::TextureFormat::Rgba16Float => 8,
        _ => 4,
    };
    let mut total = 0u64;
    for level in 0..mip_level_count {
        let w = (width >> level).max(1) as u64;
        let h = (height >> level).max(1) as u64;
        total += w * h * bpp;
    }
    total
}

/// Converts sRGB-coded u16 samples to linear f16, leaving alpha linear.
fn srgb16_to_linear_f16(pixels: &[u16]) -> Vec<u8> {
    static TABLE: LazyLock<[f16; 65536]> = LazyLock::new(|| {
        let mut table = [f16::ZERO; 65536];
        for (i, entry) in table.iter_mut().enumerate() {
            let v = i as f32 / 65535.0;
            let linear = if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
            *entry = f16::from_f32(linear);
        }
        table
    });

    let mut out = Vec::with_capacity(pixels.len() * 2);
    for (i, &v) in pixels.iter().enumerate() {
        let f = if (i + 1) % 4 == 0 {
            // Alpha: linear.
            f16::from_f32(v as f32 / 65535.0)
        } else {
            TABLE[v as usize]
        };
        out.extend_from_slice(&f.to_ne_bytes());
    }
    out
}

fn generate_mipmaps(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipelines: &mut HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    mip_level_count: u32,
) {
    if mip_level_count <= 1 {
        return;
    }
    let pipeline = pipelines.entry(format).or_insert_with(|| {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mipmap pipeline layout"),
            bind_group_layouts: &[&device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("mipmap bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                },
            )],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mipmap shader"),
            source: wgpu::ShaderSource::Wgsl(MIP_SHADER.into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mipmap pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    });

    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mipmap encoder"),
    });

    for level in 1..mip_level_count {
        let src_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("mipmap src"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: level - 1,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
            usage: None,
        });
        let dst_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("mipmap dst"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: level,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
            usage: None,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mipmap bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mipmap pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    queue.submit(std::iter::once(encoder.finish()));
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;

    use crate::image::{
        DEFAULT_WALLPAPER_BYTES, DecodedImage, Pixels, decode_image_bytes, decode_png,
    };

    use super::{RenderContext, WallpaperTexture, mip_level_count};

    fn padded_bytes_per_row(width: u32) -> u32 {
        let unpadded = width * 4;
        unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
    }

    fn draw_offscreen(
        ctx: &mut RenderContext,
        texture: &WallpaperTexture,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Vec<u8> {
        let output = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wallpaper test offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&wgpu::TextureViewDescriptor::default());
        ctx.set_uniforms(width, height, texture.size);
        ctx.draw(&view, format, texture);

        let bytes_per_row = padded_bytes_per_row(width);
        let buffer_size = u64::from(bytes_per_row * height);
        let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wallpaper test readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wallpaper test copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        ctx.device.poll(wgpu::Maintain::Wait);
        slice.get_mapped_range().to_vec()
    }

    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            c.powf(1.0 / 2.4) * 1.055 - 0.055
        }
    }

    fn reference_color(image: &DecodedImage, x: u32, y: u32, surf_w: u32, surf_h: u32) -> [u8; 4] {
        let surf = (surf_w as f32, surf_h as f32);
        let img = (image.size.0 as f32, image.size.1 as f32);
        let s = (surf.0 / img.0).max(surf.1 / img.1);
        let u = 0.5 + (((x as f32 + 0.5) / surf.0) - 0.5) * (surf.0 / (img.0 * s));
        let v = 0.5 + (((y as f32 + 0.5) / surf.1) - 0.5) * (surf.1 / (img.1 * s));

        let sample_x = u * img.0 - 0.5;
        let sample_y = v * img.1 - 0.5;
        let x0 = sample_x.floor() as i32;
        let y0 = sample_y.floor() as i32;
        let xf = sample_x - x0 as f32;
        let yf = sample_y - y0 as f32;

        let mut linear = [0.0f32; 4];
        for dy in 0..2 {
            for dx in 0..2 {
                let sx = (x0 + dx).clamp(0, image.size.0 as i32 - 1) as u32;
                let sy = (y0 + dy).clamp(0, image.size.1 as i32 - 1) as u32;
                let weight =
                    (if dx == 0 { 1.0 - xf } else { xf }) * (if dy == 0 { 1.0 - yf } else { yf });
                let offset = ((sy * image.size.0 + sx) * 4) as usize;
                let [r, g, b, a] = image.rgba8_at(offset);
                linear[0] += srgb_to_linear(r as f32 / 255.0) * weight;
                linear[1] += srgb_to_linear(g as f32 / 255.0) * weight;
                linear[2] += srgb_to_linear(b as f32 / 255.0) * weight;
                linear[3] += a as f32 * weight;
            }
        }
        [
            (linear_to_srgb(linear[0]) * 255.0).round() as u8,
            (linear_to_srgb(linear[1]) * 255.0).round() as u8,
            (linear_to_srgb(linear[2]) * 255.0).round() as u8,
            (linear[3]).round() as u8,
        ]
    }

    fn decode_default() -> DecodedImage {
        decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap()
    }

    #[test]
    fn cover_matches_reference_at_1280x800() {
        let mut ctx = block_on(RenderContext::new()).expect("RenderContext");
        let image = decode_default();
        let texture = ctx.upload_texture(&image);

        let points: [(u32, u32); 9] = [
            (0, 0),
            (1279, 0),
            (0, 799),
            (1279, 799),
            (640, 0),
            (640, 799),
            (0, 400),
            (1279, 400),
            (640, 400),
        ];

        // Guard: the prompt asks for the 9 edge/corner/center points; if the
        // image is too uniform at those spots the test cannot detect flips.
        let top_center = reference_color(&image, 640, 0, 1280, 800);
        let bottom_center = reference_color(&image, 640, 799, 1280, 800);
        let top_left = reference_color(&image, 0, 0, 1280, 800);
        let top_right = reference_color(&image, 1279, 0, 1280, 800);
        assert!(
            top_center
                .iter()
                .zip(bottom_center.iter())
                .any(|(a, b)| a.abs_diff(*b) > 30),
            "guard failed: top-center and bottom-center are too similar; cannot detect a vertical flip"
        );
        assert!(
            top_left
                .iter()
                .zip(top_right.iter())
                .any(|(a, b)| a.abs_diff(*b) > 30),
            "guard failed: top-left and top-right are too similar; cannot detect a horizontal mirror"
        );

        for format in [
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let data = draw_offscreen(&mut ctx, &texture, 1280, 800, format);
            let bytes_per_row = padded_bytes_per_row(1280);
            for &(x, y) in &points {
                let offset = (y * bytes_per_row + x * 4) as usize;
                let got = if format == wgpu::TextureFormat::Bgra8UnormSrgb {
                    [
                        data[offset + 2],
                        data[offset + 1],
                        data[offset],
                        data[offset + 3],
                    ]
                } else {
                    [
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]
                };
                let expected = reference_color(&image, x, y, 1280, 800);
                assert!(
                    got.iter()
                        .zip(expected.iter())
                        .all(|(a, b)| a.abs_diff(*b) <= 8),
                    "mismatch at ({x},{y}) for {format:?}: got {got:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn minified_quadrants_keep_orientation() {
        let mut ctx = block_on(RenderContext::new()).expect("RenderContext");
        let image = decode_default();
        let texture = ctx.upload_texture(&image);
        let (w, h) = (270, 152);
        let data = draw_offscreen(
            &mut ctx,
            &texture,
            w,
            h,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let bytes_per_row = padded_bytes_per_row(w);

        let mut rendered = [[0.0f32; 3]; 4];
        let mut counts = [0u32; 4];
        for y in 0..h {
            for x in 0..w {
                let offset = (y * bytes_per_row + x * 4) as usize;
                let q = (if x < w / 2 { 0 } else { 1 }) + (if y < h / 2 { 0 } else { 2 });
                for (c, channel) in rendered[q].iter_mut().enumerate().take(3) {
                    *channel += data[offset + c] as f32;
                }
                counts[q] += 1;
            }
        }
        for (q, row) in rendered.iter_mut().enumerate() {
            for channel in row.iter_mut().take(3) {
                *channel /= counts[q] as f32;
            }
        }

        let (img_w, img_h) = (image.size.0, image.size.1);
        let mut source = [[0.0f32; 3]; 4];
        let mut source_counts = [0u32; 4];
        for y in 0..img_h {
            for x in 0..img_w {
                let offset = ((y * img_w + x) * 4) as usize;
                let q = (if x < img_w / 2 { 0 } else { 1 }) + (if y < img_h / 2 { 0 } else { 2 });
                let [r, g, b, _] = image.rgba8_at(offset);
                source[q][0] += r as f32;
                source[q][1] += g as f32;
                source[q][2] += b as f32;
                source_counts[q] += 1;
            }
        }
        for (q, row) in source.iter_mut().enumerate() {
            for channel in row.iter_mut().take(3) {
                *channel /= source_counts[q] as f32;
            }
        }

        for (q, (row, src)) in rendered.iter().zip(source.iter()).enumerate() {
            for (c, (&r, &s)) in row.iter().take(3).zip(src.iter().take(3)).enumerate() {
                let diff = (r - s).abs();
                assert!(
                    diff <= 10.0,
                    "quadrant {q} channel {c}: rendered mean {r} vs source mean {s}, diff {diff}"
                );
            }
        }
    }

    fn make_ramp_image(depth16: bool) -> DecodedImage {
        let width = 1024u32;
        let height = 512u32;
        if depth16 {
            let mut pixels = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                for x in 0..width {
                    let r = ((x as f32 / (width - 1) as f32) * 65535.0).round() as u16;
                    let g = ((y as f32 / (height - 1) as f32) * 65535.0).round() as u16;
                    let b = (0.3f32 * 65535.0).round() as u16;
                    pixels.extend_from_slice(&[r, g, b, u16::MAX]);
                }
            }
            DecodedImage {
                size: (width, height),
                pixels: Pixels::Srgb16(pixels),
            }
        } else {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                for x in 0..width {
                    let r = ((x as f32 / (width - 1) as f32) * 255.0).round() as u8;
                    let g = ((y as f32 / (height - 1) as f32) * 255.0).round() as u8;
                    let b = (0.3f32 * 255.0).round() as u8;
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
            DecodedImage {
                size: (width, height),
                pixels: Pixels::Srgb8(rgba),
            }
        }
    }

    #[test]
    fn sixteen_bit_texture_matches_reference() {
        let mut ctx = block_on(RenderContext::new()).expect("RenderContext");
        let image = make_ramp_image(true);
        let texture = ctx.upload_texture(&image);
        assert_eq!(texture.size, (1024, 512));

        let points: [(u32, u32); 9] = [
            (0, 0),
            (1023, 0),
            (0, 511),
            (1023, 511),
            (512, 0),
            (512, 511),
            (0, 256),
            (1023, 256),
            (512, 256),
        ];

        let data = draw_offscreen(
            &mut ctx,
            &texture,
            1024,
            512,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let bytes_per_row = padded_bytes_per_row(1024);
        for &(x, y) in &points {
            let offset = (y * bytes_per_row + x * 4) as usize;
            let got = [
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ];
            let [r, g, b, a] = image.rgba8_at(((y * 1024 + x) * 4) as usize);
            let expected = [r, g, b, a];
            assert!(
                got.iter()
                    .zip(expected.iter())
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "mismatch at ({x},{y}): got {got:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn mips_follow_the_texture_format() {
        let mut rendered = [[[0.0f32; 3]; 4]; 2];
        for (i, depth16) in [false, true].into_iter().enumerate() {
            let mut ctx = block_on(RenderContext::new()).expect("RenderContext");
            let image = make_ramp_image(depth16);
            let texture = ctx.upload_texture(&image);
            assert_eq!(texture.size, (1024, 512));
            let format = if depth16 {
                wgpu::TextureFormat::Rgba16Float
            } else {
                wgpu::TextureFormat::Rgba8UnormSrgb
            };
            assert_eq!(
                ctx.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("probe"),
                        size: wgpu::Extent3d {
                            width: 1024,
                            height: 512,
                            depth_or_array_layers: 1
                        },
                        mip_level_count: 11,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .mip_level_count(),
                11,
                "probe texture should accept 11 levels"
            );

            let (w, h) = (256u32, 128u32);
            let data = draw_offscreen(
                &mut ctx,
                &texture,
                w,
                h,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            );
            let bytes_per_row = padded_bytes_per_row(w);

            let mut counts = [0u32; 4];
            for y in 0..h {
                for x in 0..w {
                    let offset = (y * bytes_per_row + x * 4) as usize;
                    let q = (if x < w / 2 { 0 } else { 1 }) + (if y < h / 2 { 0 } else { 2 });
                    rendered[i][q][0] += data[offset] as f32;
                    rendered[i][q][1] += data[offset + 1] as f32;
                    rendered[i][q][2] += data[offset + 2] as f32;
                    counts[q] += 1;
                }
            }
            for (q, row) in rendered[i].iter_mut().enumerate() {
                for channel in row.iter_mut() {
                    *channel /= counts[q] as f32;
                }
            }
        }

        for (q, (eight, sixteen)) in rendered[0].iter().zip(rendered[1].iter()).enumerate() {
            for (c, (&a, &b)) in eight.iter().zip(sixteen.iter()).enumerate() {
                let diff = (a - b).abs();
                assert!(
                    diff <= 6.0,
                    "quadrant {q} channel {c}: 8-bit mean {a} vs 16-bit mean {b}, diff {diff}"
                );
            }
        }
    }

    #[test]
    fn non_filterable_formats_get_one_level() {
        assert_eq!(
            mip_level_count(
                wgpu::TextureFormat::Rgba32Float,
                wgpu::Features::empty(),
                1024,
                512
            ),
            1
        );
        assert_eq!(
            mip_level_count(
                wgpu::TextureFormat::Rgba32Float,
                wgpu::Features::FLOAT32_FILTERABLE,
                1024,
                512,
            ),
            11
        );
        assert_eq!(
            mip_level_count(
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::Features::empty(),
                1024,
                512
            ),
            11
        );
    }

    #[test]
    #[ignore]
    fn capture_matches_reference() {
        let path = std::env::var_os("MALAYSIA_WALLPAPER_CAPTURE").expect(
            "set MALAYSIA_WALLPAPER_CAPTURE to the PNG path and run with ./dev.sh wallpaper-check",
        );
        let path = std::path::PathBuf::from(path);
        let bytes = std::fs::read(&path).expect("read capture PNG");
        let capture = decode_png(&bytes).expect("decode capture PNG");
        let (w, h) = capture.size;
        let source = decode_default();

        // The compositor's output capture can have host-window rounding artifacts
        // in the very corners, so sample well inside them.
        let inset = (w.min(h) / 8).max(2);
        let points: [(u32, u32); 9] = [
            (inset, inset),
            (w - 1 - inset, inset),
            (inset, h - 1 - inset),
            (w - 1 - inset, h - 1 - inset),
            (w / 2, inset),
            (w / 2, h - 1 - inset),
            (inset, h / 2),
            (w - 1 - inset, h / 2),
            (w / 2, h / 2),
        ];

        for (x, y) in points {
            let offset = ((y * w + x) * 4) as usize;
            let got = capture.rgba8_at(offset);
            let expected = reference_color(&source, x, y, w, h);
            assert!(
                got.iter()
                    .zip(expected.iter())
                    .all(|(a, b)| a.abs_diff(*b) <= 3),
                "mismatch at ({x},{y}) in {w}x{h} capture: got {got:?}, expected {expected:?}"
            );
        }
    }
}
