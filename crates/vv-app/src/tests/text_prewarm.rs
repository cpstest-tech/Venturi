use super::*;
use vv_core::{Clip, ClipId, Keyframed, Rational, Track, TrackKind, text_anim_presets};

fn timeline() -> Timeline {
    Timeline {
        name: "Prewarm".into(),
        fps: Rational::new(60, 1),
        resolution: (2160, 3840),
        tracks: vec![Track::new(TrackKind::Video)],
        markers: Vec::new(),
        master: Default::default(),
    }
}

fn title(start: FrameIdx, name: &str) -> Clip {
    let mut clip = Clip::from_source_range(
        ClipId(start as u64),
        ClipSource::Text,
        0,
        60,
        start,
        Rational::one(),
    );
    let mut params = TitleParams {
        content: Keyframed::constant(name.into()),
        ..Default::default()
    };
    params.anim.enter = text_anim_presets::in_preset("fade");
    clip.effects.title = Some(params);
    clip
}

#[test]
fn prewarming_tracks_the_playhead_and_ignores_hidden_or_distant_titles() {
    let mut timeline = timeline();
    timeline.tracks[0].clips = vec![
        title(0, "finished"),
        title(60, "current"),
        title(240, "upcoming"),
        title(300, "distant"),
    ];
    let mut disabled = title(120, "disabled");
    disabled.disabled = true;
    timeline.tracks[0].clips.push(disabled);
    let mut hidden = Track::new(TrackKind::Video);
    hidden.muted = true;
    hidden.clips.push(title(120, "hidden"));
    timeline.tracks.push(hidden);
    let titles = upcoming(&timeline, 90);
    assert_eq!(
        titles
            .iter()
            .map(TitleParams::display_text)
            .collect::<Vec<_>>(),
        ["current", "upcoming"]
    );
    assert_eq!(
        titles[0],
        timeline.tracks[0].clips[1]
            .effects
            .title
            .as_ref()
            .unwrap()
            .at(30)
            .animated(0.5, 1.0)
    );
    assert_eq!(titles[1].anim_state.unwrap().t_in, 0.0);
    assert_eq!(upcoming(&timeline, 300)[0].display_text(), "distant");
}

#[test]
fn prewarming_prioritizes_near_titles_and_limits_cache_pressure() {
    let mut timeline = timeline();
    timeline.tracks[0].clips = (0..12)
        .rev()
        .map(|i| title(i * 10, &i.to_string()))
        .collect();
    let titles = upcoming(&timeline, 0);
    assert_eq!(titles.len(), 8);
    assert_eq!(titles[0].display_text(), "0");
    assert_eq!(titles[7].display_text(), "7");
}
