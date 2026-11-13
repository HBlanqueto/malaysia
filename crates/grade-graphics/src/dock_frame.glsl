uniform vec4 glass;
uniform vec2 rim;
uniform vec3 corner;
uniform vec4 outline;
uniform vec3 glow_near;
uniform vec3 glow_far;

float luminance(vec3 color) {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

// macOS lights the rim to lift + share of the glass's luminance, as a white or black overlay.
vec4 rim_overlay(float glass_luminance) {
    float shift = rim.x + rim.y * glass_luminance - glass_luminance;
    if (shift >= 0.0) {
        return vec4(1.0, 1.0, 1.0, clamp(shift / max(1.0 - glass_luminance, 0.001), 0.0, 1.0));
    }
    return vec4(0.0, 0.0, 0.0, clamp(-shift / max(glass_luminance, 0.001), 0.0, 1.0));
}

float band(float dist, float depth) {
    return window_coverage(dist + depth) - window_coverage(dist + depth + 1.0);
}

float rim_depth(float dist) {
    return band(dist, 0.0)
        + glow_near.x * band(dist, 1.0) + glow_near.y * band(dist, 2.0) + glow_near.z * band(dist, 3.0)
        + glow_far.x * band(dist, 4.0) + glow_far.y * band(dist, 5.0) + glow_far.z * band(dist, 6.0);
}

// The glass over what is behind it, the rim one pixel inside with its glow, and the outline one pixel outside.
// corner = (decay of the rim, angle where the outline starts, angle where it is whole), in degrees from vertical.
vec4 dock_frame(vec2 px, vec4 rect, float radius, float exponent, vec3 behind) {
    float dist = frame_sdf(px, rect, radius, exponent);
    float angle = degrees(acos(clamp(vertical_normal(px, rect, radius, exponent), 0.0, 1.0)));
    vec3 seen = mix(behind, glass.rgb, glass.a);
    vec4 light = rim_overlay(luminance(seen));
    float lit = light.a * exp(-angle / corner.x) * rim_depth(dist);
    vec3 inside = mix(seen, light.rgb, lit);
    float dark = clamp((angle - corner.y) / (corner.z - corner.y), 0.0, 1.0);
    float ring = (window_coverage(dist - 1.0) - window_coverage(dist)) * outline.a * dark;
    return vec4(inside, 1.0) * window_coverage(dist) + vec4(outline.rgb, 1.0) * ring;
}
