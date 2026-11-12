@group(0) @binding(0)
var wallpaper: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;
@group(0) @binding(2)
var<uniform> uniforms: Uniforms;

struct Uniforms {
    origin: vec2<f32>,
    size: vec2<f32>,
    background: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = uniforms.origin + in.uv * uniforms.size;
    let color = textureSample(wallpaper, samp, uv);
    let inside = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    let blended = vec4<f32>(mix(uniforms.background.rgb, color.rgb, color.a), 1.0);
    return select(uniforms.background, blended, inside);
}
