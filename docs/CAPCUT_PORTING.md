# Porting CapCut effects to Venturi assets

How the seven "Dynamic" transitions (`crates/vv-render/assets/transitions/`
`shake_flash`, `triple_slide`, `shake_zoom`, `whirlpool`, `slice_zoom`,
`light_flash`, `zoom_flash`) were rebuilt from CapCut, and how to repeat it
for other effects, such as clip in/out animations. The tools are in
`scripts/capcut/`. The asset format is in [ASSETS.md](ASSETS.md).

The approach: read what CapCut runs, bake its keyframes with CapCut's own
code, rewrite the node graph as one WGSL pass, then compare frame by frame
against a video CapCut rendered. Constants that can't be derived are fitted
against that video.

## 1. Inputs

From the user, three things:

- **A CapCut project** using the effects, rendered to video. Use simple
  inputs: still photos, a fixed clip length (3 s), one effect per cut. The
  draft is `User Data/Projects/com.lveditor.draft/<name>/draft_content.json`.
- **The rendered video** of that project, which is the reference.
- **CapCut's `User Data/Cache/effect/<effect_id>/<hash>/`** folders, which
  hold the effects' code.

In `draft_content.json`:

- `materials.transitions[]` has `name`, `effect_id`, `duration` (µs) and
  `is_overlap`. Clip animations live in `materials.material_animations`,
  which was empty in this project.
- `tracks[0].segments[]` gives the clip order and timing; each segment's
  `extra_material_refs` links its transition.
- `materials.videos[]` says whether clips are photos, and their size against
  `canvas_config`. A clip that doesn't fill the canvas is letterboxed with
  **transparent** bars, and CapCut blends B over A, so A shows through B's
  bars. Venturi does the same.

An overlap transition of length `d` is centred on the cut: it spans
`[cut − d/2, cut + d/2]`, and progress is linear in time. This was checked
against every transition, and a timing shift test always found 0 best.
Copy the folder `<effect_id>/<hash>` (not `*_tmp`) somewhere writable before
working on it.

## 2. The three effect families seen

Look at `config.json`, then at the folder layout.

### a) "Lumi" AE exports (`lua/LumiFamily/LumiExportData.lua`)

Shake flash, Triple slide, Slice zoom, Zoom flash and Light flash.

`LumiExportData.lua` returns `data` with these fields:

- `ae_compDurations = {0, T}` is the AE comp length.
- `ae_keyframes['<Node>_<id>-<effectN>#<param>#<type>']` holds the keyframes.
  Each one is `{bezier, {t0,t1}, {{v0},{v1}}, {AE value type}, {flag}}`.
- `ae_attribute[node]` holds the static values. These override the
  defaults in each node's Lua `new()`.
- `ae_durations[node]` gives when a node and its inputs are active, in AE
  seconds. This is how "A until 0.6 s, then B" is expressed.
- `ae_transitionInputIndex = {{node, texName, 0|1}}` says which node gets
  clip A (0) and which gets B (1).

**Time**: for a transition, `aeTime = progress × T` (LumiManager: `aeTime =
frameTimestamp × (comp[2]-comp[1]) + comp[1]`). For a video animation,
`aeTime = remap(animation curTime, 0, duration, comp[1], comp[2])`, which
is the same idea over the animation's own length.

**Graph order**: `strings -n 8 AmazingFeature/main.scene` lists the nodes in
pipeline order, and the `rt/LumiTempRTn.rt` names between them show which
render target feeds which. Inside a `LumiLayer` the effects run in
`effect0, effect1, …` order. The layers composite bottom to top in scene
order, and an `Adjustment` layer re-renders everything below it.

### b) Hand-written "TransitionScript" (`lua/src/TRMain.lua`, `AE.lua`)

Zoom tremolante.

`TRMain:update(t)` holds all the logic: `elapsed = t × DURATION`, and the
keyframes are read through `AEAdapter:get("<layer>/<ADBE track>", time)`.
The shaders are in `xshader/*.frag`, and the pass order comes from
`strings main.scene` (camera names with their `rt/*.rt`). The bundled
`lua/TransitionScript.lua` was identical to `src/`; check the bundle if
they ever differ.

### c) "SeekModeScript" (`lua/SeekModeScript.lua`)

Idromassaggio. It is one script with a local `AETools` and an
`ae_attribute` table. Time is `AEprogress = t × 40/39` over 39 frames. The
script sets properties on effect nodes (`Geometry`, `lumi_Twirl`, a radial
blur, a gaussian blur). Static values such as twirl radius and blur
type/quality live only in the binary scene; see §4.

## 3. Baking keyframes (run CapCut's own interpolator)

Don't reimplement AE easing. `lua` (5.4) is installed, so run their code:

```sh
lua scripts/capcut/bake.lua      <LumiFamily dir> <samples>              > x.json   # family a
lua scripts/capcut/bake_tr.lua   <lua/src dir> <DURATION> <samples> "<layer>/<track>"... > x.json  # family b
lua scripts/capcut/bake_seek.lua <SeekModeScript.lua> <samples>          > x.json   # family c
```

All three stub `Amaz` so the files load without the engine. The output
is `{"track#component": [samples over the whole comp]}`.

Then `scripts/capcut/manifest.py <bake.json> <manifest.json> <spec.json>`
writes the asset manifest. A spec (see `scripts/capcut/specs/*.json`) lists
name, category, `default_secs` (use CapCut's
`extra.json → transition.defaultDura`) and the `curves`. Each curve is a
Python expression evaluated per sample:

- `v('track#i')` is the value now; `V('track#i', k)` is the value k samples
  later (used for motion-blur paths).
- `math` is available, plus helpers `G`/`B` (variance factors of 9-tap
  gaussian/box kernels), `box(intensity, passes)` (LumiBoxBlur σ) and
  `pin(fov)` (OpticsCompensation k1, k2).

Do every unit conversion in the spec, so the shader reads ready-made values:
radians, exposure ÷ 2.2, px ÷ design size, and fitted factors. Sample at about
100 Hz of comp time. A shake that changes direction inside 1/25 s needs
finer sampling plus several path points; see Shaky zoom, which uses 9.

Curves are evaluated on the CPU once per frame (`Asset::curve_values`), with
up to 32 per asset. Anything that depends only on time belongs in a curve,
not in the shader.

## 4. Reading static values from the binary scene

`main.scene` and `*.material` are an AMG serialized format. Search for the
property name and decode what follows:

- `004798ae 05000000` followed by 8 bytes is a little-endian **double**.
  These are saved editor values, so for animated properties they are
  meaningless.
- `3a43660d 03000000` followed by 8 bytes is an **int64**. For example, the
  twirl `radius` was 42, and radial blur `Type` was 3, meaning "Rotate".
- `5060eb27 06000000` followed by 2 floats is a **vec2**.
- Material uniforms that the Lua never sets were all 0 here.

## 5. Node semantics catalogue (as found in the shaders)

| Node | What it does |
|---|---|
| `LumiLayer` (trs) | AE layer transform. Positions are comp px, AE y-down; `anchorPoint` and `p0_*` are the parent. Screen = (q − anchor)·scale + position. `mirrorEdge` mirrors outside the layer |
| `LumiLinearWipe` | `mapProgress(x) = 2x − 0.5`, then keeps `rotate(uv−.5, rotation).y ≥ p` (± feather). At ±90° it cuts columns, at 135/−45 the diagonal (in uv, so corner to corner) |
| `LumiMotionBlur2D` | sample = (uv − position)/scale + anchor (both stored −0.5). The blur runs from the previous AE frame state (`ae_pre_*`) to the current one at w = vCenter+1 … +vIntensity, so it is linear in w. Samples are 12–24 by path length, with mirror optional |
| `LumiSShake` | Sum of sines in **design px (short side = 1080)**, with amplitude per R/G/B (RGB split). Its blur path uses `frq×2`, ±0.5/30 s, n = ceil(I·mix(2,12,quality)), and steps of 5.5 px. CapCut bug kept on purpose: each R/G segment ends on the **blue** matrix |
| `LumiExposure` | `pow(pow(x,2.2)·2^e, 1/2.2)` = `x·2^(e/2.2)`, then clamped to 1. Cheap, so apply it per tap |
| `LumiChromaticAberration`, `StrongSharpen` (strength 0) | R from uv+off·uv(1−uv) and B from uv−off·uv(1−uv); off = value × 0.2 |
| `LumiDropShadow` | Blurred input alpha (σ = width·scale/2.5 design px), offset by `distance` along `angle`, black under the input. For a straight edge this is the normal CDF of the distance, so it needs no taps |
| `LumiAnimSeqLayout` | Scale about the alignment point; `edgeType` 0 tile, 1 mirror, 2 clamp, 3 empty |
| `LumiBoxBlur` | Taps at 0 and ±2k for odd k ≤ 0.64·I, in steps of short side/1000, `iteration` × (X then Y) |
| Gaussian / `LumiDirectionBlur` (`normpdf(i/s·15, 4)`) | σ = 4·floor(I)/15 px of a 720 short side. Direction angle is `90 − angle` |
| TR `gs3`/`boxBlur` | 9 taps at ±0.25…1·R/h. The four G passes use radius R, R/2, R/4, R/8 |
| `OpticsCompensation` | Pincushion: uv·1/(1 + k1·r + k2·r²), with k from fov (see `pin`) and r scaled by W/size, H/size. Outside the frame it is transparent |
| `Geometry` (Idromassaggio) | Rotates the offset from a rotating axis line ("fold"), with mirror |
| `lumi_Twirl` | Smoothstep falloff twirl with aspect handling and the 16:9 radius boost from the shader |
| Radial blur "Rotate" | Arc of 0.225·amount degrees about the centre; the tap count scales with resolution in CapCut, but the extent does not |
| `S_Glow` / `AdjustGlow` / `BCCGlow` | Threshold, then blur, then add or screen. Run on a 360-px-wide copy, chained |

## 6. Writing the shader (one pass)

Start from the shaders in the repo; they share these patterns:

- **Inverse mapping.** For an output pixel, walk the pipeline backwards:
  final zoom, blur offset, lens, transforms, then sample A or B. Composite
  with premultiplied `b + a·(1 − b.a)`.
- **Y axis.** CapCut uv is **y-up**; Venturi's `uv` is y-down. Whenever
  angles, rotations or vertical motion matter, convert once with
  `g = (uv.x, 1 − uv.y)` and sample at `(g.x, 1 − g.y)`. Getting this wrong
  produced a vertically mirrored shake; the error metric caught it (18 563
  → 276).
- **Design units.** CapCut uses design px with the short side at 1080 (Lumi)
  or 720 (TR, glows on a 360-wide copy). Bake values divided by the design
  size, then multiply in the shader by `min(fx_size())/fx_size()`.
- **Linear blur chains collapse.** Stacked box and gaussian passes
  become one gaussian with σ² equal to the sum of the passes' variances, so
  the multi-pass pipeline needs no extra passes.
- **Several blurs at once.** Sample them jointly, Monte-Carlo style: tap i
  takes a golden-angle disc offset (gaussian weight `exp(−2r²)`, radius 2σ)
  plus a motion-path point at `fract(i·0.618)`. This gives one loop instead
  of nested loops.
- **Performance.** Keep the tap count tied to the curves (it drops to one
  sample when nothing is blurred), and cap it at 24–32. **Never rotate the
  pattern per pixel**: interleaved-gradient-noise rotation looked smoother,
  but the worst frame went from 3 ms to 21 ms at 1080p (RTX 2060) because it
  defeats the texture cache. Benchmark the worst frame, not the average.
  Current worst frames are 0.5–6 ms at 1080p.
- Keep comments short, per the repo rules. State what each curve index is in
  the header comment, and mark fitted values as "fitted to CapCut's output".

## 7. Validating against the reference

Setup, done once per session:

```sh
export CAPCUT_WORK=~/capcut-work CAPCUT_VIDEO=/path/to/reference.mp4
mkdir -p $CAPCUT_WORK/{ref,out,assets/transitions}
cp scripts/capcut/fx_frames.rs crates/vv-render/examples/   # never commit it there
cargo build --release -p vv-render --example fx_frames
# stills of A and B from the video, at 480x360 raw RGBA (<name>_s.rgba)
ffmpeg -ss 1.5 -i "$CAPCUT_VIDEO" -frames:v 1 -vf scale=480:360 -f rawvideo -pix_fmt rgba $CAPCUT_WORK/ref/dog_s.rgba
```

For a letterboxed clip, also make a copy with the bars at alpha 0 (Shaky
zoom only matched once the cat's bars were transparent). Work on the asset in
`$CAPCUT_WORK/assets/transitions/<name>/`. The harness loads it at runtime, so
there is no rebuild between edits.

```sh
scripts/capcut/cmp.sh transitions/<name> <cut s> <duration> dog cat <tag>  # CapCut on top, ours below
scripts/capcut/pair.sh <tag> 10 12 14                                     # full-size pairs
python3 -I scripts/capcut/mse.py <tag> 8 9 10 11 12                       # error per frame
SHIFT=0.02 scripts/capcut/cmp.sh ...                                      # timing check
fx_frames $CAPCUT_WORK/assets transitions/<name> a_hd.rgba b_hd.rgba 1920 1080 - 120 0 1  # worst frame time
```

The workflow:

1. Get the structure right visually: direction, signs, timing.
2. Use the error metric to settle signs and shifts, and to fit unknown
   factors on the frames where they matter. Write a constant into the shader,
   `sed` a grid of values, then pick the best.
3. Bake the fitted factors into the spec curves and remove the constants.
4. Copy the folder to `crates/vv-render/assets/transitions/`. The tests in
   `vv-render` check that every built-in compiles, and that every transition
   is A at progress 0 and B at progress 1, using its real curves.

Fitted factors found so far: the TR gaussian chain is ×1.5 and the box
chain ×2.25 (Shaky zoom), LumiBoxBlur is ×1.5 (Zoom flash), and the merged
glows in Light flash are AdjustGlow and BCC intensity ×2. Expect blur
strengths of about 1.5× the analytic value elsewhere too.

## 8. Per-transition notes

- **Shake flash**: B appears from aeTime 0.6. The order is exposure
  (flicker), exposure, SShake (blur, mirror), chroma. Worked in y-up space.
- **Triple slide**: columns at 0–0.298, 0.298–0.702 and 0.702–1. The side
  columns move against the middle one, under a shared 100→121 % zoom, with
  motion blur per column. In y-down space with offsets negated (fitted
  sign).
- **Shaky zoom**: the shake is ×3, with x only while B is invisible. B grows
  78→100 % with mirror over A. The blur path uses 9 sampled points per
  1/25 s.
- **Whirlpool**: everything is shrunk to 70 % with mirror, then fold,
  twirl, rotate blur, small gaussian, and zoomed back. A/B switch at
  t = 0.475.
- **Slice zoom**: diagonal halves (0.0028 overlap) slide to the corners
  with analytic shadows; B grows 40→100 % with mirror.
- **Zoom flash**: A zooms in until the cut, B settles from 120 %, under an
  exposure flash and a box blur (σ fitted).
- **Light flash**: the least exact. Its three chained glows are merged into
  one bloom, so the 3–4 peak frames are less blown out than CapCut. If it
  needs improving, model S_Glow, AdjustGlow (threshold falling to 0.2) and
  BCC as separate stages.

## 9. Clip animations (in / out / combo)

CapCut builds clip animations with the same machinery: Lumi exports with
`ae_effectType` "animation"/video animation, and `LumiManager` maps the
animation's own time onto the comp. Everything above therefore applies, with
these differences, checked against the clip-animation reference:

- There is **one input**: only `src`, and `dst` is the same texture in
  Venturi.
- Time runs over the **animation's duration**, not the clip's. An in
  animation runs from the clip start for its length, an out animation ends
  at the clip end, and a combo spans the clip. The slots are implemented in `clip_anim.rs`
  and `frame_provider::asset_passes`, with the asset grid in `fx_library.rs`.
  Ordinary asset effects still get progress over the whole clip.
- The clip's pass works on the composed frame (the clip already
  transformed). Moving or scaling it exposes transparency, as CapCut does.
- Ask the user for a test project with each animation on a photo clip, a
  rendered video, and the `Cache/effect` folders. Read `material_animations`
  in the draft to map names to `effect_id`, then repeat §2–§7.

The `copazione animazioni` reference project supplies fourteen effects:
Zoom 2, Stretch in/out, Side slide, Fade out, Flash in/out, Swing top left,
Slide up, Blur slide in, Motion slide, Bounce 1, Zoom 1 and Distort and
stretch. Their manifests keep the source effect ID. Regenerate them with:

```sh
python3 scripts/capcut/port_animations.py /path/to/draft_content.json /path/to/Cache/effect
```

`bake_animation.lua` evaluates the original Transform tween functions,
the native AEAdapter tracks, embedded SeekMode AETools and the custom
distortion parameter tweens. It does not need CapCut running. The Lumi
effects still use `bake.lua`. Curves are sampled at 401 points; constant
tracks are reduced to one sample. Do not normalize AEAdapter track names:
spaces and underscores distinguish different exports.

Important findings from the rendered reference:

- The input photo is 1000×646 on a 1080×1920 canvas. Preserve its transparent
  bars and composite it over the reference background only after the effect.
- Side slide, Motion slide, Flash and Slide up render their effect inside
  the fitted image rectangle. Running them directly in canvas coordinates
  stretches the blur and moves the image's edges incorrectly. Animation
  parameters 0..4 carry the rectangle and rotation from `with_assets`.
- The older Transform animations mirror outside the source frame, with a
  [-1, 2] cutoff. Dropping out-of-range samples loses Zoom 2's repeated
  images at the top and bottom. Their rotation and vertical translation
  signs were checked against the reference.
- Stretch out reverses the Lumi curves. Motion slide samples the path
  ahead by 1.66/30 s, rather than using the previous AE frame.
- Compare actual reference frame timestamps. Rounding the last sample
  to the next cut accidentally compares an outgoing animation with the
  next clip's entrance.

The single-pass shaders approximate the stacked Gaussian blurs, glow and
perspective. Distort and stretch is the least exact: its noise hash is
sensitive to GPU sine precision, and its late distortion is fitted to the
reference. These effects are visual ports, not pixel-identical CapCut
engine output.

For an animation comparison, copy and build `fx_frames.rs` as in §7, supply
an RGBA still with transparent bars, and set `CAPCUT_NATIVE=1000x646`.
`CAPCUT_BACKGROUND=0,171,59` composites the result over the background used
in this reference; omit it for the normal black background. In/out harness
progress spans the slot duration; combo progress spans the clip.
