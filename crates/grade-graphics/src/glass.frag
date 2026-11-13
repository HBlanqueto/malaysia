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
uniform float lightness;
uniform float settle;
uniform vec4 panel;
uniform vec4 outer;
uniform vec4 edge;
uniform float edge_cap;
uniform float edge_offset;

vec3 wallpaper_at(vec2 px) {
    return texture2D(tex, clamp(px / texture_size, 0.0, 1.0)).rgb;
}

// tex is the blurred stack behind the window. settle 0 takes its chroma and part of its lightness, and along the left, top and bottom edges of the panel the wallpaper just outside the window;
// settle 1 is the flat solid tone. panel and outer are x, y, width, height; edge is intensity, span, chroma, lightness.
void main() {
    vec2 px = v_coords * texture_size;
    float coverage = window_coverage(frame_sdf(px, geometry, radius, exponent));
    vec3 wallpaper = texture2D(tex, v_coords).rgb;
    vec3 color = take_tint(base, wallpaper, chroma, lightness, chroma_cap);
    vec3 reach = exp(-vec3(
        max(px.x - panel.x, 0.0),
        max(px.y - panel.y, 0.0),
        max(panel.y + panel.w - px.y, 0.0)
    ) / edge.y);
    vec3 outside = (reach.x * wallpaper_at(vec2(outer.x - edge_offset, px.y))
        + reach.y * wallpaper_at(vec2(px.x, outer.y - edge_offset))
        + reach.z * wallpaper_at(vec2(px.x, outer.y + outer.w + edge_offset)))
        / max(reach.x + reach.y + reach.z, 0.0001);
    float shows = max(reach.x, max(reach.y, reach.z)) * edge.x;
    color = mix(color, take_tint(base, outside, edge.z, edge.w, edge_cap), shows);
    color = mix(color, solid, settle);
    gl_FragColor = vec4(color, 1.0) * (coverage * alpha);
}
