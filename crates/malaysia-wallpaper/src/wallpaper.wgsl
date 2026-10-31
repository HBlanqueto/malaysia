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
    // "fill" in wallpaper.domain = cover: scale so the image covers the surface,
    // cropping the excess while preserving aspect ratio. The sampler is
    // ClampToEdge, so the UV stays within [0, 1] by construction.
    let scale = max(surf.x / img.x, surf.y / img.y);
    let uv = (in.uv - 0.5) * (surf / (img * scale)) + 0.5;
    return textureSample(wallpaper, samp, uv);
}
