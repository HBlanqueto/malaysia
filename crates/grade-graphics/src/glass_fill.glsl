#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform vec2 texture_size;
uniform vec3 base;
uniform float chroma;
uniform float chroma_cap;
uniform float lightness;
uniform vec4 panel;
uniform vec4 outer;
uniform vec4 edge;
uniform float edge_cap;
uniform float edge_offset;

vec3 wallpaper_at(vec2 px) {
    return texture2D(tex, clamp(px / texture_size, 0.0, 1.0)).rgb;
}

// The glass of an active panel at px. tex is the blurred stack behind the window: the fill takes its chroma and part of its lightness, and along the left, top and bottom edges of the panel the wallpaper just outside the window.
// panel and outer are x, y, width, height; edge is intensity, span, chroma, lightness. ramp is how much the wallpaper shows: 1 at those edges, 0 far from them.
vec3 glass_fill(vec2 px, out float ramp) {
    vec3 wallpaper = wallpaper_at(px);
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
    ramp = max(reach.x, max(reach.y, reach.z));
    return mix(color, take_tint(base, outside, edge.z, edge.w, edge_cap), ramp * edge.x);
}
