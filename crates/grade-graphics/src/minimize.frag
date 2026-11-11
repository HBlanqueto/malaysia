// The squeeze follows Wayfire's squeezimize (MIT, Scott Moreau 2024), see NOTICE.
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

// 0 = toward the bottom, 1 = top, 2 = right, 3 = left; the boxes come in that frame.
uniform float travel;
// 0 = genie, 1 = scale, 2 = suck.
uniform float effect;
// 0 = the window, 1 = gone into the target.
uniform float progress;
// x0, y0, x1, y1 over the drawn area, in the travel frame (travel is +y).
uniform vec4 window_box;
uniform vec4 target_box;

vec2 to_travel(vec2 p) {
    if (travel > 2.5) return vec2(p.y, 1.0 - p.x);
    if (travel > 1.5) return vec2(p.y, p.x);
    if (travel > 0.5) return vec2(p.x, 1.0 - p.y);
    return p;
}

vec2 to_texture(vec2 w) {
    if (travel > 2.5) return vec2(1.0 - w.y, w.x);
    if (travel > 1.5) return vec2(w.y, w.x);
    if (travel > 0.5) return vec2(w.x, 1.0 - w.y);
    return w;
}

void main() {
    vec2 c = to_travel(v_coords);
    vec4 s = window_box;
    vec4 t = target_box;
    vec2 w;
    if (effect > 0.5 && effect < 1.5) {
        float k = smoothstep(0.0, 1.0, progress);
        vec4 r = mix(s, t, k);
        w = (c - r.xy) / max(r.zw - r.xy, vec2(1e-4));
    } else {
        bool suck = effect > 1.5;
        if (suck) {
            float middle = (t.x + t.z) * 0.5;
            t.x = middle;
            t.z = middle;
        }
        // Pinch in the first quarter, slide all along: the bottom of the window narrows first.
        float pinch = suck ? progress : pow(clamp(progress, 0.0, 0.25) * 4.0, 2.0);
        float slide = progress * progress;
        float span = max(t.w - s.y, 1e-4);
        float along = (c.y - s.y) / span;
        float narrowing = pinch / (1.0 + exp(-(along * 15.0 - 10.0)));
        float x0 = mix(s.x, t.x, narrowing);
        float x1 = mix(s.z, t.z, narrowing);
        w.x = (c.x - x0) / max(x1 - x0, 1e-4);
        w.y = (c.y - s.y - slide * span) / max(s.w - s.y, 1e-4);
        if (c.y > t.w) discard;
    }
    if (w.x < 0.0 || w.y < 0.0 || w.x > 1.0 || w.y > 1.0) discard;
    gl_FragColor = texture2D(tex, to_texture(w)) * alpha;
}
