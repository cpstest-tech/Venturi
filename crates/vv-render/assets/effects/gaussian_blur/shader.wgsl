fn effect(uv: vec2<f32>) -> vec4<f32> {
    let radius = param(0u);
    if (radius <= 0.0) { return src(uv); }

    let sigma = max(radius / 3.0, 0.001);
    let scale = sigma / fx_size();
    var color = vec4<f32>(0.0);
    var total = 0.0;
    for (var y = -3; y <= 3; y = y + 1) {
        for (var x = -3; x <= 3; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y));
            let weight = exp(-0.5 * dot(offset, offset));
            let point = clamp(uv + offset * scale, vec2<f32>(0.0), vec2<f32>(1.0));
            color = color + src(point) * weight;
            total = total + weight;
        }
    }
    return color / total;
}
