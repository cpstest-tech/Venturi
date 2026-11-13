fn effect(uv: vec2<f32>) -> vec4<f32> {
    return mix(src(uv), dst(uv), fx_progress());
}
