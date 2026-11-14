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

uniform vec4 arrow;
uniform float arrow_slope;
uniform float arrow_edge;

float arrow_triangle(vec2 p, vec2 q) {
    p.x = abs(p.x);
    vec2 a = p - q * clamp(dot(p, q) / dot(q, q), 0.0, 1.0);
    vec2 b = p - q * vec2(clamp(p.x / q.x, 0.0, 1.0), 1.0);
    float s = -sign(q.y);
    vec2 d = min(vec2(dot(a, a), s * (p.x * q.y - p.y * q.x)), vec2(dot(b, b), s * (p.y - q.y)));
    return -sqrt(d.x) * sign(d.y);
}

float smooth_union(float a, float b, float k) {
    float h = max(k - abs(a - b), 0.0) / max(k, 0.0001);
    return min(a, b) - h * h * k * 0.25;
}

// arrow = (tip along the edge from its start, band height, tip radius, fillet); a zero band
// means no arrow. arrow_edge 0 top, 1 bottom, 2 left, 3 right: mapped onto the top edge.
float frame_sdf(vec2 p, vec4 rect, float radius, float exponent) {
    if (arrow.y <= 0.0) {
        return window_sdf(p, rect, radius, exponent);
    }
    if (arrow_edge > 1.5) {
        p = p.yx;
        rect = rect.yxwz;
    }
    if (mod(arrow_edge, 2.0) > 0.5) {
        p.y = 2.0 * rect.y + rect.w - p.y;
    }
    vec4 body = vec4(rect.x, rect.y + arrow.y, rect.z, rect.w - arrow.y);
    float depth = arrow.y - arrow.z + arrow.w;
    vec2 apex = vec2(rect.x + arrow.x, rect.y + arrow.z);
    float spike = arrow_triangle(p - apex, vec2(depth * arrow_slope, depth)) - arrow.z;
    return smooth_union(window_sdf(p, body, radius, exponent), spike, arrow.w);
}

float window_coverage(float dist) {
    return clamp(0.5 - dist, 0.0, 1.0);
}

vec2 frame_normal(vec2 p, vec4 rect, float radius, float exponent) {
    vec2 dx = vec2(0.5, 0.0);
    vec2 dy = vec2(0.0, 0.5);
    vec2 gradient = vec2(
        frame_sdf(p + dx, rect, radius, exponent) - frame_sdf(p - dx, rect, radius, exponent),
        frame_sdf(p + dy, rect, radius, exponent) - frame_sdf(p - dy, rect, radius, exponent)
    );
    return gradient / max(length(gradient), 0.0001);
}

float vertical_normal(vec2 p, vec4 rect, float radius, float exponent) {
    return abs(frame_normal(p, rect, radius, exponent).y);
}
