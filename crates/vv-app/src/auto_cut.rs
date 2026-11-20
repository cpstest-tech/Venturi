use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use vv_core::auto_cut::{self, Config, Energy, Segment, Word};
use vv_core::{Clip, ClipId, ClipSource, FrameIdx, TimelineId, TrackKind};

use crate::{VenturiApp, subtitles};

const CACHE_VERSION: u32 = 1;
const BLOCK_SECONDS: usize = 60;
const OVERLAP_SECONDS: usize = 2;
const RATE: usize = vv_transcribe::SAMPLE_RATE as usize;
const ENERGY_SAMPLES: usize = RATE / 100;

pub(crate) type SourceKey = (PathBuf, u64, usize, Option<&'static str>);

#[derive(Serialize, Deserialize)]
pub(crate) struct Cached {
    version: u32,
    pub duration: f64,
    pub words: Vec<Word>,
    pub energy: Energy,
}

#[derive(Default)]
pub(crate) struct State {
    pub modal: Option<Modal>,
    cache: HashMap<SourceKey, Arc<Cached>>,
}

#[derive(Default)]
pub(crate) struct Progress {
    pub stage: AtomicU8,
    pub done: AtomicU32,
    pub whisper: Arc<vv_transcribe::Progress>,
}

pub(crate) struct SourceJob {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Progress>,
    rx: mpsc::Receiver<Result<Arc<Cached>, String>>,
}

impl SourceJob {
    /// `None` while the worker runs.
    pub fn poll(&self) -> Option<Result<Arc<Cached>, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("worker_stopped".into())),
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

pub(crate) struct Analysis {
    pub segments: Vec<Segment>,
    pub energy: Energy,
}

struct AnalysisJob {
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Analysis>,
}

pub(crate) struct Modal {
    pub timeline: TimelineId,
    pub target: (usize, ClipId),
    pub clip: Clip,
    pub fps: f64,
    pub media_start: f64,
    pub media_speed: f64,
    pub generation: u64,
    key: Option<SourceKey>,
    pub source: Option<Arc<Cached>>,
    pub job: Option<SourceJob>,
    analysis_job: Option<AnalysisJob>,
    requested: Option<Instant>,
    pub analysis: Option<Analysis>,
    pub config: Config,
    pub ripple_all: bool,
    pub error: Option<String>,
    pub original_playhead: FrameIdx,
    pub original_selection: std::collections::BTreeSet<(usize, ClipId)>,
    pub active_segment: Option<usize>,
    pub preview: bool,
}

impl Drop for Modal {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(job) = &self.analysis_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl Modal {
    pub fn source_unchanged(&self) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(path, fingerprint, _, _)| source_stamp(path).ok() == Some(*fingerprint))
    }
    pub fn duration(&self) -> f64 {
        self.clip.timeline_len as f64 / self.fps
    }
    pub fn analyzing(&self) -> bool {
        self.requested.is_some() || self.analysis_job.is_some()
    }
    pub fn request_analysis(&mut self) {
        if let Some(job) = self.analysis_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.requested = Some(Instant::now());
    }
    pub fn removed_frames(&self) -> Vec<(FrameIdx, FrameIdx)> {
        self.analysis.as_ref().map_or_else(Vec::new, |a| {
            removed_frames(&a.segments, &self.clip, self.fps)
        })
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            match job.rx.try_recv() {
                Ok(Ok(source)) => {
                    self.source = Some(source);
                    self.job = None;
                    self.request_analysis();
                }
                Ok(Err(error)) => {
                    self.error = Some(error);
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("worker_stopped".into());
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self
            .requested
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(150))
        {
            self.requested = None;
            if let Some(source) = &self.source {
                let source = source.clone();
                let clip = self.clip.clone();
                let config = self.config.clone();
                let fps = self.fps;
                let media_start = self.media_start;
                let speed = self.media_speed;
                let (tx, rx) = mpsc::channel();
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = cancel.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let duration = clip.timeline_len as f64 / fps;
                    let (words, energy) = clip_local(&source, media_start, speed, duration);
                    let Some(segments) =
                        auto_cut::analyze_cancellable(&words, &energy, duration, &config, &|| {
                            worker_cancel.load(Ordering::Relaxed)
                        })
                    else {
                        return;
                    };
                    let energy = waveform_energy(&energy, duration);
                    if !worker_cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(Analysis { segments, energy });
                    }
                    ctx.request_repaint();
                });
                self.analysis_job = Some(AnalysisJob { cancel, rx });
            }
        }
        if let Some(job) = &self.analysis_job {
            match job.rx.try_recv() {
                Ok(mut analysis) => {
                    if let Some(old) = &self.analysis {
                        carry_selection(&old.segments, &mut analysis.segments);
                    }
                    self.analysis = Some(analysis);
                    self.analysis_job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("worker_stopped".into());
                    self.analysis_job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.job.is_some() || self.analyzing() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl VenturiApp {
    pub(crate) fn open_auto_cut(&mut self, ctx: &egui::Context) {
        if self.auto_cut.modal.is_some() || self.take.is_some() {
            return;
        }
        let Some(timeline) = self.timeline_id else {
            return;
        };
        let tl = &self.session.project.timelines[timeline];
        let Some(target) = self
            .timeline_state
            .selected
            .iter()
            .copied()
            .filter(|&(t, id)| {
                tl.clip(t, id)
                    .is_some_and(|c| matches!(c.source, ClipSource::Media(_)))
            })
            .min_by_key(|&(t, _)| usize::from(tl.tracks[t].kind != TrackKind::Video))
        else {
            return;
        };
        let clip = tl.clip(target.0, target.1).unwrap().clone();
        let fps = tl.fps.as_f64();
        let Some(spec) = self.speech_source(timeline, &clip) else {
            return;
        };
        let locked = tl.tracks.iter().any(|t| {
            t.locked
                && t.clips.iter().any(|c| {
                    c.id == clip.id || clip.linked_group.is_some_and(|g| c.linked_group == Some(g))
                })
        });
        let language = self.subtitles.language;
        let source = self.cached_speech(&spec);
        let error = if locked { Some("locked") } else { spec.error };
        let SpeechSource {
            path,
            stream,
            duration,
            media_start,
            media_speed,
            key,
            ..
        } = spec;
        let mut modal = Modal {
            timeline,
            target,
            clip,
            fps,
            media_start,
            media_speed,
            generation: self.session.history.generation(),
            key,
            source,
            job: None,
            analysis_job: None,
            requested: None,
            analysis: None,
            config: Config::default(),
            ripple_all: true,
            error: error.map(str::to_owned),
            original_playhead: self.timeline_state.playhead,
            original_selection: self.timeline_state.selected.clone(),
            active_segment: None,
            preview: true,
        };
        if error.is_none() && modal.source.is_none() {
            modal.job = Some(start_source(
                path,
                stream,
                duration,
                language,
                true,
                ctx.clone(),
            ));
        } else if error.is_none() && modal.source.is_some() {
            modal.request_analysis();
        }
        if self.is_timeline_playing() {
            self.toggle_playback();
        }
        self.stop_browsing();
        self.reverse_shuttle = None;
        self.reset_playback_speed_to_normal();
        self.timeline_state.playhead = modal.clip.timeline_start;
        self.ensure_active_clip_matches_playhead(true);
        self.auto_cut.modal = Some(modal);
    }

    pub(crate) fn cache_auto_cut_source(&mut self, modal: &Modal) {
        if let (Some(key), Some(source)) = (&modal.key, &modal.source) {
            self.remember_speech(key, source);
        }
    }

    pub(crate) fn remember_speech(&mut self, key: &SourceKey, source: &Arc<Cached>) {
        if self.auto_cut.cache.len() >= 4 && !self.auto_cut.cache.contains_key(key) {
            self.auto_cut.cache.clear();
        }
        self.auto_cut.cache.insert(key.clone(), source.clone());
    }

    pub(crate) fn cached_speech(&self, spec: &SpeechSource) -> Option<Arc<Cached>> {
        spec.key
            .as_ref()
            .and_then(|key| self.auto_cut.cache.get(key))
            .cloned()
    }

    /// Where the speech of `clip` comes from: its linked audio covering it if
    /// there is one, else its own audio.
    pub(crate) fn speech_source(&self, timeline: TimelineId, clip: &Clip) -> Option<SpeechSource> {
        let tl = &self.session.project.timelines[timeline];
        let fps = tl.fps.as_f64();
        let audio = tl
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .flat_map(|t| &t.clips)
            .find(|c| {
                (c.id == clip.id || clip.linked_group.is_some_and(|g| c.linked_group == Some(g)))
                    && c.timeline_start <= clip.timeline_start
                    && c.timeline_end() >= clip.timeline_end()
                    && matches!(c.source, ClipSource::Media(_))
            });
        let analysis_clip = audio.unwrap_or(clip);
        let ClipSource::Media(media) = analysis_clip.source else {
            return None;
        };
        let item = self.session.project.media_pool.get(media)?;
        let stream = analysis_clip.audio_stream_index;
        let path = item.path.clone();
        let unsupported = item.compound.is_some()
            || clip.freeze.is_some()
            || clip.disabled
            || !item.meta.has_audio;
        let fingerprint = source_stamp(&path).ok();
        let language = self.subtitles.language;
        Some(SpeechSource {
            key: fingerprint.map(|hash| (path.clone(), hash, stream, language)),
            error: if unsupported {
                Some("unsupported")
            } else if fingerprint.is_none() {
                Some("offline")
            } else {
                None
            },
            media_start: analysis_clip.media_secs_at(clip.timeline_start, fps),
            media_speed: analysis_clip.speed().as_f64(),
            duration: item.meta.duration_frames as f64 / item.meta.fps.as_f64(),
            stream,
            path,
        })
    }
}

pub(crate) struct SpeechSource {
    pub path: PathBuf,
    pub stream: usize,
    /// Of the whole media, in seconds.
    pub duration: f64,
    pub media_start: f64,
    pub media_speed: f64,
    pub key: Option<SourceKey>,
    pub error: Option<&'static str>,
}

/// Words and energy of the source re-expressed in seconds from the clip start.
pub(crate) fn clip_local(
    source: &Cached,
    media_start: f64,
    speed: f64,
    duration: f64,
) -> (Vec<Word>, Energy) {
    let words = source
        .words
        .iter()
        .filter_map(|w| {
            let start = ((w.start - media_start) / speed).max(0.0);
            let end = ((w.end - media_start) / speed).min(duration);
            (end > start).then(|| Word {
                text: w.text.clone(),
                start,
                end,
                confidence: w.confidence,
            })
        })
        .collect();
    let step = source.energy.step / speed;
    let count = (duration / step).ceil() as usize;
    let energy = Energy {
        step,
        db: (0..count)
            .map(|i| {
                let at =
                    ((media_start + i as f64 * step * speed) / source.energy.step).floor() as usize;
                source.energy.db.get(at).copied().unwrap_or(-120.0)
            })
            .collect(),
    };
    (words, energy)
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

fn removed_frames(segments: &[Segment], clip: &Clip, fps: f64) -> Vec<(FrameIdx, FrameIdx)> {
    let duration = clip.timeline_len as f64 / fps;
    let keep = auto_cut::keep_ranges(
        duration,
        segments
            .iter()
            .filter(|s| s.checked)
            .map(|s| s.start..s.end),
    );
    let removed = auto_cut::keep_ranges(duration, keep);
    vv_core::auto_cut_edit::merge_frame_ranges(
        &removed
            .into_iter()
            .filter_map(|r| {
                // Round after merging: adjacent checked rows must not leave a stray frame.
                let start = clip.timeline_start + (r.start * fps - 1e-7).ceil() as FrameIdx;
                let end = clip.timeline_start + (r.end * fps + 1e-7).floor() as FrameIdx;
                (end > start).then_some((start, end))
            })
            .collect::<Vec<_>>(),
    )
}

fn source_stamp(path: &Path) -> std::io::Result<u64> {
    let stamp = std::fs::metadata(path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_nanos() as u64)
        .unwrap_or_default();
    Ok(vv_media::content_fingerprint(path)? ^ stamp)
}

/// `transcribe: false` only reads the disk cache and fails with `no_transcript`.
pub(crate) fn start_source(
    path: PathBuf,
    stream: usize,
    duration: f64,
    language: Option<&'static str>,
    transcribe: bool,
    ctx: egui::Context,
) -> SourceJob {
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Progress::default());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn({
        let cancel = cancel.clone();
        let progress = progress.clone();
        move || {
            let result = load_source(
                &path, stream, duration, language, transcribe, &progress, &cancel,
            )
            .map(Arc::new);
            let _ = tx.send(result);
            ctx.request_repaint();
        }
    });
    SourceJob {
        cancel,
        progress,
        rx,
    }
}

fn hash_file(path: &Path, cancel: &AtomicBool, progress: &Progress) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let total = file.metadata().map_err(|e| e.to_string())?.len().max(1);
    let mut done = 0_u64;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        cancelled(cancel)?;
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
        done += n as u64;
        progress.done.store(
            (done as f64 / total as f64 * 1000.0) as u32,
            Ordering::Relaxed,
        );
    }
    Ok(hash
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn load_source(
    path: &Path,
    stream: usize,
    duration: f64,
    language: Option<&str>,
    transcribe: bool,
    progress: &Arc<Progress>,
    cancel: &Arc<AtomicBool>,
) -> Result<Cached, String> {
    let before = source_stamp(path).map_err(|e| e.to_string())?;
    let hash = hash_file(path, cancel, progress)?;
    if source_stamp(path).map_err(|e| e.to_string())? != before {
        return Err("source_changed".into());
    }
    let model_dir = subtitles::models_dir().ok_or("no_model_dir")?;
    let preferred = vv_transcribe::Model::preferred();
    let model = if vv_transcribe::model_present(&model_dir, preferred) {
        preferred
    } else {
        [vv_transcribe::Model::Turbo, vv_transcribe::Model::Small]
            .into_iter()
            .find(|m| vv_transcribe::model_present(&model_dir, *m))
            .ok_or("model_missing")?
    };
    let root = crate::library::Library::default_root()
        .ok_or("no_cache_dir")?
        .join("auto-cut");
    let cache_path = root.join(format!(
        "{hash}-{stream}-{}-{}-v{CACHE_VERSION}.json",
        model.file(),
        language.unwrap_or("auto")
    ));
    if let Ok(bytes) = std::fs::read(&cache_path)
        && let Ok(cache) = serde_json::from_slice::<Cached>(&bytes)
        && cache.version == CACHE_VERSION
        && cache.energy.step > 0.0
        && cache.duration.is_finite()
    {
        return Ok(cache);
    }
    if !transcribe {
        return Err("no_transcript".into());
    }
    progress.stage.store(1, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let mut cache = Cached {
        version: CACHE_VERSION,
        duration: 0.0,
        words: Vec::new(),
        energy: Energy {
            step: 0.01,
            db: Vec::new(),
        },
    };
    let mut buffer = Vec::new();
    let mut offset = 0_usize;
    let mut energy_sum = 0.0_f64;
    let mut energy_count = 0;
    let mut seen = 0_usize;
    let mut detected = language.map(str::to_owned);
    let mut error = None;
    let formats = vv_media::decode_audio_streams_streaming(
        path,
        &[stream],
        Some(RATE as u32),
        |_, channels, chunk| {
            if cancelled(cancel).is_err() {
                return std::ops::ControlFlow::Break(());
            }
            for frame in chunk.chunks_exact(usize::from(channels.max(1))) {
                let sample = frame.iter().sum::<f32>() / f32::from(channels.max(1));
                buffer.push(sample);
                energy_sum += f64::from(sample).powi(2);
                energy_count += 1;
                seen += 1;
                if energy_count == ENERGY_SAMPLES {
                    cache.energy.db.push(
                        (10.0 * (energy_sum / energy_count as f64).max(1e-12).log10()) as f32,
                    );
                    energy_sum = 0.0;
                    energy_count = 0;
                }
            }
            loop {
                let left = if offset == 0 { 0 } else { OVERLAP_SECONDS };
                let block = (left + BLOCK_SECONDS + OVERLAP_SECONDS) * RATE;
                if buffer.len() < block {
                    break;
                }
                progress.stage.store(2, Ordering::Relaxed);
                if let Err(e) = transcribe_block(
                    &buffer[..block],
                    offset,
                    left as f64..(left + BLOCK_SECONDS) as f64,
                    &mut cache.words,
                    &mut detected,
                    model,
                    &model_dir,
                    progress,
                    cancel,
                ) {
                    error = Some(e);
                    return std::ops::ControlFlow::Break(());
                }
                let advance = (left + BLOCK_SECONDS - OVERLAP_SECONDS) * RATE;
                buffer.drain(..advance);
                offset += advance;
                progress.done.store(
                    ((offset as f64 / RATE as f64 / duration.max(0.01)) * 1000.0).min(999.0) as u32,
                    Ordering::Relaxed,
                );
                progress.stage.store(1, Ordering::Relaxed);
            }
            std::ops::ControlFlow::Continue(())
        },
    )
    .map_err(|e| e.to_string())?;
    cancelled(cancel)?;
    if let Some(e) = error {
        return Err(e);
    }
    if formats.first().copied().flatten().is_none() {
        return Err("no_audio".into());
    }
    if !buffer.is_empty() {
        progress.stage.store(2, Ordering::Relaxed);
        let left = if offset == 0 {
            0.0
        } else {
            OVERLAP_SECONDS as f64
        };
        transcribe_block(
            &buffer,
            offset,
            left..f64::INFINITY,
            &mut cache.words,
            &mut detected,
            model,
            &model_dir,
            progress,
            cancel,
        )?;
    }
    if energy_count > 0 {
        cache
            .energy
            .db
            .push((10.0 * (energy_sum / energy_count as f64).max(1e-12).log10()) as f32);
    }
    cache.duration = seen as f64 / RATE as f64;
    if seen == 0 {
        return Err("no_audio".into());
    }
    cancelled(cancel)?;
    if source_stamp(path).map_err(|e| e.to_string())? != before {
        return Err("source_changed".into());
    }
    let json = serde_json::to_string(&cache).map_err(|e| e.to_string())?;
    // Several reopened dialogs may finish together after cancellation.
    static CACHE_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    if let Ok(_guard) = CACHE_WRITE.lock() {
        cancelled(cancel)?;
        if std::fs::create_dir_all(&root).is_ok() {
            let _ = vv_core::write_atomic(&cache_path, json.as_bytes());
        }
    }
    Ok(cache)
}

fn transcribe_block(
    samples: &[f32],
    offset: usize,
    take: std::ops::Range<f64>,
    words: &mut Vec<Word>,
    language: &mut Option<String>,
    model: vv_transcribe::Model,
    model_dir: &Path,
    progress: &Arc<Progress>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    cancelled(cancel)?;
    // Avoid hallucinated words on empty takes and avoid loading Whisper for silence.
    if samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / (samples.len().max(1) as f64)
        < 1e-10
    {
        return Ok(());
    }
    let (transcript, scores) = vv_transcribe::transcribe_scored(
        model,
        &vv_transcribe::model_path(model_dir, model),
        samples,
        language.as_deref(),
        &progress.whisper,
        cancel,
    )
    .map_err(|e| e.to_string())?;
    if language.is_none() {
        *language = Some(transcript.language.clone());
    }
    let offset = offset as f64 / RATE as f64;
    for (i, w) in transcript.words.into_iter().enumerate() {
        let local = f64::from(w.start_ms) / 1000.0;
        if !take.contains(&local) {
            continue;
        }
        let start = local + offset;
        let end = (f64::from(w.end_ms) / 1000.0).min(samples.len() as f64 / RATE as f64) + offset;
        if words
            .last()
            .is_some_and(|last| last.start >= start || (last.end > start && last.text == w.text))
        {
            continue;
        }
        if end > start {
            words.push(Word {
                text: w.text,
                start,
                end,
                confidence: scores.get(i).copied(),
            });
        }
    }
    Ok(())
}

fn carry_selection(old: &[Segment], new: &mut [Segment]) {
    let mut index = 0;
    for segment in new {
        while index < old.len() && old[index].end <= segment.start {
            index += 1;
        }
        if let Some(previous) = old[index..]
            .iter()
            .take_while(|s| s.start < segment.end)
            .filter(|s| s.kind == segment.kind && s.text == segment.text)
            .max_by(|a, b| {
                (a.end.min(segment.end) - a.start.max(segment.start))
                    .total_cmp(&(b.end.min(segment.end) - b.start.max(segment.start)))
            })
        {
            segment.checked = previous.checked;
        }
    }
}

fn waveform_energy(energy: &Energy, duration: f64) -> Energy {
    let bins = energy.db.len().min(4096);
    if bins == 0 {
        return Energy {
            step: energy.step,
            db: Vec::new(),
        };
    }
    let db = (0..bins)
        .map(|i| {
            let from = i * energy.db.len() / bins;
            let to = (i + 1) * energy.db.len() / bins;
            energy.db[from..to]
                .iter()
                .copied()
                .fold(-120.0_f32, f32::max)
        })
        .collect();
    Energy {
        step: duration / bins as f64,
        db,
    }
}

#[cfg(test)]
#[path = "tests/auto_cut.rs"]
mod tests;
