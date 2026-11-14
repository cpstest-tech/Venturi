//! Subtitles: transcription on a worker thread, and the clips built from
//! the saved words by a template. Changing the template never transcribes
//! again: the words stay in `Project::captions`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use vv_core::{CaptionWord, Captions, SetCaptions, SubtitleTemplate, TimelineId};
use vv_transcribe::{Progress, Stage, Transcript};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Section {
    #[default]
    Generate,
    Templates,
    Add,
}

pub(crate) struct TranscribeJob {
    pub timeline: TimelineId,
    pub template: String,
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<Transcript, vv_transcribe::Error>>,
}

#[derive(Default)]
pub(crate) struct SubtitleState {
    pub section: Section,
    /// `None`: detect the language.
    pub language: Option<&'static str>,
    pub selected: Option<String>,
    pub job: Option<TranscribeJob>,
    pub message: Option<Message>,
    pub previews: HashMap<String, egui::TextureHandle>,
    /// The card under the pointer and when the pointer got there.
    pub hover: Option<(String, f64)>,
    pub hover_texture: Option<egui::TextureHandle>,
}

pub(crate) enum Message {
    Error(String),
    Info(String),
}

pub(crate) fn models_dir() -> Option<PathBuf> {
    library::Library::default_root()?
        .parent()
        .map(|dir| dir.join("models"))
}

fn words_of(transcript: &Transcript) -> Vec<CaptionWord> {
    transcript
        .words
        .iter()
        .map(|w| CaptionWord {
            text: w.text.clone(),
            start_ms: w.start_ms,
            end_ms: w.end_ms,
        })
        .collect()
}

fn error_text(error: &vv_transcribe::Error) -> String {
    use vv_transcribe::Error;
    match error {
        Error::Cancelled => t!("subtitles.cancelled"),
        Error::Download(detail) => t!("subtitles.error_download", detail = detail),
        Error::Model(detail) => t!("subtitles.error_model", detail = detail),
        Error::Transcribe(detail) => t!("subtitles.error_transcribe", detail = detail),
    }
    .into_owned()
}

/// The timeline's mix as 16 kHz mono, built as the export mixes it.
fn mix_for_speech(
    project: &vv_core::Project,
    timeline_id: TimelineId,
    cancel: &AtomicBool,
) -> Result<Vec<f32>, vv_transcribe::Error> {
    let fail = |e: &dyn std::fmt::Display| vv_transcribe::Error::Transcribe(e.to_string());
    let timeline = project
        .timelines
        .get(timeline_id)
        .ok_or_else(|| fail(&"timeline not found"))?;
    let rate = vv_audio::mixer::PROJECT_SAMPLE_RATE;
    let channels = export::PROJECT_CHANNELS;
    let block = vv_transcribe::frames_per_output(rate).expect("48 kHz divides by 16 kHz")
        * usize::from(channels);
    let frames = 0..timeline.total_frames();
    let fps = timeline.fps.as_f64();
    let samples = vv_audio::mixer::timeline_frame_to_sample(frames.start, fps, rate)
        ..vv_audio::mixer::timeline_frame_to_sample(frames.end, fps, rate);
    let mut mono = Vec::new();
    let mut carry: Vec<f32> = Vec::new();
    vv_session::audio_render::render_offline(
        project,
        timeline,
        samples,
        &mut vv_session::audio_render::LevelCache::on_disk(),
        |chunk| {
            if cancel.load(Ordering::Relaxed) {
                return std::ops::ControlFlow::Break(());
            }
            carry.extend_from_slice(chunk);
            let whole = carry.len() / block * block;
            mono.extend(vv_transcribe::to_mono_16k(&carry[..whole], rate, channels));
            carry.drain(..whole);
            std::ops::ControlFlow::Continue(())
        },
    )
    .map_err(|e| fail(&e))?;
    if cancel.load(Ordering::Relaxed) {
        return Err(vv_transcribe::Error::Cancelled);
    }
    Ok(mono)
}

impl VenturiApp {
    /// Transcribes the active timeline's audio; the clips follow with
    /// `template` when it is done.
    pub(crate) fn start_transcription(&mut self, template: String) {
        let Some(timeline_id) = self.timeline_id else {
            return;
        };
        if self.subtitles.job.is_some() {
            return;
        }
        let timeline = &self.session.project.timelines[timeline_id];
        if !timeline.has_audio_clips() {
            self.subtitles.message = Some(Message::Error(t!("subtitles.no_audio").into_owned()));
            return;
        }
        let Some(model_dir) = models_dir() else {
            self.subtitles.message =
                Some(Message::Error(t!("subtitles.no_model_dir").into_owned()));
            return;
        };
        self.subtitles.message = None;
        let project = self.session.project.clone();
        let language = self.subtitles.language;
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        std::thread::spawn({
            let (progress, cancel) = (progress.clone(), cancel.clone());
            move || {
                let result = (|| {
                    let model = vv_transcribe::Model::preferred();
                    let path = vv_transcribe::ensure_model(&model_dir, model, &progress, &cancel)?;
                    let samples = mix_for_speech(&project, timeline_id, &cancel)?;
                    vv_transcribe::transcribe(model, &path, &samples, language, &progress, &cancel)
                })();
                let _ = tx.send(result);
            }
        });
        self.subtitles.job = Some(TranscribeJob {
            timeline: timeline_id,
            template,
            progress,
            cancel,
            rx,
        });
    }

    pub(crate) fn cancel_transcription(&mut self) {
        if let Some(job) = &self.subtitles.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn poll_subtitles(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.subtitles.job else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(vv_transcribe::Error::Transcribe("worker stopped".into()))
            }
        };
        let Some(job) = self.subtitles.job.take() else {
            return;
        };
        match result {
            Ok(transcript) if transcript.words.is_empty() => {
                self.subtitles.message =
                    Some(Message::Info(t!("subtitles.no_speech").into_owned()));
            }
            Ok(transcript) => {
                let captions = Captions {
                    language: transcript.language.clone(),
                    words: words_of(&transcript),
                    template: job.template.clone(),
                    linked: self
                        .session
                        .project
                        .captions
                        .get(&job.timeline)
                        .is_none_or(|c| c.linked),
                };
                self.apply_captions(job.timeline, captions);
            }
            Err(vv_transcribe::Error::Cancelled) => {
                self.subtitles.message =
                    Some(Message::Info(t!("subtitles.cancelled").into_owned()));
            }
            Err(e) => self.subtitles.message = Some(Message::Error(error_text(&e))),
        }
    }

    /// Replaces the subtitle clips with the words of `captions` in its
    /// template, in one undo step.
    fn apply_captions(&mut self, timeline_id: TimelineId, captions: Captions) {
        if !self.session.project.timelines.contains_key(timeline_id) {
            return;
        }
        let clips = if let Some(template) = SubtitleTemplate::find(&captions.template) {
            vv_core::captions::build_clips(
                &mut self.session.project,
                timeline_id,
                &captions.words,
                template,
            )
        } else if let Some(style) = self.text_subtitle_style(&captions.template) {
            style.build(&mut self.session.project, timeline_id, &captions.words)
        } else {
            return;
        };
        self.session.history.do_command(
            &mut self.session.project,
            Box::new(SetCaptions::new(timeline_id, Some(captions), clips)),
        );
    }

    /// The text template behind a `text:<id>` subtitle template id.
    fn text_subtitle_style(&self, template: &str) -> Option<crate::text_templates::SubtitleStyle> {
        use crate::text_templates::SubtitleStyle;
        let id = template.strip_prefix(crate::text_templates::SUBTITLE_PREFIX)?;
        if let Some(def) = crate::text_templates::find(id) {
            return Some(SubtitleStyle::Bundled(def));
        }
        let user = self.settings.text_templates.iter().find(|t| t.id == id)?;
        Some(SubtitleStyle::User(Box::new(user.title.clone())))
    }

    /// A template was chosen: with subtitles already there they are rebuilt
    /// from the saved words at once, without transcribing.
    pub(crate) fn choose_subtitle_template(&mut self, id: &str) {
        self.subtitles.selected = Some(id.to_owned());
        let Some(timeline_id) = self.timeline_id else {
            return;
        };
        if let Some(mut captions) = self.session.project.captions.get(&timeline_id).cloned() {
            captions.template = id.to_owned();
            self.apply_captions(timeline_id, captions);
        }
    }

    /// The "Genera sottotitoli" button: saved words are restyled, otherwise
    /// the transcription starts.
    pub(crate) fn generate_subtitles(&mut self) {
        let template = self
            .subtitles
            .selected
            .clone()
            .unwrap_or_else(|| vv_core::captions::DEFAULT_TEMPLATE.to_owned());
        self.subtitles.section = Section::Generate;
        let has_words = self
            .timeline_id
            .is_some_and(|id| self.session.project.captions.contains_key(&id));
        if has_words {
            self.choose_subtitle_template(&template);
        } else {
            self.start_transcription(template);
        }
    }

    /// Ritrascrive: runs the model again, replacing the saved words.
    pub(crate) fn retranscribe_subtitles(&mut self) {
        let template = self
            .subtitles
            .selected
            .clone()
            .unwrap_or_else(|| vv_core::captions::DEFAULT_TEMPLATE.to_owned());
        self.start_transcription(template);
    }

    pub(crate) fn transcription_stage(&self) -> Option<(Stage, f32)> {
        let job = self.subtitles.job.as_ref()?;
        Some((job.progress.stage(), job.progress.fraction()))
    }
}
