#version 100

precision highp float;
uniform sampler2D tex;
varying vec2 v_coords;

uniform vec2 texel;
uniform vec2 valid;
uniform vec2 direction;
uniform float sigma;

void main() {
    vec2 position = floor(v_coords / texel) + 0.5;
    float along = dot(position, direction);
    float limit = dot(valid, direction);
    float reach = ceil(3.0 * sigma);
    vec3 sum = texture2D(tex, position * texel).rgb;
    float total = 1.0;
    for (int tap = 1; tap <= 32; tap++) {
        float offset = float(tap);
        if (offset > reach) {
            break;
        }
        float weight = exp(-offset * offset / (2.0 * sigma * sigma));
        if (along + offset < limit) {
            sum += weight * texture2D(tex, (position + offset * direction) * texel).rgb;
            total += weight;
        }
        if (along - offset > 0.0) {
            sum += weight * texture2D(tex, (position - offset * direction) * texel).rgb;
            total += weight;
        }
    }
    gl_FragColor = vec4(sum / total, 1.0);
}
