//! Transitions and asset effects on top of the timeline model, and ungrouping
//! of compound clips. Pure editing: the shaders live in `vv-render`.

use crate::edit::ClipRef;
use crate::{
    AddTrack, AssetFx, Clip, ClipId, ClipSource, Command, CommandLabel, CompositeCommand,
    CrossTransition, Ease, EffectStack, FadeEdge, FrameIdx, History, InsertClip, LiftDelete,
    LinkClips, Project, PushDirection, RemoveMedia, SetCrossTransition, Timeline, TimelineId,
    Track, TrackKind, Transition, TransitionKind, adjacent,
};
use std::collections::HashMap;

/// The clip touching `edge` of `clip_id` on the same track (a gap of
/// `ADJACENT_TOLERANCE` frames still counts). Effect elements never pair.
pub fn adjacent_neighbor(track: &Track, clip_id: ClipId, edge: FadeEdge) -> Option<ClipId> {
    let clip = track.clip(clip_id)?;
    if clip.is_adjustment() {
        return None;
    }
    track
        .clips
        .iter()
        .find(|other| {
            other.id != clip_id
                && !other.is_adjustment()
                && match edge {
                    FadeEdge::In => adjacent(other, clip),
                    FadeEdge::Out => adjacent(clip, other),
                }
        })
        .map(|other| other.id)
}

/// Every pair of adjacent clips on the unlocked video tracks:
/// `(track, left, right)`.
pub fn adjacent_pairs(timeline: &Timeline) -> Vec<(usize, ClipId, ClipId)> {
    let mut pairs = Vec::new();
    for (index, track) in timeline.tracks.iter().enumerate() {
        if track.kind != TrackKind::Video || track.locked {
            continue;
        }
        for clip in &track.clips {
            if let Some(right) = adjacent_neighbor(track, clip.id, FadeEdge::Out) {
                pairs.push((index, clip.id, right));
            }
        }
    }
    pairs
}

/// A crossing between `left` and `right` using the shader transition `fx`,
/// `secs` long and limited to what both clips can lend. `None` if they are
/// not an adjacent pair.
pub fn asset_crossing(
    timeline: &Timeline,
    track_index: usize,
    left: ClipId,
    right: ClipId,
    fx: AssetFx,
    secs: f64,
) -> Option<CrossTransition> {
    let track = timeline.tracks.get(track_index)?;
    if track.locked || track.kind != TrackKind::Video {
        return None;
    }
    let (left_clip, right_clip) = (track.clip(left)?, track.clip(right)?);
    if !adjacent(left_clip, right_clip) || left_clip.is_adjustment() || right_clip.is_adjustment() {
        return None;
    }
    let wanted = (timeline.fps.as_f64() * secs).round() as FrameIdx;
    let duration = wanted.clamp(1, CrossTransition::max_duration(left_clip, right_clip));
    Some(CrossTransition {
        left_clip: left,
        right_clip: right,
        transition: Transition {
            kind: TransitionKind::CrossDissolve,
            duration,
            direction: PushDirection::Right,
            ease: Ease::None,
            curve: 0.0,
            asset: Some(fx),
        },
    })
}

/// Sets (or replaces) the asset transition between two adjacent clips as one
/// undo step. `false` if they are not an adjacent pair.
pub fn apply_asset_transition(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    track_index: usize,
    (left, right): (ClipId, ClipId),
    fx: AssetFx,
    secs: f64,
) -> bool {
    let Some(crossing) = project
        .timelines
        .get(timeline_id)
        .and_then(|t| asset_crossing(t, track_index, left, right, fx, secs))
    else {
        return false;
    };
    history.do_command(
        project,
        Box::new(SetCrossTransition::new(
            timeline_id,
            track_index,
            left,
            Some(crossing),
        )),
    );
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UngroupError {
    NotCompound,
    /// The container has its own effects, fades, speed or freeze: ungrouping
    /// would silently drop them.
    CarriesEffects,
}

/// Commands placing the nested clips, and how many tracks they add.
struct Placement {
    commands: Vec<Box<dyn Command>>,
    added: usize,
}

fn compound_of(project: &Project, clip: &Clip) -> Option<TimelineId> {
    let ClipSource::Media(media) = clip.source else {
        return None;
    };
    project.media_pool.get(media)?.compound
}

/// Whether `clip` is a plain container: nothing of its own to lose.
fn is_plain_container(clip: &Clip) -> bool {
    ron::to_string(&clip.effects).ok() == ron::to_string(&EffectStack::default()).ok()
        && clip.fade_in == 0
        && clip.fade_out == 0
        && clip.freeze.is_none()
        && clip.speed() == crate::Rational::one()
        && clip.rate() == crate::Rational::one()
}

/// Ungroups the compound clip at `(track_index, clip_id)` together with the
/// clips linked to it (the audio half), as one undo step. Every nested clip
/// goes back on the timeline with its own effects, at its original position,
/// trimmed to what the container showed. The first nested track of each kind
/// takes the container's track; the others the first free track above, or a
/// new one. Returns the new clips.
pub fn ungroup_compound_clip(
    project: &mut Project,
    history: &mut History,
    timeline_id: TimelineId,
    track_index: usize,
    clip_id: ClipId,
) -> Result<Vec<ClipRef>, UngroupError> {
    let timeline = project.timelines[timeline_id].clone();
    let timeline = &timeline;
    let host = timeline
        .clip(track_index, clip_id)
        .ok_or(UngroupError::NotCompound)?;
    let nested_id = compound_of(project, host).ok_or(UngroupError::NotCompound)?;
    let media_id = match host.source {
        ClipSource::Media(id) => id,
        _ => return Err(UngroupError::NotCompound),
    };
    let nested = project.timelines[nested_id].clone();

    // The container and its linked halves: the same compound on other tracks.
    let mut containers: Vec<ClipRef> = vec![(track_index, clip_id)];
    if let Some(group) = host.linked_group {
        for (index, track) in timeline.tracks.iter().enumerate() {
            for clip in &track.clips {
                if clip.id != clip_id
                    && clip.linked_group == Some(group)
                    && compound_of(project, clip) == Some(nested_id)
                {
                    containers.push((index, clip.id));
                }
            }
        }
    }
    for &(index, id) in &containers {
        let clip = timeline.clip(index, id).expect("container exists");
        if !is_plain_container(clip) {
            return Err(UngroupError::CarriesEffects);
        }
    }
    let base = |kind: TrackKind| {
        containers
            .iter()
            .map(|c| c.0)
            .filter(|&i| timeline.tracks[i].kind == kind)
            .min()
    };
    let (video_base, audio_base) = (base(TrackKind::Video), base(TrackKind::Audio));
    // Every half shows the same window of the nested timeline.
    let first = timeline.clip(track_index, clip_id).expect("host exists");
    let window_start = first.picture_frame_at(first.timeline_start);
    let window_end = window_start + first.timeline_len;
    let delta = first.timeline_start - window_start;

    let mut next_id = || project.alloc_clip_id();
    let mut commands: Vec<Box<dyn Command>> = containers
        .iter()
        .map(|&(index, id)| Box::new(LiftDelete::new(timeline_id, index, id)) as Box<dyn Command>)
        .collect();

    let mut placement = Placement {
        commands: Vec::new(),
        added: 0,
    };
    let mut tracks_len = timeline.tracks.len();
    let mut taken: HashMap<usize, Vec<(FrameIdx, FrameIdx)>> = HashMap::new();
    let mut placed_on: [Option<usize>; 2] = [None; 2];
    let mut created: Vec<ClipRef> = Vec::new();
    let mut groups: HashMap<crate::LinkGroupId, Vec<ClipRef>> = HashMap::new();
    let mut crossings: Vec<(usize, CrossTransition)> = Vec::new();

    for nested_track in &nested.tracks {
        let mut moved: Vec<Clip> = Vec::new();
        let mut id_map: HashMap<ClipId, ClipId> = HashMap::new();
        for clip in &nested_track.clips {
            let (start, end) = (clip.timeline_start, clip.timeline_end());
            if end <= window_start || start >= window_end {
                continue;
            }
            let mut clip = clip.clone();
            let head = (window_start - start).max(0);
            clip.timeline_start += head;
            clip.timeline_len -= head;
            clip.source_offset += head;
            clip.timeline_len = clip.timeline_len.min(window_end - clip.timeline_start);
            clip.timeline_start += delta;
            let new_id = next_id();
            id_map.insert(clip.id, new_id);
            clip.id = new_id;
            moved.push(clip);
        }
        if moved.is_empty() {
            continue;
        }
        let kind = nested_track.kind;
        let range = (
            moved.iter().map(|c| c.timeline_start).min().unwrap_or(0),
            moved.iter().map(|c| c.timeline_end()).max().unwrap_or(0),
        );
        let base_index = match kind {
            TrackKind::Video => video_base,
            TrackKind::Audio => audio_base,
        };
        let free = |index: usize, taken: &HashMap<usize, Vec<(FrameIdx, FrameIdx)>>| {
            let track = &timeline.tracks[index];
            track.kind == kind
                && !track.locked
                && !taken
                    .get(&index)
                    .is_some_and(|r| r.iter().any(|&(s, e)| s < range.1 && range.0 < e))
                && !track.clips.iter().any(|c| {
                    !containers.iter().any(|&(i, id)| i == index && id == c.id)
                        && c.timeline_start < range.1
                        && range.0 < c.timeline_end()
                })
        };
        let slot = usize::from(kind == TrackKind::Audio);
        let after = placed_on[slot].map(|i| i + 1);
        let target = match (after, base_index) {
            (None, Some(base)) if free(base, &taken) => base,
            (after, _) => {
                let from = after.or(base_index).unwrap_or(0);
                (from..timeline.tracks.len())
                    .find(|&i| free(i, &taken))
                    .unwrap_or_else(|| {
                        placement
                            .commands
                            .push(Box::new(AddTrack::new(timeline_id, kind)));
                        placement.added += 1;
                        tracks_len += 1;
                        tracks_len - 1
                    })
            }
        };
        placed_on[slot] = Some(target);
        taken.entry(target).or_default().push(range);
        for clip in moved {
            if let Some(group) = clip.linked_group {
                groups.entry(group).or_default().push((target, clip.id));
            }
            created.push((target, clip.id));
            let mut clip = clip;
            clip.linked_group = None;
            placement.commands.push(Box::new(InsertClip {
                timeline: timeline_id,
                track_index: target,
                clip,
            }));
        }
        for crossing in &nested_track.crossings {
            if let (Some(&left), Some(&right)) = (
                id_map.get(&crossing.left_clip),
                id_map.get(&crossing.right_clip),
            ) {
                crossings.push((
                    target,
                    CrossTransition {
                        left_clip: left,
                        right_clip: right,
                        transition: crossing.transition.clone(),
                    },
                ));
            }
        }
    }

    commands.extend(placement.commands);
    for (index, crossing) in crossings {
        commands.push(Box::new(SetCrossTransition::new(
            timeline_id,
            index,
            crossing.left_clip,
            Some(crossing),
        )));
    }
    for (_, members) in groups {
        if members.len() >= 2 {
            commands.push(Box::new(LinkClips::new(timeline_id, members)));
        }
    }
    let uses_media = |t: TimelineId, i: usize, c: &Clip| {
        matches!(c.source, ClipSource::Media(id) if id == media_id)
            && !(t == timeline_id && containers.contains(&(i, c.id)))
    };
    let still_used = project.timelines.iter().any(|(t, timeline)| {
        timeline
            .tracks
            .iter()
            .enumerate()
            .any(|(i, track)| track.clips.iter().any(|c| uses_media(t, i, c)))
    });
    if !still_used {
        commands.push(Box::new(RemoveMedia::new(media_id)));
    }
    history.do_command(
        project,
        Box::new(CompositeCommand::new(
            CommandLabel::UngroupCompoundClip,
            commands,
        )),
    );
    Ok(created)
}

#[cfg(test)]
#[path = "tests/fx.rs"]
mod tests;
