use std::collections::BTreeSet;

use crate::edit::ClipRef;
use crate::workflow::*;
use crate::{
    Clip, ClipId, ClipSource, FrameIdx, History, InsertClip, LinkClips, MediaId, MediaItem,
    MediaMeta, Project, Rational, SetTrackFlag, Timeline, TimelineId, Track, TrackFlag, TrackKind,
};

const FOLLOW_OFF: Follow = Follow {
    magnet: false,
    linkage: false,
};
const MAGNET: Follow = Follow {
    magnet: true,
    linkage: false,
};
const MAGNET_LINKAGE: Follow = Follow {
    magnet: true,
    linkage: true,
};

/// A Main Track followed by `others`.
fn project_with(others: &[TrackKind]) -> (Project, TimelineId) {
    let mut project = Project::default();
    let mut tracks = vec![Track::new_main()];
    tracks.extend(others.iter().map(|&k| Track::new(k)));
    let timeline = project.timelines.insert(Timeline {
        name: "Timeline 1".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks,
        markers: Vec::new(),
        master: Default::default(),
    });
    (project, timeline)
}

fn put(
    project: &mut Project,
    history: &mut History,
    timeline: TimelineId,
    track_index: usize,
    start: FrameIdx,
    len: FrameIdx,
) -> ClipId {
    let id = project.alloc_clip_id();
    let clip = Clip::from_source_range(id, ClipSource::SolidColor, 0, len, start, Rational::one());
    history.do_command(
        project,
        Box::new(InsertClip {
            timeline,
            track_index,
            clip,
        }),
    );
    id
}

fn spans(project: &Project, timeline: TimelineId, track: usize) -> Vec<(FrameIdx, FrameIdx)> {
    project.timelines[timeline].tracks[track]
        .clips
        .iter()
        .map(|c| (c.timeline_start, c.timeline_end()))
        .collect()
}

fn media(project: &mut Project, has_audio: bool) -> MediaId {
    project.media_pool.insert(MediaItem {
        path: "a.mp4".into(),
        meta: MediaMeta {
            duration_frames: 1000,
            fps: Rational::new(25, 1),
            width: 1920,
            height: 1080,
            has_video: true,
            has_audio,
            sample_rate: 48000,
            channels: 2,
            audio_streams: u16::from(has_audio),
            file: Default::default(),
        },
        content_hash: 7,
        compound: None,
        folder: None,
    })
}

fn put_media(
    project: &mut Project,
    history: &mut History,
    timeline: TimelineId,
    track_index: usize,
    media: MediaId,
    start: FrameIdx,
    len: FrameIdx,
) -> ClipId {
    let id = project.alloc_clip_id();
    let clip =
        Clip::from_source_range(id, ClipSource::Media(media), 0, len, start, Rational::one());
    history.do_command(
        project,
        Box::new(InsertClip {
            timeline,
            track_index,
            clip,
        }),
    );
    id
}

fn none() -> BTreeSet<ClipRef> {
    BTreeSet::new()
}

#[test]
fn q_trims_the_left_and_the_magnet_closes_the_hole() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 0, 100, 50);

    let changed = delete_side(
        &mut project,
        &mut history,
        tl,
        40,
        Side::Left,
        &none(),
        MAGNET,
    );

    assert!(changed);
    assert_eq!(spans(&project, tl, 0), vec![(0, 60), (60, 110)]);
}

#[test]
fn w_without_the_magnet_leaves_the_hole() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 0, 100, 50);

    delete_side(
        &mut project,
        &mut history,
        tl,
        40,
        Side::Right,
        &none(),
        FOLLOW_OFF,
    );

    assert_eq!(spans(&project, tl, 0), vec![(0, 40), (100, 150)]);
}

#[test]
fn w_with_the_magnet_pulls_the_next_clip_back() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 0, 100, 50);

    delete_side(
        &mut project,
        &mut history,
        tl,
        40,
        Side::Right,
        &none(),
        MAGNET,
    );

    assert_eq!(spans(&project, tl, 0), vec![(0, 40), (40, 90)]);
}

#[test]
fn delete_side_without_selection_hits_every_unlocked_crossing_clip() {
    let (mut project, tl) = project_with(&[TrackKind::Video, TrackKind::Video]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 1, 20, 100);
    put(&mut project, &mut history, tl, 2, 0, 100);
    history.do_command(
        &mut project,
        Box::new(SetTrackFlag::new(tl, 2, TrackFlag::Locked, true)),
    );

    delete_side(
        &mut project,
        &mut history,
        tl,
        50,
        Side::Right,
        &none(),
        FOLLOW_OFF,
    );

    assert_eq!(spans(&project, tl, 0), vec![(0, 50)]);
    assert_eq!(spans(&project, tl, 1), vec![(20, 50)]);
    assert_eq!(
        spans(&project, tl, 2),
        vec![(0, 100)],
        "locked track untouched"
    );
}

#[test]
fn delete_side_with_a_selection_ignores_the_other_clips() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    let main = put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 1, 0, 100);

    delete_side(
        &mut project,
        &mut history,
        tl,
        50,
        Side::Left,
        &BTreeSet::from([(0, main)]),
        FOLLOW_OFF,
    );

    assert_eq!(spans(&project, tl, 0), vec![(50, 100)]);
    assert_eq!(spans(&project, tl, 1), vec![(0, 100)]);
}

#[test]
fn delete_side_outside_every_clip_does_nothing() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 10, 20);
    let steps = history.position();

    let changed = delete_side(
        &mut project,
        &mut history,
        tl,
        10,
        Side::Left,
        &none(),
        MAGNET,
    );

    assert!(!changed);
    assert_eq!(history.position(), steps);
}

#[test]
fn delete_side_with_the_magnet_is_a_single_undo_step() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 100);
    put(&mut project, &mut history, tl, 0, 100, 50);
    let before = history.position();

    delete_side(
        &mut project,
        &mut history,
        tl,
        40,
        Side::Left,
        &none(),
        MAGNET,
    );
    assert_eq!(history.position(), before + 1);

    history.undo(&mut project);
    assert_eq!(spans(&project, tl, 0), vec![(0, 100), (100, 150)]);
}

#[test]
fn compact_main_track_closes_every_hole_including_the_leading_one() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 10, 20);
    put(&mut project, &mut history, tl, 0, 50, 10);

    assert!(compact_main_track(&mut project, &mut history, tl, &[]));

    assert_eq!(spans(&project, tl, 0), vec![(0, 20), (20, 30)]);
    assert!(!compact_main_track(&mut project, &mut history, tl, &[]));
}

#[test]
fn compact_main_track_moves_the_linked_group_with_the_clip() {
    let (mut project, tl) = project_with(&[TrackKind::Audio]);
    let mut history = History::default();
    let v = put(&mut project, &mut history, tl, 0, 30, 20);
    let a = put(&mut project, &mut history, tl, 1, 30, 20);
    history.do_command(
        &mut project,
        Box::new(LinkClips::new(tl, vec![(0, v), (1, a)])),
    );

    compact_main_track(&mut project, &mut history, tl, &[]);

    assert_eq!(spans(&project, tl, 0), vec![(0, 20)]);
    assert_eq!(spans(&project, tl, 1), vec![(0, 20)]);
}

#[test]
fn linkage_moves_the_clips_anchored_under_the_main_clip_only() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 50);
    put(&mut project, &mut history, tl, 0, 50, 50);
    put(&mut project, &mut history, tl, 1, 10, 20);
    put(&mut project, &mut history, tl, 1, 60, 20);

    delete_side(
        &mut project,
        &mut history,
        tl,
        40,
        Side::Right,
        &none(),
        MAGNET_LINKAGE,
    );

    assert_eq!(spans(&project, tl, 0), vec![(0, 40), (40, 90)]);
    assert_eq!(
        spans(&project, tl, 1),
        vec![(10, 30), (50, 70)],
        "the overlay over the second clip follows it, the first one stays"
    );
}

#[test]
fn without_linkage_the_overlays_stay_put() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 0, 50);
    put(&mut project, &mut history, tl, 0, 50, 50);
    put(&mut project, &mut history, tl, 1, 60, 20);

    delete_side(
        &mut project,
        &mut history,
        tl,
        25,
        Side::Left,
        &none(),
        MAGNET,
    );

    assert_eq!(spans(&project, tl, 1), vec![(60, 80)]);
}

#[test]
fn extract_audio_unlinks_the_audio_that_is_already_on_its_own_track() {
    let (mut project, tl) = project_with(&[TrackKind::Audio]);
    let mut history = History::default();
    let m = media(&mut project, true);
    let v = put_media(&mut project, &mut history, tl, 0, m, 0, 100);
    let a = put_media(&mut project, &mut history, tl, 1, m, 0, 100);
    history.do_command(
        &mut project,
        Box::new(LinkClips::new(tl, vec![(0, v), (1, a)])),
    );

    let freed = extract_audio(&mut project, &mut history, tl, &[(0, v)]);

    assert_eq!(freed, vec![(1, a)]);
    let timeline = &project.timelines[tl];
    assert!(timeline.clip(0, v).unwrap().linked_group.is_none());
    assert!(timeline.clip(1, a).unwrap().linked_group.is_none());
}

#[test]
fn extract_audio_recreates_missing_audio_on_a_new_row() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    let m = media(&mut project, true);
    let v = put_media(&mut project, &mut history, tl, 0, m, 20, 100);
    let before = history.position();

    let freed = extract_audio(&mut project, &mut history, tl, &[(0, v)]);

    assert_eq!(freed.len(), 1);
    let (track, id) = freed[0];
    let timeline = &project.timelines[tl];
    assert_eq!(timeline.tracks[track].kind, TrackKind::Audio);
    let audio = timeline.clip(track, id).unwrap();
    assert_eq!((audio.timeline_start, audio.timeline_len), (20, 100));
    assert!(audio.linked_group.is_none());
    assert_eq!(history.position(), before + 1, "track and clip in one step");
}

#[test]
fn extract_audio_of_a_silent_media_does_nothing() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    let m = media(&mut project, false);
    let v = put_media(&mut project, &mut history, tl, 0, m, 0, 100);
    let before = history.position();

    assert!(extract_audio(&mut project, &mut history, tl, &[(0, v)]).is_empty());
    assert_eq!(history.position(), before);
}

#[test]
fn freeze_frame_inserts_three_seconds_and_pushes_the_rest_right() {
    let (mut project, tl) = project_with(&[TrackKind::Audio]);
    let mut history = History::default();
    let m = media(&mut project, true);
    let v = put_media(&mut project, &mut history, tl, 0, m, 0, 100);
    put_media(&mut project, &mut history, tl, 0, m, 100, 50);
    let a = put_media(&mut project, &mut history, tl, 1, m, 0, 100);
    history.do_command(
        &mut project,
        Box::new(LinkClips::new(tl, vec![(0, v), (1, a)])),
    );
    let before = history.position();

    let still = freeze_frame(&mut project, &mut history, tl, 40, &none()).unwrap();

    assert_eq!(history.position(), before + 1);
    let timeline = &project.timelines[tl];
    assert_eq!(
        spans(&project, tl, 0),
        vec![(0, 40), (40, 115), (115, 175), (175, 225)]
    );
    let frozen = timeline.clip(still.0, still.1).unwrap();
    assert_eq!(frozen.freeze, Some(40));
    assert!(frozen.linked_group.is_none());
    assert_eq!(
        spans(&project, tl, 1),
        vec![(0, 40), (115, 175)],
        "the audio is cut and the right half slides"
    );

    history.undo(&mut project);
    assert_eq!(spans(&project, tl, 0), vec![(0, 100), (100, 150)]);
    assert_eq!(spans(&project, tl, 1), vec![(0, 100)]);
}

#[test]
fn freeze_frame_without_a_video_clip_under_the_playhead_does_nothing() {
    let (mut project, tl) = project_with(&[]);
    let mut history = History::default();
    let m = media(&mut project, false);
    put_media(&mut project, &mut history, tl, 0, m, 0, 10);

    assert!(freeze_frame(&mut project, &mut history, tl, 50, &none()).is_none());
}

#[test]
fn edit_points_are_every_clip_boundary() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    put(&mut project, &mut history, tl, 0, 10, 20);
    put(&mut project, &mut history, tl, 1, 25, 5);

    let points = edit_points(&project, tl);

    assert_eq!(points, vec![0, 10, 25, 30]);
    assert_eq!(previous_edit_point(&points, 25), Some(10));
    assert_eq!(next_edit_point(&points, 25), Some(30));
    assert_eq!(next_edit_point(&points, 30), None);
    assert_eq!(previous_edit_point(&points, 0), None);
}

#[test]
fn main_track_is_the_flagged_one_or_the_first_video_track() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    assert_eq!(project.timelines[tl].main_track_index(), Some(0));

    project.timelines[tl].tracks[0].main = false;
    project.timelines[tl].tracks[1].main = true;
    assert_eq!(project.timelines[tl].main_track_index(), Some(1));

    project.timelines[tl].tracks[1].main = false;
    assert_eq!(
        project.timelines[tl].main_track_index(),
        Some(0),
        "old projects have no flag"
    );
}

#[test]
fn duplicate_after_slides_the_following_clips_and_keeps_the_pairs_linked() {
    let (mut project, tl) = project_with(&[TrackKind::Audio]);
    let mut history = History::default();
    let v = put(&mut project, &mut history, tl, 0, 0, 50);
    let a = put(&mut project, &mut history, tl, 1, 0, 50);
    history.do_command(
        &mut project,
        Box::new(LinkClips::new(tl, vec![(0, v), (1, a)])),
    );
    put(&mut project, &mut history, tl, 0, 50, 30);
    let before = history.position();

    let copies = duplicate_after(&mut project, &mut history, tl, &[(0, v)]);

    assert_eq!(history.position(), before + 1);
    assert_eq!(copies.len(), 2);
    assert_eq!(spans(&project, tl, 0), vec![(0, 50), (50, 100), (100, 130)]);
    assert_eq!(spans(&project, tl, 1), vec![(0, 50), (50, 100)]);
    let timeline = &project.timelines[tl];
    let groups: Vec<_> = copies
        .iter()
        .map(|&(t, id)| timeline.clip(t, id).unwrap().linked_group)
        .collect();
    assert!(groups[0].is_some() && groups[0] == groups[1]);
    assert_ne!(groups[0], timeline.clip(0, v).unwrap().linked_group);

    history.undo(&mut project);
    assert_eq!(spans(&project, tl, 0), vec![(0, 50), (50, 80)]);
}

#[test]
fn duplicate_after_ignores_locked_tracks() {
    let (mut project, tl) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    let c = put(&mut project, &mut history, tl, 1, 0, 10);
    history.do_command(
        &mut project,
        Box::new(SetTrackFlag::new(tl, 1, TrackFlag::Locked, true)),
    );

    assert!(duplicate_after(&mut project, &mut history, tl, &[(1, c)]).is_empty());
    assert_eq!(spans(&project, tl, 1), vec![(0, 10)]);
}

#[test]
fn changing_the_timeline_resolution_is_one_undo_step() {
    let (mut project, timeline) = project_with(&[]);
    let before = project.timelines[timeline].resolution;
    let mut history = History::default();
    history.do_command(
        &mut project,
        Box::new(crate::SetTimelineResolution::new(timeline, (1080, 1920))),
    );
    assert_eq!(project.timelines[timeline].resolution, (1080, 1920));
    history.undo(&mut project);
    assert_eq!(project.timelines[timeline].resolution, before);
    history.redo(&mut project);
    assert_eq!(project.timelines[timeline].resolution, (1080, 1920));
}

#[test]
fn an_insertion_on_the_main_track_lands_on_the_nearest_clip_start() {
    let (mut project, timeline) = project_with(&[]);
    let mut history = History::default();
    put(&mut project, &mut history, timeline, 0, 0, 100);
    put(&mut project, &mut history, timeline, 0, 100, 100);
    assert_eq!(main_insert_point(&project, timeline, 130, &[]), 100);
    assert_eq!(main_insert_point(&project, timeline, 170, &[]), 200);
}

#[test]
fn opening_a_main_gap_pushes_the_later_clips_and_their_overlays() {
    let (mut project, timeline) = project_with(&[TrackKind::Video]);
    let mut history = History::default();
    put(&mut project, &mut history, timeline, 0, 0, 100);
    put(&mut project, &mut history, timeline, 0, 100, 100);
    put(&mut project, &mut history, timeline, 1, 120, 10);
    let anchors = linkage_anchors(&project, timeline);
    open_main_gap(&mut project, &mut history, timeline, 100, 50, &anchors);
    assert_eq!(spans(&project, timeline, 0), vec![(0, 100), (150, 250)]);
    assert_eq!(spans(&project, timeline, 1), vec![(170, 180)]);
}
