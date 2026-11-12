use std::borrow::Cow;

use futures_lite::future::block_on;
use wgpu::util::DeviceExt;

use crate::image::{DecodedImage, Pixels, packed_rows};
use crate::render::{fullscreen_pipeline, offscreen_texture, padded_bytes_per_row, read_back};

// Shadertoy's single-pass contract: `mainImage(out vec4, in vec2)` with `iResolution` and `iTime`,
// writing sRGB values that the sRGB surface would encode twice.
const PRELUDE: &str = "#version 450
layout(set = 0, binding = 0) uniform Frame { vec3 iResolution; float iTime; };
layout(location = 0) out vec4 malaysia_color;
";
const MAIN: &str = "
void main() {
    vec4 color = vec4(0.0);
    mainImage(color, vec2(gl_FragCoord.x, iResolution.y - gl_FragCoord.y));
    vec3 c = clamp(color.rgb, 0.0, 1.0);
    malaysia_color = vec4(mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c)), 1.0);
}
";
const SNAPSHOT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub struct Shader {
    vertex: wgpu::ShaderModule,
    fragment: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bind_group: wgpu::BindGroup,
    frame: wgpu::Buffer,
    pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

impl Shader {
    pub fn new(device: &wgpu::Device, source: &str) -> anyhow::Result<Self> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wallpaper shader"),
            source: wgpu::ShaderSource::Glsl {
                shader: Cow::Owned(format!("{PRELUDE}{source}{MAIN}")),
                stage: wgpu::naga::ShaderStage::Fragment,
                defines: &[],
            },
        });
        if let Some(error) = block_on(scope.pop()) {
            anyhow::bail!("the shader does not compile: {error}");
        }
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wallpaper shader vertex"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fullscreen.wgsl").into()),
        });
        let entries = [wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wallpaper shader frame layout"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wallpaper shader layout"),
            bind_group_layouts: &[Some(&group_layout)],
            immediate_size: 0,
        });
        let frame = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wallpaper shader frame"),
            contents: bytemuck::cast_slice(&[0.0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wallpaper shader frame"),
            layout: &group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        let mut shader = Self {
            vertex,
            fragment,
            layout,
            bind_group,
            frame,
            pipeline: None,
        };
        shader.ensure_pipeline(device, SNAPSHOT_FORMAT)?;
        Ok(shader)
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<&wgpu::RenderPipeline> {
        if self
            .pipeline
            .as_ref()
            .is_none_or(|(built, _)| *built != format)
        {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let pipeline = fullscreen_pipeline(
                device,
                "wallpaper shader pipeline",
                &self.layout,
                [&self.vertex, &self.fragment],
                format,
            );
            if let Some(error) = block_on(scope.pop()) {
                anyhow::bail!("the shader cannot draw: {error}");
            }
            self.pipeline = Some((format, pipeline));
        }
        Ok(&self
            .pipeline
            .as_ref()
            .expect("the pipeline was just built")
            .1)
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        seconds: f32,
    ) -> anyhow::Result<()> {
        queue.write_buffer(
            &self.frame,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, 1.0, seconds]),
        );
        let bind_group = self.bind_group.clone();
        let pipeline = self.ensure_pipeline(device, format)?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wallpaper shader encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wallpaper shader pass"),
                multiview_mask: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
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
        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    pub fn snapshot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
    ) -> anyhow::Result<DecodedImage> {
        let target = offscreen_texture(device, size, SNAPSHOT_FORMAT);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(device, queue, &view, SNAPSHOT_FORMAT, size, 0.0)?;
        let padded = read_back(device, queue, &target);
        let row = size.0 as usize * 4;
        let stride = padded_bytes_per_row(size.0) as usize;
        let pixels = packed_rows(&padded, stride, row, size.1 as usize);
        if pixels.len() != row * size.1 as usize {
            anyhow::bail!("the shader snapshot could not be read");
        }
        Ok(DecodedImage {
            size,
            pixels: Pixels::Srgb8(pixels),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRADIENT: &str = "void mainImage(out vec4 color, in vec2 coord) {
        color = vec4(coord.x / iResolution.x, 0.0, 1.0, 1.0);
    }";

    fn gpu() -> crate::render::RenderContext {
        block_on(crate::render::RenderContext::new()).expect("RenderContext")
    }

    #[test]
    fn a_shadertoy_shader_draws_with_its_resolution() {
        let gpu = gpu();
        let mut shader = Shader::new(&gpu.device, GRADIENT).unwrap();
        let image = shader.snapshot(&gpu.device, &gpu.queue, (64, 8)).unwrap();
        let [left_r, _, left_b, _] = image.rgba8_at(0);
        let [right_r, _, _, _] = image.rgba8_at(63 * 4);
        assert!(left_r < 16 && left_b > 240 && right_r > 240);
    }

    #[test]
    fn a_shader_that_does_not_compile_is_an_error() {
        let gpu = gpu();
        assert!(Shader::new(&gpu.device, "void mainImage(").is_err());
    }
}
