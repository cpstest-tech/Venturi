// A is cut along the diagonal and its halves slide apart to the corners
// (curves 1-2), each casting a soft drop shadow, revealing B zooming in
// from 40% with mirrored edges (curve 0). A straight blurred edge is the
// normal CDF of the distance to it, so the shadow blur needs no taps.

fn mirror2(p: vec2<f32>) -> vec2<f32> {
    return abs(p + 1.0 - 2.0 * floor((p + 1.0) * 0.5) - 1.0);
}

fn cdf(x: f32) -> f32 {
    return 1.0 / (1.0 + exp(-1.702 * x));
}

// `side` +1 keeps the lower-right half, -1 the upper-left one. `toward` is
// the shadow's sampling offset, `opacity` its strength.
fn half(q: vec2<f32>, side: f32, toward: vec2<f32>, opacity: f32) -> vec4<f32> {
    let px = fx_size();
    let design = 1080.0 / min(px.x, px.y);
    let norm = length(1.0 / px) / design;
    // CapCut's wipe keeps a 0.0028 overlap past the diagonal.
    let d = side * (q.x + q.y - 1.0) + 0.0028;
    var layer = vec4<f32>(0.0);
    if (d >= 0.0 && all(q >= vec2<f32>(0.0)) && all(q <= vec2<f32>(1.0))) {
        layer = src(q);
    }
    let s = q + toward;
    let ds = side * (s.x + s.y - 1.0) + 0.0028;
    let shadow = cdf(ds / norm / 11.43) * opacity;
    return layer + vec4<f32>(0.0, 0.0, 0.0, shadow) * (1.0 - layer.a);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let m = min(fx_size().x, fx_size().y);
    let push = 5.0 * m * m / (1080.0 * 1080.0 * fx_size().x) * 0.70710678;
    let b = dst(mirror2((uv - 0.5) / curve(0u) + 0.5));
    let lower = half(uv - vec2<f32>(curve(1u)), 1.0, vec2<f32>(push, push), 1.0);
    let upper = half(uv - vec2<f32>(curve(2u)), -1.0, vec2<f32>(-push, -push), curve(3u));
    let below = lower + b * (1.0 - lower.a);
    return upper + below * (1.0 - upper.a);
}
