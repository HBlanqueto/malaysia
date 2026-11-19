const float RIM_SOFTNESS = 0.05;

float bend_at(float inward) {
    float x = 1.0 - inward;
    float top = sqrt(1.0 + RIM_SOFTNESS);
    return (top - sqrt(1.0 - x * x + RIM_SOFTNESS)) / (top - sqrt(RIM_SOFTNESS));
}
