#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 texture_size;
uniform vec4 geometry;
uniform float radius;

float fade(float inside) {
    return smoothstep(-radius, radius, inside);
}

void main() {
    vec2 px = v_coords * texture_size;
    vec2 near = px - geometry.xy;
    vec2 far = geometry.xy + geometry.zw - px;
    float coverage = fade(near.x) * fade(near.y) * fade(far.x) * fade(far.y);

    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    gl_FragColor = color * (coverage * alpha);
}
