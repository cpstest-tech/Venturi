fn effect(uv: vec2<f32>) -> vec4<f32> {
    let d = (uv - vec2<f32>(0.5)) * param(0u) * 2.0;
    let r = src(uv + d);
    let g = src(uv);
    let b = src(uv - d);
    return vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
}
