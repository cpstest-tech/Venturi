// Curves: position xy, scale xy, rotation, alpha, radial blur, blur xy,
// exposure, stretch, lens, rotation xy, depth, glow; previous TRS at 16..20,
// glow exposure at 21.
const NATIVE: bool = false;
const MIRROR: bool = false;
const EXTENDED: bool = false;

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

fn read_clip(p: vec2<f32>) -> vec4<f32> {
    if EXTENDED && (any(p < vec2<f32>(-1.0)) || any(p > vec2<f32>(2.0))) {
        return vec4<f32>(0.0);
    }
    if !MIRROR && (any(p < vec2<f32>(0.0)) || any(p > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    let q = select(p, mirror_uv(p), MIRROR);
    return src(select(q, frame_uv(q), NATIVE));
}

fn mapped(uv: vec2<f32>, w: f32) -> vec2<f32> {
    let size = fx_size() * select(vec2<f32>(1.0), native_size(), NATIVE);
    let aspect = size.x / size.y;
    var p = uv - 0.5;
    let stretch = curve(10);
    if p.x * stretch > 0.0 {
        let normalized_width = fx_size().x / length(fx_size());
        p.x /= 1.0 + 2.0 * abs(stretch * p.x) * normalized_width;
    }
    let r2 = dot(p, p);
    p /= 1.0 + curve(11) * r2;
    let position = mix(vec2<f32>(curve(16), curve(17)), vec2<f32>(curve(0), curve(1)), w);
    p -= position;
    p.x *= aspect;
    let rx = curve(12);
    let ry = curve(13);
    let camera = aspect / (2.0 * tan(0.3455752));
    let a = cos(ry) + p.x * sin(ry) / camera;
    let b = sin(rx) * sin(ry) - p.x * sin(rx) * cos(ry) / camera;
    let c = p.y * sin(ry) / camera;
    let d = cos(rx) - p.y * sin(rx) * cos(ry) / camera;
    let det = a * d - b * c;
    let q = p * (1.0 - curve(14));
    p = vec2<f32>(d * q.x - b * q.y, -c * q.x + a * q.y) / max(abs(det), 0.00001) * sign(det);
    let angle = mix(curve(20), curve(4), w);
    p = vec2<f32>(cos(angle) * p.x + sin(angle) * p.y, -sin(angle) * p.x + cos(angle) * p.y);
    p.x /= aspect;
    let scale = mix(vec2<f32>(curve(18), curve(19)), vec2<f32>(curve(2), curve(3)), w);
    return p / max(scale, vec2<f32>(0.00001)) + 0.5;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let g = select(uv, native_uv(uv), NATIVE);
    if NATIVE && (any(g < vec2<f32>(0.0)) || any(g > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    let blur = vec2<f32>(curve(7), curve(8));
    let motion = length(vec2<f32>(curve(0) - curve(16), curve(1) - curve(17)))
        + abs(curve(2) - curve(18)) + abs(curve(4) - curve(20));
    let count = select(1u, 24u, length(blur) + abs(curve(6)) + motion + curve(15) > 0.00001);
    var color = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        let radius = sqrt(t) * 2.0;
        let phi = f32(i) * 2.399963;
        let offset = vec2<f32>(cos(phi), sin(phi)) * radius * blur;
        let w = select(1.0, 0.75 + 0.5 * fract(t * 17.0), motion > 0.00001);
        var p = mapped(g + offset, w);
        p += (0.5 - p) * t * curve(6);
        let weight = select(1.0, exp(-2.0 * t), length(blur) > 0.00001);
        var tap = read_clip(p);
        tap = vec4<f32>(min(tap.rgb * exp2(curve(9)), vec3<f32>(tap.a)), tap.a);
        if curve(15) > 0.0 {
            let glow_offset = vec2<f32>(cos(phi), sin(phi)) * radius * curve(15);
            let bloom = read_clip(mapped(g + glow_offset, 1.0));
            tap = vec4<f32>(min(tap.rgb + bloom.rgb * (exp2(curve(21)) - 1.0), vec3<f32>(max(tap.a, bloom.a))), max(tap.a, bloom.a));
        }
        color += tap * weight;
        total += weight;
    }
    return color / total * clamp(curve(5), 0.0, 1.0);
}
