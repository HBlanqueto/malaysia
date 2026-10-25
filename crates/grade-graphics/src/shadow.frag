precision mediump float;

varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float blur;
uniform vec2 offset;
uniform vec4 color;
uniform float alpha;

float rounded_rect_sdf(vec2 p, vec2 center, vec2 sz, float r) {
    vec2 d = abs(p - center) - sz * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

void main() {
    // Smithay gives v_coords in top-left origin already; size is the element
    // size in physical pixels. Stay local: no gl_FragCoord flipping.
    vec2 px = v_coords * size;

    // The shadow is the glow *outside* the rounded window. The window itself
    // is the clear hole; offset is the direction the shadow is cast, so the
    // hole moves in the same direction (positive Y casts it downward).
    vec2 center = rect.xy + rect.zw * 0.5 + offset;
    float dist = rounded_rect_sdf(px, center, rect.zw, radius);

    float d = max(dist, 0.0);
    // CSS blur radius: sigma = blur / 2.
    float sigma = max(blur / 2.0, 0.001);
    float shadow = exp(-d * d / (2.0 * sigma * sigma));
    // Keep the shadow out of the window: only the exterior coverage counts.
    float exterior = clamp(0.5 + dist, 0.0, 1.0);

    // Smithay blends premultiplied; multiply the whole colour, not just alpha.
    gl_FragColor = color * (shadow * exterior * alpha);
}
