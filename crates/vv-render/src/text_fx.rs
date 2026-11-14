//! Mask operations behind the title styles (`vv_core::TextStyle`): outward
//! growth, blur, shift and gradient bands. Everything works on the 8-bit
//! coverage masks of `text`, limited to the area the mask occupies.

use crate::text::TextMask;
use rayon::prelude::*;
use vv_core::{Gradient, GradientKind, Rgba, StrokeJoin};

#[derive(Clone, Copy)]
pub struct Bounds {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Bounds {
    fn grow(self, by: usize, mask: &TextMask) -> Self {
        Self {
            x0: self.x0.saturating_sub(by),
            y0: self.y0.saturating_sub(by),
            x1: (self.x1 + by).min(mask.width as usize),
            y1: (self.y1 + by).min(mask.height as usize),
        }
    }
}

pub fn empty_like(mask: &TextMask) -> TextMask {
    TextMask {
        width: mask.width,
        height: mask.height,
        data: vec![0; mask.data.len()],
    }
}

pub fn bounds(mask: &TextMask) -> Option<Bounds> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let mut b = Bounds {
        x0: w,
        y0: h,
        x1: 0,
        y1: 0,
    };
    for y in 0..h {
        let row = &mask.data[y * w..(y + 1) * w];
        if let Some(first) = row.iter().position(|&v| v != 0) {
            let last = row.iter().rposition(|&v| v != 0).unwrap_or(first);
            b.x0 = b.x0.min(first);
            b.x1 = b.x1.max(last + 1);
            b.y0 = b.y0.min(y);
            b.y1 = b.y1.max(y + 1);
        }
    }
    (b.x1 > b.x0).then_some(b)
}

/// 1D squared distance transform (Felzenszwalb & Huttenlocher).
fn edt_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let qf = q as f64;
        loop {
            let p = v[k];
            let pf = p as f64;
            let s = ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * qf - 2.0 * pf);
            if s <= z[k] {
                k -= 1;
            } else {
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f64::INFINITY;
                break;
            }
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *o = d * d + f[v[k]];
    }
}

/// Squared distance of every pixel in `b` to the nearest pixel at >= 50%.
fn distance_field(mask: &TextMask, b: Bounds) -> Vec<f64> {
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    const FAR: f64 = 1e9;
    let stride = mask.width as usize;
    let scratch = |n: usize| (vec![0f64; n], vec![0usize; n], vec![0f64; n + 1]);
    // Columns first, each one contiguous in `columns`.
    let mut columns = vec![0f64; w * h];
    columns
        .par_chunks_mut(h)
        .enumerate()
        .for_each_init(
            || (vec![0f64; h], scratch(h)),
            |(seed, (out, v, z)), (x, column)| {
                for (y, s) in seed.iter_mut().enumerate() {
                    let inside = mask.data[(y + b.y0) * stride + x + b.x0] >= 128;
                    *s = if inside { 0.0 } else { FAR };
                }
                edt_1d(seed, out, v, z);
                column.copy_from_slice(out);
            },
        );
    let mut g = vec![0f64; w * h];
    g.par_chunks_mut(w).enumerate().for_each_init(
        || (vec![0f64; w], scratch(w)),
        |(row_in, (out, v, z)), (y, row)| {
            for (x, r) in row_in.iter_mut().enumerate() {
                *r = columns[x * h + y];
            }
            edt_1d(row_in, out, v, z);
            row.copy_from_slice(out);
        },
    );
    g
}

fn grow_round(mask: &TextMask, radius: f32) -> TextMask {
    let mut out = mask.clone();
    let Some(b) = bounds(mask) else { return out };
    let b = b.grow(radius.ceil() as usize + 2, mask);
    let d2 = distance_field(mask, b);
    let w = b.x1 - b.x0;
    let stride = mask.width as usize;
    out.data[b.y0 * stride..b.y1 * stride]
        .par_chunks_mut(stride)
        .zip(d2.par_chunks(w))
        .for_each(|(row, d2)| {
            for (px, d2) in row[b.x0..b.x1].iter_mut().zip(d2) {
                let d = d2.sqrt() as f32;
                let cov = ((radius + 0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8;
                *px = (*px).max(cov);
            }
        });
    out
}

fn grow_square(mask: &TextMask, radius: f32) -> TextMask {
    let r = radius.round().max(0.0) as usize;
    let Some(b) = bounds(mask) else {
        return mask.clone();
    };
    let b = b.grow(r, mask);
    let stride = mask.width as usize;
    let mut tmp = empty_like(mask);
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            let lo = x.saturating_sub(r).max(b.x0);
            let hi = (x + r + 1).min(b.x1);
            tmp.data[y * stride + x] = mask.data[y * stride + lo..y * stride + hi]
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
        }
    }
    let mut out = empty_like(mask);
    for y in b.y0..b.y1 {
        let (lo, hi) = (y.saturating_sub(r).max(b.y0), (y + r + 1).min(b.y1));
        for x in b.x0..b.x1 {
            out.data[y * stride + x] = (lo..hi)
                .map(|yy| tmp.data[yy * stride + x])
                .max()
                .unwrap_or(0);
        }
    }
    out
}

/// Mask grown by `radius` pixels, with the corners of the given join.
pub fn grow(mask: &TextMask, radius: f32, join: StrokeJoin) -> TextMask {
    if radius <= 0.0 {
        return mask.clone();
    }
    match join {
        StrokeJoin::Round => grow_round(mask, radius),
        StrokeJoin::Miter => grow_square(mask, radius),
        StrokeJoin::Bevel => {
            let mut out = grow_square(mask, radius);
            let round = grow_round(mask, radius * 1.3);
            for (o, r) in out.data.iter_mut().zip(&round.data) {
                *o = (*o).min(*r);
            }
            out
        }
    }
}

/// Box blur of radius `r` along the rows of `src`, `w` wide.
fn box_blur_rows(src: &[u8], dst: &mut [u8], w: usize, r: usize) {
    let div = (2 * r + 1) as u64;
    // Exact for every sum a row of bytes can reach.
    let recip = (1u64 << 40) / div + 1;
    dst.par_chunks_mut(w)
        .zip(src.par_chunks(w))
        .for_each(|(dst, src)| {
            let mut sum: u64 = src[..=r.min(w - 1)].iter().map(|&v| u64::from(v)).sum();
            for i in 0..w {
                dst[i] = (((sum + div / 2) * recip) >> 40) as u8;
                if i + r + 1 < w {
                    sum += u64::from(src[i + r + 1]);
                }
                if i >= r {
                    sum -= u64::from(src[i - r]);
                }
            }
        });
}

fn transposed(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    out.par_chunks_mut(h).enumerate().for_each(|(x, column)| {
        for (y, v) in column.iter_mut().enumerate() {
            *v = src[y * w + x];
        }
    });
    out
}

/// Approximate Gaussian blur (three box passes) of standard deviation `sigma`.
pub fn blur(mask: &TextMask, sigma: f32) -> TextMask {
    let Some(b) = bounds(mask) else {
        return mask.clone();
    };
    if sigma < 0.5 {
        return mask.clone();
    }
    let r = ((sigma * 12.0f32.sqrt() / 3.0f32.sqrt() - 1.0) / 2.0)
        .round()
        .max(1.0) as usize;
    let b = b.grow(r * 3 + 1, mask);
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    let stride = mask.width as usize;
    let mut a = vec![0u8; w * h];
    for (row, y) in a.chunks_mut(w).zip(b.y0..) {
        row.copy_from_slice(&mask.data[y * stride + b.x0..y * stride + b.x1]);
    }
    let mut c = vec![0u8; w * h];
    for _ in 0..3 {
        box_blur_rows(&a, &mut c, w, r);
        std::mem::swap(&mut a, &mut c);
    }
    // The columns as rows: contiguous, and blurred in parallel like them.
    let mut a = transposed(&a, w, h);
    for _ in 0..3 {
        box_blur_rows(&a, &mut c, h, r);
        std::mem::swap(&mut a, &mut c);
    }
    let a = transposed(&a, h, w);
    let mut out = mask.clone();
    for (row, y) in a.chunks(w).zip(b.y0..) {
        out.data[y * stride + b.x0..y * stride + b.x1].copy_from_slice(row);
    }
    out
}

/// Frame pixels per texel a blur of `sigma` loses nothing at.
pub fn blur_step(sigma: f32) -> u32 {
    match sigma {
        s if s >= 24.0 => 8,
        s if s >= 12.0 => 4,
        s if s >= 6.0 => 2,
        _ => 1,
    }
}

/// `mask` at 1/`step` of its size: the mean of each block, or with `solid`
/// full where the block has a pixel at >= 50%.
pub fn reduced(mask: &TextMask, step: u32, solid: bool) -> TextMask {
    let st = step.max(1) as usize;
    let (w, h) = (mask.width as usize, mask.height as usize);
    let (rw, rh) = (w.div_ceil(st).max(1), h.div_ceil(st).max(1));
    let mut data = vec![0u8; rw * rh];
    data.par_chunks_mut(rw).enumerate().for_each(|(ry, row)| {
        let mut sums = vec![0u32; rw];
        for y in ry * st..((ry + 1) * st).min(h) {
            for (x, &v) in mask.data[y * w..(y + 1) * w].iter().enumerate() {
                sums[x / st] += if solid { u32::from(v >= 128) } else { u32::from(v) };
            }
        }
        let area = (st * st) as u32;
        for (out, sum) in row.iter_mut().zip(sums) {
            *out = if solid {
                if sum > 0 { 255 } else { 0 }
            } else {
                ((sum + area / 2) / area) as u8
            };
        }
    });
    TextMask {
        width: rw as u32,
        height: rh as u32,
        data,
    }
}

/// A `reduced` mask back at `size`, bilinear.
pub fn enlarged(mask: &TextMask, step: u32, size: (u32, u32)) -> TextMask {
    let (w, h) = (size.0 as usize, size.1 as usize);
    let (sw, sh) = (mask.width as usize, mask.height as usize);
    let st = step.max(1) as f32;
    // Texel and weight pairs along one axis; outside the mask weighs 0.
    let taps = |len: usize, n: usize| -> Vec<[(usize, f32); 2]> {
        (0..len)
            .map(|i| {
                let pos = (i as f32 + 0.5) / st - 0.5;
                let lo = pos.floor();
                let f = pos - lo;
                let tap = |t: f32, weight: f32| {
                    if t >= 0.0 && (t as usize) < n {
                        (t as usize, weight)
                    } else {
                        (0, 0.0)
                    }
                };
                [tap(lo, 1.0 - f), tap(lo + 1.0, f)]
            })
            .collect()
    };
    let (tx, ty) = (taps(w, sw), taps(h, sh));
    let mut data = vec![0u8; w * h];
    data.par_chunks_mut(w).zip(&ty).for_each(|(row, ty)| {
        for (out, tx) in row.iter_mut().zip(&tx) {
            let line = |(y, wy): (usize, f32)| {
                (f32::from(mask.data[y * sw + tx[0].0]) * tx[0].1
                    + f32::from(mask.data[y * sw + tx[1].0]) * tx[1].1)
                    * wy
            };
            *out = (line(ty[0]) + line(ty[1]) + 0.5) as u8;
        }
    });
    TextMask {
        width: size.0,
        height: size.1,
        data,
    }
}

/// The halo of a glow of `radius` around `silhouette`, and the frame pixels
/// per texel it is kept at: nothing in it is finer than its blur.
pub fn glow(silhouette: &TextMask, radius: f32, intensity: f32) -> (TextMask, u32) {
    let sigma = radius * 0.65;
    let step = blur_step(sigma);
    let core = if step == 1 {
        grow(silhouette, radius * 0.35, StrokeJoin::Round)
    } else {
        // A solid block reaches half a texel past the ink, on average.
        let solid = reduced(silhouette, step, true);
        grow(&solid, radius * 0.35 / step as f32 - 0.5, StrokeJoin::Round)
    };
    let halo = blur(&core, sigma / step as f32);
    (scaled(&halo, intensity), step)
}

/// `silhouette` moved and blurred, and the frame pixels per texel of the result.
pub fn soft_shadow(silhouette: &TextMask, dx: i32, dy: i32, sigma: f32) -> (TextMask, u32) {
    let moved = shifted(silhouette, dx, dy);
    let step = blur_step(sigma);
    if step == 1 {
        return (blur(&moved, sigma), 1);
    }
    (blur(&reduced(&moved, step, false), sigma / step as f32), step)
}

pub fn shifted(mask: &TextMask, dx: i32, dy: i32) -> TextMask {
    let mut out = empty_like(mask);
    union_shifted(&mut out, mask, dx, dy);
    out
}

/// `dst = max(dst, src moved by (dx, dy))`.
pub fn union_shifted(dst: &mut TextMask, src: &TextMask, dx: i32, dy: i32) {
    let Some(b) = bounds(src) else { return };
    let (w, h) = (src.width as i32, src.height as i32);
    for y in b.y0 as i32..b.y1 as i32 {
        let ty = y + dy;
        if ty < 0 || ty >= h {
            continue;
        }
        for x in b.x0 as i32..b.x1 as i32 {
            let tx = x + dx;
            if tx < 0 || tx >= w {
                continue;
            }
            let v = src.data[(y * w + x) as usize];
            let d = &mut dst.data[(ty * w + tx) as usize];
            *d = (*d).max(v);
        }
    }
}

pub fn scaled(mask: &TextMask, factor: f32) -> TextMask {
    TextMask {
        width: mask.width,
        height: mask.height,
        data: mask
            .data
            .iter()
            .map(|&v| (v as f32 * factor).round().clamp(0.0, 255.0) as u8)
            .collect(),
    }
}

/// Copies of `mask` along a direction, one pixel at a time up to `depth`.
pub fn extruded(mask: &TextMask, depth: f32, angle_deg: f32) -> TextMask {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let mut out = mask.clone();
    let Some(b) = bounds(mask) else { return out };
    let shifts: Vec<(i32, i32)> = (1..=depth.round().max(0.0) as i32)
        .map(|k| {
            let k = k as f32;
            ((k * cos).round() as i32, (-k * sin).round() as i32)
        })
        .collect();
    let (w, h) = (mask.width as i32, mask.height as i32);
    out.data
        .par_chunks_mut(w as usize)
        .enumerate()
        .for_each(|(ty, row)| {
            for &(dx, dy) in &shifts {
                let y = ty as i32 - dy;
                if y < b.y0 as i32 || y >= b.y1 as i32 || y >= h {
                    continue;
                }
                let x0 = (b.x0 as i32).max(-dx);
                let x1 = (b.x1 as i32).min(w - dx);
                if x1 <= x0 {
                    continue;
                }
                let src = &mask.data[(y * w + x0) as usize..(y * w + x1) as usize];
                let dst = &mut row[(x0 + dx) as usize..(x1 + dx) as usize];
                for (d, &v) in dst.iter_mut().zip(src) {
                    *d = (*d).max(v);
                }
            }
        });
    out
}

/// The weight of one stop of a gradient over the frame: its layer fades in
/// between the stop before and its own, so "over" blending gives the exact
/// mix (see `gradient_layers`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    pub radial: bool,
    pub centre: (f32, f32),
    /// Linear: the gradient parameter grows by this per pixel along each
    /// axis. Radial: `.0` per pixel of distance.
    pub dir: (f32, f32),
    /// Added to the parameter, which then wraps; 0 leaves it alone.
    pub offset: f32,
    pub prev: f32,
    /// 1 / (position of the stop - `prev`).
    pub inv: f32,
}

impl Ramp {
    #[inline]
    pub fn weight(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (x - self.centre.0, y - self.centre.1);
        let t = if self.radial {
            (dx * dx + dy * dy).sqrt() * self.dir.0
        } else {
            0.5 + dx * self.dir.0 + dy * self.dir.1
        };
        let t = if self.offset == 0.0 {
            t
        } else {
            (t + self.offset).rem_euclid(1.0)
        };
        ((t - self.prev) * self.inv).clamp(0.0, 1.0)
    }
}

/// One ramp and colour per stop of `g`, bottom layer first, over an area of
/// `size` around `centre`.
pub fn gradient_ramps(
    g: &Gradient,
    centre: (f32, f32),
    size: (f32, f32),
    extra_angle: f32,
    opacity: f32,
) -> Vec<(Ramp, Rgba)> {
    let mut stops = g.stops.clone();
    stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let (radial, dir) = match g.kind {
        GradientKind::Linear => {
            let (sin, cos) = (g.angle + extra_angle).to_radians().sin_cos();
            let len = (size.0 * cos.abs() + size.1 * sin.abs()).max(1.0);
            (false, (cos / len, sin / len))
        }
        GradientKind::Radial => (true, (1.0 / (0.5 * size.0.hypot(size.1)).max(1.0), 0.0)),
    };
    stops
        .iter()
        .enumerate()
        .map(|(k, stop)| {
            // The first stop covers everything: the others go over it.
            let (prev, inv) = match k.checked_sub(1) {
                None => (-1e9, 1.0),
                Some(before) => {
                    let prev = stops[before].pos;
                    (prev, 1.0 / (stop.pos - prev).max(1e-6))
                }
            };
            let ramp = Ramp {
                radial,
                centre,
                dir,
                offset: g.offset_now(),
                prev,
                inv,
            };
            let color = Rgba {
                a: stop.color.a * opacity,
                ..stop.color
            };
            (ramp, color)
        })
        .collect()
}

/// A gradient over `coverage` as stacked solid layers, one per stop.
pub fn gradient_layers(
    coverage: &TextMask,
    g: &Gradient,
    centre: (f32, f32),
    size: (f32, f32),
    extra_angle: f32,
    opacity: f32,
) -> Vec<(TextMask, Rgba)> {
    let Some(b) = bounds(coverage) else {
        return Vec::new();
    };
    let stride = coverage.width as usize;
    gradient_ramps(g, centre, size, extra_angle, opacity)
        .into_iter()
        .map(|(ramp, color)| {
            let mut mask = empty_like(coverage);
            mask.data[b.y0 * stride..b.y1 * stride]
                .par_chunks_mut(stride)
                .zip(coverage.data[b.y0 * stride..].par_chunks(stride))
                .zip(b.y0..b.y1)
                .for_each(|((row, coverage), y)| {
                    for x in b.x0..b.x1 {
                        let c = coverage[x];
                        if c != 0 {
                            let w = ramp.weight(x as f32 + 0.5, y as f32 + 0.5);
                            row[x] = (f32::from(c) * w).round() as u8;
                        }
                    }
                });
            (mask, color)
        })
        .collect()
}
