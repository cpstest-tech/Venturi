fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = straight(src(uv));
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let v = clamp((l - 0.5) * param(0u) + 0.5, 0.0, 1.0);
    return premul(vec4<f32>(vec3<f32>(v), c.a));
}
