//! Mask operations behind the title styles (`vv_core::TextStyle`): outward
//! growth, blur, shift and gradient bands. Everything works on the 8-bit
//! coverage masks of `text`, limited to the area the mask occupies.

use crate::text::TextMask;
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
    let mut g = vec![FAR; w * h];
    for y in 0..h {
        for x in 0..w {
            if mask.data[(y + b.y0) * stride + x + b.x0] >= 128 {
                g[y * w + x] = 0.0;
            }
        }
    }
    let n = w.max(h);
    let (mut v, mut z) = (vec![0usize; n], vec![0f64; n + 1]);
    let (mut col, mut col_out) = (vec![0f64; h], vec![0f64; h]);
    for x in 0..w {
        for y in 0..h {
            col[y] = g[y * w + x];
        }
        edt_1d(&col, &mut col_out, &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = col_out[y];
        }
    }
    let mut row_out = vec![0f64; w];
    for y in 0..h {
        edt_1d(&g[y * w..(y + 1) * w], &mut row_out, &mut v, &mut z);
        g[y * w..(y + 1) * w].copy_from_slice(&row_out);
    }
    g
}

fn grow_round(mask: &TextMask, radius: f32) -> TextMask {
    let mut out = mask.clone();
    let Some(b) = bounds(mask) else { return out };
    let b = b.grow(radius.ceil() as usize + 2, mask);
    let d2 = distance_field(mask, b);
    let w = b.x1 - b.x0;
    let stride = mask.width as usize;
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            let d = d2[(y - b.y0) * w + x - b.x0].sqrt() as f32;
            let cov = ((radius + 0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8;
            let px = &mut out.data[y * stride + x];
            *px = (*px).max(cov);
        }
    }
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

fn box_blur_pass(src: &[u8], dst: &mut [u8], stride: usize, b: Bounds, r: usize, vertical: bool) {
    let (len, lines) = if vertical {
        (b.y1 - b.y0, b.x1 - b.x0)
    } else {
        (b.x1 - b.x0, b.y1 - b.y0)
    };
    let at = |line: usize, i: usize| {
        if vertical {
            (b.y0 + i) * stride + b.x0 + line
        } else {
            (b.y0 + line) * stride + b.x0 + i
        }
    };
    let div = (2 * r + 1) as u32;
    for line in 0..lines {
        let mut sum = 0u32;
        for i in 0..=r.min(len - 1) {
            sum += src[at(line, i)] as u32;
        }
        for i in 0..len {
            dst[at(line, i)] = ((sum + div / 2) / div) as u8;
            let add = i + r + 1;
            if add < len {
                sum += src[at(line, add)] as u32;
            }
            if i >= r {
                sum -= src[at(line, i - r)] as u32;
            }
        }
    }
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
    let stride = mask.width as usize;
    let mut a = mask.data.clone();
    let mut c = vec![0u8; a.len()];
    for _ in 0..3 {
        box_blur_pass(&a, &mut c, stride, b, r, false);
        box_blur_pass(&c, &mut a, stride, b, r, true);
    }
    TextMask {
        width: mask.width,
        height: mask.height,
        data: a,
    }
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
    for k in 1..=depth.round().max(0.0) as i32 {
        let k = k as f32;
        union_shifted(
            &mut out,
            mask,
            (k * cos).round() as i32,
            (-k * sin).round() as i32,
        );
    }
    out
}

/// Gradient parameter of a pixel, before the offset (0-1 across the area).
fn gradient_t(
    g: &Gradient,
    centre: (f32, f32),
    size: (f32, f32),
    extra_angle: f32,
    x: f32,
    y: f32,
) -> f32 {
    let (dx, dy) = (x - centre.0, y - centre.1);
    let t = match g.kind {
        GradientKind::Linear => {
            let (sin, cos) = (g.angle + extra_angle).to_radians().sin_cos();
            let len = (size.0 * cos.abs() + size.1 * sin.abs()).max(1.0);
            0.5 + (dx * cos + dy * sin) / len
        }
        GradientKind::Radial => {
            let radius = (0.5 * size.0.hypot(size.1)).max(1.0);
            dx.hypot(dy) / radius
        }
    };
    let offset = g.offset_now();
    if offset == 0.0 {
        t
    } else {
        (t + offset).rem_euclid(1.0)
    }
}

/// A gradient over `coverage` as stacked solid layers: layer k fades in
/// between stop k-1 and stop k, so "over" blending gives the exact mix.
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
    let mut stops = g.stops.clone();
    stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let stride = coverage.width as usize;
    let mut layers = Vec::with_capacity(stops.len());
    for (k, stop) in stops.iter().enumerate() {
        let mut mask = empty_like(coverage);
        for y in b.y0..b.y1 {
            for x in b.x0..b.x1 {
                let c = coverage.data[y * stride + x];
                if c == 0 {
                    continue;
                }
                let t = gradient_t(g, centre, size, extra_angle, x as f32 + 0.5, y as f32 + 0.5);
                let w = if k == 0 {
                    1.0
                } else {
                    let prev = stops[k - 1].pos;
                    ((t - prev) / (stop.pos - prev).max(1e-6)).clamp(0.0, 1.0)
                };
                mask.data[y * stride + x] = (c as f32 * w).round() as u8;
            }
        }
        let color = Rgba {
            a: stop.color.a * opacity,
            ..stop.color
        };
        layers.push((mask, color));
    }
    layers
}
