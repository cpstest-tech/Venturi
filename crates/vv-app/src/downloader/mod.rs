//! Downloads from YouTube, Instagram, TikTok and Spotify (matched on
//! YouTube) through yt-dlp, in the background: each job has its own thread,
//! at most `SLOTS` of them download at once.

pub mod source;
pub mod spotify;
pub mod ytdlp;

use serde::{Deserialize, Serialize};
use source::{Link, Site};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

const SLOTS: usize = 2;
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const CUT_TIMEOUT: Duration = Duration::from_secs(3 * 3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioFormat {
    /// The stream as served (Opus or AAC): no generation loss.
    #[default]
    Original,
    Wav,
    Flac,
    Mp3,
}

impl AudioFormat {
    pub const ALL: [Self; 4] = [Self::Original, Self::Wav, Self::Flac, Self::Mp3];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Music,
    Sfx,
    Project,
}

pub const HEIGHTS: [u32; 5] = [2160, 1440, 1080, 720, 480];

/// The choices of the last download, offered again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub audio_format: AudioFormat,
    pub max_height: u32,
    pub prefer_h264: bool,
    /// Browser whose cookies yt-dlp reads (private Instagram posts); empty
    /// for none.
    pub cookies_browser: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            audio_format: AudioFormat::Original,
            max_height: 1080,
            prefer_h264: true,
            cookies_browser: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub title: String,
    pub kind: MediaKind,
    pub audio_format: AudioFormat,
    pub max_height: u32,
    pub prefer_h264: bool,
    pub range: Option<(f64, f64)>,
    pub destination: Destination,
    /// Searched on YouTube when the job starts.
    pub spotify: Option<spotify::Track>,
    /// Of the session that asked: the project may have changed since.
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Waiting,
    Preparing,
    Downloading {
        fraction: Option<f32>,
        speed: Option<f64>,
        eta: Option<f64>,
    },
    Processing,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct JobView {
    pub id: u64,
    pub request: Request,
    pub phase: Phase,
}

impl JobView {
    pub fn is_active(&self) -> bool {
        !matches!(self.phase, Phase::Failed(_))
    }
}

pub struct Finished {
    pub request: Request,
    pub result: Result<Vec<PathBuf>, String>,
}

pub const CANCELLED: &str = "Cancelled";

#[derive(Debug, Clone)]
pub struct Dirs {
    pub tools: PathBuf,
    /// Copied into the library on import: only a cache.
    pub for_library: PathBuf,
    /// Used in place by the project: kept.
    pub for_project: PathBuf,
}

impl Dirs {
    pub fn default_dirs() -> Self {
        let data = crate::library::Library::default_root()
            .and_then(|root| Some(root.parent()?.to_path_buf()))
            .unwrap_or_else(|| vv_media::cache_dir("data"));
        Self {
            tools: data.join("tools"),
            for_library: vv_media::cache_dir("downloads"),
            for_project: data.join("downloads"),
        }
    }
}

struct Job {
    view: JobView,
    cancel: Arc<AtomicBool>,
}

struct Shared {
    jobs: Mutex<Vec<Job>>,
    running: Mutex<usize>,
    freed: Condvar,
    ctx: egui::Context,
}

impl Shared {
    fn set_phase(&self, id: u64, phase: Phase) {
        if let Some(job) = self
            .jobs
            .lock()
            .unwrap()
            .iter_mut()
            .find(|j| j.view.id == id)
        {
            job.view.phase = phase;
        }
        self.ctx.request_repaint();
    }

    fn acquire(&self, cancel: &AtomicBool) -> Result<SlotGuard<'_>, String> {
        let mut running = self.running.lock().unwrap();
        while *running >= SLOTS {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.into());
            }
            running = self
                .freed
                .wait_timeout(running, Duration::from_millis(200))
                .unwrap()
                .0;
        }
        *running += 1;
        Ok(SlotGuard(self))
    }
}

struct SlotGuard<'a>(&'a Shared);

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        *self.0.running.lock().unwrap() -= 1;
        self.0.freed.notify_one();
    }
}

pub struct Downloader {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Finished>,
    rx: mpsc::Receiver<Finished>,
    next_id: u64,
    dirs: Dirs,
}

impl Downloader {
    pub fn new(ctx: egui::Context, dirs: Dirs) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            shared: Arc::new(Shared {
                jobs: Mutex::new(Vec::new()),
                running: Mutex::new(0),
                freed: Condvar::new(),
                ctx,
            }),
            tx,
            rx,
            next_id: 1,
            dirs,
        }
    }

    pub fn enqueue(&mut self, request: Request, env: ytdlp::Env) {
        let id = self.next_id;
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.shared.jobs.lock().unwrap().push(Job {
            view: JobView {
                id,
                request: request.clone(),
                phase: Phase::Waiting,
            },
            cancel: cancel.clone(),
        });
        let shared = self.shared.clone();
        let tx = self.tx.clone();
        let dirs = self.dirs.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("download-{id}"))
            .spawn(move || {
                let result = run_job(&shared, id, &request, &env, &dirs, &cancel);
                {
                    let mut jobs = shared.jobs.lock().unwrap();
                    match &result {
                        Err(error) if error != CANCELLED => {
                            if let Some(job) = jobs.iter_mut().find(|j| j.view.id == id) {
                                job.view.phase = Phase::Failed(error.clone());
                            }
                        }
                        _ => jobs.retain(|j| j.view.id != id),
                    }
                }
                let _ = tx.send(Finished { request, result });
                shared.ctx.request_repaint();
            });
        if let Err(error) = spawned {
            self.shared.set_phase(id, Phase::Failed(error.to_string()));
        }
    }

    pub fn ctx(&self) -> egui::Context {
        self.shared.ctx.clone()
    }

    pub fn jobs(&self) -> Vec<JobView> {
        self.shared
            .jobs
            .lock()
            .unwrap()
            .iter()
            .map(|j| j.view.clone())
            .collect()
    }

    pub fn cancel(&self, id: u64) {
        if let Some(job) = self
            .shared
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.view.id == id)
        {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Drops a failed job from the list.
    pub fn dismiss(&self, id: u64) {
        self.shared
            .jobs
            .lock()
            .unwrap()
            .retain(|j| j.view.id != id || j.view.is_active());
    }

    pub fn retry(&mut self, id: u64, env: ytdlp::Env) {
        let request = self
            .shared
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.view.id == id && !j.view.is_active())
            .map(|j| j.view.request.clone());
        if let Some(request) = request {
            self.dismiss(id);
            self.enqueue(request, env);
        }
    }

    pub fn poll(&self) -> Vec<Finished> {
        self.rx.try_iter().collect()
    }

    pub fn probe(&self, link: Link, env: ytdlp::Env) -> Probe {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let stop = cancel.clone();
        let ctx = self.shared.ctx.clone();
        let tools = self.dirs.tools.clone();
        let spawned = std::thread::Builder::new()
            .name("download-probe".into())
            .spawn(move || {
                let _ = tx.send(probe(&link, &env, &tools, &stop));
                ctx.request_repaint();
            });
        if let Err(error) = spawned {
            let (tx, failed) = mpsc::channel();
            let _ = tx.send(Err(error.to_string()));
            return Probe { rx: failed, cancel };
        }
        Probe { rx, cancel }
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        for job in self.shared.jobs.lock().unwrap().iter() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

pub enum Probed {
    Media {
        info: ytdlp::Info,
        thumbnail: Option<vv_media::Thumbnail>,
    },
    Spotify(spotify::Collection),
}

pub struct Probe {
    rx: mpsc::Receiver<Result<Probed, String>>,
    cancel: Arc<AtomicBool>,
}

impl Probe {
    pub fn poll(&self) -> Option<Result<Probed, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("The lookup stopped".into())),
        }
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn probe(
    link: &Link,
    env: &ytdlp::Env,
    tools: &std::path::Path,
    cancel: &AtomicBool,
) -> Result<Probed, String> {
    if link.site == Site::Spotify {
        let (kind, id) = source::spotify_item(&link.url).ok_or("Unsupported Spotify link")?;
        return spotify::resolve(kind, &id, cancel).map(Probed::Spotify);
    }
    let binary = ytdlp::ensure(tools, cancel)?;
    let output = crate::ai_sequence::process::run(
        &mut ytdlp::command(&binary, &ytdlp::probe_args(env, &link.url)),
        cancel,
        PROBE_TIMEOUT,
    )
    .map_err(|e| clean_error(&e))?;
    let info = ytdlp::parse_info(&output)?;
    let thumbnail = info
        .thumbnail
        .as_deref()
        .and_then(|url| load_thumbnail(url, cancel));
    Ok(Probed::Media { info, thumbnail })
}

/// Decoded by FFmpeg like any still: it needs an image extension.
fn load_thumbnail(url: &str, cancel: &AtomicBool) -> Option<vv_media::Thumbnail> {
    let bytes = ytdlp::fetch(url, 8 << 20, cancel).ok()?;
    let path_part = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path_part
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| vv_media::IMAGE_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or_else(|| "jpg".into());
    let dir = vv_media::cache_dir("download-thumbs");
    std::fs::create_dir_all(&dir).ok()?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let file = dir.join(format!("{}-{n}.{ext}", std::process::id()));
    std::fs::write(&file, bytes).ok()?;
    let thumbnail = vv_media::generate_thumbnail(&file, 0.0, 480).ok();
    let _ = std::fs::remove_file(&file);
    thumbnail
}

fn run_job(
    shared: &Shared,
    id: u64,
    request: &Request,
    env: &ytdlp::Env,
    dirs: &Dirs,
    cancel: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    let _slot = shared.acquire(cancel)?;
    shared.set_phase(id, Phase::Preparing);
    let binary = ytdlp::ensure(&dirs.tools, cancel)?;
    let url = match &request.spotify {
        Some(track) => find_on_youtube(&binary, env, track, cancel)?,
        None => request.url.clone(),
    };
    let out_dir = match request.destination {
        Destination::Project => &dirs.for_project,
        Destination::Music | Destination::Sfx => &dirs.for_library,
    };
    std::fs::create_dir_all(out_dir).map_err(|e| format!("Cannot create {out_dir:?}: {e}"))?;
    shared.set_phase(
        id,
        Phase::Downloading {
            fraction: None,
            speed: None,
            eta: None,
        },
    );
    let ffmpeg = ytdlp::ffmpeg_binary(env);
    let native_sections = request.range.is_some()
        && ffmpeg
            .as_deref()
            .is_some_and(|ffmpeg| ytdlp::ffmpeg_reads_https(ffmpeg, cancel));
    let mut files = Vec::new();
    let mut last_error = None;
    let args = ytdlp::download_args(env, request, &url, out_dir, native_sections);
    let command = ytdlp::command(&binary, &args);
    let outcome = ytdlp::run_streaming(command, cancel, |line| match ytdlp::parse_line(line) {
        Some(ytdlp::Line::Progress {
            fraction,
            speed,
            eta,
        }) => shared.set_phase(
            id,
            Phase::Downloading {
                fraction,
                speed,
                eta,
            },
        ),
        Some(ytdlp::Line::PostProcess(_)) => shared.set_phase(id, Phase::Processing),
        Some(ytdlp::Line::File(path)) => files.push(path),
        Some(ytdlp::Line::Error(message)) => last_error = Some(message),
        None => {}
    });
    let files = match outcome {
        Err(e) if e == CANCELLED => return Err(e),
        Err(e) => return Err(last_error.unwrap_or(e)),
        Ok(()) if files.is_empty() => {
            return Err(last_error.unwrap_or_else(|| "yt-dlp produced no file".into()));
        }
        Ok(()) => files,
    };
    match request.range {
        Some(range) if !native_sections => {
            shared.set_phase(id, Phase::Processing);
            let ffmpeg = ffmpeg.ok_or("ffmpeg is needed to cut a section")?;
            files
                .iter()
                .map(|file| cut(&ffmpeg, file, range, request.kind, cancel))
                .collect()
        }
        _ => Ok(files),
    }
}

/// Keeps only `range` of a whole download, replacing the file. Video is
/// re-encoded so the cut lands on the exact frame.
fn cut(
    ffmpeg: &std::path::Path,
    file: &std::path::Path,
    (start, end): (f64, f64),
    kind: MediaKind,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let (ext, codec): (String, &[&str]) = match kind {
        MediaKind::Audio => (
            file.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("m4a")
                .to_owned(),
            &["-c", "copy"],
        ),
        MediaKind::Video => (
            "mp4".to_owned(),
            &[
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
            ],
        ),
    };
    let partial = file.with_extension(format!("cut.{ext}"));
    let target = file.with_extension(&ext);
    let mut command = std::process::Command::new(ffmpeg);
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss"])
        .arg(format!("{start:.3}"))
        .arg("-i")
        .arg(file)
        .arg("-t")
        .arg(format!("{:.3}", end - start))
        .args(["-map", "0:v?", "-map", "0:a?"])
        .args(codec)
        .arg(&partial);
    let result = crate::ai_sequence::process::run(&mut command, cancel, CUT_TIMEOUT);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(if error == CANCELLED {
            error
        } else {
            format!("Cut failed: {error}")
        });
    }
    let _ = std::fs::remove_file(file);
    std::fs::rename(&partial, &target).map_err(|e| format!("Cannot save the cut: {e}"))?;
    Ok(target)
}

fn find_on_youtube(
    binary: &std::path::Path,
    env: &ytdlp::Env,
    track: &spotify::Track,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let output = crate::ai_sequence::process::run(
        &mut ytdlp::command(binary, &ytdlp::search_args(env, &track.search_query())),
        cancel,
        PROBE_TIMEOUT,
    )
    .map_err(|e| clean_error(&e))?;
    let candidates = ytdlp::parse_search(&output)?;
    let best = spotify::best_match(track, &candidates)
        .ok_or_else(|| format!("No match on YouTube for \"{}\"", track.label()))?;
    Ok(candidates[best].url.clone())
}

/// The `ERROR:` line of yt-dlp's stderr instead of the whole dump.
fn clean_error(error: &str) -> String {
    error
        .lines()
        .find_map(|line| line.trim().strip_prefix("ERROR:"))
        .map(|line| line.trim().to_owned())
        .unwrap_or_else(|| error.lines().next().unwrap_or(error).to_owned())
}

/// A system notification, for when the window is not in front.
pub fn notify_desktop(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    let command = {
        let mut c = std::process::Command::new("notify-send");
        c.args(["-a", "Tandem", "-i", "tandem", title, body]);
        Some(c)
    };
    #[cfg(target_os = "macos")]
    let command = {
        let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let mut c = std::process::Command::new("osascript");
        c.arg("-e").arg(format!(
            "display notification \"{}\" with title \"{}\"",
            quote(body),
            quote(title)
        ));
        Some(c)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let command: Option<std::process::Command> = {
        let _ = (title, body);
        None
    };
    if let Some(mut command) = command {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Ok(mut child) = command.spawn() {
            std::thread::spawn(move || child.wait());
        }
    }
}

#[cfg(test)]
#[path = "../tests/downloader/mod.rs"]
mod tests;
