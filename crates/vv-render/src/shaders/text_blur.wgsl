// One axis of the Gaussian blur of a coverage target (see `text_gpu.rs`).

struct Params {
    // Texels to step by along the axis being blurred.
    direction: vec2<f32>,
    sigma: f32,
    radius: f32,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let at = vec2<i32>(position.xy);
    let direction = vec2<i32>(params.direction);
    let radius = i32(params.radius);
    var sum = 0.0;
    var total = 0.0;
    for (var i = -radius; i <= radius; i = i + 1) {
        let weight = exp(-0.5 * f32(i * i) / (params.sigma * params.sigma));
        total += weight;
        // Outside the target there is nothing: it weighs, but adds 0.
        let p = at + direction * i;
        if (all(p >= vec2<i32>(0)) && all(p < size)) {
            sum += weight * textureLoad(source, p, 0).r;
        }
    }
    return vec4<f32>(sum / total);
}
