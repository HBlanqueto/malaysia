#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec4 frame_from_texture;
uniform vec2 frame_size;
uniform float radius;

float coverage(vec2 edge, float corner_radius);

void main() {
    vec4 color = texture2D(tex, v_coords);

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0) * alpha;
#else
    color = color * alpha;
#endif

    vec2 position = v_coords * frame_from_texture.xy + frame_from_texture.zw;
    gl_FragColor = color * coverage(min(position, frame_size - position), radius);
}
