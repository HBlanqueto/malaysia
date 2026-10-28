// One window shape for clip, border and shadow.
//
// `p` is in physical pixels, relative to the element; `rect` is that element's
// window rectangle (xy origin, zw size) and `radius` is in the same unit.
// `exponent` == 2.0 is the plain rounded rectangle; larger values make the
// corners squarer: |x|^n + |y|^n = radius^n around the nearest corner centre.
float window_sdf(vec2 p, vec4 rect, float radius, float exponent) {
    vec2 half_size = rect.zw * 0.5;
    vec2 center = rect.xy + half_size;
    vec2 corner = abs(p - center) - half_size + radius;
    vec2 outside = max(corner, 0.0);
    return pow(pow(outside.x, exponent) + pow(outside.y, exponent), 1.0 / exponent) - radius;
}

// Exactly one physical pixel of antialiasing around the shape.
float window_coverage(float dist) {
    return clamp(0.5 - dist, 0.0, 1.0);
}
