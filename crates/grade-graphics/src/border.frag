varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float width;
uniform float side;
uniform float scale;
uniform float exponent;
uniform vec4 color;
uniform vec4 edge_color;
uniform float alpha;

void main() {
    vec2 px = v_coords * size * scale;
    float dist = frame_sdf(px, rect, radius, exponent);

    float coverage;
    if (side > 0.0) {
        coverage = window_coverage(dist - width) - window_coverage(dist);
    } else {
        coverage = window_coverage(dist) - window_coverage(dist + width);
    }

    float from_edge = min(px.y - rect.y, rect.y + rect.w - px.y);
    vec4 tint = mix(edge_color, color, smoothstep(0.0, max(radius, 1.0), from_edge));
    gl_FragColor = tint * (coverage * alpha);
}
