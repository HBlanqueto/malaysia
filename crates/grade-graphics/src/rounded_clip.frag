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
uniform float exponent;

void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    float coverage = window_coverage(dist);

    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    gl_FragColor = color * (coverage * alpha);
}
