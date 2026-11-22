#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 coords_scale;
uniform vec2 edge;

void main() {
    vec4 blurred = texture2D(tex, clamp(v_coords * coords_scale, edge, 1.0 - edge));
    gl_FragColor = vec4(blurred.rgb / max(blurred.a, 0.0001), 1.0) * alpha;
}
