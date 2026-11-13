fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = fx_progress();
    let ua = vec2<f32>(uv.x + p, uv.y);
    let ub = vec2<f32>(uv.x - (1.0 - p), uv.y);
    if (ua.x <= 1.0) { return src(ua); }
    return dst(ub);
}
