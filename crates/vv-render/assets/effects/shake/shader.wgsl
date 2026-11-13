fn effect(uv: vec2<f32>) -> vec4<f32> {
    let t = fx_time() * param(1u);
    let off = param(0u) * vec2<f32>(sin(t * 1.3), cos(t * 1.7));
    let p = clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0));
    return src(p);
}
