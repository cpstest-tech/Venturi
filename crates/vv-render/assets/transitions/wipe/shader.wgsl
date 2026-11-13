fn effect(uv: vec2<f32>) -> vec4<f32> {
    let soft = max(param(0u), 0.001);
    let edge = fx_progress() * (1.0 + soft) - soft;
    let m = smoothstep(edge, edge + soft, uv.x);
    return mix(dst(uv), src(uv), m);
}
