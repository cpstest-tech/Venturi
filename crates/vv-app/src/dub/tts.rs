//! Speech synthesis for the dub, behind `Voice` so providers can be swapped.
//!
//! Shipped: Piper, local and offline. Its phonemizer (espeak-ng) is a C
//! library, so Piper runs as the release's own binary instead of being
//! linked; both binary and voices are downloaded on first use, after the
//! user confirms, into the models folder.
//!
//! Extension point for a cloned voice (XTTS run locally, or a service):
//! implement `Voice` (synthesize one line into a WAV file), return it from
//! `voice_for` for its voice id. A cloned voice also needs a
//! reference sample of the speaker: take it from the clip's own audio
//! (the transcript gives clean phrase boundaries) and keep it next to the
//! models, never inside the project. A service must be opted into in the
//! settings like the Gemini translation, saying that audio leaves the device.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use vv_matting::{ModelFile, Progress};

/// One synthesized line.
pub(crate) trait Voice: Send + Sync {
    /// Writes `text` spoken into `out` (WAV).
    fn synthesize(&self, text: &str, out: &Path, cancel: &AtomicBool) -> Result<(), String>;
}

/// A Piper voice whose training data allows any use.
pub(crate) struct VoiceInfo {
    pub id: &'static str,
    pub language: &'static str,
    pub name: &'static str,
    pub license: &'static str,
    model: (u64, &'static str),
    config: (u64, &'static str),
}

impl VoiceInfo {
    fn base_url(&self) -> String {
        let locale = self.id.split('-').next().unwrap_or_default();
        let family = locale.split('_').next().unwrap_or_default();
        let speaker = self.id.split('-').nth(1).unwrap_or_default();
        let quality = self.id.split('-').nth(2).unwrap_or_default();
        format!(
            "https://huggingface.co/rhasspy/piper-voices/resolve/{VOICE_REVISION}/{family}/{locale}/{speaker}/{quality}/{}",
            self.id
        )
    }

    pub fn card_url(&self) -> String {
        self.base_url()
            .rsplit_once('/')
            .map_or_else(String::new, |(dir, _)| format!("{dir}/MODEL_CARD"))
    }

    pub fn cache_id(&self) -> &str {
        self.model.1
    }

    pub fn size_mb(&self) -> u32 {
        (self.model.0 + self.config.0).div_ceil(1_000_000) as u32
    }
}

const VOICE_REVISION: &str = "b4e358ffcb232591bf73a540e70f7de528e4174a";

const fn voice(
    id: &'static str,
    language: &'static str,
    name: &'static str,
    license: &'static str,
    model: (u64, &'static str),
    config: (u64, &'static str),
) -> VoiceInfo {
    VoiceInfo {
        id,
        language,
        name,
        license,
        model,
        config,
    }
}

pub(crate) const VOICES: &[VoiceInfo] = &[
    voice(
        "it_IT-paola-medium",
        "it",
        "Paola",
        "CC0",
        (
            63511038,
            "6fc918b5a0ea6137382833dddfa567bffbe6a5060c02043c87192ee59c04210c",
        ),
        (
            7099,
            "aea19c0a7fce29fbc359b93f10e7902854401e4c95ae2ea328ae516b15d296cf",
        ),
    ),
    voice(
        "en_US-joe-medium",
        "en",
        "Joe",
        "CC0",
        (
            63201294,
            "58afce0321b8d9c46d7cdf9c16500cc55a793b4220212dba6b70fb788b3baf06",
        ),
        (
            4794,
            "3d6d5410b3795cb1950595247ef8f06190719e6fdbfa3a2356d8ec368e1aad33",
        ),
    ),
    voice(
        "en_US-kristin-medium",
        "en",
        "Kristin",
        "Public domain",
        (
            63531379,
            "5849957f929cbf720c258f8458692d6103fff2f0e3d3b19c8259474bb06a18d4",
        ),
        (
            4968,
            "5681426d4aead22195de70531eeeeddb46493cfaffc5764b2ea3db73428b651c",
        ),
    ),
    voice(
        "en_US-john-medium",
        "en",
        "John",
        "Public domain",
        (
            63531379,
            "789c6c875726e627ddee93d51d8727859abe9c091c3d141591f4b83c2072e988",
        ),
        (
            4965,
            "af60f177b6b550f3d7a302720c0fb89e7f94a82b5dca464775ef63b1c69ba09a",
        ),
    ),
    voice(
        "es_ES-davefx-medium",
        "es",
        "Davefx",
        "CC0",
        (
            63201294,
            "6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917",
        ),
        (
            4817,
            "0e0dda87c732f6f38771ff274a6380d9252f327dca77aa2963d5fbdf9ec54842",
        ),
    ),
    voice(
        "es_MX-ald-medium",
        "es",
        "Ald (MX)",
        "Unlicense",
        (
            63201294,
            "019b3803293c93e34a206dd2e53a3889209a514e786fd7144f7b70196c579b63",
        ),
        (
            4878,
            "5a71498158e04afc8099bfd019c7e87c68eb9d042505a2b1a87e5c1ac2b1a61d",
        ),
    ),
    voice(
        "fr_FR-siwis-medium",
        "fr",
        "Siwis",
        "CC BY 4.0",
        (
            63201294,
            "641d1ab097da2b81128c076810edb052b385decc8be3381814802a64a73baf99",
        ),
        (
            4875,
            "39479916c2db192b5ac9764daddd0c744d83e023ad890c6976c0633ae4df8959",
        ),
    ),
    voice(
        "de_DE-thorsten-medium",
        "de",
        "Thorsten",
        "CC0",
        (
            63201294,
            "7e64762d8e5118bb578f2eea6207e1a35a8e0c30595010b666f983fc87bb7819",
        ),
        (
            4819,
            "974adee790533adb273a1ac88f49027d2a1b8f0f2cf4905954a4791e79264e85",
        ),
    ),
    voice(
        "pt_BR-faber-medium",
        "pt",
        "Faber",
        "CC0",
        (
            63201294,
            "858555e3a064209c57088fe6bd70c4c3dc54d03eaa00c45d5ecaf43a33f95aa7",
        ),
        (
            4855,
            "7e694de195ae3fc36dd732c445eb04fb49b649854893cb5506b978f0d50a1d6f",
        ),
    ),
    voice(
        "pt_BR-cadu-medium",
        "pt",
        "Cadu",
        "CC0",
        (
            62950044,
            "765f0809a6ea9035d4a6d0d008dbf8876e68b2dd32029312672fa8f405bdb535",
        ),
        (
            5040,
            "5fe03aa3d4901880554905b12075713cd552598c8a350455a1ec73f8b4e6be19",
        ),
    ),
];

/// Languages a dub can be made in: those with a voice.
pub(crate) fn languages() -> Vec<&'static str> {
    let mut out: Vec<&str> = VOICES.iter().map(|v| v.language).collect();
    out.dedup();
    out
}

pub(crate) fn voices_for(language: &str) -> impl Iterator<Item = &'static VoiceInfo> + '_ {
    VOICES.iter().filter(move |v| v.language == language)
}

pub(crate) fn find_voice(id: &str) -> Option<&'static VoiceInfo> {
    VOICES.iter().find(|v| v.id == id)
}

const PIPER_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";

/// The release archive for this platform, its size and SHA-256 (computed
/// on download, the release publishes none).
fn piper_asset() -> Option<(&'static str, u64, &'static str)> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => (
            "piper_linux_x86_64.tar.gz",
            26_460_462,
            "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992",
        ),
        ("linux", "aarch64") => (
            "piper_linux_aarch64.tar.gz",
            26_004_717,
            "fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb",
        ),
        ("windows", "x86_64") => (
            "piper_windows_amd64.zip",
            22_477_236,
            "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea",
        ),
        _ => return None,
    })
}

pub(crate) fn piper_supported() -> bool {
    piper_asset().is_some()
}

fn piper_size_mb() -> u32 {
    piper_asset().map_or(0, |(_, bytes, _)| bytes.div_ceil(1_000_000) as u32)
}

fn piper_dir(models: &Path) -> PathBuf {
    models.join("piper")
}

fn piper_binary(models: &Path) -> PathBuf {
    piper_dir(models).join(if cfg!(windows) { "piper.exe" } else { "piper" })
}

fn voice_files(models: &Path, v: &VoiceInfo) -> (ModelFile, ModelFile, PathBuf) {
    let base = v.base_url();
    (
        ModelFile::new(format!("{}.onnx", v.id), format!("{base}.onnx"), v.model.0),
        ModelFile::new(
            format!("{}.onnx.json", v.id),
            format!("{base}.onnx.json"),
            v.config.0,
        ),
        models.join("piper-voices"),
    )
}

/// What is still to download for `voice`: (engine, voice) sizes in MB.
pub(crate) fn missing(models: &Path, voice: &VoiceInfo) -> (Option<u32>, Option<u32>) {
    let engine = (!piper_binary(models).is_file()).then(piper_size_mb);
    let (model, config, dir) = voice_files(models, voice);
    let voice_missing =
        !(vv_matting::model_present(&dir, &model) && vv_matting::model_present(&dir, &config));
    (engine, voice_missing.then(|| voice.size_mb()))
}

/// Downloads the engine (checked against its SHA-256 and unpacked with the
/// system `tar`, which reads zip files on Windows too) and the voice.
pub(crate) fn install(
    models: &Path,
    voice: &VoiceInfo,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if !piper_binary(models).is_file() {
        let (asset, bytes, sha) = piper_asset().ok_or("piper_unsupported")?;
        let dir = piper_dir(models);
        let scratch = models.join(format!(".piper-install-{}", std::process::id()));
        let result = (|| {
            let file = ModelFile::new(asset, format!("{PIPER_RELEASE}/{asset}"), bytes);
            let archive = checked_download(&scratch, &file, sha, progress, cancel)?;
            let mut tar = std::process::Command::new("tar");
            tar.arg("-xf").arg(&archive).arg("-C").arg(&scratch);
            crate::ai_sequence::process::run_with_input(
                &mut tar,
                cancel,
                Duration::from_secs(120),
                None,
            )?;
            std::fs::rename(scratch.join("piper"), &dir)
                .or_else(|_| {
                    if piper_binary(models).is_file() {
                        Ok(())
                    } else {
                        Err(std::io::Error::other("Cannot install Piper"))
                    }
                })
                .map_err(|e| e.to_string())
        })();
        let _ = std::fs::remove_dir_all(&scratch);
        result?;
    }
    let (model, config, dir) = voice_files(models, voice);
    checked_download(&dir, &config, voice.config.1, progress, cancel)?;
    checked_download(&dir, &model, voice.model.1, progress, cancel)?;
    Ok(())
}

fn checked_download(
    dir: &Path,
    file: &ModelFile,
    checksum: &str,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let path = vv_matting::ensure_model(dir, file, progress, cancel).map_err(|e| e.to_string())?;
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    let digest: String = ring::digest::digest(&ring::digest::SHA256, &data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != checksum {
        let _ = std::fs::remove_file(&path);
        return Err("Downloaded Piper file failed SHA-256 verification".into());
    }
    Ok(path)
}

pub(crate) struct Piper {
    binary: PathBuf,
    model: PathBuf,
}

impl Voice for Piper {
    fn synthesize(&self, text: &str, out: &Path, cancel: &AtomicBool) -> Result<(), String> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .arg("--model")
            .arg(&self.model)
            .arg("--sentence_silence")
            .arg("0.1")
            .arg("--output_file")
            .arg(out);
        let line = format!("{}\n", text.replace(['\n', '\r'], " "));
        crate::ai_sequence::process::run_with_input(
            &mut command,
            cancel,
            Duration::from_secs(120),
            Some(line.into_bytes()),
        )?;
        match std::fs::metadata(out) {
            Ok(m) if m.len() > 44 => Ok(()),
            _ => Err("Piper wrote no audio".into()),
        }
    }
}

/// The installed voice `id`, ready to speak.
pub(crate) fn voice_for(models: &Path, id: &str) -> Result<Box<dyn Voice>, String> {
    let info = find_voice(id).ok_or("unknown voice")?;
    let (model, config, dir) = voice_files(models, info);
    let binary = piper_binary(models);
    if !binary.is_file()
        || !vv_matting::model_present(&dir, &model)
        || !vv_matting::model_present(&dir, &config)
    {
        return Err("voice_missing".into());
    }
    Ok(Box::new(Piper {
        binary,
        model: dir.join(format!("{}.onnx", info.id)),
    }))
}
