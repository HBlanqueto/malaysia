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

uniform vec3 lens;

// liquid-glass.frag with the frame's normal: the blurred backdrop bends along it, deepest at the rim, and each channel bends a little differently. lens = (depth, shift, split), ctdesign::DockSurface.
vec3 refracted(vec2 px) {
    float thickness = min(geometry.z, geometry.w);
    float depth = lens.x * 0.5 * thickness;
    float inward = clamp(-frame_sdf(px, geometry, radius, exponent) / depth, 0.0, 1.0);
    float bend = 1.0 - sqrt(max(1.0 - (1.0 - inward) * (1.0 - inward), 0.0));
    vec2 shift = frame_normal(px, geometry, radius, exponent) * bend * lens.y * thickness;
    return vec3(
        texture2D(tex, (px - shift * (1.0 - lens.z)) / texture_size).r,
        texture2D(tex, (px - shift) / texture_size).g,
        texture2D(tex, (px - shift * (1.0 + lens.z)) / texture_size).b
    );
}

void main() {
    vec2 px = v_coords * texture_size;
    gl_FragColor = dock_frame(px, geometry, radius, exponent, refracted(px)) * alpha;
}
