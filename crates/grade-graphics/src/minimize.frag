#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform float turn;
uniform float flip;
uniform vec2 rows;
uniform float cut;
uniform vec2 wide;
uniform vec2 mouth;
uniform vec2 taper;

const float PI = 3.14159265;
const float TINY = 1e-4;

vec2 to_lane(vec2 point) {
    vec2 turned = mix(point, point.yx, turn);
    return vec2(turned.x, mix(turned.y, 1.0 - turned.y, flip));
}

vec2 to_upright(vec2 point) {
    vec2 unflipped = vec2(point.x, mix(point.y, 1.0 - point.y, flip));
    return mix(unflipped, unflipped.yx, turn);
}

float funnel_at(float depth) {
    float t = clamp((depth - taper.x) / max(taper.y - taper.x, TINY), 0.0, 1.0);
    return 0.5 - 0.5 * cos(PI * t);
}

float inside(float fraction) {
    return step(0.0, fraction) * step(fraction, 1.0);
}

void main() {
    vec2 here = to_lane(v_coords);
    vec2 sides = mix(wide, mouth, funnel_at(here.y));
    vec2 cell = vec2(
        (here.x - sides.x) / max(sides.y - sides.x, TINY),
        (here.y - rows.x) / max(rows.y - rows.x, TINY)
    );
    float shown = inside(cell.x) * inside(cell.y) * step(here.y, cut);
    gl_FragColor = texture2D(tex, to_upright(clamp(cell, 0.0, 1.0))) * (alpha * shown);
}
