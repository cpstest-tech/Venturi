//! Animated titles. The glyph groups (block, line, word or character) are
//! rasterized once with their whole style into small sprites, cached apart
//! from the frame; a frame only says how each sprite moves, scales, fades
//! and blurs (`Part`). The compositor draws the parts on the GPU;
//! `draw_parts` does the same on the CPU for whoever wants the masks.

use crate::text::{
    GlyphRect, PlacedMask, TextMask, TitleLayer, TitleRender, background_layers, block_origin,
    coverage_mask, reach, rect_bounds, style_layers, text_attrs, with_fonts,
};
use crate::text_fx::{self, Ramp};
use cosmic_text::{Attrs, Buffer, Color, Metrics, Shaping};
use rayon::prelude::*;
use std::sync::{Arc, Mutex, OnceLock};
use vv_core::{AnimPlan, Decor, DecorShape, RevealDir, Rgba, TextFill, TitleParams, UnitFx};

/// A mask of a sprite where it sits before the animation moves it.
#[derive(Clone)]
pub(crate) struct SpriteMask {
    pub mask: Arc<TextMask>,
    /// Frame pixel of its top-left.
    pub origin: (i32, i32),
    /// Frame pixels per texel.
    pub step: u32,
}

impl SpriteMask {
    /// `mask` (top-left at `origin`, `step` frame pixels per texel) cut to
    /// its covered texels plus a blank border for the bilinear sampling.
    fn cropped(mask: &TextMask, origin: (i32, i32), step: u32) -> Self {
        let Some(b) = text_fx::bounds(mask) else {
            return Self {
                mask: Arc::new(TextMask::empty(1, 1)),
                origin,
                step,
            };
        };
        let (x0, y0) = (b.x0.saturating_sub(1), b.y0.saturating_sub(1));
        let (x1, y1) = (
            (b.x1 + 1).min(mask.width as usize),
            (b.y1 + 1).min(mask.height as usize),
        );
        let w = x1 - x0;
        let stride = mask.width as usize;
        let mut data = Vec::with_capacity(w * (y1 - y0));
        for y in y0..y1 {
            data.extend_from_slice(&mask.data[y * stride + x0..y * stride + x1]);
        }
        Self {
            mask: Arc::new(TextMask {
                width: w as u32,
                height: (y1 - y0) as u32,
                data,
            }),
            origin: (
                origin.0 + (x0 as u32 * step) as i32,
                origin.1 + (y0 as u32 * step) as i32,
            ),
            step,
        }
    }

    /// Frame pixels it covers: x0, y0, x1, y1.
    pub fn frame_box(&self) -> (f32, f32, f32, f32) {
        let (x, y) = (self.origin.0 as f32, self.origin.1 as f32);
        (
            x,
            y,
            x + (self.mask.width * self.step) as f32,
            y + (self.mask.height * self.step) as f32,
        )
    }
}

/// A sprite uncovered progressively across its ink box.
#[derive(Clone, Copy)]
pub(crate) struct Reveal {
    pub progress: f32,
    pub dir: RevealDir,
    /// x0, y0, x1, y1 in frame pixels.
    pub ink: (f32, f32, f32, f32),
}

/// Width of the soft edge of a reveal, as a share of the ink box.
pub(crate) const REVEAL_SOFT: f32 = 0.12;

/// One sprite mask as a frame shows it. The parts of a layer go "over" each
/// other as coverage.
#[derive(Clone)]
pub(crate) struct Part {
    pub src: SpriteMask,
    pub xf: Xf,
    pub alpha: f32,
    /// Blur in frame pixels; none under 0.5.
    pub sigma: f32,
    pub reveal: Option<Reveal>,
    /// Weight of a gradient stop, over the frame position of the texel.
    pub ramp: Option<Ramp>,
}

struct Sprite {
    /// Index in `AnimPlan::groups`.
    group: usize,
    /// Ink box of the glyphs, in frame pixels (x0, y0, x1, y1).
    ink: (f32, f32, f32, f32),
    /// Characters from the line centre, for the tracking animation.
    track_pos: f32,
    layers: Vec<(SpriteMask, Rgba)>,
    fill_start: usize,
    text: SpriteMask,
    halo: Option<SpriteMask>,
}

pub(crate) struct Sprites {
    sprites: Vec<Sprite>,
    /// Block origin and size in frame pixels.
    block: (f32, f32, f32, f32),
    font_px: f32,
    size: (u32, u32),
    block_center: (f32, f32),
    background: Vec<(PlacedMask, Rgba)>,
}

/// The animation state is not in the key: the sprites are the same at
/// every frame.
type SpriteKey = (TitleParams, (u32, u32), (u32, u32));

/// Most recent last. A cell is filled by whoever asks first, and anyone
/// else asking meanwhile waits for it instead of building the same sprites.
type SpriteCache = Mutex<Vec<(SpriteKey, Arc<OnceLock<Arc<Sprites>>>)>>;

pub(crate) fn cache() -> &'static SpriteCache {
    static CACHE: OnceLock<SpriteCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Several templates stack titles, subtitles follow one another, and the
/// library previews them at their own size while the player shows the
/// timeline.
const SPRITE_CACHE_LEN: usize = 24;

fn sprite_key(
    params: &TitleParams,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> SpriteKey {
    let mut styled = params.clone();
    styled.anim_state = None;
    (styled, timeline_size, output_size)
}

pub(crate) fn has_sprites(
    params: &TitleParams,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> bool {
    let key = sprite_key(params, timeline_size, output_size);
    let cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    cache.iter().any(|(k, _)| *k == key)
}

/// The sprites of `params`, built now if nobody did.
pub(crate) fn build(
    params: &TitleParams,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> Arc<Sprites> {
    let key = sprite_key(params, timeline_size, output_size);
    let cell = {
        let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
        match cache.iter().position(|(k, _)| *k == key) {
            Some(pos) => {
                let entry = cache.remove(pos);
                let cell = entry.1.clone();
                cache.push(entry);
                cell
            }
            None => {
                if cache.len() >= SPRITE_CACHE_LEN {
                    cache.remove(0);
                }
                let cell = Arc::new(OnceLock::new());
                cache.push((key.clone(), cell.clone()));
                cell
            }
        }
    };
    cell.get_or_init(|| {
        let plan = vv_core::plan(&key.0.anim, &key.0.display_text(), &Default::default());
        Arc::new(build_sprites(&key.0, &plan, timeline_size, output_size))
    })
    .clone()
}

pub(crate) fn rasterize(
    params: &TitleParams,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> TitleRender {
    let state = params.anim_state.unwrap_or_default();
    let plan = vv_core::plan(&params.anim, &params.display_text(), &state);
    let sprites = build(params, timeline_size, output_size);
    compose(&sprites, params, &plan)
}

fn build_sprites(
    params: &TitleParams,
    plan: &AnimPlan,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> Sprites {
    let (width, height) = (output_size.0.max(1), output_size.1.max(1));
    let scale = width as f32 / timeline_size.0.max(1) as f32;
    let font_px = (params.size * scale).max(1.0);
    let line_height = (font_px * 1.2 + params.line_spacing * scale).max(1.0);
    let (attrs, align) = text_attrs(params);
    let text = params.display_text();

    let lines: Vec<&str> = text.split('\n').collect();
    let mut spans: Vec<(String, Attrs)> = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let mut current: Option<(usize, String)> = None;
        for (byte, c) in line.char_indices() {
            let id = plan
                .groups
                .iter()
                .position(|g| g.contains(li, byte))
                .map_or(0, |g| g + 1);
            match &mut current {
                Some((cur, buf)) if *cur == id => buf.push(c),
                _ => {
                    if let Some((cur, buf)) = current.take() {
                        spans.push((buf, attrs.clone().color(code_color(cur))));
                    }
                    current = Some((id, c.to_string()));
                }
            }
        }
        if let Some((cur, buf)) = current.take() {
            spans.push((buf, attrs.clone().color(code_color(cur))));
        }
        if li + 1 < lines.len() {
            spans.push(("\n".into(), attrs.clone().color(code_color(0))));
        }
    }

    // Coverage rectangles per group.
    let mut rects: Vec<Vec<GlyphRect>> = vec![Vec::new(); plan.groups.len()];
    let (block_w, block_h, origin_x, origin_y) = with_fonts(|font_system, swash| {
        let mut buffer = Buffer::new(font_system, Metrics::new(font_px, line_height));
        let spans_ref = || spans.iter().map(|(t, a)| (t.as_str(), a.clone()));
        buffer.set_size(None, None);
        buffer.set_rich_text(spans_ref(), &attrs, Shaping::Advanced, Some(align));
        buffer.shape_until_scroll(font_system, false);
        let block_w = buffer.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
        buffer.set_size(Some(block_w + 1.0), None);
        buffer.shape_until_scroll(font_system, false);
        let block_h = buffer
            .layout_runs()
            .map(|r| r.line_top + r.line_height)
            .fold(0.0, f32::max);
        let (origin_x, origin_y) = block_origin(params, (width, height), scale, (block_w, block_h));
        buffer.draw(
            font_system,
            swash,
            Color::rgb(0, 0, 0),
            |x, y, w, h, color| {
                let id = (u32::from(color.r()) << 16
                    | u32::from(color.g()) << 8
                    | u32::from(color.b())) as usize;
                if id == 0 || color.a() == 0 {
                    return;
                }
                if let Some(group) = rects.get_mut(id - 1) {
                    group.push((x + origin_x, y + origin_y, w, h, color.a()));
                }
            },
        );
        (block_w, block_h, origin_x, origin_y)
    });

    let uses_halo = params_uses_halo(params);
    let margin = reach(params, scale, font_px, uses_halo);
    let block_center = (
        origin_x as f32 + block_w / 2.0,
        origin_y as f32 + block_h / 2.0,
    );
    let built: Vec<Option<Sprite>> = rects
        .par_iter()
        .enumerate()
        .map(|(gi, group_rects)| {
            let bounds = rect_bounds(group_rects)?;
            let origin = (bounds.0 - margin, bounds.1 - margin);
            let size = (
                (bounds.2 - bounds.0 + 2 * margin).max(1) as u32,
                (bounds.3 - bounds.1 + 2 * margin).max(1) as u32,
            );
            let text_mask = coverage_mask(group_rects, origin, size);
            let local_center = (
                block_center.0 - origin.0 as f32,
                block_center.1 - origin.1 as f32,
            );
            let (halo, (layers, fill_start)) = rayon::join(
                || {
                    uses_halo.then(|| {
                        let (halo, step) = text_fx::glow(&text_mask, font_px * 0.25, 1.5);
                        SpriteMask::cropped(&halo, origin, step)
                    })
                },
                || {
                    style_layers(
                        text_mask.clone(),
                        params,
                        scale,
                        local_center,
                        (block_w, block_h),
                    )
                },
            );
            let group = plan.groups[gi];
            let line = lines.get(group.line).copied().unwrap_or("");
            let first = line[..group.start.min(line.len())].chars().count() as f32;
            let end = group.end.min(line.len());
            let count = line[group.start.min(end)..end].chars().count().max(1) as f32;
            let line_chars = line.chars().count().max(1) as f32;
            Some(Sprite {
                group: gi,
                ink: (
                    bounds.0 as f32,
                    bounds.1 as f32,
                    bounds.2 as f32,
                    bounds.3 as f32,
                ),
                track_pos: first + (count - 1.0) / 2.0 - (line_chars - 1.0) / 2.0,
                layers: layers
                    .par_iter()
                    .map(|l| (SpriteMask::cropped(&l.mask, origin, l.step), l.color))
                    .collect(),
                fill_start,
                text: SpriteMask::cropped(&text_mask, origin, 1),
                halo,
            })
        })
        .collect();
    let background = background_layers(
        params,
        (width, height),
        scale,
        (block_center, (block_w, block_h)),
    );
    Sprites {
        sprites: built.into_iter().flatten().collect(),
        block: (origin_x as f32, origin_y as f32, block_w, block_h),
        font_px,
        size: (width, height),
        block_center,
        background,
    }
}

fn params_uses_halo(params: &TitleParams) -> bool {
    [&params.anim.enter, &params.anim.exit, &params.anim.looped]
        .into_iter()
        .flatten()
        .any(|d| d.tracks.iter().any(|t| t.prop == vv_core::AnimProp::Glow))
}

fn code_color(id: usize) -> Color {
    Color::rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// The stops of the fill gradient scrolled by `offset`, bottom first.
fn flowing_fill(sprites: &Sprites, params: &TitleParams, offset: f32) -> Vec<(Ramp, Rgba)> {
    let TextFill::Gradient(g) = &params.style.fill else {
        return Vec::new();
    };
    let mut g = g.clone();
    g.offset.default += offset;
    text_fx::gradient_ramps(
        &g,
        sprites.block_center,
        (sprites.block.2, sprites.block.3),
        0.0,
        params.color.a,
    )
}

fn compose(sprites: &Sprites, params: &TitleParams, plan: &AnimPlan) -> TitleRender {
    let frame = sprites.size;
    let fx_of = |sprite: &Sprite| plan.fx.get(sprite.group).copied().unwrap_or(UnitFx::REST);
    let part = |src: &SpriteMask, sprite: &Sprite, weight: f32, ramp: Option<Ramp>| {
        let fx = fx_of(sprite);
        let alpha = (fx.opacity * weight).clamp(0.0, 1.0);
        (alpha > 0.0).then(|| {
            let (xf, sigma) = placement(sprite, sprites.font_px, &fx);
            Part {
                src: src.clone(),
                xf,
                alpha,
                sigma,
                reveal: (fx.reveal < 1.0).then_some(Reveal {
                    progress: fx.reveal.max(0.0),
                    dir: fx.reveal_dir,
                    ink: sprite.ink,
                }),
                ramp,
            }
        })
    };

    let mean_opacity = if plan.fx.is_empty() {
        1.0
    } else {
        plan.fx.iter().map(|f| f.opacity).sum::<f32>() / plan.fx.len() as f32
    };
    let mut layers: Vec<TitleLayer> = sprites
        .background
        .iter()
        .map(|(m, c)| {
            let color = Rgba {
                a: c.a * mean_opacity,
                ..*c
            };
            TitleLayer::Mask(m.clone(), color)
        })
        .collect();
    for draw in &plan.decor {
        layers.push(TitleLayer::Mask(
            decor_mask(sprites, &draw.decor, draw.progress),
            draw.decor.color,
        ));
    }

    let mut halos: Vec<(Vec<Part>, Rgba)> = Vec::new();
    let mut tints: Vec<(Vec<Part>, Rgba)> = Vec::new();
    let slot = |jobs: &mut Vec<(Vec<Part>, Rgba)>, color: Rgba| -> usize {
        jobs.iter().position(|j| j.1 == color).unwrap_or_else(|| {
            jobs.push((Vec::new(), color));
            jobs.len() - 1
        })
    };
    for sprite in &sprites.sprites {
        let fx = fx_of(sprite);
        if fx.tint.1 > 0.0 {
            let i = slot(&mut tints, fx.tint.0);
            tints[i]
                .0
                .extend(part(&sprite.text, sprite, fx.tint.1.clamp(0.0, 1.0), None));
        }
        if let (Some(halo), true) = (&sprite.halo, fx.glow.1 > 0.0) {
            let i = slot(&mut halos, fx.glow.0);
            halos[i]
                .0
                .extend(part(halo, sprite, fx.glow.1.clamp(0.0, 1.0), None));
        }
    }

    // A moving gradient is not in the cached fill: its stops are weighed
    // over the bare glyphs as they are drawn.
    let flowing = if (plan.gradient_offset * 512.0).round() == 0.0 {
        Vec::new()
    } else {
        flowing_fill(sprites, params, plan.gradient_offset)
    };
    let count = sprites.sprites.first().map_or(0, |s| s.layers.len());
    let mut stack: Vec<(Vec<Part>, Rgba)> = Vec::with_capacity(count);
    for li in 0..count {
        let mut parts = Vec::new();
        let mut color = Rgba::BLACK;
        for sprite in &sprites.sprites {
            let Some((layer, cached)) = sprite.layers.get(li) else {
                continue;
            };
            let fill = li.checked_sub(sprite.fill_start);
            let keep = if fill.is_some() {
                1.0 - fx_of(sprite).tint.1.clamp(0.0, 1.0)
            } else {
                1.0
            };
            // Same shape as the cached stack, or the layers would not line up.
            let stop = fill
                .filter(|_| flowing.len() == sprite.layers.len() - sprite.fill_start)
                .map(|k| flowing[k]);
            match stop {
                Some((ramp, c)) => {
                    color = c;
                    parts.extend(part(&sprite.text, sprite, keep, Some(ramp)));
                }
                None => {
                    color = *cached;
                    parts.extend(part(layer, sprite, keep, None));
                }
            }
        }
        stack.push((parts, color));
    }

    // Halos sit under the glyphs, tints on top of the fill.
    layers.extend(
        halos
            .into_iter()
            .chain(stack)
            .chain(tints)
            .map(|(parts, color)| TitleLayer::Parts(parts, color)),
    );
    if layers.is_empty() {
        layers.push(TitleLayer::Mask(PlacedMask::nothing(), Rgba::BLACK));
    }
    TitleRender {
        size: frame,
        layers,
    }
}

/// How `fx` moves a sprite, and the blur it asks for in frame pixels.
fn placement(sprite: &Sprite, font_px: f32, fx: &UnitFx) -> (Xf, f32) {
    let tracking = fx.tracking / 1000.0 * font_px * sprite.track_pos;
    let shift = (fx.offset[0] * font_px + tracking, -fx.offset[1] * font_px);
    let pivot = (
        (sprite.ink.0 + sprite.ink.2) / 2.0,
        (sprite.ink.1 + sprite.ink.3) / 2.0,
    );
    let xf = Xf {
        scale: fx.scale,
        rotation: fx.rotation.to_radians(),
        shift,
        pivot,
    };
    (xf, fx.blur * font_px * 0.5)
}

/// Frame pixels a blur of `sigma` spreads by.
pub(crate) fn blur_pad(sigma: f32) -> i32 {
    if sigma >= 0.5 {
        (sigma * 3.0).ceil() as i32 + 2
    } else {
        0
    }
}

/// Frame pixels per texel a part blurred by `sigma` is drawn at before the
/// blur: a wide one leaves no detail to keep, and costs a fraction.
pub(crate) fn blur_step(sigma: f32) -> u32 {
    (sigma / 3.0).floor().clamp(1.0, 4.0) as u32
}

/// Rows per band of a layer drawn in parallel.
const BAND_ROWS: usize = 32;

/// A mutable view of mask pixels, `width` wide.
struct Canvas<'a> {
    data: &'a mut [u8],
    width: i32,
    height: i32,
}

impl<'a> Canvas<'a> {
    fn of(mask: &'a mut TextMask) -> Self {
        Self {
            width: mask.width as i32,
            height: mask.height as i32,
            data: &mut mask.data,
        }
    }
}

/// A part ready to draw: moved directly, or blurred beforehand.
enum Prepared<'a> {
    Moved(&'a Part),
    /// Blurred at 1/`step` of the resolution, top-left at the frame point.
    Blurred(TextMask, (i32, i32), u32, f32),
}

fn prepare(part: &Part) -> Option<Prepared<'_>> {
    let sigma = part.sigma;
    if sigma < 0.5 {
        return Some(Prepared::Moved(part));
    }
    let step = blur_step(sigma);
    let pad = blur_pad(sigma);
    let (x0, y0, x1, y1) = dest_box(&part.src, &part.xf)?;
    let origin = (x0 - pad, y0 - pad);
    let size = |len: i32| ((len + 2 * pad).max(1) as u32).div_ceil(step);
    let mut temp = TextMask::empty(size(x1 - x0), size(y1 - y0));
    blit(&mut Canvas::of(&mut temp), origin, step, part, 1.0);
    Some(Prepared::Blurred(
        text_fx::blur(&temp, sigma / step as f32),
        origin,
        step,
        part.alpha,
    ))
}

/// `parts` into one mask over the area they reach, as the compositor draws
/// them on the GPU.
pub(crate) fn draw_parts(parts: &[Part], frame: (u32, u32)) -> PlacedMask {
    let prepared: Vec<Prepared> = parts.par_iter().filter_map(prepare).collect();

    let mut area: Option<(i32, i32, i32, i32)> = None;
    for part in &prepared {
        let b = match part {
            Prepared::Moved(part) => dest_box(&part.src, &part.xf),
            Prepared::Blurred(mask, (x, y), step, _) => Some((
                *x,
                *y,
                x + (mask.width * step) as i32,
                y + (mask.height * step) as i32,
            )),
        };
        let Some(b) = b else { continue };
        area = Some(match area {
            None => b,
            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        });
    }
    let Some((x0, y0, x1, y1)) = area else {
        return PlacedMask::nothing();
    };
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (x1.min(frame.0 as i32), y1.min(frame.1 as i32));
    if x1 <= x0 || y1 <= y0 {
        return PlacedMask::nothing();
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut data = vec![0u8; (w * h) as usize];
    data.par_chunks_mut(BAND_ROWS * w as usize)
        .enumerate()
        .for_each(|(band, rows)| {
            let mut strip = Canvas {
                width: w as i32,
                height: (rows.len() / w as usize) as i32,
                data: rows,
            };
            let origin = (x0, y0 + (band * BAND_ROWS) as i32);
            for part in &prepared {
                match part {
                    Prepared::Moved(part) => blit(&mut strip, origin, 1, part, part.alpha),
                    Prepared::Blurred(mask, at, step, alpha) => {
                        over(&mut strip, origin, mask, *at, *step, *alpha)
                    }
                }
            }
        });
    PlacedMask {
        x: x0 as u32,
        y: y0 as u32,
        mask: Arc::new(TextMask {
            width: w,
            height: h,
            data,
        }),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Xf {
    pub scale: [f32; 2],
    pub rotation: f32,
    pub shift: (f32, f32),
    pub pivot: (f32, f32),
}

impl Xf {
    fn forward(&self, p: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotation.sin_cos();
        let (x, y) = (
            (p.0 - self.pivot.0) * self.scale[0],
            (p.1 - self.pivot.1) * self.scale[1],
        );
        (
            self.pivot.0 + x * cos - y * sin + self.shift.0,
            self.pivot.1 + x * sin + y * cos + self.shift.1,
        )
    }

    /// The inverse as `p -> (a·p + c, b·p + d)`: (ax, ay, c, bx, by, d).
    fn inverse_affine(&self) -> [f32; 6] {
        let (sin, cos) = self.rotation.sin_cos();
        let (sx, sy) = (1.0 / self.scale[0].max(1e-3), 1.0 / self.scale[1].max(1e-3));
        let (tx, ty) = (self.pivot.0 + self.shift.0, self.pivot.1 + self.shift.1);
        // x' = pivot.x + (cos·(x - tx) + sin·(y - ty)) / scale.x, likewise y'.
        [
            cos * sx,
            sin * sx,
            self.pivot.0 - (cos * tx + sin * ty) * sx,
            -sin * sy,
            cos * sy,
            self.pivot.1 - (-sin * tx + cos * ty) * sy,
        ]
    }

    fn is_translation(&self) -> bool {
        self.rotation == 0.0 && self.scale == [1.0, 1.0]
    }
}

/// Frame-pixel box covered by `src` once moved.
pub(crate) fn dest_box(src: &SpriteMask, xf: &Xf) -> Option<(i32, i32, i32, i32)> {
    let (bx0, by0, bx1, by1) = src.frame_box();
    let corners = [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)].map(|p| xf.forward(p));
    let x0 = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor() as i32;
    let y0 = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor() as i32;
    let x1 = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil() as i32 + 1;
    let y1 = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil() as i32 + 1;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

#[inline]
fn over_px(d: &mut u8, s: f32) {
    let cur = f32::from(*d);
    // `+ 0.5` rather than `round()`: without SSE4.1 that is a libm call.
    *d = (cur + s * (255.0 - cur) / 255.0 + 0.5).min(255.0) as u8;
}

/// `v.floor() as i32` without the libm call `floor` is on baseline x86-64.
#[inline]
fn floor_i32(v: f32) -> i32 {
    let i = v as i32;
    if (i as f32) > v { i - 1 } else { i }
}

/// The two taps of a bilinear sample along one axis: indices clamped into
/// the mask and weights 0 for the taps outside it.
#[derive(Clone, Copy)]
struct Taps {
    i0: usize,
    w0: f32,
    i1: usize,
    w1: f32,
}

impl Taps {
    /// `None` when both taps are outside `0..len`.
    fn at(pos: f32, len: i32) -> Option<Self> {
        let i = floor_i32(pos);
        let f = pos - i as f32;
        if i < -1 || i >= len {
            return None;
        }
        let tap = |i: i32, w: f32| {
            if (0..len).contains(&i) {
                (i as usize, w)
            } else {
                (0, 0.0)
            }
        };
        let (i0, w0) = tap(i, 1.0 - f);
        let (i1, w1) = tap(i + 1, f);
        Some(Self { i0, w0, i1, w1 })
    }
}

#[inline]
fn sample(data: &[u8], w: usize, tx: &Taps, ty: &Taps) -> f32 {
    let (r0, r1) = (ty.i0 * w, ty.i1 * w);
    let row = |r: usize| f32::from(data[r + tx.i0]) * tx.w0 + f32::from(data[r + tx.i1]) * tx.w1;
    row(r0) * ty.w0 + row(r1) * ty.w1
}

/// Draws `part` with `alpha` into `dst`, whose pixel (0, 0) covers the
/// frame from `dst_origin` and each pixel `step` frame pixels.
fn blit(dst: &mut Canvas, dst_origin: (i32, i32), step: u32, part: &Part, alpha: f32) {
    let (src, xf) = (&part.src, &part.xf);
    let (dw, dh) = (dst.width, dst.height);
    let (sw, sh) = (src.mask.width as i32, src.mask.height as i32);
    let plain = part.reveal.is_none() && part.ramp.is_none();
    if step == 1 && src.step == 1 && plain && xf.is_translation() {
        let (fx, fy) = (xf.shift.0.round(), xf.shift.1.round());
        if (xf.shift.0 - fx).abs() < 1e-4 && (xf.shift.1 - fy).abs() < 1e-4 {
            // Whole-pixel move: bilinear would sample the texels themselves.
            let ox = src.origin.0 + fx as i32 - dst_origin.0;
            let oy = src.origin.1 + fy as i32 - dst_origin.1;
            let (x0, x1) = (ox.max(0), (ox + sw).min(dw));
            for y in oy.max(0)..(oy + sh).min(dh) {
                let srow = ((y - oy) * sw) as usize;
                let drow = (y * dw) as usize;
                for x in x0..x1 {
                    let v = src.mask.data[srow + (x - ox) as usize];
                    if v != 0 {
                        over_px(&mut dst.data[drow + x as usize], f32::from(v) * alpha);
                    }
                }
            }
            return;
        }
    }
    let Some((x0, y0, x1, y1)) = dest_box(src, xf) else {
        return;
    };
    let st = step as i32;
    let (lx0, ly0) = (
        (x0 - dst_origin.0).div_euclid(st).max(0),
        (y0 - dst_origin.1).div_euclid(st).max(0),
    );
    let (lx1, ly1) = (
        ((x1 - dst_origin.0) + st - 1).div_euclid(st).min(dw),
        ((y1 - dst_origin.1) + st - 1).div_euclid(st).min(dh),
    );
    if lx1 <= lx0 || ly1 <= ly0 {
        return;
    }
    let frame_x = |lx: i32| dst_origin.0 as f32 + (lx as f32 + 0.5) * step as f32;
    let frame_y = |ly: i32| dst_origin.1 as f32 + (ly as f32 + 0.5) * step as f32;
    let [ax, ay, c, bx, by, d] = xf.inverse_affine();
    // Frame point to texel of the source, texel centres on the integers.
    let per_texel = 1.0 / src.step as f32;
    let texel_x = |fx: f32| (fx - src.origin.0 as f32) * per_texel - 0.5;
    let texel_y = |fy: f32| (fy - src.origin.1 as f32) * per_texel - 0.5;
    let sw_u = sw as usize;
    if ay == 0.0 && bx == 0.0 && plain {
        // No rotation: the source column depends only on x, the row only on y.
        let cols: Vec<(usize, Taps)> = (lx0..lx1)
            .filter_map(|lx| Some((lx as usize, Taps::at(texel_x(ax * frame_x(lx) + c), sw)?)))
            .collect();
        for ly in ly0..ly1 {
            let Some(ty) = Taps::at(texel_y(by * frame_y(ly) + d), sh) else {
                continue;
            };
            let drow = (ly * dw) as usize;
            for (lx, tx) in &cols {
                let value = sample(&src.mask.data, sw_u, tx, &ty);
                if value != 0.0 {
                    over_px(&mut dst.data[drow + lx], value * alpha);
                }
            }
        }
        return;
    }
    for ly in ly0..ly1 {
        let py = frame_y(ly);
        let drow = (ly * dw) as usize;
        for lx in lx0..lx1 {
            let px = frame_x(lx);
            let (fx, fy) = (ax * px + ay * py + c, bx * px + by * py + d);
            let (Some(tx), Some(ty)) = (Taps::at(texel_x(fx), sw), Taps::at(texel_y(fy), sh))
            else {
                continue;
            };
            let mut value = sample(&src.mask.data, sw_u, &tx, &ty);
            if value == 0.0 {
                continue;
            }
            if let Some(reveal) = &part.reveal {
                let ink = reveal.ink;
                let (ink_w, ink_h) = ((ink.2 - ink.0).max(1.0), (ink.3 - ink.1).max(1.0));
                let u = match reveal.dir {
                    RevealDir::Right => (fx - ink.0) / ink_w,
                    RevealDir::Left => 1.0 - (fx - ink.0) / ink_w,
                    RevealDir::Down => (fy - ink.1) / ink_h,
                    RevealDir::Up => 1.0 - (fy - ink.1) / ink_h,
                };
                let shown = reveal.progress * (1.0 + REVEAL_SOFT) - u;
                value *= (shown / REVEAL_SOFT).clamp(0.0, 1.0);
            }
            if let Some(ramp) = &part.ramp {
                value *= ramp.weight(fx, fy);
            }
            over_px(&mut dst.data[drow + lx as usize], value * alpha);
        }
    }
}

/// Alpha-over of `src * alpha` into `dst` (top-left at `dst_origin` in the
/// frame); `src` starts at `src_origin` and each of its pixels covers
/// `step` frame pixels.
fn over(
    dst: &mut Canvas,
    dst_origin: (i32, i32),
    src: &TextMask,
    src_origin: (i32, i32),
    step: u32,
    alpha: f32,
) {
    let (dw, dh) = (dst.width, dst.height);
    let (ox, oy) = (src_origin.0 - dst_origin.0, src_origin.1 - dst_origin.1);
    let st = step as i32;
    let (sw, sh) = (src.width as i32, src.height as i32);
    let (x0, x1) = (ox.max(0), (ox + sw * st).min(dw));
    let (y0, y1) = (oy.max(0), (oy + sh * st).min(dh));
    if step == 1 {
        for ty in y0..y1 {
            let srow = ((ty - oy) * sw) as usize;
            let drow = (ty * dw) as usize;
            for tx in x0..x1 {
                let v = f32::from(src.data[srow + (tx - ox) as usize]) * alpha;
                if v != 0.0 {
                    over_px(&mut dst.data[drow + tx as usize], v);
                }
            }
        }
        return;
    }
    let to_src = |t: i32, o: i32| (t - o) as f32 / step as f32 + 0.5 / step as f32 - 0.5;
    let cols: Vec<(usize, Taps)> = (x0..x1)
        .filter_map(|tx| Some((tx as usize, Taps::at(to_src(tx, ox), sw)?)))
        .collect();
    for ty in y0..y1 {
        let Some(tap_y) = Taps::at(to_src(ty, oy), sh) else {
            continue;
        };
        let drow = (ty * dw) as usize;
        for (tx, tap_x) in &cols {
            let v = sample(&src.data, sw as usize, tap_x, &tap_y) * alpha;
            if v != 0.0 {
                over_px(&mut dst.data[drow + tx], v);
            }
        }
    }
}

fn decor_mask(sprites: &Sprites, decor: &Decor, progress: f32) -> PlacedMask {
    let (width, height) = sprites.size;
    let font = sprites.font_px;
    let (bx, by, bw, bh) = sprites.block;
    let thick = (decor.thickness * font).max(1.0);
    let margin = (decor.offset.abs() + 1.0) * font + thick + 2.0;
    let x0 = ((bx - margin).floor() as i32).max(0);
    let y0 = ((by - margin).floor() as i32).max(0);
    let x1 = ((bx + bw + margin).ceil() as i32).min(width as i32);
    let y1 = ((by + bh + margin).ceil() as i32).min(height as i32);
    let p = progress.clamp(0.0, 1.0);
    if p <= 0.0 || x1 <= x0 || y1 <= y0 {
        return PlacedMask::nothing();
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut data = vec![0u8; (w * h) as usize];
    let mut put = |x: i32, y: i32, coverage: f32| {
        let (lx, ly) = (x - x0, y - y0);
        if lx >= 0 && ly >= 0 && lx < w as i32 && ly < h as i32 {
            let d = &mut data[(ly as u32 * w + lx as u32) as usize];
            let cur = f32::from(*d);
            *d = (cur + coverage * 255.0 * (255.0 - cur) / 255.0)
                .round()
                .min(255.0) as u8;
        }
    };
    match decor.shape {
        DecorShape::Underline | DecorShape::Scribble => {
            let y = by + bh + (decor.offset - 0.15) * font;
            let (x0, x1) = (bx - 0.05 * font, bx + bw + 0.05 * font);
            let points = if decor.shape == DecorShape::Underline {
                vec![(x0, y), (x1, y)]
            } else {
                scribble(x0, x1, y, font)
            };
            let path = trim(&points, p, decor.from_center);
            stroke(&path, thick / 2.0, &mut put);
        }
        DecorShape::Bar => {
            let pad = (decor.offset + 0.2) * font;
            let (x0, x1) = (bx - pad, bx + bw + pad);
            let (y0, y1) = (by - pad * 0.5, by + bh + pad * 0.5);
            let (a, b) = if decor.from_center {
                let c = (x0 + x1) / 2.0;
                (c - (x1 - x0) * p / 2.0, c + (x1 - x0) * p / 2.0)
            } else {
                (x0, x0 + (x1 - x0) * p)
            };
            fill_rect(a, y0, b, y1, &mut put);
        }
        DecorShape::SideBar => {
            let x = bx - (0.3 + decor.offset) * font - thick;
            fill_rect(x, by, x + thick, by + bh * p, &mut put);
        }
    }
    PlacedMask {
        x: x0 as u32,
        y: y0 as u32,
        mask: Arc::new(TextMask {
            width: w,
            height: h,
            data,
        }),
    }
}

fn fill_rect(x0: f32, y0: f32, x1: f32, y1: f32, put: &mut impl FnMut(i32, i32, f32)) {
    for y in y0.floor() as i32..y1.ceil() as i32 {
        for x in x0.floor() as i32..x1.ceil() as i32 {
            let cx = ((x as f32 + 1.0).min(x1) - (x as f32).max(x0)).clamp(0.0, 1.0);
            let cy = ((y as f32 + 1.0).min(y1) - (y as f32).max(y0)).clamp(0.0, 1.0);
            put(x, y, cx * cy);
        }
    }
}

/// Zigzag with a deterministic hand-drawn irregularity.
fn scribble(x0: f32, x1: f32, y: f32, font: f32) -> Vec<(f32, f32)> {
    let step = font * 0.3;
    let n = (((x1 - x0) / step).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let wobble = ((i * 2654435761) % 1000) as f32 / 1000.0;
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            (
                x0 + (x1 - x0) * i as f32 / n as f32,
                y + side * font * (0.07 + 0.05 * wobble),
            )
        })
        .collect()
}

/// The part of `points` covering `fraction` of its length (centred when
/// `centred`).
fn trim(points: &[(f32, f32)], fraction: f32, centred: bool) -> Vec<(f32, f32)> {
    let lens: Vec<f32> = points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .collect();
    let total: f32 = lens.iter().sum();
    let (from, to) = if centred {
        (
            total * (1.0 - fraction) / 2.0,
            total * (1.0 + fraction) / 2.0,
        )
    } else {
        (0.0, total * fraction)
    };
    let mut out = Vec::new();
    let mut at = 0.0;
    for (i, &len) in lens.iter().enumerate() {
        let (a, b) = (points[i], points[i + 1]);
        let lerp = |t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        let (s, e) = (at, at + len);
        at = e;
        if e <= from || s >= to || len <= 0.0 {
            continue;
        }
        let ts = ((from - s) / len).clamp(0.0, 1.0);
        let te = ((to - s) / len).clamp(0.0, 1.0);
        if out.is_empty() {
            out.push(lerp(ts));
        }
        out.push(lerp(te));
    }
    out
}

fn stroke(path: &[(f32, f32)], radius: f32, put: &mut impl FnMut(i32, i32, f32)) {
    if path.len() < 2 {
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in path {
        x0 = x0.min(p.0);
        y0 = y0.min(p.1);
        x1 = x1.max(p.0);
        y1 = y1.max(p.1);
    }
    let r = radius + 1.0;
    for y in (y0 - r).floor() as i32..(y1 + r).ceil() as i32 {
        for x in (x0 - r).floor() as i32..(x1 + r).ceil() as i32 {
            let c = (x as f32 + 0.5, y as f32 + 0.5);
            let d = path
                .windows(2)
                .map(|w| segment_distance(c, w[0], w[1]))
                .fold(f32::MAX, f32::min);
            let coverage = (0.5 + radius - d).clamp(0.0, 1.0);
            if coverage > 0.0 {
                put(x, y, coverage);
            }
        }
    }
}

fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + dx * t)).hypot(p.1 - (a.1 + dy * t))
}

#[cfg(test)]
#[path = "tests/text_anim_render.rs"]
mod tests;
