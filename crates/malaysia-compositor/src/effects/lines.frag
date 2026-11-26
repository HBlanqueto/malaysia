precision highp float;

varying vec2 v_coords;
uniform vec2 size;
uniform float width;
uniform float radius;
uniform vec4 outline;
uniform vec4 inner_line;

float coverage(vec2 edge, float corner_radius);

void main() {
    vec2 position = floor(v_coords * size) + 0.5;
    vec2 edge = min(position, size - position);
    float outer_shape = coverage(edge, radius + width);
    float frame_shape = coverage(edge - width, radius);
    float inner_shape = coverage(edge - 2.0 * width, radius - width);
    gl_FragColor = outline * (outer_shape - frame_shape) + inner_line * (frame_shape - inner_shape);
}
