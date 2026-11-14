use super::*;

const RATE: usize = SAMPLE_RATE as usize;

/// `(seconds, loud)` stretches: a 200 Hz tone or a faint hiss.
fn audio(parts: &[(f32, bool)]) -> Vec<f32> {
    let mut samples = Vec::new();
    for &(secs, loud) in parts {
        for i in 0..(secs * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let hiss = ((i * 7919) % 101) as f32 / 101.0 - 0.5;
            samples.push(if loud {
                0.3 * (t * 200.0 * std::f32::consts::TAU).sin()
            } else {
                0.0005 * hiss
            });
        }
    }
    samples
}

fn word(text: &str, start_ms: u32, end_ms: u32) -> Word {
    Word {
        text: text.to_owned(),
        start_ms,
        end_ms,
    }
}

fn speech(spans: &[Range<u32>], total_ms: u32) -> Speech {
    Speech {
        spans: spans.to_vec(),
        total_ms,
    }
}

#[test]
fn the_voice_is_found_between_the_pauses() {
    let found = detect_speech(&audio(&[
        (1.0, false),
        (1.0, true),
        (2.0, false),
        (1.0, true),
        (0.5, false),
    ]))
    .unwrap();
    assert_eq!(found.spans.len(), 2);
    let near = |a: u32, b: u32| a.abs_diff(b) <= PAD_MS + 20;
    assert!(near(found.spans[0].start, 1000) && near(found.spans[0].end, 2000));
    assert!(near(found.spans[1].start, 4000) && near(found.spans[1].end, 5000));
    assert_eq!(found.total_ms, 5500);
}

#[test]
fn a_short_dip_inside_a_word_is_not_a_pause() {
    let found = detect_speech(&audio(&[
        (1.0, false),
        (0.5, true),
        (0.08, false),
        (0.5, true),
        (1.0, false),
    ]))
    .unwrap();
    assert_eq!(found.spans.len(), 1);
}

#[test]
fn audio_without_a_quiet_floor_is_left_alone() {
    assert_eq!(detect_speech(&audio(&[(3.0, true)])), None);
}

#[test]
fn a_word_placed_in_a_long_pause_is_dropped() {
    let mut words = vec![
        word("gratis.", 0, 900),
        word("Se", 1500, 2000),
        word("ti", 3920, 4300),
    ];
    fit_to_speech(&mut words, &speech(&[0..1000, 3900..5000], 5000));
    let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["gratis.", "ti"]);
}

#[test]
fn a_word_starting_in_a_pause_moves_to_where_the_voice_resumes() {
    let mut words = vec![word("Se", 3000, 4100), word("ti", 4200, 4400)];
    fit_to_speech(
        &mut words,
        &speech(std::slice::from_ref(&(3900..5000)), 5000),
    );
    assert_eq!(words[0].start_ms, 3900);
}

#[test]
fn a_word_ends_where_the_pause_starts() {
    let mut words = vec![word("corpo", 500, 2000), word("troviamo", 2000, 2500)];
    fit_to_speech(&mut words, &speech(&[0..800, 1900..3000], 3000));
    assert_eq!(words[0].end_ms, 800);
    assert_eq!(words[1].start_ms, 2000);
}

#[test]
fn a_word_in_a_short_pause_is_kept() {
    let mut words = vec![word("ha", 900, 1100), word("la", 1150, 1300)];
    fit_to_speech(&mut words, &speech(&[0..850, 1400..2000], 2000));
    assert_eq!(words.len(), 2);
}
