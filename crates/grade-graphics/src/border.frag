varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float width;
uniform float side;
uniform float scale;
uniform float exponent;
uniform vec4 color;
uniform float alpha;

void main() {
    // `size` is the element's logical size; `scale` lands on the physical
    // pixels the rect, radius and width are already expressed in.
    vec2 px = v_coords * size * scale;
    float dist = window_sdf(px, rect, radius, exponent);

    // side ==  1.0 -> ring outside the rounded rect (outer border).
    // side == -1.0 -> ring inside the rounded rect (inner line).
    float coverage;
    if (side > 0.0) {
        coverage = window_coverage(dist - width) - window_coverage(dist);
    } else {
        coverage = window_coverage(dist) - window_coverage(dist + width);
    }

    // Premultiplied output for Smithay's blend.
    gl_FragColor = color * (coverage * alpha);
}
