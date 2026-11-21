//! «Doppiaggio»: transcript (the existing engine) → phrases → translation
//! (`translate`) → review table → voice (`tts`) → fit → one audio track per
//! language, ducking and subtitles, all as one undo step.

pub(crate) mod translate;
pub(crate) mod tts;
pub(crate) mod ui;

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use serde::{Deserialize, Serialize};
use vv_core::dub::{self, DubTrack, Ducked, Language, SetTrackDub};
use vv_core::{
    AudioEffect, AudioEffectKind, Clip, ClipId, ClipSource, CommandLabel, CompositeCommand,
    FrameIdx, MixerChannel, NormalizeMode, Rational, SetLevel, TimelineId, TrackKind,
};

use crate::VenturiApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginalVoice {
    Duck,
    Mute,
}

/// Dubbing choices, remembered between sessions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub language: String,
    pub voice: String,
    pub translator: translate::Provider,
    /// Names and terms never translated, comma separated.
    pub glossary: String,
    pub original: OriginalVoice,
    pub voice_duck_db: f32,
    pub music_duck_db: f32,
    /// How much faster than recorded a line may play, 0.2 = +20 %.
    pub max_stretch: f32,
    pub subtitles: bool,
    pub normalize: bool,
    /// Consent, given in the settings, to send the phrases' text to Gemini.
    pub allow_cloud_translation: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            language: "en".into(),
            voice: "en_US-joe-medium".into(),
            translator: translate::Provider::Manual,
            glossary: "Apple, iPad, iPhone, Mac, Sidecar".into(),
            original: OriginalVoice::Duck,
            voice_duck_db: -14.0,
            music_duck_db: -10.0,
            max_stretch: 0.2,
            subtitles: true,
            normalize: true,
            allow_cloud_translation: false,
        }
    }
}

pub(crate) const MAX_STRETCH: std::ops::RangeInclusive<f32> = 0.0..=0.3;
/// Loudness the dub track is brought to.
const DUB_LUFS: f32 = -16.0;
const MUTED_DB: f32 = -1000.0;

#[derive(Clone, Debug)]
pub(crate) struct Audio {
    pub path: PathBuf,
    pub secs: f64,
    /// `audio_key` of the voice and text it was made from.
    pub key: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// Timeline seconds of the original phrase.
    pub start: f64,
    pub end: f64,
    pub window: Range<f64>,
    pub source: String,
    pub text: String,
    /// The text came from a translation or the user, not a copy waiting for one.
    pub translated: bool,
    pub audio: Option<Audio>,
    pub error: Option<(JobKind, String)>,
    pub busy: bool,
}

impl Row {
    pub fn fit(&self, max_stretch: f32) -> Option<dub::Fit> {
        let audio = self.audio.as_ref()?;
        Some(dub::fit(
            audio.secs,
            self.window.end - self.window.start,
            f64::from(max_stretch),
        ))
    }

    pub fn audio_fresh(&self, voice: &str) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|a| a.key == audio_key(voice, &self.text))
    }
}

pub(crate) enum Message {
    Info(String),
    Error(String),
}

enum Msg {
    Text(usize, String),
    Prepared(Result<Box<Prepared>, String>),
    Progress,
    TextFailed(Vec<usize>, String),
    Audio(usize, Result<Audio, String>),
    Done(Option<translate::Error>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    Translate,
    Shorten,
    Speak,
    Prepare,
}

struct Job {
    kind: JobKind,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Msg>,
    total: usize,
    done: usize,
    translation: Option<TranslationSettings>,
}

struct Install {
    progress: Arc<vv_matting::Progress>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<(), String>>,
    rows: Option<Vec<usize>>,
}

struct WhisperInstall {
    progress: Arc<vv_transcribe::Progress>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<PathBuf, vv_transcribe::Error>>,
}

#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub rows: Vec<Row>,
    rows_for: Option<(TimelineId, String)>,
    stamp: Option<(u64, u64, u64)>,
    install_rows: Option<Vec<usize>>,
    job: Option<Job>,
    install: Option<Install>,
    whisper: Option<WhisperInstall>,
    confirm_whisper: bool,
    /// The download is waiting for the user's OK.
    pub confirm_install: bool,
    pub message: Option<Message>,
    translations: Option<HashMap<String, String>>,
    /// History generation of a dub just made, until confirmed or undone.
    preview: Option<u64>,
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct TranslationSettings {
    provider: translate::Provider,
    model: String,
    glossary: Vec<String>,
    language: String,
}

impl TranslationSettings {
    fn key(&self, text: &str) -> String {
        translate::cache_key(
            self.provider,
            &self.model,
            &self.language,
            &self.glossary,
            text,
        )
    }
}

struct Prepared {
    timeline: TimelineId,
    generation: u64,
    prefs: Prefs,
    rows: Vec<(Row, dub::Fit)>,
    wav: PathBuf,
    offsets: Vec<(FrameIdx, FrameIdx)>,
    meta: vv_core::MediaMeta,
}

fn cache_dir() -> Option<PathBuf> {
    Some(crate::library::Library::default_root()?.join("dub"))
}

fn sha(material: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, material.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn audio_key(voice: &str, text: &str) -> String {
    let version = tts::find_voice(voice).map_or("", tts::VoiceInfo::cache_id);
    sha(&format!(
        "piper-1\u{1f}{voice}\u{1f}{version}\u{1f}{}",
        text.trim()
    ))
}

fn audio_path(key: &str) -> Option<PathBuf> {
    Some(cache_dir()?.join("audio").join(format!("{key}.wav")))
}

fn audio_secs(path: &Path) -> Option<f64> {
    let meta = vv_media::probe_media(path).ok()?;
    Some(meta.duration_frames as f64 / meta.fps.as_f64())
}

fn speak(
    voice: &dyn tts::Voice,
    voice_id: &str,
    text: &str,
    cancel: &AtomicBool,
) -> Result<Audio, String> {
    let key = audio_key(voice_id, text);
    let path = audio_path(&key).ok_or("no cache folder")?;
    if let Some(secs) = audio_secs(&path) {
        return Ok(Audio { path, secs, key });
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let secs = write_wav_atomically(&path, |part| {
        voice.synthesize(text, part, cancel)?;
        audio_secs(part).ok_or_else(|| "unreadable voice file".to_owned())
    })?;
    Ok(Audio { path, secs, key })
}

impl State {
    pub fn busy(&self) -> Option<(JobKind, usize, usize)> {
        self.job.as_ref().map(|j| (j.kind, j.done, j.total))
    }

    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(install) = &self.install {
            install.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(whisper) = &self.whisper {
            whisper.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn installing(&self) -> Option<f32> {
        self.install.as_ref().map(|i| i.progress.fraction())
    }

    fn translations(&mut self) -> &mut HashMap<String, String> {
        self.translations.get_or_insert_with(|| {
            cache_dir()
                .and_then(|d| std::fs::read(d.join("translations.json")).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        })
    }

    fn save_translations(&self) {
        let (Some(map), Some(dir)) = (&self.translations, cache_dir()) else {
            return;
        };
        if std::fs::create_dir_all(&dir).is_ok()
            && let Ok(json) = serde_json::to_vec(map)
        {
            let _ = vv_core::write_atomic(&dir.join("translations.json"), &json);
        }
    }
}

impl VenturiApp {
    pub(crate) fn open_dub(&mut self) {
        self.dub.open = true;
        self.dub.message = None;
    }

    /// The phrases of the current transcript, keeping what was already
    /// translated or voiced for the same phrase.
    pub(crate) fn refresh_dub_rows(&mut self) {
        let Some(timeline) = self.timeline_id else {
            self.dub.rows.clear();
            return;
        };
        self.refresh_transcript();
        let Some(model) = self
            .transcript
            .model
            .as_ref()
            .filter(|m| m.timeline == timeline)
        else {
            return;
        };
        let language = self.settings.shorts.dub.language.clone();
        let same = self.dub.rows_for.as_ref() == Some(&(timeline, language.clone()));
        if same && self.dub.stamp == Some(model.stamp) {
            return;
        }
        self.dub.stamp = Some(model.stamp);
        let mut words: Vec<dub::TimedWord> = model
            .words
            .iter()
            .map(|w| dub::TimedWord {
                text: w.text.clone(),
                start: w.start,
                end: w.end,
            })
            .collect();
        let tl = &self.session.project.timelines[timeline];
        let end = tl.total_frames() as f64 / tl.fps.as_f64();
        words.sort_by(|a, b| a.start.total_cmp(&b.start));
        let phrases = dub::phrases(&words);
        let spans: Vec<(f64, f64)> = phrases.iter().map(|p| (p.start, p.end)).collect();
        let windows = dub::windows(&spans, end);
        let old: HashMap<_, _> = std::mem::take(&mut self.dub.rows)
            .into_iter()
            .map(|row| {
                (
                    ((row.start * 1000.0).round() as i64, row.source.clone()),
                    row,
                )
            })
            .collect();
        self.dub.rows = phrases
            .into_iter()
            .zip(windows)
            .map(|(p, window)| {
                let kept = same
                    .then(|| old.get(&((p.start * 1000.0).round() as i64, p.text.clone())))
                    .flatten();
                Row {
                    start: p.start,
                    end: p.end,
                    window,
                    text: kept.map_or_else(|| p.text.clone(), |o| o.text.clone()),
                    translated: kept.is_some_and(|o| o.translated),
                    audio: kept.and_then(|o| o.audio.clone()),
                    error: kept.and_then(|r| r.error.clone()),
                    busy: false,
                    source: p.text,
                }
            })
            .collect();
        self.dub.rows_for = Some((timeline, language));
        self.fill_cached_translations();
    }

    fn dub_gemini_model(&self) -> String {
        let provider = crate::ai_sequence::Provider::Gemini;
        if self.settings.ai_provider == provider {
            self.settings.ai_model.clone()
        } else {
            self.settings
                .ai_provider_models
                .get(provider.id())
                .cloned()
                .unwrap_or_else(|| crate::settings::Settings::default().ai_model)
        }
    }

    fn translator_settings(&self) -> TranslationSettings {
        let prefs = &self.settings.shorts.dub;
        TranslationSettings {
            provider: prefs.translator,
            model: self.dub_gemini_model(),
            glossary: translate::glossary(&prefs.glossary),
            language: prefs.language.clone(),
        }
    }

    fn fill_cached_translations(&mut self) {
        let settings = self.translator_settings();
        if settings.provider == translate::Provider::Manual {
            return;
        }
        let keys: Vec<(usize, String)> = self
            .dub
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.translated)
            .map(|(i, r)| (i, settings.key(&r.source)))
            .collect();
        let found: Vec<(usize, String)> = {
            let map = self.dub.translations();
            keys.into_iter()
                .filter_map(|(i, k)| map.get(&k).map(|t| (i, t.clone())))
                .collect()
        };
        for (i, text) in found {
            self.dub.rows[i].text = text;
            self.dub.rows[i].translated = true;
        }
    }

    /// Why the chosen translator cannot run, if it cannot.
    pub(crate) fn translator_blocked(&self) -> Option<std::borrow::Cow<'static, str>> {
        let prefs = &self.settings.shorts.dub;
        match prefs.translator {
            translate::Provider::Manual => None,
            translate::Provider::Gemini if !prefs.allow_cloud_translation => {
                Some(t!("dub.consent_missing"))
            }
            translate::Provider::Gemini => {
                let key = match crate::gemini::configured_key() {
                    Ok(key) => key,
                    Err(_) => return Some(t!("dub.key_missing")),
                };
                self.settings
                    .sequence_api_access()
                    .check(&key, &self.dub_gemini_model())
                    .err()
                    .map(std::borrow::Cow::Owned)
            }
        }
    }

    fn translator(&self) -> Result<Box<dyn translate::Translator>, String> {
        if let Some(why) = self.translator_blocked() {
            return Err(why.into_owned());
        }
        let prefs = &self.settings.shorts.dub;
        Ok(match prefs.translator {
            translate::Provider::Manual => Box::new(translate::Manual),
            translate::Provider::Gemini => Box::new(translate::Gemini {
                key: crate::gemini::configured_key()?,
                model: self.dub_gemini_model(),
            }),
        })
    }

    /// Translates `rows` (all those not translated yet when `None`); with
    /// `shorter`, rewrites them shorter instead.
    pub(crate) fn start_dub_translation(&mut self, rows: Option<Vec<usize>>, shorter: bool) {
        if self.dub.job.is_some() {
            return;
        }
        let translator = match self.translator() {
            Ok(t) => t,
            Err(e) => {
                self.dub.message = Some(Message::Error(e));
                return;
            }
        };
        let max_stretch = f64::from(self.settings.shorts.dub.max_stretch);
        let wanted: Vec<usize> = rows.unwrap_or_else(|| {
            (0..self.dub.rows.len())
                .filter(|&i| !self.dub.rows[i].translated)
                .collect()
        });
        let items: Vec<translate::Item> = wanted
            .iter()
            .filter_map(|&i| {
                let r = self.dub.rows.get(i)?;
                let window = (r.window.end - r.window.start) * (1.0 + max_stretch);
                let (text, audio) = if shorter {
                    (r.text.clone(), r.audio.as_ref().map(|a| a.secs))
                } else {
                    (r.source.clone(), Some(r.end - r.start))
                };
                Some(translate::Item {
                    id: i,
                    max_chars: dub::target_chars(&text, audio, window),
                    text,
                })
            })
            .collect();
        if items.is_empty() {
            self.dub.message = Some(Message::Info(t!("dub.nothing_to_translate").into_owned()));
            return;
        }
        for item in &items {
            self.dub.rows[item.id].busy = true;
            self.dub.rows[item.id].error = None;
        }
        let translation = self.translator_settings();
        let language = translation.language.clone();
        let glossary = translation.glossary.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let total = items.len();
        let job_cancel = cancel.clone();
        std::thread::spawn(move || {
            for batch in items.chunks(translate::BATCH) {
                match translator.translate(batch, &language, &glossary, shorter, &job_cancel) {
                    Ok(done) => {
                        let empty: Vec<usize> = batch
                            .iter()
                            .map(|i| i.id)
                            .filter(|id| !done.iter().any(|(d, _)| d == id))
                            .collect();
                        for (id, text) in done {
                            if let Some(item) = batch.iter().find(|item| item.id == id)
                                && let Some(term) =
                                    translate::missing_term(&item.text, &text, &glossary)
                            {
                                let error = t!("dub.missing_term", term = term).into_owned();
                                let _ = tx.send(Msg::TextFailed(vec![id], error));
                            } else {
                                let _ = tx.send(Msg::Text(id, text));
                            }
                        }
                        if !empty.is_empty() {
                            let _ = tx.send(Msg::TextFailed(empty, "empty".into()));
                        }
                    }
                    Err(translate::Error::Empty(ids)) => {
                        let _ = tx.send(Msg::TextFailed(ids, "empty".into()));
                    }
                    Err(e) => {
                        let rest: Vec<usize> = items
                            .iter()
                            .map(|i| i.id)
                            .skip_while(|id| *id != batch[0].id)
                            .collect();
                        let _ = tx.send(Msg::TextFailed(rest, e.to_string()));
                        let _ = tx.send(Msg::Done(Some(e)));
                        return;
                    }
                }
            }
            let _ = tx.send(Msg::Done(None));
        });
        self.dub.message = None;
        self.dub.job = Some(Job {
            kind: if shorter {
                JobKind::Shorten
            } else {
                JobKind::Translate
            },
            cancel,
            rx,
            total,
            done: 0,
            translation: Some(translation),
        });
    }

    /// The download the voice needs, if any: (engine MB, voice MB).
    pub(crate) fn dub_voice_missing(&self) -> Option<(Option<u32>, Option<u32>)> {
        let voice = tts::find_voice(&self.settings.shorts.dub.voice)?;
        let models = crate::subtitles::models_dir()?;
        match tts::missing(&models, voice) {
            (None, None) => None,
            missing => Some(missing),
        }
    }

    pub(crate) fn start_dub_install(&mut self) {
        let (Some(voice), Some(models)) = (
            tts::find_voice(&self.settings.shorts.dub.voice),
            crate::subtitles::models_dir(),
        ) else {
            return;
        };
        self.dub.confirm_install = false;
        self.dub.message = None;
        let progress = Arc::new(vv_matting::Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (job_progress, job_cancel) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            let _ = tx.send(tts::install(&models, voice, &job_progress, &job_cancel));
        });
        self.dub.install = Some(Install {
            progress,
            cancel,
            rx,
            rows: self.dub.install_rows.take(),
        });
    }

    /// Voices the rows whose audio is missing or stale (or just `rows`).
    pub(crate) fn start_dub_speech(&mut self, rows: Option<Vec<usize>>) {
        if self.dub.job.is_some() {
            return;
        }
        if self.dub_voice_missing().is_some() {
            self.dub.confirm_install = true;
            self.dub.install_rows = rows;
            return;
        }
        let voice_id = self.settings.shorts.dub.voice.clone();
        let voice = match crate::subtitles::models_dir()
            .ok_or_else(|| "no models folder".to_owned())
            .and_then(|m| tts::voice_for(&m, &voice_id))
        {
            Ok(v) => v,
            Err(e) => {
                self.dub.message = Some(Message::Error(e));
                return;
            }
        };
        let wanted: Vec<(usize, String)> = match rows {
            Some(rows) => rows
                .into_iter()
                .filter_map(|i| Some((i, self.dub.rows.get(i)?.text.clone())))
                .collect::<Vec<_>>(),
            None => self
                .dub
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.audio_fresh(&voice_id))
                .map(|(i, r)| (i, r.text.clone()))
                .collect::<Vec<_>>(),
        }
        .into_iter()
        .filter(|(_, t)| !t.trim().is_empty())
        .collect();
        if wanted.is_empty() {
            self.dub.message = Some(Message::Info(t!("dub.nothing_to_voice").into_owned()));
            return;
        }
        for (i, _) in &wanted {
            self.dub.rows[*i].busy = true;
            self.dub.rows[*i].error = None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let total = wanted.len();
        let job_cancel = cancel.clone();
        std::thread::spawn(move || {
            for (i, text) in wanted {
                if job_cancel.load(Ordering::Relaxed) {
                    break;
                }
                let _ = tx.send(Msg::Audio(i, speak(&*voice, &voice_id, &text, &job_cancel)));
            }
            let _ = tx.send(Msg::Done(None));
        });
        self.dub.message = None;
        self.dub.job = Some(Job {
            kind: JobKind::Speak,
            cancel,
            rx,
            total,
            done: 0,
            translation: None,
        });
    }

    fn dub_whisper_missing(&self) -> Option<(PathBuf, vv_transcribe::Model)> {
        let model = self.transcript.model.as_ref()?;
        if model.missing.is_empty() {
            return None;
        }
        let dir = crate::subtitles::models_dir()?;
        let missing = [vv_transcribe::Model::Turbo, vv_transcribe::Model::Small]
            .into_iter()
            .all(|model| !vv_transcribe::model_present(&dir, model));
        missing.then(|| (dir, vv_transcribe::Model::preferred()))
    }

    fn start_dub_whisper_install(&mut self) {
        self.dub.confirm_whisper = false;
        let Some((dir, model)) = self.dub_whisper_missing() else {
            return;
        };
        let progress = Arc::new(vv_transcribe::Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (job_progress, job_cancel) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            let _ = tx.send(vv_transcribe::ensure_model(
                &dir,
                model,
                &job_progress,
                &job_cancel,
            ));
        });
        self.dub.message = None;
        self.dub.whisper = Some(WhisperInstall {
            progress,
            cancel,
            rx,
        });
    }

    pub(crate) fn poll_dub(&mut self, ctx: &egui::Context) {
        if let Some(whisper) = &self.dub.whisper {
            match whisper.rx.try_recv() {
                Ok(result) => {
                    let whisper = self.dub.whisper.take().unwrap();
                    if !whisper.cancel.load(Ordering::Relaxed) {
                        match result {
                            Ok(_) => self.retry_transcript_after_model_install(),
                            Err(e) => {
                                self.dub.message = Some(Message::Error(
                                    t!("dub.whisper_failed", error = e.to_string()).into_owned(),
                                ))
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => self.dub.whisper = None,
            }
        }
        if let Some(install) = &self.dub.install {
            match install.rx.try_recv() {
                Ok(result) => {
                    let install = self.dub.install.take().unwrap();
                    if install.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    match result {
                        Ok(()) => self.start_dub_speech(install.rows),
                        Err(e) => {
                            self.dub.message = Some(Message::Error(
                                t!("dub.install_failed", error = e).into_owned(),
                            ))
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(150))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.dub.install = None,
            }
        }
        let Some(job) = &mut self.dub.job else {
            return;
        };
        let mut finished = None;
        let mut texts = Vec::new();
        let mut failed = Vec::new();
        let mut audio = Vec::new();
        let mut prepared = None;
        loop {
            match job.rx.try_recv() {
                Ok(Msg::Prepared(result)) => prepared = Some(result),
                Ok(Msg::Progress) => job.done += 1,
                Ok(Msg::Text(i, t)) => {
                    job.done += 1;
                    texts.push((i, t));
                }
                Ok(Msg::TextFailed(ids, e)) => {
                    job.done += ids.len();
                    failed.push((ids, e));
                }
                Ok(Msg::Audio(i, r)) => {
                    job.done += 1;
                    audio.push((i, r));
                }
                Ok(Msg::Done(e)) => {
                    finished = Some(e);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = Some(None);
                    break;
                }
            }
        }
        let kind = job.kind;
        let cancelled = job.cancel.load(Ordering::Relaxed);
        let translation = job.translation.as_ref();
        let mut cached = Vec::new();
        for (i, text) in texts {
            let Some(row) = self.dub.rows.get_mut(i) else {
                continue;
            };
            row.busy = false;
            if kind == JobKind::Translate
                && let Some(settings) =
                    translation.filter(|t| t.provider == translate::Provider::Gemini)
            {
                cached.push((settings.key(&row.source), text.clone()));
            }
            row.text = text;
            row.translated = true;
        }
        if !cached.is_empty() {
            self.dub.translations().extend(cached);
        }
        for (ids, e) in failed {
            for i in ids {
                if let Some(row) = self.dub.rows.get_mut(i) {
                    row.busy = false;
                    row.error = (!cancelled).then(|| {
                        (kind, {
                            if e == "empty" {
                                t!("dub.empty_translation").into_owned()
                            } else {
                                e.clone()
                            }
                        })
                    });
                }
            }
        }
        for (i, result) in audio {
            if let Some(row) = self.dub.rows.get_mut(i) {
                row.busy = false;
                match result {
                    Ok(a) => {
                        row.audio = Some(a);
                        row.error = None;
                    }
                    Err(e) if !cancelled => row.error = Some((JobKind::Speak, e)),
                    Err(_) => {}
                }
            }
        }
        match finished {
            None => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Some(error) => {
                self.dub.job = None;
                for row in &mut self.dub.rows {
                    row.busy = false;
                }
                if matches!(kind, JobKind::Translate) {
                    self.dub.save_translations();
                }
                if cancelled {
                    self.dub.message = Some(Message::Info(t!("dub.cancelled").into_owned()));
                    return;
                }
                if let Some(result) = prepared {
                    match result.and_then(|prepared| self.build_dub(*prepared)) {
                        Ok(()) => self.listen_dub_row(0),
                        Err(e) => self.dub.message = Some(Message::Error(e)),
                    }
                    return;
                }
                self.dub.message = error.map(|e| {
                    Message::Error(match e {
                        translate::Error::Cancelled => t!("dub.cancelled").into_owned(),
                        translate::Error::Quota(m) => t!("dub.quota", detail = m).into_owned(),
                        translate::Error::Unreachable(m) => {
                            t!("dub.unreachable", detail = m).into_owned()
                        }
                        other => {
                            t!("dub.translate_failed", detail = other.to_string()).into_owned()
                        }
                    })
                });
            }
        }
    }

    /// Plays the dub from `row`.
    pub(crate) fn listen_dub_row(&mut self, row: usize) {
        let Some(start) = self.dub.rows.get(row).map(|r| r.start) else {
            return;
        };
        self.seek_transcript(start);
        if !self.is_timeline_playing() {
            self.toggle_playback();
        }
    }

    pub(crate) fn dub_preview_pending(&mut self) -> bool {
        if self
            .dub
            .preview
            .is_some_and(|g| g != self.session.history.generation())
        {
            self.dub.preview = None;
        }
        self.dub.preview.is_some()
    }

    pub(crate) fn confirm_dub(&mut self) {
        self.dub.preview = None;
    }

    pub(crate) fn discard_dub(&mut self) {
        if self.dub_preview_pending() && self.dub.preview.take().is_some() {
            self.session.history.undo(&mut self.session.project);
        }
    }

    /// The clip carrying the sound of transcript clip `id`: its linked audio
    /// covering it, or itself. Found by id: track indices move when the
    /// earlier dub of the language is removed.
    fn voice_clip(&self, timeline: TimelineId, id: ClipId) -> Option<(usize, ClipId)> {
        let tl = &self.session.project.timelines[timeline];
        let (_, clip) = tl
            .tracks
            .iter()
            .enumerate()
            .find_map(|(i, t)| t.clip(id).map(|c| (i, c)))?;
        tl.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == TrackKind::Audio)
            .flat_map(|(i, t)| t.clips.iter().map(move |c| (i, c)))
            .find(|(_, c)| {
                c.id == clip.id
                    || (clip.linked_group.is_some_and(|g| c.linked_group == Some(g))
                        && c.timeline_start <= clip.timeline_start
                        && c.timeline_end() >= clip.timeline_end())
            })
            .map(|(i, c)| (i, c.id))
    }

    pub(crate) fn start_dub_preview(&mut self) {
        let Some(timeline) = self.timeline_id else {
            return;
        };
        let prefs = self.settings.shorts.dub.clone();
        let rows: Option<Vec<_>> = self
            .dub
            .rows
            .iter()
            .map(|r| {
                let fit = r.fit(prefs.max_stretch)?;
                (fit.fits && r.audio_fresh(&prefs.voice)).then(|| (r.clone(), fit))
            })
            .collect();
        let Some(rows) = rows.filter(|r| !r.is_empty()) else {
            self.dub.message = Some(Message::Error(t!("dub.nothing_voiced").into_owned()));
            return;
        };
        let generation = self.session.history.generation();
        let path = self.session.path().map(Path::to_path_buf);
        let total = rows.len() + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let job_cancel = cancel.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let (wav, offsets) =
                    concatenate(&rows, &prefs.language, path.as_deref(), &job_cancel, || {
                        let _ = tx.send(Msg::Progress);
                    })?;
                let meta = vv_media::probe_media(&wav).map_err(|e| e.to_string())?;
                Ok(Box::new(Prepared {
                    timeline,
                    generation,
                    prefs,
                    rows,
                    wav,
                    offsets,
                    meta,
                }))
            })();
            let _ = tx.send(Msg::Prepared(result));
            let _ = tx.send(Msg::Done(None));
        });
        self.dub.message = None;
        self.dub.job = Some(Job {
            kind: JobKind::Prepare,
            cancel,
            rx,
            total,
            done: 0,
            translation: None,
        });
    }

    fn build_dub(&mut self, prepared: Prepared) -> Result<(), String> {
        let Prepared {
            timeline,
            generation,
            prefs,
            rows,
            wav,
            offsets,
            meta,
        } = prepared;
        if self.timeline_id != Some(timeline) || self.session.history.generation() != generation {
            return Err(t!("dub.timeline_changed").into_owned());
        }
        let fps = self.session.project.timelines[timeline].fps;
        let group = self.session.history.begin_group();
        let media = self
            .session
            .media_with_path(&wav)
            .unwrap_or_else(|| self.session.add_media(wav, meta.clone()));

        if !dub::languages(&self.session.project.timelines[timeline]).is_empty() {
            self.apply_dub_language(timeline, &Language::Original);
        }
        let replaced: Vec<usize> = self.session.project.timelines[timeline]
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.dub.as_ref().is_some_and(|d| d.language == prefs.language))
            .map(|(i, _)| i)
            .collect();
        for &i in replaced.iter().rev() {
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::RemoveTrack::new(timeline, i)),
            );
        }

        let track = vv_core::edit::add_track(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            TrackKind::Audio,
        );
        if prefs.normalize {
            self.session.history.do_command(
                &mut self.session.project,
                Box::new(vv_core::AddAudioEffect::new(
                    timeline,
                    MixerChannel::Track(track),
                    AudioEffect::new(AudioEffectKind::Normalize {
                        target_db: DUB_LUFS,
                        mode: NormalizeMode::Loudness,
                        set_level: SetLevel::Relative,
                    }),
                )),
            );
        }
        let conform = Rational::conform_rate(fps, meta.fps);
        let f = fps.as_f64();
        let clips: Vec<(usize, Clip, Option<u64>)> = rows
            .iter()
            .zip(&offsets)
            .map(|((row, fit), &(from, to))| {
                let mut clip = Clip::from_source_range(
                    self.session.project.alloc_clip_id(),
                    ClipSource::Media(media),
                    from,
                    to,
                    (row.start * f).round() as FrameIdx,
                    conform,
                );
                if fit.speed > 1.0 + 1e-6 {
                    clip.set_speed(Rational::from_percent(fit.speed * 100.0), conform);
                }
                let window = ((row.window.end - row.start) * f).floor().max(1.0) as FrameIdx;
                clip.timeline_len = clip.timeline_len.min(window);
                (track, clip, None)
            })
            .collect();
        vv_core::edit::insert_clips(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            clips,
            CommandLabel::Dub,
        );

        let regions =
            vv_core::style_pack::speech_regions(rows.iter().map(|(r, _)| r.window.clone()));
        let mut voices: Vec<(usize, ClipId)> = self
            .transcript
            .model
            .as_ref()
            .filter(|m| m.timeline == timeline)
            .map(|m| m.clips.iter().map(|c| c.id).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| self.voice_clip(timeline, id))
            .collect();
        voices.sort_unstable_by_key(|(track, id)| (*track, id.0));
        voices.dedup();
        let music: Vec<(usize, ClipId)> = self
            .music_clips(timeline)
            .into_iter()
            .filter(|c| !voices.contains(c) && c.0 != track)
            .collect();
        let voice_db = match prefs.original {
            OriginalVoice::Duck => prefs.voice_duck_db,
            OriginalVoice::Mute => MUTED_DB,
        };
        let mut ducked = Vec::new();
        let mut skipped = 0;
        for (targets, amount) in [(voices, voice_db), (music, prefs.music_duck_db)] {
            for (t, id) in targets {
                let Some(clip) = self.session.project.timelines[timeline].clip(t, id) else {
                    continue;
                };
                let base = clip.effects.gain_db.clone();
                if self.session.project.timelines[timeline].tracks[t].locked || !base.is_constant()
                {
                    skipped += 1;
                    continue;
                }
                let after = if amount == MUTED_DB {
                    vv_core::Keyframed::constant(MUTED_DB)
                } else {
                    vv_core::style_pack::ducking_gain(clip, f, &regions, base.default, amount)
                };
                if after == base {
                    continue;
                }
                self.session.history.do_command(
                    &mut self.session.project,
                    Box::new(vv_core::SetClipValue::new(
                        timeline,
                        t,
                        id,
                        CommandLabel::Gain,
                        after.clone(),
                        |c| &mut c.effects.gain_db,
                    )),
                );
                ducked.push(Ducked {
                    clip: id,
                    before: base,
                    after,
                });
            }
        }
        self.session.history.do_command(
            &mut self.session.project,
            Box::new(SetTrackDub::new(
                timeline,
                track,
                Some(DubTrack {
                    language: prefs.language.clone(),
                    voice: prefs.voice.clone(),
                    ducked,
                }),
            )),
        );
        if prefs.subtitles {
            self.add_dub_subtitles(timeline, &rows, &prefs.language);
        }
        self.apply_dub_language(timeline, &Language::Dub(prefs.language.clone()));
        self.session.history.end_group_as(group, CommandLabel::Dub);
        self.dub.preview = Some(self.session.history.generation());
        if skipped > 0 {
            self.dub.message = Some(Message::Info(
                t!("dub.manual_gain", count = skipped).into_owned(),
            ));
        }
        Ok(())
    }

    fn add_dub_subtitles(
        &mut self,
        timeline: TimelineId,
        rows: &[(Row, dub::Fit)],
        language: &str,
    ) {
        let words: Vec<vv_core::CaptionWord> = rows
            .iter()
            .flat_map(|(r, fit)| dub::caption_words(&r.text, r.start, r.start + fit.length))
            .collect();
        let template = self
            .session
            .project
            .captions
            .get(&timeline)
            .map(|c| c.template.clone())
            .or_else(|| self.subtitles.selected.clone())
            .unwrap_or_else(|| vv_core::captions::DEFAULT_TEMPLATE.to_owned());
        let Some(mut clips) = self
            .subtitle_clips(timeline, &words, &template)
            .or_else(|| self.subtitle_clips(timeline, &words, vv_core::captions::DEFAULT_TEMPLATE))
        else {
            return;
        };
        let tl = &self.session.project.timelines[timeline];
        let look: Vec<Clip> = vv_core::captions::subtitle_track(tl)
            .map(|i| tl.tracks[i].clips.clone())
            .unwrap_or_default();
        crate::subtitles::carry_look(&look, &mut clips);
        let track = vv_core::edit::add_track(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            TrackKind::Video,
        );
        vv_core::edit::insert_clips(
            &mut self.session.project,
            &mut self.session.history,
            timeline,
            clips.into_iter().map(|c| (track, c, None::<u64>)).collect(),
            CommandLabel::Dub,
        );
        self.session.history.do_command(
            &mut self.session.project,
            Box::new(SetTrackDub::new(
                timeline,
                track,
                Some(DubTrack {
                    language: language.to_owned(),
                    voice: String::new(),
                    ducked: Vec::new(),
                }),
            )),
        );
    }

    /// Mutes and gains so that `choice` plays, as one step.
    pub(crate) fn apply_dub_language(&mut self, timeline: TimelineId, choice: &Language) {
        let change = dub::switch(&self.session.project.timelines[timeline], choice);
        if change == dub::Switch::default() {
            return;
        }
        let mut commands: Vec<Box<dyn vv_core::Command>> = Vec::new();
        for (track, muted) in change.mutes {
            commands.push(Box::new(vv_core::SetTrackFlag::new(
                timeline,
                track,
                vv_core::TrackFlag::Muted,
                muted,
            )));
        }
        for (track, clip, gain) in change.gains {
            commands.push(Box::new(vv_core::SetClipValue::new(
                timeline,
                track,
                clip,
                CommandLabel::Gain,
                gain,
                |c| &mut c.effects.gain_db,
            )));
        }
        self.session.history.do_command(
            &mut self.session.project,
            Box::new(CompositeCommand::new(CommandLabel::DubLanguage, commands)),
        );
    }

    /// The language playing now: the only unmuted dub, or the original.
    pub(crate) fn playing_dub(&self, timeline: TimelineId) -> Language {
        let tl = &self.session.project.timelines[timeline];
        let playing: Vec<&str> = tl
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio && !t.muted)
            .filter_map(|t| t.dub.as_ref().map(|d| d.language.as_str()))
            .collect();
        match playing.as_slice() {
            [] => Language::Original,
            [one] => Language::Dub((*one).to_owned()),
            _ => Language::AsIs,
        }
    }
}

/// The rows' voice files one after the other in one WAV, each padded to
/// whole media frames; returns it with each row's media frame range.
fn concatenate(
    rows: &[(Row, dub::Fit)],
    language: &str,
    project: Option<&Path>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(),
) -> Result<(PathBuf, Vec<(FrameIdx, FrameIdx)>), String> {
    let mut samples: Vec<f32> = Vec::new();
    let mut rate = 0u32;
    let mut offsets = Vec::new();
    let media_fps = vv_media::AUDIO_ONLY_FPS.as_f64();
    for (row, _) in rows {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let audio = row.audio.as_ref().ok_or("missing voice")?;
        let buffer = vv_media::decode_audio_track(&audio.path, 0)
            .map_err(|e| e.to_string())?
            .ok_or("voice file without audio")?;
        if rate == 0 {
            rate = buffer.sample_rate;
        } else if buffer.sample_rate != rate {
            return Err("voice files with different sample rates".into());
        }
        let channels = usize::from(buffer.channels.max(1));
        let per_frame = (f64::from(rate) / media_fps).round() as usize;
        let start = samples.len() / per_frame.max(1);
        samples.extend(
            buffer
                .samples
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
        let padded = samples.len().div_ceil(per_frame.max(1)) * per_frame;
        samples.resize(padded, 0.0);
        offsets.push((
            start as FrameIdx,
            (samples.len() / per_frame.max(1)) as FrameIdx,
        ));
        progress();
    }
    let dir = match project.and_then(Path::parent) {
        Some(dir) => dir.join("Dub"),
        None => cache_dir().ok_or("no cache folder")?.join("tracks"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let keys: String = rows
        .iter()
        .filter_map(|(r, _)| r.audio.as_ref().map(|a| a.key.as_str()))
        .collect();
    let path = dir.join(format!("dub-{language}-{}.wav", &sha(&keys)[..12]));
    if !path.is_file() {
        write_wav_atomically(&path, |part| {
            vv_media::audio_file::write_audio_file(
                part,
                vv_media::audio_file::AudioFileFormat::Wav,
                &samples,
                rate,
                1,
            )
            .map_err(|e| e.to_string())?;
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            Ok(())
        })?;
    }
    Ok((path, offsets))
}

fn write_wav_atomically<T>(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let part = path.with_extension(format!("{}-{id}.part.wav", std::process::id()));
    let result = write(&part).and_then(|value| match std::fs::rename(&part, path) {
        Ok(()) => Ok(value),
        Err(_) if path.is_file() => Ok(value),
        Err(e) => Err(e.to_string()),
    });
    let _ = std::fs::remove_file(&part);
    result
}

#[cfg(test)]
#[path = "../tests/dub/mod.rs"]
mod tests;
