fn hash(n: f32) -> f32 { return fract(sin(n * 127.1) * 43758.5453); }

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let step_t = floor(fx_time() * 12.0);
    let band = floor(uv.y * 24.0);
    let hit = step(0.75, hash(band + step_t * 7.0));
    let shift = (hash(band * 3.1 + step_t) - 0.5) * 0.12 * param(0u) * hit;
    let split = 0.012 * param(0u) * (0.4 + hit);
    let r = src(vec2<f32>(uv.x + shift + split, uv.y));
    let g = src(vec2<f32>(uv.x + shift, uv.y));
    let b = src(vec2<f32>(uv.x + shift - split, uv.y));
    return vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
}
