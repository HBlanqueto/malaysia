varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float blur;
uniform vec2 offset;
uniform float scale;
uniform float exponent;
uniform vec4 color;
uniform float alpha;

float erf(float x) {
    float a = abs(x);
    float t = 1.0 / (1.0 + 0.47047 * a);
    float y = 1.0 - t * (0.3480242 + t * (-0.0958798 + t * 0.7478556)) * exp(-a * a);
    return sign(x) * y;
}

void main() {
    vec2 px = v_coords * size * scale;

    float d = window_sdf(px, vec4(rect.xy + offset, rect.zw), radius, exponent);

    float sigma = max(blur / 2.0, 0.001);
    float shadow = 0.5 - 0.5 * erf(d / (sqrt(2.0) * sigma));

    float cover = window_coverage(window_sdf(px, rect, radius, exponent));

    gl_FragColor = color * (shadow * (1.0 - cover) * alpha);
}
