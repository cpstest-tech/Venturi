fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = straight(src(uv));
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let sat = 1.0 + param(0u) * 0.9;
    var rgb = mix(vec3<f32>(l), c.rgb, sat);
    rgb = (rgb - vec3<f32>(0.5)) * (1.0 + param(0u) * 0.15) + vec3<f32>(0.5);
    return premul(vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a));
}
