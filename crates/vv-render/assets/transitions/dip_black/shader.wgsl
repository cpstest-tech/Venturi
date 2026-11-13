fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = fx_progress();
    if (p < 0.5) {
        let a = src(uv);
        return vec4<f32>(a.rgb * (1.0 - p * 2.0), a.a);
    }
    let b = dst(uv);
    return vec4<f32>(b.rgb * ((p - 0.5) * 2.0), b.a);
}
