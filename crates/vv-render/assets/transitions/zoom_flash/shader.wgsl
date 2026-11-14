// A zooms in (curve 0) until the cut at half way, then B settles from 120%
// (curve 1); over both, an exposure flash (2, log2 gain with gamma folded
// in) and CapCut's iterated box blur as its equivalent gaussian (3, fitted
// to CapCut's output).

fn layer(p: vec2<f32>) -> vec4<f32> {
    var c: vec4<f32>;
    if (fx_progress() < 0.5) {
        c = src((p - 0.5) / curve(0u) + 0.5);
    } else {
        c = dst((p - 0.5) / curve(1u) + 0.5);
    }
    return vec4<f32>(min(c.rgb * exp2(curve(2u)), vec3<f32>(c.a)), c.a);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let sigma = curve(3u) * min(fx_size().x, fx_size().y) / fx_size();
    let n = i32(clamp(ceil(2.0 * sigma.y * fx_size().y / 1.5), 1.0, 32.0));
    if (n == 1) { return layer(uv); }
    var acc = vec4<f32>(0.0);
    var weight = 0.0;
    for (var i = 0; i < n; i = i + 1) {
        let r = sqrt((f32(i) + 0.5) / f32(n));
        let a = f32(i) * 2.3999632;
        let w = exp(-2.0 * r * r);
        let p = clamp(uv + vec2<f32>(cos(a), sin(a)) * r * 2.0 * sigma, vec2<f32>(0.0), vec2<f32>(1.0));
        acc = acc + layer(p) * w;
        weight = weight + w;
    }
    return acc / weight;
}
