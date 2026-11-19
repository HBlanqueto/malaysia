@group(0) @binding(0)
var luma: texture_2d<f32>;
@group(0) @binding(1)
var chroma: texture_2d<f32>;
@group(0) @binding(2)
var samp: sampler;
@group(0) @binding(3)
var<uniform> yuv: Yuv;

struct Yuv {
    coefficients: vec4<f32>,
    range: vec4<f32>,
};

const CHROMA_NEUTRAL: f32 = 128.0 / 255.0;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let kr = yuv.coefficients.x;
    let kb = yuv.coefficients.y;
    let kg = 1.0 - kr - kb;
    let y = (textureSample(luma, samp, in.uv).r - yuv.range.x) * yuv.range.y;
    let cbcr = (textureSample(chroma, samp, in.uv).rg - CHROMA_NEUTRAL) * yuv.range.z;
    let r = y + 2.0 * (1.0 - kr) * cbcr.y;
    let b = y + 2.0 * (1.0 - kb) * cbcr.x;
    let g = (y - kr * r - kb * b) / kg;
    return vec4<f32>(clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
