// Curves: twist angle, twist amount, scale, distortion, direction blur, zoom blur.
fn native_size() -> vec2<f32> {
    return select(vec2<f32>(1.0), vec2<f32>(param(2), param(3)), param(2) > 0.0);
}

fn native_uv(uv: vec2<f32>) -> vec2<f32> {
    let center = select(vec2<f32>(0.5), vec2<f32>(param(0), param(1)), param(2) > 0.0);
    let p = (uv - center) * fx_size();
    let r = param(4);
    return vec2<f32>(cos(r) * p.x + sin(r) * p.y, -sin(r) * p.x + cos(r) * p.y) / (native_size() * fx_size()) + 0.5;
}

fn frame_uv(uv: vec2<f32>) -> vec2<f32> {
    let center = select(vec2<f32>(0.5), vec2<f32>(param(0), param(1)), param(2) > 0.0);
    let p = (uv - 0.5) * native_size() * fx_size();
    let r = param(4);
    return vec2<f32>(cos(r) * p.x - sin(r) * p.y, sin(r) * p.x + cos(r) * p.y) / fx_size() + center;
}

fn mirror_uv(p: vec2<f32>) -> vec2<f32> {
    return 1.0 - abs(1.0 - (p - 2.0 * floor(p / 2.0)));
}

fn random2(p: vec2<f32>) -> vec2<f32> {
    return -1.0 + 2.0 * fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(dot(random2(cell), f), dot(random2(cell + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0)), u.x),
        mix(dot(random2(cell + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0)), dot(random2(cell + 1.0), f - 1.0), u.x), u.y);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let local = native_uv(uv);
    if any(local < vec2<f32>(0.0)) || any(local > vec2<f32>(1.0)) { return vec4<f32>(0.0); }
    let g = local;
    if curve(2) < 0.001 { return vec4<f32>(0.0); }
    let scale = curve(2);
    let frag = (g * vec2<f32>((fx_size().x * native_size().x) / (fx_size().y * native_size().y), 1.0) - 0.5) / scale + 0.5;
    let xy = vec2<f32>(noise(frag * 0.3 + vec2<f32>(47.8, 59.2)), noise((1.0 - frag) * 0.3 + vec2<f32>(47.8, 59.2)));
    let angle = curve(0);
    let twisted = vec2<f32>(cos(angle) * xy.x + sin(angle) * xy.y, -sin(angle) * xy.x + cos(angle) * xy.y);
    let base = (g - 0.5) / scale + 0.5;
    let warped = base - curve(1) * twisted;
    var p = warped;
    p.x = 1.0 - p.x;
    p += vec2<f32>(0.12, -0.12);
    p /= sqrt(1.0 + dot(p, p) * curve(3) * 6.0);
    p -= vec2<f32>(0.12, -0.12);
    p.x = 1.0 - p.x;
    var color = vec4<f32>(0.0);
    var total = 0.0;
    let count = select(1u, 24u, curve(4) + curve(5) > 0.00001);
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        var q = p;
        var weight = 1.0;
        if curve(5) > 0.00001 {
            q = warped + (0.5 - base) * t * curve(5);
            weight = 4.0 * t * (1.0 - t);
        } else if curve(4) > 0.00001 {
            q += normalize(vec2<f32>(-1.0, 2.0)) * (t * 50.0 - 25.0) * curve(4) / fx_size();
        }
        q = mirror_uv(q);
        color += src(frame_uv(q)) * weight;
        total += weight;
    }
    return color / total;
}
