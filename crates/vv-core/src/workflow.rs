//! Timeline editing of the CapCut-style workflow: delete around the
//! playhead, Main Track Magnet, Linkage, extract audio, freeze frame.
//! Like `edit`, each operation is one undo step and the caller owns the
//! selection.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::edit::{self, ClipRef};
use crate::{
    Clip, ClipId, ClipSource, Command, CommandLabel, CompositeCommand, EffectStack, FrameIdx,
    History, InsertClip, LinkClips, LinkGroupId, MoveClips, Project, TimelineId, TrackKind,
    TrimClip, TrimEdge, UnlinkClip,
};

/// Length of the freeze frame clip, in seconds.
pub const FREEZE_SECONDS: f64 = 3.0;

/// The timeline options an edit has to honour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Follow {
    /// Main Track Magnet: no holes left on the Main Track.
    pub magnet: bool,
    /// Linkage: clips of other tracks follow the Main Track clip under them.
    pub linkage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The unlocked clips an edit at `frame` acts on: the selection (with its
/// linked groups) or, without one, every clip. Only those strictly crossing
/// `frame` count.
pub fn crossing_targets(
    project: &Project,
    timeline_id: TimelineId,
    frame: FrameIdx,
    selected: &BTreeSet<ClipRef>,
) -> Vec<ClipRef> {
    let tl = &project.timelines[timeline_id];
    let mut chosen: BTreeSet<ClipRef> = BTreeSet::new();
    if selected.is_empty() {
        for (track_index, track) in tl.tracks.iter().enumerate() {
            chosen.extend(track.clips.iter().map(|c| (track_index, c.id)));
        }
    } else {
        for &(track_index, clip_id) in selected {
            chosen.insert((track_index, clip_id));
            chosen.extend(tl.linked_members(track_index, clip_id));
        }
    }
    chosen
        .into_iter()
        .filter(|&(track_index, clip_id)| {
            !tl.is_locked(track_index)
                && tl
                    .clip(track_index, clip_id)
                    .is_some_and(|c| frame > c.timeline_start && frame < c.timeline_end())
        })
        .collect()
}

/// Q / W: cuts away everything left or right of `frame` from the clips
/// returned by `crossing_targets`. No ripple of its own: with `magnet` the
/// hole on the Main Track closes. Returns whether anything changed.
pub fn delete_side(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    frame: FrameIdx,
    side: Side,
    selected: &BTreeSet<ClipRef>,
    follow: Follow,
) -> bool {
    let targets = crossing_targets(project, timeline_id, frame, selected);
    if targets.is_empty() {
        return false;
    }
    let anchors = if follow.magnet && follow.linkage {
        linkage_anchors(project, timeline_id)
    } else {
        Vec::new()
    };
    let edge = match side {
        Side::Left => TrimEdge::Start,
        Side::Right => TrimEdge::End,
    };
    let mark = history.begin_group();
    let commands: Vec<Box<dyn Command>> = targets
        .into_iter()
        .map(|(track_index, clip_id)| {
            Box::new(TrimClip::new(
                timeline_id,
                track_index,
                clip_id,
                edge,
                frame,
            )) as Box<dyn Command>
        })
        .collect();
    history.do_command(
        project,
        Box::new(CompositeCommand::new(CommandLabel::TrimClips, commands)),
    );
    if follow.magnet {
        compact_main_track(project, history, timeline_id, &anchors);
    }
    history.end_group_as(mark, CommandLabel::TrimClips);
    true
}

/// Each clip outside the Main Track with the Main Track clip under its start,
/// the one it follows when Linkage is on. Computed on the fly, nothing is
/// stored.
pub fn linkage_anchors(project: &Project, timeline_id: TimelineId) -> Vec<(ClipRef, ClipId)> {
    let tl = &project.timelines[timeline_id];
    let Some(main) = tl.main_track_index() else {
        return Vec::new();
    };
    let mut anchors = Vec::new();
    for (track_index, track) in tl.tracks.iter().enumerate() {
        if track_index == main || track.locked {
            continue;
        }
        for clip in &track.clips {
            let under = tl.tracks[main].clips.iter().find(|m| {
                m.timeline_start <= clip.timeline_start && clip.timeline_start < m.timeline_end()
            });
            if let Some(under) = under {
                anchors.push(((track_index, clip.id), under.id));
            }
        }
    }
    anchors
}

/// Where an insertion on a magnetic Main Track lands: the clip start (or
/// the end of the track) nearest to `reference`, ignoring `exclude`.
pub fn main_insert_point(
    project: &Project,
    timeline_id: TimelineId,
    reference: FrameIdx,
    exclude: &[ClipId],
) -> FrameIdx {
    let tl = &project.timelines[timeline_id];
    let Some(main) = tl.main_track_index() else {
        return reference;
    };
    let others = tl.tracks[main]
        .clips
        .iter()
        .filter(|c| !exclude.contains(&c.id));
    let end = others.clone().map(|c| c.timeline_end()).max().unwrap_or(0);
    others
        .map(|c| c.timeline_start)
        .chain([0, end])
        .min_by_key(|&b| (b - reference).abs())
        .unwrap_or(0)
}

/// The clips `open_main_gap` pushes: the Main Track ones starting at or
/// after `at`, their linked groups and the clips anchored to them.
pub fn main_gap_clips(
    project: &Project,
    timeline_id: TimelineId,
    at: FrameIdx,
    anchors: &[(ClipRef, ClipId)],
) -> Vec<ClipRef> {
    let tl = &project.timelines[timeline_id];
    let Some(main) = tl.main_track_index() else {
        return Vec::new();
    };
    if tl.tracks[main].locked {
        return Vec::new();
    }
    let pushed: HashSet<ClipId> = tl.tracks[main]
        .clips
        .iter()
        .filter(|c| c.timeline_start >= at)
        .map(|c| c.id)
        .collect();
    let mut seen: HashSet<ClipRef> = HashSet::new();
    let mut clips = Vec::new();
    let mut push = |clip: ClipRef| {
        if !tl.is_locked(clip.0) && seen.insert(clip) {
            clips.push(clip);
        }
    };
    for clip in tl.tracks[main]
        .clips
        .iter()
        .filter(|c| pushed.contains(&c.id))
    {
        push((main, clip.id));
        for member in tl.linked_members(main, clip.id) {
            push(member);
        }
    }
    for &(clip, anchor) in anchors {
        if pushed.contains(&anchor) {
            push(clip);
        }
    }
    clips
}

/// Pushes `main_gap_clips` forward by `len`: the room a drop inserts into
/// on a magnetic Main Track. One undo step.
pub fn open_main_gap(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    at: FrameIdx,
    len: FrameIdx,
    anchors: &[(ClipRef, ClipId)],
) {
    if len <= 0 {
        return;
    }
    let tl = &project.timelines[timeline_id];
    let moves: Vec<(ClipId, usize, usize, FrameIdx)> =
        main_gap_clips(project, timeline_id, at, anchors)
            .into_iter()
            .filter_map(|(track, id)| {
                let clip = tl.clip(track, id)?;
                Some((id, track, track, clip.timeline_start + len))
            })
            .collect();
    if !moves.is_empty() {
        history.do_command(project, Box::new(MoveClips::new(timeline_id, moves)));
    }
}

/// Main Track Magnet: slides the Main Track clips back so no hole is left,
/// before the first clip included. Their linked groups follow, and with
/// `anchors` (see `linkage_anchors`, taken before the edit) so do the
/// clips of the other tracks anchored to them. One undo step.
pub fn compact_main_track(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    anchors: &[(ClipRef, ClipId)],
) -> bool {
    let tl = &project.timelines[timeline_id];
    let Some(main) = tl.main_track_index() else {
        return false;
    };
    if tl.tracks[main].locked {
        return false;
    }
    let mut cursor: FrameIdx = 0;
    let mut deltas: HashMap<ClipId, FrameIdx> = HashMap::new();
    for clip in &tl.tracks[main].clips {
        deltas.insert(clip.id, cursor - clip.timeline_start);
        cursor += clip.timeline_len;
    }

    let mut moves: Vec<(ClipId, usize, usize, FrameIdx)> = Vec::new();
    let mut seen: HashSet<ClipRef> = HashSet::new();
    let mut push = |track_index: usize, clip_id: ClipId, delta: FrameIdx| {
        if delta == 0 || tl.is_locked(track_index) || !seen.insert((track_index, clip_id)) {
            return;
        }
        if let Some(clip) = tl.clip(track_index, clip_id) {
            let start = (clip.timeline_start + delta).max(0);
            moves.push((clip_id, track_index, track_index, start));
        }
    };
    for clip in &tl.tracks[main].clips {
        let delta = deltas[&clip.id];
        push(main, clip.id, delta);
        for (track_index, id) in tl.linked_members(main, clip.id) {
            push(track_index, id, delta);
        }
    }
    for &((track_index, clip_id), anchor) in anchors {
        let Some(&delta) = deltas.get(&anchor) else {
            continue;
        };
        push(track_index, clip_id, delta);
        for (track_index, id) in tl.linked_members(track_index, clip_id) {
            push(track_index, id, delta);
        }
    }
    if moves.is_empty() {
        return false;
    }
    let mark = history.begin_group();
    history.do_command(project, Box::new(MoveClips::new(timeline_id, moves)));
    edit::cut_overlaps(project, history, timeline_id);
    history.end_group_as(mark, CommandLabel::MoveClips);
    true
}

/// Separate Audio: the audio of a video clip becomes independent, on a row of
/// its own below. Audio already on its own track (the usual import) is only
/// unlinked from the picture; a video whose audio is missing gets it back as
/// new unlinked clips. Returns the audio clips now free.
pub fn extract_audio(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    clips: &[ClipRef],
) -> Vec<ClipRef> {
    let mut freed: Vec<ClipRef> = Vec::new();
    let mark = history.begin_group();
    for &(track_index, clip_id) in clips {
        let tl = &project.timelines[timeline_id];
        let Some(clip) = tl.clip(track_index, clip_id) else {
            continue;
        };
        let ClipSource::Media(media_id) = clip.source else {
            continue;
        };
        if tl.tracks[track_index].kind != TrackKind::Video || tl.is_locked(track_index) {
            continue;
        }
        let audio_members: Vec<ClipRef> = tl
            .linked_members(track_index, clip_id)
            .into_iter()
            .filter(|&(t, _)| tl.tracks[t].kind == TrackKind::Audio)
            .collect();
        if !audio_members.is_empty() {
            freed.extend(audio_members);
            history.do_command(
                project,
                Box::new(UnlinkClip::new(timeline_id, track_index, clip_id)),
            );
            continue;
        }
        let Some(meta) = project.media_pool.get(media_id).map(|m| m.meta.clone()) else {
            continue;
        };
        if !meta.has_audio {
            continue;
        }
        let original = clip.clone();
        let (start, end) = (original.timeline_start, original.timeline_end());
        let mut used: HashSet<usize> = HashSet::new();
        for stream in 0..meta.audio_stream_count() {
            let tl = &project.timelines[timeline_id];
            let free_track = tl
                .tracks_of_kind(TrackKind::Audio)
                .find(|(i, t)| {
                    !t.locked
                        && !used.contains(i)
                        && !t
                            .clips
                            .iter()
                            .any(|c| c.timeline_start < end && c.timeline_end() > start)
                })
                .map(|(i, _)| i);
            let target = free_track.unwrap_or_else(|| {
                edit::add_track(project, history, timeline_id, TrackKind::Audio)
            });
            used.insert(target);
            let mut audio = original.clone();
            audio.id = project.alloc_clip_id();
            audio.audio_stream_index = stream;
            audio.linked_group = None;
            audio.effects = EffectStack::default();
            audio.fade_in = 0;
            audio.fade_out = 0;
            audio.freeze = None;
            audio.display_color = None;
            freed.push((target, audio.id));
            history.do_command(
                project,
                Box::new(InsertClip {
                    timeline: timeline_id,
                    track_index: target,
                    clip: audio,
                }),
            );
        }
    }
    history.end_group_as(mark, CommandLabel::UnlinkClips);
    freed
}

/// Freeze Frame: [`FREEZE_SECONDS`] of the picture under `frame`, inserted at
/// `frame`; what follows on the unlocked tracks slides right to make room.
/// The clip is the selected video one under `frame` or, without a selection,
/// the topmost. Returns the new still clip.
pub fn freeze_frame(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    frame: FrameIdx,
    selected: &BTreeSet<ClipRef>,
) -> Option<ClipRef> {
    let tl = &project.timelines[timeline_id];
    let under = |track_index: usize, clip: &Clip| {
        tl.tracks[track_index].kind == TrackKind::Video
            && !tl.is_locked(track_index)
            && matches!(clip.source, ClipSource::Media(_))
            && clip.contains(frame)
    };
    let (track_index, original) = if selected.is_empty() {
        tl.tracks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, t)| t.clips.iter().find(|c| under(i, c)).map(|c| (i, c.clone())))?
    } else {
        selected.iter().find_map(|&(i, id)| {
            tl.clip(i, id)
                .filter(|c| under(i, c))
                .map(|c| (i, c.clone()))
        })?
    };
    let picture = original.picture_frame_at(frame);
    let len = (tl.fps.as_f64() * FREEZE_SECONDS).round().max(1.0) as FrameIdx;
    let mut group_members: BTreeSet<ClipRef> = tl
        .linked_members(track_index, original.id)
        .into_iter()
        .collect();
    group_members.insert((track_index, original.id));

    let mark = history.begin_group();
    if frame > original.timeline_start {
        edit::split_clips(project, history, timeline_id, frame, Some(&group_members));
    }
    let tl = &project.timelines[timeline_id];
    let moves: Vec<(ClipId, usize, usize, FrameIdx)> = tl
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.locked)
        .flat_map(|(i, t)| {
            t.clips
                .iter()
                .filter(|c| c.timeline_start >= frame)
                .map(move |c| (c.id, i, i, c.timeline_start + len))
        })
        .collect();
    if !moves.is_empty() {
        history.do_command(project, Box::new(MoveClips::new(timeline_id, moves)));
    }
    let mut still = original;
    still.id = project.alloc_clip_id();
    still.timeline_start = frame;
    still.timeline_len = len;
    still.linked_group = None;
    still.freeze = Some(picture);
    still.fade_in = 0;
    still.fade_out = 0;
    let still_id = still.id;
    history.do_command(
        project,
        Box::new(InsertClip {
            timeline: timeline_id,
            track_index,
            clip: still,
        }),
    );
    history.end_group_as(mark, CommandLabel::FreezeFrame);
    Some((track_index, still_id))
}

/// Ctrl+D: copies the clips (and their linked groups) right after the
/// selection, sliding what follows on the unlocked tracks to make room.
/// Returns the copies.
pub fn duplicate_after(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    clips: &[ClipRef],
) -> Vec<ClipRef> {
    let tl = &project.timelines[timeline_id];
    let mut set: BTreeSet<ClipRef> = BTreeSet::new();
    for &(track_index, clip_id) in clips {
        if tl.is_locked(track_index) || tl.clip(track_index, clip_id).is_none() {
            continue;
        }
        set.insert((track_index, clip_id));
        set.extend(
            tl.linked_members(track_index, clip_id)
                .into_iter()
                .filter(|&(t, _)| !tl.is_locked(t)),
        );
    }
    let originals: Vec<(usize, Clip)> = set
        .iter()
        .filter_map(|&(t, id)| tl.clip(t, id).map(|c| (t, c.clone())))
        .collect();
    let Some(start) = originals.iter().map(|(_, c)| c.timeline_start).min() else {
        return Vec::new();
    };
    let end = originals
        .iter()
        .map(|(_, c)| c.timeline_end())
        .max()
        .unwrap_or(start);
    let span = end - start;
    let moves: Vec<(ClipId, usize, usize, FrameIdx)> = tl
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.locked)
        .flat_map(|(i, t)| {
            t.clips
                .iter()
                .filter(|c| c.timeline_start >= end)
                .map(move |c| (c.id, i, i, c.timeline_start + span))
        })
        .collect();

    let mark = history.begin_group();
    if !moves.is_empty() {
        history.do_command(project, Box::new(MoveClips::new(timeline_id, moves)));
    }
    let mut groups: HashMap<LinkGroupId, Vec<ClipRef>> = HashMap::new();
    let mut created: Vec<ClipRef> = Vec::new();
    for (track_index, mut clip) in originals {
        let group = clip.linked_group.take();
        clip.id = project.alloc_clip_id();
        clip.timeline_start += span;
        created.push((track_index, clip.id));
        if let Some(group) = group {
            groups
                .entry(group)
                .or_default()
                .push((track_index, clip.id));
        }
        history.do_command(
            project,
            Box::new(InsertClip {
                timeline: timeline_id,
                track_index,
                clip,
            }),
        );
    }
    for members in groups.into_values() {
        if members.len() >= 2 {
            history.do_command(project, Box::new(LinkClips::new(timeline_id, members)));
        }
    }
    history.end_group_as(mark, CommandLabel::DuplicateClips);
    created
}

/// Every frame where a clip starts or ends, sorted, for the ↑/↓ jumps.
pub fn edit_points(project: &Project, timeline_id: TimelineId) -> Vec<FrameIdx> {
    let mut points: BTreeSet<FrameIdx> = BTreeSet::from([0]);
    for track in &project.timelines[timeline_id].tracks {
        for clip in &track.clips {
            points.insert(clip.timeline_start);
            points.insert(clip.timeline_end());
        }
    }
    points.into_iter().collect()
}

pub fn previous_edit_point(points: &[FrameIdx], frame: FrameIdx) -> Option<FrameIdx> {
    points.iter().rev().find(|&&p| p < frame).copied()
}

pub fn next_edit_point(points: &[FrameIdx], frame: FrameIdx) -> Option<FrameIdx> {
    points.iter().find(|&&p| p > frame).copied()
}
