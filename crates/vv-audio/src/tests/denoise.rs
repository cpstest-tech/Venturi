use super::*;

const RATE: u32 = 48_000;

fn noise(len: usize, amplitude: f32, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.log10()
}

#[test]
fn a_unity_gate_is_the_input_delayed_by_its_latency() {
    let curve = Curve {
        over: 0.0,
        floor: 1.0,
        release_s: 0.05,
    };
    let mut gate = DenoiseProcessor::with_curve(curve, RATE, 2);
    let input = noise(2 * 20_000, 0.5, 7);
    let mut output = input.clone();
    gate.process(&mut output);
    let delay = latency(RATE) as usize * 2;
    for (out, inp) in output[delay..].iter().zip(&input) {
        assert!((out - inp).abs() < 1e-4, "{out} vs {inp}");
    }
    assert!(output[..delay].iter().all(|s| s.abs() < 1e-6));
}

#[test]
fn steady_noise_is_lowered_by_the_strength() {
    let input = noise(RATE as usize * 4, 0.01, 3);
    let mut reductions = Vec::new();
    for strength in NoiseStrength::ALL {
        let mut gate = DenoiseProcessor::new(strength, RATE, 1);
        let mut output = input.clone();
        gate.process(&mut output);
        let tail = RATE as usize * 2..;
        reductions.push(db(rms(&output[tail.clone()]) / rms(&input[tail])));
    }
    assert!(reductions[0] < -5.0, "{reductions:?}");
    assert!(reductions[1] < reductions[0] - 3.0, "{reductions:?}");
    assert!(reductions[2] < reductions[1] - 3.0, "{reductions:?}");
}

#[test]
fn speech_over_the_noise_keeps_its_level() {
    let len = RATE as usize * 5;
    let mut input = noise(len, 0.003, 11);
    let burst = |i: usize| (i / (RATE as usize / 10)) % 5 < 2;
    for (i, s) in input.iter_mut().enumerate() {
        if burst(i) {
            let t = i as f32 / RATE as f32;
            *s += 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.1 * (2.0 * std::f32::consts::PI * 1320.0 * t).sin();
        }
    }
    let mut gate = DenoiseProcessor::new(NoiseStrength::High, RATE, 1);
    let mut output = input.clone();
    gate.process(&mut output);
    let delay = latency(RATE) as usize;
    let from = RATE as usize * 3;
    let (mut kept, mut was) = (Vec::new(), Vec::new());
    for i in from..len - delay {
        // Away from the edges of the bursts.
        if burst(i) && burst(i - 1000) && burst(i + 1000) {
            kept.push(output[i + delay]);
            was.push(input[i]);
        }
    }
    let change = db(rms(&kept) / rms(&was));
    assert!(change.abs() < 1.5, "{change} dB");
}

#[test]
fn processing_is_independent_of_the_block_size() {
    let input = noise(30_000, 0.1, 5);
    let mut whole = input.clone();
    DenoiseProcessor::new(NoiseStrength::Medium, RATE, 2).process(&mut whole);
    let mut gate = DenoiseProcessor::new(NoiseStrength::Medium, RATE, 2);
    let mut chunked = input;
    for chunk in chunked.chunks_mut(2 * 37) {
        gate.process(chunk);
    }
    assert_eq!(whole, chunked);
}

#[test]
fn the_frame_is_about_21_ms_at_any_rate() {
    assert_eq!(frame_len(48_000), 1024);
    assert_eq!(frame_len(44_100), 1024);
    assert_eq!(frame_len(96_000), 2048);
    assert_eq!(frame_len(100), 256);
}
