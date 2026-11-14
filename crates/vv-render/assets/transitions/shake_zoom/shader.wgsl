// A shakes, B grows in from 78% over it; both motion-blurred over 1/25 s
// (curves 0-17: the shake at 9 instants of it), mirrored at the edges.
// CapCut's chain of vertical gaussian/box and horizontal box blurs is
// linear, so it collapses into one elliptical gaussian sampled together
// with the motion path. Its sigmas (21-23) are fitted to CapCut's output.

fn mirror2(p: vec2<f32>) -> vec2<f32> {
    let m = p - 2.0 * floor(p * 0.5);
    return select(m, 2.0 - m, m > vec2<f32>(1.0));
}

fn shake(t: f32) -> vec2<f32> {
    let x = t * 8.0;
    let i = u32(min(floor(x), 7.0));
    let a = vec2<f32>(curve(2u * i), curve(2u * i + 1u));
    let b = vec2<f32>(curve(2u * i + 2u), curve(2u * i + 3u));
    return mix(a, b, x - f32(i));
}

fn layers(uv: vec2<f32>, t: f32, short: vec2<f32>) -> vec4<f32> {
    let d = shake(t) * short;
    let a = src(mirror2(uv - d));
    let opacity = curve(20u);
    if (opacity <= 0.0) { return a; }
    let s = mix(curve(18u), curve(19u), t);
    let b = dst(mirror2((uv - d - 0.5) / s + 0.5)) * opacity;
    return b + a * (1.0 - b.a);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let short = vec2<f32>(min(fx_size().x, fx_size().y)) / fx_size();
    let sigma = vec2<f32>(curve(23u), length(vec2<f32>(curve(21u), curve(22u)))) * short;
    var lo = vec2<f32>(curve(0u), curve(1u));
    var hi = lo;
    for (var i = 1u; i < 9u; i = i + 1u) {
        let p = vec2<f32>(curve(2u * i), curve(2u * i + 1u));
        lo = min(lo, p);
        hi = max(hi, p);
    }
    let zoom = abs(curve(19u) - curve(18u)) * curve(20u) * 0.5;
    let extent = max(max(length((hi - lo) * short), zoom), 2.0 * max(sigma.x, sigma.y)) * fx_size().y;
    let n = i32(clamp(ceil(extent / 1.5), 1.0, 32.0));
    if (n == 1) { return layers(uv, 0.0, short); }
    var acc = vec4<f32>(0.0);
    var weight = 0.0;
    for (var i = 0; i < n; i = i + 1) {
        let r = sqrt((f32(i) + 0.5) / f32(n));
        let a = f32(i) * 2.3999632;
        let o = vec2<f32>(cos(a), sin(a)) * r * 2.0 * sigma;
        let w = exp(-2.0 * r * r);
        acc = acc + layers(uv + o, fract(f32(i) * 0.618034), short) * w;
        weight = weight + w;
    }
    return acc / weight;
}
