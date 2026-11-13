use super::*;

fn token(text: &str, t0: i64, t1: i64) -> RawToken {
    (text.to_owned(), t0, t1)
}

#[test]
fn tokens_without_a_leading_space_continue_the_word() {
    let words = words_from_tokens(&[
        token("[_BEG_]", 0, 0),
        token(" Ciao", 10, 30),
        token(" a", 30, 40),
        token(" tut", 40, 55),
        token("ti", 55, 70),
        token("<|endoftext|>", 70, 70),
    ]);
    let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["Ciao", "a", "tutti"]);
    assert_eq!((words[2].start_ms, words[2].end_ms), (400, 700));
}

#[test]
fn overlapping_words_are_trimmed_to_the_next_start() {
    let words = words_from_tokens(&[token(" uno", 0, 50), token(" due", 40, 80)]);
    assert_eq!(words[0].end_ms, 400);
}

#[test]
fn zero_length_words_get_a_minimum_span() {
    let words = words_from_tokens(&[token(" uno", 10, 10), token(" due", 10, 20)]);
    assert!(words[0].end_ms > words[0].start_ms);
}

#[test]
fn resampling_halves_the_rate_and_averages_channels() {
    let stereo: Vec<f32> = (0..64).flat_map(|_| [1.0, 0.0]).collect();
    let mono = to_mono_16k(&stereo, 32_000, 2);
    assert_eq!(mono.len(), 32);
    assert!(mono.iter().all(|&s| (s - 0.5).abs() < 1e-6));
}

#[test]
fn a_multiple_rate_averages_blocks_of_frames() {
    let mono = to_mono_16k(&[0.0, 0.0, 0.3, 0.3, 0.6, 0.6, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0], 48_000, 2);
    assert_eq!(mono.len(), 2);
    assert!((mono[0] - 0.3).abs() < 1e-6);
    assert_eq!(frames_per_output(48_000), Some(3));
    assert_eq!(frames_per_output(44_100), None);
}

#[test]
fn a_present_model_is_not_downloaded_again() {
    let dir = std::env::temp_dir().join(format!("vv-transcribe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(model_path(&dir), b"x").unwrap();
    let path = ensure_model(&dir, &Progress::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(path, model_path(&dir));
    std::fs::remove_dir_all(&dir).unwrap();
}
