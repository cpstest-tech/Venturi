use super::*;
use crate::{CompoundPlan, Rational, compound_clip_commands, plan_compound_clip};

fn project_with_tracks(kinds: &[TrackKind]) -> (Project, TimelineId) {
    let mut project = Project::default();
    let timeline = project.timelines.insert(Timeline {
        name: "Timeline 1".into(),
        fps: Rational::new(25, 1),
        resolution: (1920, 1080),
        tracks: kinds.iter().map(|k| Track::new(*k)).collect(),
        markers: Vec::new(),
        master: Default::default(),
    });
    (project, timeline)
}

fn put(
    project: &mut Project,
    timeline: TimelineId,
    track: usize,
    source: ClipSource,
    start: FrameIdx,
    len: FrameIdx,
) -> ClipId {
    let clip = Clip::from_source_range(
        project.alloc_clip_id(),
        source,
        0,
        len,
        start,
        Rational::one(),
    );
    let id = clip.id;
    project.timelines[timeline].tracks[track].insert_sorted(clip);
    id
}

fn make_compound(
    project: &mut Project,
    history: &mut History,
    timeline: TimelineId,
    selection: &[(usize, ClipId)],
) -> crate::MediaId {
    let plan: CompoundPlan = plan_compound_clip(project, timeline, selection).unwrap();
    let (media, commands) = compound_clip_commands(project, timeline, selection, &plan);
    history.do_command(
        project,
        Box::new(CompositeCommand::new(
            CommandLabel::MakeCompoundClip,
            commands,
        )),
    );
    media
}

#[test]
fn ungrouping_gives_back_every_layer_with_its_effects() {
    let (mut project, timeline) =
        project_with_tracks(&[TrackKind::Video, TrackKind::Video, TrackKind::Audio]);
    let base = put(&mut project, timeline, 0, ClipSource::SolidColor, 10, 60);
    let text = put(&mut project, timeline, 1, ClipSource::Text, 20, 30);
    let element = put(&mut project, timeline, 1, ClipSource::Adjustment, 60, 10);
    project.timelines[timeline]
        .clip_mut(1, element)
        .unwrap()
        .effects
        .assets = vec![AssetFx::new("filters/noir", vec![0.7])];
    let sound = put(&mut project, timeline, 2, ClipSource::SolidColor, 10, 60);
    let selection = vec![(0, base), (1, text), (1, element), (2, sound)];
    let mut history = History::default();
    let media = make_compound(&mut project, &mut history, timeline, &selection);
    assert_eq!(project.timelines[timeline].tracks[0].clips.len(), 1);

    let host = project.timelines[timeline].tracks[0].clips[0].id;
    let created = ungroup_compound_clip(&mut project, &mut history, timeline, 0, host).unwrap();

    assert_eq!(created.len(), 4, "nothing lost");
    let tl = &project.timelines[timeline];
    assert_eq!(tl.tracks[0].clips.len(), 1);
    assert_eq!(tl.tracks[0].clips[0].timeline_start, 10);
    assert_eq!(tl.tracks[0].clips[0].timeline_len, 60);
    let upper: Vec<_> = tl.tracks[1]
        .clips
        .iter()
        .map(|c| c.timeline_start)
        .collect();
    assert_eq!(upper, vec![20, 60]);
    assert!(matches!(
        tl.tracks[1].clips[1].source,
        ClipSource::Adjustment
    ));
    assert_eq!(
        tl.tracks[1].clips[1].effects.assets,
        vec![AssetFx::new("filters/noir", vec![0.7])]
    );
    assert_eq!(tl.tracks[2].clips.len(), 1);
    assert!(!project.media_pool.contains_key(media), "container removed");

    history.undo(&mut project);
    let tl = &project.timelines[timeline];
    assert_eq!(tl.tracks[0].clips.len(), 1);
    assert!(matches!(tl.tracks[0].clips[0].source, ClipSource::Media(id) if id == media));
    assert!(tl.tracks[1].clips.is_empty());
}

#[test]
fn ungrouping_a_trimmed_container_keeps_only_what_it_showed() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 100);
    let mut history = History::default();
    make_compound(&mut project, &mut history, timeline, &[(0, a)]);
    let host = project.timelines[timeline].tracks[0].clips[0].id;
    {
        let clip = project.timelines[timeline].clip_mut(0, host).unwrap();
        clip.source_offset += 30;
        clip.timeline_start += 30;
        clip.timeline_len = 40;
    }
    ungroup_compound_clip(&mut project, &mut history, timeline, 0, host).unwrap();
    let clip = &project.timelines[timeline].tracks[0].clips[0];
    assert_eq!((clip.timeline_start, clip.timeline_len), (30, 40));
}

#[test]
fn ungrouping_refuses_a_container_with_its_own_effects() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 50);
    let mut history = History::default();
    make_compound(&mut project, &mut history, timeline, &[(0, a)]);
    let host = project.timelines[timeline].tracks[0].clips[0].id;
    project.timelines[timeline]
        .clip_mut(0, host)
        .unwrap()
        .effects
        .assets = vec![AssetFx::new("effects/pulse", vec![])];
    assert_eq!(
        ungroup_compound_clip(&mut project, &mut history, timeline, 0, host),
        Err(UngroupError::CarriesEffects)
    );
    assert_eq!(project.timelines[timeline].tracks[0].clips.len(), 1);
}

#[test]
fn ungrouping_a_plain_clip_is_refused() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 50);
    let mut history = History::default();
    assert_eq!(
        ungroup_compound_clip(&mut project, &mut history, timeline, 0, a),
        Err(UngroupError::NotCompound)
    );
}

#[test]
fn only_clips_touching_within_one_frame_pair_up() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 50);
    let b = put(&mut project, timeline, 0, ClipSource::SolidColor, 51, 50);
    let c = put(&mut project, timeline, 0, ClipSource::SolidColor, 103, 50);
    let tl = &project.timelines[timeline];
    assert_eq!(adjacent_neighbor(&tl.tracks[0], a, FadeEdge::Out), Some(b));
    assert_eq!(adjacent_neighbor(&tl.tracks[0], b, FadeEdge::Out), None);
    assert_eq!(adjacent_neighbor(&tl.tracks[0], c, FadeEdge::In), None);
    assert_eq!(adjacent_pairs(tl), vec![(0, a, b)]);
}

#[test]
fn an_asset_transition_is_clamped_to_what_the_clips_can_lend() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 4);
    let b = put(&mut project, timeline, 0, ClipSource::SolidColor, 4, 100);
    let far = put(&mut project, timeline, 0, ClipSource::SolidColor, 200, 10);
    let mut history = History::default();
    let fx = || AssetFx::new("transitions/dissolve", vec![]);
    assert!(apply_asset_transition(
        &mut project,
        &mut history,
        timeline,
        0,
        (a, b),
        fx(),
        0.5
    ));
    let crossing = &project.timelines[timeline].tracks[0].crossings[0];
    assert_eq!(crossing.transition.duration, 8, "twice the shorter clip");

    assert!(!apply_asset_transition(
        &mut project,
        &mut history,
        timeline,
        0,
        (b, far),
        fx(),
        0.5
    ));
    assert_eq!(project.timelines[timeline].tracks[0].crossings.len(), 1);

    history.undo(&mut project);
    assert!(project.timelines[timeline].tracks[0].crossings.is_empty());
}

#[test]
fn the_default_transition_lasts_half_a_second() {
    let (mut project, timeline) = project_with_tracks(&[TrackKind::Video]);
    let a = put(&mut project, timeline, 0, ClipSource::SolidColor, 0, 100);
    let b = put(&mut project, timeline, 0, ClipSource::SolidColor, 100, 100);
    let crossing = asset_crossing(
        &project.timelines[timeline],
        0,
        a,
        b,
        AssetFx::new("transitions/wipe", vec![]),
        0.5,
    )
    .unwrap();
    assert_eq!(crossing.transition.duration, 13);
}

fn element_assets() -> Vec<AssetFx> {
    vec![AssetFx::new("noir".to_owned(), vec![1.0])]
}

#[test]
fn an_element_goes_on_a_new_effects_track_on_top() {
    let (mut project, tl) = project_with_tracks(&[TrackKind::Video, TrackKind::Video]);
    let mut history = History::default();
    let (track, id) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        None,
        10,
        75,
        element_assets(),
    );
    let timeline = &project.timelines[tl];
    assert_eq!(track, 2, "above every existing track");
    assert!(timeline.tracks[track].effects);
    let clip = timeline.clip(track, id).unwrap();
    assert!(clip.is_effect_element());
    assert_eq!((clip.timeline_start, clip.timeline_len), (10, 75));
    history.undo(&mut project);
    assert_eq!(project.timelines[tl].tracks.len(), 2, "one undo step");
}

#[test]
fn elements_share_an_effects_track_while_they_fit() {
    let (mut project, tl) = project_with_tracks(&[TrackKind::Video]);
    let mut history = History::default();
    let (first, _) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        None,
        0,
        50,
        element_assets(),
    );
    let (second, _) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        None,
        50,
        50,
        element_assets(),
    );
    assert_eq!(first, second, "free space on the same track");
    let (third, _) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        Some(first),
        25,
        50,
        element_assets(),
    );
    assert_ne!(third, first, "overlapping: a new track");
    assert!(project.timelines[tl].tracks[third].effects);
}

#[test]
fn a_locked_effects_track_is_skipped() {
    let (mut project, tl) = project_with_tracks(&[TrackKind::Video]);
    let mut history = History::default();
    let (first, _) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        None,
        0,
        10,
        element_assets(),
    );
    project.timelines[tl].tracks[first].locked = true;
    let (second, _) = insert_fx_element(
        &mut project,
        &mut history,
        tl,
        Some(first),
        100,
        10,
        element_assets(),
    );
    assert_ne!(second, first);
}

#[test]
fn old_files_get_their_effects_tracks_marked() {
    let (mut project, tl) =
        project_with_tracks(&[TrackKind::Video, TrackKind::Video, TrackKind::Video]);
    let element = put(&mut project, tl, 1, ClipSource::Adjustment, 0, 10);
    project.timelines[tl]
        .clip_mut(1, element)
        .unwrap()
        .effects
        .assets = element_assets();
    put(&mut project, tl, 2, ClipSource::Text, 0, 10);
    mark_effects_tracks(&mut project);
    let flags: Vec<bool> = project.timelines[tl]
        .tracks
        .iter()
        .map(|t| t.effects)
        .collect();
    assert_eq!(flags, [false, true, false]);
}
