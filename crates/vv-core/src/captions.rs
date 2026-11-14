//! Subtitles: the transcription (words with times) lives in the project apart
//! from its look. A template turns the words into text clips on the
//! subtitle track; changing the template rebuilds the clips from the words.

use serde::{Deserialize, Serialize};

use crate::command::{Command, CommandLabel};
use crate::model::{
    Clip, ClipFilter, ClipSource, FilterKind, FontCase, FrameIdx, HAnchor, Keyframed, Project,
    Rational, Rgba, TextAlign, TimelineId, TitleBackground, TitleParams, Track, TrackKind, VAnchor,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionWord {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Captions {
    pub language: String,
    pub words: Vec<CaptionWord>,
    /// Id of the `SubtitleTemplate` the clips were last built with.
    pub template: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    WordByWord,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reveal {
    /// One clip per line.
    Line,
    /// One clip per word, showing that word alone.
    Single,
    /// One clip per word, showing the line up to that word.
    Cumulative,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubtitleTemplate {
    pub id: &'static str,
    pub category: TemplateCategory,
    reveal: Reveal,
    /// Of the frame height.
    size: f32,
    weight: u16,
    case: FontCase,
    color: [f32; 3],
    /// Alpha of the box behind the text, 0 = none.
    box_alpha: f32,
    /// A dark halo behind the text (the Drop Shadow filter).
    halo: bool,
}

pub const DEFAULT_TEMPLATE: &str = "line_classic";

const fn template(
    id: &'static str,
    category: TemplateCategory,
    reveal: Reveal,
    size: f32,
    weight: u16,
    case: FontCase,
    color: [f32; 3],
    box_alpha: f32,
    halo: bool,
) -> SubtitleTemplate {
    SubtitleTemplate {
        id,
        category,
        reveal,
        size,
        weight,
        case,
        color,
        box_alpha,
        halo,
    }
}

use TemplateCategory::{Line, WordByWord};

pub const TEMPLATES: [SubtitleTemplate; 6] = [
    template(
        "word_pop",
        WordByWord,
        Reveal::Single,
        0.11,
        800,
        FontCase::Upper,
        [1.0, 1.0, 1.0],
        0.0,
        true,
    ),
    template(
        "word_yellow",
        WordByWord,
        Reveal::Single,
        0.11,
        800,
        FontCase::Upper,
        [1.0, 0.85, 0.1],
        0.0,
        true,
    ),
    template(
        "word_build",
        WordByWord,
        Reveal::Cumulative,
        0.07,
        700,
        FontCase::Mixed,
        [1.0, 1.0, 1.0],
        0.0,
        true,
    ),
    template(
        "line_classic",
        Line,
        Reveal::Line,
        0.055,
        600,
        FontCase::Mixed,
        [1.0, 1.0, 1.0],
        0.6,
        false,
    ),
    template(
        "line_outline",
        Line,
        Reveal::Line,
        0.06,
        800,
        FontCase::Mixed,
        [1.0, 1.0, 1.0],
        0.0,
        true,
    ),
    template(
        "line_yellow",
        Line,
        Reveal::Line,
        0.055,
        700,
        FontCase::Mixed,
        [1.0, 0.85, 0.1],
        0.0,
        true,
    ),
];

impl SubtitleTemplate {
    pub fn find(id: &str) -> Option<&'static SubtitleTemplate> {
        TEMPLATES.iter().find(|t| t.id == id)
    }

    pub fn default_template() -> &'static SubtitleTemplate {
        Self::find(DEFAULT_TEMPLATE).expect("the default template exists")
    }

    /// The text style for a timeline of `resolution`, with `text` as content.
    pub fn title(&self, text: &str, resolution: (u32, u32)) -> TitleParams {
        let height = resolution.1 as f32;
        let [r, g, b] = self.color;
        let mut title = TitleParams {
            font_weight: self.weight,
            color: Rgba { r, g, b, a: 1.0 },
            size: (height * self.size).round().max(8.0),
            case: self.case,
            align: TextAlign::Center,
            anchor: (HAnchor::Center, VAnchor::Bottom),
            position: [0.0, -height * 0.4],
            ..TitleParams::default()
        };
        title.content = Keyframed::constant(text.to_owned());
        if self.box_alpha > 0.0 {
            title.background = TitleBackground {
                enabled: true,
                color: Rgba::BLACK,
                opacity: self.box_alpha * 100.0,
                ..TitleBackground::default()
            };
        }
        title
    }
}

/// A line of captions: what an SRT cue holds.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionLine {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
    /// Indices of its words in the transcription.
    pub words: std::ops::Range<usize>,
}

const MAX_LINE_CHARS: usize = 32;
const MAX_LINE_MS: u32 = 4500;
const PAUSE_MS: u32 = 700;
const HOLD_MS: u32 = 300;

fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', ')', '»'])
        .ends_with(['.', '!', '?', '…'])
}

/// Splits the words where a viewer needs a new line: too long, a pause,
/// the end of a sentence.
pub fn caption_lines(words: &[CaptionWord]) -> Vec<CaptionLine> {
    caption_lines_within(words, MAX_LINE_CHARS)
}

/// `caption_lines` with lines of at most `max_chars` characters.
pub fn caption_lines_within(words: &[CaptionWord], max_chars: usize) -> Vec<CaptionLine> {
    let mut lines = Vec::new();
    let mut first = 0;
    let mut chars = 0;
    for (i, word) in words.iter().enumerate() {
        let width = word.text.chars().count();
        if i > first {
            let previous = &words[i - 1];
            let too_long = chars + 1 + width > max_chars
                || word.end_ms.saturating_sub(words[first].start_ms) > MAX_LINE_MS;
            let pause = word.start_ms.saturating_sub(previous.end_ms) > PAUSE_MS;
            if too_long || pause || ends_sentence(&previous.text) {
                lines.push(line_of(words, first..i));
                first = i;
                chars = 0;
            }
        }
        chars += if chars == 0 { width } else { 1 + width };
    }
    if first < words.len() {
        lines.push(line_of(words, first..words.len()));
    }
    lines
}

fn line_of(words: &[CaptionWord], range: std::ops::Range<usize>) -> CaptionLine {
    let slice = &words[range.clone()];
    CaptionLine {
        text: slice
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        start_ms: slice[0].start_ms,
        end_ms: slice[slice.len() - 1].end_ms,
        words: range,
    }
}

/// `(start_ms, end_ms, text)` of every clip the template makes.
fn cues(words: &[CaptionWord], reveal: Reveal, max_chars: usize) -> Vec<(u32, u32, String)> {
    let lines = caption_lines_within(words, max_chars);
    let mut cues = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let next_line_start = lines.get(n + 1).map(|l| l.start_ms);
        let line_end = hold(line.end_ms, next_line_start);
        match reveal {
            Reveal::Line => cues.push((line.start_ms, line_end, line.text.clone())),
            Reveal::Single | Reveal::Cumulative => {
                for i in line.words.clone() {
                    let end = if i + 1 < line.words.end {
                        words[i + 1]
                            .start_ms
                            .max(words[i].end_ms.min(words[i + 1].start_ms))
                    } else {
                        line_end
                    };
                    let text = match reveal {
                        Reveal::Single => words[i].text.clone(),
                        _ => words[line.words.start..=i]
                            .iter()
                            .map(|w| w.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    };
                    cues.push((words[i].start_ms, end, text));
                }
            }
        }
    }
    cues
}

/// A cue stays a little after its last word, without reaching the next one.
fn hold(end_ms: u32, next_start: Option<u32>) -> u32 {
    let held = end_ms + HOLD_MS;
    next_start.map_or(held, |next| held.min(next.max(end_ms)))
}

fn to_frames(ms: u32, fps: Rational) -> FrameIdx {
    let (num, den) = (i128::from(fps.num), i128::from(fps.den.max(1)));
    ((2 * i128::from(ms) * num + den * 1000) / (2 * den * 1000)) as FrameIdx
}

/// The clips of `words` in `template`, in order and never overlapping.
/// Ids come from `project`.
pub fn build_clips(
    project: &mut Project,
    timeline: TimelineId,
    words: &[CaptionWord],
    template: &SubtitleTemplate,
) -> Vec<Clip> {
    build_clips_with(
        project,
        timeline,
        words,
        template.reveal,
        template.halo,
        &|text, resolution| template.title(text, resolution),
    )
}

/// `build_clips` with any look: `title` styles the text of each clip. A
/// title with karaoke gets the times of its words.
pub fn build_clips_with(
    project: &mut Project,
    timeline: TimelineId,
    words: &[CaptionWord],
    reveal: Reveal,
    halo: bool,
    title: &dyn Fn(&str, (u32, u32)) -> TitleParams,
) -> Vec<Clip> {
    let Some(t) = project.timelines.get(timeline) else {
        return Vec::new();
    };
    let (fps, resolution) = (t.fps, t.resolution);
    let mut clips: Vec<Clip> = Vec::new();
    let mut cursor: FrameIdx = 0;
    let max_chars = max_chars_for(&title("0", resolution), resolution);
    for (start_ms, end_ms, text) in cues(words, reveal, max_chars) {
        let start = to_frames(start_ms, fps).max(cursor);
        let end = to_frames(end_ms, fps).max(start + 1);
        let id = project.alloc_clip_id();
        let mut clip =
            Clip::from_source_range(id, ClipSource::Text, 0, end - start, start, Rational::one());
        let mut styled = title(&text, resolution);
        let estimated = text.chars().count() as f32 * styled.size * 0.6;
        let room = resolution.0 as f32 * 0.88;
        if estimated > room {
            styled.size = (styled.size * room / estimated).floor().max(8.0);
        }
        if let Some(karaoke) = &mut styled.anim.karaoke
            && let Some(times) =
                words_within(words, start_ms, end_ms, text.split_whitespace().count())
        {
            karaoke.word_ms = times;
        }
        clip.effects.title = Some(styled);
        if halo {
            clip.effects.filters.push(halo_filter(resolution.1));
        }
        cursor = end;
        clips.push(clip);
    }
    clips
}

/// Characters of a line that fit the frame width at the title's size.
fn max_chars_for(title: &TitleParams, resolution: (u32, u32)) -> usize {
    let fit = resolution.0 as f32 * 0.85 / (title.size.max(1.0) * 0.6);
    (fit as usize).clamp(6, MAX_LINE_CHARS)
}

fn halo_filter(height: u32) -> ClipFilter {
    let height = height as f32;
    ClipFilter {
        radius: Keyframed::constant(height * 0.004),
        amount: Keyframed::constant(100.0),
        grow: Keyframed::constant(height * 0.003),
        offset: [Keyframed::constant(0.0), Keyframed::constant(0.0)],
        color: Rgba::BLACK,
        ..ClipFilter::new(FilterKind::DropShadow)
    }
}

/// The words of `captions` spoken inside `start_ms..end_ms`, as `[start, end]`
/// in ms from `start_ms`; `None` unless there are exactly `expected` of them
/// (the words of the clip's text).
pub fn clip_word_ms(
    captions: &Captions,
    start_ms: u32,
    end_ms: u32,
    expected: usize,
) -> Option<Vec<[u32; 2]>> {
    words_within(&captions.words, start_ms, end_ms, expected)
}

fn words_within(
    all: &[CaptionWord],
    start_ms: u32,
    end_ms: u32,
    expected: usize,
) -> Option<Vec<[u32; 2]>> {
    let words: Vec<[u32; 2]> = all
        .iter()
        .filter(|w| w.start_ms >= start_ms && w.start_ms < end_ms)
        .map(|w| [w.start_ms - start_ms, w.end_ms.saturating_sub(start_ms)])
        .collect();
    (words.len() == expected).then_some(words)
}

/// The index of the subtitle track, if the timeline has one.
pub fn subtitle_track(timeline: &crate::model::Timeline) -> Option<usize> {
    timeline.tracks.iter().position(|t| t.subtitles)
}

/// Whether the timeline shows subtitles, generated or not.
pub fn has_subtitles(project: &Project, timeline: TimelineId) -> bool {
    project.captions.contains_key(&timeline)
        || project
            .timelines
            .get(timeline)
            .and_then(subtitle_track)
            .is_some_and(|i| !project.timelines[timeline].tracks[i].clips.is_empty())
}

fn srt_time(ms: u32) -> String {
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn to_srt(words: &[CaptionWord]) -> String {
    let mut out = String::new();
    for (n, line) in caption_lines(words).iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            n + 1,
            srt_time(line.start_ms),
            srt_time(line.end_ms.max(line.start_ms + 1)),
            line.text
        ));
    }
    out
}

pub fn to_txt(words: &[CaptionWord]) -> String {
    let mut text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    text.push('\n');
    text
}

/// SRT of the text clips of the subtitle track, for subtitles without a
/// transcription (or edited by hand).
pub fn clips_to_srt(timeline: &crate::model::Timeline) -> String {
    let Some(track) = subtitle_track(timeline).map(|i| &timeline.tracks[i]) else {
        return String::new();
    };
    let to_ms =
        |frame: FrameIdx| (frame.max(0) as f64 / timeline.fps.as_f64() * 1000.0).round() as u32;
    let mut out = String::new();
    for (n, clip) in track.clips.iter().enumerate() {
        let text = clip
            .effects
            .title
            .as_ref()
            .map(|t| t.content.default.clone())
            .unwrap_or_default();
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            n + 1,
            srt_time(to_ms(clip.timeline_start)),
            srt_time(to_ms(clip.timeline_end())),
            text
        ));
    }
    out
}

/// The texts of the subtitle track's clips as running text.
pub fn clips_to_txt(timeline: &crate::model::Timeline) -> String {
    let Some(track) = subtitle_track(timeline).map(|i| &timeline.tracks[i]) else {
        return String::new();
    };
    let mut text = track
        .clips
        .iter()
        .filter_map(|c| c.effects.title.as_ref())
        .map(|t| t.content.default.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    text.push('\n');
    text
}

/// Replaces the subtitle track's clips and the stored transcription in one
/// step. `captions: None` removes the transcription too.
#[derive(Debug)]
pub struct SetCaptions {
    timeline: TimelineId,
    captions: Option<Captions>,
    clips: Vec<Clip>,
    undo: Option<Undo>,
}

#[derive(Debug)]
struct Undo {
    captions: Option<Captions>,
    /// The track index and its previous clips; `None` if the command made the track.
    clips: Option<(usize, Vec<Clip>)>,
}

impl SetCaptions {
    pub fn new(timeline: TimelineId, captions: Option<Captions>, clips: Vec<Clip>) -> Self {
        Self {
            timeline,
            captions,
            clips,
            undo: None,
        }
    }
}

impl Command for SetCaptions {
    fn label(&self) -> CommandLabel {
        CommandLabel::SetCaptions
    }

    fn apply(&mut self, project: &mut Project) {
        let previous_captions = match &self.captions {
            Some(captions) => project.captions.insert(self.timeline, captions.clone()),
            None => project.captions.remove(&self.timeline),
        };
        let Some(timeline) = project.timelines.get_mut(self.timeline) else {
            return;
        };
        let clips = std::mem::take(&mut self.clips);
        let previous_clips = match subtitle_track(timeline) {
            Some(index) => {
                let old = std::mem::replace(&mut timeline.tracks[index].clips, clips);
                Some((index, old))
            }
            None => {
                let mut track = Track::new(TrackKind::Video);
                track.subtitles = true;
                track.clips = clips;
                timeline.tracks.push(track);
                None
            }
        };
        self.undo = Some(Undo {
            captions: previous_captions,
            clips: previous_clips,
        });
    }

    fn undo(&self, project: &mut Project) {
        let Some(undo) = &self.undo else {
            return;
        };
        match &undo.captions {
            Some(captions) => {
                project.captions.insert(self.timeline, captions.clone());
            }
            None => {
                project.captions.remove(&self.timeline);
            }
        }
        let Some(timeline) = project.timelines.get_mut(self.timeline) else {
            return;
        };
        match &undo.clips {
            Some((index, clips)) => {
                if let Some(track) = timeline.tracks.get_mut(*index) {
                    track.clips = clips.clone();
                }
            }
            None => {
                if let Some(index) = subtitle_track(timeline) {
                    timeline.tracks.remove(index);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/captions.rs"]
mod tests;
