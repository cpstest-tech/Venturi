# Effects, filters and transitions as assets

Each one is a folder with a manifest and a shader, so new ones need no Rust.

```
assets/<effects|filters|transitions|animations>/<name>/manifest.json
assets/<effects|filters|transitions|animations>/<name>/shader.wgsl
```

Built-ins live in `crates/vv-render/assets/` and are embedded by `build.rs`
(rebuild to pick up a new folder). Extra folders with the same layout are read
at startup from `$XDG_DATA_HOME/venturi/assets/`; one with the id of a
built-in replaces it. The asset id saved in projects is `<kind folder>/<name>`.

## manifest.json

```json
{
  "kind": "transition",
  "name": { "en": "Wipe", "it": "Tendina" },
  "category": "motion",
  "default_secs": 0.5,
  "params": [
    { "id": "softness", "name": { "en": "Softness", "it": "Morbidezza" },
      "min": 0, "max": 0.5, "default": 0.08 }
  ]
}
```

`category` groups the sidebar (known ids are translated in `locales/fx.yml`,
others show capitalised). `default_secs` is the length of a transition or of an
effect element (3 s if absent). At most 8 `params`.

Clip animations use `"kind": "animation"` and category `in`, `out` or `combo`.
They appear in the clip's Animation tab, with shader previews on hover.
Clicking an animation, including the selected one, returns the playhead to
the selected clip's start. Duration adjustments leave the playhead in place.
In and out slots have independent durations, fitted proportionally when their
sum exceeds the clip length. A combo spans the clip. Progress uses timeline
time, including when a clip is trimmed or its playback speed changes.
Old projects without animation slots keep their previous behavior.

Animation shader parameter slots are reserved for the engine: 0/1 are the
normalized image center, 2/3 its fitted width/height and 4 its rotation in
radians. This lets effects whose CapCut pipeline runs at native image size
work on letterboxed clips. A missing rectangle defaults to the whole frame.

`curves` (optional, at most 32) animates values over the progress: each is a
list of samples at even steps from progress 0 to 1, linearly interpolated on
the CPU once per frame, so a keyframed animation costs nothing per pixel:

```json
"curves": [[0, 0.2, 1, 0.2, 0], [1, 1.5, 1]]
```

## shader.wgsl

Only `fn effect(uv: vec2<f32>) -> vec4<f32>`; `uv` is 0..1 from the top left.
The engine prelude (`vv-render/src/custom_shader.rs`) provides:

| | |
|---|---|
| `src(uv)`, `dst(uv)` | input A and B, premultiplied RGBA. `dst` is the incoming clip of a transition (same as `src` for effects) |
| `fx_size()` | frame size in pixels |
| `fx_progress()` | 0..1: transition progress (already eased), or position inside the clip |
| `fx_time()` | seconds since the start of the clip or transition |
| `param(i)` | the manifest parameters, clamped to their range |
| `curve(i)` | the manifest curves at the current progress (0 if absent) |
| `straight(c)`, `premul(c)` | alpha conversions |

Return premultiplied RGBA. The result is blended over `src` by the element's
opacity (fades of an effect element), so no extra handling is needed.

Every tap is a texture read per pixel: keep loops bounded, with a count that
varies smoothly across the frame (better: only with the curves), and sample
neighbouring pixels at neighbouring places. A pattern rotated per pixel
looks smoother but defeats the texture cache and is several times slower.

Avoid WGSL reserved words as identifiers (`active`, `filter`, `target`...).
A shader that does not compile is reported on stderr once and skipped (the
input shows unchanged).
