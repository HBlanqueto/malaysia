precision highp float;

varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
uniform vec2 origin;
uniform vec2 bounds;
uniform float radius;
uniform float width;
uniform float sigma;
uniform float y_offset;
uniform float spread;

float coverage(vec2 edge, float corner_radius);

vec2 erf(vec2 x) {
    vec2 s = sign(x);
    vec2 a = abs(x);
    vec2 v = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    v *= v;
    return s - s / (v * v);
}

float gaussian(float x) {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * 3.141592653589793) * sigma);
}

float blurred_row(float x, float y, vec2 half_size, float corner) {
    float delta = min(half_size.y - corner - abs(y), 0.0);
    float curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    vec2 integral = 0.5 + 0.5 * erf((x + vec2(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

void main() {
    vec2 window = v_coords * size - origin;
    vec2 inside = min(window, bounds - window);
    float hidden = coverage(inside - width, radius - width);
    if (hidden >= 1.0) {
        gl_FragColor = vec4(0.0);
        return;
    }
    vec2 half_size = bounds * 0.5 + spread;
    vec2 point = window - bounds * 0.5 - vec2(0.0, y_offset);
    float corner = radius + spread;
    float start = clamp(-3.0 * sigma, point.y - half_size.y, point.y + half_size.y);
    float end = clamp(3.0 * sigma, point.y - half_size.y, point.y + half_size.y);
    float row_height = (end - start) / 8.0;
    float y = start + row_height * 0.5;
    float shadow = 0.0;
    for (int row = 0; row < 8; row++) {
        shadow += blurred_row(point.x, point.y - y, half_size, corner) * gaussian(y) * row_height;
        y += row_height;
    }
    gl_FragColor = vec4(0.0, 0.0, 0.0, alpha * shadow * (1.0 - hidden));
}
