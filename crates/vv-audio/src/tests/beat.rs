use super::*;

/// Deterministic noise in -1..1.
fn noise(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// Decaying noise bursts at the given times, over a quiet noise floor.
fn clicks(seconds: f64, times: &[f64]) -> Vec<f32> {
    let rate = f64::from(SAMPLE_RATE);
    let mut out = vec![0.0_f32; (seconds * rate) as usize];
    let mut state = 0x2545_f491_4f6c_dd1d;
    for s in &mut out {
        *s = 0.01 * noise(&mut state);
    }
    for &t in times {
        let start = (t * rate) as usize;
        for i in 0..(0.05 * rate) as usize {
            if let Some(s) = out.get_mut(start + i) {
                *s += 0.8 * (-(i as f32) / 200.0).exp() * noise(&mut state);
            }
        }
    }
    out
}

fn steady(bpm: f64, seconds: f64, offset: f64) -> Vec<f64> {
    let period = 60.0 / bpm;
    (0..)
        .map(|i| offset + i as f64 * period)
        .take_while(|t| *t < seconds)
        .collect()
}

fn analyse(samples: &[f32]) -> Result<Beats, BeatError> {
    let mut detector = OnsetDetector::new();
    for chunk in samples.chunks(4000) {
        detector.push(chunk);
    }
    detector.finish()
}

#[test]
fn a_steady_click_track_gives_its_tempo_and_beats() {
    for bpm in [90.0, 120.0, 128.0, 150.0] {
        let truth = steady(bpm, 30.0, 0.5);
        let beats = analyse(&clicks(30.0, &truth)).unwrap();
        assert!((beats.bpm - bpm).abs() < 2.0, "{bpm}: {}", beats.bpm);
        assert!(beats.stable, "{bpm}");
        let matched = beats
            .times
            .iter()
            .filter(|t| truth.iter().any(|b| (*t - b).abs() < 0.05))
            .count();
        assert!(
            matched as f64 >= 0.9 * beats.times.len() as f64,
            "{bpm}: {matched}/{}",
            beats.times.len()
        );
        assert!(beats.times.len() as f64 >= 0.85 * truth.len() as f64);
    }
}

#[test]
fn noise_has_no_rhythm() {
    let mut state = 7_u64;
    let samples: Vec<f32> = (0..SAMPLE_RATE as usize * 20)
        .map(|_| 0.3 * noise(&mut state))
        .collect();
    assert_eq!(analyse(&samples), Err(BeatError::NoRhythm));
}

#[test]
fn silence_and_short_audio_are_refused() {
    let silent = vec![0.0; SAMPLE_RATE as usize * 10];
    assert_eq!(analyse(&silent), Err(BeatError::Silent));
    let short = clicks(2.0, &steady(120.0, 2.0, 0.1));
    assert_eq!(analyse(&short), Err(BeatError::TooShort));
}

#[test]
fn a_drifting_tempo_is_flagged() {
    // 100 BPM speeding up to 140 BPM.
    let mut times = Vec::new();
    let mut t = 0.3;
    while t < 60.0 {
        times.push(t);
        let bpm = 100.0 + 40.0 * t / 60.0;
        t += 60.0 / bpm;
    }
    let beats = analyse(&clicks(60.0, &times)).unwrap();
    assert!(!beats.stable);
}
