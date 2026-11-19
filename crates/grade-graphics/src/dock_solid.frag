varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float scale;
uniform float exponent;
uniform float alpha;

void main() {
    vec2 px = v_coords * size * scale;
    float dist = frame_sdf(px, rect, radius, exponent);
    gl_FragColor = dock_frame(dist, frame_normal(px, rect, radius, exponent), vec3(0.0), past_corner(px, rect, radius)) * alpha;
}
