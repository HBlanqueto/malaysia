#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 size;
uniform vec2 direction;
uniform vec2 decay;
uniform float reach;

const int MOST_TAPS = 64;

float inside(vec2 index) {
    return index.x < 0.0 || index.y < 0.0 || index.x >= size.x || index.y >= size.y ? 0.0 : 1.0;
}

vec4 at(vec2 index) {
    return inside(index) * texture2D(tex, (index + 0.5) / size);
}

void main() {
    vec2 here = floor(v_coords * size);
    vec4 sum = at(here);
    float total = 1.0;
    float weight = 1.0;
    float ratio = decay.x;
    for (int i = 1; i <= MOST_TAPS; i++) {
        float tap = float(i);
        if (tap > reach) {
            break;
        }
        weight *= ratio;
        ratio *= decay.y;
        vec2 ahead = here + tap * direction;
        vec2 behind = here - tap * direction;
        sum += weight * (at(ahead) + at(behind));
        total += weight * (inside(ahead) + inside(behind));
    }
    gl_FragColor = sum / total * alpha;
}
