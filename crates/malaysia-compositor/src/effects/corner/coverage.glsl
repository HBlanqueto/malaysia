
uniform vec4 curve;
uniform float rise;

float slope(vec4 bezier, float t) {
    float w = 1.0 - t;
    return 3.0 * w * w * (bezier.y - bezier.x)
        + 6.0 * w * t * (bezier.z - bezier.y)
        + 3.0 * t * t * (bezier.w - bezier.z);
}

float signed_distance(vec2 edge, float corner_radius) {
    float u = max(edge.x, edge.y);
    float v = min(edge.x, edge.y);
    vec4 bezier = curve * corner_radius;
    if (u >= bezier.x) {
        return v;
    }
    if (u >= bezier.w) {
        float corner_rise = rise * corner_radius;
        float t = (bezier.x - u) / (bezier.x - bezier.w);
        for (int iteration = 0; iteration < 2; iteration++) {
            float w = 1.0 - t;
            float position = w * w * w * bezier.x
                + 3.0 * w * w * t * bezier.y
                + 3.0 * w * t * t * bezier.z
                + t * t * t * bezier.w;
            t = clamp(t - (position - u) / slope(bezier, t), 0.0, 1.0);
        }
        float tangent = slope(bezier, t);
        return (v - corner_rise * t * t * t) * abs(tangent)
            / length(vec2(tangent, 3.0 * corner_rise * t * t));
    }
    return corner_radius - length(vec2(corner_radius) - vec2(u, v));
}

float coverage(vec2 edge, float corner_radius) {
    return clamp(signed_distance(edge, corner_radius) + 0.5, 0.0, 1.0);
}
