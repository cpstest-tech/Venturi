use super::*;
use crate::{Clip, ClipSource, Interpolation, Rational, Track};

fn word(text: &str, start: f64, end: f64) -> TimedWord {
    TimedWord {
        text: text.into(),
        start,
        end,
    }
}

#[test]
fn phrases_break_on_pauses_and_sentence_ends() {
    let words = [
        word("Ciao", 0.0, 0.3),
        word("a", 0.35, 0.4),
        word("tutti.", 0.45, 1.8),
        word("Oggi", 1.9, 2.2),
        word("parliamo", 2.25, 2.8),
        word("di", 3.6, 3.7),
        word("iPad.", 3.75, 4.2),
    ];
    let p = phrases(&words);
    let texts: Vec<&str> = p.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts, ["Ciao a tutti.", "Oggi parliamo", "di iPad."]);
    assert_eq!((p[1].start, p[1].end), (1.9, 2.8));
}

#[test]
fn a_long_monologue_is_split_before_the_maximum() {
    let words: Vec<TimedWord> = (0..100)
        .map(|i| word("parola", i as f64 * 0.3, i as f64 * 0.3 + 0.25))
        .collect();
    let p = phrases(&words);
    assert!(p.len() >= 3);
    assert!(p.iter().all(|p| p.end - p.start <= MAX_PHRASE));
}

#[test]
fn windows_take_some_of_the_silence_but_not_the_next_phrase() {
    let w = windows(&[(0.0, 2.0), (2.2, 4.0), (6.0, 7.0)], 7.2);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        close(w[0].end, 2.15),
        "stops before the next phrase: {:?}",
        w[0]
    );
    assert!(close(w[1].end, 4.4), "takes the spill: {:?}", w[1]);
    assert!(close(w[2].end, 7.15));
}

#[test]
fn a_line_is_sped_up_within_the_limit_and_flagged_beyond() {
    let short = fit(1.5, 2.0, 0.2);
    assert_eq!((short.speed, short.length, short.fits), (1.0, 1.5, true));
    let a_bit_long = fit(2.3, 2.0, 0.2);
    assert!((a_bit_long.speed - 1.15).abs() < 1e-9 && a_bit_long.fits);
    let too_long = fit(3.0, 2.0, 0.2);
    assert_eq!(too_long.speed, 1.2);
    assert!(!too_long.fits);
    assert_eq!(too_long.length, 2.0, "cut at the window end");
}

#[test]
fn the_target_length_follows_the_pace_of_the_line() {
    assert_eq!(target_chars("Ciao a tutti", None, 2.0), 12);
    assert_eq!(target_chars("Ciao a tutti", Some(3.0), 2.0), 8);
}

#[test]
fn subtitle_words_cover_the_window_by_length() {
    let w = caption_words("Hello everyone here", 1.0, 3.0);
    assert_eq!(w.len(), 3);
    assert_eq!(w[0].start_ms, 1000);
    assert_eq!(w[2].end_ms, 3000);
    assert!(w[1].end_ms - w[1].start_ms > w[0].end_ms - w[0].start_ms);
}

fn clip(id: u64, start: i64) -> Clip {
    Clip::from_source_range(
        ClipId(id),
        ClipSource::SolidColor,
        0,
        100,
        start,
        Rational::one(),
    )
}

fn ducked(level: f32) -> Keyframed<f32> {
    let mut gain = Keyframed::constant(0.0);
    gain.upsert(10, level, Interpolation::Linear);
    gain
}

fn timeline() -> Timeline {
    let mut voice = Track::new(TrackKind::Audio);
    let mut original = clip(1, 0);
    original.effects.gain_db = ducked(-12.0);
    voice.clips.push(original);
    let dub = |language: &str, level: f32| {
        let mut t = Track::new(TrackKind::Audio);
        t.clips.push(clip(10 + (-level) as u64, 0));
        t.dub = Some(DubTrack {
            language: language.into(),
            voice: "v".into(),
            ducked: vec![Ducked {
                clip: ClipId(1),
                before: Keyframed::constant(0.0),
                after: ducked(level),
            }],
        });
        t
    };
    let mut subtitles = Track::new(TrackKind::Video);
    subtitles.subtitles = true;
    let mut en_subtitles = Track::new(TrackKind::Video);
    en_subtitles.dub = Some(DubTrack {
        language: "en".into(),
        voice: String::new(),
        ducked: vec![],
    });
    Timeline {
        name: "t".into(),
        fps: Rational::new(30, 1),
        resolution: (1080, 1920),
        tracks: vec![
            voice,
            dub("en", -12.0),
            dub("es", -60.0),
            subtitles,
            en_subtitles,
        ],
        markers: Vec::new(),
        master: Default::default(),
    }
}

#[test]
fn choosing_a_dub_mutes_the_others_and_uses_its_ducking() {
    let tl = timeline();
    let s = switch(&tl, &Language::Dub("es".into()));
    assert_eq!(s.mutes, vec![(1, true), (4, true)]);
    assert_eq!(s.gains, vec![(0, ClipId(1), ducked(-60.0))]);

    let s = switch(&tl, &Language::Dub("en".into()));
    assert_eq!(
        s.mutes,
        vec![(2, true), (3, true)],
        "its own subtitles replace the original ones"
    );
    assert!(s.gains.is_empty());
}

#[test]
fn the_original_mutes_every_dub_and_restores_the_gain() {
    let tl = timeline();
    let s = switch(&tl, &Language::Original);
    assert_eq!(s.mutes, vec![(1, true), (2, true), (4, true)]);
    assert_eq!(s.gains, vec![(0, ClipId(1), Keyframed::constant(0.0))]);
    assert_eq!(switch(&tl, &Language::AsIs), Switch::default());
}

#[test]
fn a_gain_edited_by_hand_after_the_dub_is_left_alone() {
    let mut tl = timeline();
    tl.tracks[0].clips[0].effects.gain_db = Keyframed::constant(-3.0);
    assert!(switch(&tl, &Language::Original).gains.is_empty());
}

#[test]
fn the_export_snapshot_gets_the_switch() {
    let mut project = Project::default();
    let id = project.timelines.insert(timeline());
    apply_language(&mut project, id, &Language::Original);
    let tl = &project.timelines[id];
    assert!(tl.tracks[1].muted && tl.tracks[2].muted);
    assert_eq!(
        tl.tracks[0].clips[0].effects.gain_db,
        Keyframed::constant(0.0)
    );
    assert_eq!(languages(tl), ["en", "es"]);
}

#[test]
fn windows_bound_overlapping_word_timestamps() {
    let w = windows(&[(0.0, 2.2), (2.0, 5.1)], 5.0);
    assert_eq!(w, [0.0..2.0, 2.0..5.0]);
}
