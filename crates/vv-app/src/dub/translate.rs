//! Translation of the dub's phrases, behind `Translator`. Only the text of
//! the phrases and the glossary leave the device, and only through a
//! provider the user enabled in the settings.

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// The original text is copied: translate by hand, nothing is sent.
    Manual,
    Gemini,
}

/// A phrase to translate and the length it should keep.
pub(crate) struct Item {
    pub id: usize,
    pub text: String,
    pub max_chars: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    Cancelled,
    Unreachable(String),
    Quota(String),
    Api(String),
    /// Some ids came back empty.
    Empty(Vec<usize>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Unreachable(e) | Self::Quota(e) | Self::Api(e) => f.write_str(e),
            Self::Empty(ids) => write!(f, "{} empty translations", ids.len()),
        }
    }
}

pub(crate) trait Translator: Send {
    /// One translation per item, by id, in `language` (ISO 639-1).
    fn translate(
        &self,
        items: &[Item],
        language: &str,
        glossary: &[String],
        shorter: bool,
        cancel: &AtomicBool,
    ) -> Result<Vec<(usize, String)>, Error>;
}

pub(crate) struct Manual;

impl Translator for Manual {
    fn translate(
        &self,
        items: &[Item],
        _: &str,
        _: &[String],
        _: bool,
        _: &AtomicBool,
    ) -> Result<Vec<(usize, String)>, Error> {
        Ok(items.iter().map(|i| (i.id, i.text.clone())).collect())
    }
}

pub(crate) struct Gemini {
    pub key: String,
    pub model: String,
}

/// Items per request: long enough to keep the context, short enough that a
/// failure costs little.
pub(crate) const BATCH: usize = 40;

pub(crate) fn language_name(code: &str) -> &'static str {
    match code {
        "it" => "Italian",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "ru" => "Russian",
        "pl" => "Polish",
        "tr" => "Turkish",
        "ja" => "Japanese",
        "zh" => "Chinese",
        _ => "the target language",
    }
}

pub(crate) fn prompt(items: &[Item], language: &str, glossary: &[String], shorter: bool) -> String {
    let lines: Vec<serde_json::Value> = items
        .iter()
        .map(|i| serde_json::json!({"id": i.id, "text": i.text, "max_chars": i.max_chars}))
        .collect();
    let keep = if glossary.is_empty() {
        String::new()
    } else {
        format!(
            " Never translate these names and terms, keep them exactly: {}.",
            glossary.join(", ")
        )
    };
    let task = if shorter {
        "Rewrite each line more briefly, keeping its meaning, so it can be spoken in less time"
    } else {
        "Translate each line of a video's dialogue for a dub"
    };
    format!(
        "{task}, into {}. Each line will be spoken in the time the original took: \
         keep the length close to the original and never above max_chars characters. \
         Keep product names, brands and technical terms (for example Apple, iPad, Sidecar) \
         as they are.{keep} Keep the tone. Return one entry per id, with the same ids.\n{}",
        language_name(language),
        serde_json::Value::Array(lines)
    )
}

pub(crate) fn missing_term<'a>(
    source: &str,
    translated: &str,
    glossary: &'a [String],
) -> Option<&'a str> {
    fn contains_term(text: &str, term: &str) -> bool {
        text.match_indices(term).any(|(i, _)| {
            !text[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                && !text[i + term.len()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
        })
    }
    glossary
        .iter()
        .map(String::as_str)
        .find(|term| contains_term(source, term) && !contains_term(translated, term))
}

/// The translations in a reply, by id; ids missing or empty are reported.
pub(crate) fn parse_reply(text: &str, items: &[Item]) -> Result<Vec<(usize, String)>, Error> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Error::Api(format!("invalid reply: {e}")))?;
    let entries = value["translations"]
        .as_array()
        .ok_or_else(|| Error::Api("reply without translations".into()))?;
    let mut out = Vec::new();
    let mut empty = Vec::new();
    for item in items {
        let found = entries
            .iter()
            .find(|e| e["id"].as_u64() == Some(item.id as u64))
            .and_then(|e| e["text"].as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty());
        match found {
            Some(t) => out.push((item.id, t.to_owned())),
            None => empty.push(item.id),
        }
    }
    if out.is_empty() && !empty.is_empty() {
        return Err(Error::Empty(empty));
    }
    Ok(out)
}

/// Classifies an error of `gemini::generate`, whose messages carry the
/// status (the key already redacted).
pub(crate) fn classify(message: String) -> Error {
    if message == "Cancelled" {
        Error::Cancelled
    } else if message.contains("error 429") {
        Error::Quota(message)
    } else if message.starts_with("Gemini error") {
        Error::Api(message)
    } else {
        Error::Unreachable(message)
    }
}

impl Translator for Gemini {
    fn translate(
        &self,
        items: &[Item],
        language: &str,
        glossary: &[String],
        shorter: bool,
        cancel: &AtomicBool,
    ) -> Result<Vec<(usize, String)>, Error> {
        let schema = serde_json::json!({
            "type": "OBJECT",
            "properties": {
                "translations": {
                    "type": "ARRAY",
                    "items": {
                        "type": "OBJECT",
                        "properties": {"id": {"type": "INTEGER"}, "text": {"type": "STRING"}},
                        "required": ["id", "text"]
                    }
                }
            },
            "required": ["translations"]
        });
        let request = serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": prompt(items, language, glossary, shorter)}]}],
            "generationConfig": {"responseMimeType": "application/json", "responseSchema": schema}
        });
        let body = crate::gemini::generate_with_timeout(
            &request,
            &self.key,
            cancel,
            &[&self.model],
            false,
            std::time::Duration::from_secs(45),
        )
        .map_err(classify)?;
        let text: String = body
            .pointer("/candidates/0/content/parts")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter(|p| p["thought"] != true)
            .filter_map(|p| p["text"].as_str())
            .collect();
        parse_reply(&text, items)
    }
}

/// SHA-256 of what decides a translation, the key of its cache entry.
pub(crate) fn cache_key(
    provider: Provider,
    model: &str,
    language: &str,
    glossary: &[String],
    text: &str,
) -> String {
    let material = format!(
        "{provider:?}\u{1f}{model}\u{1f}{language}\u{1f}{}\u{1f}{text}",
        glossary.join("\u{1e}")
    );
    ring::digest::digest(&ring::digest::SHA256, material.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn glossary(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}
