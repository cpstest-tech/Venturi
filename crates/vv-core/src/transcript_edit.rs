//! Editing through the transcript: what part of a clip goes when words are
//! deleted, and corrections of misheard words that reach the subtitles too.

use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auto_cut::{self, Energy, Word};
use crate::captions::{self, Captions};
use crate::{Clip, Command, CommandLabel, FrameIdx, Project, TimelineId};

/// A word heard wrong and fixed by hand. Keyed by the source and the word's
/// time in it, so it survives cuts, moves and reopening the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptEdit {
    pub path: PathBuf,
    pub stream: usize,
    /// Start of the word in the source.
    pub start_ms: u32,
    /// What the speech engine heard.
    pub original: String,
    pub text: String,
}

impl TranscriptEdit {
    fn same_word(&self, other: &Self) -> bool {
        self.path == other.path
            && self.stream == other.stream
            && self.start_ms == other.start_ms
            && self.original == other.original
    }
}

const SNAP_RADIUS: f64 = 0.08;

pub fn source_ms(seconds: f64) -> u32 {
    (seconds.max(0.0) * 1000.0).round() as u32
}

/// The correction of a source word, if any.
pub fn corrected<'a>(
    edits: &'a [TranscriptEdit],
    path: &Path,
    stream: usize,
    start_ms: u32,
    original: &str,
) -> Option<&'a str> {
    edits
        .iter()
        .find(|e| {
            e.path == path && e.stream == stream && e.start_ms == start_ms && e.original == original
        })
        .map(|e| e.text.as_str())
}

/// Sorted, deduplicated indices grouped into runs of consecutive ones.
pub fn runs(indices: impl IntoIterator<Item = usize>) -> Vec<Range<usize>> {
    let mut indices: Vec<usize> = indices.into_iter().collect();
    indices.sort_unstable();
    indices.dedup();
    let mut out: Vec<Range<usize>> = Vec::new();
    for i in indices {
        match out.last_mut() {
            Some(last) if last.end == i => last.end = i + 1,
            _ => out.push(i..i + 1),
        }
    }
    out
}

/// Seconds of the clip removed when the word `runs` are deleted (`words`
/// sorted, clip-local). Each side keeps up to `margin` of the pause next to
/// it, never cuts into a kept word and moves to the quietest nearby point,
/// so the join does not click or swallow a syllable.
pub fn word_cuts(
    words: &[Word],
    runs: &[Range<usize>],
    duration: f64,
    margin: f64,
    energy: &Energy,
) -> Vec<Range<f64>> {
    let margin = margin.max(0.0);
    let step = energy.step.max(1e-6);
    let mut out = Vec::new();
    for run in runs {
        if run.start >= run.end || run.end > words.len() {
            continue;
        }
        let first = &words[run.start];
        let last = &words[run.end - 1];
        let prev_end = run.start.checked_sub(1).map_or(0.0, |i| words[i].end);
        let next_start = words.get(run.end).map_or(duration, |w| w.start);
        let mut start = first.start.min(prev_end + margin).max(prev_end);
        let mut end = last.end.max(next_start - margin).min(next_start);
        let (lo, hi) = (
            (prev_end + margin * 0.5).max(start - SNAP_RADIUS),
            first.start.min(start + SNAP_RADIUS),
        );
        if hi - lo > step {
            start = auto_cut::snap(energy, start, lo, hi);
        }
        let (lo, hi) = (
            last.end.max(end - SNAP_RADIUS),
            (next_start - margin * 0.5).min(end + SNAP_RADIUS),
        );
        if hi - lo > step {
            end = auto_cut::snap(energy, end, lo, hi);
        }
        let (start, end) = (start.clamp(0.0, duration), end.clamp(0.0, duration));
        if end > start {
            out.push(start..end);
        }
    }
    auto_cut::merge_ranges(out)
}

fn token_matches(token: &str, word: &str) -> bool {
    let bare = |s: &str| {
        s.trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    token == word || (!bare(word).is_empty() && bare(token) == bare(word))
}

fn ms_to_frame(ms: u32, fps: f64) -> FrameIdx {
    (f64::from(ms) * fps / 1000.0).round() as FrameIdx
}

/// `old` becomes `new` in the saved subtitle words and in the subtitle clips
/// spanning `span_ms` (timeline ms). The timing is never touched.
fn patch_subtitles(
    project: &mut Project,
    timeline: TimelineId,
    span_ms: (u32, u32),
    old: &str,
    new: &str,
) {
    let overlap = |s: u32, e: u32| e.min(span_ms.1).saturating_sub(s.max(span_ms.0));
    if let Some(captions) = project.captions.get_mut(&timeline)
        && let Some(word) = captions
            .words
            .iter_mut()
            .filter(|w| overlap(w.start_ms, w.end_ms) > 0 && token_matches(&w.text, old))
            .max_by_key(|w| overlap(w.start_ms, w.end_ms))
    {
        word.text = new.to_owned();
    }
    let Some(tl) = project.timelines.get_mut(timeline) else {
        return;
    };
    let fps = tl.fps.as_f64();
    let Some(index) = captions::subtitle_track(tl) else {
        return;
    };
    let (from, to) = (ms_to_frame(span_ms.0, fps), ms_to_frame(span_ms.1, fps));
    for clip in &mut tl.tracks[index].clips {
        if clip.timeline_start > to || clip.timeline_end() < from {
            continue;
        }
        let Some(title) = &mut clip.effects.title else {
            continue;
        };
        let content = &title.content.default;
        let tokens: Vec<&str> = content.split(' ').collect();
        if let Some(at) = tokens.iter().position(|t| token_matches(t, old)) {
            let mut tokens: Vec<String> = tokens.into_iter().map(str::to_owned).collect();
            tokens[at] = new.to_owned();
            title.content.default = tokens.join(" ");
            break;
        }
    }
}

#[derive(Debug)]
struct Snapshot {
    edits: Vec<TranscriptEdit>,
    captions: Option<Captions>,
    subtitles: Option<(usize, Vec<Clip>)>,
}

/// Fixes one word of the transcript; the subtitles showing it follow.
#[derive(Debug)]
pub struct CorrectWord {
    timeline: TimelineId,
    edit: TranscriptEdit,
    /// The text shown before: what the subtitles hold now.
    shown: String,
    span_ms: (u32, u32),
    undo: Option<Snapshot>,
}

impl CorrectWord {
    pub fn new(
        timeline: TimelineId,
        edit: TranscriptEdit,
        shown: String,
        span_ms: (u32, u32),
    ) -> Self {
        Self {
            timeline,
            edit,
            shown,
            span_ms,
            undo: None,
        }
    }
}

impl Command for CorrectWord {
    fn label(&self) -> CommandLabel {
        CommandLabel::CorrectWord
    }

    fn apply(&mut self, project: &mut Project) {
        let subtitles = project
            .timelines
            .get(self.timeline)
            .and_then(|tl| captions::subtitle_track(tl).map(|i| (i, tl.tracks[i].clips.clone())));
        self.undo = Some(Snapshot {
            edits: project.transcript_edits.clone(),
            captions: project.captions.get(&self.timeline).cloned(),
            subtitles,
        });
        project
            .transcript_edits
            .retain(|e| !e.same_word(&self.edit));
        if self.edit.text != self.edit.original {
            project.transcript_edits.push(self.edit.clone());
        }
        patch_subtitles(
            project,
            self.timeline,
            self.span_ms,
            &self.shown,
            &self.edit.text,
        );
    }

    fn undo(&self, project: &mut Project) {
        let Some(snapshot) = &self.undo else {
            return;
        };
        project.transcript_edits = snapshot.edits.clone();
        match &snapshot.captions {
            Some(c) => {
                project.captions.insert(self.timeline, c.clone());
            }
            None => {
                project.captions.remove(&self.timeline);
            }
        }
        if let (Some((index, clips)), Some(tl)) = (
            &snapshot.subtitles,
            project.timelines.get_mut(self.timeline),
        ) && let Some(track) = tl.tracks.get_mut(*index)
        {
            track.clips = clips.clone();
        }
    }
}

#[cfg(test)]
#[path = "tests/transcript_edit.rs"]
mod tests;
