fn effect(uv: vec2<f32>) -> vec4<f32> {
    let motion = 0.5 + 0.5 * sin(fx_time() * param(1u) * 6.2831853);
    let zoom = 1.0 + param(0u) * motion;
    let aspect = vec2<f32>(fx_size().x / fx_size().y, 1.0);
    let centered = (uv - vec2<f32>(0.5)) * aspect;
    let r2 = dot(centered, centered);
    let warped = centered * (1.0 - param(2u) * r2) / zoom;
    let point = warped / aspect + vec2<f32>(0.5);
    if (any(point < vec2<f32>(0.0)) || any(point > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return src(point);
}
