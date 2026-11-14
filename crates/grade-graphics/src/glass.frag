uniform float alpha;
varying vec2 v_coords;

uniform vec4 geometry;
uniform float radius;
uniform float exponent;
uniform vec3 solid;
uniform float settle;

// settle 0 is the glass; settle 1 is the flat solid tone.
void main() {
    vec2 px = v_coords * texture_size;
    float coverage = window_coverage(frame_sdf(px, geometry, radius, exponent));
    float ramp;
    vec3 color = mix(glass_fill(px, ramp), solid, settle);
    gl_FragColor = vec4(color, 1.0) * (coverage * alpha);
}
