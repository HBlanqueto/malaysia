uniform float alpha;
varying vec2 v_coords;

uniform vec4 geometry;
uniform float radius;
uniform float exponent;
uniform vec3 rim;
uniform float rim_cap;
uniform float opacity;
uniform float width;

// The inner border of the panel, derived from the fill under it: rim.x lighter (rim.y more where the wallpaper shows), with rim.z times its chroma up to rim_cap and the same hue.
void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    float coverage = window_coverage(dist) - window_coverage(dist + width);
    float ramp;
    vec3 fill = oklab_from_linear(linear_from_srgb(glass_fill(px, ramp)));
    float fill_chroma = length(fill.yz);
    vec2 ab = fill.yz * (min(fill_chroma * rim.z, rim_cap) / max(fill_chroma, 0.000001));
    vec3 lab = vec3(min(fill.x + rim.x + rim.y * ramp, 1.0), ab);
    vec3 color = clamp(srgb_from_linear(clamp(linear_from_oklab(lab), 0.0, 1.0)), 0.0, 1.0);
    gl_FragColor = vec4(color, 1.0) * (coverage * alpha * opacity);
}
