varying vec2 v_coords;

uniform vec2 size;
uniform vec4 rect;
uniform float radius;
uniform float scale;
uniform float exponent;
uniform float alpha;

// With transparency reduced the glass is opaque, so nothing behind it shows.
void main() {
    vec2 px = v_coords * size * scale;
    gl_FragColor = dock_frame(px, rect, radius, exponent, vec3(0.0)) * alpha;
}
