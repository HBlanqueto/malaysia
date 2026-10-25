precision mediump float;

uniform vec2 output_size;
uniform vec4 rect;
uniform float radius;
uniform float blur;
uniform vec2 offset;
uniform vec4 color;
uniform float alpha;

float rounded_rect_sdf(vec2 p, vec2 center, vec2 size, float r) {
    vec2 d = abs(p - center) - size * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

void main() {
    // Smithay's coordinates are top-left origin; gl_FragCoord is bottom-left.
    vec2 px = vec2(gl_FragCoord.x, output_size.y - gl_FragCoord.y);

    vec2 center = rect.xy + rect.zw * 0.5 - offset;
    float dist = rounded_rect_sdf(px, center, rect.zw, radius);

    // The shadow is only outside the rounded window.
    float d = max(dist, 0.0);
    float sigma = max(blur * 0.5, 0.001);
    float shadow = exp(-d * d / (2.0 * sigma * sigma));

    vec4 c = color;
    c.a *= shadow * alpha;
    gl_FragColor = c;
}
