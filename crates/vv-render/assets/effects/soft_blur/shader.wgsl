fn effect(uv: vec2<f32>) -> vec4<f32> {
    let px = param(0u) / fx_size();
    var acc = src(uv) * 0.2;
    var w = 0.2;
    for (var i = 0; i < 12; i = i + 1) {
        let a = f32(i) * 2.399963;
        let r = sqrt((f32(i) + 0.5) / 12.0);
        let o = vec2<f32>(cos(a), sin(a)) * r * px;
        acc = acc + src(clamp(uv + o, vec2<f32>(0.0), vec2<f32>(1.0))) * 0.0667;
        w = w + 0.0667;
    }
    return acc / w;
}
