fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = param(0u);
    var x = uv.x;
    if (x > c) { x = 2.0 * c - x; }
    if (x < 0.0) { return vec4<f32>(0.0); }
    return src(vec2<f32>(x, uv.y));
}
