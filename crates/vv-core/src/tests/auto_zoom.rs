use super::*;
use crate::{ClipSource, History, Rational, Timeline, Track};

fn word(text: &str, start: f64, end: f64) -> Word {
    Word {
        text: text.into(),
        start,
        end,
        confidence: None,
    }
}

/// Steady -30 dB with the given (start, end, dB) bumps.
fn energy(duration: f64, bumps: &[(f64, f64, f32)]) -> Energy {
    let step = 0.01;
    let db = (0..(duration / step) as usize)
        .map(|i| {
            let t = i as f64 * step;
            bumps
                .iter()
                .find(|b| (b.0..b.1).contains(&t))
                .map_or(-30.0, |b| b.2)
        })
        .collect();
    Energy { step, db }
}

/// A steady flow of words, one every 0.3 s, without pauses.
fn speech(duration: f64) -> Vec<Word> {
    (0..(duration / 0.3) as usize)
        .map(|i| {
            let start = i as f64 * 0.3;
            word("parola", start, start + 0.25)
        })
        .collect()
}

fn clip(len: FrameIdx) -> Clip {
    Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        0,
        len,
        100,
        Rational::one(),
    )
}

#[test]
fn a_loud_word_is_emphasised_over_its_neighbours() {
    let words = speech(6.0);
    let loud = 10;
    let e = energy(6.0, &[(words[loud].start, words[loud].end, -18.0)]);
    let scores = emphasis(&words, &e);
    let best = (1..words.len())
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .unwrap();
    assert_eq!(best, loud);
}

#[test]
fn a_word_after_a_pause_starts_a_sentence() {
    let words = vec![
        word("uno", 0.0, 0.3),
        word("due", 0.35, 0.6),
        word("Tre", 1.6, 1.9),
    ];
    let scores = emphasis(&words, &energy(2.0, &[]));
    assert!(scores[2] > scores[1]);
}

#[test]
fn zooms_keep_the_minimum_gap() {
    let words = speech(30.0);
    let bumps: Vec<_> = words
        .iter()
        .step_by(3)
        .map(|w| (w.start, w.end, -15.0))
        .collect();
    let e = energy(30.0, &bumps);
    let config = Config {
        min_gap: 3.0,
        ..Config::default()
    };
    let zooms = plan(&words, &e, 30.0, &config);
    assert!(zooms.len() >= 5);
    for pair in zooms.windows(2) {
        assert!(pair[1].at - pair[0].at >= 3.0 - 1e-9);
    }
}

#[test]
fn higher_frequency_gives_more_zooms() {
    let words = speech(30.0);
    let bumps: Vec<_> = words
        .iter()
        .step_by(2)
        .enumerate()
        .map(|(i, w)| (w.start, w.end, -30.0 + (i % 4) as f32 * 3.0))
        .collect();
    let e = energy(30.0, &bumps);
    let count = |frequency: Frequency| {
        let config = Config {
            frequency,
            min_gap: frequency.default_gap(),
            ..Config::default()
        };
        plan(&words, &e, 30.0, &config).len()
    };
    assert!(count(Frequency::Low) < count(Frequency::High));
}

#[test]
fn alternate_levels_take_turns() {
    let words: Vec<_> = (0..6)
        .map(|i| word("Frase.", i as f64 * 4.0, i as f64 * 4.0 + 0.4))
        .collect();
    let config = Config {
        intensity: 1.2,
        alternate: true,
        ..Config::default()
    };
    let zooms = plan(&words, &energy(24.0, &[]), 24.0, &config);
    assert!(zooms.len() >= 4);
    assert!((zooms[0].scale - 1.2).abs() < 1e-6);
    assert!(zooms[1].scale < 1.2 && zooms[1].scale > 1.0);
    assert!((zooms[2].scale - 1.2).abs() < 1e-6);
}

#[test]
fn keyframes_punch_in_and_come_back() {
    let fps = 25.0;
    let c = clip(250);
    let zoom = Zoom {
        at: 2.0,
        hold_end: 3.0,
        scale: 1.2,
        shift: [0.0, 0.0],
        word: "x".into(),
    };
    let (transform, auto) = keyframes(&c, &[zoom], &Config::default(), fps, (1080, 1920));
    let zx = transform.track(TransformParam::ZoomX);
    assert_eq!(zx.keyframes().len(), 4);
    assert!((zx.value_at(50) - 1.2).abs() < 1e-6);
    assert!((zx.value_at(70) - 1.2).abs() < 1e-6);
    assert!((zx.value_at(0) - 1.0).abs() < 1e-6);
    assert!((zx.value_at(249) - 1.0).abs() < 1e-6);
    assert_eq!(auto.len(), 8);
    assert!(transform.track(TransformParam::PositionX).is_constant());
}

#[test]
fn removing_keeps_hand_made_and_edited_keyframes() {
    let fps = 25.0;
    let mut c = clip(250);
    c.effects
        .transform
        .track_mut(TransformParam::ZoomX)
        .upsert(240, 1.5, Interpolation::Linear);
    let zoom = Zoom {
        at: 2.0,
        hold_end: 3.0,
        scale: 1.2,
        shift: [0.0, 0.0],
        word: "x".into(),
    };
    let (transform, auto) = keyframes(&c, &[zoom], &Config::default(), fps, (1080, 1920));
    c.effects.transform = transform;
    c.auto_keyframes = auto;
    // The user retouches the peak by hand.
    c.effects
        .transform
        .track_mut(TransformParam::ZoomX)
        .upsert(50, 1.3, Interpolation::Linear);
    let stripped = strip(&c.effects.transform, &c.auto_keyframes, Origin::Zoom);
    let frames: Vec<_> = stripped
        .track(TransformParam::ZoomX)
        .keyframes()
        .iter()
        .map(|k| k.0)
        .collect();
    assert_eq!(frames, vec![50, 240]);
    assert!(stripped.track(TransformParam::ZoomY).is_constant());
}

#[test]
fn a_zoom_over_a_hand_made_keyframe_is_skipped() {
    let mut c = clip(250);
    c.effects
        .transform
        .track_mut(TransformParam::ZoomY)
        .upsert(55, 1.1, Interpolation::Linear);
    let zoom = Zoom {
        at: 2.0,
        hold_end: 3.0,
        scale: 1.2,
        shift: [0.0, 0.0],
        word: "x".into(),
    };
    let (_, auto) = keyframes(&c, &[zoom], &Config::default(), 25.0, (1080, 1920));
    assert!(auto.is_empty());
}

#[test]
fn shift_stays_inside_the_zoom_margin() {
    let c = clip(250);
    let zoom = Zoom {
        at: 2.0,
        hold_end: 3.0,
        scale: 1.2,
        shift: [1.0, -1.0],
        word: "x".into(),
    };
    let config = Config {
        shift: true,
        ..Config::default()
    };
    let (transform, _) = keyframes(&c, &[zoom], &config, 25.0, (1000, 2000));
    let x = transform.track(TransformParam::PositionX).value_at(60);
    let y = transform.track(TransformParam::PositionY).value_at(60);
    assert!(x > 0.0 && x < 0.2 * 1000.0 / 2.0);
    assert!(y < 0.0 && y > -0.2 * 2000.0 / 2.0);
}

#[test]
fn close_zooms_chain_without_returning() {
    let c = clip(500);
    let z = |at: f64, scale: f32| Zoom {
        at,
        hold_end: at + 2.0,
        scale,
        shift: [0.0, 0.0],
        word: "x".into(),
    };
    let (transform, _) = keyframes(
        &c,
        &[z(2.0, 1.2), z(3.0, 1.1)],
        &Config::default(),
        25.0,
        (1080, 1920),
    );
    let zx = transform.track(TransformParam::ZoomX);
    let between = zx.value_at((2.6 * 25.0) as FrameIdx);
    assert!(between > 1.15, "{between}");
}

#[test]
fn the_command_round_trips_through_undo() {
    let mut project = Project::default();
    let mut track = Track::new_main();
    track.clips.push(clip(250));
    let timeline = project.timelines.insert(Timeline {
        name: "T".into(),
        fps: Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![track],
        markers: Vec::new(),
        master: Default::default(),
    });
    let zoom = Zoom {
        at: 2.0,
        hold_end: 3.0,
        scale: 1.2,
        shift: [0.0, 0.0],
        word: "x".into(),
    };
    let c = project.timelines[timeline].clip(0, ClipId(1)).unwrap();
    let (transform, auto) = keyframes(c, &[zoom], &Config::default(), 25.0, (1080, 1920));
    let mut history = History::default();
    history.do_command(
        &mut project,
        Box::new(SetAutoZoom::new(timeline, 0, ClipId(1), transform, auto)),
    );
    let c = project.timelines[timeline].clip(0, ClipId(1)).unwrap();
    assert!(!c.auto_keyframes.is_empty());
    let remove = SetAutoZoom::remove(timeline, 0, c, Origin::Zoom);
    history.do_command(&mut project, Box::new(remove));
    let c = project.timelines[timeline].clip(0, ClipId(1)).unwrap();
    assert!(c.effects.transform.is_constant());
    history.undo(&mut project);
    history.undo(&mut project);
    let c = project.timelines[timeline].clip(0, ClipId(1)).unwrap();
    assert!(c.effects.transform.is_constant() && c.auto_keyframes.is_empty());
}
