//! Beat markers on the timeline and «cuts on the beat»: clips laid out one
//! after the other, each lasting a whole number of beats.

use std::collections::BTreeSet;

use crate::{
    Clip, ClipColor, ClipId, ClipSource, Command, CommandLabel, CompositeCommand, FrameIdx,
    InsertClip, LiftDelete, Marker, MarkerId, Project, Timeline, TimelineId, TrackKind,
    make_room_for_ranges,
};

pub const BEAT_COLOR: ClipColor = ClipColor::Cyan;

/// Replaces the whole marker list of a timeline in one undo step.
#[derive(Debug)]
pub struct SetMarkers {
    pub timeline: TimelineId,
    markers: Vec<Marker>,
    old: Option<Vec<Marker>>,
}

impl SetMarkers {
    pub fn new(timeline: TimelineId, mut markers: Vec<Marker>) -> Self {
        markers.sort_by_key(|m| m.start);
        Self {
            timeline,
            markers,
            old: None,
        }
    }
}

impl Command for SetMarkers {
    fn label(&self) -> CommandLabel {
        CommandLabel::BeatMarkers
    }

    fn apply(&mut self, project: &mut Project) {
        let markers = &mut project.timelines[self.timeline].markers;
        self.old = Some(std::mem::replace(markers, self.markers.clone()));
    }

    fn undo(&self, project: &mut Project) {
        if let Some(old) = &self.old {
            project.timelines[self.timeline].markers = old.clone();
        }
    }
}

/// The markers of `timeline` with the beats of `clip` in place of the beat
/// markers it already had over its span. `beats` are seconds in the media;
/// `media_start` is the media second at the start of the clip.
pub fn with_clip_beats(
    timeline: &Timeline,
    clip: &Clip,
    beats: &[f64],
    media_start: f64,
    speed: f64,
) -> Vec<Marker> {
    let fps = timeline.fps.as_f64();
    let span = clip.timeline_start..clip.timeline_end();
    let mut markers: Vec<Marker> = timeline
        .markers
        .iter()
        .filter(|m| !m.beat || !span.contains(&m.start))
        .cloned()
        .collect();
    let taken: BTreeSet<FrameIdx> = markers.iter().map(|m| m.start).collect();
    let mut frames: Vec<FrameIdx> = beats
        .iter()
        .map(|&t| clip.timeline_start + ((t - media_start) / speed * fps).round() as FrameIdx)
        .filter(|f| span.contains(f) && !taken.contains(f))
        .collect();
    frames.dedup();
    for (id, start) in (timeline.alloc_marker_id().0..).zip(frames) {
        markers.push(Marker {
            id: MarkerId(id),
            start,
            duration: 0,
            note: String::new(),
            color: BEAT_COLOR,
            beat: true,
        });
    }
    markers.sort_by_key(|m| m.start);
    markers
}

pub fn without_beats(timeline: &Timeline) -> Vec<Marker> {
    timeline
        .markers
        .iter()
        .filter(|m| !m.beat)
        .cloned()
        .collect()
}

pub fn beat_frames(timeline: &Timeline) -> Vec<FrameIdx> {
    let mut frames: Vec<FrameIdx> = timeline
        .markers
        .iter()
        .filter(|m| m.beat)
        .map(|m| m.start)
        .collect();
    frames.sort_unstable();
    frames.dedup();
    frames
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutsError {
    NoClips,
    NotEnoughBeats,
    NoBeatAfterStart,
    Locked,
}

/// Lays the selected video-track clips out on the beats, in timeline order,
/// on the track of the first one: each lasts `every` beats (shorter if its
/// media ends first). Linked audio follows. What lies under the new spans is
/// overwritten. One undo step.
pub fn cuts_on_beats(
    project: &mut Project,
    timeline_id: TimelineId,
    selected: &[(usize, ClipId)],
    every: usize,
) -> Result<CompositeCommand, CutsError> {
    let every = every.max(1);
    let timeline = &project.timelines[timeline_id];
    let beats = beat_frames(timeline);
    if beats.len() < 2 {
        return Err(CutsError::NotEnoughBeats);
    }
    let mut clips: Vec<(usize, &Clip)> = selected
        .iter()
        .filter(|(t, _)| {
            timeline
                .tracks
                .get(*t)
                .is_some_and(|t| t.kind == TrackKind::Video)
        })
        .filter_map(|&(t, id)| Some((t, timeline.clip(t, id)?)))
        .collect();
    clips.sort_by_key(|(t, c)| (c.timeline_start, *t));
    clips.dedup_by_key(|(_, c)| c.id);
    let Some(&(target_track, first)) = clips.first() else {
        return Err(CutsError::NoClips);
    };
    let first_beat = beats.partition_point(|&b| b < first.timeline_start);
    if first_beat >= beats.len() {
        return Err(CutsError::NoBeatAfterStart);
    }
    let mut periods: Vec<FrameIdx> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    periods.sort_unstable();
    let period = periods[periods.len() / 2].max(1);
    let boundary = |k: usize| {
        let i = first_beat + k * every;
        match beats.get(i) {
            Some(&b) => b,
            None => beats[beats.len() - 1] + (i - (beats.len() - 1)) as FrameIdx * period,
        }
    };
    let max_len = |clip: &Clip| -> Option<FrameIdx> {
        let ClipSource::Media(id) = clip.source else {
            return None;
        };
        let item = project.media_pool.get(id)?;
        (!item.meta.is_image() && clip.freeze.is_none())
            .then(|| clip.timeline_frame_at(item.meta.duration_frames) - clip.timeline_start)
    };

    // Where each clip was, and where it goes.
    let mut moved: Vec<(usize, ClipId)> = Vec::new();
    let mut placed: Vec<(usize, Clip)> = Vec::new();
    let mut place = |from: usize, to: usize, c: &Clip, start: FrameIdx, len: FrameIdx| {
        if moved.iter().any(|m| m.1 == c.id) {
            return;
        }
        let mut c = c.clone();
        c.timeline_start = start;
        c.timeline_len = len;
        c.fade_in = c.fade_in.min(len);
        c.fade_out = c.fade_out.min(len);
        moved.push((from, c.id));
        placed.push((to, c));
    };
    for (i, &(track, clip)) in clips.iter().enumerate() {
        let start = boundary(i);
        let slot = (boundary(i + 1) - start).max(1);
        let len = max_len(clip).map_or(slot, |m| slot.min(m.max(1)));
        place(track, target_track, clip, start, len);
        let Some(group) = clip.linked_group else {
            continue;
        };
        for (t, other) in timeline.tracks.iter().enumerate() {
            for linked in other
                .clips
                .iter()
                .filter(|c| c.linked_group == Some(group) && c.id != clip.id)
            {
                let own = max_len(linked).map_or(len, |m| len.min(m.max(1)));
                place(t, t, linked, start, own);
            }
        }
    }
    if moved
        .iter()
        .map(|m| m.0)
        .chain(placed.iter().map(|p| p.0))
        .any(|t| timeline.is_locked(t))
    {
        return Err(CutsError::Locked);
    }

    let mut commands: Vec<Box<dyn Command>> = moved
        .iter()
        .map(|&(t, id)| Box::new(LiftDelete::new(timeline_id, t, id)) as Box<dyn Command>)
        .collect();
    let ranges: Vec<(usize, FrameIdx, FrameIdx)> = placed
        .iter()
        .map(|(t, c)| (*t, c.timeline_start, c.timeline_end()))
        .collect();
    make_room_for_ranges(project, timeline_id, &ranges, &moved, &mut commands);
    for (track_index, clip) in placed {
        commands.push(Box::new(InsertClip {
            timeline: timeline_id,
            track_index,
            clip,
        }));
    }
    Ok(CompositeCommand::new(CommandLabel::BeatCuts, commands))
}

#[cfg(test)]
#[path = "tests/beat_sync.rs"]
mod tests;
