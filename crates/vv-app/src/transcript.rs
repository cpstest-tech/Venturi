//! Text-based editing: the words of the Main Track clips (the Automatic cut
//! source cache, so one transcription serves both) placed in timeline time,
//! with the pauses, false starts and fillers of the same analysis. Deleting
//! words runs the Automatic cut command on the clips they belong to.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use vv_core::auto_cut::{self, Config, Energy, SegmentType, Word};
use vv_core::transcript_edit::{self, CorrectWord, TranscriptEdit};
use vv_core::{Clip, ClipId, ClipSource, CommandLabel, FrameIdx, TimelineId, TrackKind};

use crate::VenturiApp;
use crate::auto_cut::{Cached, SourceJob, SourceKey, SpeechSource, clip_local, start_source};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Speech,
    FalseStart,
    Filler,
}

pub(crate) struct TWord {
    pub text: String,
    pub original: String,
    pub source_ms: u32,
    /// Timeline seconds.
    pub start: f64,
    pub end: f64,
    pub confidence: Option<f32>,
    pub clip: usize,
    /// Index among the words of its clip.
    pub local_index: usize,
    pub mark: Mark,
}

pub(crate) struct TPause {
    pub clip: usize,
    /// Timeline seconds.
    pub start: f64,
    pub end: f64,
    /// Clip-local seconds: what a cut removes.
    pub local: Range<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Word(usize),
    Pause(usize),
}

pub(crate) struct TClip {
    pub track: usize,
    pub id: ClipId,
    /// Timeline seconds.
    pub start: f64,
    pub duration: f64,
    pub path: PathBuf,
    pub stream: usize,
    pub words: Vec<Word>,
    pub energy: Energy,
    pub sentences: Vec<Range<usize>>,
    /// Index of its first word in `Model::words`.
    pub first_word: usize,
}

pub(crate) struct Model {
    pub timeline: TimelineId,
    pub stamp: (u64, u64, u64),
    pub clips: Vec<TClip>,
    pub words: Vec<TWord>,
    pub pauses: Vec<TPause>,
    pub items: Vec<Item>,
    /// Sources of candidate clips not in the session cache yet.
    pub missing: Vec<(SourceKey, SpeechSource)>,
    /// Candidate clips that cannot be read: offline file, unsupported.
    pub unreadable: usize,
    pub candidates: usize,
}

impl Model {
    pub fn item_span(&self, item: Item) -> (f64, f64) {
        match item {
            Item::Word(w) => (self.words[w].start, self.words[w].end),
            Item::Pause(p) => (self.pauses[p].start, self.pauses[p].end),
        }
    }

    pub fn item_clip(&self, item: Item) -> usize {
        match item {
            Item::Word(w) => self.words[w].clip,
            Item::Pause(p) => self.pauses[p].clip,
        }
    }

    /// The word spoken at `seconds`.
    pub fn word_at(&self, seconds: f64) -> Option<usize> {
        let i = self.words.partition_point(|w| w.start <= seconds);
        let w = i.checked_sub(1)?;
        (seconds < self.words[w].end).then_some(w)
    }

    /// Item indices of the sentence holding `word`.
    pub fn sentence_items(&self, word: usize) -> Vec<usize> {
        let w = &self.words[word];
        let clip = &self.clips[w.clip];
        let Some(range) = clip.sentences.iter().find(|r| r.contains(&w.local_index)) else {
            return Vec::new();
        };
        let words: Range<usize> = clip.first_word + range.start..clip.first_word + range.end;
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item, Item::Word(i) if words.contains(i)))
            .map(|(i, _)| i)
            .collect()
    }

    /// Item index of `word`.
    pub fn word_item(&self, word: usize) -> Option<usize> {
        self.items.iter().position(|i| *i == Item::Word(word))
    }

    /// Item indices of the words marked `mark` and, with `pauses`, every pause.
    pub fn marked_items(&self, mark: Option<Mark>, pauses: bool) -> BTreeSet<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| match item {
                Item::Word(w) => mark.is_some_and(|m| self.words[*w].mark == m),
                Item::Pause(_) => pauses,
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// Lowercase letters and digits only: what search and matching compare.
pub(crate) fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Word ranges matching `query`: one word per query word, the last one
/// matched as a prefix, so «ciao mon» finds «Ciao, mondo».
pub(crate) fn search(words: &[TWord], query: &str) -> Vec<Range<usize>> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(normalize)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let normalized: Vec<String> = words.iter().map(|w| normalize(&w.text)).collect();
    (0..normalized.len().saturating_sub(tokens.len() - 1))
        .filter(|&i| {
            tokens.iter().enumerate().all(|(k, token)| {
                let word = &normalized[i + k];
                if tokens.len() == 1 {
                    word.contains(token.as_str())
                } else if k + 1 == tokens.len() {
                    word.starts_with(token.as_str())
                } else {
                    word == token
                }
            })
        })
        .map(|i| i..i + tokens.len())
        .collect()
}

/// Words and pauses in time order, pauses before the word they precede.
pub(crate) fn interleave(words: &[TWord], pauses: &[TPause]) -> Vec<Item> {
    let mut items = Vec::with_capacity(words.len() + pauses.len());
    let mut p = 0;
    for (w, word) in words.iter().enumerate() {
        while p < pauses.len()
            && (pauses[p].clip < word.clip
                || (pauses[p].clip == word.clip && pauses[p].start <= word.start))
        {
            items.push(Item::Pause(p));
            p += 1;
        }
        items.push(Item::Word(w));
    }
    items.extend((p..pauses.len()).map(Item::Pause));
    items
}

#[derive(Default)]
struct Jobs {
    queue: VecDeque<(SourceKey, SpeechSource, bool)>,
    running: Option<(SourceKey, SourceJob, bool)>,
    /// Last failure per source: `no_transcript`, `model_missing`...
    errors: HashMap<SourceKey, String>,
    looked_up: HashSet<SourceKey>,
    /// Sources of the current «Genera trascrizione» batch, and how many are done.
    batch: (usize, usize),
}

#[derive(Default)]
pub(crate) struct State {
    pub model: Option<Model>,
    /// Indices into `Model::items`.
    pub selection: BTreeSet<usize>,
    pub anchor: Option<usize>,
    /// The panel got the last click: Del, Ctrl+A and Esc are its own.
    pub focused: bool,
    pub delete_requested: bool,
    pub select_all_requested: bool,
    pub search: String,
    pub match_index: usize,
    /// The word being corrected and the text typed so far.
    pub editing: Option<(usize, String)>,
    pub error: Option<String>,
    pub layout: Option<crate::transcript_ui::Layout>,
    /// Current word when the view last followed playback.
    pub followed: Option<usize>,
    jobs: Jobs,
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some((_, job, _)) = &self.jobs.running {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

pub(crate) enum JobStatus {
    Idle,
    LookingUp,
    Transcribing {
        stage: u8,
        fraction: f32,
        done: usize,
        total: usize,
    },
}

/// The clip of `source` re-expressed as transcript words, the corrections
/// applied. Words cut in half by the clip edge count if their middle is in.
fn clip_words(
    source: &Cached,
    spec: &SpeechSource,
    duration: f64,
    edits: &[TranscriptEdit],
) -> Vec<(Word, u32, String)> {
    source
        .words
        .iter()
        .filter_map(|w| {
            let start = (w.start - spec.media_start) / spec.media_speed;
            let end = (w.end - spec.media_start) / spec.media_speed;
            let middle = (start + end) / 2.0;
            if !(0.0..duration).contains(&middle) || end <= start {
                return None;
            }
            let ms = transcript_edit::source_ms(w.start);
            let text = transcript_edit::corrected(edits, &spec.path, spec.stream, ms, &w.text)
                .unwrap_or(&w.text)
                .to_owned();
            Some((
                Word {
                    text,
                    start: start.max(0.0),
                    end: end.min(duration),
                    confidence: w.confidence,
                },
                ms,
                w.text.clone(),
            ))
        })
        .collect()
}

impl VenturiApp {
    pub(crate) fn transcript_config(&self) -> Config {
        Config {
            padding: self.settings.shorts.transcript.margin,
            crossfade_seconds: self.settings.shorts.transcript.crossfade,
            ..Config::default()
        }
    }

    /// The clips the transcript follows: the Main Track's, else those of the
    /// first audio track holding media.
    pub(crate) fn transcript_candidates(&self, timeline: TimelineId) -> Vec<(usize, Clip)> {
        let tl = &self.session.project.timelines[timeline];
        let with_audio = |track: usize| -> Vec<(usize, Clip)> {
            tl.tracks[track]
                .clips
                .iter()
                .filter(|c| matches!(c.source, ClipSource::Media(_)) && !c.disabled)
                .filter(|c| {
                    self.speech_source(timeline, c)
                        .is_some_and(|s| s.error != Some("unsupported"))
                })
                .map(|c| (track, c.clone()))
                .collect()
        };
        let main = tl.main_track_index().map(with_audio).unwrap_or_default();
        if !main.is_empty() {
            return main;
        }
        tl.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == TrackKind::Audio && t.dub.is_none())
            .map(|(i, _)| with_audio(i))
            .find(|clips| !clips.is_empty())
            .unwrap_or_default()
    }

    /// Where someone speaks, timeline seconds: the transcript words, and the
    /// whole span of a speech clip with no transcript yet.
    pub(crate) fn speech_spans(&mut self, timeline: TimelineId) -> Vec<Range<f64>> {
        self.refresh_transcript();
        let fps = self.session.project.timelines[timeline].fps.as_f64();
        let mut spans: Vec<Range<f64>> = self
            .transcript
            .model
            .as_ref()
            .filter(|m| m.timeline == timeline)
            .map_or_else(Vec::new, |m| {
                m.words.iter().map(|w| w.start..w.end).collect()
            });
        let main = self.session.project.timelines[timeline].main_track_index();
        for (track, clip) in self.transcript_candidates(timeline) {
            let transcribed = self
                .speech_source(timeline, &clip)
                .is_some_and(|spec| self.cached_speech(&spec).is_some());
            // An audio track standing in for the Main Track may be music:
            // only its words count until it is transcribed.
            if !transcribed && Some(track) == main {
                spans.push(clip.timeline_start as f64 / fps..clip.timeline_end() as f64 / fps);
            }
        }
        spans
    }

    /// Rebuilds the model when the timeline, the cache or the settings moved.
    pub(crate) fn refresh_transcript(&mut self) {
        let Some(timeline) = self.timeline_id else {
            self.transcript.model = None;
            return;
        };
        let stamp = (
            self.session.history.generation(),
            self.auto_cut.cache_revision,
            self.settings.shorts.transcript.margin.to_bits(),
        );
        if self
            .transcript
            .model
            .as_ref()
            .is_some_and(|m| m.timeline == timeline && m.stamp == stamp)
        {
            return;
        }
        let model = self.build_transcript(timeline, stamp);
        let same_items = self
            .transcript
            .model
            .as_ref()
            .is_some_and(|old| old.timeline == timeline && old.items == model.items);
        if !same_items {
            self.transcript.selection.clear();
            self.transcript.anchor = None;
            self.transcript.editing = None;
            self.transcript.layout = None;
        }
        self.transcript.model = Some(model);
    }

    fn build_transcript(&self, timeline: TimelineId, stamp: (u64, u64, u64)) -> Model {
        let tl = &self.session.project.timelines[timeline];
        let fps = tl.fps.as_f64();
        let config = self.transcript_config();
        let edits = &self.session.project.transcript_edits;
        let mut candidates = self.transcript_candidates(timeline);
        candidates.sort_by_key(|(_, c)| c.timeline_start);
        let mut model = Model {
            timeline,
            stamp,
            clips: Vec::new(),
            words: Vec::new(),
            pauses: Vec::new(),
            items: Vec::new(),
            missing: Vec::new(),
            unreadable: 0,
            candidates: candidates.len(),
        };
        for (track, clip) in candidates {
            let Some(spec) = self.speech_source(timeline, &clip) else {
                continue;
            };
            let Some(key) = spec.key.clone().filter(|_| spec.error.is_none()) else {
                model.unreadable += 1;
                continue;
            };
            let Some(source) = self.cached_speech(&spec) else {
                if !model.missing.iter().any(|(k, _)| *k == key) {
                    model.missing.push((key, spec));
                }
                continue;
            };
            let duration = clip.timeline_len as f64 / fps;
            let (_, energy) = clip_local(&source, spec.media_start, spec.media_speed, duration);
            let found = clip_words(&source, &spec, duration, edits);
            let words: Vec<Word> = found.iter().map(|(w, _, _)| w.clone()).collect();
            let segments = auto_cut::analyze(&words, &energy, duration, &config);
            let index = model.clips.len();
            let start = clip.timeline_start as f64 / fps;
            let first_word = model.words.len();
            for (local_index, (word, ms, original)) in found.into_iter().enumerate() {
                let middle = (word.start + word.end) / 2.0;
                let mark = segments
                    .iter()
                    .find(|s| s.start <= middle && middle < s.end)
                    .map_or(Mark::Speech, |s| match s.kind {
                        SegmentType::FalseStart => Mark::FalseStart,
                        SegmentType::Filler => Mark::Filler,
                        _ => Mark::Speech,
                    });
                model.words.push(TWord {
                    text: word.text,
                    original,
                    source_ms: ms,
                    start: start + word.start,
                    end: start + word.end,
                    confidence: word.confidence,
                    clip: index,
                    local_index,
                    mark,
                });
            }
            model.pauses.extend(
                segments
                    .iter()
                    .filter(|s| s.kind == SegmentType::Silence)
                    .map(|s| TPause {
                        clip: index,
                        start: start + s.start,
                        end: start + s.end,
                        local: s.start..s.end,
                    }),
            );
            model.clips.push(TClip {
                track,
                id: clip.id,
                start,
                duration,
                path: spec.path.clone(),
                stream: spec.stream,
                sentences: auto_cut::sentences(&words, &config),
                words,
                energy,
                first_word,
            });
        }
        model.items = interleave(&model.words, &model.pauses);
        model
    }

    /// Starts the next lookup or transcription; collects finished ones.
    pub(crate) fn poll_transcript_jobs(&mut self, ctx: &egui::Context) {
        let finished = self
            .transcript
            .jobs
            .running
            .as_ref()
            .and_then(|(_, job, _)| job.poll());
        if let Some(result) = finished {
            let (key, _, transcribing) = self.transcript.jobs.running.take().unwrap();
            if transcribing {
                self.transcript.jobs.batch.1 += 1;
            }
            match result {
                Ok(source) => {
                    self.transcript.jobs.errors.remove(&key);
                    self.remember_speech(&key, &source);
                }
                Err(error) => {
                    if error == "cancelled" {
                        self.transcript.jobs.queue.clear();
                    }
                    self.transcript.jobs.errors.insert(key, error);
                }
            }
        }
        if self.transcript.jobs.running.is_none() {
            let lookups: Vec<_> = self.transcript.model.as_ref().map_or_else(Vec::new, |m| {
                m.missing
                    .iter()
                    .filter(|(k, _)| !self.transcript.jobs.looked_up.contains(k))
                    .cloned()
                    .collect()
            });
            for (key, spec) in lookups {
                self.transcript.jobs.looked_up.insert(key.clone());
                self.transcript.jobs.queue.push_back((key, spec, false));
            }
            if let Some((key, spec, transcribe)) = self.transcript.jobs.queue.pop_front() {
                let job = start_source(
                    spec.path.clone(),
                    spec.stream,
                    spec.duration,
                    self.subtitles.language,
                    transcribe,
                    ctx.clone(),
                );
                self.transcript.jobs.running = Some((key, job, transcribe));
            } else {
                self.transcript.jobs.batch = (0, 0);
            }
        }
        if self.transcript.jobs.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(crate) fn transcript_job_status(&self) -> JobStatus {
        let jobs = &self.transcript.jobs;
        match &jobs.running {
            None => JobStatus::Idle,
            Some((_, _, false)) => JobStatus::LookingUp,
            Some((_, job, true)) => {
                let stage = job.progress.stage.load(Ordering::Relaxed);
                JobStatus::Transcribing {
                    stage,
                    fraction: if stage == 2 {
                        job.progress.whisper.fraction()
                    } else {
                        job.progress.done.load(Ordering::Relaxed) as f32 / 1000.0
                    },
                    done: jobs.batch.1,
                    total: jobs.batch.0.max(1),
                }
            }
        }
    }

    /// Missing sources whose lookup found no transcript (or was cancelled),
    /// and the first other failure, if any.
    pub(crate) fn transcript_gaps(&self) -> (usize, Option<String>) {
        let Some(model) = &self.transcript.model else {
            return (0, None);
        };
        let mut to_transcribe = 0;
        let mut failure = None;
        for (key, _) in &model.missing {
            match self.transcript.jobs.errors.get(key).map(String::as_str) {
                Some("no_transcript" | "cancelled") => to_transcribe += 1,
                Some(other) => failure = failure.or_else(|| Some(other.to_owned())),
                None => {}
            }
        }
        (to_transcribe, failure)
    }

    pub(crate) fn retry_transcript_after_model_install(&mut self) {
        for error in self.transcript.jobs.errors.values_mut() {
            if error == "model_missing" {
                "no_transcript".clone_into(error);
            }
        }
        self.transcribe_missing();
    }

    pub(crate) fn transcribe_missing(&mut self) {
        let Some(model) = &self.transcript.model else {
            return;
        };
        let jobs = &mut self.transcript.jobs;
        let pending: Vec<_> = model
            .missing
            .iter()
            .filter(|(k, _)| {
                matches!(
                    jobs.errors.get(k).map(String::as_str),
                    Some("no_transcript" | "cancelled")
                )
            })
            .cloned()
            .collect();
        jobs.batch = (pending.len(), 0);
        for (key, spec) in pending {
            jobs.errors.remove(&key);
            jobs.queue.push_back((key, spec, true));
        }
    }

    pub(crate) fn cancel_transcript_jobs(&mut self) {
        self.transcript.jobs.queue.clear();
        if let Some((_, job, _)) = &self.transcript.jobs.running {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn seek_transcript(&mut self, seconds: f64) {
        let Some(timeline) = self.timeline_id else {
            return;
        };
        let fps = self.session.project.timelines[timeline].fps.as_f64();
        self.stop_browsing();
        self.timeline_state.playhead = ((seconds * fps).round() as FrameIdx).max(0);
        self.timeline_state.reveal_playhead = true;
        self.ensure_active_clip_matches_playhead(true);
        if !self.is_timeline_playing() {
            self.play_scrub_audio();
        }
    }

    /// Cuts the selected words and pauses from their clips: one Automatic
    /// cut per clip, the last one first so earlier clips keep their place,
    /// all in one undo step.
    pub(crate) fn cut_transcript_selection(&mut self) {
        self.transcript.error = None;
        let Some(model) = &self.transcript.model else {
            return;
        };
        if model.stamp.0 != self.session.history.generation() {
            self.transcript.error = Some("timeline_changed".into());
            return;
        }
        let timeline = model.timeline;
        let tl = &self.session.project.timelines[timeline];
        let fps = tl.fps.as_f64();
        let margin = self.settings.shorts.transcript.margin;
        let mut per_clip: BTreeMap<usize, (Vec<usize>, Vec<Range<f64>>)> = BTreeMap::new();
        for &i in &self.transcript.selection {
            match model.items.get(i) {
                Some(Item::Word(w)) => {
                    let w = &model.words[*w];
                    per_clip.entry(w.clip).or_default().0.push(w.local_index);
                }
                Some(Item::Pause(p)) => {
                    let p = &model.pauses[*p];
                    per_clip.entry(p.clip).or_default().1.push(p.local.clone());
                }
                None => {}
            }
        }
        let mut plans = Vec::new();
        for (index, (words, pauses)) in per_clip {
            let c = &model.clips[index];
            let Some(clip) = tl.clip(c.track, c.id) else {
                continue;
            };
            let mut ranges = transcript_edit::word_cuts(
                &c.words,
                &transcript_edit::runs(words),
                c.duration,
                margin,
                &c.energy,
            );
            ranges.extend(pauses);
            let frames = crate::auto_cut::frames_of_ranges(ranges, clip, fps);
            if !frames.is_empty() {
                plans.push(((c.track, c.id), frames));
            }
        }
        if plans.is_empty() {
            self.transcript.error = Some("nothing_to_cut".into());
            return;
        }
        plans.sort_by_key(|(_, frames)| std::cmp::Reverse(frames[0].0));
        let first_cut = plans.iter().map(|(_, f)| f[0].0).min().unwrap_or(0);
        if self.is_timeline_playing() {
            self.toggle_playback();
        }
        let ripple_all = self.settings.shorts.transcript.ripple_all;
        let crossfade = self.settings.shorts.transcript.crossfade;
        let group = self.session.history.begin_group();
        let mut applied = 0;
        let mut failure = None;
        for (target, frames) in plans {
            match vv_core::auto_cut_edit::AutoCut::plan(
                &mut self.session.project,
                timeline,
                target,
                &frames,
                ripple_all,
                crossfade,
            ) {
                Ok(command) => {
                    self.session
                        .history
                        .do_command(&mut self.session.project, Box::new(command));
                    applied += 1;
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        self.session
            .history
            .end_group_as(group, CommandLabel::TextCut);
        if let Some(e) = failure {
            // All or nothing: half a text edit would be harder to read than none.
            if applied > 0 {
                self.session.history.undo(&mut self.session.project);
            }
            self.transcript.error = Some(e.into());
            return;
        }
        self.transcript.selection.clear();
        self.transcript.anchor = None;
        self.timeline_state.playhead = first_cut;
        self.timeline_state.reveal_playhead = true;
        self.ensure_active_clip_matches_playhead(true);
    }

    pub(crate) fn correct_transcript_word(&mut self, word: usize, text: &str) {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(model) = &self.transcript.model else {
            return;
        };
        let Some(w) = model.words.get(word) else {
            return;
        };
        if text.is_empty() || text == w.text {
            return;
        }
        let clip = &model.clips[w.clip];
        let edit = TranscriptEdit {
            path: clip.path.clone(),
            stream: clip.stream,
            start_ms: w.source_ms,
            original: w.original.clone(),
            text,
        };
        let span = (
            transcript_edit::source_ms(w.start),
            transcript_edit::source_ms(w.end),
        );
        let command = CorrectWord::new(model.timeline, edit, w.text.clone(), span);
        self.session
            .history
            .do_command(&mut self.session.project, Box::new(command));
    }

    /// Selects the clip under `word` (or the first one) and opens Automatic cut.
    pub(crate) fn auto_cut_from_transcript(&mut self, ctx: &egui::Context, word: Option<usize>) {
        let Some(model) = &self.transcript.model else {
            return;
        };
        let clip = word
            .and_then(|w| model.words.get(w))
            .map(|w| w.clip)
            .or((!model.clips.is_empty()).then_some(0));
        let Some(c) = clip.map(|i| &model.clips[i]) else {
            return;
        };
        let key = (c.track, c.id);
        self.timeline_state
            .set_selection(BTreeSet::from([key]), Some(key));
        self.open_auto_cut(ctx);
    }
}

#[cfg(test)]
#[path = "tests/transcript.rs"]
mod tests;
