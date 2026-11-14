fn effect(uv: vec2<f32>) -> vec4<f32> {
    let phase = fract(fx_time() * param(1u));
    let pulse = 1.0 - smoothstep(0.0, 0.18, phase);
    let amount = pulse * param(0u);
    let source = src(uv);
    let exposed = source.rgb * exp2(amount * 1.35);
    let flashed = mix(exposed, vec3<f32>(1.0), amount * 0.22);
    return vec4<f32>(min(flashed, vec3<f32>(source.a)), source.a);
}
