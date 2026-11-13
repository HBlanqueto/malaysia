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
uniform vec4 panel;
uniform float panel_radius;
uniform float panel_exponent;
uniform vec3 shadow;
uniform float shade;

// The island's shadow on the window around it, inside the window's own shape: darkest next to the panel and fading with the distance, in a dark tone that takes the chroma of the wallpaper behind each stretch.
// panel is x, y, width, height; shadow is opacity, span, tint.
void main() {
    vec2 px = v_coords * texture_size;
    float inside_window = window_coverage(frame_sdf(px, geometry, radius, exponent));
    float dist = frame_sdf(px, panel, panel_radius, panel_exponent);
    vec3 wallpaper = oklab_from_linear(linear_from_srgb(texture2D(tex, v_coords).rgb));
    vec3 color = srgb_from_linear(clamp(linear_from_oklab(vec3(shade, wallpaper.yz * shadow.z)), 0.0, 1.0));
    float opacity = shadow.x * exp(-max(dist, 0.0) / shadow.y) * clamp(dist + 0.5, 0.0, 1.0) * inside_window;
    gl_FragColor = vec4(color, 1.0) * (opacity * alpha);
}
