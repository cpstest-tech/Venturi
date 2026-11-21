use super::*;

const RATE: u32 = 48_000;

/// A 1 kHz sine at `lufs` (mono; K weighting is about flat there).
fn tone(lufs: f32, seconds: f32) -> Vec<f32> {
    let amplitude = (2.0 * 10f32.powf((lufs + 0.691) / 10.0)).sqrt();
    (0..(RATE as f32 * seconds) as usize)
        .map(|i| amplitude * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / RATE as f32).sin())
        .collect()
}

fn run(params: &AutoLevel, input: &[f32]) -> (Vec<f32>, f32) {
    let mut level = AutoLevelProcessor::new(params, RATE, 1);
    let mut output = input.to_vec();
    level.process(&mut output);
    (output, level.gain_db())
}

#[test]
fn a_quiet_voice_is_brought_up_to_the_target() {
    let (_, gain) = run(&AutoLevel::DEFAULT, &tone(-22.0, 8.0));
    assert!((gain - 6.0).abs() < 0.7, "{gain} dB");
}

#[test]
fn a_loud_passage_is_brought_down_without_going_over_full_scale() {
    let (output, gain) = run(&AutoLevel::DEFAULT, &tone(-6.0, 6.0));
    assert!((gain + 10.0).abs() < 0.7, "{gain} dB");
    assert!(output.iter().all(|s| s.abs() <= CEILING + 1e-4));
}

#[test]
fn the_correction_stops_at_the_maximum() {
    let params = AutoLevel {
        max_gain_db: 6.0,
        ..AutoLevel::DEFAULT
    };
    let (_, gain) = run(&params, &tone(-28.0, 8.0));
    assert!((gain - 6.0).abs() < 0.3, "{gain} dB");
}

#[test]
fn silence_and_faint_noise_are_not_raised() {
    let (_, gain) = run(&AutoLevel::DEFAULT, &tone(-60.0, 4.0));
    assert_eq!(gain, 0.0);
}

#[test]
fn processing_is_independent_of_the_block_size() {
    let input = tone(-20.0, 2.0);
    let (whole, _) = run(&AutoLevel::DEFAULT, &input);
    let mut level = AutoLevelProcessor::new(&AutoLevel::DEFAULT, RATE, 1);
    let mut chunked = input;
    for chunk in chunked.chunks_mut(123) {
        level.process(chunk);
    }
    assert_eq!(whole, chunked);
}
