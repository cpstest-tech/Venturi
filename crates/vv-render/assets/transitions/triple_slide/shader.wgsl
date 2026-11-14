// Three columns of each clip slide vertically with motion blur, the outer
// ones against the middle one, under a shared zoom. Offsets are the
// anchor-minus-position of each column (previous and current AE frame).
const FLIP: f32 = -1.0;

fn hash(a: f32, b: f32) -> f32 {
    var q = fract(vec2<f32>(a, b) * 13.517);
    q = q + dot(q, q.yx + vec2<f32>(22.541));
    return fract((q.x + q.y) * q.y);
}

// The column at `uv`, blurred along the path its offset takes between
// the previous frame and 2.55 frames after it, as CapCut's MotionBlur2D.
fn column(uv: vec2<f32>, pre: f32, cur: f32, b: bool) -> vec4<f32> {
    let start = mix(pre, cur, 0.75) * FLIP;
    let end = mix(pre, cur, 2.55) * FLIP;
    let n = floor(12.0 + 12.0 * smoothstep(0.0, 0.2, abs(end - start)));
    var acc = vec4<f32>(0.0);
    for (var i = 0.0; i < n; i = i + 1.0) {
        let t = i / (n - 1.0) + (hash(i + uv.x, i * uv.y) - 0.5) / n;
        let y = uv.y + mix(start, end, t);
        if (y >= 0.0 && y <= 1.0) {
            let q = vec2<f32>(uv.x, y);
            if (b) { acc = acc + dst(q); } else { acc = acc + src(q); }
        }
    }
    return acc / n;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let q = (uv - 0.5 - vec2<f32>(curve(8u), curve(9u))) / curve(10u) + 0.5;
    if (q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0) { return vec4<f32>(0.0); }
    let middle = q.x >= 0.298 && q.x <= 0.702;
    var a: vec4<f32>;
    var b: vec4<f32>;
    if (middle) {
        a = column(q, curve(2u), curve(3u), false);
        b = column(q, curve(6u), curve(7u), true);
    } else {
        a = column(q, curve(0u), curve(1u), false);
        b = column(q, curve(4u), curve(5u), true);
    }
    return a + b * (1.0 - a.a);
}
