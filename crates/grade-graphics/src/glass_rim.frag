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
uniform vec4 line;
uniform float chroma;
uniform float chroma_cap;
uniform vec4 tint;
uniform float width;

// The inner border of the panel: a line of the token's lightness (white in dark, gray in light) that takes the chroma of the wallpaper behind each stretch, or of the accent (tint.a is 1).
void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    float coverage = window_coverage(dist) - window_coverage(dist + width);
    vec3 wallpaper = texture2D(tex, v_coords).rgb;
    vec3 color = take_chroma_capped(line.rgb, mix(wallpaper, tint.rgb, tint.a), chroma, chroma_cap);
    gl_FragColor = vec4(color * line.a, line.a) * (coverage * alpha);
}
