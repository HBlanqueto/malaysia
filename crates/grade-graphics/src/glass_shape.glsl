uniform vec2 glass_size;
uniform vec4 glass_rect;
uniform float glass_corner;
uniform float glass_exponent;
uniform float glass_unit;

float glass_distance(vec2 at, out vec2 normal) {
    normal = frame_normal(at, glass_rect, glass_corner, glass_exponent);
    return window_sdf(at, glass_rect, glass_corner, glass_exponent);
}
