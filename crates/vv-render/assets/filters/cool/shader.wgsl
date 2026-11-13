fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = straight(src(uv));
    let a = param(0u) * 0.18;
    return premul(vec4<f32>(clamp(c.rgb + vec3<f32>(-a, a * 0.1, a), vec3<f32>(0.0), vec3<f32>(1.0)), c.a));
}
