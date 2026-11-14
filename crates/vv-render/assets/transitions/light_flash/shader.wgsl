// A squashes vertically (curve 0) with an RGB split (1-2); B drops in
// (3-4) through a pincushion lens (7-8) with its own RGB split (5-6), over
// A. On top a bloom (9-12: CapCut stacks three glows, merged here and
// fitted to its output) and a vertical gaussian blur (13). Both blurs are
// sampled together; worked in CapCut's y-up space.
const COMP: f32 = 1.066667;

fn mirror2(p: vec2<f32>) -> vec2<f32> {
    return abs(p + 1.0 - 2.0 * floor((p + 1.0) * 0.5) - 1.0);
}

fn tex_a(g: vec2<f32>) -> vec4<f32> { return src(vec2<f32>(g.x, 1.0 - g.y)); }
fn tex_b(g: vec2<f32>) -> vec4<f32> { return dst(vec2<f32>(g.x, 1.0 - g.y)); }

fn split(c: vec4<f32>, r: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(r.r, c.g, b.b, (r.a + c.a + b.a) / 3.0);
}

fn layer_a(g: vec2<f32>, rgb: bool) -> vec4<f32> {
    let q = mirror2((g - 0.5) / vec2<f32>(1.0, curve(0u)) + 0.5);
    if (!rgb) { return tex_a(q); }
    let off = vec2<f32>(curve(1u), curve(2u)) * (1.0 - q) * q;
    return split(tex_a(q), tex_a(q + off), tex_a(q - off));
}

fn place_b(c: vec2<f32>) -> vec2<f32> {
    return mirror2((c - 0.5 - vec2<f32>(0.0, curve(3u))) / curve(4u) + 0.5);
}

fn layer_b(g: vec2<f32>, rgb: bool) -> vec4<f32> {
    let d = g - 0.5;
    let x = vec2<f32>(d.x, d.y * fx_size().y / fx_size().x);
    let r = length(x);
    let f = 1.0 / (1.0 + curve(7u) * r + curve(8u) * r * r);
    let c = d * f + 0.5;
    if (f <= 0.0 || any(c < vec2<f32>(0.0)) || any(c > vec2<f32>(1.0))) { return vec4<f32>(0.0); }
    if (!rgb) { return tex_b(place_b(c)); }
    let off = vec2<f32>(curve(5u), curve(6u)) * (1.0 - c) * c;
    return split(tex_b(place_b(c)), tex_b(place_b(c + off)), tex_b(place_b(c - off)));
}

fn base(g: vec2<f32>, rgb: bool) -> vec4<f32> {
    let t = fx_progress() * COMP;
    var a = vec4<f32>(0.0);
    if (t < 0.666667) { a = layer_a(g, rgb); }
    if (t < 0.433333) { return a; }
    let b = layer_b(g, rgb);
    return b + a * (1.0 - b.a);
}

// S_Glow's highlight mask: the part of each channel above 0.5.
fn highlight(c: vec4<f32>) -> vec4<f32> {
    let over = max(c.rgb - 0.5, vec3<f32>(0.0)) / 0.5;
    return c * ((over.r + over.g + over.b) / max(c.r + c.g + c.b, 1e-6));
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let g = vec2<f32>(uv.x, 1.0 - uv.y);
    let gain = curve(9u) + curve(10u) + curve(12u);
    let glow = vec2<f32>(15.0 / 360.0) * vec2<f32>(1.0, fx_size().x / fx_size().y);
    let dir = curve(13u) * min(fx_size().x, fx_size().y) / fx_size().y;
    var extent = 2.0 * dir * fx_size().y;
    if (gain > 0.0) { extent = max(extent, 2.0 * glow.x * fx_size().x); }
    let n = i32(clamp(ceil(extent / 2.0), 1.0, 24.0));
    if (n == 1) { return clamp(base(g, true), vec4<f32>(0.0), vec4<f32>(1.0)); }
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < n; i = i + 1) {
        let u = clamp(fract(f32(i) * 0.618034 + 0.5 / f32(n)), 0.01, 0.99);
        let p = g + vec2<f32>(0.0, dir * log(u / (1.0 - u)) / 1.702);
        var c = base(p, true);
        if (gain > 0.0) {
            let r = sqrt((f32(i) + 0.5) / f32(n));
            let a = f32(i) * 2.3999632;
            let o = vec2<f32>(cos(a), sin(a)) * r * 2.0 * glow;
            let h = base(p + o, false);
            let t = curve(11u);
            let low = pow(smoothstep(vec3<f32>(t - 0.075 * t), vec3<f32>(t + 0.115), h.rgb), vec3<f32>(1.0 / 2.2));
            let w = exp(-2.0 * r * r) * 2.31;
            c = c + (highlight(h) * (curve(9u) + curve(12u)) + vec4<f32>(low * curve(10u), 0.0)) * w;
        }
        acc = acc + clamp(c, vec4<f32>(0.0), vec4<f32>(1.0));
    }
    return acc / f32(n);
}
