use super::*;
use crate::{History, LinkGroupId, MediaItem, MediaMeta, Rational, Track};

fn project() -> (Project, TimelineId) {
    let mut project = Project::default();
    let timeline = project.timelines.insert(Timeline {
        name: "T".into(),
        fps: Rational::new(25, 1),
        resolution: (1080, 1920),
        tracks: vec![
            Track::new_main(),
            Track::new(TrackKind::Video),
            Track::new(TrackKind::Audio),
        ],
        markers: Vec::new(),
        master: Default::default(),
    });
    (project, timeline)
}

fn solid(
    project: &mut Project,
    timeline: TimelineId,
    track: usize,
    start: FrameIdx,
    len: FrameIdx,
) -> ClipId {
    let id = project.alloc_clip_id();
    let clip = Clip::from_source_range(id, ClipSource::SolidColor, 0, len, start, Rational::one());
    project.timelines[timeline].tracks[track].insert_sorted(clip);
    id
}

fn media(project: &mut Project, frames: FrameIdx) -> crate::MediaId {
    project.media_pool.insert(MediaItem {
        path: "a.mp4".into(),
        meta: MediaMeta {
            duration_frames: frames,
            fps: Rational::new(25, 1),
            width: 1920,
            height: 1080,
            has_video: true,
            has_audio: true,
            sample_rate: 48000,
            channels: 2,
            audio_streams: 1,
            file: Default::default(),
        },
        content_hash: 7,
        compound: None,
        folder: None,
    })
}

/// A beat every 10 frames from frame 20.
fn beats(project: &mut Project, timeline: TimelineId, count: usize) {
    let tl = &project.timelines[timeline];
    let clip = Clip::from_source_range(
        ClipId(999),
        ClipSource::SolidColor,
        0,
        1000,
        0,
        Rational::one(),
    );
    let times: Vec<f64> = (0..count).map(|i| (20 + 10 * i) as f64 / 25.0).collect();
    let markers = with_clip_beats(tl, &clip, &times, 0.0, 1.0);
    project.timelines[timeline].markers = markers;
}

#[test]
fn beats_become_markers_inside_the_clip_only() {
    let (project, timeline) = project();
    let tl = &project.timelines[timeline];
    let clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        0,
        100,
        50,
        Rational::one(),
    );
    // Media seconds 1..=6, clip starts at media second 1 on timeline frame 50.
    let times: Vec<f64> = (1..=6).map(f64::from).collect();
    let markers = with_clip_beats(tl, &clip, &times, 1.0, 1.0);
    let frames: Vec<_> = markers.iter().map(|m| m.start).collect();
    assert_eq!(frames, vec![50, 75, 100, 125]);
    assert!(markers.iter().all(|m| m.beat));
}

#[test]
fn reanalysing_replaces_the_beats_and_keeps_user_markers() {
    let (mut project, timeline) = project();
    project.timelines[timeline].markers.push(Marker {
        id: MarkerId(0),
        start: 65,
        duration: 0,
        note: "mine".into(),
        color: Marker::default_color(),
        beat: false,
    });
    beats(&mut project, timeline, 5);
    assert_eq!(project.timelines[timeline].markers.len(), 6);
    beats(&mut project, timeline, 3);
    let tl = &project.timelines[timeline];
    assert_eq!(tl.markers.len(), 4);
    assert!(tl.markers.iter().any(|m| m.note == "mine"));
    let ids: BTreeSet<_> = tl.markers.iter().map(|m| m.id).collect();
    assert_eq!(ids.len(), tl.markers.len());
    assert_eq!(without_beats(tl).len(), 1);
}

#[test]
fn set_markers_undoes_in_one_step() {
    let (mut project, timeline) = project();
    let mut history = History::default();
    let tl = &project.timelines[timeline];
    let clip = Clip::from_source_range(
        ClipId(1),
        ClipSource::SolidColor,
        0,
        100,
        0,
        Rational::one(),
    );
    let markers = with_clip_beats(tl, &clip, &[0.4, 0.8, 1.2], 0.0, 1.0);
    history.do_command(&mut project, Box::new(SetMarkers::new(timeline, markers)));
    assert_eq!(project.timelines[timeline].markers.len(), 3);
    history.undo(&mut project);
    assert!(project.timelines[timeline].markers.is_empty());
}

#[test]
fn clips_are_laid_out_on_every_beat() {
    let (mut project, timeline) = project();
    beats(&mut project, timeline, 8);
    let a = solid(&mut project, timeline, 0, 15, 50);
    let b = solid(&mut project, timeline, 0, 65, 50);
    let c = solid(&mut project, timeline, 0, 115, 50);
    let mut history = History::default();
    let cmd = cuts_on_beats(&mut project, timeline, &[(0, c), (0, a), (0, b)], 1).unwrap();
    history.do_command(&mut project, Box::new(cmd));
    let tl = &project.timelines[timeline];
    let spans: Vec<_> = [a, b, c]
        .iter()
        .map(|id| {
            let c = tl.clip(0, *id).unwrap();
            (c.timeline_start, c.timeline_len)
        })
        .collect();
    assert_eq!(spans, vec![(20, 10), (30, 10), (40, 10)]);
    history.undo(&mut project);
    assert_eq!(
        project.timelines[timeline]
            .clip(0, a)
            .unwrap()
            .timeline_start,
        15
    );
    assert_eq!(project.timelines[timeline].tracks[0].clips.len(), 3);
}

#[test]
fn every_two_beats_and_extrapolated_past_the_last_one() {
    let (mut project, timeline) = project();
    beats(&mut project, timeline, 4);
    let a = solid(&mut project, timeline, 1, 20, 5);
    let b = solid(&mut project, timeline, 1, 30, 5);
    let c = solid(&mut project, timeline, 1, 40, 5);
    let cmd = cuts_on_beats(&mut project, timeline, &[(1, a), (1, b), (1, c)], 2).unwrap();
    History::default().do_command(&mut project, Box::new(cmd));
    let tl = &project.timelines[timeline];
    let c = tl.clip(1, c).unwrap();
    assert_eq!((c.timeline_start, c.timeline_len), (60, 20));
}

#[test]
fn a_short_video_keeps_its_length_and_its_audio_follows() {
    let (mut project, timeline) = project();
    beats(&mut project, timeline, 6);
    let id = media(&mut project, 8);
    let group = LinkGroupId(1);
    let put = |project: &mut Project, track: usize| {
        let cid = project.alloc_clip_id();
        let mut clip =
            Clip::from_source_range(cid, ClipSource::Media(id), 0, 8, 10, Rational::one());
        clip.linked_group = Some(group);
        project.timelines[timeline].tracks[track].insert_sorted(clip);
        cid
    };
    let video = put(&mut project, 0);
    let audio = put(&mut project, 2);
    let cmd = cuts_on_beats(&mut project, timeline, &[(0, video)], 4).unwrap();
    History::default().do_command(&mut project, Box::new(cmd));
    let tl = &project.timelines[timeline];
    let v = tl.clip(0, video).unwrap();
    let a = tl.clip(2, audio).unwrap();
    assert_eq!((v.timeline_start, v.timeline_len), (20, 8));
    assert_eq!((a.timeline_start, a.timeline_len), (20, 8));
}

#[test]
fn what_is_underneath_is_overwritten() {
    let (mut project, timeline) = project();
    beats(&mut project, timeline, 6);
    let other = solid(&mut project, timeline, 0, 20, 100);
    let a = solid(&mut project, timeline, 1, 25, 5);
    let cmd = cuts_on_beats(&mut project, timeline, &[(1, a)], 2).unwrap();
    History::default().do_command(&mut project, Box::new(cmd));
    let tl = &project.timelines[timeline];
    let a = tl.clip(1, a).unwrap();
    assert_eq!((a.timeline_start, a.timeline_len), (30, 20));
    assert_eq!(tl.tracks[1].clips.len(), 1);
    assert!(tl.clip(0, other).is_some());
}

#[test]
fn errors_are_reported() {
    let (mut project, timeline) = project();
    let a = solid(&mut project, timeline, 0, 0, 10);
    assert_eq!(
        cuts_on_beats(&mut project, timeline, &[(0, a)], 1).err(),
        Some(CutsError::NotEnoughBeats)
    );
    beats(&mut project, timeline, 4);
    assert_eq!(
        cuts_on_beats(&mut project, timeline, &[], 1).err(),
        Some(CutsError::NoClips)
    );
    let late = solid(&mut project, timeline, 1, 500, 10);
    assert_eq!(
        cuts_on_beats(&mut project, timeline, &[(1, late)], 1).err(),
        Some(CutsError::NoBeatAfterStart)
    );
    project.timelines[timeline].tracks[0].locked = true;
    assert_eq!(
        cuts_on_beats(&mut project, timeline, &[(0, a)], 1).err(),
        Some(CutsError::Locked)
    );
}
