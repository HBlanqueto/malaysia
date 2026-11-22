#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform float shell;
uniform float saturation;
uniform vec4 veil;
uniform vec4 limits;
uniform vec4 levels_a;
uniform vec4 levels_b;
uniform vec4 levels_c;
uniform vec4 tone_a;
uniform vec4 tone_b;
uniform vec4 tone_c;
uniform vec4 knots_a;
uniform float knots_b;
uniform vec4 chroma_a;
uniform float chroma_b;
uniform vec4 face;
uniform vec4 fill;
uniform vec4 delta_a;
uniform vec4 delta_b;
uniform float delta_c;
uniform float rim_on;
uniform vec4 angle1_a;
uniform vec4 angle1_b;
uniform vec4 angle1_c;
uniform vec4 angle2_a;
uniform vec4 angle2_b;
uniform vec4 angle2_c;
uniform vec4 dist1_a;
uniform vec4 dist1_b;
uniform vec4 dist1_c;
uniform vec4 dist1_d;
uniform vec4 dist1_e;
uniform vec4 dist1_f;
uniform vec4 dist1_g;
uniform vec4 dist1_h;
uniform vec4 dist1_i;
uniform vec4 dist2_a;
uniform vec4 dist2_b;
uniform vec4 dist2_c;
uniform vec4 dist2_d;
uniform vec4 dist2_e;
uniform vec4 dist2_f;
uniform vec4 dist2_g;
uniform vec4 dist2_h;
uniform vec4 dist2_i;

float component(vec4 values, int index) {
    if (index == 0) {
        return values.x;
    }
    if (index == 1) {
        return values.y;
    }
    if (index == 2) {
        return values.z;
    }
    return values.w;
}

float twelve(vec4 a, vec4 b, vec4 c, int index) {
    if (index < 4) {
        return component(a, index);
    }
    if (index < 8) {
        return component(b, index - 4);
    }
    return component(c, index - 8);
}

float five(vec4 a, float b, int index) {
    if (index < 4) {
        return component(a, index);
    }
    return b;
}

float tone_at(float luma) {
    if (luma <= twelve(levels_a, levels_b, levels_c, 0)) {
        return twelve(tone_a, tone_b, tone_c, 0);
    }
    for (int i = 1; i < 12; i++) {
        float high = twelve(levels_a, levels_b, levels_c, i);
        if (luma <= high) {
            float low = twelve(levels_a, levels_b, levels_c, i - 1);
            return mix(twelve(tone_a, tone_b, tone_c, i - 1), twelve(tone_a, tone_b, tone_c, i), (luma - low) / (high - low));
        }
    }
    return twelve(tone_a, tone_b, tone_c, 11);
}

float chroma_at(float luma) {
    if (luma <= five(knots_a, knots_b, 0)) {
        return five(chroma_a, chroma_b, 0);
    }
    for (int i = 1; i < 5; i++) {
        float high = five(knots_a, knots_b, i);
        if (luma <= high) {
            float low = five(knots_a, knots_b, i - 1);
            return mix(five(chroma_a, chroma_b, i - 1), five(chroma_a, chroma_b, i), (luma - low) / (high - low));
        }
    }
    return five(chroma_a, chroma_b, 4);
}

float delta_value(int index) {
    if (index < 4) {
        return component(delta_a, index);
    }
    if (index < 8) {
        return component(delta_b, index - 4);
    }
    return delta_c;
}

float delta_at(float luma) {
    float at = clamp(luma * 8.0, 0.0, 7.999);
    float below = 0.0;
    float above = 0.0;
    for (int i = 0; i < 8; i++) {
        if (float(i) <= at && at < float(i + 1)) {
            below = delta_value(i);
            above = delta_value(i + 1);
        }
    }
    return mix(below, above, fract(at)) / 255.0;
}

vec3 glass_face(vec3 backdrop, float luma) {
    vec3 saturated = clamp(vec3(luma) + face.z * (backdrop - luma), 0.0, 1.0);
    vec3 tone = vec3(face.y) + (face.x - face.y) * saturated * (1.0 - (1.0 - face.w) * luma);
    return mix(tone, fill.rgb, fill.a) + delta_at(luma);
}

float rim_center(int bin) {
    if (bin < 24) {
        return -8.0 + 0.5 * float(bin) + 0.25;
    }
    if (bin == 24) {
        return 5.0;
    }
    return 7.0 + 2.0 * float(bin - 25);
}

float rim_profile(int index, vec4 a, vec4 b, vec4 c, vec4 d, vec4 e, vec4 f, vec4 g, vec4 h, vec4 i) {
    if (index < 4) {
        return component(a, index);
    }
    if (index < 8) {
        return component(b, index - 4);
    }
    if (index < 12) {
        return component(c, index - 8);
    }
    if (index < 16) {
        return component(d, index - 12);
    }
    if (index < 20) {
        return component(e, index - 16);
    }
    if (index < 24) {
        return component(f, index - 20);
    }
    if (index < 28) {
        return component(g, index - 24);
    }
    if (index < 32) {
        return component(h, index - 28);
    }
    return component(i, index - 32);
}

float rim_angle(vec4 a, vec4 b, vec4 c, float at) {
    int below = int(floor(at));
    int above = below < 11 ? below + 1 : 11;
    return mix(twelve(a, b, c, below), twelve(a, b, c, above), at - float(below));
}

float rim_levels(float depth, float angle) {
    if (depth < -8.0 || depth > 26.0) {
        return 0.0;
    }
    float folded = 90.0 - abs(mod(angle, 180.0) - 90.0);
    float angle_at = clamp(folded / 7.5 - 0.5, 0.0, 11.0);
    int at = 33;
    for (int i = 0; i < 34; i++) {
        if (depth < rim_center(i + 1)) {
            at = i;
            break;
        }
    }
    float t = clamp((depth - rim_center(at)) / (rim_center(at + 1) - rim_center(at)), 0.0, 1.0);
    float first = mix(
        rim_profile(at, dist1_a, dist1_b, dist1_c, dist1_d, dist1_e, dist1_f, dist1_g, dist1_h, dist1_i),
        rim_profile(at + 1, dist1_a, dist1_b, dist1_c, dist1_d, dist1_e, dist1_f, dist1_g, dist1_h, dist1_i),
        t);
    float second = mix(
        rim_profile(at, dist2_a, dist2_b, dist2_c, dist2_d, dist2_e, dist2_f, dist2_g, dist2_h, dist2_i),
        rim_profile(at + 1, dist2_a, dist2_b, dist2_c, dist2_d, dist2_e, dist2_f, dist2_g, dist2_h, dist2_i),
        t);
    return rim_angle(angle1_a, angle1_b, angle1_c, angle_at) * first + rim_angle(angle2_a, angle2_b, angle2_c, angle_at) * second;
}

vec3 material(vec3 backdrop, float luma) {
    vec3 saturated = clamp(vec3(luma) + saturation * (backdrop - luma), 0.0, 1.0);
    vec3 veiled = saturated + (veil.rgb - saturated) * veil.a;
    return limits.z * max(min(veiled, vec3(limits.x)), vec3(limits.y)) + limits.w;
}

vec3 shell_glass(vec3 backdrop, float luma) {
    return vec3(tone_at(luma)) + chroma_at(luma) * (backdrop - luma);
}

void main() {
    vec3 backdrop = texture2D(tex, v_coords).rgb;
    float luma = dot(backdrop, vec3(0.2126, 0.7152, 0.0722));
    vec3 shown = shell > 1.5 ? glass_face(backdrop, luma) : shell > 0.5 ? shell_glass(backdrop, luma) : material(backdrop, luma);
    if (rim_on > 0.5) {
        vec2 at = v_coords * glass_size;
        vec2 normal;
        float depth = glass_distance(at, normal) / glass_unit;
        shown += rim_levels(depth, degrees(atan(-normal.y, normal.x))) / 255.0;
    }
    gl_FragColor = vec4(clamp(shown, 0.0, 1.0), 1.0) * alpha;
}
