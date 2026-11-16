use crate::{
    Captions, Clip, ClipId, Command, CommandLabel, FrameIdx, LinkGroupId, Project, Timeline,
    TimelineId, TrackKind,
};

pub fn merge_frame_ranges(ranges: &[(FrameIdx, FrameIdx)]) -> Vec<(FrameIdx, FrameIdx)> {
    let mut ranges: Vec<_> = ranges.iter().copied().filter(|&(s, e)| e > s).collect();
    ranges.sort_unstable();
    let mut out: Vec<(FrameIdx, FrameIdx)> = Vec::new();
    for (s, e) in ranges {
        if let Some(last) = out.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
        } else {
            out.push((s, e));
        }
    }
    out
}

struct CutMap {
    ranges: Vec<(FrameIdx, FrameIdx)>,
    removed: Vec<FrameIdx>,
}

impl CutMap {
    fn new(ranges: Vec<(FrameIdx, FrameIdx)>) -> Self {
        let mut removed = vec![0];
        for &(s, e) in &ranges {
            removed.push(removed.last().copied().unwrap_or(0) + e - s);
        }
        Self { ranges, removed }
    }

    fn shift(&self, at: FrameIdx) -> FrameIdx {
        let i = self.ranges.partition_point(|&(_, e)| e <= at);
        at - self.removed[i] - self.ranges.get(i).map_or(0, |&(s, _)| (at - s).max(0))
    }
}

fn slices(clip: &Clip, cuts: &[(FrameIdx, FrameIdx)]) -> Vec<(FrameIdx, FrameIdx)> {
    let mut out = Vec::new();
    let mut cursor = clip.timeline_start;
    let first = cuts.partition_point(|&(_, e)| e <= cursor);
    for &(s, e) in &cuts[first..] {
        if s >= clip.timeline_end() {
            break;
        }
        if cursor < s {
            out.push((cursor, s));
        }
        cursor = cursor.max(e);
    }
    if cursor < clip.timeline_end() {
        out.push((cursor, clip.timeline_end()));
    }
    out
}

#[derive(Debug)]
pub struct AutoCut {
    timeline: TimelineId,
    before: Timeline,
    after: Timeline,
    captions_before: Option<Captions>,
    captions_after: Option<Captions>,
    pub selection: Option<(usize, ClipId)>,
    pub start: FrameIdx,
}

impl AutoCut {
    pub fn plan(
        project: &mut Project,
        timeline: TimelineId,
        target: (usize, ClipId),
        cuts: &[(FrameIdx, FrameIdx)],
        ripple_all: bool,
        crossfade_seconds: f64,
    ) -> Result<Self, &'static str> {
        let before = project
            .timelines
            .get(timeline)
            .ok_or("timeline_missing")?
            .clone();
        let original = before.clip(target.0, target.1).ok_or("clip_missing")?;
        let start = original.timeline_start;
        let end = original.timeline_end();
        let group = original.linked_group;
        let linked =
            |c: &Clip| c.id == target.1 || group.is_some_and(|g| c.linked_group == Some(g));
        if before
            .tracks
            .iter()
            .any(|t| t.locked && t.clips.iter().any(linked))
        {
            return Err("locked");
        }
        let affected: Vec<_> = before
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.locked && (ripple_all || t.clips.iter().any(linked)))
            .map(|(i, _)| i)
            .collect();
        let cuts = merge_frame_ranges(
            &cuts
                .iter()
                .map(|&(s, e)| (s.max(start), e.min(end)))
                .collect::<Vec<_>>(),
        );
        if cuts.is_empty() {
            return Err("no_cuts");
        }
        let cut_map = CutMap::new(cuts.clone());
        let remapped_groups: std::collections::HashSet<_> = affected
            .iter()
            .flat_map(|&i| &before.tracks[i].clips)
            .filter(|clip| {
                let i = cuts.partition_point(|&(_, e)| e <= clip.timeline_start);
                cuts.get(i).is_some_and(|&(s, _)| s < clip.timeline_end())
            })
            .filter_map(|clip| clip.linked_group)
            .collect();
        let mut after = before.clone();
        let mut selection = None;
        let mut groups: std::collections::HashMap<(LinkGroupId, usize), LinkGroupId> =
            std::collections::HashMap::new();
        for &track_index in &affected {
            let track = &mut after.tracks[track_index];
            let mut changed = Vec::new();
            track.clips.clear();
            for clip in &before.tracks[track_index].clips {
                let pieces = slices(clip, &cuts);
                let total: FrameIdx = pieces.iter().map(|&(s, e)| e - s).sum();
                let mut offset = 0;
                let touched = pieces.len() != 1
                    || pieces.first().is_some_and(|&(s, e)| {
                        s != clip.timeline_start || e != clip.timeline_end()
                    });
                if touched {
                    changed.push(clip.id);
                }
                for (index, &(s, e)) in pieces.iter().enumerate() {
                    let mut piece = clip.clone();
                    if index > 0 {
                        piece.id = project.alloc_clip_id();
                    }
                    piece.source_offset += s - clip.timeline_start;
                    piece.timeline_start = cut_map.shift(s);
                    piece.timeline_len = e - s;
                    if let Some(g) = clip.linked_group.filter(|g| remapped_groups.contains(g)) {
                        // A shared time identifies corresponding pieces even after earlier trims.
                        let slot = cuts.partition_point(|&(_, end)| end <= s);
                        piece.linked_group = Some(
                            *groups
                                .entry((g, slot))
                                .or_insert_with(|| project.alloc_link_group_id()),
                        );
                    }
                    if touched {
                        piece.effects.drop_keyframes_before(piece.source_in());
                        piece.effects.drop_keyframes_from(piece.source_out());
                        if index > 0 {
                            piece.fade_in = 0;
                            piece.effects.transition_in = None;
                            piece.effects.animations.in_asset = None;
                            piece.effects.animations.entrance = None;
                            if let Some(title) = &mut piece.effects.title {
                                title.anim.enter = None;
                            }
                        }
                        if index + 1 < pieces.len() {
                            piece.fade_out = 0;
                            piece.effects.transition_out = None;
                            piece.effects.animations.out_asset = None;
                            piece.effects.animations.exit = None;
                            if let Some(title) = &mut piece.effects.title {
                                title.anim.exit = None;
                            }
                        }
                        piece.effects.animations.combo_offset_seconds =
                            clip.effects.animations.combo_offset_seconds
                                + offset as f64 / before.fps.as_f64();
                        piece.effects.animations.combo_length_seconds = Some(
                            clip.effects
                                .animations
                                .combo_length_seconds
                                .unwrap_or(total as f64 / before.fps.as_f64()),
                        );
                        piece.audio_splice_in_seconds = if index > 0 {
                            crossfade_seconds
                                .clamp(0.0, 0.02)
                                .min(piece.timeline_len as f64 / before.fps.as_f64())
                        } else {
                            clip.audio_splice_in_seconds
                        };
                        piece.audio_splice_out_seconds = if index + 1 < pieces.len() {
                            crossfade_seconds
                                .clamp(0.0, 0.02)
                                .min((pieces[index + 1].0 - e) as f64 / before.fps.as_f64())
                                .min(
                                    (pieces[index + 1].1 - pieces[index + 1].0) as f64
                                        / before.fps.as_f64(),
                                )
                        } else {
                            clip.audio_splice_out_seconds
                        };
                        if track.kind != TrackKind::Audio {
                            piece.audio_splice_in_seconds = 0.0;
                            piece.audio_splice_out_seconds = 0.0;
                        }
                        piece.fade_in = piece.fade_in.min(piece.timeline_len);
                        piece.fade_out = piece.fade_out.min(piece.timeline_len);
                    }
                    if clip.id == target.1 && selection.is_none() {
                        selection = Some((track_index, piece.id));
                    }
                    offset += e - s;
                    track.clips.push(piece);
                }
            }
            track.clips.sort_by_key(|c| c.timeline_start);
            track
                .crossings
                .retain(|c| !changed.contains(&c.left_clip) && !changed.contains(&c.right_clip));
        }
        if ripple_all {
            for marker in &mut after.markers {
                let end = cut_map.shift(marker.end());
                marker.start = cut_map.shift(marker.start);
                marker.duration = (end - marker.start).max(0);
            }
        }
        let captions_before = project.captions.get(&timeline).cloned();
        let mut captions_after = captions_before.clone();
        if affected.iter().any(|&i| before.tracks[i].subtitles)
            && let Some(captions) = &mut captions_after
        {
            let fps = before.fps.as_f64();
            let milliseconds = |frame: FrameIdx| (frame as f64 * 1000.0 / fps).round() as u32;
            let ms_cuts: Vec<_> = cuts
                .iter()
                .map(|&(s, e)| (milliseconds(s), milliseconds(e)))
                .collect();
            captions.words.retain(|w| {
                let i = ms_cuts.partition_point(|&(_, e)| e <= w.start_ms);
                ms_cuts.get(i).is_none_or(|&(s, _)| s >= w.end_ms)
            });
            let ms_map = CutMap::new(
                ms_cuts
                    .iter()
                    .map(|&(s, e)| (i64::from(s), i64::from(e)))
                    .collect(),
            );
            for word in &mut captions.words {
                word.start_ms = ms_map.shift(i64::from(word.start_ms)) as u32;
                word.end_ms = ms_map.shift(i64::from(word.end_ms)) as u32;
            }
        }
        Ok(Self {
            timeline,
            before,
            after,
            captions_before,
            captions_after,
            selection,
            start,
        })
    }
}

impl Command for AutoCut {
    fn label(&self) -> CommandLabel {
        CommandLabel::AutoCut
    }
    fn apply(&mut self, project: &mut Project) {
        project.timelines[self.timeline] = self.after.clone();
        if let Some(c) = &self.captions_after {
            project.captions.insert(self.timeline, c.clone());
        }
    }
    fn undo(&self, project: &mut Project) {
        project.timelines[self.timeline] = self.before.clone();
        if let Some(c) = &self.captions_before {
            project.captions.insert(self.timeline, c.clone());
        } else {
            project.captions.remove(&self.timeline);
        }
    }
}

#[cfg(test)]
#[path = "tests/auto_cut_edit.rs"]
mod tests;
