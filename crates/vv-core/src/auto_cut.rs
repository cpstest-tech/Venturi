use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Config {
    pub silence_db: f32,
    pub min_silence: f64,
    pub padding: f64,
    pub snap_radius: f64,
    pub sentence_gap: f64,
    pub restart_pause: f64,
    pub restart_sentences: usize,
    pub restart_window: f64,
    pub similarity: f64,
    pub min_repeat_words: usize,
    pub low_confidence: f32,
    pub crossfade_seconds: f64,
    pub fillers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            silence_db: -35.0,
            min_silence: 0.4,
            padding: 0.12,
            snap_radius: 0.08,
            sentence_gap: 0.65,
            restart_pause: 1.0,
            restart_sentences: 3,
            restart_window: 15.0,
            similarity: 0.8,
            min_repeat_words: 3,
            low_confidence: 0.45,
            crossfade_seconds: 0.015,
            fillers: [
                "ehm",
                "eh",
                "uhm",
                "cioè",
                "allora",
                "tipo",
                "diciamo",
                "praticamente",
                "insomma",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Energy {
    pub step: f64,
    pub db: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
    Speech,
    Silence,
    FalseStart,
    Filler,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub id: usize,
    pub start: f64,
    pub end: f64,
    #[serde(rename = "type")]
    pub kind: SegmentType,
    pub text: String,
    pub checked: bool,
}

pub fn merge_ranges(ranges: impl IntoIterator<Item = Range<f64>>) -> Vec<Range<f64>> {
    let mut ranges: Vec<_> = ranges
        .into_iter()
        .filter(|r| r.start.is_finite() && r.end.is_finite() && r.end > r.start)
        .collect();
    ranges.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut out: Vec<Range<f64>> = Vec::new();
    for r in ranges {
        if let Some(last) = out.last_mut()
            && r.start <= last.end + 1e-7
        {
            last.end = last.end.max(r.end);
        } else {
            out.push(r);
        }
    }
    out
}

pub fn keep_ranges(
    duration: f64,
    removed: impl IntoIterator<Item = Range<f64>>,
) -> Vec<Range<f64>> {
    if !duration.is_finite() || duration <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = 0.0_f64;
    for r in merge_ranges(
        removed
            .into_iter()
            .map(|r| r.start.clamp(0.0, duration)..r.end.clamp(0.0, duration)),
    ) {
        if r.start > cursor {
            out.push(cursor..r.start);
        }
        cursor = cursor.max(r.end);
    }
    if cursor < duration {
        out.push(cursor..duration);
    }
    out
}

fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn similarity(a: &[String], b: &[String]) -> f64 {
    if a.len().max(b.len()) > 96 {
        let a: std::collections::HashSet<_> = a.windows(3).collect();
        let b: std::collections::HashSet<_> = b.windows(3).collect();
        let shared = a.intersection(&b).count();
        return shared as f64 / (a.len() + b.len() - shared).max(1) as f64;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let diagonal = previous;
            previous = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(previous + 1)
                .min(diagonal + usize::from(x != y));
        }
    }
    1.0 - row[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
}

fn sentences(words: &[Word], config: &Config) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, w) in words.iter().enumerate() {
        if w.text.ends_with(['.', '?', '!', '…'])
            || i + 1 == words.len()
            || words[i + 1].start - w.end >= config.sentence_gap
        {
            out.push(start..i + 1);
            start = i + 1;
        }
    }
    out
}

fn false_starts(
    words: &[Word],
    sentences: &[Range<usize>],
    config: &Config,
    cancelled: &impl Fn() -> bool,
) -> Option<Vec<Range<f64>>> {
    let normalized: Vec<_> = sentences
        .iter()
        .map(|r| {
            normalize(
                &words[r.clone()]
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })
        .collect();
    let mut out = Vec::new();
    for (i, sentence) in sentences.iter().enumerate() {
        if cancelled() {
            return None;
        }
        let first = &words[sentence.start];
        let last = &words[sentence.end - 1];
        let a = &normalized[i];
        if a.len() < config.min_repeat_words {
            continue;
        }
        let low = words[sentence.clone()]
            .iter()
            .filter_map(|w| w.confidence)
            .reduce(f32::min)
            .is_some_and(|v| v < config.low_confidence);
        for j in i + 1..sentences.len().min(i + 1 + config.restart_sentences) {
            let next = &words[sentences[j].start];
            if next.start - first.start > config.restart_window {
                break;
            }
            let b = &normalized[j];
            let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
            let truncated = !last.text.ends_with(['.', '?', '!']) || low;
            let repeats_start = prefix >= config.min_repeat_words
                && b.len() > a.len()
                && (prefix as f64 / a.len() as f64 >= config.similarity)
                && (truncated || next.start - last.end >= config.restart_pause);
            if (b.len() >= config.min_repeat_words && similarity(a, b) >= config.similarity)
                || repeats_start
            {
                out.push(first.start..last.end);
                break;
            }
        }
    }
    Some(merge_ranges(out))
}

fn snap(energy: &Energy, edge: f64, lo: f64, hi: f64) -> f64 {
    if energy.step <= 0.0 || energy.db.is_empty() {
        return edge;
    }
    let start = (lo / energy.step).ceil().max(0.0) as usize;
    let end = ((hi / energy.step).floor().max(0.0) as usize).min(energy.db.len().saturating_sub(1));
    (start..=end)
        .min_by(|&a, &b| {
            energy.db[a].total_cmp(&energy.db[b]).then_with(|| {
                (a as f64 * energy.step - edge)
                    .abs()
                    .total_cmp(&(b as f64 * energy.step - edge).abs())
            })
        })
        .map_or(edge, |i| i as f64 * energy.step)
}

pub fn analyze(words: &[Word], energy: &Energy, duration: f64, config: &Config) -> Vec<Segment> {
    analyze_cancellable(words, energy, duration, config, &|| false).unwrap_or_default()
}

pub fn analyze_cancellable(
    words: &[Word],
    energy: &Energy,
    duration: f64,
    config: &Config,
    cancelled: &impl Fn() -> bool,
) -> Option<Vec<Segment>> {
    if cancelled() {
        return None;
    }
    if !duration.is_finite() || duration <= 0.0 {
        return Some(Vec::new());
    }
    let mut words: Vec<_> = words
        .iter()
        .filter(|w| {
            w.start.is_finite()
                && w.end.is_finite()
                && w.end > w.start
                && w.start < duration
                && w.end > 0.0
        })
        .cloned()
        .collect();
    words.sort_by(|a, b| a.start.total_cmp(&b.start));
    for word in &mut words {
        word.start = word.start.max(0.0);
        word.end = word.end.min(duration);
    }
    let mut quiet = Vec::new();
    let mut cursor = 0.0_f64;
    for (index, word) in words.iter().enumerate() {
        if index.is_multiple_of(1024) && cancelled() {
            return None;
        }
        if word.start - cursor >= config.min_silence {
            quiet.push(cursor..word.start);
        }
        cursor = cursor.max(word.end);
    }
    if !words.is_empty() && duration - cursor >= config.min_silence {
        quiet.push(cursor..duration);
    }
    if energy.step > 0.0 {
        let mut start = None;
        for i in 0..=energy.db.len() {
            if i.is_multiple_of(1024) && cancelled() {
                return None;
            }
            let low = energy.db.get(i).is_some_and(|db| *db < config.silence_db);
            if low && start.is_none() {
                start = Some(i);
            }
            if !low && let Some(s) = start.take() {
                let r = s as f64 * energy.step..(i as f64 * energy.step).min(duration);
                if r.end - r.start >= config.min_silence {
                    quiet.push(r);
                }
            }
        }
    }
    let protected = merge_ranges(words.iter().map(|w| {
        (w.start - config.padding.max(0.0)).max(0.0)
            ..(w.end + config.padding.max(0.0)).min(duration)
    }));
    let mut silence = Vec::new();
    let mut protect_index = 0;
    for r in merge_ranges(quiet) {
        if cancelled() {
            return None;
        }
        let mut cursor = if r.start <= 0.0 {
            0.0
        } else {
            r.start + config.padding.max(0.0)
        };
        let end = if r.end >= duration {
            duration
        } else {
            r.end - config.padding.max(0.0)
        };
        while protect_index < protected.len() && protected[protect_index].end <= cursor {
            protect_index += 1;
        }
        for p in protected[protect_index..]
            .iter()
            .take_while(|p| p.start < end)
        {
            if p.start > cursor {
                silence.push(cursor..p.start.min(end));
            }
            cursor = cursor.max(p.end);
        }
        if cursor < end {
            silence.push(cursor..end);
        }
    }
    let silence: Vec<_> = silence
        .into_iter()
        .filter_map(|r| {
            let original_length = r.end - r.start
                + if r.start > 0.0 {
                    config.padding.max(0.0)
                } else {
                    0.0
                }
                + if r.end < duration {
                    config.padding.max(0.0)
                } else {
                    0.0
                };
            if original_length + 1e-7 < config.min_silence {
                return None;
            }
            let start = if r.start == 0.0 {
                0.0
            } else {
                snap(
                    energy,
                    r.start,
                    r.start,
                    (r.start + config.snap_radius.max(0.0)).min(r.end),
                )
            };
            let end = if r.end == duration {
                duration
            } else {
                snap(
                    energy,
                    r.end,
                    (r.end - config.snap_radius.max(0.0)).max(start),
                    r.end,
                )
            };
            (end > start).then_some(start..end)
        })
        .collect();
    let sentences = sentences(&words, config);
    let restarts = false_starts(&words, &sentences, config, cancelled)?;
    let fillers: Vec<_> = words
        .iter()
        .filter(|w| {
            config
                .fillers
                .iter()
                .any(|f| normalize(f) == normalize(&w.text))
        })
        .map(|w| w.start..w.end)
        .collect();
    let mut edges = vec![0.0, duration];
    for ranges in [&silence, &restarts, &fillers] {
        for r in ranges {
            edges.extend([r.start, r.end]);
        }
    }
    for w in &words {
        edges.extend([w.start, w.end]);
    }
    edges.sort_by(f64::total_cmp);
    edges.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let mut indices = [0; 3];
    let mut word_index = 0;
    let sentence_starts: Vec<_> = sentences.iter().map(|r| words[r.start].start).collect();
    let mut out: Vec<Segment> = Vec::new();
    for (index, edge) in edges.windows(2).enumerate() {
        if index.is_multiple_of(1024) && cancelled() {
            return None;
        }
        let (start, end) = (edge[0], edge[1]);
        let middle = (start + end) / 2.0;
        let mut kind = SegmentType::Speech;
        for (i, (ranges, candidate)) in [
            (&silence, SegmentType::Silence),
            (&fillers, SegmentType::Filler),
            (&restarts, SegmentType::FalseStart),
        ]
        .iter()
        .enumerate()
        {
            while indices[i] < ranges.len() && ranges[indices[i]].end <= middle {
                indices[i] += 1;
            }
            if ranges
                .get(indices[i])
                .is_some_and(|r| r.start <= middle && middle < r.end)
            {
                kind = *candidate;
            }
        }
        let mut text = Vec::new();
        while word_index < words.len() && words[word_index].start < end - 1e-7 {
            if words[word_index].start >= start - 1e-7 {
                text.push(words[word_index].text.as_str());
            }
            word_index += 1;
        }
        let text = text.join(" ");
        let sentence_boundary = sentence_starts
            .binary_search_by(|s| s.total_cmp(&start))
            .is_ok();
        if let Some(last) = out.last_mut()
            && last.kind == kind
            && !sentence_boundary
        {
            last.end = end;
            if !text.is_empty() {
                if !last.text.is_empty() {
                    last.text.push(' ');
                }
                last.text.push_str(&text);
            }
        } else {
            out.push(Segment {
                id: out.len(),
                start,
                end,
                kind,
                text,
                checked: matches!(kind, SegmentType::Silence | SegmentType::FalseStart),
            });
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "tests/auto_cut.rs"]
mod tests;
