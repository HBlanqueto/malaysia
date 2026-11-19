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
uniform vec4 tint;

const float PUSH = 0.171;
const float PUSH_RISE = 0.36;
const float PUSH_HOLD = 0.56;
const float PULL = 0.8;
const float PULL_WIDTH = 0.055;
const float PULL_SOFT = 0.011;
const float SHADE_DEPTH = 0.45;
const float SHADE_SIDE = 0.09;
const float RING_WIDTH = 0.022;
const float RING_MIN_WIDTH = 1.0;
const float RING_DARK = 0.5;
const float RING_DARK_LUMA = 0.25;
const float RIM_LIGHT = 0.1;
const float RIM_LIGHT_DEPTH = 0.075;
const float RIM_LIGHT_FLOOR = 0.35;
const float GLOW = 0.05;
const float TINT_LIFT = 0.16;

vec4 sample_at(vec2 px) {
    vec4 color = texture2D(tex, px / texture_size);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    return color;
}

float lens_push(float inward) {
    float outer = smoothstep(0.0, PUSH_RISE, inward) * (1.0 - smoothstep(PUSH_HOLD, 1.0, inward));
    float pull = 1.0 - smoothstep(PULL_WIDTH - PULL_SOFT, PULL_WIDTH + PULL_SOFT, inward);
    return PUSH * outer - PULL * pull;
}

float line_at(float dist, float centre, float thickness) {
    float width = max(RING_WIDTH * thickness, RING_MIN_WIDTH);
    return 1.0 - smoothstep(0.5 * width, 0.5 * width + 0.5, abs(dist - centre));
}

void main() {
    vec2 px = v_coords * texture_size;
    float dist = frame_sdf(px, geometry, radius, exponent);
    vec2 normal = frame_normal(px, geometry, radius, exponent);
    float thickness = min(geometry.z, geometry.w);
    float inward = clamp(-dist / (lens.x * thickness), 0.0, 1.0);
    vec4 behind = sample_at(px - normal * lens_push(inward) * lens.y * thickness);
    vec4 body = behind * (1.0 - tint.a) + tint;
    float ramp = clamp(1.0 + dist / (SHADE_DEPTH * thickness), 0.0, 1.0);
    float bright = smoothstep(0.2, 0.35, (behind.r + behind.g + behind.b) / 3.0);
    body = vec4(body.rgb * (1.0 - bright * ramp * SHADE_SIDE * abs(normal.x)), body.a);
    body.rgb += vec3((GLOW * ramp * ramp + TINT_LIFT * tint.a) * (1.0 - bright)) * body.a;
    float light = RIM_LIGHT * (RIM_LIGHT_FLOOR + (1.0 - RIM_LIGHT_FLOOR) * normal.y * normal.y);
    body.rgb = mix(body.rgb, vec3(body.a), light * line_at(dist, -RIM_LIGHT_DEPTH * thickness, thickness));
    vec3 here = sample_at(px).rgb;
    float ground = (here.r + here.g + here.b) / 3.0;
    vec3 rim = here * (1.0 - RING_DARK + RING_DARK_LUMA * ground) * body.a;
    body.rgb = mix(body.rgb, rim, line_at(dist, -0.5, thickness));
    gl_FragColor = body * window_coverage(dist) * alpha;
}
