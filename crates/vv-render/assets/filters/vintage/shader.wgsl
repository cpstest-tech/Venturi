fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = straight(src(uv));
    var c = s;
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let sepia = vec3<f32>(l * 1.08, l * 0.94, l * 0.76);
    let faded = mix(vec3<f32>(0.08, 0.06, 0.05), vec3<f32>(0.98, 0.95, 0.88), sepia);
    c = vec4<f32>(mix(s.rgb, faded, param(0u)), s.a);
    return premul(c);
}
