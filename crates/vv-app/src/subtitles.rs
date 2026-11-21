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
    Edit,
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
    /// The line being typed into (its first word) and its text, committed
    /// when the field loses focus.
    pub editing: Option<(usize, String)>,
    /// Keeps a drag of a line's times one undo step.
    pub drag_group: Option<vv_core::GroupMark>,
    /// Start of the selected subtitle clip the list last scrolled to.
    pub scrolled_to: Option<u32>,
    /// The properties panel showed a subtitle clip last frame.
    pub panel_on_subtitle: bool,
}

pub(crate) enum Message {
    Error(String),
    Info(String),
}

static MODELS_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// The folder chosen in the settings for every downloaded model.
pub(crate) fn set_models_dir(dir: Option<PathBuf>) {
    *MODELS_DIR.write().unwrap_or_else(|e| e.into_inner()) = dir;
}

pub(crate) fn models_dir() -> Option<PathBuf> {
    if let Some(dir) = MODELS_DIR.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return Some(dir);
    }
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
    /// Edits the words of the subtitles (made empty if there are none yet)
    /// and rebuilds their clips, keeping the look and place of the current ones.
    pub(crate) fn edit_caption_words(&mut self, edit: impl FnOnce(&mut Vec<CaptionWord>)) {
        let Some(timeline_id) = self.timeline_id else {
            return;
        };
        let mut captions = self
            .session
            .project
            .captions
            .get(&timeline_id)
            .cloned()
            .unwrap_or_else(|| Captions {
                template: self
                    .subtitles
                    .selected
                    .clone()
                    .unwrap_or_else(|| vv_core::captions::DEFAULT_TEMPLATE.to_owned()),
                ..Captions::default()
            });
        edit(&mut captions.words);
        let timeline = &self.session.project.timelines[timeline_id];
        let old: Vec<vv_core::Clip> = vv_core::captions::subtitle_track(timeline)
            .map(|i| timeline.tracks[i].clips.clone())
            .unwrap_or_default();
        self.apply_captions_with(timeline_id, captions, &old);
    }

    fn apply_captions(&mut self, timeline_id: TimelineId, captions: Captions) {
        self.apply_captions_with(timeline_id, captions, &[]);
    }

    /// `look_from`: clips whose look the new ones take, each from the
    /// nearest in time.
    fn apply_captions_with(
        &mut self,
        timeline_id: TimelineId,
        captions: Captions,
        look_from: &[vv_core::Clip],
    ) {
        if !self.session.project.timelines.contains_key(timeline_id) {
            return;
        }
        let Some(mut clips) = self.subtitle_clips(timeline_id, &captions.words, &captions.template)
        else {
            return;
        };
        carry_look(look_from, &mut clips);
        self.session.history.do_command(
            &mut self.session.project,
            Box::new(SetCaptions::new(timeline_id, Some(captions), clips)),
        );
    }

    /// The subtitle clips of `words` in `template` (a subtitle template or
    /// a `text:<id>` one).
    pub(crate) fn subtitle_clips(
        &mut self,
        timeline_id: TimelineId,
        words: &[CaptionWord],
        template: &str,
    ) -> Option<Vec<vv_core::Clip>> {
        if let Some(template) = SubtitleTemplate::find(template) {
            Some(vv_core::captions::build_clips(
                &mut self.session.project,
                timeline_id,
                words,
                template,
            ))
        } else {
            let style = self.text_subtitle_style(template)?;
            Some(style.build(&mut self.session.project, timeline_id, words))
        }
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

/// Gives each of `clips` the effects (style, position, filters) of the
/// nearest of `old`, keeping its own text and word times: editing the words
/// must not undo what the user did to the subtitles.
pub(crate) fn carry_look(old: &[vv_core::Clip], clips: &mut [vv_core::Clip]) {
    for clip in clips {
        let Some(reference) = old
            .iter()
            .min_by_key(|o| (o.timeline_start - clip.timeline_start).abs())
        else {
            return;
        };
        let Some(own) = clip.effects.title.take() else {
            continue;
        };
        let mut effects = reference.effects.clone();
        match &mut effects.title {
            Some(title) => {
                title.content = own.content;
                if let (Some(karaoke), Some(own_karaoke)) =
                    (&mut title.anim.karaoke, &own.anim.karaoke)
                {
                    karaoke.word_ms = own_karaoke.word_ms.clone();
                }
            }
            None => effects.title = Some(own),
        }
        clip.effects = effects;
    }
}
