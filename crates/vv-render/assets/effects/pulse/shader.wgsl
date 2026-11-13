fn effect(uv: vec2<f32>) -> vec4<f32> {
    let k = 1.0 + param(0u) * (0.5 + 0.5 * sin(fx_time() * param(1u) * 6.2831853));
    let p = (uv - vec2<f32>(0.5)) / k + vec2<f32>(0.5);
    if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) { return vec4<f32>(0.0); }
    return src(p);
}
