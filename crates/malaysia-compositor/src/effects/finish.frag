#version 100

precision highp float;
uniform sampler2D tex;
varying vec2 v_coords;

uniform vec2 texel;
uniform vec2 valid;
uniform float saturation;
uniform vec4 veil;
uniform float cap;

void main() {
    vec2 inside = clamp(v_coords, 0.5 * texel, (valid - 0.5) * texel);
    vec3 color = texture2D(tex, inside).rgb;
    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = clamp(luma + saturation * (color - luma), 0.0, 1.0);
    color = min(veil.rgb + color * (1.0 - veil.a), cap);
    gl_FragColor = vec4(color, 1.0);
}
