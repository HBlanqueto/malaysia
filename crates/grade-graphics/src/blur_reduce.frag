#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 source_size;
uniform vec2 target_size;
uniform vec4 region;
uniform float span;
uniform float straight;

vec4 gathered(vec2 index) {
    if (index.x < region.x || index.y < region.y || index.x >= region.z || index.y >= region.w) {
        return vec4(0.0);
    }
    vec4 texel = texture2D(tex, (index + 0.5) / source_size);
    return vec4(texel.rgb, mix(texel.a, 1.0, straight));
}

void main() {
    vec2 index = floor(v_coords * target_size) * span;
    vec4 sum = gathered(index);
    if (span > 1.5) {
        sum += gathered(index + vec2(1.0, 0.0)) + gathered(index + vec2(0.0, 1.0)) + gathered(index + vec2(1.0, 1.0));
        sum *= 0.25;
    }
    gl_FragColor = sum * alpha;
}
