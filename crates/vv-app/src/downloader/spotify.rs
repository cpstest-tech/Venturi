//! Spotify audio is DRM-protected: a link is resolved to artist, title and
//! length from the public embed page, then matched on YouTube.

use super::source::SpotifyKind;
use super::ytdlp::Candidate;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub title: String,
    pub artists: String,
    pub duration: f64,
}

impl Track {
    pub fn label(&self) -> String {
        if self.artists.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artists, self.title)
        }
    }

    pub fn search_query(&self) -> String {
        format!("ytsearch8:{}", self.label())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    pub name: String,
    pub tracks: Vec<Track>,
}

pub fn resolve(kind: SpotifyKind, id: &str, cancel: &AtomicBool) -> Result<Collection, String> {
    let url = format!("https://open.spotify.com/embed/{}/{id}", kind.path());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .map_err(|e| format!("Spotify: {e}"))?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let mut html = String::new();
    response
        .body_mut()
        .with_config()
        .limit(8 << 20)
        .reader()
        .read_to_string(&mut html)
        .map_err(|e| format!("Spotify: {e}"))?;
    parse_embed(&html)
}

pub fn parse_embed(html: &str) -> Result<Collection, String> {
    const START: &str = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let json = html
        .split_once(START)
        .and_then(|(_, rest)| rest.split_once("</script>"))
        .map(|(json, _)| json)
        .ok_or("Spotify page without track data")?;
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Spotify data: {e}"))?;
    let entity = &value["props"]["pageProps"]["state"]["data"]["entity"];
    let text = |v: &serde_json::Value| clean(v.as_str().unwrap_or_default());
    let seconds = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) / 1000.0;
    let name = text(&entity["name"]);
    let tracks: Vec<Track> = match entity["trackList"].as_array() {
        Some(list) => list
            .iter()
            .filter(|t| t["isPlayable"].as_bool() != Some(false))
            .map(|t| Track {
                title: text(&t["title"]),
                artists: text(&t["subtitle"]),
                duration: seconds(&t["duration"]),
            })
            .collect(),
        None => vec![Track {
            title: text(&entity["title"]).or_if_empty(&name),
            artists: entity["artists"]
                .as_array()
                .map(|a| a.iter().map(|a| text(&a["name"])).collect::<Vec<_>>())
                .unwrap_or_default()
                .join(", "),
            duration: seconds(&entity["duration"]),
        }],
    };
    let tracks: Vec<Track> = tracks.into_iter().filter(|t| !t.title.is_empty()).collect();
    if tracks.is_empty() {
        return Err("No playable tracks in the Spotify link".into());
    }
    Ok(Collection { name, tracks })
}

trait OrIfEmpty {
    fn or_if_empty(self, other: &str) -> String;
}

impl OrIfEmpty for String {
    fn or_if_empty(self, other: &str) -> String {
        if self.is_empty() {
            other.to_owned()
        } else {
            self
        }
    }
}

/// The embed page separates artists with non-breaking spaces.
fn clean(text: &str) -> String {
    text.replace('\u{a0}', " ").trim().to_owned()
}

/// The search result most likely to be the same recording: close in length,
/// not a cover/live/remix the Spotify title does not ask for, and preferably
/// from YouTube Music's auto-generated "- Topic" channel.
pub fn best_match(track: &Track, candidates: &[Candidate]) -> Option<usize> {
    const VARIANTS: [&str; 10] = [
        "live",
        "cover",
        "remix",
        "karaoke",
        "instrumental",
        "8d",
        "slowed",
        "sped up",
        "reverb",
        "nightcore",
    ];
    let wanted = track.label().to_lowercase();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let off = match candidate.duration {
                Some(d) if track.duration > 0.0 => (d - track.duration).abs(),
                _ => 6.0,
            };
            if off > 15.0 {
                return None;
            }
            let title = candidate.title.to_lowercase();
            let variants = VARIANTS
                .iter()
                .filter(|v| title.contains(*v) && !wanted.contains(*v))
                .count() as f64;
            let topic = candidate.channel.ends_with(" - Topic");
            let mentions = track
                .title
                .to_lowercase()
                .split_whitespace()
                .filter(|word| title.contains(word))
                .count() as f64
                / track.title.split_whitespace().count().max(1) as f64;
            let score = off + variants * 20.0 + (1.0 - mentions) * 10.0
                - if topic { 4.0 } else { 0.0 }
                + index as f64 * 0.5;
            Some((index, score))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

#[cfg(test)]
#[path = "../tests/downloader/spotify.rs"]
mod tests;
