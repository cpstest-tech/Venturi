//! What the export modal decides, without any UI: sizes, bitrates, the
//! estimate and the file name.

use std::path::{Path, PathBuf};

use vv_core::{ClipSource, MAX_COMPOUND_DEPTH, MediaId, Project, Rational, TimelineId, TrackKind};

/// Audio bitrate the exports use, and what the size estimate counts.
pub const AUDIO_KBPS: u32 = vv_media::encode::DEFAULT_AUDIO_BITRATE_KBPS;
const AUDIO_MBPS: f64 = AUDIO_KBPS as f64 / 1000.0;
/// Uncompressed 16-bit stereo at 48 kHz, in Mbps.
const PCM_MBPS: f64 = 1.536;
const FLAC_RATIO: f64 = 0.6;
const MAX_MBPS: f64 = 500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolutionChoice {
    P480,
    P720,
    P1080,
    P2K,
    P4K,
    MatchBest,
}

impl ResolutionChoice {
    pub const ALL: [Self; 6] = [
        Self::P480,
        Self::P720,
        Self::P1080,
        Self::P2K,
        Self::P4K,
        Self::MatchBest,
    ];

    /// Short side in pixels; `None` for "match the best video".
    pub fn short_side(self) -> Option<u32> {
        match self {
            Self::P480 => Some(480),
            Self::P720 => Some(720),
            Self::P1080 => Some(1080),
            Self::P2K => Some(1440),
            Self::P4K => Some(2160),
            Self::MatchBest => None,
        }
    }
}

/// The preset closest to the project's short side; 1080p without media.
pub fn default_resolution(project_short_side: u32, has_media: bool) -> ResolutionChoice {
    if !has_media {
        return ResolutionChoice::P1080;
    }
    ResolutionChoice::ALL
        .into_iter()
        .filter_map(|c| c.short_side().map(|s| (c, s)))
        .min_by_key(|&(_, short)| short.abs_diff(project_short_side))
        .map_or(ResolutionChoice::P1080, |(choice, _)| choice)
}

/// The short side `choice` asks for.
pub fn target_short_side(
    choice: ResolutionChoice,
    project_short_side: u32,
    best: Option<u32>,
) -> u32 {
    choice.short_side().or(best).unwrap_or(project_short_side)
}

/// The timeline's proportions with `short_side` as the shorter one, even.
pub fn scaled_size(timeline_size: (u32, u32), short_side: u32) -> (u32, u32) {
    let (w, h) = timeline_size;
    let short = w.min(h).max(1);
    if short == short_side {
        return timeline_size;
    }
    let scale = |value: u32| {
        let scaled =
            (u64::from(value) * u64::from(short_side) + u64::from(short) / 2) / u64::from(short);
        ((scaled as u32) & !1).max(2)
    };
    (scale(w), scale(h))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BitrateChoice {
    MatchBest,
    Recommended,
    Lower,
    Higher,
    Custom,
}

impl BitrateChoice {
    pub const ALL: [Self; 5] = [
        Self::MatchBest,
        Self::Recommended,
        Self::Lower,
        Self::Higher,
        Self::Custom,
    ];
}

/// H.264 needs about a tenth of a bit per pixel; the newer codecs less.
pub fn recommended_mbps(size: (u32, u32), fps: f64, codec_factor: f64) -> f64 {
    let pixels_per_second = f64::from(size.0) * f64::from(size.1) * fps;
    round_tenth((pixels_per_second * 0.1 * codec_factor / 1e6).clamp(1.0, MAX_MBPS))
}

fn round_tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// The bitrate of `choice`; `detected` is the highest of the clips.
pub fn choose_mbps(
    choice: BitrateChoice,
    recommended: f64,
    detected: Option<f64>,
    custom: f64,
) -> f64 {
    match choice {
        BitrateChoice::MatchBest => detected.unwrap_or(recommended),
        BitrateChoice::Recommended => recommended,
        BitrateChoice::Lower => round_tenth(recommended * 0.7).max(0.5),
        BitrateChoice::Higher => round_tenth(recommended * 1.5),
        BitrateChoice::Custom => custom,
    }
    .clamp(0.1, MAX_MBPS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodecChoice {
    H264,
    Hevc,
    Av1,
}

impl CodecChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "HEVC (H.265)",
            Self::Av1 => "AV1",
        }
    }

    /// Bits per pixel relative to H.264.
    pub fn factor(self) -> f64 {
        match self {
            Self::H264 => 1.0,
            Self::Hevc => 0.7,
            Self::Av1 => 0.55,
        }
    }

    /// The engine encodes H.264 only.
    pub fn available(self) -> bool {
        self == Self::H264
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    Mp4,
    Mov,
}

impl Container {
    pub const ALL: [Self; 2] = [Self::Mp4, Self::Mov];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
        }
    }
}

/// Whether `container` can hold `codec`.
pub fn valid_combination(codec: CodecChoice, container: Container) -> bool {
    !(codec == CodecChoice::Av1 && container == Container::Mov)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FpsChoice {
    F24,
    F25,
    F30,
    F50,
    F60,
    F120,
    MatchBest,
}

impl FpsChoice {
    pub const ALL: [Self; 7] = [
        Self::F24,
        Self::F25,
        Self::F30,
        Self::F50,
        Self::F60,
        Self::F120,
        Self::MatchBest,
    ];

    pub fn fixed(self) -> Option<Rational> {
        let fps = match self {
            Self::F24 => 24,
            Self::F25 => 25,
            Self::F30 => 30,
            Self::F50 => 50,
            Self::F60 => 60,
            Self::F120 => 120,
            Self::MatchBest => return None,
        };
        Some(Rational::new(fps, 1))
    }
}

/// The frame rate `choice` asks for.
pub fn target_fps(choice: FpsChoice, timeline: Rational, best: Option<Rational>) -> Rational {
    choice.fixed().or(best).unwrap_or(timeline)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionFormat {
    Srt,
    Txt,
}

impl CaptionFormat {
    pub const ALL: [Self; 2] = [Self::Srt, Self::Txt];

    pub fn label(self) -> &'static str {
        match self {
            Self::Srt => "SRT",
            Self::Txt => "TXT",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Txt => "txt",
        }
    }
}

/// The media of the active video clips of `timeline`: not photos, not on a
/// hidden track, not disabled; compound clips count through their content.
pub fn active_video_media(project: &Project, timeline: TimelineId) -> Vec<MediaId> {
    let mut found = Vec::new();
    collect_video_media(project, timeline, 0, &mut found);
    found
}

fn collect_video_media(
    project: &Project,
    timeline: TimelineId,
    depth: u32,
    found: &mut Vec<MediaId>,
) {
    let Some(timeline) = project.timelines.get(timeline) else {
        return;
    };
    if depth >= MAX_COMPOUND_DEPTH {
        return;
    }
    for track in timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video && !t.muted)
    {
        for clip in track.clips.iter().filter(|c| !c.disabled) {
            let ClipSource::Media(id) = clip.source else {
                continue;
            };
            let Some(item) = project.media_pool.get(id) else {
                continue;
            };
            if let Some(nested) = item.compound {
                collect_video_media(project, nested, depth + 1, found);
            } else if item.meta.has_video && !item.meta.is_image() && !found.contains(&id) {
                found.push(id);
            }
        }
    }
}

/// Short side of the largest of `media`.
pub fn best_short_side(project: &Project, media: &[MediaId]) -> Option<u32> {
    media
        .iter()
        .filter_map(|&id| project.media_pool.get(id))
        .map(|item| item.meta.width.min(item.meta.height))
        .max()
        .filter(|&side| side > 0)
}

/// The highest frame rate of `media`.
pub fn best_fps(project: &Project, media: &[MediaId]) -> Option<Rational> {
    media
        .iter()
        .filter_map(|&id| project.media_pool.get(id))
        .map(|item| item.meta.fps)
        .filter(|fps| fps.num > 0 && fps.den > 0)
        .max_by(|a, b| a.as_f64().total_cmp(&b.as_f64()))
}

/// kbps from `ffprobe -show_entries stream=bit_rate:format=bit_rate`: the
/// video stream's, else the container's.
pub fn parse_bitrate_kbps(output: &str) -> Option<u32> {
    let mut values = output
        .lines()
        .map(|line| line.trim().parse::<f64>().ok().filter(|v| *v > 0.0));
    values
        .by_ref()
        .flatten()
        .next()
        .map(|bits| (bits / 1000.0).round() as u32)
}

pub fn probe_bitrate_kbps(path: &Path) -> Option<u32> {
    let output = crate::spawn::no_console(&mut std::process::Command::new("ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=bit_rate:format=bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_bitrate_kbps(&String::from_utf8_lossy(&output.stdout)))
        .flatten()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Wav,
    Aac,
    Flac,
}

impl AudioFormat {
    pub fn engine(self) -> vv_media::AudioFileFormat {
        match self {
            Self::Mp3 => vv_media::AudioFileFormat::Mp3,
            Self::Wav => vv_media::AudioFileFormat::Wav,
            Self::Aac => vv_media::AudioFileFormat::Aac,
            Self::Flac => vv_media::AudioFileFormat::Flac,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Wav => "WAV",
            Self::Aac => "AAC",
            Self::Flac => "FLAC",
        }
    }

    fn mbps(self) -> f64 {
        match self {
            Self::Mp3 | Self::Aac => AUDIO_MBPS,
            Self::Wav => PCM_MBPS,
            Self::Flac => PCM_MBPS * FLAC_RATIO,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Estimate {
    pub video_mbps: f64,
    pub has_video: bool,
    /// Audio muxed into the video file.
    pub video_has_audio: bool,
    pub audio_file: Option<AudioFormat>,
    pub seconds: f64,
}

impl Estimate {
    /// (bitrate of the video in Mbps + that of the audio) × seconds ÷ 8.
    pub fn megabytes(&self) -> f64 {
        let mbps = if self.has_video {
            self.video_mbps
                + if self.video_has_audio {
                    AUDIO_MBPS
                } else {
                    0.0
                }
        } else {
            self.audio_file.map_or(0.0, AudioFormat::mbps)
        };
        mbps * self.seconds / 8.0
    }
}

pub fn format_duration(seconds: f64) -> String {
    let total = seconds.round().max(0.0) as u64;
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        format!(
            "{}h {:02}m {:02}s",
            total / 3600,
            total / 60 % 60,
            total % 60
        )
    }
}

/// Decimal megabytes, as file managers show them; a tenth below 10.
pub fn format_megabytes(megabytes: f64) -> String {
    if megabytes >= 1000.0 {
        format!("{:.1} GB", megabytes / 1000.0)
    } else if megabytes >= 10.0 {
        format!("{megabytes:.0} MB")
    } else {
        format!("{megabytes:.1} MB")
    }
}

pub fn format_mbps(mbps: f64) -> String {
    format!("{mbps:.1} Mbps")
}

/// `base`, or `base_1`, `base_2`... if `taken` says the file exists.
pub fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (1..)
        .map(|n| format!("{base}_{n}"))
        .find(|name| !taken(name))
        .expect("an unbounded range")
}

/// A file name from user text: no separators.
pub fn sanitize_name(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| {
            if std::path::is_separator(c) || c == '\0' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `~/Videos/Exports`.
pub fn default_export_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map_or_else(PathBuf::new, |home| {
            PathBuf::from(home).join("Videos").join("Exports")
        })
}

#[cfg(test)]
#[path = "tests/export_plan.rs"]
mod tests;
