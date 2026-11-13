fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = fx_progress();
    let blocks = max(fx_size().x / (1.0 + sin(p * 3.1415927) * param(0u)), 2.0);
    let cell = vec2<f32>(blocks, blocks * fx_size().y / fx_size().x);
    let q = (floor(uv * cell) + vec2<f32>(0.5)) / cell;
    return mix(src(q), dst(q), smoothstep(0.35, 0.65, p));
}
