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
uniform vec3 base;
uniform vec3 solid;
uniform float chroma;
uniform float chroma_cap;
uniform float settle;

// tex is the blurred wallpaper. settle 0 takes its chroma; settle 1 is the flat solid tone.
void main() {
    vec2 px = v_coords * texture_size;
    float coverage = window_coverage(frame_sdf(px, geometry, radius, exponent));
    vec3 wallpaper = texture2D(tex, v_coords).rgb;
    vec3 color = mix(take_chroma_capped(base, wallpaper, chroma, chroma_cap), solid, settle);
    gl_FragColor = vec4(color, 1.0) * (coverage * alpha);
}
