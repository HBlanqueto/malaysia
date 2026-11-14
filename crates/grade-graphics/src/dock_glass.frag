#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 texture_size;
uniform vec4 geometry;
uniform float radius;
uniform float exponent;

uniform vec4 lens;

// The Liquid Glass config's liquid-glass.frag over the sharp backdrop, along the frame's normal: a soft blur first, as its Kawase pass does, then the rim shows what lies deeper in, each colour channel a little differently. lens = (depth, strength, split, blur), ctdesign::DockSurface.
vec3 soft(vec2 px, float spread) {
    vec2 low = geometry.xy;
    vec2 high = geometry.xy + geometry.zw;
    return 0.25 * (
        texture2D(tex, clamp(px + vec2(spread), low, high) / texture_size).rgb
        + texture2D(tex, clamp(px - vec2(spread), low, high) / texture_size).rgb
        + texture2D(tex, clamp(px + vec2(spread, -spread), low, high) / texture_size).rgb
        + texture2D(tex, clamp(px + vec2(-spread, spread), low, high) / texture_size).rgb
    );
}

vec3 refracted(vec2 px) {
    float thickness = min(geometry.z, geometry.w);
    float inward = clamp(-frame_sdf(px, geometry, radius, exponent) / (lens.x * thickness), 0.0, 1.0);
    float bend = 1.0 - sqrt(max(1.0 - (1.0 - inward) * (1.0 - inward), 0.0));
    vec2 shift = frame_normal(px, geometry, radius, exponent) * bend * lens.y * thickness;
    float spread = lens.w * thickness;
    return vec3(
        soft(px - shift * (1.0 - lens.z), spread).r,
        soft(px - shift, spread).g,
        soft(px - shift * (1.0 + lens.z), spread).b
    );
}

void main() {
    vec2 px = v_coords * texture_size;
    gl_FragColor = dock_frame(px, geometry, radius, exponent, refracted(px)) * alpha;
}
