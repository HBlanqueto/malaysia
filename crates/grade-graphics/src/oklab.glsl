// Same space and steps as ctappearance::backdrop::take_chroma: the base keeps its lightness.
vec3 linear_from_srgb(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
}

vec3 srgb_from_linear(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}

vec3 oklab_from_linear(vec3 c) {
    float l = pow(max(0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b, 0.0), 1.0 / 3.0);
    float m = pow(max(0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b, 0.0), 1.0 / 3.0);
    float s = pow(max(0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b, 0.0), 1.0 / 3.0);
    return vec3(
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s
    );
}

vec3 linear_from_oklab(vec3 lab) {
    float l = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    float m = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    float s = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    l = l * l * l;
    m = m * m * m;
    s = s * s * s;
    return vec3(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
    );
}

// Same saturation as ctappearance::backdrop::take_chroma_capped; `lightness` is how much of the source's lightness the base takes.
vec3 take_tint(vec3 base, vec3 source, float amount, float lightness, float cap) {
    vec3 p = oklab_from_linear(linear_from_srgb(base));
    vec3 q = oklab_from_linear(linear_from_srgb(source));
    vec2 ab = mix(p.yz, q.yz, clamp(amount, 0.0, 1.0));
    ab /= sqrt(1.0 + dot(ab, ab) / (cap * cap));
    float l = mix(p.x, q.x, clamp(lightness, 0.0, 1.0));
    return clamp(srgb_from_linear(clamp(linear_from_oklab(vec3(l, ab)), 0.0, 1.0)), 0.0, 1.0);
}

vec3 take_chroma_capped(vec3 base, vec3 source, float amount, float cap) {
    return take_tint(base, source, amount, 0.0, cap);
}

vec3 take_chroma(vec3 base, vec3 source, float amount) {
    vec3 p = oklab_from_linear(linear_from_srgb(base));
    vec3 q = oklab_from_linear(linear_from_srgb(source));
    vec3 lab = vec3(p.x, mix(p.yz, q.yz, clamp(amount, 0.0, 1.0)));
    return clamp(srgb_from_linear(clamp(linear_from_oklab(lab), 0.0, 1.0)), 0.0, 1.0);
}
