//! Noise reduction for steady noise: a short-time spectral gate whose noise
//! floor is the quietest level of each bin over the last 1.5 s (minimum
//! statistics), so the pauses between the words teach it the noise. The gain never drops below a floor and is
//! smoothed in time and frequency, which keeps the "musical noise" (the
//! metallic warble of a plain spectral subtraction) out.

use vv_core::NoiseStrength;

use crate::spectrum::fft;

/// FFT frames are about 21 ms at any rate; the hop is a quarter of it.
pub fn frame_len(sample_rate: u32) -> usize {
    ((sample_rate as f32 * 0.021) as usize)
        .next_power_of_two()
        .clamp(256, 4096)
}

/// What the gate delays its output by, in frames: the mixer reads the track
/// that much ahead.
pub fn latency(sample_rate: u32) -> u64 {
    frame_len(sample_rate) as u64
}

#[derive(Debug, Clone, Copy)]
struct Curve {
    /// How many times the noise power is taken away.
    over: f32,
    /// Lowest gain, linear.
    floor: f32,
    release_s: f32,
}

impl Curve {
    fn of(strength: NoiseStrength) -> Self {
        let (over, floor_db, release_s) = match strength {
            NoiseStrength::Low => (1.5, -9.0, 0.06),
            NoiseStrength::Medium => (2.0, -15.0, 0.08),
            NoiseStrength::High => (3.0, -22.0, 0.10),
        };
        Self {
            over,
            floor: 10f32.powf(floor_db / 20.0),
            release_s,
        }
    }
}

/// The noise floor is the minimum over `SUB_WINDOWS` of `SUB_WINDOW_S`.
const SUB_WINDOWS: usize = 8;
const SUB_WINDOW_S: f32 = 0.19;
/// The minimum of a smoothed noise spectrum is about 5 dB below its mean.
const MIN_BIAS: f32 = 3.0;
const POWER_SMOOTH_S: f32 = 0.03;

pub struct DenoiseProcessor {
    curve: Curve,
    channels: usize,
    n: usize,
    hop: usize,
    window: Vec<f32>,
    /// Per channel, `n` each: the last `n` input samples.
    input: Vec<f32>,
    /// Per channel, `n` each: the overlap-add of the processed frames.
    acc: Vec<f32>,
    /// Per channel, `hop` each: what plays until the next frame.
    ready: Vec<f32>,
    fill: usize,
    spec_re: Vec<f32>,
    spec_im: Vec<f32>,
    power: Vec<f32>,
    gain: Vec<f32>,
    scratch: Vec<f32>,
    /// `SUB_WINDOWS` rows of `bins`: the minimum power of each past window.
    minima: Vec<f32>,
    current_min: Vec<f32>,
    minima_filled: usize,
    minima_next: usize,
    sub_frames: usize,
    sub_len: usize,
    /// FFT frames since the reset.
    seen: usize,
    power_keep: f32,
    release_keep: f32,
}

impl DenoiseProcessor {
    pub fn new(strength: NoiseStrength, sample_rate: u32, channels: u16) -> Self {
        Self::with_curve(Curve::of(strength), sample_rate, channels)
    }

    fn with_curve(curve: Curve, sample_rate: u32, channels: u16) -> Self {
        let n = frame_len(sample_rate);
        let hop = n / 4;
        let ch = channels.max(1) as usize;
        let bins = n / 2 + 1;
        let frames_per_s = sample_rate.max(1) as f32 / hop as f32;
        let window = (0..n)
            .map(|i| {
                let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
                hann.sqrt()
            })
            .collect();
        Self {
            curve,
            channels: ch,
            n,
            hop,
            window,
            input: vec![0.0; ch * n],
            acc: vec![0.0; ch * n],
            ready: vec![0.0; ch * hop],
            fill: 0,
            spec_re: vec![0.0; ch * n],
            spec_im: vec![0.0; ch * n],
            power: vec![0.0; bins],
            gain: vec![1.0; bins],
            scratch: vec![0.0; bins],
            minima: vec![0.0; SUB_WINDOWS * bins],
            current_min: vec![f32::MAX; bins],
            minima_filled: 0,
            minima_next: 0,
            sub_frames: 0,
            sub_len: ((SUB_WINDOW_S * frames_per_s) as usize).max(1),
            seen: 0,
            power_keep: (-1.0 / (POWER_SMOOTH_S * frames_per_s)).exp(),
            release_keep: (-1.0 / (curve.release_s * frames_per_s)).exp(),
        }
    }

    pub fn reset(&mut self) {
        self.input.fill(0.0);
        self.acc.fill(0.0);
        self.ready.fill(0.0);
        self.fill = 0;
        self.power.fill(0.0);
        self.gain.fill(1.0);
        self.current_min.fill(f32::MAX);
        self.minima_filled = 0;
        self.minima_next = 0;
        self.sub_frames = 0;
        self.seen = 0;
    }

    pub fn take_memory_from(&mut self, other: &Self) {
        if (self.n, self.channels) != (other.n, other.channels) {
            return;
        }
        self.input.copy_from_slice(&other.input);
        self.acc.copy_from_slice(&other.acc);
        self.ready.copy_from_slice(&other.ready);
        self.fill = other.fill;
        self.power.copy_from_slice(&other.power);
        self.gain.copy_from_slice(&other.gain);
        self.minima.copy_from_slice(&other.minima);
        self.current_min.copy_from_slice(&other.current_min);
        self.minima_filled = other.minima_filled;
        self.minima_next = other.minima_next;
        self.sub_frames = other.sub_frames;
        self.seen = other.seen;
    }

    /// In place, interleaved, delayed by `latency`. No allocations: it runs
    /// in the realtime callback.
    pub fn process(&mut self, samples: &mut [f32]) {
        let (n, hop) = (self.n, self.hop);
        for frame in samples.chunks_exact_mut(self.channels) {
            for (c, s) in frame.iter_mut().enumerate() {
                let x = *s;
                *s = self.ready[c * hop + self.fill];
                self.input[c * n + n - hop + self.fill] = x;
            }
            self.fill += 1;
            if self.fill == hop {
                self.fill = 0;
                self.run_frame();
            }
        }
    }

    fn run_frame(&mut self) {
        let (n, hop) = (self.n, self.hop);
        let bins = n / 2 + 1;
        self.scratch.fill(0.0);
        for c in 0..self.channels {
            let re = &mut self.spec_re[c * n..(c + 1) * n];
            let im = &mut self.spec_im[c * n..(c + 1) * n];
            for ((r, x), w) in re.iter_mut().zip(&self.input[c * n..]).zip(&self.window) {
                *r = x * w;
            }
            im.fill(0.0);
            fft(re, im);
            for k in 0..bins {
                self.scratch[k] += re[k] * re[k] + im[k] * im[k];
            }
        }
        self.update_gains();
        for c in 0..self.channels {
            let re = &mut self.spec_re[c * n..(c + 1) * n];
            let im = &mut self.spec_im[c * n..(c + 1) * n];
            for k in 0..n {
                let g = self.gain[if k < bins { k } else { n - k }];
                re[k] *= g;
                // Conjugated: a forward FFT of the conjugate is the inverse.
                im[k] *= -g;
            }
            fft(re, im);
            // sqrt-Hann in and out at a quarter hop overlap-adds to 2.
            let scale = 0.5 / n as f32;
            let acc = &mut self.acc[c * n..(c + 1) * n];
            for ((a, r), w) in acc.iter_mut().zip(re.iter()).zip(&self.window) {
                *a += r * w * scale;
            }
            self.ready[c * hop..(c + 1) * hop].copy_from_slice(&acc[..hop]);
            acc.copy_within(hop.., 0);
            acc[n - hop..].fill(0.0);
            self.input[c * n..(c + 1) * n].copy_within(hop.., 0);
        }
    }

    /// `scratch`: the power of each bin, summed over the channels; one gain
    /// for all of them keeps the stereo image still.
    fn update_gains(&mut self) {
        let bins = self.power.len();
        let norm = 1.0 / self.channels as f32;
        self.seen += 1;
        // The first frames still hold the silence of the reset.
        let warm = 4;
        if self.seen < warm {
            return;
        }
        for k in 0..bins {
            let p = self.scratch[k] * norm;
            self.power[k] = if self.seen == warm {
                p
            } else {
                self.power[k] * self.power_keep + p * (1.0 - self.power_keep)
            };
            self.current_min[k] = self.current_min[k].min(self.power[k]);
        }
        self.sub_frames += 1;
        if self.sub_frames == self.sub_len {
            let row = self.minima_next * bins;
            self.minima[row..row + bins].copy_from_slice(&self.current_min);
            self.current_min.fill(f32::MAX);
            self.minima_next = (self.minima_next + 1) % SUB_WINDOWS;
            self.minima_filled = (self.minima_filled + 1).min(SUB_WINDOWS);
            self.sub_frames = 0;
        }
        if self.minima_filled == 0 {
            return;
        }
        let curve = self.curve;
        for k in 0..bins {
            let noise = (0..self.minima_filled)
                .map(|w| self.minima[w * bins + k])
                .fold(self.current_min[k], f32::min)
                * MIN_BIAS;
            let ratio = curve.over * noise / self.power[k].max(1e-14);
            self.scratch[k] = (1.0 - ratio).max(0.0).sqrt().max(curve.floor);
        }
        for k in 0..bins {
            let lo = self.scratch[k.saturating_sub(1)];
            let hi = self.scratch[(k + 1).min(bins - 1)];
            let target = 0.25 * lo + 0.5 * self.scratch[k] + 0.25 * hi;
            let gain = &mut self.gain[k];
            *gain = if target > *gain {
                target
            } else {
                *gain * self.release_keep + target * (1.0 - self.release_keep)
            };
        }
    }
}

impl crate::effect::Effect for DenoiseProcessor {
    fn process(&mut self, samples: &mut [f32]) -> Option<crate::dynamics::BandActivity> {
        DenoiseProcessor::process(self, samples);
        None
    }

    fn reset(&mut self) {
        DenoiseProcessor::reset(self);
    }

    fn take_memory_from(&mut self, previous: &dyn crate::effect::Effect) {
        if let Some(previous) = previous.as_any().downcast_ref() {
            DenoiseProcessor::take_memory_from(self, previous);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "tests/denoise.rs"]
mod tests;
