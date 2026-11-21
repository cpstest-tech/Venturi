use super::*;
use crate::{ClipId, ClipSource, Rational};

fn full_pack() -> StylePack {
    StylePack {
        description: "Test".into(),
        subtitles: Some(SubtitleStyle {
            template: "word_pop".into(),
        }),
        auto_zoom: Some(auto_zoom::Config::default()),
        animations: Some(Animations {
            clip_in: Some("zoom_2".into()),
            text_in: Some("pop".into()),
            ..Animations::default()
        }),
        music: Some(Music::default()),
        transition: Some(TransitionStyle {
            asset: "dissolve".into(),
            seconds: 0.5,
        }),
        intro: Some(Bumper {
            template: "neon_pulse".into(),
            text: "Ciao".into(),
        }),
        outro: None,
        safe_zone: Some(SafeZone {
            platform: "tiktok".into(),
        }),
        ..StylePack::new("p1", "Pack")
    }
}

#[test]
fn a_pack_survives_a_round_trip() {
    let pack = full_pack();
    assert_eq!(parse(pack.to_json().as_bytes()), Ok(pack.clone()));
    assert_eq!(pack.components().len(), 7);
    assert!(!pack.has(Component::Outro));
}

#[test]
fn a_newer_pack_is_refused_with_its_version() {
    let json = full_pack()
        .to_json()
        .replace("\"version\": 1", "\"version\": 7");
    assert_eq!(
        parse(json.as_bytes()),
        Err(PackError::Newer {
            found: 7,
            supported: VERSION
        })
    );
}

#[test]
fn other_files_and_broken_packs_are_told_apart() {
    assert_eq!(parse(b"{\"id\": \"x\"}"), Err(PackError::NotAPack));
    assert!(matches!(parse(b"{ not json"), Err(PackError::Corrupt(_))));
    let no_version = format!("{{\"format\": \"{FORMAT}\", \"id\": \"a\", \"name\": \"b\"}}");
    assert!(matches!(
        parse(no_version.as_bytes()),
        Err(PackError::Corrupt(_))
    ));
    let no_name =
        format!("{{\"format\": \"{FORMAT}\", \"version\": 1, \"id\": \"a\", \"name\": \" \"}}");
    assert!(matches!(
        parse(no_name.as_bytes()),
        Err(PackError::Corrupt(_))
    ));
}

#[test]
fn unknown_fields_and_missing_parts_still_read() {
    let json = format!(
        "{{\"format\": \"{FORMAT}\", \"version\": 1, \"id\": \"a\", \"name\": \"B\",
          \"future_part\": {{\"x\": 1}}, \"music\": {{\"ducking_db\": -9}}}}"
    );
    let pack = parse(json.as_bytes()).unwrap();
    assert_eq!(pack.components(), vec![Component::Music]);
    let music = pack.music.unwrap();
    assert_eq!(music.ducking_db, -9.0);
    assert_eq!(music.volume_db, Music::default().volume_db);
}

#[test]
fn close_speech_merges_so_the_music_does_not_pump() {
    assert_eq!(
        speech_regions([3.0..4.0, 0.0..1.0, 1.2..2.0]),
        vec![0.0..2.0, 3.0..4.0]
    );
}

#[test]
fn the_music_dips_under_speech_and_comes_back() {
    let clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        0,
        250,
        0,
        Rational::one(),
    );
    let gain = ducking_gain(&clip, 25.0, &[2.0..4.0], -6.0, -12.0);
    let at = |secs: f64| gain.value_at((secs * 25.0) as FrameIdx);
    assert_eq!(at(0.5), -6.0);
    assert_eq!(at(3.0), -18.0);
    assert_eq!(at(6.0), -6.0);
    assert!(at(1.95) < -6.0 && at(1.95) > -18.0);
}

#[test]
fn speech_at_the_edges_starts_or_ends_the_clip_ducked() {
    let clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        50,
        150,
        25,
        Rational::one(),
    );
    let gain = ducking_gain(&clip, 25.0, &[0.0..2.0, 4.5..9.0], -6.0, -12.0);
    assert_eq!(gain.value_at(clip.source_frame_at(25)), -18.0);
    assert_eq!(gain.value_at(clip.source_frame_at(124)), -18.0);
    assert_eq!(gain.value_at(clip.source_frame_at(85)), -6.0);
    let silent = ducking_gain(&clip, 25.0, &[], -6.0, -12.0);
    assert!(silent.is_constant());
}
