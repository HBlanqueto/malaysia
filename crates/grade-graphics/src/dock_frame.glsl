uniform vec4 glass;
uniform vec2 rim;
uniform vec3 corner;
uniform vec4 outline;
uniform vec3 glow_near;
uniform vec3 glow_far;
uniform vec3 tone;
uniform vec2 tone_outline;

float luminance(vec3 color) {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

// macOS lights the rim to lift + share of the glass's luminance, as a white or black overlay.
vec4 rim_overlay(float glass_luminance) {
    float shift = rim.x + rim.y * glass_luminance - glass_luminance;
    if (shift >= 0.0) {
        return vec4(1.0, 1.0, 1.0, clamp(shift / max(1.0 - glass_luminance, 0.001), 0.0, 1.0));
    }
    return vec4(0.0, 0.0, 0.0, clamp(-shift / max(glass_luminance, 0.001), 0.0, 1.0));
}

vec3 oklab_from_srgb(vec3 color) {
    return oklab_from_linear(linear_from_srgb(color));
}

bool in_gamut(vec3 linear) {
    return all(greaterThanEqual(linear, vec3(0.0))) && all(lessThanEqual(linear, vec3(1.0)));
}

// The chroma shrinks until the colour fits sRGB, as ctappearance's icon plates do, so the hue holds.
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

// ctdesign::DockTone, sent as tone = (chroma, lift, gain); a gain of 0 keeps the rim and outline of `rim` and `outline`.
vec3 toned_glass(vec3 seen, vec3 behind) {
    return srgb_from_oklab(vec3(oklab_from_srgb(seen).x + tone.y, oklab_from_srgb(behind).yz * tone.x));
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

// The glass over what is behind it, the rim one pixel inside with its glow, and the outline one pixel outside.
// corner = (decay of the rim, angle where the outline starts, angle where it is whole), in degrees from vertical.
vec4 dock_frame(vec2 px, vec4 rect, float radius, float exponent, vec3 behind) {
    float dist = frame_sdf(px, rect, radius, exponent);
    float angle = degrees(acos(clamp(vertical_normal(px, rect, radius, exponent), 0.0, 1.0)));
    vec3 seen = mix(behind, glass.rgb, glass.a);
    float edge = exp(-angle / corner.x) * rim_depth(dist);
    vec3 inside;
    vec4 line = outline;
    if (tone.z > 0.0) {
        vec3 body = toned_glass(seen, behind);
        inside = mix(body, min(body * tone.z, vec3(1.0)), clamp(edge, 0.0, 1.0));
        line = vec4(toned_outline(body), 1.0);
    } else {
        vec4 light = rim_overlay(luminance(seen));
        inside = mix(seen, light.rgb, light.a * edge);
    }
    float dark = clamp((angle - corner.y) / (corner.z - corner.y), 0.0, 1.0);
    float ring = (window_coverage(dist - 1.0) - window_coverage(dist)) * line.a * dark;
    return vec4(inside, 1.0) * window_coverage(dist) + vec4(line.rgb, 1.0) * ring;
}
