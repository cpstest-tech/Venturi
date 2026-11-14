//! The text templates shipped with the app (JSON in `assets/text-templates`).

use std::sync::OnceLock;
use vv_core::text_template::{TemplateCategory, TextTemplateDef};

macro_rules! bundled {
    ($($name:literal),* $(,)?) => {
        &[$(include_str!(concat!("../assets/text-templates/", $name, ".json"))),*]
    };
}

const SOURCES: &[&str] = bundled![
    "neon_pulse",
    "bubble_pop",
    "hype_impact",
    "sticker_pack",
    "gradient_wave",
    "retro_80s",
    "handwritten_marker",
    "glitch_rgb",
    "karaoke_pop",
    "comic_boom",
    "elegant_serif",
    "pixel_game",
    "fire_ice",
    "sunset_script",
    "outline_hollow",
    "lower_third_bold",
    "capcut_pause",
    "capcut_onfire",
    "capcut_modern",
    "capcut_wide",
    "capcut_blue",
    "capcut_cracked",
    "capcut_purple",
];

/// Prefix of a template id in `Captions::template`.
pub(crate) const SUBTITLE_PREFIX: &str = "text:";

pub(crate) fn all() -> &'static [TextTemplateDef] {
    static ALL: OnceLock<Vec<TextTemplateDef>> = OnceLock::new();
    ALL.get_or_init(|| {
        SOURCES
            .iter()
            .filter_map(|json| match TextTemplateDef::from_json(json) {
                Ok(def) => Some(def),
                Err(e) => {
                    eprintln!("text template skipped: {e}");
                    None
                }
            })
            .collect()
    })
}

pub(crate) fn find(id: &str) -> Option<&'static TextTemplateDef> {
    all().iter().find(|t| t.id == id)
}

pub(crate) fn in_category(
    category: TemplateCategory,
) -> impl Iterator<Item = &'static TextTemplateDef> {
    all().iter().filter(move |t| t.category == category)
}

/// Whether every word of `query` is in the name, a tag or the category label.
pub(crate) fn matches(def: &TextTemplateDef, category_label: &str, query: &str) -> bool {
    let haystack = format!("{} {} {}", def.name, def.tags.join(" "), category_label).to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// A text template used as the look of subtitles.
pub(crate) enum SubtitleStyle {
    Bundled(&'static TextTemplateDef),
    User(Box<vv_core::TitleParams>),
}

impl SubtitleStyle {
    pub(crate) fn build(
        &self,
        project: &mut vv_core::Project,
        timeline: vv_core::TimelineId,
        words: &[vv_core::CaptionWord],
    ) -> Vec<vv_core::Clip> {
        use vv_core::captions::{Reveal, build_clips_with};
        match self {
            Self::Bundled(def) => {
                let reveal = if def.subtitle.word_by_word {
                    Reveal::Single
                } else {
                    Reveal::Line
                };
                build_clips_with(project, timeline, words, reveal, false, &|text, res| {
                    def.subtitle_title(text, res)
                })
            }
            Self::User(base) => build_clips_with(
                project,
                timeline,
                words,
                Reveal::Line,
                false,
                &|text, res| vv_core::text_template::subtitle_from_title(base, text, res, 0.06),
            ),
        }
    }
}

#[cfg(test)]
#[path = "tests/text_templates.rs"]
mod tests;
