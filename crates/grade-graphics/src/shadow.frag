precision mediump float;

varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform vec4 window_rect;
uniform float radius;
uniform float blur;
uniform vec2 offset;
uniform vec4 color;
uniform float alpha;

float rounded_rect_sdf(vec2 p, vec2 center, vec2 sz, float r) {
    vec2 d = abs(p - center) - sz * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

// Abramowitz & Stegun 7.1.26 approximation of the error function.
float erf(float x) {
    float a = abs(x);
    float t = 1.0 / (1.0 + 0.47047 * a);
    float y = 1.0 - t * (0.3480242 + t * (-0.0958798 + t * 0.7478556)) * exp(-a * a);
    return sign(x) * y;
}

// 1-pixel antialiased coverage of a rounded rectangle.
float window_coverage(vec2 p, vec2 center, vec2 sz, float r) {
    float d = rounded_rect_sdf(p, center, sz, r);
    return clamp(0.5 - d, 0.0, 1.0);
}

void main() {
    // Smithay gives v_coords in top-left origin already; size is the element
    // size in physical pixels. Stay local: no gl_FragCoord flipping.
    vec2 px = v_coords * size;

    // The shadow is a rounded rectangle displaced by `offset`, blurred by a
    // Gaussian whose sigma is half the CSS blur radius. The signed distance
    // is negative inside the shadow shape and positive outside; the Gaussian
    // CDF gives high alpha inside and fades to zero far outside.
    vec2 shadow_center = rect.xy + rect.zw * 0.5 + offset;
    float d = rounded_rect_sdf(px, shadow_center, rect.zw, radius);

    float sigma = max(blur / 2.0, 0.001);
    float shadow = 0.5 - 0.5 * erf(d / (sqrt(2.0) * sigma));

    // Cut a hole in the shape of the unshifted window so the shadow does not
    // paint over the window itself. The transition uses the same 1-pixel SDF
    // coverage so the edge blends smoothly with no bright gap.
    vec2 window_center = window_rect.xy + window_rect.zw * 0.5;
    float cover = window_coverage(px, window_center, window_rect.zw, radius);

    // Smithay blends premultiplied; multiply the whole colour, not just alpha.
    gl_FragColor = color * (shadow * (1.0 - cover) * alpha);
}
