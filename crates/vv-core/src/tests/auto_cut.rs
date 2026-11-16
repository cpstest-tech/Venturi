use super::*;

fn word(text: &str, start: f64, end: f64) -> Word {
    Word {
        text: text.into(),
        start,
        end,
        confidence: Some(0.95),
    }
}

fn energy(seconds: f64, db: f32) -> Energy {
    Energy {
        step: 0.01,
        db: vec![db; (seconds * 100.0).round() as usize],
    }
}

#[test]
fn keep_ranges_clip_merge_and_complement_overlapping_removals() {
    assert_eq!(
        keep_ranges(10.0, [4.0..6.0, -1.0..1.0, 5.0..8.0, 8.0..9.0]),
        vec![1.0..4.0, 9.0..10.0]
    );
    assert_eq!(keep_ranges(10.0, []), vec![0.0..10.0]);
    assert!(keep_ranges(10.0, [-2.0..12.0]).is_empty());
    assert_eq!(
        merge_ranges([1.0..2.0, 2.0..3.0, 7.0..8.0]),
        vec![1.0..3.0, 7.0..8.0]
    );
}

#[test]
fn silence_keeps_padding_and_snapping_cannot_cross_speech() {
    let words = [word("Prima.", 0.0, 1.0), word("Dopo.", 2.0, 3.0)];
    let mut audio = energy(3.0, -15.0);
    audio.db[100..200].fill(-50.0);
    audio.db[115] = -90.0;
    audio.db[185] = -90.0;
    let segments = analyze(&words, &audio, 3.0, &Config::default());
    let silence = segments
        .iter()
        .find(|s| s.kind == SegmentType::Silence)
        .unwrap();
    assert!((silence.start - 1.15).abs() < 1e-6);
    assert!((silence.end - 1.85).abs() < 1e-6);
    assert!(silence.checked);
    let config = Config {
        padding: 0.3,
        ..Config::default()
    };
    let segments = analyze(&words, &audio, 3.0, &config);
    for s in segments.iter().filter(|s| s.kind == SegmentType::Silence) {
        assert!(s.start >= 1.3 && s.end <= 1.7);
    }
}

#[test]
fn energy_silence_never_overrides_words_or_creates_short_breath_cuts() {
    let words = [word("Prima.", 0.0, 0.2), word("Dopo.", 0.5, 0.7)];
    let segments = analyze(&words, &energy(2.0, -80.0), 2.0, &Config::default());
    assert!(
        segments
            .iter()
            .all(|s| s.kind != SegmentType::Silence || s.start >= 0.5)
    );
    assert!(
        segments
            .iter()
            .any(|s| s.kind == SegmentType::Silence && s.start >= 0.82)
    );
    assert!(
        segments
            .iter()
            .filter(|s| s.kind == SegmentType::Silence)
            .all(|s| !(s.start < 0.2 && s.end > 0.0) && !(s.start < 0.7 && s.end > 0.5))
    );
}

#[test]
fn quiet_whole_clip_is_removable_but_untranscribed_loud_audio_is_kept() {
    let segments = analyze(&[], &energy(4.0, -90.0), 4.0, &Config::default());
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].kind, SegmentType::Silence);
    assert_eq!(
        (segments[0].start, segments[0].end, segments[0].checked),
        (0.0, 4.0, true)
    );
    assert!(
        analyze(&[], &energy(4.0, -10.0), 4.0, &Config::default())
            .iter()
            .all(|s| !s.checked)
    );
}

#[test]
fn italian_repeated_takes_keep_only_the_last_take() {
    let mut words = Vec::new();
    for take in 0..3 {
        for (i, text) in ["Oggi", "vi", "mostro", "questo", "progetto."]
            .into_iter()
            .enumerate()
        {
            let start = take as f64 * 3.0 + i as f64 * 0.3;
            words.push(word(text, start, start + 0.25));
        }
    }
    let segments = analyze(&words, &energy(8.0, -15.0), 8.0, &Config::default());
    let false_starts: Vec<_> = segments
        .iter()
        .filter(|s| s.kind == SegmentType::FalseStart)
        .collect();
    assert_eq!(false_starts.len(), 2);
    assert!(false_starts.iter().all(|s| s.checked && s.end < 6.0));
    assert!(
        segments.iter().any(|s| s.kind == SegmentType::Speech
            && s.start >= 6.0
            && s.text.contains("progetto"))
    );
}

#[test]
fn truncated_italian_prefix_is_a_false_start_and_fillers_are_opt_in() {
    let words = [
        word("Oggi", 0.0, 0.25),
        word("vi", 0.3, 0.5),
        word("mostro", 0.6, 0.9),
        word("Oggi", 2.0, 2.25),
        word("vi", 2.3, 2.5),
        word("mostro", 2.6, 2.9),
        word("il", 3.0, 3.1),
        word("progetto.", 3.2, 3.6),
        word("Ehm,", 4.5, 4.8),
    ];
    let segments = analyze(&words, &energy(5.0, -15.0), 5.0, &Config::default());
    assert!(
        segments
            .iter()
            .any(|s| s.kind == SegmentType::FalseStart && s.end <= 0.9 + 1e-7)
    );
    assert!(
        segments
            .iter()
            .any(|s| s.kind == SegmentType::Filler && !s.checked && s.text == "Ehm,")
    );
}

#[test]
fn similar_subjects_and_repetitions_outside_the_window_are_kept() {
    let words = [
        word("Il", 0.0, 0.2),
        word("video", 0.3, 0.5),
        word("inizia.", 0.6, 0.9),
        word("Il", 2.0, 2.2),
        word("video", 2.3, 2.5),
        word("finisce.", 2.6, 2.9),
        word("Il", 20.0, 20.2),
        word("video", 20.3, 20.5),
        word("inizia.", 20.6, 20.9),
    ];
    assert!(
        analyze(&words, &energy(21.0, -15.0), 21.0, &Config::default())
            .iter()
            .all(|s| s.kind != SegmentType::FalseStart)
    );
}

#[test]
fn cancelling_long_analysis_returns_no_partial_suggestions() {
    let polls = std::cell::Cell::new(0);
    let audio = energy(1000.0, -80.0);
    let result = analyze_cancellable(&[], &audio, 1000.0, &Config::default(), &|| {
        polls.set(polls.get() + 1);
        polls.get() > 3
    });
    assert!(result.is_none());
}

#[test]
fn segment_wire_format_uses_the_requested_type_names() {
    let segment = Segment {
        id: 0,
        start: 0.0,
        end: 1.0,
        kind: SegmentType::FalseStart,
        text: "Restart".into(),
        checked: true,
    };
    let json = serde_json::to_value(segment).unwrap();
    assert_eq!(json["type"], "false_start");
    assert!(json.get("kind").is_none());
}

#[test]
fn continuous_speech_has_no_default_removals() {
    let words = [
        word("Questo", 0.0, 0.4),
        word("video", 0.4, 0.8),
        word("continua.", 0.8, 1.2),
    ];
    let segments = analyze(&words, &energy(1.2, -15.0), 1.2, &Config::default());
    assert!(
        segments
            .iter()
            .all(|s| s.kind == SegmentType::Speech && !s.checked)
    );
}

#[test]
fn long_sentences_compare_their_full_text_instead_of_only_the_prefix() {
    let mut a: Vec<_> = (0..96).map(|i| format!("shared{i}")).collect();
    let mut b = a.clone();
    a.extend((0..104).map(|i| format!("first{i}")));
    b.extend((0..104).map(|i| format!("second{i}")));
    assert!(similarity(&a, &b) < Config::default().similarity);
    assert_eq!(similarity(&a, &a), 1.0);
}
