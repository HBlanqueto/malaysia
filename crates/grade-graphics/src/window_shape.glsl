float window_sdf(vec2 p, vec4 rect, float radius, float exponent) {
    vec2 half_size = rect.zw * 0.5;
    vec2 center = rect.xy + half_size;
    radius = min(radius, min(half_size.x, half_size.y));
    vec2 corner = abs(p - center) - half_size + radius;
    vec2 outside = max(corner, 0.0);
    float inside = min(max(corner.x, corner.y), 0.0);
    return pow(pow(outside.x, exponent) + pow(outside.y, exponent), 1.0 / exponent)
        + inside - radius;
}

float window_coverage(float dist) {
    return clamp(0.5 - dist, 0.0, 1.0);
}
