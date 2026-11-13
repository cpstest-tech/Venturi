//! Local speech-to-text: whisper.cpp through `whisper-rs`, with the model
//! downloaded on first use. Everything here is blocking: callers run it on a
//! worker thread and poll `Progress`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const MODEL_FILE: &str = "ggml-small-q5_1.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin";
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

pub fn model_path(dir: &Path) -> PathBuf {
    dir.join(MODEL_FILE)
}

pub fn model_present(dir: &Path) -> bool {
    model_path(dir).is_file()
}

/// Downloads the model into `dir` unless it is already there. The file is
/// written under a temporary name, so an interrupted download is never
/// mistaken for a model.
pub fn ensure_model(
    dir: &Path,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<PathBuf, Error> {
    let path = model_path(dir);
    if path.is_file() {
        return Ok(path);
    }
    progress.enter(Stage::Download);
    std::fs::create_dir_all(dir).map_err(|e| Error::Download(e.to_string()))?;
    let part = path.with_extension("part");
    let result = download(&part, progress, cancel);
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    std::fs::rename(&part, &path).map_err(|e| Error::Download(e.to_string()))?;
    Ok(path)
}

fn download(part: &Path, progress: &Progress, cancel: &AtomicBool) -> Result<(), Error> {
    let fail = |e: &dyn std::fmt::Display| Error::Download(e.to_string());
    let mut response = ureq::get(MODEL_URL).call().map_err(|e| fail(&e))?;
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
    if rate % SAMPLE_RATE == 0 {
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
    (rate % SAMPLE_RATE == 0).then(|| (rate / SAMPLE_RATE) as usize)
}

/// A token as whisper reports it: text and times in centiseconds.
pub type RawToken = (String, i64, i64);

/// Words from whisper's tokens: a token starting with a space opens a word,
/// the others continue it; special tokens are dropped.
pub fn words_from_tokens(tokens: &[RawToken]) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut continues = false;
    for (text, t0, t1) in tokens {
        if text.starts_with("[_") || text.starts_with("<|") {
            continue;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let start_ms = (*t0).max(0) as u32 * 10;
        let end_ms = ((*t1).max(*t0)).max(0) as u32 * 10;
        let opens = text.starts_with(' ') || words.is_empty() || !continues;
        match words.last_mut() {
            Some(last) if !opens => {
                last.text.push_str(trimmed);
                last.end_ms = last.end_ms.max(end_ms);
            }
            _ => words.push(Word {
                text: trimmed.to_owned(),
                start_ms,
                end_ms,
            }),
        }
        continues = true;
    }
    for i in 1..words.len() {
        // Overlaps and zero-length words: each word ends where the next starts.
        let next_start = words[i].start_ms;
        let word = &mut words[i - 1];
        if word.end_ms > next_start || word.end_ms <= word.start_ms {
            word.end_ms = next_start.max(word.start_ms + 1);
        }
    }
    words
}

/// Transcribes 16 kHz mono `samples`. `language`: ISO code, `None` to detect.
pub fn transcribe(
    model: &Path,
    samples: &[f32],
    language: Option<&str>,
    progress: &Arc<Progress>,
    cancel: &Arc<AtomicBool>,
) -> Result<Transcript, Error> {
    progress.enter(Stage::Transcribe);
    let model = model
        .to_str()
        .ok_or_else(|| Error::Model("model path is not valid UTF-8".into()))?;
    let context = WhisperContext::new_with_params(model, WhisperContextParameters::default())
        .map_err(|e| Error::Model(e.to_string()))?;
    let mut state = context
        .create_state()
        .map_err(|e| Error::Model(e.to_string()))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(language.unwrap_or("auto")));
    params.set_token_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    params.set_n_threads(threads as i32);
    {
        let progress = progress.clone();
        params.set_progress_callback_safe(move |percent: i32| {
            progress.set(f64::from(percent) / 100.0);
        });
        let cancel = cancel.clone();
        params.set_abort_callback_safe(move || cancel.load(Ordering::Relaxed));
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
            let text = token.to_str_lossy().map(|t| t.into_owned()).unwrap_or_default();
            tokens.push((text, data.t0, data.t1));
        }
    }
    let detected = language.map(str::to_owned).or_else(|| {
        whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_owned)
    });
    progress.set(1.0);
    Ok(Transcript {
        language: detected.unwrap_or_default(),
        words: words_from_tokens(&tokens),
    })
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
