//! Style packs: a look saved once and re-applied to any edit in one click.
//! A pack is versioned JSON and every component is optional: it applies only
//! what it holds, and the user picks which of those parts to use.

use std::ops::Range;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Clip, FrameIdx, Interpolation, Keyframed, auto_zoom};

pub const FORMAT: &str = "venturi-style-pack";
/// Bumped on a change an older reader would misread; `migrate` then gains
/// the step from the previous version.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylePack {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<SubtitleStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_zoom: Option<auto_zoom::Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animations: Option<Animations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<Music>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<TransitionStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro: Option<Bumper>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outro: Option<Bumper>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_zone: Option<SafeZone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleStyle {
    /// A `SubtitleTemplate` id or `text:<text template id>`.
    pub template: String,
}

/// In/Out of the clips: asset ids for video and photos, text preset ids
/// (`text_anim_presets`) for titles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animations {
    pub clip_in: Option<String>,
    pub clip_in_secs: f64,
    pub clip_out: Option<String>,
    pub clip_out_secs: f64,
    pub text_in: Option<String>,
    pub text_out: Option<String>,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            clip_in: None,
            clip_in_secs: 0.5,
            clip_out: None,
            clip_out_secs: 0.5,
            text_in: None,
            text_out: None,
        }
    }
}

/// Background music. Without a file the level and the ducking go to the
/// music already on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Music {
    pub file: Option<PathBuf>,
    /// Level of the music, dB.
    pub volume_db: f32,
    /// Extra reduction while someone speaks, dB (negative).
    pub ducking_db: f32,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            file: None,
            volume_db: -6.0,
            ducking_db: -12.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionStyle {
    /// A transition asset id.
    pub asset: String,
    pub seconds: f64,
}

/// An intro or outro title: a text template (it sets the length),
/// optionally with its own text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bumper {
    pub template: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafeZone {
    /// Id of a platform in `safe_zones.json`.
    pub platform: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Component {
    Subtitles,
    AutoZoom,
    Animations,
    Music,
    Transition,
    Intro,
    Outro,
    SafeZone,
}

impl Component {
    pub const ALL: [Self; 8] = [
        Self::Subtitles,
        Self::AutoZoom,
        Self::Animations,
        Self::Music,
        Self::Transition,
        Self::Intro,
        Self::Outro,
        Self::SafeZone,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Subtitles => "subtitles",
            Self::AutoZoom => "auto_zoom",
            Self::Animations => "animations",
            Self::Music => "music",
            Self::Transition => "transition",
            Self::Intro => "intro",
            Self::Outro => "outro",
            Self::SafeZone => "safe_zone",
        }
    }
}

impl StylePack {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            id: id.into(),
            name: name.into(),
            description: String::new(),
            subtitles: None,
            auto_zoom: None,
            animations: None,
            music: None,
            transition: None,
            intro: None,
            outro: None,
            safe_zone: None,
        }
    }

    pub fn has(&self, component: Component) -> bool {
        match component {
            Component::Subtitles => self.subtitles.is_some(),
            Component::AutoZoom => self.auto_zoom.is_some(),
            Component::Animations => self.animations.is_some(),
            Component::Music => self.music.is_some(),
            Component::Transition => self.transition.is_some(),
            Component::Intro => self.intro.is_some(),
            Component::Outro => self.outro.is_some(),
            Component::SafeZone => self.safe_zone.is_some(),
        }
    }

    pub fn components(&self) -> Vec<Component> {
        Component::ALL
            .into_iter()
            .filter(|c| self.has(*c))
            .collect()
    }

    pub fn to_json(&self) -> String {
        let mut pack = self.clone();
        pack.format = FORMAT.into();
        pack.version = VERSION;
        serde_json::to_string_pretty(&pack).expect("a pack always serializes")
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum PackError {
    #[error("not a style pack")]
    NotAPack,
    #[error("unreadable style pack: {0}")]
    Corrupt(String),
    #[error("style pack of format {found}, this version reads up to {supported}")]
    Newer { found: u32, supported: u32 },
}

/// Reads a pack of any known version, migrated to the current one.
pub fn parse(bytes: &[u8]) -> Result<StylePack, PackError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| PackError::Corrupt(e.to_string()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(PackError::NotAPack);
    }
    let found = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| PackError::Corrupt("version".into()))? as u32;
    if found > VERSION {
        return Err(PackError::Newer {
            found,
            supported: VERSION,
        });
    }
    let mut pack: StylePack = serde_json::from_value(migrate(value, found))
        .map_err(|e| PackError::Corrupt(e.to_string()))?;
    if pack.id.trim().is_empty() || pack.name.trim().is_empty() {
        return Err(PackError::Corrupt("id/name".into()));
    }
    pack.version = VERSION;
    Ok(pack)
}

/// Version 1 is the first: nothing to migrate yet. Each later version adds
/// a step here rewriting the JSON of the one before.
fn migrate(value: serde_json::Value, _from: u32) -> serde_json::Value {
    value
}

/// Seconds of a lead before the music dips, and of its return after speech.
pub const DUCK_ATTACK: f64 = 0.15;
pub const DUCK_RELEASE: f64 = 0.35;

/// Speech ranges (timeline seconds) merged when the music would not have
/// time to come back up between them.
pub fn speech_regions(ranges: impl IntoIterator<Item = Range<f64>>) -> Vec<Range<f64>> {
    let mut ranges: Vec<Range<f64>> = ranges.into_iter().filter(|r| r.end > r.start).collect();
    ranges.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut out: Vec<Range<f64>> = Vec::new();
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start - last.end < DUCK_ATTACK + DUCK_RELEASE => {
                last.end = last.end.max(r.end);
            }
            _ => out.push(r),
        }
    }
    out
}

/// Gain of a music clip at `volume_db`, dipping by `ducking_db` over the
/// speech `regions` (timeline seconds, from `speech_regions`).
pub fn ducking_gain(
    clip: &Clip,
    fps: f64,
    regions: &[Range<f64>],
    volume_db: f32,
    ducking_db: f32,
) -> Keyframed<f32> {
    let mut gain = Keyframed::constant(volume_db);
    let first = clip.timeline_start;
    let last = clip.timeline_end() - 1;
    let ducked = volume_db + ducking_db.min(0.0);
    let frame = |secs: f64| ((secs * fps).round() as FrameIdx).clamp(first, last.max(first));
    let mut put = |secs: f64, value: f32| {
        gain.upsert(
            clip.source_frame_at(frame(secs)),
            value,
            Interpolation::Linear,
        );
    };
    let start_secs = first as f64 / fps;
    let end_secs = clip.timeline_end() as f64 / fps;
    for r in regions {
        if r.end <= start_secs || r.start >= end_secs {
            continue;
        }
        if r.start - DUCK_ATTACK > start_secs {
            put(r.start - DUCK_ATTACK, volume_db);
            put(r.start, ducked);
        } else {
            put(start_secs, ducked);
        }
        if r.end + DUCK_RELEASE < end_secs {
            put(r.end, ducked);
            put(r.end + DUCK_RELEASE, volume_db);
        } else {
            put(end_secs, ducked);
        }
    }
    gain
}

#[cfg(test)]
#[path = "tests/style_pack.rs"]
mod tests;
