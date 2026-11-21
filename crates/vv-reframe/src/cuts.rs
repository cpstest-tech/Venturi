//! Hard cuts from colour histograms of consecutive frames.

use vv_core::FrameIdx;
use vv_media::FrameYuv420;

const LUMA_BINS: usize = 32;
const CHROMA_BINS: usize = 16;
/// Below this a frame change is never a cut, whatever its neighbours do.
const MIN_DISTANCE: f32 = 0.25;
/// How far above the usual change of the surrounding frames a cut stands.
const OVER_MEDIAN: f32 = 4.0;
const WINDOW: usize = 15;
const MIN_SHOT_SECS: f64 = 0.5;

/// Normalised luma and chroma histograms of one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameHistogram {
    luma: [f32; LUMA_BINS],
    u: [f32; CHROMA_BINS],
    v: [f32; CHROMA_BINS],
}

impl FrameHistogram {
    pub fn of(frame: &FrameYuv420) -> Self {
        let mut h = Self {
            luma: [0.0; LUMA_BINS],
            u: [0.0; CHROMA_BINS],
            v: [0.0; CHROMA_BINS],
        };
        for &y in &frame.y {
            h.luma[usize::from(y) * LUMA_BINS / 256] += 1.0;
        }
        for cy in 0..frame.chroma_height as usize {
            for cx in 0..frame.chroma_width as usize {
                let (u, v) = frame.chroma_at(cx, cy);
                h.u[usize::from(u) * CHROMA_BINS / 256] += 1.0;
                h.v[usize::from(v) * CHROMA_BINS / 256] += 1.0;
            }
        }
        let normalise = |bins: &mut [f32]| {
            let total: f32 = bins.iter().sum();
            if total > 0.0 {
                bins.iter_mut().for_each(|b| *b /= total);
            }
        };
        normalise(&mut h.luma);
        normalise(&mut h.u);
        normalise(&mut h.v);
        h
    }

    /// 0 = same colours, 1 = nothing in common.
    pub fn distance(&self, other: &Self) -> f32 {
        let half_l1 =
            |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / 2.0;
        0.6 * half_l1(&self.luma, &other.luma)
            + 0.2 * half_l1(&self.u, &other.u)
            + 0.2 * half_l1(&self.v, &other.v)
    }
}

/// First frame of every new shot. `histograms` sorted by frame; a gap in
/// the frame numbers is not compared across.
pub fn find_cuts(histograms: &[(FrameIdx, FrameHistogram)], fps: f64) -> Vec<FrameIdx> {
    let changes: Vec<(FrameIdx, f32)> = histograms
        .windows(2)
        .filter(|w| w[1].0 == w[0].0 + 1)
        .map(|w| (w[1].0, w[1].1.distance(&w[0].1)))
        .collect();
    let min_gap = (MIN_SHOT_SECS * fps).round().max(1.0) as FrameIdx;
    let mut cuts: Vec<FrameIdx> = Vec::new();
    for (i, &(frame, d)) in changes.iter().enumerate() {
        if d < MIN_DISTANCE {
            continue;
        }
        let around: Vec<f32> = changes
            [i.saturating_sub(WINDOW)..(i + WINDOW + 1).min(changes.len())]
            .iter()
            .enumerate()
            .filter(|(j, _)| *j + i.saturating_sub(WINDOW) != i)
            .map(|(_, c)| c.1)
            .collect();
        let mut sorted = around.clone();
        sorted.sort_by(f32::total_cmp);
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        if d < median * OVER_MEDIAN + 0.05 {
            continue;
        }
        match cuts.last() {
            Some(&last) if frame - last < min_gap => {}
            _ => cuts.push(frame),
        }
    }
    cuts
}
