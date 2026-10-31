//! Vulkan/WGPU rendering for one wallpaper texture shared across outputs.

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use wgpu::util::DeviceExt;

use crate::image::DecodedImage;

/// GPU state shared by every output surface.
pub struct RenderContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: Option<wgpu::RenderPipeline>,
    mip_pipeline: wgpu::RenderPipeline,
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

const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let uv = vec2<f32>(
        f32(idx == 1u) * 2.0 + -1.0,
        f32(idx == 2u) * 2.0 + -1.0,
    );
    var out: VertexOutput;
    out.position = vec4<f32>(uv, 0.0, 1.0);
    out.uv = uv * 0.5 + vec2<f32>(0.5);
    return out;
}

@group(0) @binding(0)
var wallpaper: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;
@group(0) @binding(2)
var<uniform> uniforms: Uniforms;

struct Uniforms {
    surface_size: vec2<f32>,
    image_size: vec2<f32>,
    _pad: vec2<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let img = uniforms.image_size;
    let surf = uniforms.surface_size;
    let scale = max(surf.x / img.x, surf.y / img.y);
    let scaled = img * scale;
    let offset = (surf - scaled) / (2.0 * surf);
    let factor = scaled / surf;
    let uv = clamp(in.uv * factor + offset, vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSample(wallpaper, samp, uv);
}
"#;

const MIP_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let uv = vec2<f32>(
        f32(idx == 1u) * 2.0 + -1.0,
        f32(idx == 2u) * 2.0 + -1.0,
    );
    var out: VertexOutput;
    out.position = vec4<f32>(uv, 0.0, 1.0);
    out.uv = uv * 0.5 + vec2<f32>(0.5);
    return out;
}

@group(0) @binding(0)
var src: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.uv);
}
"#;

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
            .ok_or_else(|| anyhow::anyhow!("no Vulkan adapter found; is RADV available?"))?;

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
                    required_limits: wgpu::Limits::downlevel_defaults(),
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

        let mip_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            });

        let mip_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mipmap pipeline layout"),
            bind_group_layouts: &[&mip_bind_group_layout],
            push_constant_ranges: &[],
        });

        let mip_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mipmap shader"),
            source: wgpu::ShaderSource::Wgsl(MIP_SHADER.into()),
        });

        let mip_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mipmap pipeline"),
            layout: Some(&mip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &mip_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &mip_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
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
            mip_pipeline,
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
    pub fn upload_texture(&self, image: &DecodedImage) -> WallpaperTexture {
        let (width, height) = image.size;
        let mip_level_count = max_mip_levels(width, height);
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
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
            &self.mip_pipeline,
            &self.sampler,
            &texture,
            mip_level_count,
            width,
            height,
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
        self.ensure_pipeline(format);
        let pipeline = self.pipeline.as_ref().expect("pipeline was just ensured");
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wallpaper encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wallpaper pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

#[allow(clippy::too_many_arguments)]
fn generate_mipmaps(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
    mip_level_count: u32,
    width: u32,
    height: u32,
) {
    if mip_level_count <= 1 {
        return;
    }
    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mipmap encoder"),
    });

    let mut src_width = width;
    let mut src_height = height;

    for level in 1..mip_level_count {
        let dst_width = src_width.max(1) / 2;
        let dst_height = src_height.max(1) / 2;

        let src_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("mipmap src"),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
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
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
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

        src_width = dst_width;
        src_height = dst_height;
    }

    queue.submit(std::iter::once(encoder.finish()));
}
