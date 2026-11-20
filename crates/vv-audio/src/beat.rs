//! Tempo and beat positions of a music track: spectral-flux onsets, tempo by
//! autocorrelation weighted around 120 BPM, beats by dynamic programming
//! (Ellis 2007, the method behind librosa's `beat_track`).

use crate::spectrum::fft;

/// Mono input rate expected by `OnsetDetector`.
pub const SAMPLE_RATE: u32 = 22_050;
const FFT: usize = 2048;
const HOP: usize = 512;
const BANDS: usize = 40;
const MIN_SECONDS: f64 = 5.0;
const BPM_RANGE: (f64, f64) = (50.0, 220.0);
const PRIOR_BPM: f64 = 120.0;
/// Penalty of an inter-beat interval away from the period (librosa's default).
const TIGHTNESS: f64 = 100.0;
/// Peak of the tempo autocorrelation against lag 0, under which there is no pulse.
const MIN_CLARITY: f32 = 0.12;
/// Mean deviation of the local tempos from the global one marking a drifting tempo.
const MAX_DRIFT: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatError {
    TooShort,
    Silent,
    NoRhythm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Beats {
    pub bpm: f64,
    /// Seconds from the start of the analysed audio.
    pub times: Vec<f64>,
    /// 0..=1, how clearly the tempo stands out.
    pub clarity: f32,
    /// `false`: the tempo drifts, the beats follow it but may wander.
    pub stable: bool,
}

/// Fed with mono samples at `SAMPLE_RATE`, in chunks of any size.
pub struct OnsetDetector {
    window: Vec<f32>,
    band_of: Vec<Option<usize>>,
    buffer: Vec<f32>,
    previous: Vec<f32>,
    envelope: Vec<f32>,
    peak: f32,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Default for OnsetDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl OnsetDetector {
    pub fn new() -> Self {
        let window = (0..FFT)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT as f32).cos())
            .collect();
        let (low, high) = (30.0_f64, 11_000.0_f64);
        let band_of = (0..=FFT / 2)
            .map(|k| {
                let hz = k as f64 * f64::from(SAMPLE_RATE) / FFT as f64;
                (hz >= low && hz < high)
                    .then(|| ((hz / low).ln() / (high / low).ln() * BANDS as f64) as usize)
            })
            .collect();
        Self {
            window,
            band_of,
            buffer: Vec::with_capacity(FFT * 2),
            previous: Vec::new(),
            envelope: Vec::new(),
            peak: 0.0,
            re: vec![0.0; FFT],
            im: vec![0.0; FFT],
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.peak = self.peak.max(s.abs());
        }
        self.buffer.extend_from_slice(samples);
        let mut start = 0;
        while self.buffer.len() - start >= FFT {
            self.frame(start);
            start += HOP;
        }
        self.buffer.drain(..start);
    }

    fn frame(&mut self, start: usize) {
        for (i, (re, im)) in self.re.iter_mut().zip(&mut self.im).enumerate() {
            *re = self.buffer[start + i] * self.window[i];
            *im = 0.0;
        }
        fft(&mut self.re, &mut self.im);
        let mut bands = vec![0.0_f32; BANDS];
        for (k, band) in self.band_of.iter().enumerate() {
            if let Some(b) = band {
                bands[*b] += (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt();
            }
        }
        for b in &mut bands {
            *b = (1.0 + 10.0 * *b / FFT as f32).ln();
        }
        let flux = if self.previous.is_empty() {
            0.0
        } else {
            bands
                .iter()
                .zip(&self.previous)
                .map(|(b, p)| (b - p).max(0.0))
                .sum()
        };
        self.envelope.push(flux);
        self.previous = bands;
    }

    /// Seconds analysed so far.
    pub fn seconds(&self) -> f64 {
        (self.envelope.len() * HOP) as f64 / f64::from(SAMPLE_RATE)
    }

    pub fn finish(self) -> Result<Beats, BeatError> {
        if self.seconds() < MIN_SECONDS {
            return Err(BeatError::TooShort);
        }
        if self.peak < 1e-4 {
            return Err(BeatError::Silent);
        }
        track(&self.envelope)
    }
}

const FRAME_RATE: f64 = SAMPLE_RATE as f64 / HOP as f64;

fn frame_seconds(frame: usize) -> f64 {
    (frame * HOP + FFT / 2) as f64 / f64::from(SAMPLE_RATE)
}

/// Rhythmic part of the envelope: local mean removed, unit deviation.
fn normalize(envelope: &[f32]) -> Option<Vec<f32>> {
    let half = (FRAME_RATE * 0.5) as usize;
    let mut prefix = vec![0.0_f64; envelope.len() + 1];
    for (i, v) in envelope.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(*v);
    }
    let out: Vec<f32> = (0..envelope.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(envelope.len()));
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (f64::from(envelope[i]) - mean).max(0.0) as f32
        })
        .collect();
    let mean = out.iter().map(|v| f64::from(*v)).sum::<f64>() / out.len().max(1) as f64;
    let var = out
        .iter()
        .map(|v| (f64::from(*v) - mean).powi(2))
        .sum::<f64>()
        / out.len().max(1) as f64;
    let std = var.sqrt();
    (std > 1e-9).then(|| out.iter().map(|v| (f64::from(*v) / std) as f32).collect())
}

fn autocorrelation(env: &[f32], lags: std::ops::RangeInclusive<usize>) -> Vec<f32> {
    let mean = env.iter().sum::<f32>() / env.len().max(1) as f32;
    let centred: Vec<f32> = env.iter().map(|v| v - mean).collect();
    (0..=*lags.end())
        .map(|lag| {
            if lag != 0 && !lags.contains(&lag) || lag >= centred.len() {
                return 0.0;
            }
            centred[lag..]
                .iter()
                .zip(&centred)
                .map(|(a, b)| a * b)
                .sum::<f32>()
                / (centred.len() - lag) as f32
        })
        .collect()
}

fn lag_range() -> std::ops::RangeInclusive<usize> {
    let lag = |bpm: f64| (60.0 * FRAME_RATE / bpm).round() as usize;
    lag(BPM_RANGE.1)..=lag(BPM_RANGE.0)
}

/// Best period in frames (fractional), and its clarity.
fn tempo(env: &[f32], lags: std::ops::RangeInclusive<usize>, prior: bool) -> Option<(f64, f32)> {
    let ac = autocorrelation(env, lags.clone());
    if ac[0] <= 0.0 {
        return None;
    }
    let weight = |lag: usize| {
        if !prior {
            return 1.0;
        }
        let bpm = 60.0 * FRAME_RATE / lag as f64;
        (-0.5 * (bpm / PRIOR_BPM).log2().powi(2)).exp() as f32
    };
    let best = lags
        .clone()
        .filter(|&l| l < ac.len())
        .max_by(|&a, &b| (ac[a] * weight(a)).total_cmp(&(ac[b] * weight(b))))?;
    let refined = if best > *lags.start() && best + 1 < ac.len() {
        let (a, b, c) = (ac[best - 1], ac[best], ac[best + 1]);
        let d = a - 2.0 * b + c;
        if d.abs() > 1e-12 {
            best as f64 + f64::from(0.5 * (a - c) / d).clamp(-0.5, 0.5)
        } else {
            best as f64
        }
    } else {
        best as f64
    };
    Some((refined, (ac[best] / ac[0]).clamp(0.0, 1.0)))
}

/// Mean relative deviation of the tempo in 12 s windows from `period`.
fn drift(env: &[f32], period: f64) -> f64 {
    let window = (12.0 * FRAME_RATE) as usize;
    let step = window / 2;
    if env.len() < window * 3 / 2 {
        return 0.0;
    }
    let lags = ((period * 0.75) as usize).max(1)..=(period * 1.25).ceil() as usize;
    let deviations: Vec<f64> = (0..=env.len() - window)
        .step_by(step)
        .filter_map(|start| tempo(&env[start..start + window], lags.clone(), false))
        .map(|(local, _)| (local - period).abs() / period)
        .collect();
    deviations.iter().sum::<f64>() / deviations.len().max(1) as f64
}

fn track(envelope: &[f32]) -> Result<Beats, BeatError> {
    let env = normalize(envelope).ok_or(BeatError::Silent)?;
    let (period, clarity) = tempo(&env, lag_range(), true).ok_or(BeatError::NoRhythm)?;
    if clarity < MIN_CLARITY {
        return Err(BeatError::NoRhythm);
    }
    let stable = drift(&env, period) <= MAX_DRIFT;

    let reach = period.round() as isize;
    let gauss: Vec<f32> = (-reach..=reach)
        .map(|i| (-0.5 * (i as f64 * 32.0 / period).powi(2)).exp() as f32)
        .collect();
    let local: Vec<f32> = (0..env.len() as isize)
        .map(|i| {
            gauss
                .iter()
                .enumerate()
                .filter_map(|(k, g)| {
                    let j = i + k as isize - reach;
                    (j >= 0 && (j as usize) < env.len()).then(|| g * env[j as usize])
                })
                .sum()
        })
        .collect();

    let mut cumulative = vec![0.0_f64; env.len()];
    let mut back = vec![usize::MAX; env.len()];
    let (near, far) = (
        (period / 2.0).round() as usize,
        (period * 2.0).round() as usize,
    );
    for i in 0..env.len() {
        let mut best: Option<(f64, usize)> = None;
        if i >= near.max(1) {
            let from = i.saturating_sub(far);
            for (j, previous) in cumulative
                .iter()
                .enumerate()
                .take(i - near.max(1) + 1)
                .skip(from)
            {
                let score = previous - TIGHTNESS * ((i - j) as f64 / period).ln().powi(2);
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, j));
                }
            }
        }
        cumulative[i] = f64::from(local[i]);
        if let Some((score, j)) = best.filter(|(s, _)| *s > 0.0) {
            cumulative[i] += score;
            back[i] = j;
        }
    }

    let maxima: Vec<f64> = (1..env.len().saturating_sub(1))
        .filter(|&i| cumulative[i] > cumulative[i - 1] && cumulative[i] >= cumulative[i + 1])
        .map(|i| cumulative[i])
        .collect();
    let mut sorted = maxima.clone();
    sorted.sort_by(f64::total_cmp);
    let threshold = 0.5 * sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    let last = (1..env.len().saturating_sub(1))
        .rev()
        .find(|&i| {
            cumulative[i] > cumulative[i - 1]
                && cumulative[i] >= cumulative[i + 1]
                && cumulative[i] > threshold
        })
        .ok_or(BeatError::NoRhythm)?;
    let mut frames = vec![last];
    while let Some(&previous) = frames.last().map(|&f| &back[f]) {
        if previous == usize::MAX {
            break;
        }
        frames.push(previous);
    }
    frames.reverse();

    // Drop weak beats at the edges (fade-ins, silence before the song).
    let rms = (local.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>()
        / local.len().max(1) as f64)
        .sqrt();
    let strong = |f: &usize| f64::from(local[*f]) > 0.5 * rms;
    let first = frames.iter().position(strong).unwrap_or(0);
    let end = frames
        .iter()
        .rposition(strong)
        .map_or(frames.len(), |p| p + 1);
    let times: Vec<f64> = frames[first..end.max(first)]
        .iter()
        .map(|&f| frame_seconds(f))
        .collect();
    if times.len() < 4 {
        return Err(BeatError::NoRhythm);
    }
    Ok(Beats {
        bpm: 60.0 * FRAME_RATE / period,
        times,
        clarity,
        stable,
    })
}

#[cfg(test)]
#[path = "tests/beat.rs"]
mod tests;
