use super::*;
use crate::{History, Id, Rational, Track};

fn fixture() -> (Project, TimelineId, (usize, ClipId)) {
    let mut project = Project::default();
    let group = project.alloc_link_group_id();
    let mut tracks = vec![
        Track::new_main(),
        Track::new(TrackKind::Audio),
        Track::new(TrackKind::Video),
    ];
    let mut target = None;
    for (i, track) in tracks.iter_mut().enumerate() {
        let id = project.alloc_clip_id();
        let mut clip = Clip::from_source_range(
            id,
            crate::ClipSource::SolidColor,
            100,
            200,
            20,
            Rational::one(),
        );
        if i < 2 {
            clip.linked_group = Some(group);
        }
        clip.effects.animations.in_asset = Some("in".into());
        clip.effects.animations.out_asset = Some("out".into());
        clip.effects.animations.combo_asset = Some("combo".into());
        track.clips.push(clip);
        if i == 0 {
            target = Some((i, id));
        }
    }
    let timeline = project.timelines.insert(Timeline {
        name: "Test".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks,
        markers: Vec::new(),
        master: Default::default(),
    });
    (project, timeline, target.unwrap())
}

fn geometry(
    project: &Project,
    timeline: TimelineId,
) -> Vec<Vec<(ClipId, FrameIdx, FrameIdx, FrameIdx)>> {
    project.timelines[timeline]
        .tracks
        .iter()
        .map(|t| {
            t.clips
                .iter()
                .map(|c| (c.id, c.timeline_start, c.timeline_len, c.source_offset))
                .collect()
        })
        .collect()
}

#[test]
fn linked_pieces_ripple_once_keep_source_offsets_and_undo_together() {
    let (mut project, timeline, target) = fixture();
    let before = geometry(&project, timeline);
    let command = AutoCut::plan(
        &mut project,
        timeline,
        target,
        &[(40, 50), (50, 60), (80, 90)],
        true,
        0.015,
    )
    .unwrap();
    let mut history = History::default();
    history.do_command(&mut project, Box::new(command));
    assert_eq!(history.position(), 1);
    let tl = &project.timelines[timeline];
    for track in &tl.tracks {
        assert_eq!(
            track
                .clips
                .iter()
                .map(|c| (c.timeline_start, c.timeline_len, c.source_offset))
                .collect::<Vec<_>>(),
            vec![(20, 20, 100), (40, 20, 140), (60, 30, 170)]
        );
    }
    for i in 0..3 {
        assert_eq!(
            tl.tracks[0].clips[i].linked_group,
            tl.tracks[1].clips[i].linked_group
        );
    }
    assert_ne!(
        tl.tracks[0].clips[0].linked_group,
        tl.tracks[0].clips[1].linked_group
    );
    assert_eq!(tl.tracks[1].clips[0].audio_splice_out_seconds, 0.015);
    assert_eq!(tl.tracks[1].clips[1].audio_splice_in_seconds, 0.015);
    assert_eq!(tl.tracks[1].clips[2].audio_splice_out_seconds, 0.0);
    let after = geometry(&project, timeline);
    history.undo(&mut project);
    assert_eq!(geometry(&project, timeline), before);
    history.redo(&mut project);
    assert_eq!(geometry(&project, timeline), after);
}

#[test]
fn in_and_out_only_run_at_outer_edges_and_combo_uses_one_clock() {
    let (mut project, timeline, target) = fixture();
    let mut command = AutoCut::plan(
        &mut project,
        timeline,
        target,
        &[(40, 50), (70, 80)],
        true,
        0.015,
    )
    .unwrap();
    command.apply(&mut project);
    let clips = &project.timelines[timeline].tracks[0].clips;
    assert!(clips[0].effects.animations.in_asset.is_some());
    assert!(clips[0].effects.animations.out_asset.is_none());
    assert!(clips[1].effects.animations.in_asset.is_none());
    assert!(clips[1].effects.animations.out_asset.is_none());
    assert!(clips[2].effects.animations.in_asset.is_none());
    assert!(clips[2].effects.animations.out_asset.is_some());
    assert_eq!(clips[1].effects.animations.combo_offset_seconds, 0.8);
    assert_eq!(clips[2].effects.animations.combo_offset_seconds, 1.6);
    assert_eq!(clips[2].effects.animations.combo_length_seconds, Some(3.2));
}

#[test]
fn ripple_disabled_preserves_other_tracks_and_locked_links_refuse_the_edit() {
    let (mut project, timeline, target) = fixture();
    let overlay = project.timelines[timeline].tracks[2].clips[0].clone();
    let mut command =
        AutoCut::plan(&mut project, timeline, target, &[(40, 60)], false, 0.015).unwrap();
    command.apply(&mut project);
    let actual = &project.timelines[timeline].tracks[2].clips[0];
    assert_eq!(
        (actual.timeline_start, actual.timeline_len),
        (overlay.timeline_start, overlay.timeline_len)
    );
    command.undo(&mut project);
    project.timelines[timeline].tracks[1].locked = true;
    assert_eq!(
        AutoCut::plan(&mut project, timeline, target, &[(40, 60)], true, 0.015).unwrap_err(),
        "locked"
    );
}

#[test]
fn whole_silent_clip_can_be_deleted_and_restored() {
    let (mut project, timeline, target) = fixture();
    let before = geometry(&project, timeline);
    let mut command =
        AutoCut::plan(&mut project, timeline, target, &[(0, 200)], true, 0.015).unwrap();
    assert!(command.selection.is_none());
    command.apply(&mut project);
    assert!(
        project.timelines[timeline]
            .tracks
            .iter()
            .all(|t| t.clips.is_empty())
    );
    command.undo(&mut project);
    assert_eq!(geometry(&project, timeline), before);
}

#[test]
fn recutting_a_trimmed_fast_clip_keeps_its_source_mapping() {
    let (mut project, timeline, target) = fixture();
    let clip = project.timelines[timeline]
        .clip_mut(target.0, target.1)
        .unwrap();
    clip.source = crate::ClipSource::Media(crate::MediaId::from_raw(1));
    clip.set_speed(Rational::new(2, 1), Rational::one());
    let original = clip.clone();
    let mut command =
        AutoCut::plan(&mut project, timeline, target, &[(30, 40)], false, 0.015).unwrap();
    command.apply(&mut project);
    let clips = &project.timelines[timeline].tracks[0].clips;
    assert_eq!(clips[1].speed(), original.speed());
    assert_eq!(
        clips[1].source_frame_at(clips[1].timeline_start),
        original.source_frame_at(40)
    );
}

#[test]
fn splice_and_combo_timing_survive_project_serialization() {
    let (mut project, timeline, target) = fixture();
    let mut command =
        AutoCut::plan(&mut project, timeline, target, &[(40, 50)], true, 0.015).unwrap();
    command.apply(&mut project);
    let saved = crate::project_to_string(&project).unwrap();
    let restored: Project = ron::from_str(&saved).unwrap();
    let clips = &restored.timelines[timeline].tracks[1].clips;
    assert_eq!(clips[0].audio_splice_out_seconds, 0.015);
    assert_eq!(clips[1].audio_splice_in_seconds, 0.015);
    assert_eq!(clips[1].effects.animations.combo_offset_seconds, 0.8);
    assert_eq!(clips[1].effects.animations.combo_length_seconds, Some(3.6));
}

#[test]
fn untouched_short_linked_audio_keeps_the_link_to_the_matching_video_piece() {
    let (mut project, timeline, target) = fixture();
    let audio = &mut project.timelines[timeline].tracks[1].clips[0];
    audio.timeline_start = 90;
    audio.timeline_len = 30;
    audio.source_offset = 170;
    let mut command = AutoCut::plan(
        &mut project,
        timeline,
        target,
        &[(40, 60), (80, 90)],
        false,
        0.015,
    )
    .unwrap();
    command.apply(&mut project);
    let tl = &project.timelines[timeline];
    assert_eq!(tl.tracks[1].clips[0].timeline_start, 60);
    assert_eq!(
        tl.tracks[1].clips[0].linked_group,
        tl.tracks[0].clips[2].linked_group
    );
}

#[test]
fn ripple_updates_saved_words_and_marker_ranges_and_respects_other_locked_tracks() {
    let (mut project, timeline, target) = fixture();
    project.timelines[timeline].tracks[2].locked = true;
    project.timelines[timeline].tracks[1].subtitles = true;
    project.timelines[timeline].markers.push(crate::Marker {
        id: crate::MarkerId(1),
        start: 30,
        duration: 60,
        note: "Range".into(),
        color: crate::Marker::default_color(),
    });
    let captions = Captions {
        words: vec![
            crate::CaptionWord {
                text: "remove".into(),
                start_ms: 1700,
                end_ms: 1900,
            },
            crate::CaptionWord {
                text: "keep".into(),
                start_ms: 2500,
                end_ms: 2700,
            },
        ],
        ..Default::default()
    };
    project.captions.insert(timeline, captions.clone());
    let mut command =
        AutoCut::plan(&mut project, timeline, target, &[(40, 60)], true, 0.015).unwrap();
    command.apply(&mut project);
    let tl = &project.timelines[timeline];
    assert_eq!((tl.markers[0].start, tl.markers[0].duration), (30, 40));
    assert_eq!(
        (
            tl.tracks[2].clips[0].timeline_start,
            tl.tracks[2].clips[0].timeline_len
        ),
        (20, 100)
    );
    assert_eq!(project.captions[&timeline].words.len(), 1);
    assert_eq!(project.captions[&timeline].words[0].start_ms, 1700);
    command.undo(&mut project);
    assert_eq!(project.captions[&timeline], captions);
}
