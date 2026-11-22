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
    gl_FragColor = vec4(clamp(shown, 0.0, 1.0), 1.0) * alpha;
}
