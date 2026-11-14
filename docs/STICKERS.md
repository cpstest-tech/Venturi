# Stickers

The Sticker tab of the Library block has six categories: Shapes, Emoji,
Arrows, Social, Icons and Yours, plus Favorites. Animated stickers show a
play badge on their tile; searching "animated" lists them.

| Category | Source | License |
|---|---|---|
| Shapes | drawn in code (`stickers.rs`, SVG in a 100x100 box, white) | none needed |
| Animated shapes, arrows, annotations, icons | drawn in code (`sticker_anim.rs`), white | none needed |
| Pastel arrows (`pastel-*`) | drawn in code, own colors with a white edge, not tintable | none needed |
| Emoji | [Noto Emoji 3D](https://github.com/googlefonts/noto-emoji) PNGs; the animated ones from Noto Emoji Animation (WebP) | Apache 2.0, CC BY 4.0 |
| Arrows, Social, Icons | [Phosphor](https://github.com/phosphor-icons/core) (fill weight) and [Lucide](https://github.com/lucide-icons/lucide) | MIT, ISC |
| Yours | files imported by the user (PNG, WebP, GIF, SVG, WebM) | the user's |

Only the *names* are in the repository (`crates/vv-app/assets/stickers`).
Images are downloaded when a tile is first shown and kept in
`$XDG_DATA_HOME/venturi/stickers/` (`thumb/` 128 px PNG, `full/` the file that
is used, `svg/` the icon sources, `src/` the downloaded animations). Nothing
is fetched for tiles that are never scrolled to. Shapes and the procedural
animations need no network, except the icon-based ones, which fetch their icon.

## Static and animated

- A tile asks the `Store` for its thumbnail; four worker threads download and
  rasterize. Thumbnails queued before the user changes category are skipped.
- Icons are rasterized with `resvg`, white with a dark halo so they stay
  visible on bright footage.
- An animated sticker is a **VP9 WebM with alpha**, 512 px, 30 fps, looped
  to at least 6 s. The decoder already plays it like any clip: `vv-media`
  opens VP8/VP9 streams that declare `alpha_mode` with libvpx, because
  FFmpeg's own decoders drop the alpha. It is cut to 3 s when added to the
  timeline and can be stretched up to the 6 s baked in.
- Noto animations are decoded from WebP with FFmpeg (`vv_media::anim`),
  resampled to 30 fps honoring each frame's delay, and re-encoded. The
  procedural ones are rendered per frame (`sticker_anim::frame`, one loop on
  all cores) and encoded the same way. Baking takes about 1.5 s on a worker.
- Proxies are skipped for media with alpha (`vv_media::has_alpha`): a proxy
  would lose the transparency.
- Using a sticker (the "+", a double click, a drag) fetches or bakes the full
  file and imports it into the hidden library folder `ribbon-sticker-catalog`.
  From there it behaves like any library asset: projects get their own copy
  of the file, so they keep working offline and when the cache is cleaned.
- Favorites of catalog stickers are stored as `sticker:<id>` in the settings.

## Previews

An animated sticker's tile shows a download badge until its full file is on
disk; a click downloads (or bakes) it, the badge turns into a spinner, and the
tile then plays one loop of the sticker. The WebM is decoded at 96 px in a
background thread (`vv_media::anim::decode_preview`, which also finds the
length of the loop the file repeats), kept as frames and shown through one
texture per tile; at most 40 tiles keep their loop. Adding the sticker to the
timeline downloads it too, so the click is optional.

## Color

Catalog stickers (`shape-`, `lucide-`, `phosphor-`, `anim-` files) are white.
Their Video > Basic properties have a "Sticker color" row: ten swatches and
the free picker. It adds a `Colorize` filter (`FilterKind::Colorize`) that
multiplies the clip by a color, so the alpha and the dark halo stay as they
are. The same filter can be dragged on any clip from the Effects panel. In the
shader (`transform.wgsl`, `apply_filter` id 3) the color arrives packed as
`r << 16 | g << 8 | b` in the filter's scalar parameter, which a f32 holds
exactly. Emoji are not tinted.

## Importing your own

Import and dropped files always go to Yours. SVG becomes a 1024 px PNG,
a GIF or an animated WebP becomes a looping WebM (`stickers::convert_for_import`,
in the background); PNG, WebP stills and WebM with alpha are used as they are.

## Updating the catalogs

```
python3 scripts/stickers/gen_catalog.py
```

Needs network access. It rewrites `emoji.tsv` (with the animated flag),
`lucide.txt` and `phosphor.txt`. These ignored tests check the live URLs and
the baking: `cargo test -p vv-app -- --ignored downloads_each_source animated_stickers_encode`.
