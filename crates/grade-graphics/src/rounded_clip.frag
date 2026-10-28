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
    float dist = window_sdf(px, geometry, radius, exponent);
    float coverage = window_coverage(dist);

    vec4 color = texture2D(tex, v_coords);
    // Smithay blends premultiplied; scale RGB together with alpha.
    gl_FragColor = color * (coverage * alpha);
}
