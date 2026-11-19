void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec2 p = vec2(uv.x * iResolution.x / iResolution.y, uv.y);
    float t = iTime * 0.05;
    float wave = 0.0;
    for (int i = 1; i <= 4; i++) {
        float f = float(i);
        wave += sin(p.x * 1.6 * f + t * (1.0 + 0.3 * f) + sin(p.y * 2.1 * f - t * 0.7) * 0.8) / f;
    }
    float band = 0.5 + 0.5 * sin(p.y * 3.0 + wave * 0.9 + t);
    vec3 deep = vec3(0.04, 0.08, 0.20);
    vec3 mid = vec3(0.10, 0.22, 0.48);
    vec3 light = vec3(0.30, 0.50, 0.82);
    vec3 color = mix(deep, mid, smoothstep(0.0, 1.0, uv.y + 0.25 * wave));
    color = mix(color, light, pow(band, 6.0) * 0.55);
    float vignette = smoothstep(1.25, 0.35, length(uv - vec2(0.5, 0.45)));
    fragColor = vec4(color * mix(0.75, 1.0, vignette), 1.0);
}
