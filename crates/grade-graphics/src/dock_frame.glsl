uniform vec4 glass;
uniform vec2 rim;
uniform vec3 corner;
uniform float rim_side;
uniform vec4 outline;
uniform vec3 glow_near;
uniform vec3 glow_far;
uniform vec4 tone;
uniform vec2 tone_outline;
uniform vec4 plate;
uniform vec4 plate_edge;
uniform vec4 plate_ramp;

float luminance(vec3 color) {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

vec4 rim_overlay(float glass_luminance) {
    float shift = rim.x + rim.y * glass_luminance - glass_luminance;
    if (shift >= 0.0) {
        return vec4(1.0, 1.0, 1.0, clamp(shift / max(1.0 - glass_luminance, 0.001), 0.0, 1.0));
    }
    return vec4(0.0, 0.0, 0.0, clamp(-shift / max(glass_luminance, 0.001), 0.0, 1.0));
}

vec3 light_toward(vec3 color, float target) {
    float color_luminance = max(luminance(color), 0.001);
    float gain = target / color_luminance;
    if (tone.z > 0.0) {
        gain = min(gain, tone.w);
    }
    return min(color * gain + (target - color_luminance * gain), vec3(1.0));
}

vec3 oklab_from_srgb(vec3 color) {
    return oklab_from_linear(linear_from_srgb(color));
}

bool in_gamut(vec3 linear) {
    return all(greaterThanEqual(linear, vec3(0.0))) && all(lessThanEqual(linear, vec3(1.0)));
}

vec3 srgb_from_oklab(vec3 lab) {
    float fit = 1.0;
    if (!in_gamut(linear_from_oklab(lab))) {
        float low = 0.0;
        float high = 1.0;
        for (int i = 0; i < 10; i++) {
            float mid = 0.5 * (low + high);
            if (in_gamut(linear_from_oklab(vec3(lab.x, lab.yz * mid)))) {
                low = mid;
            } else {
                high = mid;
            }
        }
        fit = low;
    }
    return srgb_from_linear(clamp(linear_from_oklab(vec3(lab.x, lab.yz * fit)), 0.0, 1.0));
}

const float CAP_SOFTNESS = 0.1;

vec3 toned_glass(vec3 seen, vec3 behind) {
    float lifted = oklab_from_srgb(seen).x + tone.y;
    float lightness = smooth_union(lifted, tone.z, CAP_SOFTNESS);
    float chroma = tone.x * min(lightness / max(lifted, 0.001), 1.0);
    return srgb_from_oklab(vec3(lightness, oklab_from_srgb(behind).yz * chroma));
}

vec3 toned_outline(vec3 body) {
    vec3 lab = oklab_from_srgb(body);
    return srgb_from_oklab(vec3(lab.x * tone_outline.x, lab.yz * tone_outline.y));
}

float band(float dist, float depth) {
    return window_coverage(dist + depth) - window_coverage(dist + depth + 1.0);
}

float rim_depth(float dist) {
    return band(dist, 0.0)
        + glow_near.x * band(dist, 1.0) + glow_near.y * band(dist, 2.0) + glow_near.z * band(dist, 3.0)
        + glow_far.x * band(dist, 4.0) + glow_far.y * band(dist, 5.0) + glow_far.z * band(dist, 6.0);
}

float past_corner(vec2 p, vec4 rect, float radius) {
    vec2 near = min(p - rect.xy, rect.xy + rect.zw - p);
    return near.x < radius ? max(near.y - radius, 0.0) : 0.0;
}

vec4 plate_frame(float dist, float angle, vec3 seen) {
    vec3 lab = oklab_from_srgb(seen);
    float lightness = mix(lab.x, plate.x, plate.y);
    vec3 color = srgb_from_oklab(vec3(lightness, lab.yz * plate.z));
    float light = band(dist, 1.0);
    if (light > 0.0) {
        vec3 lit = light_toward(color, luminance(color) + plate_ramp.w);
        color = mix(color, lit, light * exp(-pow(angle / plate_edge.x, plate_edge.y)));
    }
    float outline = band(dist, 0.0);
    if (outline > 0.0) {
        vec3 line = srgb_from_oklab(vec3(lightness * plate_edge.w, lab.yz * plate.z));
        float grow = clamp((min(angle, 90.0) - plate_ramp.x) / (plate_ramp.y - plate_ramp.x), 0.0, 1.0);
        color = mix(color, line, outline * (plate_edge.z + (1.0 - plate_edge.z) * grow));
    }
    return vec4(color, 1.0) * window_coverage(dist);
}

vec4 dock_frame(float dist, vec2 normal, vec3 behind, float along) {
    float angle = degrees(acos(clamp(abs(normal.y), 0.0, 1.0)));
    vec3 seen = mix(behind, glass.rgb, glass.a);
    if (plate.w > 0.5) {
        return plate_frame(dist, angle + along * plate_ramp.z, seen);
    }
    float edge = max(exp(-angle / corner.x), rim_side) * rim_depth(dist);
    bool toned = tone.z > 0.0;
    vec3 body = seen;
    vec3 inside;
    if (toned) {
        body = toned_glass(seen, behind);
        vec3 lit = light_toward(body, rim.x + rim.y * luminance(body));
        inside = mix(body, lit, clamp(edge, 0.0, 1.0));
    } else {
        vec4 light = rim_overlay(luminance(seen));
        inside = mix(seen, light.rgb, light.a * edge);
    }
    float dark = clamp((angle - corner.y) / (corner.z - corner.y), 0.0, 1.0);
    float ring = (window_coverage(dist - 1.0) - window_coverage(dist)) * (toned ? 1.0 : outline.a) * dark;
    vec3 line = outline.rgb;
    if (toned && ring > 0.0) {
        line = toned_outline(body);
    }
    return vec4(inside, 1.0) * window_coverage(dist) + vec4(line, 1.0) * ring;
}
