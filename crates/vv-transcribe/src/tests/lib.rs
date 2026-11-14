use super::*;

fn token(text: &str, t0: i64, t1: i64) -> RawToken {
    RawToken {
        text: text.to_owned(),
        t0,
        t1,
        dtw: -1,
    }
}

fn aligned(text: &str, dtw: i64) -> RawToken {
    RawToken {
        dtw,
        ..token(text, 0, 0)
    }
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
fn a_word_starts_at_its_dtw_time_slightly_earlier() {
    let words = words_from_tokens(&[
        RawToken {
            dtw: 346,
            ..token(" Se", 304, 308)
        },
        aligned(" ti", 348),
    ]);
    assert_eq!(words[0].start_ms, 3460 - DTW_LEAD_MS);
    assert_eq!(words[0].end_ms, words[1].start_ms);
}

#[test]
fn words_stacked_on_one_instant_are_spread_apart() {
    let words = words_from_tokens(&[
        aligned(" che", 100),
        aligned(" è", 100),
        aligned(" una", 101),
    ]);
    assert!(words[1].start_ms >= words[0].start_ms + 80);
    assert!(words[2].start_ms >= words[1].start_ms + 80);
}

#[test]
fn a_word_before_a_long_gap_does_not_last_the_whole_gap() {
    let words = words_from_tokens(&[aligned(" piace", 100), aligned(" lo", 500)]);
    assert!(words[0].end_ms - words[0].start_ms < 1500);
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
    let mono = to_mono_16k(
        &[0.0, 0.0, 0.3, 0.3, 0.6, 0.6, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        48_000,
        2,
    );
    assert_eq!(mono.len(), 2);
    assert!((mono[0] - 0.3).abs() < 1e-6);
    assert_eq!(frames_per_output(48_000), Some(3));
    assert_eq!(frames_per_output(44_100), None);
}

#[test]
fn a_present_model_is_not_downloaded_again() {
    let dir = std::env::temp_dir().join(format!("vv-transcribe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(model_path(&dir, Model::Small), b"x").unwrap();
    let path = ensure_model(
        &dir,
        Model::Small,
        &Progress::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(path, model_path(&dir, Model::Small));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_abort_callback_reads_the_cancel_flag() {
    let cancel = AtomicBool::new(false);
    let data = std::ptr::from_ref(&cancel).cast_mut().cast();
    assert!(!unsafe { abort_requested(data) });
    cancel.store(true, Ordering::Relaxed);
    assert!(unsafe { abort_requested(data) });
}

/// Needs the real model: `cargo test -p vv-transcribe -- --ignored`.
#[test]
#[ignore]
fn repeated_transcriptions_never_abort_on_their_own() {
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join(".local/share/venturi/models");
    let path = model_path(&dir, Model::Small);
    let samples: Vec<f32> = (0..SAMPLE_RATE as usize * 40)
        .map(|i| (i as f32 * 0.05).sin() * 0.1)
        .collect();
    for _ in 0..4 {
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        transcribe(
            Model::Small,
            &path,
            &samples,
            Some("it"),
            &progress,
            &cancel,
        )
        .unwrap();
    }
}

#[test]
fn every_language_of_the_menu_has_a_style_prompt() {
    for code in LANGUAGES {
        assert!(style_prompt(code).is_some(), "{code}");
    }
}
