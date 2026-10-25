precision mediump float;
//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

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

float rounded_rect_sdf(vec2 p, vec2 center, vec2 size, float r) {
    vec2 d = abs(p - center) - size * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

void main() {
    vec2 px = v_coords * texture_size;
    vec2 center = geometry.xy + geometry.zw * 0.5;
    float dist = rounded_rect_sdf(px, center, geometry.zw, radius);
    float coverage = 0.5 - dist;
    coverage = clamp(coverage, 0.0, 1.0);

    vec4 color = texture2D(tex, v_coords);
    color.a *= coverage * alpha;
    gl_FragColor = color;
}
