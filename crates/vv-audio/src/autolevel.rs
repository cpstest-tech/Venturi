//! Automatic level: a slow gain that keeps the short-term loudness (K
//! weighted, BS.1770) near a target, so a voice that drifts away from the
//! microphone or a quiet sentence comes back up. Silence and room noise
//! below the gate are not raised. No latency: it follows what it heard.

use vv_core::{AutoLevel, LevelSpeed};

use crate::loudness::{Biquad, k_weighting};

/// The gain changes every 10 ms, ramped sample by sample in between.
const STEP_S: f32 = 0.01;
const MAX_WINDOW_STEPS: usize = 100;
/// Right after a reset: the first window settles fast instead of fading in.
const WARM_TAU_S: f32 = 0.15;
const MIN_WARM_STEPS: usize = 10;
/// Safety against the peaks a boost brings over full scale.
const CEILING: f32 = 0.966;
const SAFETY_RELEASE_S: f32 = 0.06;
const ABSOLUTE_GATE_LUFS: f32 = -50.0;

fn speed_params(speed: LevelSpeed) -> (usize, f32) {
    // Measured window (steps) and time constant of the gain (s).
    match speed {
        LevelSpeed::Slow => (100, 3.0),
        LevelSpeed::Medium => (60, 1.5),
        LevelSpeed::Fast => (40, 0.6),
    }
}

pub struct AutoLevelProcessor {
    params: AutoLevel,
    channels: usize,
    step: usize,
    window: usize,
    filters: Vec<[Biquad; 2]>,
    /// Mean square of each past step, `MAX_WINDOW_STEPS` long.
    ring: Vec<f64>,
    ring_next: usize,
    ring_filled: usize,
    acc: f64,
    pos: usize,
    gain_db: f32,
    gain_from: f32,
    gain_to: f32,
    safety: f32,
    keep_up: f32,
    keep_down: f32,
    keep_warm: f32,
    safety_release: f32,
}

impl AutoLevelProcessor {
    pub fn new(params: &AutoLevel, sample_rate: u32, channels: u16) -> Self {
        let rate = sample_rate.max(1) as f32;
        let step = ((rate * STEP_S) as usize).max(1);
        let (window, tau) = speed_params(params.speed);
        let keep = |tau: f32| (-(step as f32) / (rate * tau)).exp();
        let ch = channels.max(1) as usize;
        Self {
            params: *params,
            channels: ch,
            step,
            window,
            filters: (0..ch).map(|_| k_weighting(rate as f64)).collect(),
            ring: vec![0.0; MAX_WINDOW_STEPS],
            ring_next: 0,
            ring_filled: 0,
            acc: 0.0,
            pos: 0,
            gain_db: 0.0,
            gain_from: 1.0,
            gain_to: 1.0,
            safety: 1.0,
            keep_up: keep(tau),
            // Down faster: a loud passage must not blast through.
            keep_down: keep(tau / 4.0),
            keep_warm: keep(WARM_TAU_S),
            safety_release: (-1.0 / (rate * SAFETY_RELEASE_S)).exp(),
        }
    }

    pub fn reset(&mut self) {
        for filters in &mut self.filters {
            for f in filters.iter_mut() {
                f.clear();
            }
        }
        self.ring.fill(0.0);
        self.ring_next = 0;
        self.ring_filled = 0;
        self.acc = 0.0;
        self.pos = 0;
        self.gain_db = 0.0;
        self.gain_from = 1.0;
        self.gain_to = 1.0;
        self.safety = 1.0;
    }

    pub fn take_memory_from(&mut self, other: &Self) {
        if (self.channels, self.step) != (other.channels, other.step) {
            return;
        }
        self.filters.clone_from_slice(&other.filters);
        self.ring.copy_from_slice(&other.ring);
        self.ring_next = other.ring_next % self.window;
        self.ring_filled = other.ring_filled.min(self.window);
        self.acc = other.acc;
        self.pos = other.pos;
        self.gain_db = other.gain_db;
        self.gain_from = other.gain_from;
        self.gain_to = other.gain_to;
        self.safety = other.safety;
    }

    /// The gain it applies now, dB.
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// In place, interleaved. No allocations: it runs in the realtime
    /// callback.
    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.chunks_exact_mut(self.channels) {
            let mut power = 0.0f64;
            for (x, filters) in frame.iter().zip(&mut self.filters) {
                let shelved = filters[0].process(*x as f64);
                let y = filters[1].process(shelved);
                power += y * y;
            }
            self.acc += power;
            let t = self.pos as f32 / self.step as f32;
            let gain = self.gain_from + (self.gain_to - self.gain_from) * t;
            let peak = frame.iter().fold(0.0f32, |p, x| p.max(x.abs())) * gain;
            self.safety = if peak * self.safety > CEILING {
                CEILING / peak
            } else {
                1.0 - (1.0 - self.safety) * self.safety_release
            };
            let g = gain * self.safety;
            frame.iter_mut().for_each(|x| *x *= g);
            self.pos += 1;
            if self.pos == self.step {
                self.end_step();
            }
        }
    }

    fn end_step(&mut self) {
        self.ring[self.ring_next] = self.acc / self.step as f64;
        self.ring_next = (self.ring_next + 1) % self.window;
        self.ring_filled = (self.ring_filled + 1).min(self.window);
        self.acc = 0.0;
        self.pos = 0;
        self.gain_from = self.gain_to;
        if self.ring_filled < MIN_WARM_STEPS.min(self.window) {
            return;
        }
        let mean = self.ring[..self.ring_filled].iter().sum::<f64>() / self.ring_filled as f64;
        let loudness = -0.691 + 10.0 * mean.max(1e-12).log10() as f32;
        let AutoLevel {
            target_lufs,
            max_gain_db,
            ..
        } = self.params;
        let gate = ABSOLUTE_GATE_LUFS.max(target_lufs - max_gain_db - 8.0);
        if loudness < gate {
            return;
        }
        let wanted = (target_lufs - loudness).clamp(-max_gain_db, max_gain_db);
        let keep = if self.ring_filled < self.window {
            self.keep_warm
        } else if wanted < self.gain_db {
            self.keep_down
        } else {
            self.keep_up
        };
        self.gain_db = wanted + (self.gain_db - wanted) * keep;
        self.gain_to = 10f32.powf(self.gain_db / 20.0);
    }
}

impl crate::effect::Effect for AutoLevelProcessor {
    fn process(&mut self, samples: &mut [f32]) -> Option<crate::dynamics::BandActivity> {
        AutoLevelProcessor::process(self, samples);
        None
    }

    fn reset(&mut self) {
        AutoLevelProcessor::reset(self);
    }

    fn take_memory_from(&mut self, previous: &dyn crate::effect::Effect) {
        if let Some(previous) = previous.as_any().downcast_ref() {
            AutoLevelProcessor::take_memory_from(self, previous);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "tests/autolevel.rs"]
mod tests;
