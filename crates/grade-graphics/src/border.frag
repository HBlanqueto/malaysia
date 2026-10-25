precision mediump float;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float width;
uniform float side;
uniform vec4 color;
uniform float alpha;

float rounded_rect_sdf(vec2 p, vec2 center, vec2 sz, float r) {
    vec2 d = abs(p - center) - sz * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

float sdf_coverage(float dist) {
    return clamp(0.5 - dist, 0.0, 1.0);
}

void main() {
    vec2 px = v_coords * size;
    vec2 center = rect.xy + rect.zw * 0.5;
    float dist = rounded_rect_sdf(px, center, rect.zw, radius);

    // side ==  1.0 -> ring outside the rounded rect (outer border).
    // side == -1.0 -> ring inside the rounded rect (inner line).
    float coverage;
    if (side > 0.0) {
        coverage = sdf_coverage(dist - width) - sdf_coverage(dist);
    } else {
        coverage = sdf_coverage(dist) - sdf_coverage(dist + width);
    }

    // Premultiplied output for Smithay's blend.
    gl_FragColor = color * (coverage * alpha);
}
