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

float rounded_rect_sdf(vec2 p, vec2 center, vec2 sz, float r) {
    vec2 d = abs(p - center) - sz * 0.5 + r;
    return length(max(d, 0.0)) - r;
}

void main() {
    vec2 px = v_coords * texture_size;
    vec2 center = geometry.xy + geometry.zw * 0.5;
    float dist = rounded_rect_sdf(px, center, geometry.zw, radius);
    // 1-pixel antialiased coverage from the signed distance.
    float coverage = clamp(0.5 - dist, 0.0, 1.0);

    vec4 color = texture2D(tex, v_coords);
    // Smithay blends premultiplied; scale RGB together with alpha.
    gl_FragColor = color * (coverage * alpha);
}
