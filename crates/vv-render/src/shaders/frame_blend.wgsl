// Motion blur: weighted sum of frames, and the difference between two frames.
// Bindings are unique across entry points: each pipeline derives its layout.

struct Blend {
    size: vec2<u32>,
    weight: f32,
    gamma: f32,
    clear: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> sum: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> blend: Blend;

// Premultiplied colour: `gamma` on rgb only, alpha adds up linearly.
fn to_blend_space(c: vec4<f32>, gamma: f32) -> vec4<f32> {
    return vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(gamma)), c.a);
}

@compute @workgroup_size(8, 8)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= blend.size) {
        return;
    }
    let i = id.y * blend.size.x + id.x;
    let c = to_blend_space(textureLoad(frame, vec2<i32>(id.xy), 0), blend.gamma) * blend.weight;
    if blend.clear != 0u {
        sum[i] = c;
    } else {
        sum[i] = sum[i] + c;
    }
}

@group(0) @binding(3) var<storage, read> resolved: array<vec4<f32>>;
@group(0) @binding(4) var<uniform> resolve_params: Blend;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_resolve(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<u32>(position.xy);
    let c = resolved[p.y * resolve_params.size.x + p.x];
    return to_blend_space(c, 1.0 / resolve_params.gamma);
}

struct Difference {
    // Sampled grid, one sample every `stride` pixels.
    grid: vec2<u32>,
    stride: u32,
    _pad: u32,
}

@group(0) @binding(5) var diff_a: texture_2d<f32>;
@group(0) @binding(6) var diff_b: texture_2d<f32>;
@group(0) @binding(7) var<storage, read_write> diff_total: atomic<u32>;
@group(0) @binding(8) var<uniform> difference_params: Difference;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
// One atomic per workgroup, in fixed point: 256 samples × 1024 per unit
// leave room for thousands of workgroups in a u32.
const FIXED_ONE: f32 = 1024.0;

var<workgroup> partial: array<f32, 256>;

@compute @workgroup_size(16, 16)
fn difference(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    var d = 0.0;
    if all(id.xy < difference_params.grid) {
        let p = vec2<i32>(id.xy * difference_params.stride);
        let a = dot(textureLoad(diff_a, p, 0).rgb, LUMA);
        let b = dot(textureLoad(diff_b, p, 0).rgb, LUMA);
        d = abs(a - b);
    }
    partial[local] = d;
    workgroupBarrier();
    for (var step = 128u; step > 0u; step = step >> 1u) {
        if local < step {
            partial[local] = partial[local] + partial[local + step];
        }
        workgroupBarrier();
    }
    if local == 0u {
        atomicAdd(&diff_total, u32(round(partial[0] * FIXED_ONE)));
    }
}
