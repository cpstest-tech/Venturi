// Exposures as log2 gains (gamma 2.2 folded in), then an RGB-split shake
// with motion blur and mirrored edges, then chromatic aberration. Worked in
// CapCut's y-up space, where the shake is defined.
const COMP: f32 = 1.24;
const CUT: f32 = 0.6;
const STEPS: i32 = 7;
const MAX_SUB: f32 = 4.0;

fn mirror1(x: f32) -> f32 {
    let m = x + 1.0 - 2.0 * floor((x + 1.0) * 0.5);
    return abs(m - 1.0);
}

fn base(uv: vec2<f32>) -> vec4<f32> {
    let q = vec2<f32>(mirror1(uv.x), 1.0 - mirror1(uv.y));
    var c = src(q);
    if (fx_progress() * COMP >= CUT) {
        let b = dst(q);
        c = b + c * (1.0 - b.a);
    }
    let g0 = exp2(curve(0u));
    let g1 = exp2(curve(1u));
    let rgb = min(min(c.rgb * g0, vec3<f32>(c.a)) * g1, vec3<f32>(c.a));
    return vec4<f32>(rgb, c.a);
}

fn wave(t: f32) -> vec3<f32> {
    let x = 3.14159265 * t * 16.0;
    let wx = (cos(x) * 0.5 + sin(x * 1.5) * 0.32 + cos(x * 2.5) * 0.18) * 0.5;
    let wy = (sin(x) * 0.5 + cos(x * 1.5) * 0.32 + sin(x * 2.5) * 0.18) * 0.5;
    let wz = sin(x) * 0.5 + sin(x * 1.5) * 0.32 + cos(x * 2.5) * 0.18;
    return vec3<f32>(wx * 192.0, wy * 192.0, wz * 100.0);
}

// Where output pixel `p` (design px) samples the input for a shake offset `d`.
fn shaken(p: vec2<f32>, d: vec3<f32>, design: vec2<f32>) -> vec2<f32> {
    let k = max(1080.0 + d.z, 108.0) / 1080.0;
    let c = design * 0.5;
    return ((p - c + d.xy) * k + c) / design;
}

fn effect(screen: vec2<f32>) -> vec4<f32> {
    let uv = vec2<f32>(screen.x, 1.0 - screen.y);
    let design = fx_size() * (1080.0 / min(fx_size().x, fx_size().y));
    let ratio = vec3<f32>(1.44, 1.7, 1.76);
    let amp = curve(2u);
    let ca = vec2<f32>(curve(3u), curve(4u)) * (1.0 - uv) * uv;
    let uvs = array<vec2<f32>, 3>(uv + ca, uv, uv - ca);
    if (amp <= 0.0) {
        return vec4<f32>(base(uvs[0]).r, base(uvs[1]).g, base(uvs[2]).b, base(uv).a);
    }
    let t = fx_progress() * COMP;
    var waves: array<vec3<f32>, 8>;
    for (var i = 0; i <= STEPS; i = i + 1) {
        waves[i] = wave(t + (f32(i) / f32(STEPS) - 0.5) / 30.0) * amp;
    }
    // Like CapCut, one sample per 5.5 design px of path (capped).
    var longest = 0.0;
    for (var i = 0; i < STEPS; i = i + 1) {
        longest = max(longest, length((waves[i + 1] - waves[i]).xy) * ratio.z);
    }
    let sub = i32(clamp(ceil(longest / 5.5), 1.0, MAX_SUB));
    var out = vec4<f32>(0.0);
    for (var ch = 0; ch < 3; ch = ch + 1) {
        let p = uvs[ch] * design;
        var acc = vec4<f32>(0.0);
        for (var i = 0; i < STEPS; i = i + 1) {
            // CapCut ends every channel's segment on the blue path.
            let a = shaken(p, waves[i] * ratio[ch], design);
            let b = shaken(p, waves[i + 1] * ratio.z, design);
            for (var j = 0; j < sub; j = j + 1) {
                acc = acc + base(mix(a, b, f32(j) / f32(sub)));
            }
        }
        acc = acc / f32(STEPS * sub);
        out[ch] = acc[ch];
        out.a = max(out.a, acc.a);
    }
    return out;
}
