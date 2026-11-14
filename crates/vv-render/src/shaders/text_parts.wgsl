// The sprites of an animated title drawn into a coverage target, one
// instance per sprite mask: see `text_gpu.rs` and `Part`.

struct Instance {
    @builtin(vertex_index) vertex: u32,
    // The mask in frame pixels before the move: x0, y0, x1, y1.
    @location(0) quad: vec4<f32>,
    // Rows of the 2x2 matrix applied around the pivot.
    @location(1) matrix: vec4<f32>,
    // Pivot, then the shift added after the matrix.
    @location(2) pivot_shift: vec4<f32>,
    // Frame rectangle the target covers: x, y, width, height.
    @location(3) dest: vec4<f32>,
    // x: opacity. y: frame pixels per texel. z: reveal progress, < 0 without
    // one. w: reveal direction (right, left, down, up).
    @location(4) misc: vec4<f32>,
    // Ink box of the reveal: x, y, width, height.
    @location(5) ink: vec4<f32>,
    // Gradient ramp: centre, then direction (see `text_fx::Ramp`).
    @location(6) ramp_a: vec4<f32>,
    // x: 0 none, 1 linear, 2 radial. y: offset. z: `prev`. w: `inv`.
    @location(7) ramp_b: vec4<f32>,
};

struct Varyings {
    @builtin(position) position: vec4<f32>,
    // Frame pixel of the mask being drawn, before the move.
    @location(0) source: vec2<f32>,
    @location(1) @interpolate(flat) quad: vec4<f32>,
    @location(2) @interpolate(flat) misc: vec4<f32>,
    @location(3) @interpolate(flat) ink: vec4<f32>,
    @location(4) @interpolate(flat) ramp_a: vec4<f32>,
    @location(5) @interpolate(flat) ramp_b: vec4<f32>,
};

@group(0) @binding(0) var mask_tex: texture_2d<f32>;
@group(0) @binding(1) var mask_sampler: sampler;

const REVEAL_SOFT: f32 = 0.12;

@vertex
fn vs_main(in: Instance) -> Varyings {
    let corner = vec2<f32>(f32(in.vertex & 1u), f32(in.vertex >> 1u));
    // Half a texel past the mask: a bilinear tap still reaches it there.
    let margin = vec2<f32>(0.5 * in.misc.y);
    let source = mix(in.quad.xy - margin, in.quad.zw + margin, corner);
    let local = source - in.pivot_shift.xy;
    let moved = vec2<f32>(dot(in.matrix.xy, local), dot(in.matrix.zw, local))
        + in.pivot_shift.xy + in.pivot_shift.zw;
    let ndc = (moved - in.dest.xy) / in.dest.zw * 2.0 - 1.0;
    var out: Varyings;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.source = source;
    out.quad = in.quad;
    out.misc = in.misc;
    out.ink = in.ink;
    out.ramp_a = in.ramp_a;
    out.ramp_b = in.ramp_b;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let texels = (in.quad.zw - in.quad.xy) / in.misc.y;
    let texel = (in.source - in.quad.xy) / in.misc.y;
    // A tap outside the mask weighs 0: the clamped sample is the edge texel
    // alone, faded by how far outside the other tap is.
    let edge = clamp(texel + 0.5, vec2<f32>(0.0), vec2<f32>(1.0))
        * clamp(texels - texel + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    var value = textureSampleLevel(mask_tex, mask_sampler, texel / texels, 0.0).r * edge.x * edge.y;
    if (in.misc.z >= 0.0) {
        let across = (in.source - in.ink.xy) / max(in.ink.zw, vec2<f32>(1.0));
        var u = across.x;
        if (in.misc.w > 2.5) {
            u = 1.0 - across.y;
        } else if (in.misc.w > 1.5) {
            u = across.y;
        } else if (in.misc.w > 0.5) {
            u = 1.0 - across.x;
        }
        value *= clamp((in.misc.z * (1.0 + REVEAL_SOFT) - u) / REVEAL_SOFT, 0.0, 1.0);
    }
    if (in.ramp_b.x > 0.5) {
        let from_centre = in.source - in.ramp_a.xy;
        var t = 0.5 + dot(from_centre, in.ramp_a.zw);
        if (in.ramp_b.x > 1.5) {
            t = length(from_centre) * in.ramp_a.z;
        }
        if (in.ramp_b.y != 0.0) {
            t = fract(t + in.ramp_b.y);
        }
        value *= clamp((t - in.ramp_b.z) * in.ramp_b.w, 0.0, 1.0);
    }
    return vec4<f32>(value * in.misc.x);
}
