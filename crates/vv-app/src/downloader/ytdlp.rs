//! yt-dlp: the managed binary, its command lines and its output.

use super::{AudioFormat, MediaKind, Request};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, SystemTime};

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
/// Sites change often and old releases stop working within weeks.
const REFRESH_AFTER: Duration = Duration::from_secs(5 * 24 * 3600);
const PROGRESS: &str = "VVDL";
const POSTPROCESS: &str = "VVPP";
const FILE: &str = "VVFILE";

static INSTALL: Mutex<()> = Mutex::new(());

fn asset_name() -> Result<&'static str, String> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "yt-dlp_linux",
        ("linux", "aarch64") => "yt-dlp_linux_aarch64",
        ("macos", _) => "yt-dlp_macos",
        ("windows", "x86_64") => "yt-dlp.exe",
        ("windows", "aarch64") => "yt-dlp_arm64.exe",
        (os, arch) => return Err(format!("yt-dlp has no build for {os}/{arch}")),
    })
}

/// The managed binary, downloaded (and checked against the release's
/// SHA-256 list) on first use and refreshed when it gets old. A failed
/// refresh keeps the binary already there.
pub fn ensure(tools: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let _guard = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let binary = tools.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    let age = std::fs::metadata(&binary)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok());
    match age {
        Some(age) if age < REFRESH_AFTER => Ok(binary),
        Some(_) => {
            if let Err(e) = install(tools, &binary, cancel) {
                eprintln!("yt-dlp refresh failed, keeping the old one: {e}");
            }
            Ok(binary)
        }
        None => install(tools, &binary, cancel).map(|()| binary),
    }
}

fn install(tools: &Path, binary: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let asset = asset_name()?;
    let sums = String::from_utf8(fetch(&format!("{RELEASES}/SHA2-256SUMS"), 1 << 20, cancel)?)
        .map_err(|_| "Invalid yt-dlp checksum list")?;
    let expected = checksum_for(&sums, asset).ok_or("yt-dlp checksum not found")?;
    let bytes = fetch(&format!("{RELEASES}/{asset}"), 200 << 20, cancel)?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let actual: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        return Err("The downloaded yt-dlp failed its checksum".into());
    }
    std::fs::create_dir_all(tools).map_err(|e| format!("Cannot create {tools:?}: {e}"))?;
    let partial = tools.join("yt-dlp.partial");
    std::fs::write(&partial, &bytes).map_err(|e| format!("Cannot save yt-dlp: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&partial, binary).map_err(|e| format!("Cannot install yt-dlp: {e}"))
}

pub(super) fn checksum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == asset).then(|| hash.to_ascii_lowercase())
    })
}

pub(super) fn fetch(url: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| format!("Download failed ({url}): {e}"))?;
    let mut reader = response.body_mut().with_config().limit(limit).reader();
    let mut bytes = Vec::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|e| format!("Download failed ({url}): {e}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn ffmpeg_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// The folder of the ffmpeg and ffprobe shipped with the app; without it
/// yt-dlp looks on PATH.
pub fn bundled_ffmpeg_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.to_path_buf()))
        .filter(|dir| dir.join(ffmpeg_name()).is_file())
}

pub fn ffmpeg_binary(env: &Env) -> Option<PathBuf> {
    match &env.ffmpeg {
        Some(dir) => Some(dir.join(ffmpeg_name())),
        None => on_path(ffmpeg_name()),
    }
}

/// yt-dlp downloads a section by handing the stream URL to ffmpeg; the
/// bundled one has no TLS, so it gets the whole file and the cut is made
/// locally instead.
pub fn ffmpeg_reads_https(ffmpeg: &Path, cancel: &AtomicBool) -> bool {
    crate::ai_sequence::process::run(
        Command::new(ffmpeg).args(["-hide_banner", "-protocols"]),
        cancel,
        Duration::from_secs(20),
    )
    .is_ok_and(|out| reads_https(&out))
}

pub(super) fn reads_https(protocols: &str) -> bool {
    protocols
        .split("Output:")
        .next()
        .is_some_and(|input| input.lines().any(|l| l.trim() == "https"))
}

pub fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// What every invocation shares. YouTube needs a JavaScript runtime for its
/// player challenges: yt-dlp finds Deno by itself, Node must be named.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// Folder holding ffmpeg and ffprobe.
    pub ffmpeg: Option<PathBuf>,
    pub node: Option<PathBuf>,
    pub cookies_browser: Option<String>,
}

impl Env {
    fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["--ignore-config", "--no-warnings", "--color", "never"]
            .map(String::from)
            .to_vec();
        if let Some(ffmpeg) = &self.ffmpeg {
            args.push("--ffmpeg-location".into());
            args.push(ffmpeg.display().to_string());
        }
        if let Some(node) = &self.node {
            args.push("--js-runtimes".into());
            args.push(format!("node:{}", node.display()));
        }
        if let Some(browser) = self.cookies_browser.as_deref().filter(|b| !b.is_empty()) {
            args.push("--cookies-from-browser".into());
            args.push(browser.into());
        }
        args
    }
}

const INFO_FIELDS: &str = "%(.{id,title,uploader,channel,duration,thumbnail,chapters,heatmap,webpage_url,extractor_key})j";

pub fn probe_args(env: &Env, url: &str) -> Vec<String> {
    let mut args = env.args();
    args.extend(["--no-playlist", "--print", INFO_FIELDS, "--", url].map(String::from));
    args
}

pub fn search_args(env: &Env, query: &str) -> Vec<String> {
    let mut args = env.args();
    args.extend(["--flat-playlist", "-J", "--", query].map(String::from));
    args
}

/// `native_sections`: whether yt-dlp itself fetches only `request.range`.
pub fn download_args(
    env: &Env,
    request: &Request,
    url: &str,
    out_dir: &Path,
    native_sections: bool,
) -> Vec<String> {
    let mut args = env.args();
    let push = |args: &mut Vec<String>, items: &[&str]| {
        args.extend(items.iter().map(|s| (*s).to_owned()));
    };
    push(
        &mut args,
        &[
            "--no-playlist",
            "--newline",
            "--progress",
            "--force-overwrites",
            "--progress-template",
            &format!(
                "download:{PROGRESS} %(progress.downloaded_bytes)s %(progress.total_bytes)s \
                 %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s"
            ),
            "--progress-template",
            &format!("postprocess:{POSTPROCESS} %(progress.postprocessor)s"),
            "--print",
            &format!("after_move:{FILE} %(filepath)s"),
        ],
    );
    let section = request
        .range
        .map(|(start, end)| format!(" {}-{}", file_time(start), file_time(end)))
        .unwrap_or_default();
    args.push("-P".into());
    args.push(out_dir.display().to_string());
    args.push("-o".into());
    args.push(format!("%(title).80B{section} [%(id)s].%(ext)s"));
    match request.kind {
        MediaKind::Audio => {
            push(&mut args, &["-f", "bestaudio/best", "-x"]);
            let format = match request.audio_format {
                AudioFormat::Original => None,
                AudioFormat::Wav => Some("wav"),
                AudioFormat::Flac => Some("flac"),
                AudioFormat::Mp3 => Some("mp3"),
            };
            if let Some(format) = format {
                push(
                    &mut args,
                    &["--audio-format", format, "--audio-quality", "0"],
                );
            }
        }
        MediaKind::Video => {
            // vp9, not av1, as the fallback: FFmpeg here decodes AV1 only
            // in hardware.
            let codec = if request.prefer_h264 { "h264" } else { "vp9" };
            push(
                &mut args,
                &[
                    "-f",
                    "bv*+ba/b",
                    "-S",
                    &format!("res:{},vcodec:{codec},acodec:aac", request.max_height),
                    "--merge-output-format",
                    "mp4/mkv",
                ],
            );
        }
    }
    if let Some((start, end)) = request.range.filter(|_| native_sections) {
        args.push("--download-sections".into());
        args.push(format!("*{start:.3}-{end:.3}"));
        if request.kind == MediaKind::Video {
            args.push("--force-keyframes-at-cuts".into());
        }
    }
    args.push("--".into());
    args.push(url.into());
    args
}

fn file_time(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}m{:02}s", total / 60, total % 60)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Progress {
        fraction: Option<f32>,
        speed: Option<f64>,
        eta: Option<f64>,
    },
    PostProcess(String),
    File(PathBuf),
    Error(String),
}

pub fn parse_line(line: &str) -> Option<Line> {
    let line = line.trim();
    let number = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok());
    if let Some(rest) = line.strip_prefix(PROGRESS) {
        let mut fields = rest.split_whitespace();
        let done = number(fields.next());
        let total = number(fields.next());
        let estimate = number(fields.next());
        let speed = number(fields.next());
        let eta = number(fields.next());
        let fraction = done
            .zip(total.or(estimate).filter(|t| *t > 0.0))
            .map(|(done, total)| (done / total).clamp(0.0, 1.0) as f32);
        return Some(Line::Progress {
            fraction,
            speed,
            eta,
        });
    }
    if let Some(rest) = line.strip_prefix(POSTPROCESS) {
        return Some(Line::PostProcess(rest.trim().to_owned()));
    }
    if let Some(rest) = line.strip_prefix(FILE) {
        let path = rest.trim();
        return (!path.is_empty()).then(|| Line::File(PathBuf::from(path)));
    }
    line.strip_prefix("ERROR:")
        .map(|message| Line::Error(message.trim().to_owned()))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chapter {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub title: String,
    pub uploader: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub chapters: Vec<Chapter>,
    /// YouTube's "most replayed" curve: (start, end, 0..=1).
    pub heatmap: Vec<(f64, f64, f32)>,
    /// More than one video behind the link (an Instagram carousel).
    pub items: usize,
}

/// One JSON object per line, one line per video.
pub fn parse_info(output: &str) -> Result<Info, String> {
    let mut lines = output.lines().filter(|l| l.trim_start().starts_with('{'));
    let first = lines.next().ok_or("yt-dlp returned no information")?;
    let items = 1 + lines.count();
    let value: serde_json::Value =
        serde_json::from_str(first).map_err(|e| format!("yt-dlp output: {e}"))?;
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
    let uploader = Some(text("uploader"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text("channel"));
    let chapters = value["chapters"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    Some(Chapter {
                        start: c["start_time"].as_f64()?,
                        end: c["end_time"].as_f64()?,
                        title: c["title"].as_str().unwrap_or_default().to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let heatmap = value["heatmap"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|h| {
                    Some((
                        h["start_time"].as_f64()?,
                        h["end_time"].as_f64()?,
                        h["value"].as_f64()? as f32,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Info {
        title: text("title"),
        uploader,
        duration: value["duration"].as_f64().filter(|d| *d > 0.0),
        thumbnail: value["thumbnail"].as_str().map(str::to_owned),
        chapters,
        heatmap,
        items,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub channel: String,
    pub duration: Option<f64>,
}

pub fn parse_search(output: &str) -> Result<Vec<Candidate>, String> {
    let value: serde_json::Value =
        serde_json::from_str(output.trim()).map_err(|e| format!("yt-dlp search: {e}"))?;
    Ok(value["entries"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| {
                    let url = e["url"].as_str().map(str::to_owned).or_else(|| {
                        e["id"]
                            .as_str()
                            .map(|id| format!("https://www.youtube.com/watch?v={id}"))
                    })?;
                    Some(Candidate {
                        url,
                        title: e["title"].as_str().unwrap_or_default().to_owned(),
                        channel: e["channel"]
                            .as_str()
                            .or(e["uploader"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        duration: e["duration"].as_f64(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn command(binary: &Path, args: &[String]) -> Command {
    let mut command = Command::new(binary);
    command.args(args);
    // yt-dlp writes file names in the console's encoding otherwise.
    command.env("PYTHONIOENCODING", "utf-8");
    command
}

/// Runs a download, handing every output line (stdout and stderr) to
/// `on_line`. Cancelling kills yt-dlp and the ffmpeg it started.
pub fn run_streaming(
    mut command: Command,
    cancel: &AtomicBool,
    mut on_line: impl FnMut(&str),
) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    crate::spawn::no_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start yt-dlp: {e}"))?;
    #[cfg(windows)]
    let tree = crate::ai_sequence::process::ProcessTree::new(&child);
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let tx = tx.clone();
        std::thread::spawn(move || split_lines(stream, |line| tx.send(line).is_ok()))
    })
    .collect();
    drop(tx);
    let mut cancelled = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(&line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
    }
    #[cfg(unix)]
    if cancelled {
        // SAFETY: signals the process group created above.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    drop(tree);
    if cancelled {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    for reader in readers {
        let _ = reader.join();
    }
    if cancelled {
        return Err("Cancelled".into());
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("yt-dlp exited with {status}"))
    }
}

/// yt-dlp redraws progress with `\r` unless `--newline` took effect.
fn split_lines(mut stream: impl Read, mut emit: impl FnMut(String) -> bool) {
    let mut pending = Vec::new();
    let mut buffer = [0u8; 8192];
    while let Ok(count) = stream.read(&mut buffer) {
        if count == 0 {
            break;
        }
        for &byte in &buffer[..count] {
            if byte == b'\n' || byte == b'\r' {
                if !pending.is_empty() {
                    let line = String::from_utf8_lossy(&pending).into_owned();
                    pending.clear();
                    if !emit(line) {
                        return;
                    }
                }
            } else {
                pending.push(byte);
            }
        }
    }
    if !pending.is_empty() {
        emit(String::from_utf8_lossy(&pending).into_owned());
    }
}

#[cfg(test)]
#[path = "../tests/downloader/ytdlp.rs"]
mod tests;
