//! Export motion blur, ported from f0e/blur (GPL-3.0): every timeline frame
//! is composed, optionally interpolated to a higher frame rate along its
//! optical flow (duplicates first replaced by interpolated frames), and each
//! output frame is the weighted average of the high-rate frames around it.

use std::collections::{BTreeMap, HashMap};

use vv_core::{FrameIdx, Rational};
use vv_render::{BlendSum, Compositor, FramePairFlow, PooledTexture};

#[derive(Clone, Debug, PartialEq)]
pub struct MotionBlurSettings {
    /// 0 = no blur, 1 = the whole interval between output frames.
    pub amount: f32,
    pub weighting: Weighting,
    pub gaussian: GaussianParams,
    /// Frames are raised to this power before blending and back after:
    /// above 1 the bright parts dominate the trail.
    pub gamma: f32,
    /// `None` blends only the timeline's own frames.
    pub interpolation: Option<FpsTarget>,
    pub deduplicate: Option<Deduplicate>,
}

impl Default for MotionBlurSettings {
    /// blur's defaults.
    fn default() -> Self {
        Self {
            amount: 1.0,
            weighting: Weighting::Equal,
            gaussian: GaussianParams::default(),
            gamma: 1.0,
            interpolation: Some(FpsTarget::Fixed(600)),
            deduplicate: Some(Deduplicate::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FpsTarget {
    Fixed(u32),
    /// Of the timeline's frame rate.
    Multiplier(f32),
}

impl FpsTarget {
    /// blur's syntax: `600` or `5x`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(multiplier) = text.strip_suffix(['x', 'X']) {
            let multiplier: f32 = multiplier.trim().parse().ok()?;
            return (multiplier.is_finite() && multiplier > 0.0)
                .then_some(Self::Multiplier(multiplier));
        }
        let fps: u32 = text.parse().ok()?;
        (fps > 0).then_some(Self::Fixed(fps))
    }

    pub fn fps(self, timeline_fps: Rational) -> f64 {
        match self {
            Self::Fixed(fps) => f64::from(fps),
            Self::Multiplier(m) => timeline_fps.as_f64() * f64::from(m),
        }
    }
}

impl std::fmt::Display for FpsTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(fps) => write!(f, "{fps}"),
            Self::Multiplier(m) => write!(f, "{m}x"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deduplicate {
    /// How many frames ahead a changed frame is looked for.
    pub range: u32,
    /// Mean luma difference (0..1) under which a frame is a duplicate.
    pub threshold: f32,
}

impl Default for Deduplicate {
    fn default() -> Self {
        Self {
            range: 2,
            threshold: 0.001,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Weighting {
    Equal,
    GaussianSym,
    Vegas,
    Pyramid,
    Gaussian,
    Ascending,
    Descending,
    GaussianReverse,
    /// Stretched over the blended frames.
    Custom(Vec<f32>),
}

impl Weighting {
    pub const PRESETS: [Self; 8] = [
        Self::Equal,
        Self::GaussianSym,
        Self::Vegas,
        Self::Pyramid,
        Self::Gaussian,
        Self::Ascending,
        Self::Descending,
        Self::GaussianReverse,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::GaussianSym => "gaussian_sym",
            Self::Vegas => "vegas",
            Self::Pyramid => "pyramid",
            Self::Gaussian => "gaussian",
            Self::Ascending => "ascending",
            Self::Descending => "descending",
            Self::GaussianReverse => "gaussian_reverse",
            Self::Custom(_) => "custom",
        }
    }

    /// Comma-separated weights, as blur takes them: `5, 3, 3, 2, 1`.
    pub fn parse_custom(text: &str) -> Option<Self> {
        let weights: Vec<f32> = text
            .split(',')
            .map(|w| w.trim().parse::<f32>().ok().filter(|w| w.is_finite()))
            .collect::<Option<_>>()?;
        let usable = !weights.is_empty() && weights.iter().any(|w| *w != 0.0);
        usable.then_some(Self::Custom(weights))
    }

    pub fn uses_gaussian(&self) -> bool {
        matches!(
            self,
            Self::Gaussian | Self::GaussianReverse | Self::GaussianSym
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaussianParams {
    pub mean: f32,
    pub std_dev: f32,
    /// The x range the frames are spread over.
    pub bound: (f32, f32),
}

impl Default for GaussianParams {
    fn default() -> Self {
        Self {
            mean: 2.0,
            std_dev: 1.0,
            bound: (0.0, 2.0),
        }
    }
}

fn normalize(weights: impl IntoIterator<Item = f64>) -> Vec<f32> {
    let mut weights: Vec<f64> = weights.into_iter().collect();
    let min = weights.iter().copied().fold(f64::INFINITY, f64::min);
    if min < 0.0 {
        weights.iter_mut().for_each(|w| *w = *w - min + 1.0);
    }
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return vec![1.0 / weights.len().max(1) as f32; weights.len()];
    }
    weights.iter().map(|w| (w / total) as f32).collect()
}

fn scale_range(n: usize, start: f64, end: f64) -> Vec<f64> {
    if n <= 1 {
        return vec![start; n];
    }
    (0..n)
        .map(|i| start + i as f64 * (end - start) / (n - 1) as f64)
        .collect()
}

fn gaussian(frames: usize, mean: f64, std_dev: f64, bound: (f64, f64)) -> Vec<f32> {
    let std_dev = if std_dev == 0.0 { 1e-6 } else { std_dev };
    normalize(
        scale_range(frames, bound.0, bound.1)
            .into_iter()
            .map(|x| (-(x - mean).powi(2) / (2.0 * std_dev * std_dev)).exp()),
    )
}

/// The weights of `frames` blended frames, oldest first, summing to 1.
pub fn weights(frames: usize, weighting: &Weighting, gaussian_params: &GaussianParams) -> Vec<f32> {
    let n = frames as f64;
    let GaussianParams {
        mean,
        std_dev,
        bound,
    } = *gaussian_params;
    let (mean, std_dev, bound) = (
        f64::from(mean),
        f64::from(std_dev),
        (f64::from(bound.0), f64::from(bound.1)),
    );
    match weighting {
        Weighting::Equal => vec![1.0 / frames.max(1) as f32; frames],
        Weighting::Ascending => normalize((1..=frames).map(|i| i as f64)),
        Weighting::Descending => normalize((1..=frames).rev().map(|i| i as f64)),
        Weighting::Pyramid => {
            let half = (n - 1.0) / 2.0;
            normalize((0..frames).map(|i| half - (i as f64 - half).abs() + 1.0))
        }
        Weighting::Gaussian => gaussian(frames, mean, std_dev, bound),
        Weighting::GaussianReverse => {
            let mut w = gaussian(frames, mean, std_dev, bound);
            w.reverse();
            w
        }
        Weighting::GaussianSym => {
            let max = bound.0.abs().max(bound.1.abs());
            gaussian(frames, 0.0, std_dev, (-max, max))
        }
        Weighting::Vegas => {
            if frames.is_multiple_of(2) && frames >= 2 {
                let mut w = vec![2.0; frames];
                w[0] = 1.0;
                w[frames - 1] = 1.0;
                normalize(w)
            } else {
                normalize(vec![1.0; frames])
            }
        }
        Weighting::Custom(custom) => {
            if custom.is_empty() {
                return weights(frames, &Weighting::Equal, gaussian_params);
            }
            let indices = scale_range(frames, 0.0, custom.len() as f64 - 0.1);
            normalize(
                indices
                    .into_iter()
                    .map(|i| f64::from(custom[(i as usize).min(custom.len() - 1)])),
            )
        }
    }
}

/// The rate frames are blended at, and how many make each output frame
/// (1 = no blur).
pub fn blend_summary(
    settings: &MotionBlurSettings,
    timeline_fps: Rational,
    output_fps: Rational,
) -> (f64, usize) {
    let plan = Plan::new(settings, timeline_fps, output_fps, 1);
    (
        timeline_fps.as_f64() * f64::from(plan.multiplier),
        plan.weights.len(),
    )
}

/// How the output frames map onto the high-rate frames.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    /// High-rate frames per timeline frame.
    pub multiplier: u32,
    /// Odd count, centred on the output frame.
    pub weights: Vec<f32>,
    pub source_frames: usize,
    timeline_fps: Rational,
    output_fps: Rational,
}

impl Plan {
    pub(crate) fn new(
        settings: &MotionBlurSettings,
        timeline_fps: Rational,
        output_fps: Rational,
        source_frames: usize,
    ) -> Self {
        let multiplier = settings
            .interpolation
            .map(|target| (target.fps(timeline_fps) / timeline_fps.as_f64()).round())
            .filter(|m| m.is_finite() && *m > 1.0)
            .map_or(1, |m| m.min(64.0) as u32);
        let high_fps = timeline_fps.as_f64() * f64::from(multiplier);
        let frame_gap = (high_fps / output_fps.as_f64()).floor();
        let mut blur_frames = (frame_gap * f64::from(settings.amount.max(0.0))).floor() as usize;
        let weights = if blur_frames == 0 {
            vec![1.0]
        } else {
            if blur_frames.is_multiple_of(2) {
                blur_frames += 1;
            }
            weights(blur_frames, &settings.weighting, &settings.gaussian)
        };
        Self {
            multiplier,
            weights,
            source_frames,
            timeline_fps,
            output_fps,
        }
    }

    fn radius(&self) -> i64 {
        (self.weights.len() / 2) as i64
    }

    fn last_high(&self) -> i64 {
        (self.source_frames.max(1) as i64 - 1) * i64::from(self.multiplier)
    }

    /// The high-rate frame output frame `n` is centred on.
    pub(crate) fn center(&self, n: FrameIdx) -> i64 {
        let num = i128::from(n)
            * i128::from(self.timeline_fps.num)
            * i128::from(self.multiplier)
            * i128::from(self.output_fps.den);
        let den = i128::from(self.timeline_fps.den) * i128::from(self.output_fps.num);
        ((num / den.max(1)) as i64).min(self.last_high())
    }

    /// High-rate frames of output frame `n`, with their weight, clamped to the
    /// clip as blur does (the edge frames repeat).
    pub(crate) fn taps(&self, n: FrameIdx) -> impl Iterator<Item = (i64, f32)> + '_ {
        let center = self.center(n);
        let last = self.last_high();
        let r = self.radius();
        self.weights
            .iter()
            .enumerate()
            .map(move |(i, w)| ((center + i as i64 - r).clamp(0, last), *w))
    }

    /// The last timeline frame output frame `n` reads.
    fn last_source(&self, n: FrameIdx) -> usize {
        let high = (self.center(n) + self.radius()).min(self.last_high());
        (high as u64).div_ceil(u64::from(self.multiplier)) as usize
    }

    fn first_source(&self, n: FrameIdx) -> usize {
        let high = (self.center(n) - self.radius()).max(0);
        (high / i64::from(self.multiplier)) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Sample {
    Frame(usize),
    Between { a: usize, b: usize, t: f32 },
}

/// Which frames are real motion (anchors) and which are duplicates to be
/// replaced by interpolation, as blur's `fill_drops`: a duplicate is
/// replaced only if a changed frame follows within `range`.
#[derive(Debug, Default)]
pub(crate) struct Anchors {
    /// Per received frame: differs from the one before.
    changed: Vec<bool>,
    range: usize,
}

impl Anchors {
    pub(crate) fn new(range: usize) -> Self {
        Self {
            changed: Vec::new(),
            range,
        }
    }

    pub(crate) fn push(&mut self, changed: bool) {
        let first = self.changed.is_empty();
        self.changed.push(changed || first);
    }

    /// `Some` once frames up to `i + range` are known, or at `end`.
    fn is_anchor(&self, i: usize, end: bool) -> Option<bool> {
        if self.changed[i] {
            return Some(true);
        }
        let last = (i + self.range).min(self.changed.len() - 1);
        if self.changed[i + 1..=last].iter().any(|c| *c) {
            return Some(false);
        }
        (end || i + self.range < self.changed.len()).then_some(true)
    }

    /// The anchors around position `h / multiplier`. Asked only once enough
    /// frames are known (`Engine::ready`).
    pub(crate) fn sample(&self, h: i64, multiplier: u32, end: bool) -> Sample {
        let m = i64::from(multiplier);
        let i = (h / m) as usize;
        let k = h % m;
        let anchor = |j: usize| self.is_anchor(j, end).unwrap_or(true);
        if k == 0 && anchor(i) {
            return Sample::Frame(i);
        }
        let a = (0..=i).rev().find(|&j| anchor(j)).unwrap_or(0);
        let Some(b) = (i + 1..self.changed.len()).find(|&j| anchor(j)) else {
            return Sample::Frame(a);
        };
        let t = ((i - a) as i64 * m + k) as f32 / ((b - a) as i64 * m) as f32;
        Sample::Between { a, b, t }
    }

    /// The latest frame known to be an anchor at or before `i`: nothing
    /// before it is needed from `i` on.
    fn anchor_at_or_before(&self, i: usize) -> usize {
        let i = i.min(self.changed.len().saturating_sub(1));
        (0..=i)
            .rev()
            .find(|&j| self.is_anchor(j, false) == Some(true))
            .unwrap_or(0)
    }
}

/// Turns the composed timeline frames, pushed in order, into the blurred
/// output frames (I420), on the export's compose thread.
pub(crate) struct Engine<'a> {
    compositor: &'a Compositor,
    plan: Plan,
    gamma: f32,
    deduplicate: Option<Deduplicate>,
    anchors: Anchors,
    frames: BTreeMap<usize, PooledTexture>,
    flows: HashMap<(usize, usize), FramePairFlow>,
    received: usize,
    next_output: FrameIdx,
    total_outputs: FrameIdx,
    sum: Option<BlendSum>,
    size: (u32, u32),
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        compositor: &'a Compositor,
        settings: &MotionBlurSettings,
        timeline_fps: Rational,
        output_fps: Rational,
        source_frames: usize,
        total_outputs: FrameIdx,
        size: (u32, u32),
    ) -> Self {
        let deduplicate = settings.deduplicate.filter(|d| d.range > 0);
        Self {
            compositor,
            plan: Plan::new(settings, timeline_fps, output_fps, source_frames),
            gamma: if settings.gamma > 0.0 {
                settings.gamma
            } else {
                1.0
            },
            anchors: Anchors::new(deduplicate.map_or(0, |d| d.range as usize)),
            deduplicate,
            frames: BTreeMap::new(),
            flows: HashMap::new(),
            received: 0,
            next_output: 0,
            total_outputs,
            sum: None,
            size,
        }
    }

    /// The next timeline frame; returns the output frames it completes.
    pub(crate) fn push(&mut self, frame: PooledTexture) -> Vec<Vec<u8>> {
        let changed = match (
            self.deduplicate,
            self.frames.get(&self.received.wrapping_sub(1)),
        ) {
            (Some(dedup), Some(previous)) => {
                self.compositor.frame_difference(previous, &frame) >= dedup.threshold
            }
            _ => true,
        };
        self.anchors.push(changed);
        self.frames.insert(self.received, frame);
        self.received += 1;
        self.drain(false)
    }

    /// After the last timeline frame: the output frames still due.
    pub(crate) fn finish(&mut self) -> Vec<Vec<u8>> {
        self.plan.source_frames = self.plan.source_frames.min(self.received);
        self.drain(true)
    }

    fn ready(&self, n: FrameIdx) -> bool {
        let needed = self.plan.last_source(n) + self.anchors.range;
        needed < self.received
    }

    fn drain(&mut self, end: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while self.next_output < self.total_outputs
            && self.received > 0
            && (end || self.ready(self.next_output))
        {
            out.push(self.render(self.next_output, end));
            self.next_output += 1;
            self.evict();
        }
        out
    }

    fn evict(&mut self) {
        if self.next_output >= self.total_outputs {
            return;
        }
        let first = self.plan.first_source(self.next_output);
        let keep_from = self
            .anchors
            .anchor_at_or_before(first)
            .min(self.received.saturating_sub(1));
        self.frames = self.frames.split_off(&keep_from);
        self.flows.retain(|(a, _), _| *a >= keep_from);
    }

    fn render(&mut self, n: FrameIdx, end: bool) -> Vec<u8> {
        let mut samples: Vec<(Sample, f32)> = Vec::with_capacity(self.plan.weights.len());
        for (h, w) in self.plan.taps(n) {
            let sample = self.anchors.sample(h, self.plan.multiplier, end);
            match samples.iter_mut().find(|(s, _)| *s == sample) {
                Some((_, weight)) => *weight += w,
                None => samples.push((sample, w)),
            }
        }
        if let [(Sample::Frame(i), _)] = samples[..] {
            return self.to_i420(&self.frames[&i]);
        }
        let mut sum = match self.sum.take() {
            Some(mut sum) => {
                sum.reset(self.gamma);
                sum
            }
            None => self.compositor.blend_sum(self.size, self.gamma),
        };
        for (sample, weight) in samples {
            match sample {
                Sample::Frame(i) => self
                    .compositor
                    .blend_add(&mut sum, &self.frames[&i], weight),
                Sample::Between { a, b, t } => {
                    let (fa, fb) = (&self.frames[&a], &self.frames[&b]);
                    let flow = self
                        .flows
                        .entry((a, b))
                        .or_insert_with(|| self.compositor.frame_flow(fa, fb));
                    let frame = self.compositor.interpolate_frame(fa, fb, flow, t);
                    self.compositor.blend_add(&mut sum, &frame, weight);
                }
            }
        }
        let blended = self.compositor.blend_resolve(&sum);
        self.sum = Some(sum);
        self.to_i420(&blended)
    }

    fn to_i420(&self, texture: &vv_render::wgpu::Texture) -> Vec<u8> {
        let (width, height) = self.size;
        let layer = vv_render::Layer::new(
            vv_render::LayerContent::Texture {
                texture,
                source_size: (width, height),
            },
            vv_core::Transform::default(),
        );
        self.compositor
            .render_layers_i420(&[layer], vv_render::OutputFrame::exact(width, height))
    }
}

#[cfg(test)]
#[path = "tests/motion_blur.rs"]
mod tests;
