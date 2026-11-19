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

uniform vec2 lens;

vec3 refracted(vec2 px, float dist, vec2 normal) {
    float thickness = min(geometry.z, geometry.w);
    float inward = clamp(-dist / (lens.x * thickness), 0.0, 1.0);
    return texture2D(tex, (px - normal * bend_at(inward) * lens.y * thickness) / texture_size).rgb;
}

void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    vec2 normal = frame_normal(px, geometry, radius, exponent);
    gl_FragColor = dock_frame(dist, normal, refracted(px, dist, normal), past_corner(px, geometry, radius)) * alpha;
}
