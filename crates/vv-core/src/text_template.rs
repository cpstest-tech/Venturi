//! Text templates: a versioned JSON description of one or more text layers
//! (style, animations, sample text) with a category, tags and a default
//! duration. Measures are written for a 1080-pixel-high frame and scaled to
//! the timeline; colours are `#RRGGBB` / `#RRGGBBAA`.

use crate::model::{
    FontCase, HAnchor, Keyframed, Rgba, TextAlign, TitleBackground, TitleParams, VAnchor,
};
use crate::text_anim::{AnimDescriptor, Granularity, Karaoke, TextAnimations};
use crate::text_anim_presets;
use crate::text_style::{
    BackgroundShape, Gradient, GradientKind, GradientStop, StrokeJoin, TextExtrude, TextFill,
    TextGlow, TextShadow, TextStroke, TextStyle,
};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
const REFERENCE_HEIGHT: f32 = 1080.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCategory {
    Titles,
    Social,
    LowerThird,
    Karaoke,
    Retro,
    Elegant,
}

impl TemplateCategory {
    pub const ALL: [TemplateCategory; 6] = [
        Self::Titles,
        Self::Social,
        Self::LowerThird,
        Self::Karaoke,
        Self::Retro,
        Self::Elegant,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Titles => "titles",
            Self::Social => "social",
            Self::LowerThird => "lower_third",
            Self::Karaoke => "karaoke",
            Self::Retro => "retro",
            Self::Elegant => "elegant",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("invalid template JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("template format {0} is newer than this version understands")]
    Newer(u32),
    #[error("bad colour {0:?}")]
    Colour(String),
    #[error("unknown animation preset {0:?}")]
    Preset(String),
    #[error("template has no layers")]
    NoLayers,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextTemplateDef {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub category: TemplateCategory,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Seconds.
    pub duration: f32,
    pub layers: Vec<LayerDef>,
    /// How it styles subtitles; absent = lines.
    #[serde(default)]
    pub subtitle: SubtitleDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleDef {
    /// One clip per line or per word.
    #[serde(default)]
    pub word_by_word: bool,
    /// Of the frame height.
    #[serde(default = "default_subtitle_size")]
    pub size: f32,
}

fn default_subtitle_size() -> f32 {
    0.06
}

impl Default for SubtitleDef {
    fn default() -> Self {
        Self {
            word_by_word: false,
            size: default_subtitle_size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerDef {
    #[serde(default)]
    pub underlays: Vec<LayerDef>,
    #[serde(default)]
    pub sticker: Option<crate::TextSticker>,
    pub text: String,
    #[serde(default)]
    pub font: String,
    #[serde(default = "default_weight")]
    pub weight: u16,
    #[serde(default)]
    pub italic: bool,
    pub size: f32,
    pub color: String,
    #[serde(default)]
    pub tracking: f32,
    #[serde(default)]
    pub line_spacing: f32,
    #[serde(default)]
    pub case: CaseDef,
    #[serde(default)]
    pub align: AlignDef,
    /// From the centre of the frame, Y upwards.
    #[serde(default)]
    pub position: [f32; 2],
    /// Which point of the text block falls on `position`.
    #[serde(default)]
    pub anchor: AnchorDef,
    #[serde(default)]
    pub fill: Option<GradientDef>,
    #[serde(default)]
    pub strokes: Vec<StrokeDef>,
    #[serde(default)]
    pub shadows: Vec<ShadowDef>,
    #[serde(default)]
    pub glows: Vec<GlowDef>,
    #[serde(default)]
    pub extrude: Option<ExtrudeDef>,
    #[serde(default)]
    pub background: Option<BackgroundDef>,
    #[serde(default)]
    pub anim: AnimDef,
    /// Seconds after the template start.
    #[serde(default)]
    pub start: f32,
    /// Seconds; absent = until the end of the template.
    #[serde(default)]
    pub length: Option<f32>,
}

fn default_weight() -> u16 {
    400
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseDef {
    #[default]
    Mixed,
    Upper,
    Lower,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignDef {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AnchorDef {
    #[serde(default)]
    pub h: HDef,
    #[serde(default)]
    pub v: VDef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HDef {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VDef {
    Top,
    #[default]
    Middle,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientDef {
    #[serde(default)]
    pub radial: bool,
    /// Degrees, 0 = left to right.
    #[serde(default = "default_angle")]
    pub angle: f32,
    /// `[position, colour]`, sorted by position.
    pub stops: Vec<(f32, String)>,
}

fn default_angle() -> f32 {
    90.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeDef {
    pub color: String,
    pub width: f32,
    #[serde(default)]
    pub join: JoinDef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinDef {
    #[default]
    Round,
    Miter,
    Bevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowDef {
    pub color: String,
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub blur: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 {
    100.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlowDef {
    pub color: String,
    pub radius: f32,
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

fn default_intensity() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtrudeDef {
    pub color: String,
    pub depth: f32,
    #[serde(default = "default_extrude_angle")]
    pub angle: f32,
}

fn default_extrude_angle() -> f32 {
    -45.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundDef {
    #[serde(default)]
    pub pill: bool,
    pub color: String,
    #[serde(default)]
    pub gradient: Option<GradientDef>,
    /// In font sizes.
    #[serde(default = "default_padding")]
    pub padding: f32,
    /// Fraction of the shorter side, up to 0.5.
    #[serde(default = "default_corner")]
    pub corner_radius: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub outline_color: Option<String>,
    #[serde(default)]
    pub outline_width: f32,
    #[serde(default)]
    pub rotation: f32,
}

fn default_padding() -> f32 {
    0.25
}

fn default_corner() -> f32 {
    0.2
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnimDef {
    #[serde(default)]
    pub enter_custom: Option<AnimDescriptor>,
    #[serde(default, rename = "in")]
    pub enter: Option<AnimRef>,
    #[serde(default, rename = "out")]
    pub exit: Option<AnimRef>,
    #[serde(default, rename = "loop")]
    pub looped: Option<AnimRef>,
    #[serde(default)]
    pub karaoke: Option<KaraokeDef>,
}

/// A built-in preset with optional overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimRef {
    pub preset: String,
    #[serde(default)]
    pub duration: Option<f32>,
    #[serde(default)]
    pub stagger: Option<f32>,
    #[serde(default)]
    pub granularity: Option<GranularityDef>,
    #[serde(default)]
    pub tint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GranularityDef {
    Block,
    Line,
    Word,
    Char,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KaraokeDef {
    pub color: String,
    #[serde(default = "default_karaoke_scale")]
    pub scale: f32,
    #[serde(default)]
    pub keep_past: bool,
}

fn default_karaoke_scale() -> f32 {
    1.12
}

/// One text clip a template makes.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateClip {
    pub title: TitleParams,
    pub start_secs: f32,
    pub length_secs: f32,
}

pub fn parse_color(text: &str) -> Result<Rgba, TemplateError> {
    let bad = || TemplateError::Colour(text.to_owned());
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return Err(bad());
    }
    let byte = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .map(|v| f32::from(v) / 255.0)
            .map_err(|_| bad())
    };
    Ok(Rgba {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a: if hex.len() == 8 { byte(6)? } else { 1.0 },
    })
}

impl TextTemplateDef {
    pub fn from_json(json: &str) -> Result<Self, TemplateError> {
        let def: Self = serde_json::from_str(json)?;
        if def.version > FORMAT_VERSION {
            return Err(TemplateError::Newer(def.version));
        }
        if def.layers.is_empty() {
            return Err(TemplateError::NoLayers);
        }
        def.layers.iter().try_for_each(LayerDef::validate)?;
        Ok(def)
    }

    pub fn to_json(&self) -> Result<String, TemplateError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn is_karaoke(&self) -> bool {
        self.layers.iter().any(|l| l.anim.karaoke.is_some())
    }

    /// The clips for a timeline of `resolution`, bottom layer first.
    pub fn clips(&self, resolution: (u32, u32)) -> Vec<TemplateClip> {
        let scale = resolution.1 as f32 / REFERENCE_HEIGHT;
        self.layers
            .iter()
            .filter_map(|layer| {
                let title = layer.title(scale).ok()?;
                let length = layer.length.unwrap_or(self.duration - layer.start).max(0.1);
                Some(TemplateClip {
                    title,
                    start_secs: layer.start.max(0.0),
                    length_secs: length,
                })
            })
            .collect()
    }

    /// The first layer's style carrying `text`, placed as a subtitle.
    pub fn subtitle_title(&self, text: &str, resolution: (u32, u32)) -> TitleParams {
        let height = resolution.1 as f32;
        let mut title = self
            .layers
            .first()
            .and_then(|l| {
                let scale = height * self.subtitle.size / l.size.max(1.0);
                l.title(scale).ok()
            })
            .unwrap_or_default();
        title.content = Keyframed::constant(text.to_owned());
        title.anchor = (HAnchor::Center, VAnchor::Bottom);
        title.position = [0.0, -height * 0.4];
        title.align = TextAlign::Center;
        title.background.center = [0.0, 0.0];
        title
    }
}

impl LayerDef {
    fn validate(&self) -> Result<(), TemplateError> {
        self.title(1.0).map(|_| ())
    }

    /// Title for a frame `scale` times the 1080-pixel reference.
    pub fn title(&self, scale: f32) -> Result<TitleParams, TemplateError> {
        let gradient = |g: &GradientDef| -> Result<Gradient, TemplateError> {
            let stops = g
                .stops
                .iter()
                .map(|(pos, color)| {
                    Ok(GradientStop {
                        pos: *pos,
                        color: parse_color(color)?,
                    })
                })
                .collect::<Result<Vec<_>, TemplateError>>()?;
            Ok(Gradient {
                kind: if g.radial {
                    GradientKind::Radial
                } else {
                    GradientKind::Linear
                },
                stops,
                angle: g.angle,
                offset: Keyframed::constant(0.0),
            })
        };
        let mut style = TextStyle::default();
        style.sticker = self.sticker.clone();
        for layer in &self.underlays {
            let underlay = layer.title(scale)?;
            style.underlays.push(crate::TextUnderlay {
                color: underlay.color,
                offset: [
                    underlay.position[0] - self.position[0] * scale,
                    underlay.position[1] - self.position[1] * scale,
                ],
                style: underlay.style,
            });
        }
        if let Some(g) = &self.fill {
            style.fill = TextFill::Gradient(gradient(g)?);
        }
        for s in &self.strokes {
            style.strokes.push(TextStroke {
                color: parse_color(&s.color)?,
                width: s.width * scale,
                join: match s.join {
                    JoinDef::Round => StrokeJoin::Round,
                    JoinDef::Miter => StrokeJoin::Miter,
                    JoinDef::Bevel => StrokeJoin::Bevel,
                },
            });
        }
        for s in &self.shadows {
            style.shadows.push(TextShadow {
                offset: s.offset.map(|v| v * scale),
                blur: s.blur * scale,
                color: parse_color(&s.color)?,
                opacity: s.opacity,
            });
        }
        for g in &self.glows {
            style.glows.push(TextGlow {
                radius: g.radius * scale,
                intensity: g.intensity,
                color: parse_color(&g.color)?,
            });
        }
        if let Some(e) = &self.extrude {
            style.extrude = Some(TextExtrude {
                depth: e.depth * scale,
                angle: e.angle,
                color: parse_color(&e.color)?,
            });
        }
        let mut background = TitleBackground::default();
        if let Some(b) = &self.background {
            background = TitleBackground {
                enabled: true,
                color: parse_color(&b.color)?,
                opacity: b.opacity,
                padding: b.padding,
                corner_radius: b.corner_radius,
                shape: if b.pill {
                    BackgroundShape::Pill
                } else {
                    BackgroundShape::RoundedRect
                },
                gradient: b.gradient.as_ref().map(gradient).transpose()?,
                outline_color: b
                    .outline_color
                    .as_deref()
                    .map(parse_color)
                    .transpose()?
                    .unwrap_or(Rgba::WHITE),
                outline_width: b.outline_width * scale,
                rotation: b.rotation,
                ..TitleBackground::default()
            };
        }
        let mut title = TitleParams {
            content: Keyframed::constant(self.text.clone()),
            font_family: self.font.clone(),
            font_weight: self.weight,
            italic: self.italic,
            color: parse_color(&self.color)?,
            size: (self.size * scale).round().max(8.0),
            tracking: self.tracking,
            line_spacing: self.line_spacing * scale,
            case: match self.case {
                CaseDef::Mixed => FontCase::Mixed,
                CaseDef::Upper => FontCase::Upper,
                CaseDef::Lower => FontCase::Lower,
                CaseDef::Title => FontCase::Title,
            },
            align: match self.align {
                AlignDef::Left => TextAlign::Left,
                AlignDef::Center => TextAlign::Center,
                AlignDef::Right => TextAlign::Right,
            },
            position: self.position.map(|v| v * scale),
            anchor: (
                match self.anchor.h {
                    HDef::Left => HAnchor::Left,
                    HDef::Center => HAnchor::Center,
                    HDef::Right => HAnchor::Right,
                },
                match self.anchor.v {
                    VDef::Top => VAnchor::Top,
                    VDef::Middle => VAnchor::Middle,
                    VDef::Bottom => VAnchor::Bottom,
                },
            ),
            background,
            style,
            ..TitleParams::default()
        };
        title.anim = self.anim.build()?;
        if self.sticker.is_some() && title.anim.looped.is_none() {
            title.anim.looped = Some(AnimDescriptor::default());
        }
        Ok(title)
    }
}

impl AnimDef {
    fn build(&self) -> Result<TextAnimations, TemplateError> {
        let resolve = |r: &Option<AnimRef>,
                       lookup: fn(&str) -> Option<AnimDescriptor>|
         -> Result<Option<AnimDescriptor>, TemplateError> {
            let Some(r) = r else {
                return Ok(None);
            };
            let mut d = lookup(&r.preset).ok_or_else(|| TemplateError::Preset(r.preset.clone()))?;
            if let Some(v) = r.duration {
                d.duration = v;
            }
            if let Some(v) = r.stagger {
                d.stagger = v.clamp(0.0, 0.95);
            }
            if let Some(g) = r.granularity {
                d.granularity = match g {
                    GranularityDef::Block => Granularity::Block,
                    GranularityDef::Line => Granularity::Line,
                    GranularityDef::Word => Granularity::Word,
                    GranularityDef::Char => Granularity::Char,
                };
            }
            if let Some(c) = &r.tint {
                d.tint = parse_color(c)?;
            }
            Ok(Some(d))
        };
        let karaoke = self
            .karaoke
            .as_ref()
            .map(|k| {
                Ok::<_, TemplateError>(Karaoke {
                    color: parse_color(&k.color)?,
                    scale: k.scale,
                    keep_past: k.keep_past,
                    ..text_anim_presets::karaoke()
                })
            })
            .transpose()?;
        Ok(TextAnimations {
            enter: self
                .enter_custom
                .clone()
                .or(resolve(&self.enter, text_anim_presets::in_preset)?),
            exit: resolve(&self.exit, text_anim_presets::out_preset)?,
            looped: resolve(&self.looped, text_anim_presets::loop_preset)?,
            karaoke,
        })
    }
}

/// `base` (a title saved by the user) carrying `text` as a subtitle line
/// whose size is `size` of the frame height; its measures follow the size.
pub fn subtitle_from_title(
    base: &TitleParams,
    text: &str,
    resolution: (u32, u32),
    size: f32,
) -> TitleParams {
    let height = resolution.1 as f32;
    let new_size = (height * size).round().max(8.0);
    let k = new_size / base.size.max(1.0);
    let mut title = base.clone();
    title.size = new_size;
    title.line_spacing *= k;
    scale_style(&mut title.style, k);
    title.background.outline_width *= k;
    title.content = Keyframed::constant(text.to_owned());
    title.anchor = (HAnchor::Center, VAnchor::Bottom);
    title.position = [0.0, -height * 0.4];
    title.align = TextAlign::Center;
    title.background.center = [0.0, 0.0];
    title
}

fn scale_style(style: &mut TextStyle, scale: f32) {
    for stroke in &mut style.strokes {
        stroke.width *= scale;
    }
    for shadow in &mut style.shadows {
        shadow.offset = shadow.offset.map(|v| v * scale);
        shadow.blur *= scale;
    }
    for glow in &mut style.glows {
        glow.radius *= scale;
    }
    if let Some(extrude) = &mut style.extrude {
        extrude.depth *= scale;
    }
    for underlay in &mut style.underlays {
        underlay.offset = underlay.offset.map(|v| v * scale);
        scale_style(&mut underlay.style, scale);
    }
}

#[cfg(test)]
#[path = "tests/text_template.rs"]
mod tests;
