fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = fx_progress();
    let c = vec2<f32>(0.5);
    let ka = 1.0 + p * param(0u);
    let kb = 1.0 + (1.0 - p) * param(0u);
    var a = vec4<f32>(0.0);
    var b = vec4<f32>(0.0);
    let pa = (uv - c) / ka + c;
    let pb = (uv - c) / kb + c;
    if (pa.x >= 0.0 && pa.x <= 1.0 && pa.y >= 0.0 && pa.y <= 1.0) { a = src(pa); }
    if (pb.x >= 0.0 && pb.x <= 1.0 && pb.y >= 0.0 && pb.y <= 1.0) { b = dst(pb); }
    let m = smoothstep(0.25, 0.75, p);
    return mix(a, b, m);
}
