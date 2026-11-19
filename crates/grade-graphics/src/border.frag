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
uniform vec4 glow;
uniform float glow_depth;
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

    float vertical = vertical_normal(px, rect, radius, exponent);
    vec4 tint = mix(color, edge_color, vertical) * coverage;
    float below_line = -dist - width;
    if (side < 0.0 && below_line > 0.0 && below_line < glow_depth) {
        tint += glow * (vertical * exp(-3.3 * below_line / glow_depth));
    }
    gl_FragColor = tint * alpha;
}
