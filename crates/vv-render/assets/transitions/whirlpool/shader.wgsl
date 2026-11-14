// Both clips shrunk to 70% with mirrored edges, warped by a rotating-axis
// fold (curves 0-1) and a twirl (2), blurred by a rotation (3, degrees of
// arc) and a small gaussian (4), then zoomed back. Worked in CapCut's
// y-up space, where its angles are defined.
const PI: f32 = 3.1415926;

fn mirror2(p: vec2<f32>) -> vec2<f32> {
    return abs(p + 1.0 - 2.0 * floor((p + 1.0) * 0.5) - 1.0);
}

fn clip(g: vec2<f32>) -> vec4<f32> {
    let p = mirror2((g - 0.5) / 0.7 + 0.5);
    let uv = vec2<f32>(p.x, 1.0 - p.y);
    if (fx_progress() < 0.475) { return src(uv); }
    return dst(uv);
}

fn fold(g: vec2<f32>) -> vec2<f32> {
    let uv = g - 0.5;
    let axis = curve(1u);
    let k1 = tan(axis + PI * 0.5);
    let k2 = tan(axis);
    let x = (uv.y - k2 * uv.x) / (k1 - k2);
    let at = vec2<f32>(x, x * k1);
    let d = uv - at;
    let a = -curve(0u);
    let r = vec2<f32>(cos(a) * d.x + sin(a) * d.y, -sin(a) * d.x + cos(a) * d.y);
    return mirror2(r + at + 0.5);
}

fn twirl(g: vec2<f32>, ratio: f32, radius: f32) -> vec2<f32> {
    var tc = g - 0.5;
    if (ratio < 1.0) { tc.x = tc.x * ratio; } else { tc.y = tc.y / ratio; }
    let dist = length(tc);
    if (dist < radius) {
        let theta = (1.0 - smoothstep(0.0, 1.0, dist / radius)) * radians(curve(2u));
        let s = sin(theta);
        let c = cos(theta);
        tc = vec2<f32>(c * tc.x - s * tc.y, s * tc.x + c * tc.y);
    }
    if (ratio < 1.0) { tc.x = tc.x / ratio; } else { tc.y = tc.y * ratio; }
    return tc + 0.5;
}

fn spin(g: vec2<f32>, degrees: f32, ratio: f32) -> vec2<f32> {
    let t = radians(degrees);
    var p = g - 0.5;
    p.y = p.y / ratio;
    p = vec2<f32>(cos(t) * p.x - sin(t) * p.y, sin(t) * p.x + cos(t) * p.y);
    p.y = p.y * ratio;
    return clamp(p + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
}

fn chain(g: vec2<f32>, degrees: f32, ratio: f32, radius: f32) -> vec4<f32> {
    let t = twirl(spin(g, degrees, ratio), ratio, radius);
    if (t.x < 0.0 || t.x > 1.0 || t.y < 0.0 || t.y > 1.0) { return vec4<f32>(0.0); }
    return clip(fold(t));
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let ratio = fx_size().x / fx_size().y;
    let radius = 0.42 * mix(1.0, 1.25, smoothstep(0.0, 0.45, 16.0 / 9.0 / max(ratio, 1.0 / ratio) - 1.0));
    let g = (vec2<f32>(uv.x, 1.0 - uv.y) - 0.5) * 0.7 + 0.5;
    let arc = curve(3u);
    let sigma = curve(4u) * min(fx_size().x, fx_size().y) / fx_size();
    let arc_px = radians(abs(arc)) * length((g - 0.5) * fx_size());
    let n = i32(clamp(max(ceil(arc_px / 1.5), select(1.0, 16.0, sigma.y * fx_size().y > 0.5)), 1.0, 24.0));
    if (n == 1) { return chain(g, 0.0, ratio, radius); }
    var acc = vec4<f32>(0.0);
    var weight = 0.0;
    for (var i = 0; i < n; i = i + 1) {
        let r = sqrt((f32(i) + 0.5) / f32(n));
        let a = f32(i) * 2.3999632;
        let w = exp(-2.0 * r * r);
        let o = vec2<f32>(cos(a), sin(a)) * r * 2.0 * sigma;
        acc = acc + chain(g + o, arc * fract(f32(i) * 0.618034), ratio, radius) * w;
        weight = weight + w;
    }
    return acc / weight;
}
