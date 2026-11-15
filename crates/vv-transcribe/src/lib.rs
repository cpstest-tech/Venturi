//! Local speech-to-text: whisper.cpp through `whisper-rs`, with the model
//! downloaded on first use. Everything here is blocking: callers run it on a
//! worker thread and poll `Progress`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

mod speech;
pub use speech::{Speech, detect_speech, fit_to_speech};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    /// Fast enough on any CPU, but rougher text.
    Small,
    /// Much better text and punctuation; on a CPU about 2x slower than small.
    Turbo,
}

impl Model {
    pub fn file(self) -> &'static str {
        match self {
            Self::Small => "ggml-small-q5_1.bin",
            Self::Turbo => "ggml-large-v3-turbo-q8_0.bin",
        }
    }

    pub fn size_mb(self) -> u32 {
        match self {
            Self::Small => 190,
            Self::Turbo => 874,
        }
    }

    fn url(self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file()
        )
    }

    fn dtw_preset(self) -> DtwModelPreset {
        match self {
            Self::Small => DtwModelPreset::Small,
            Self::Turbo => DtwModelPreset::LargeV3Turbo,
        }
    }

    /// Turbo where a GPU runs it (40 s of speech in under 2 s on an RTX
    /// 2060), small otherwise.
    pub fn preferred() -> Self {
        if gpu_name().is_some() {
            Self::Turbo
        } else {
            Self::Small
        }
    }
}

/// The GPU whisper runs on, if Vulkan finds one.
pub fn gpu_name() -> Option<&'static str> {
    static GPU: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    GPU.get_or_init(|| {
        #[cfg(not(target_os = "macos"))]
        {
            whisper_rs::vulkan::list_devices()
                .into_iter()
                .next()
                .map(|d| d.name)
        }
        #[cfg(target_os = "macos")]
        {
            Some("Apple GPU".to_string())
        }
    })
    .as_deref()
}
/// What whisper.cpp wants.
pub const SAMPLE_RATE: u32 = 16_000;

/// `(code, English name)`; the UI translates the names through its locale.
pub const LANGUAGES: [&str; 12] = [
    "it", "en", "es", "fr", "de", "pt", "nl", "ru", "pl", "tr", "ja", "zh",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transcript {
    pub language: String,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Download(String),
    Model(String),
    Transcribe(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Download(e) | Self::Model(e) | Self::Transcribe(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    Idle = 0,
    Download = 1,
    Transcribe = 2,
}

/// Shared with the UI thread, which polls it.
#[derive(Default)]
pub struct Progress {
    stage: AtomicU8,
    /// 0..=1000 within the current stage.
    permille: AtomicU32,
}

impl Progress {
    pub fn stage(&self) -> Stage {
        match self.stage.load(Ordering::Relaxed) {
            1 => Stage::Download,
            2 => Stage::Transcribe,
            _ => Stage::Idle,
        }
    }

    pub fn fraction(&self) -> f32 {
        self.permille.load(Ordering::Relaxed) as f32 / 1000.0
    }

    fn enter(&self, stage: Stage) {
        self.stage.store(stage as u8, Ordering::Relaxed);
        self.permille.store(0, Ordering::Relaxed);
    }

    fn set(&self, fraction: f64) {
        let permille = (fraction.clamp(0.0, 1.0) * 1000.0) as u32;
        self.permille.store(permille, Ordering::Relaxed);
    }
}

pub fn model_path(dir: &Path, model: Model) -> PathBuf {
    dir.join(model.file())
}

pub fn model_present(dir: &Path, model: Model) -> bool {
    model_path(dir, model).is_file()
}

/// Downloads the model into `dir` unless it is already there. The file is
/// written under a temporary name, so an interrupted download is never
/// mistaken for a model.
pub fn ensure_model(
    dir: &Path,
    model: Model,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<PathBuf, Error> {
    let path = model_path(dir, model);
    if path.is_file() {
        return Ok(path);
    }
    progress.enter(Stage::Download);
    std::fs::create_dir_all(dir).map_err(|e| Error::Download(e.to_string()))?;
    let part = path.with_extension("part");
    let result = download(&model.url(), &part, progress, cancel);
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    std::fs::rename(&part, &path).map_err(|e| Error::Download(e.to_string()))?;
    Ok(path)
}

fn download(url: &str, part: &Path, progress: &Progress, cancel: &AtomicBool) -> Result<(), Error> {
    let fail = |e: &dyn std::fmt::Display| Error::Download(e.to_string());
    let mut response = ureq::get(url).call().map_err(|e| fail(&e))?;
    let total = response.body().content_length();
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(part).map_err(|e| fail(&e))?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let read = reader.read(&mut buffer).map_err(|e| fail(&e))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| fail(&e))?;
        done += read as u64;
        if let Some(total) = total {
            progress.set(done as f64 / total as f64);
        }
    }
    file.sync_all().map_err(|e| fail(&e))?;
    match total {
        Some(total) if total != done => Err(Error::Download(format!(
            "incomplete download ({done} of {total} bytes)"
        ))),
        _ => Ok(()),
    }
}

/// Mono at 16 kHz from interleaved samples, by averaging the channels and
/// then the frames of each output sample (a multiple of 16 kHz) or by linear
/// interpolation: enough for speech recognition. Whole frames only: callers
/// that feed chunks keep the remainder (`frames_per_output`) for the next.
pub fn to_mono_16k(interleaved: &[f32], rate: u32, channels: u16) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    let mono: Vec<f32> = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    if rate == SAMPLE_RATE || mono.is_empty() {
        return mono;
    }
    if rate.is_multiple_of(SAMPLE_RATE) {
        let block = (rate / SAMPLE_RATE) as usize;
        return mono
            .chunks_exact(block)
            .map(|frames| frames.iter().sum::<f32>() / block as f32)
            .collect();
    }
    let step = f64::from(rate) / f64::from(SAMPLE_RATE);
    let count = (mono.len() as f64 / step) as usize;
    (0..count)
        .map(|i| {
            let position = i as f64 * step;
            let index = position as usize;
            let next = mono.get(index + 1).copied().unwrap_or(mono[index]);
            let t = (position - index as f64) as f32;
            mono[index] + (next - mono[index]) * t
        })
        .collect()
}

/// Input frames `to_mono_16k` turns into one sample, for a rate that is a
/// multiple of 16 kHz.
pub fn frames_per_output(rate: u32) -> Option<usize> {
    rate.is_multiple_of(SAMPLE_RATE)
        .then_some((rate / SAMPLE_RATE) as usize)
}

/// A token as whisper reports it, times in centiseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct RawToken {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    /// Where the cross-attention alignment (DTW) puts it, -1 if it did not.
    pub dtw: i64,
}

/// DTW marks a token about this late compared to where its sound starts
/// (measured on Italian speech after pauses).
const DTW_LEAD_MS: u32 = 80;

/// Words from whisper's tokens: a token starting with a space opens a word,
/// the others continue it; special tokens are dropped. A word starts at the
/// DTW time of its first token, much closer to the audio than `t0`, and
/// ends where the next one starts.
pub fn words_from_tokens(tokens: &[RawToken]) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut last_t1 = 0i64;
    for token in tokens {
        let text = token.text.as_str();
        if text.starts_with("[_") || text.starts_with("<|") {
            continue;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        last_t1 = last_t1.max(token.t1);
        match words.last_mut() {
            Some(last) if !text.starts_with(' ') => last.text.push_str(trimmed),
            _ => {
                let start_ms = if token.dtw >= 0 {
                    (token.dtw as u32 * 10).saturating_sub(DTW_LEAD_MS)
                } else {
                    token.t0.max(0) as u32 * 10
                };
                let start_ms = start_ms.max(words.last().map_or(0, speech::earliest_next));
                words.push(Word {
                    text: trimmed.to_owned(),
                    start_ms,
                    end_ms: 0,
                });
            }
        }
    }
    let last_end = last_t1.max(0) as u32 * 10;
    for i in 0..words.len() {
        let start = words[i].start_ms;
        let next = words.get(i + 1).map_or(last_end, |w| w.start_ms);
        let end = next.min(start + speech::longest_word_ms(&words[i].text));
        words[i].end_ms = end.max(start + 1);
    }
    words
}

/// `data` is an `AtomicBool`: the cancel flag.
unsafe extern "C" fn abort_requested(data: *mut std::ffi::c_void) -> bool {
    // SAFETY: see where it is installed in `transcribe`.
    unsafe { &*data.cast::<AtomicBool>() }.load(Ordering::Relaxed)
}

/// A few punctuated sentences in the language: without them the large
/// models often write everything lowercase and without full stops.
fn style_prompt(language: &str) -> Option<&'static str> {
    Some(match language {
        "it" => "Ciao a tutti, oggi vi parlo di una cosa. Pronti? Iniziamo.",
        "en" => "Hi everyone, today I'll tell you about something. Ready? Let's start.",
        "es" => "Hola a todos, hoy os hablo de una cosa. ¿Listos? Empezamos.",
        "fr" => "Salut tout le monde, aujourd'hui je vous parle d'une chose. Prêts ? On commence.",
        "de" => "Hallo zusammen, heute erzähle ich euch etwas. Bereit? Los geht's.",
        "pt" => "Olá a todos, hoje vou falar de uma coisa. Prontos? Vamos começar.",
        "nl" => "Hallo allemaal, vandaag vertel ik jullie iets. Klaar? We beginnen.",
        "ru" => "Всем привет, сегодня я расскажу вам об одной вещи. Готовы? Начинаем.",
        "pl" => "Cześć wszystkim, dzisiaj opowiem wam o jednej rzeczy. Gotowi? Zaczynamy.",
        "tr" => "Herkese merhaba, bugün size bir şeyden bahsedeceğim. Hazır mısınız? Başlıyoruz.",
        "ja" => {
            "皆さん、こんにちは。今日はあることについて話します。準備はいいですか？始めましょう。"
        }
        "zh" => "大家好，今天我来跟你们聊一件事。准备好了吗？我们开始吧。",
        _ => return None,
    })
}

/// The language spoken in the first 30 s of voice.
fn detect_language(
    state: &mut whisper_rs::WhisperState,
    samples: &[f32],
    speech: Option<&Speech>,
    threads: usize,
) -> Option<&'static str> {
    let first_voice_ms = speech.and_then(|s| s.spans.first()).map_or(0, |s| s.start);
    let from = (first_voice_ms as usize * SAMPLE_RATE as usize / 1000).min(samples.len());
    let to = (from + 30 * SAMPLE_RATE as usize).min(samples.len());
    state.pcm_to_mel(&samples[from..to], threads).ok()?;
    let (id, _) = state.lang_detect(0, threads).ok()?;
    whisper_rs::get_lang_str(id)
}

/// Transcribes 16 kHz mono `samples` with `model`, loaded from `path`.
/// `language`: ISO code, `None` to detect.
pub fn transcribe(
    model: Model,
    path: &Path,
    samples: &[f32],
    language: Option<&str>,
    progress: &Arc<Progress>,
    cancel: &Arc<AtomicBool>,
) -> Result<Transcript, Error> {
    progress.enter(Stage::Transcribe);
    let path = path
        .to_str()
        .ok_or_else(|| Error::Model("model path is not valid UTF-8".into()))?;
    let mut context_params = WhisperContextParameters::default();
    context_params.use_gpu(true);
    context_params.dtw_parameters(DtwParameters {
        mode: DtwMode::ModelPreset {
            model_preset: model.dtw_preset(),
        },
        ..DtwParameters::default()
    });
    let context = WhisperContext::new_with_params(path, context_params)
        .map_err(|e| Error::Model(e.to_string()))?;
    let mut state = context
        .create_state()
        .map_err(|e| Error::Model(e.to_string()))?;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let speech = detect_speech(samples);
    let language = language
        .or_else(|| detect_language(&mut state, samples, speech.as_ref(), threads))
        .unwrap_or("auto");

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(language));
    if let Some(prompt) = style_prompt(language) {
        params.set_initial_prompt(prompt);
    }
    params.set_token_timestamps(true);
    // "[Musica]", "(ride)" and the like are not words.
    params.set_suppress_nst(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_n_threads(threads as i32);
    {
        let progress = progress.clone();
        params.set_progress_callback_safe(move |percent: i32| {
            progress.set(f64::from(percent) / 100.0);
        });
    }
    // Not `set_abort_callback_safe`: in whisper-rs 0.16 its trampoline reads
    // the boxed closure as the wrong type, so it sees garbage and aborts the
    // encoder at random ("Generic whisper error ... 6").
    // SAFETY: `cancel` outlives `state.full`, the only caller of the callback.
    unsafe {
        params.set_abort_callback(Some(abort_requested));
        params.set_abort_callback_user_data(Arc::as_ptr(cancel).cast_mut().cast());
    }

    let ran = state.full(params, samples);
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    ran.map_err(|e| Error::Transcribe(e.to_string()))?;

    let mut tokens: Vec<RawToken> = Vec::new();
    for segment in state.as_iter() {
        for index in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(index) else {
                continue;
            };
            let data = token.token_data();
            let text = token
                .to_str_lossy()
                .map(|t| t.into_owned())
                .unwrap_or_default();
            tokens.push(RawToken {
                text,
                t0: data.t0,
                t1: data.t1,
                dtw: data.t_dtw,
            });
        }
    }
    let detected = match language {
        "auto" => whisper_rs::get_lang_str(state.full_lang_id_from_state()).unwrap_or_default(),
        code => code,
    };
    let mut words = words_from_tokens(&tokens);
    if let Some(speech) = &speech {
        fit_to_speech(&mut words, speech);
    }
    progress.set(1.0);
    Ok(Transcript {
        language: detected.to_owned(),
        words,
    })
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
