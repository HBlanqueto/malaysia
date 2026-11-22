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
uniform vec4 strip;
uniform vec3 along;
uniform vec4 stops_at;
uniform vec4 stops_alpha;

float fade(float t) {
    float shown = stops_alpha.x;
    shown = mix(shown, stops_alpha.y, clamp((t - stops_at.x) / max(stops_at.y - stops_at.x, 0.0001), 0.0, 1.0));
    shown = mix(shown, stops_alpha.z, clamp((t - stops_at.y) / max(stops_at.z - stops_at.y, 0.0001), 0.0, 1.0));
    shown = mix(shown, stops_alpha.w, clamp((t - stops_at.z) / max(stops_at.w - stops_at.z, 0.0001), 0.0, 1.0));
    return shown;
}

void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    float coverage = window_coverage(dist);
    float t = dot((px - strip.xy) / strip.zw, along.xy) + along.z;

    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    gl_FragColor = color * (coverage * fade(clamp(t, 0.0, 1.0)) * alpha);
}
