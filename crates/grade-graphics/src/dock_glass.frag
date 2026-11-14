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

const int SOFT_TAPS = 3;
const float SOFT_SPACING = 2.0;
const float RIM_SOFTNESS = 0.05;

// Bilinear taps no more than two pixels apart blur a hard edge instead of echoing it.
vec3 soft(vec2 px) {
    vec2 low = geometry.xy;
    vec2 high = geometry.xy + geometry.zw;
    float sigma = 0.5 * float(SOFT_TAPS);
    vec3 sum = vec3(0.0);
    float weights = 0.0;
    for (int y = -SOFT_TAPS; y <= SOFT_TAPS; y++) {
        for (int x = -SOFT_TAPS; x <= SOFT_TAPS; x++) {
            vec2 tap = vec2(float(x), float(y));
            float weight = exp(-0.5 * dot(tap, tap) / (sigma * sigma));
            sum += weight * texture2D(tex, clamp(px + tap * SOFT_SPACING, low, high) / texture_size).rgb;
            weights += weight;
        }
    }
    return sum / weights;
}

// A circle's slope is infinite at the rim; RIM_SOFTNESS flattens it there and keeps both ends, 1 at the rim and 0 at the depth.
float bend_at(float inward) {
    float x = 1.0 - inward;
    float top = sqrt(1.0 + RIM_SOFTNESS);
    return (top - sqrt(1.0 - x * x + RIM_SOFTNESS)) / (top - sqrt(RIM_SOFTNESS));
}

// The rim shows what lies deeper in, along the frame's normal. lens = (depth, strength) as shares of the Dock's thickness, ctdesign::DockSurface.
vec3 refracted(vec2 px) {
    float thickness = min(geometry.z, geometry.w);
    float inward = clamp(-frame_sdf(px, geometry, radius, exponent) / (lens.x * thickness), 0.0, 1.0);
    return soft(px - frame_normal(px, geometry, radius, exponent) * bend_at(inward) * lens.y * thickness);
}

void main() {
    vec2 px = v_coords * texture_size;
    gl_FragColor = dock_frame(px, geometry, radius, exponent, refracted(px)) * alpha;
}
