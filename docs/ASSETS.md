# Effects, filters and transitions as assets

Each one is a folder with a manifest and a shader, so new ones need no Rust.

```
assets/<effects|filters|transitions>/<name>/manifest.json
assets/<effects|filters|transitions>/<name>/shader.wgsl
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
| `straight(c)`, `premul(c)` | alpha conversions |

Return premultiplied RGBA. The result is blended over `src` by the element's
opacity (fades of an effect element), so no extra handling is needed.

Avoid WGSL reserved words as identifiers (`active`, `filter`, `target`...).
A shader that does not compile is reported on stderr once and skipped (the
input shows unchanged).
