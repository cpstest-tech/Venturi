fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = straight(src(uv));
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let teal = vec3<f32>(0.0, 0.18, 0.22);
    let orange = vec3<f32>(0.25, 0.12, -0.05);
    let rgb = c.rgb + mix(teal, orange, smoothstep(0.2, 0.8, l)) * param(0u);
    return premul(vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a));
}
