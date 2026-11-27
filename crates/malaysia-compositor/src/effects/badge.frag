precision highp float;

varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
uniform float radius;
uniform vec4 fill;
uniform vec4 cross_color;
uniform float cross_reach;
uniform float cross_width;

float coverage(vec2 edge, float corner_radius);

float overlap(float position, float half_extent) {
    return max(min(position + 0.5, half_extent) - max(position - 0.5, -half_extent), 0.0);
}

float arm(vec2 point, vec2 direction) {
    return overlap(dot(point, direction), cross_reach)
        * overlap(dot(point, vec2(-direction.y, direction.x)), cross_width * 0.5);
}

void main() {
    vec2 position = floor(v_coords * size) + 0.5;
    vec2 point = position - size * 0.5;
    float crossed = max(arm(point, normalize(vec2(1.0, 1.0))), arm(point, normalize(vec2(1.0, -1.0))));
    gl_FragColor = mix(fill, cross_color, crossed) * coverage(min(position, size - position), radius) * alpha;
}
