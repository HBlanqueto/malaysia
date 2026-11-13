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
    gl_FragColor = dock_frame(px, geometry, radius, exponent, texture2D(tex, v_coords).rgb) * alpha;
}
