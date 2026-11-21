//! Dubbing: phrases cut from the transcript, the fit of a synthesized line
//! into its window, and which tracks play for each language. Producing the
//! translation and the voice is the app's job.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::captions::CaptionWord;
use crate::{ClipId, Command, CommandLabel, Keyframed, Project, Timeline, TimelineId, TrackKind};

/// A word in timeline seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A phrase to dub, timeline seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

const PAUSE: f64 = 0.5;
const MIN_SENTENCE: f64 = 1.5;
pub const MAX_PHRASE: f64 = 12.0;

/// Phrases of `words` (sorted): a new one after a pause, after a sentence
/// end once the phrase is long enough, and before it grows past `MAX_PHRASE`.
pub fn phrases(words: &[TimedWord]) -> Vec<Phrase> {
    let mut out: Vec<Phrase> = Vec::new();
    let mut open: Option<Phrase> = None;
    for w in words.iter().filter(|w| !w.text.trim().is_empty()) {
        if let Some(p) = &open {
            let ends_sentence = p.text.trim_end().ends_with(['.', '!', '?']);
            if w.start - p.end >= PAUSE
                || (ends_sentence && p.end - p.start >= MIN_SENTENCE)
                || w.end - p.start > MAX_PHRASE
            {
                out.extend(open.take());
            }
        }
        match &mut open {
            Some(p) => {
                p.end = w.end.max(p.end);
                p.text.push(' ');
                p.text.push_str(w.text.trim());
            }
            None => {
                open = Some(Phrase {
                    start: w.start,
                    end: w.end,
                    text: w.text.trim().to_owned(),
                })
            }
        }
    }
    out.extend(open);
    out
}

/// Up to this much of the silence after a phrase may hold its dub.
pub const SPILL: f64 = 0.4;
const GAP_BEFORE_NEXT: f64 = 0.05;

/// The window each phrase's dub may fill: its own span, plus the start of
/// the silence before the next one.
pub fn windows(phrases: &[(f64, f64)], end_of_edit: f64) -> Vec<Range<f64>> {
    phrases
        .iter()
        .enumerate()
        .map(|(i, &(start, end))| {
            let next = phrases.get(i + 1).map_or(end_of_edit, |n| n.0);
            let end = end.min(next);
            start
                ..(end + SPILL)
                    .min(next - GAP_BEFORE_NEXT)
                    .max(end)
                    .max(start)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// Playback speed of the line: 1 or faster.
    pub speed: f64,
    /// Seconds it lasts at that speed.
    pub length: f64,
    /// Fits the window within the allowed stretch.
    pub fits: bool,
}

/// A line of `audio` seconds in a `window`: sped up when longer, never
/// beyond `1 + max_stretch` (then it overflows and is cut at the window end).
pub fn fit(audio: f64, window: f64, max_stretch: f64) -> Fit {
    let needed = if window > 0.0 {
        audio / window
    } else {
        f64::INFINITY
    };
    let limit = 1.0 + max_stretch.max(0.0);
    let speed = needed.clamp(1.0, limit);
    Fit {
        speed,
        length: (audio / speed).min(window.max(0.0)),
        fits: needed <= limit + 1e-9,
    }
}

/// How many characters a translation should aim for so that, read at the
/// pace of the original, it fills the same window.
pub fn target_chars(source: &str, audio: Option<f64>, window: f64) -> usize {
    let chars = source.chars().count().max(1);
    match audio {
        Some(a) if a > 0.0 => ((chars as f64 * window / a).floor() as usize).max(4),
        _ => chars,
    }
}

/// The words of `text` spread over `start..end`, each by its length, for
/// subtitles in the language of the dub.
pub fn caption_words(text: &str, start: f64, end: f64) -> Vec<CaptionWord> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let weights: Vec<f64> = words
        .iter()
        .map(|w| w.chars().count() as f64 + 1.0)
        .collect();
    let total: f64 = weights.iter().sum();
    let mut at = start;
    words
        .iter()
        .zip(&weights)
        .map(|(w, weight)| {
            let next = at + (end - start) * weight / total.max(1.0);
            let word = CaptionWord {
                text: (*w).to_owned(),
                start_ms: (at * 1000.0).round().max(0.0) as u32,
                end_ms: (next * 1000.0).round().max(0.0) as u32,
            };
            at = next;
            word
        })
        .collect()
}

/// A clip's gain before and after a dub lowered it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ducked {
    pub clip: ClipId,
    pub before: Keyframed<f32>,
    pub after: Keyframed<f32>,
}

/// What makes a track part of a dub.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DubTrack {
    /// ISO 639-1 code.
    pub language: String,
    /// Voice id; empty on the subtitle track.
    #[serde(default)]
    pub voice: String,
    /// The original voice and the music lowered for this dub.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ducked: Vec<Ducked>,
}

/// Which dub plays: in the editor (one undo step) and on an export snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    /// Whatever the tracks are set to now.
    #[default]
    AsIs,
    Original,
    Dub(String),
}

pub fn languages(timeline: &Timeline) -> Vec<String> {
    let mut out: Vec<String> = timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .filter_map(|t| t.dub.as_ref().map(|d| d.language.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switch {
    /// Track index, muted.
    pub mutes: Vec<(usize, bool)>,
    /// Track index, clip, gain.
    pub gains: Vec<(usize, ClipId, Keyframed<f32>)>,
}

/// What changes so that `choice` plays: the other dubs and their subtitles
/// muted, the original subtitles muted when the chosen dub has its own, and
/// the gains each dub saved restored. Clips edited since the dub keep their gain.
pub fn switch(timeline: &Timeline, choice: &Language) -> Switch {
    let chosen = match choice {
        Language::AsIs => return Switch::default(),
        Language::Original => None,
        Language::Dub(l) => Some(l.as_str()),
    };
    let mut out = Switch::default();
    let has_own_subtitles = timeline.tracks.iter().any(|t| {
        t.kind == TrackKind::Video
            && t.dub
                .as_ref()
                .is_some_and(|d| Some(d.language.as_str()) == chosen)
    });
    for (i, track) in timeline.tracks.iter().enumerate() {
        let muted = match &track.dub {
            Some(d) => Some(d.language.as_str()) != chosen,
            None if track.subtitles && timeline.tracks.iter().any(|t| t.dub.is_some()) => {
                has_own_subtitles
            }
            None => continue,
        };
        if track.muted != muted {
            out.mutes.push((i, muted));
        }
    }
    let ducked: Vec<(&str, &Ducked)> = timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .filter_map(|t| t.dub.as_ref())
        .flat_map(|d| d.ducked.iter().map(move |k| (d.language.as_str(), k)))
        .collect();
    let mut seen: Vec<ClipId> = Vec::new();
    for &(_, k) in &ducked {
        if seen.contains(&k.clip) {
            continue;
        }
        seen.push(k.clip);
        let Some((track, clip)) = timeline
            .tracks
            .iter()
            .enumerate()
            .find_map(|(i, t)| t.clip(k.clip).map(|c| (i, c)))
        else {
            continue;
        };
        let ours = ducked
            .iter()
            .any(|(_, d)| d.clip == k.clip && clip.effects.gain_db == d.after)
            || clip.effects.gain_db == k.before;
        if !ours {
            continue;
        }
        let wanted = ducked
            .iter()
            .find(|(l, d)| Some(*l) == chosen && d.clip == k.clip)
            .map_or(&k.before, |(_, d)| &d.after);
        if clip.effects.gain_db != *wanted {
            out.gains.push((track, k.clip, wanted.clone()));
        }
    }
    out
}

/// `switch` applied directly, for the export snapshot.
pub fn apply_language(project: &mut Project, timeline: TimelineId, choice: &Language) {
    let Some(tl) = project.timelines.get_mut(timeline) else {
        return;
    };
    let change = switch(tl, choice);
    for (track, muted) in change.mutes {
        tl.tracks[track].muted = muted;
    }
    for (track, clip, gain) in change.gains {
        if let Some(c) = tl.tracks[track].clip_mut(clip) {
            c.effects.gain_db = gain;
        }
    }
}

/// Sets or clears the dub tag of a track.
#[derive(Debug)]
pub struct SetTrackDub {
    pub timeline: TimelineId,
    pub track_index: usize,
    value: Option<DubTrack>,
    old: Option<Option<DubTrack>>,
}

impl SetTrackDub {
    pub fn new(timeline: TimelineId, track_index: usize, value: Option<DubTrack>) -> Self {
        Self {
            timeline,
            track_index,
            value,
            old: None,
        }
    }
}

impl Command for SetTrackDub {
    fn label(&self) -> CommandLabel {
        CommandLabel::Dub
    }

    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.timelines[self.timeline]
            .tracks
            .get_mut(self.track_index)
        {
            self.old = Some(std::mem::replace(&mut track.dub, self.value.clone()));
        }
    }

    fn undo(&self, project: &mut Project) {
        if let (Some(old), Some(track)) = (
            &self.old,
            project.timelines[self.timeline]
                .tracks
                .get_mut(self.track_index),
        ) {
            track.dub = old.clone();
        }
    }
}

#[cfg(test)]
#[path = "tests/dub.rs"]
mod tests;
