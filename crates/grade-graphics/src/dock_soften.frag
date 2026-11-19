#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 texel;
uniform vec2 direction;
uniform float spread;

const float SIGMA = 3.0;
const float REACH = 3.0;
const int MOST_PAIRS = 18;

vec3 at(vec2 coords) {
    return texture2D(tex, clamp(coords, 0.5 * texel, 1.0 - 0.5 * texel)).rgb;
}

float gaussian(float x, float sigma) {
    return exp(-0.5 * x * x / (sigma * sigma));
}

void main() {
    float sigma = SIGMA * spread;
    float reach = ceil(REACH * sigma);
    vec2 pixel = direction * texel;
    vec3 sum = at(v_coords);
    float weights = 1.0;
    for (int i = 1; i <= MOST_PAIRS; i++) {
        float near = float(2 * i - 1);
        if (near > reach) {
            break;
        }
        float far = near + 1.0;
        float near_weight = gaussian(near, sigma);
        float far_weight = far > reach ? 0.0 : gaussian(far, sigma);
        float weight = near_weight + far_weight;
        float offset = (near * near_weight + far * far_weight) / weight;
        sum += weight * (at(v_coords + offset * pixel) + at(v_coords - offset * pixel));
        weights += 2.0 * weight;
    }
    gl_FragColor = vec4(sum / weights, 1.0) * alpha;
}
