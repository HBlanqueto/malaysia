#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 coords_scale;
uniform vec2 edge;
uniform vec4 lens;
uniform vec4 profile;
uniform vec4 share_a;
uniform vec4 share_b;
uniform float constant_weight;

float share_at(float depth) {
    if (depth <= share_b.x) {
        return share_a.x;
    }
    if (depth <= share_b.y) {
        return mix(share_a.x, share_a.y, (depth - share_b.x) / (share_b.y - share_b.x));
    }
    if (depth <= share_b.z) {
        return mix(share_a.y, share_a.z, (depth - share_b.y) / (share_b.z - share_b.y));
    }
    return share_a.z;
}

float lens_offset(float depth) {
    float u = depth / lens.y;
    if (lens.y <= 0.0 || u < 0.0 || u >= 1.0) {
        return 0.0;
    }
    return abs(lens.x) * lens.z * (exp(-lens.w * u) - exp(-lens.w)) / (1.0 - exp(-lens.w));
}

void main() {
    vec2 at = v_coords * glass_size;
    vec2 normal;
    float distance_px = glass_distance(at, normal);
    float distance_model = distance_px / glass_unit;
    float shift = distance_model < 0.0 ? lens_offset(-distance_model) * glass_unit : 0.0;
    vec4 blurred = texture2D(tex, clamp((at - normal * shift) / glass_size * coords_scale, edge, 1.0 - edge));
    float weight = 1.0;
    if (constant_weight < 0.5) {
        float sigma = min(profile.x * share_at(distance_model), profile.y);
        weight = clamp((sigma - profile.z) / (profile.w - profile.z), 0.0, 1.0);
    }
    gl_FragColor = vec4(blurred.rgb / max(blurred.a, 0.0001) * weight, weight) * alpha;
}
