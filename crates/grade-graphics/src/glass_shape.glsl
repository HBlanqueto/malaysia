uniform vec2 glass_size;
uniform vec4 glass_rect;
uniform float glass_corner;
uniform float glass_unit;

float glass_distance(vec2 at, out vec2 normal) {
    vec2 from_centre = at - (glass_rect.xy + glass_rect.zw * 0.5);
    vec2 beyond = abs(from_centre) - (glass_rect.zw * 0.5 - glass_corner);
    vec2 side = vec2(from_centre.x >= 0.0 ? 1.0 : -1.0, from_centre.y >= 0.0 ? 1.0 : -1.0);
    if (beyond.x > 0.0 && beyond.y > 0.0) {
        float length_out = length(beyond);
        normal = side * beyond / length_out;
        return length_out - glass_corner;
    }
    if (beyond.x > beyond.y) {
        normal = vec2(side.x, 0.0);
        return beyond.x - glass_corner;
    }
    normal = vec2(0.0, side.y);
    return beyond.y - glass_corner;
}
