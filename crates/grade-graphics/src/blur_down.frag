#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 half_pixel;
uniform float offset;

// Offscreen textures repeat; clamping keeps one edge from bleeding into the other.
vec4 sample_at(vec2 shift) {
    return texture2D(tex, clamp(v_coords + shift, half_pixel, 1.0 - half_pixel));
}

void main() {
    vec2 spread = half_pixel * offset;
    vec4 color = sample_at(vec2(0.0)) * 4.0
        + sample_at(-spread)
        + sample_at(spread)
        + sample_at(vec2(spread.x, -spread.y))
        + sample_at(vec2(-spread.x, spread.y));
    gl_FragColor = color * (alpha / 8.0);
}
