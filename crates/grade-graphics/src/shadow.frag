varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform vec4 window_rect;
uniform float radius;
uniform float blur;
uniform vec2 offset;
uniform float scale;
uniform float exponent;
uniform vec4 color;
uniform float alpha;

// Abramowitz & Stegun 7.1.26 approximation of the error function.
float erf(float x) {
    float a = abs(x);
    float t = 1.0 / (1.0 + 0.47047 * a);
    float y = 1.0 - t * (0.3480242 + t * (-0.0958798 + t * 0.7478556)) * exp(-a * a);
    return sign(x) * y;
}

void main() {
    // `size` is the element's logical size; `scale` lands on the physical
    // pixels the rect, radius, blur and offset are already expressed in.
    vec2 px = v_coords * size * scale;

    // The shadow is the window shape displaced by `offset`, blurred by a
    // Gaussian whose sigma is half the CSS blur radius. The signed distance
    // is negative inside the shadow shape and positive outside; the Gaussian
    // CDF gives high alpha inside and fades to zero far outside.
    float d = window_sdf(px, vec4(rect.xy + offset, rect.zw), radius, exponent);

    float sigma = max(blur / 2.0, 0.001);
    float shadow = 0.5 - 0.5 * erf(d / (sqrt(2.0) * sigma));

    // Cut a hole in the shape of the unshifted window so the shadow does not
    // paint over the window itself. The transition uses the same one-physical-
    // pixel coverage so the edge blends smoothly with no bright gap.
    float cover = window_coverage(window_sdf(px, window_rect, radius, exponent));

    // Smithay blends premultiplied; multiply the whole colour, not just alpha.
    gl_FragColor = color * (shadow * (1.0 - cover) * alpha);
}
