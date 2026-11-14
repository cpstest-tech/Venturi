# Stickers

The Sticker tab of the Library block has six categories: Shapes, Emoji,
Arrows, Social, Icons and Yours, plus Favorites.

| Category | Source | License |
|---|---|---|
| Shapes | drawn in code (`stickers.rs`, SVG in a 100x100 box, 10 colors) | none needed |
| Emoji | [Noto Emoji 3D](https://github.com/googlefonts/noto-emoji) PNGs | Apache 2.0 |
| Arrows, Social, Icons | [Phosphor](https://github.com/phosphor-icons/core) (fill weight) and [Lucide](https://github.com/lucide-icons/lucide) | MIT, ISC |
| Yours | files imported by the user (PNG, WebP) | the user's |

Only the *names* are in the repository (`crates/vv-app/assets/stickers`).
Images are downloaded when a tile is first shown and kept in
`$XDG_DATA_HOME/venturi/stickers/` (`thumb/` 128 px, `full/` 512 px, `svg/`
the icon sources). Nothing is fetched for tiles that are never scrolled to.
Shapes need no network.

## Flow

- A tile asks the `Store` for its thumbnail; four worker threads download
  and rasterize. Thumbnails queued before the user changes category are
  skipped.
- Icons are rasterized with `resvg`, white with a dark halo so they stay
  visible on bright footage.
- Using a sticker (the "+", a double click, a drag) fetches the 512 px image
  and imports it into the hidden library folder `ribbon-sticker-catalog`.
  From there it behaves like any library asset: projects get their own copy
  of the file, so they keep working offline and when the cache is cleaned.
- Favorites of catalog stickers are stored as `sticker:<id>` in the settings.

## Updating the catalogs

```
python3 scripts/stickers/gen_catalog.py
```

Needs network access. It rewrites `emoji.tsv`, `lucide.txt` and
`phosphor.txt`. `cargo test -p vv-app -- --ignored downloads_each_source`
checks that the download URLs still work.

## Not done yet

- Animated stickers: Noto publishes animated WebP/Lottie for many emoji
  (`fonts.gstatic.com/s/e/notoemoji/latest/<code>/512.webp`), but the
  animated GIF/WebP decoder is still missing (see `gap-analysis.md`).
- A color picker for icons (they are white).
