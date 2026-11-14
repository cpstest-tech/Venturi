//! Animated titles. The glyph groups (block, line, word or character) are
//! rasterized once with their whole style into small sprites, cached apart
//! from the frame; each frame only moves, scales, fades and blurs the
//! sprites into the usual stack of layer masks, so the compositor sees an
//! ordinary `TitleRender`.

use crate::text::{
    TextMask, TitleRender, background_layers, style_layers, text_attrs,
};
use crate::text_fx;
use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache};
use std::sync::{Arc, Mutex, OnceLock};
use vv_core::{
    AnimPlan, Decor, DecorShape, HAnchor, RevealDir, Rgba, TextFill, TitleParams, UnitFx, VAnchor,
};

struct Sprite {
    /// Index in `AnimPlan::groups`.
    group: usize,
    /// Top-left of the sprite masks in frame pixels.
    origin: (i32, i32),
    /// Ink box of the glyphs, in frame pixels (x0, y0, x1, y1).
    ink: (f32, f32, f32, f32),
    /// Characters from the line centre, for the tracking animation.
    track_pos: f32,
    layers: Vec<(TextMask, Rgba)>,
    fill_start: usize,
    text: TextMask,
    halo: Option<TextMask>,
}

pub(crate) struct Sprites {
    sprites: Vec<Sprite>,
    /// Block origin and size in frame pixels.
    block: (f32, f32, f32, f32),
    font_px: f32,
    size: (u32, u32),
    scale: f32,
    block_center: (f32, f32),
}

type SpriteKey = (TitleParams, (u32, u32), (u32, u32), i32);

fn cache() -> &'static Mutex<Vec<(SpriteKey, Arc<Sprites>)>> {
    static CACHE: OnceLock<Mutex<Vec<(SpriteKey, Arc<Sprites>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

const SPRITE_CACHE_LEN: usize = 3;

pub(crate) fn rasterize(
    font_system: &mut FontSystem,
    swash: &mut SwashCache,
    params: &TitleParams,
    timeline_size: (u32, u32),
    output_size: (u32, u32),
) -> TitleRender {
    let state = params.anim_state.unwrap_or_default();
    let text = params.display_text();
    let mut plan = vv_core::plan(&params.anim, &text, &state);

    // The gradient scrolls as a whole: it is part of the sprites.
    let mut styled = params.clone();
    styled.anim_state = None;
    if let TextFill::Gradient(g) = &mut styled.style.fill {
        g.offset.default += plan.gradient_offset;
    }
    let offset_q = (plan.gradient_offset * 512.0).round() as i32;
    let key: SpriteKey = (styled.clone(), timeline_size, output_size, offset_q);

    let cached = {
        let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
        cache.iter().position(|(k, _)| *k == key).map(|pos| {
            let entry = cache.remove(pos);
            let sprites = entry.1.clone();
            cache.push(entry);
            sprites
        })
    };
    let sprites = cached.unwrap_or_else(|| {
        let built = Arc::new(build_sprites(
            font_system,
            swash,
            &styled,
            &plan,
            timeline_size,
            output_size,
        ));
        let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= SPRITE_CACHE_LEN {
            cache.remove(0);
        }
        cache.push((key, built.clone()));
        built
    });
    plan.gradient_offset = 0.0;
    compose(&sprites, &styled, &plan)
}

/// Pixels a style can reach beyond the glyphs.
fn reach(params: &TitleParams, scale: f32, font_px: f32, halo: bool) -> i32 {
    let style = &params.style;
    let stroke = style.strokes.iter().map(|s| s.width).fold(0.0, f32::max);
    let shadow = style
        .shadows
        .iter()
        .map(|s| s.offset[0].abs().max(s.offset[1].abs()) + s.blur * 1.5)
        .fold(0.0, f32::max);
    let glow = style.glows.iter().map(|g| g.radius).fold(0.0, f32::max);
    let extrude = style.extrude.as_ref().map_or(0.0, |e| e.depth);
    let mut total = (stroke + shadow + glow + extrude) * scale;
    total += font_px * if halo { 0.6 } else { 0.2 };
    total.ceil() as i32 + 2
}

fn build_sprites(
    font_system: &mut FontSystem,
    swash: &mut SwashCache,
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

    let anchor_x = width as f32 / 2.0 + params.position[0] * scale;
    let anchor_y = height as f32 / 2.0 - params.position[1] * scale;
    let origin_x = (anchor_x
        - match params.anchor.0 {
            HAnchor::Left => 0.0,
            HAnchor::Center => block_w / 2.0,
            HAnchor::Right => block_w,
        })
    .round() as i32;
    let origin_y = (anchor_y
        - match params.anchor.1 {
            VAnchor::Top => 0.0,
            VAnchor::Middle => block_h / 2.0,
            VAnchor::Bottom => block_h,
        })
    .round() as i32;

    // Coverage rectangles per group.
    let mut rects: Vec<Vec<(i32, i32, u32, u32, u8)>> = vec![Vec::new(); plan.groups.len()];
    buffer.draw(font_system, swash, Color::rgb(0, 0, 0), |x, y, w, h, color| {
        let id = (u32::from(color.r()) << 16 | u32::from(color.g()) << 8 | u32::from(color.b()))
            as usize;
        if id == 0 || color.a() == 0 {
            return;
        }
        if let Some(group) = rects.get_mut(id - 1) {
            group.push((x + origin_x, y + origin_y, w, h, color.a()));
        }
    });

    let uses_halo = params_uses_halo(params);
    let margin = reach(params, scale, font_px, uses_halo);
    let block_center = (
        origin_x as f32 + block_w / 2.0,
        origin_y as f32 + block_h / 2.0,
    );
    let mut sprites = Vec::new();
    for (gi, group_rects) in rects.iter().enumerate() {
        let Some(bounds) = rect_bounds(group_rects) else {
            continue;
        };
        let origin = (bounds.0 - margin, bounds.1 - margin);
        let size = (
            (bounds.2 - bounds.0 + 2 * margin).max(1) as u32,
            (bounds.3 - bounds.1 + 2 * margin).max(1) as u32,
        );
        let mut data = vec![0u8; (size.0 * size.1) as usize];
        for &(x, y, w, h, coverage) in group_rects {
            for py in (y - origin.1).max(0)..(y - origin.1 + h as i32).min(size.1 as i32) {
                for px in (x - origin.0).max(0)..(x - origin.0 + w as i32).min(size.0 as i32) {
                    let dst = &mut data[(py as u32 * size.0 + px as u32) as usize];
                    *dst = (u32::from(*dst)
                        + u32::from(coverage) * (255 - u32::from(*dst)) / 255)
                        as u8;
                }
            }
        }
        let text_mask = TextMask {
            width: size.0,
            height: size.1,
            data,
        };
        let local_center = (block_center.0 - origin.0 as f32, block_center.1 - origin.1 as f32);
        let (layers, fill_start) = style_layers(
            text_mask.clone(),
            params,
            scale,
            local_center,
            (block_w, block_h),
        );
        let halo = uses_halo.then(|| {
            let r = font_px * 0.25;
            let core = text_fx::grow(&text_mask, r * 0.35, vv_core::StrokeJoin::Round);
            text_fx::scaled(&text_fx::blur(&core, r * 0.65), 1.5)
        });
        let group = plan.groups[gi];
        let line = lines.get(group.line).copied().unwrap_or("");
        let first = line[..group.start.min(line.len())].chars().count() as f32;
        let end = group.end.min(line.len());
        let count = line[group.start.min(end)..end].chars().count().max(1) as f32;
        let line_chars = line.chars().count().max(1) as f32;
        sprites.push(Sprite {
            group: gi,
            origin,
            ink: (
                bounds.0 as f32,
                bounds.1 as f32,
                bounds.2 as f32,
                bounds.3 as f32,
            ),
            track_pos: first + (count - 1.0) / 2.0 - (line_chars - 1.0) / 2.0,
            layers,
            fill_start,
            text: text_mask,
            halo,
        });
    }
    Sprites {
        sprites,
        block: (origin_x as f32, origin_y as f32, block_w, block_h),
        font_px,
        size: (width, height),
        scale,
        block_center,
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

fn rect_bounds(rects: &[(i32, i32, u32, u32, u8)]) -> Option<(i32, i32, i32, i32)> {
    rects.iter().fold(None, |acc, &(x, y, w, h, _)| {
        let (x1, y1) = (x + w as i32, y + h as i32);
        Some(match acc {
            None => (x, y, x1, y1),
            Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x1), d.max(y1)),
        })
    })
}

fn compose(sprites: &Sprites, params: &TitleParams, plan: &AnimPlan) -> TitleRender {
    let (width, height) = sprites.size;
    let blank = || TextMask {
        width,
        height,
        data: vec![0u8; (width * height) as usize],
    };
    let fx_of = |sprite: &Sprite| plan.fx.get(sprite.group).copied().unwrap_or(UnitFx::REST);

    let mean_opacity = if plan.fx.is_empty() {
        1.0
    } else {
        plan.fx.iter().map(|f| f.opacity).sum::<f32>() / plan.fx.len() as f32
    };
    let mut layers = background_layers(
        params,
        sprites.size,
        sprites.scale,
        (sprites.block_center, (sprites.block.2, sprites.block.3)),
    );
    for (_, color) in &mut layers {
        color.a *= mean_opacity;
    }
    for draw in &plan.decor {
        layers.push((
            decor_mask(sprites, &draw.decor, draw.progress),
            draw.decor.color,
        ));
    }

    let count = sprites.sprites.first().map_or(0, |s| s.layers.len());
    let mut stack: Vec<(TextMask, Rgba)> = Vec::new();
    for li in 0..count {
        let mut mask = blank();
        let mut color = None;
        for sprite in &sprites.sprites {
            let Some((layer, c)) = sprite.layers.get(li) else {
                continue;
            };
            color = Some(*c);
            let fx = fx_of(sprite);
            let keep = if li >= sprite.fill_start {
                1.0 - fx.tint.1.clamp(0.0, 1.0)
            } else {
                1.0
            };
            blit_sprite(&mut mask, layer, sprite, sprites, &fx, keep);
        }
        stack.push((mask, color.unwrap_or(Rgba::BLACK)));
    }

    let mut halos: Vec<(Rgba, TextMask)> = Vec::new();
    let mut tints: Vec<(Rgba, TextMask)> = Vec::new();
    for sprite in &sprites.sprites {
        let fx = fx_of(sprite);
        if fx.tint.1 > 0.0 {
            let slot = slot_for(&mut tints, fx.tint.0, &blank);
            blit_sprite(slot, &sprite.text, sprite, sprites, &fx, fx.tint.1.clamp(0.0, 1.0));
        }
        if let (Some(halo), true) = (&sprite.halo, fx.glow.1 > 0.0) {
            let slot = slot_for(&mut halos, fx.glow.0, &blank);
            blit_sprite(slot, halo, sprite, sprites, &fx, fx.glow.1.clamp(0.0, 1.0));
        }
    }

    // Halos sit under the glyphs, tints on top of the fill.
    let fill_at = sprites.sprites.first().map_or(0, |s| s.fill_start);
    let mut tail = stack.split_off(fill_at.min(stack.len()));
    layers.extend(halos.into_iter().map(|(c, m)| (m, c)));
    layers.extend(stack);
    layers.append(&mut tail);
    layers.extend(tints.into_iter().map(|(c, m)| (m, c)));
    if layers.is_empty() {
        layers.push((blank(), Rgba::BLACK));
    }
    TitleRender { layers }
}

fn slot_for<'a>(
    slots: &'a mut Vec<(Rgba, TextMask)>,
    color: Rgba,
    blank: &dyn Fn() -> TextMask,
) -> &'a mut TextMask {
    let pos = slots.iter().position(|(c, _)| *c == color).unwrap_or_else(|| {
        slots.push((color, blank()));
        slots.len() - 1
    });
    &mut slots[pos].1
}

/// Draws `src` (a mask of `sprite`) into `dst` as `fx` moves it. `weight`
/// scales the coverage.
fn blit_sprite(
    dst: &mut TextMask,
    src: &TextMask,
    sprite: &Sprite,
    sprites: &Sprites,
    fx: &UnitFx,
    weight: f32,
) {
    let font_px = sprites.font_px;
    let alpha = (fx.opacity * weight).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let tracking = fx.tracking / 1000.0 * font_px * sprite.track_pos;
    let shift = (
        fx.offset[0] * font_px + tracking,
        -fx.offset[1] * font_px,
    );
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
    let reveal = (fx.reveal < 1.0).then_some((fx.reveal.max(0.0), fx.reveal_dir));
    let sigma = fx.blur * font_px * 0.5;
    if sigma >= 0.5 {
        let pad = (sigma * 3.0).ceil() as i32 + 2;
        let Some((x0, y0, x1, y1)) = dest_box(src, sprite.origin, &xf) else {
            return;
        };
        let (ox, oy) = (x0 - pad, y0 - pad);
        let mut temp = TextMask {
            width: (x1 - x0 + 2 * pad).max(1) as u32,
            height: (y1 - y0 + 2 * pad).max(1) as u32,
            data: vec![0; ((x1 - x0 + 2 * pad).max(1) * (y1 - y0 + 2 * pad).max(1)) as usize],
        };
        blit(&mut temp, (ox, oy), src, sprite, &xf, 1.0, reveal);
        let blurred = text_fx::blur(&temp, sigma);
        over(dst, &blurred, (ox, oy), alpha);
    } else {
        blit(dst, (0, 0), src, sprite, &xf, alpha, reveal);
    }
}

struct Xf {
    scale: [f32; 2],
    rotation: f32,
    shift: (f32, f32),
    pivot: (f32, f32),
}

impl Xf {
    fn forward(&self, p: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotation.sin_cos();
        let (x, y) = ((p.0 - self.pivot.0) * self.scale[0], (p.1 - self.pivot.1) * self.scale[1]);
        (
            self.pivot.0 + x * cos - y * sin + self.shift.0,
            self.pivot.1 + x * sin + y * cos + self.shift.1,
        )
    }

    fn inverse(&self, p: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotation.sin_cos();
        let (x, y) = (p.0 - self.pivot.0 - self.shift.0, p.1 - self.pivot.1 - self.shift.1);
        (
            self.pivot.0 + (x * cos + y * sin) / self.scale[0].max(1e-3),
            self.pivot.1 + (-x * sin + y * cos) / self.scale[1].max(1e-3),
        )
    }
}

/// Frame-pixel box covered by `src` once moved.
fn dest_box(src: &TextMask, origin: (i32, i32), xf: &Xf) -> Option<(i32, i32, i32, i32)> {
    let (w, h) = (src.width as f32, src.height as f32);
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
        .map(|(x, y)| xf.forward((x + origin.0 as f32, y + origin.1 as f32)));
    let x0 = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor() as i32;
    let y0 = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor() as i32;
    let x1 = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil() as i32 + 1;
    let y1 = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil() as i32 + 1;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

fn blit(
    dst: &mut TextMask,
    dst_origin: (i32, i32),
    src: &TextMask,
    sprite: &Sprite,
    xf: &Xf,
    alpha: f32,
    reveal: Option<(f32, RevealDir)>,
) {
    let Some((x0, y0, x1, y1)) = dest_box(src, sprite.origin, xf) else {
        return;
    };
    let (dw, dh) = (dst.width as i32, dst.height as i32);
    let (sw, sh) = (src.width as i32, src.height as i32);
    let ink = sprite.ink;
    let (ink_w, ink_h) = ((ink.2 - ink.0).max(1.0), (ink.3 - ink.1).max(1.0));
    for y in y0.max(dst_origin.1)..y1.min(dst_origin.1 + dh) {
        for x in x0.max(dst_origin.0)..x1.min(dst_origin.0 + dw) {
            let (fx, fy) = xf.inverse((x as f32 + 0.5, y as f32 + 0.5));
            let (sx, sy) = (fx - sprite.origin.0 as f32 - 0.5, fy - sprite.origin.1 as f32 - 0.5);
            let mut value = bilinear(src, sw, sh, sx, sy);
            if value == 0.0 {
                continue;
            }
            if let Some((progress, dir)) = reveal {
                let u = match dir {
                    RevealDir::Right => (fx - ink.0) / ink_w,
                    RevealDir::Left => 1.0 - (fx - ink.0) / ink_w,
                    RevealDir::Down => (fy - ink.1) / ink_h,
                    RevealDir::Up => 1.0 - (fy - ink.1) / ink_h,
                };
                const SOFT: f32 = 0.12;
                value *= ((progress * (1.0 + SOFT) - u) / SOFT).clamp(0.0, 1.0);
            }
            let s = value * alpha;
            let d = &mut dst.data[((y - dst_origin.1) * dw + (x - dst_origin.0)) as usize];
            let cur = f32::from(*d);
            *d = (cur + s * (255.0 - cur) / 255.0).round().min(255.0) as u8;
        }
    }
}

fn bilinear(src: &TextMask, w: i32, h: i32, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |ix: i32, iy: i32| -> f32 {
        if ix < 0 || iy < 0 || ix >= w || iy >= h {
            0.0
        } else {
            f32::from(src.data[(iy * w + ix) as usize])
        }
    };
    let (ix, iy) = (x0 as i32, y0 as i32);
    let top = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
    let bottom = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Alpha-over of `src * alpha` placed at `src_origin`.
fn over(dst: &mut TextMask, src: &TextMask, src_origin: (i32, i32), alpha: f32) {
    let (dw, dh) = (dst.width as i32, dst.height as i32);
    for y in 0..src.height as i32 {
        let ty = y + src_origin.1;
        if ty < 0 || ty >= dh {
            continue;
        }
        for x in 0..src.width as i32 {
            let tx = x + src_origin.0;
            if tx < 0 || tx >= dw {
                continue;
            }
            let v = f32::from(src.data[(y * src.width as i32 + x) as usize]) * alpha;
            if v == 0.0 {
                continue;
            }
            let d = &mut dst.data[(ty * dw + tx) as usize];
            let cur = f32::from(*d);
            *d = (cur + v * (255.0 - cur) / 255.0).round().min(255.0) as u8;
        }
    }
}

fn decor_mask(sprites: &Sprites, decor: &Decor, progress: f32) -> TextMask {
    let (width, height) = sprites.size;
    let mut data = vec![0u8; (width * height) as usize];
    let font = sprites.font_px;
    let (bx, by, bw, bh) = sprites.block;
    let p = progress.clamp(0.0, 1.0);
    if p > 0.0 {
        let mut put = |x: i32, y: i32, coverage: f32| {
            if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                let d = &mut data[(y as u32 * width + x as u32) as usize];
                let cur = f32::from(*d);
                *d = (cur + coverage * 255.0 * (255.0 - cur) / 255.0).round().min(255.0) as u8;
            }
        };
        let thick = (decor.thickness * font).max(1.0);
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
    }
    TextMask {
        width,
        height,
        data,
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
        (total * (1.0 - fraction) / 2.0, total * (1.0 + fraction) / 2.0)
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
