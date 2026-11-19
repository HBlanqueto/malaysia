#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform mat3 color_matrix;
uniform float invert;
uniform float contrast;

void main() {
    vec4 color = texture2D(tex, v_coords);
    vec3 rgb = color.a > 0.0 ? color.rgb / color.a : vec3(0.0);
    rgb = clamp(color_matrix * rgb, 0.0, 1.0);
    rgb = mix(rgb, vec3(1.0) - rgb, invert);
    rgb = clamp((rgb - 0.5) * contrast + 0.5, 0.0, 1.0);
    gl_FragColor = vec4(rgb * color.a, color.a) * alpha;
}
